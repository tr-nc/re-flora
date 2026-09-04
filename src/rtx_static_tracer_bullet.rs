//! PROTOTYPE/TRACER BULLET: throwaway static RTX traversal ceiling experiment.
//!
//! This builds immutable synthetic acceleration structures once, measures three traversal
//! implementations, writes one artifact, and drops every resource before production startup.
//! It intentionally has no edit, refit, rebuild, streaming, fallback, or publication lifecycle.

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use re_flora_vkn::{
    build_aabb_blas_profiled, build_tlas_profiled, build_triangle_blas_profiled,
    execute_one_time_command, khr, vk, AccelStruct, Allocator, Buffer, BufferUsage,
    ComputePipeline, DescriptorPool, Device, Extent3D, MemoryLocation, PipelineStage, Resource,
    ShaderModule, TimestampQueryPool, VulkanContext,
};
use resource_container_derive::ResourceContainer;
use serde::Serialize;
use std::{ffi::CStr, path::Path, time::Instant};

const WORLD_DIMENSION: u32 = 64;
const RAY_GRID: u32 = 1024;
const RAY_COUNT: u32 = RAY_GRID * RAY_GRID;
const CANDIDATE_BUDGET: u32 = 4096;
const MAX_DDA_STEPS: u32 = 256;
const WARMUPS_PER_MODE: u32 = 2;
const INVALID_INDEX: u32 = u32::MAX;
const HIT_T_TOLERANCE: f32 = 2.0e-3;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BulletRay {
    origin: [f32; 3],
    t_min: f32,
    direction: [f32; 3],
    t_max: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BulletResult {
    hit_t: f32,
    voxel_index: u32,
    face_index: u32,
    normal_code: u32,
    primitive_index: u32,
    candidate_count: u32,
    rejected_candidate_count: u32,
    generated_candidate_count: u32,
    committed_candidate_count: u32,
    traversal_exhausted: u32,
    committed_disagreement: u32,
    confirmed_candidate_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BulletPushConstants {
    mode: u32,
    world_dimension: u32,
    ray_count: u32,
    candidate_budget: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Aabb {
    min_x: f32,
    min_y: f32,
    min_z: f32,
    max_x: f32,
    max_y: f32,
    max_z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FaceData {
    voxel_index: u32,
    face_index: u32,
    normal_code: u32,
    padding: u32,
}

#[derive(ResourceContainer)]
struct BulletResources {
    triangle_tlas: Resource<AccelStruct>,
    aabb_tlas: Resource<AccelStruct>,
    triangle_faces: Resource<Buffer>,
    surface_voxels: Resource<Buffer>,
    occupancy: Resource<Buffer>,
    rays: Resource<Buffer>,
    results: Resource<Buffer>,
}

struct BuiltVolume {
    pipeline: ComputePipeline,
    resources: BulletResources,
    _triangle_blas: AccelStruct,
    _aabb_blas: AccelStruct,
    build: StaticBuildEvidence,
}

struct ExtractedGeometry {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    faces: Vec<FaceData>,
    aabbs: Vec<Aabb>,
    surface_voxels: Vec<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TraversalMode {
    SoftwareDda,
    VoxelAabbExact,
    ExposedFaceTriangles,
}

impl TraversalMode {
    const ALL: [Self; 3] = [
        Self::SoftwareDda,
        Self::VoxelAabbExact,
        Self::ExposedFaceTriangles,
    ];

    fn shader_value(self) -> u32 {
        match self {
            Self::SoftwareDda => 0,
            Self::VoxelAabbExact => 1,
            Self::ExposedFaceTriangles => 2,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::SoftwareDda => "software_dda",
            Self::VoxelAabbExact => "voxel_aabb_exact",
            Self::ExposedFaceTriangles => "exposed_face_triangles",
        }
    }
}

#[derive(Clone, Copy)]
enum VolumeKind {
    Sparse,
    Dense,
    ShellCavity,
}

impl VolumeKind {
    const ALL: [Self; 3] = [Self::Sparse, Self::Dense, Self::ShellCavity];

    fn name(self) -> &'static str {
        match self {
            Self::Sparse => "sparse_5_percent_with_fixture",
            Self::Dense => "dense_75_percent_with_fixture",
            Self::ShellCavity => "shell_cavity",
        }
    }
}

#[derive(Serialize)]
struct Artifact {
    schema: &'static str,
    prototype_notice: &'static str,
    fixed_baseline: &'static str,
    generated_at: String,
    command: Vec<String>,
    machine: MachineIdentity,
    workload: WorkloadIdentity,
    coverage: Vec<&'static str>,
    memory_before_bytes: u64,
    volumes: Vec<VolumeResult>,
}

#[derive(Serialize)]
struct MachineIdentity {
    device_name: String,
    vendor_id: u32,
    device_id: u32,
    device_type: String,
    api_version: String,
    driver_version_raw: u32,
    device_uuid_hex: String,
    driver_uuid_hex: String,
    timestamp_period_ns: f32,
    acceleration_structure_extension: bool,
    ray_query_extension: bool,
    ray_tracing_pipeline_extension: bool,
    acceleration_structure_feature: bool,
    ray_query_feature: bool,
}

#[derive(Serialize)]
struct WorkloadIdentity {
    world_dimension: u32,
    ray_grid_width: u32,
    ray_grid_height: u32,
    ray_count: u32,
    candidate_budget: u32,
    max_dda_steps: u32,
    warmups_per_mode: u32,
    sample_order: Vec<&'static str>,
    volumes: Vec<&'static str>,
}

#[derive(Serialize)]
struct VolumeResult {
    name: &'static str,
    occupied_voxel_count: u32,
    surface_voxel_count: u32,
    exposed_face_count: u32,
    triangle_primitive_count: u32,
    actual_density_percent: f64,
    build: StaticBuildEvidence,
    static_live_resource_bytes: u64,
    build_peak_accounted_bytes: u64,
    peak_device_local_heap_usage_bytes: u64,
    samples: Vec<TraversalSample>,
}

#[derive(Serialize)]
struct StaticBuildEvidence {
    triangle_extraction_host_ms: f64,
    aabb_extraction_host_ms: f64,
    triangle_vertex_input_bytes: u64,
    triangle_index_input_bytes: u64,
    triangle_metadata_bytes: u64,
    aabb_input_bytes: u64,
    aabb_metadata_bytes: u64,
    triangle_blas: AsBuildResult,
    triangle_tlas: AsBuildResult,
    aabb_blas: AsBuildResult,
    aabb_tlas: AsBuildResult,
}

#[derive(Serialize)]
struct AsBuildResult {
    primitive_count: u32,
    acceleration_structure_bytes: u64,
    scratch_bytes: u64,
    host_build_ms: f64,
    gpu_build_ms: f64,
}

#[derive(Serialize)]
struct TraversalSample {
    order_index: u32,
    mode: TraversalMode,
    ray_count: u32,
    gpu_ms: f64,
    host_wait_ms: f64,
    ns_per_ray: f64,
    mrays_per_second: f64,
    hit_count: u32,
    candidate_count: u64,
    rejected_candidate_count: u64,
    generated_candidate_count: u64,
    confirmed_candidate_count: u64,
    committed_candidate_count: u64,
    traversal_exhausted_count: u32,
    committed_disagreement_count: u32,
    correctness: CorrectnessResult,
}

#[derive(Serialize)]
struct CorrectnessResult {
    reference_ray_count: u32,
    false_positive_count: u32,
    committed_false_positive_count: u32,
    false_negative_count: u32,
    wrong_voxel_count: u32,
    wrong_face_count: u32,
    wrong_normal_count: u32,
    primitive_mapping_mismatch_count: u32,
    hit_t_mismatch_count: u32,
    max_hit_t_error: f32,
    hit_t_tolerance: f32,
    first_mismatches: Vec<MismatchDetail>,
}

#[derive(Serialize)]
struct MismatchDetail {
    ray_index: u32,
    expected_voxel: u32,
    actual_voxel: u32,
    expected_face: u32,
    actual_face: u32,
    expected_hit_t: f32,
    actual_hit_t: f32,
    primitive_index: u32,
}

#[derive(Clone, Copy)]
struct CpuHit {
    hit_t: f32,
    voxel_index: u32,
    face_index: u32,
    normal_code: u32,
}

pub(crate) fn run(vulkan_ctx: &VulkanContext, allocator: Allocator, output: &Path) -> Result<()> {
    let machine = machine_identity(vulkan_ctx)?;
    anyhow::ensure!(
        machine.acceleration_structure_extension
            && machine.ray_query_extension
            && machine.acceleration_structure_feature
            && machine.ray_query_feature,
        "selected device did not expose enabled acceleration-structure/ray-query capability"
    );
    log::info!(
        "[RTX_STATIC_TRACER_BULLET][PROTOTYPE][CAPABILITY] device={} as_ext={} ray_query_ext={} ray_pipeline_ext={} as_feature={} ray_query_feature={}",
        machine.device_name,
        machine.acceleration_structure_extension,
        machine.ray_query_extension,
        machine.ray_tracing_pipeline_extension,
        machine.acceleration_structure_feature,
        machine.ray_query_feature,
    );

    let memory_before_bytes = device_local_heap_usage_bytes(vulkan_ctx);
    let shader = ShaderModule::from_precompiled(
        vulkan_ctx.device(),
        "shader/experiments/rtx_static_tracer_bullet.comp",
        "main",
    )
    .map_err(anyhow::Error::msg)
    .context("load static tracer-bullet shader")?;
    let descriptor_pool = DescriptorPool::new_for_hardware_ray_query(vulkan_ctx.device())
        .context("create tracer-bullet descriptor pool")?;
    let acc_device = khr::acceleration_structure::Device::new(
        vulkan_ctx.instance().as_raw(),
        vulkan_ctx.device(),
    );
    let rays = benchmark_rays();
    let ray_buffer = typed_buffer(
        vulkan_ctx.device(),
        allocator.clone(),
        &rays,
        vk::BufferUsageFlags::STORAGE_BUFFER,
        MemoryLocation::CpuToGpu,
    )?;
    let result_buffer = Buffer::new_sized(
        vulkan_ctx.device().clone(),
        allocator.clone(),
        BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
        MemoryLocation::GpuToCpu,
        u64::from(RAY_COUNT) * std::mem::size_of::<BulletResult>() as u64,
    );

    let mut volumes = Vec::new();
    for kind in VolumeKind::ALL {
        let occupancy = make_volume(kind);
        let occupied_voxel_count = occupied_voxel_count(&occupancy);
        let references = cpu_reference(&occupancy, &rays);
        let occupancy_buffer = typed_buffer(
            vulkan_ctx.device(),
            allocator.clone(),
            &occupancy,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        let (geometry, triangle_extraction_host_ms, aabb_extraction_host_ms) =
            extract_geometry_profiled(&occupancy);
        let face_data = geometry.faces.clone();
        let surface_voxels = geometry.surface_voxels.clone();
        let surface_voxel_count = surface_voxels.len() as u32;
        let exposed_face_count = face_data.len() as u32;
        let triangle_primitive_count = geometry.indices.len() as u32 / 3;
        let built = build_volume(
            vulkan_ctx,
            allocator.clone(),
            &acc_device,
            &shader,
            &descriptor_pool,
            occupancy_buffer,
            ray_buffer.clone(),
            result_buffer.clone(),
            geometry,
            triangle_extraction_host_ms,
            aabb_extraction_host_ms,
        )?;

        for mode in TraversalMode::ALL {
            for _ in 0..WARMUPS_PER_MODE {
                let _ = dispatch(vulkan_ctx, &built.pipeline, mode);
            }
        }
        let order = sample_order();
        let mut samples = Vec::with_capacity(order.len());
        for (order_index, mode) in order.into_iter().enumerate() {
            let (gpu_ms, host_wait_ms) = dispatch(vulkan_ctx, &built.pipeline, mode);
            let bytes = built
                .resources
                .results
                .read_back()
                .context("read tracer-bullet results")?;
            let results = bytemuck::try_cast_slice::<u8, BulletResult>(&bytes)
                .map_err(|error| anyhow::anyhow!("decode tracer-bullet results: {error}"))?;
            samples.push(summarize_sample(
                order_index as u32,
                mode,
                gpu_ms,
                host_wait_ms,
                results,
                &references,
                &face_data,
                &surface_voxels,
            ));
        }
        let peak_device_local_heap_usage_bytes = device_local_heap_usage_bytes(vulkan_ctx);
        let static_live_resource_bytes = static_live_resource_bytes(
            &built.build,
            built.resources.occupancy.get_size_bytes(),
            built.resources.rays.get_size_bytes(),
            built.resources.results.get_size_bytes(),
        );
        let build_peak_accounted_bytes = build_peak_accounted_bytes(
            &built.build,
            built.resources.occupancy.get_size_bytes(),
            built.resources.rays.get_size_bytes(),
            built.resources.results.get_size_bytes(),
        );
        log_volume_summary(
            kind,
            &built.build,
            &samples,
            triangle_primitive_count,
            surface_voxel_count,
        );
        volumes.push(VolumeResult {
            name: kind.name(),
            occupied_voxel_count,
            surface_voxel_count,
            exposed_face_count,
            triangle_primitive_count,
            actual_density_percent: occupied_voxel_count as f64 * 100.0
                / f64::from(WORLD_DIMENSION.pow(3)),
            build: built.build,
            static_live_resource_bytes,
            build_peak_accounted_bytes,
            peak_device_local_heap_usage_bytes,
            samples,
        });
    }

    let order = sample_order();
    let artifact = Artifact {
        schema: "re-flora.rtx-static-tracer-bullet.v1",
        prototype_notice: "PROTOTYPE/TRACER BULLET: static build-once traversal ceiling only; discardable and not production architecture",
        fixed_baseline: "7ce60e06f1b70793c18339ce60a59a61c985aa82",
        generated_at: chrono::Local::now().to_rfc3339(),
        command: std::env::args().collect(),
        machine,
        workload: WorkloadIdentity {
            world_dimension: WORLD_DIMENSION,
            ray_grid_width: RAY_GRID,
            ray_grid_height: RAY_GRID,
            ray_count: RAY_COUNT,
            candidate_budget: CANDIDATE_BUDGET,
            max_dda_steps: MAX_DDA_STEPS,
            warmups_per_mode: WARMUPS_PER_MODE,
            sample_order: order.iter().map(|mode| mode.name()).collect(),
            volumes: VolumeKind::ALL.iter().map(|kind| kind.name()).collect(),
        },
        coverage: vec![
            "sparse",
            "dense",
            "shell",
            "cavity",
            "axis_parallel",
            "grazing",
            "world_boundary",
            "diagonal",
        ],
        memory_before_bytes,
        volumes,
    };
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("create tracer-bullet output directory {}", parent.display())
        })?;
    }
    std::fs::write(output, toml::to_string_pretty(&artifact)?)
        .with_context(|| format!("write tracer-bullet artifact {}", output.display()))?;
    log::info!(
        "[RTX_STATIC_TRACER_BULLET][PROTOTYPE][COMPLETE] artifact={} volumes={} rays_per_sample={} resources=dropped-before-production",
        output.display(),
        artifact.volumes.len(),
        RAY_COUNT,
    );
    Ok(())
}

fn sample_order() -> Vec<TraversalMode> {
    let block = [
        TraversalMode::SoftwareDda,
        TraversalMode::VoxelAabbExact,
        TraversalMode::ExposedFaceTriangles,
        TraversalMode::ExposedFaceTriangles,
        TraversalMode::VoxelAabbExact,
        TraversalMode::SoftwareDda,
        TraversalMode::ExposedFaceTriangles,
        TraversalMode::VoxelAabbExact,
        TraversalMode::SoftwareDda,
        TraversalMode::SoftwareDda,
        TraversalMode::VoxelAabbExact,
        TraversalMode::ExposedFaceTriangles,
    ];
    block.repeat(3)
}

#[allow(clippy::too_many_arguments)]
fn build_volume(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    acc_device: &khr::acceleration_structure::Device,
    shader: &ShaderModule,
    descriptor_pool: &DescriptorPool,
    occupancy: Buffer,
    rays: Buffer,
    results: Buffer,
    geometry: ExtractedGeometry,
    triangle_extraction_host_ms: f64,
    aabb_extraction_host_ms: f64,
) -> Result<BuiltVolume> {
    let vertex_buffer = typed_buffer(
        vulkan_ctx.device(),
        allocator.clone(),
        &geometry.vertices,
        vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        MemoryLocation::CpuToGpu,
    )?;
    let index_buffer = typed_buffer(
        vulkan_ctx.device(),
        allocator.clone(),
        &geometry.indices,
        vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        MemoryLocation::CpuToGpu,
    )?;
    let face_buffer = typed_buffer(
        vulkan_ctx.device(),
        allocator.clone(),
        &geometry.faces,
        vk::BufferUsageFlags::STORAGE_BUFFER,
        MemoryLocation::CpuToGpu,
    )?;
    let aabb_buffer = typed_buffer(
        vulkan_ctx.device(),
        allocator.clone(),
        &geometry.aabbs,
        vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        MemoryLocation::CpuToGpu,
    )?;
    let surface_voxel_buffer = typed_buffer(
        vulkan_ctx.device(),
        allocator.clone(),
        &geometry.surface_voxels,
        vk::BufferUsageFlags::STORAGE_BUFFER,
        MemoryLocation::CpuToGpu,
    )?;

    let triangle_primitive_count = geometry.indices.len() as u32 / 3;
    let triangle_blas = build_triangle_blas_profiled(
        vulkan_ctx,
        allocator.clone(),
        acc_device.clone(),
        &vertex_buffer,
        geometry.vertices.len() as u32,
        &index_buffer,
        triangle_primitive_count,
    );
    let triangle_tlas = build_single_instance_tlas(
        vulkan_ctx,
        allocator.clone(),
        acc_device,
        &triangle_blas.acceleration_structure,
    )?;
    let aabb_blas = build_aabb_blas_profiled(
        vulkan_ctx,
        allocator.clone(),
        acc_device.clone(),
        &aabb_buffer,
        geometry.aabbs.len() as u32,
    );
    let aabb_tlas = build_single_instance_tlas(
        vulkan_ctx,
        allocator,
        acc_device,
        &aabb_blas.acceleration_structure,
    )?;
    let build = StaticBuildEvidence {
        triangle_extraction_host_ms,
        aabb_extraction_host_ms,
        triangle_vertex_input_bytes: vertex_buffer.get_size_bytes(),
        triangle_index_input_bytes: index_buffer.get_size_bytes(),
        triangle_metadata_bytes: face_buffer.get_size_bytes(),
        aabb_input_bytes: aabb_buffer.get_size_bytes(),
        aabb_metadata_bytes: surface_voxel_buffer.get_size_bytes(),
        triangle_blas: as_build_result(triangle_primitive_count, &triangle_blas),
        triangle_tlas: as_build_result(1, &triangle_tlas),
        aabb_blas: as_build_result(geometry.aabbs.len() as u32, &aabb_blas),
        aabb_tlas: as_build_result(1, &aabb_tlas),
    };
    let resources = BulletResources {
        triangle_tlas: Resource::new(triangle_tlas.acceleration_structure.clone()),
        aabb_tlas: Resource::new(aabb_tlas.acceleration_structure.clone()),
        triangle_faces: Resource::new(face_buffer),
        surface_voxels: Resource::new(surface_voxel_buffer),
        occupancy: Resource::new(occupancy),
        rays: Resource::new(rays),
        results: Resource::new(results),
    };
    let pipeline =
        ComputePipeline::new(vulkan_ctx.device(), shader, descriptor_pool, &[&resources]);
    Ok(BuiltVolume {
        pipeline,
        resources,
        _triangle_blas: triangle_blas.acceleration_structure,
        _aabb_blas: aabb_blas.acceleration_structure,
        build,
    })
}

fn build_single_instance_tlas(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    acc_device: &khr::acceleration_structure::Device,
    blas: &AccelStruct,
) -> Result<re_flora_vkn::ProfiledAccelerationStructure> {
    let instance = vk::AccelerationStructureInstanceKHR {
        transform: vk::TransformMatrixKHR {
            matrix: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        },
        instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xff),
        instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(0, 0),
        acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
            device_handle: blas.get_device_address(),
        },
    };
    let instance_bytes = unsafe {
        std::slice::from_raw_parts(
            (&instance as *const vk::AccelerationStructureInstanceKHR).cast::<u8>(),
            std::mem::size_of_val(&instance),
        )
    };
    let instance_buffer = Buffer::new_sized(
        vulkan_ctx.device().clone(),
        allocator.clone(),
        BufferUsage::from_flags(
            vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        ),
        MemoryLocation::CpuToGpu,
        instance_bytes.len() as u64,
    );
    instance_buffer.fill_with_raw_u8(instance_bytes)?;
    Ok(build_tlas_profiled(
        vulkan_ctx,
        allocator,
        acc_device.clone(),
        &instance_buffer,
        1,
    ))
}

fn as_build_result(
    primitive_count: u32,
    build: &re_flora_vkn::ProfiledAccelerationStructure,
) -> AsBuildResult {
    AsBuildResult {
        primitive_count,
        acceleration_structure_bytes: build.acceleration_structure_bytes,
        scratch_bytes: build.scratch_bytes,
        host_build_ms: build.host_build_ms,
        gpu_build_ms: build.gpu_build_ms,
    }
}

fn dispatch(
    vulkan_ctx: &VulkanContext,
    pipeline: &ComputePipeline,
    mode: TraversalMode,
) -> (f64, f64) {
    let timestamps = TimestampQueryPool::maybe_new(vulkan_ctx, 2, "RTX_STATIC_TRACER_BULLET")
        .expect("static tracer bullet requires GPU timestamps");
    let constants = BulletPushConstants {
        mode: mode.shader_value(),
        world_dimension: WORLD_DIMENSION,
        ray_count: RAY_COUNT,
        candidate_budget: CANDIDATE_BUDGET,
    };
    let started = Instant::now();
    execute_one_time_command(
        vulkan_ctx.device(),
        vulkan_ctx.command_pool(),
        &vulkan_ctx.get_general_queue(),
        |command_buffer| {
            timestamps.record_reset(command_buffer, 2);
            timestamps.record_timestamp(command_buffer, PipelineStage::TOP_OF_PIPE, 0);
            pipeline.record(
                command_buffer,
                Extent3D::new(RAY_COUNT, 1, 1),
                Some(bytemuck::bytes_of(&constants)),
            );
            timestamps.record_timestamp(command_buffer, PipelineStage::BOTTOM_OF_PIPE, 1);
        },
    );
    let host_wait_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let values = timestamps
        .read_u64(2)
        .expect("tracer-bullet timestamps must be ready after queue idle");
    let gpu_ms = values[1].saturating_sub(values[0]) as f64
        * timestamps.timestamp_period_ns() as f64
        / 1_000_000.0;
    (gpu_ms, host_wait_ms)
}

#[allow(clippy::too_many_arguments)]
fn summarize_sample(
    order_index: u32,
    mode: TraversalMode,
    gpu_ms: f64,
    host_wait_ms: f64,
    results: &[BulletResult],
    reference: &[CpuHit],
    faces: &[FaceData],
    surface_voxels: &[u32],
) -> TraversalSample {
    let mut false_positive_count = 0;
    let mut false_negative_count = 0;
    let mut wrong_voxel_count = 0;
    let mut wrong_face_count = 0;
    let mut wrong_normal_count = 0;
    let mut primitive_mapping_mismatch_count = 0;
    let mut hit_t_mismatch_count = 0;
    let mut max_hit_t_error = 0.0f32;
    let mut first_mismatches = Vec::new();
    for (ray_index, (actual, expected)) in results.iter().zip(reference).enumerate() {
        let actual_hit = actual.voxel_index != INVALID_INDEX;
        let expected_hit = expected.voxel_index != INVALID_INDEX;
        if actual_hit && !expected_hit {
            false_positive_count += 1;
        } else if !actual_hit && expected_hit {
            false_negative_count += 1;
        }
        if actual_hit && expected_hit {
            wrong_voxel_count += u32::from(actual.voxel_index != expected.voxel_index);
            wrong_face_count += u32::from(actual.face_index != expected.face_index);
            wrong_normal_count += u32::from(actual.normal_code != expected.normal_code);
            let error = (actual.hit_t - expected.hit_t).abs();
            max_hit_t_error = max_hit_t_error.max(error);
            hit_t_mismatch_count += u32::from(error > HIT_T_TOLERANCE);
        }
        if actual_hit {
            let mapping_matches = match mode {
                TraversalMode::SoftwareDda => actual.primitive_index == INVALID_INDEX,
                TraversalMode::VoxelAabbExact => surface_voxels
                    .get(actual.primitive_index as usize)
                    .is_some_and(|&voxel| voxel == actual.voxel_index),
                TraversalMode::ExposedFaceTriangles => faces
                    .get(actual.primitive_index as usize / 2)
                    .is_some_and(|face| {
                        face.voxel_index == actual.voxel_index
                            && face.face_index == actual.face_index
                            && face.normal_code == actual.normal_code
                    }),
            };
            primitive_mapping_mismatch_count += u32::from(!mapping_matches);
        }
        let mismatch = actual_hit != expected_hit
            || (actual_hit
                && expected_hit
                && (actual.voxel_index != expected.voxel_index
                    || actual.face_index != expected.face_index
                    || actual.normal_code != expected.normal_code
                    || (actual.hit_t - expected.hit_t).abs() > HIT_T_TOLERANCE));
        if mismatch && first_mismatches.len() < 16 {
            first_mismatches.push(MismatchDetail {
                ray_index: ray_index as u32,
                expected_voxel: expected.voxel_index,
                actual_voxel: actual.voxel_index,
                expected_face: expected.face_index,
                actual_face: actual.face_index,
                expected_hit_t: expected.hit_t,
                actual_hit_t: actual.hit_t,
                primitive_index: actual.primitive_index,
            });
        }
    }
    let candidate_count = results.iter().map(|r| u64::from(r.candidate_count)).sum();
    let rejected_candidate_count = results
        .iter()
        .map(|r| u64::from(r.rejected_candidate_count))
        .sum();
    let generated_candidate_count = results
        .iter()
        .map(|r| u64::from(r.generated_candidate_count))
        .sum();
    let confirmed_candidate_count = results
        .iter()
        .map(|r| u64::from(r.confirmed_candidate_count))
        .sum();
    let committed_candidate_count = results
        .iter()
        .map(|r| u64::from(r.committed_candidate_count))
        .sum();
    let hit_count = results
        .iter()
        .filter(|result| result.voxel_index != INVALID_INDEX)
        .count() as u32;
    TraversalSample {
        order_index,
        mode,
        ray_count: RAY_COUNT,
        gpu_ms,
        host_wait_ms,
        ns_per_ray: gpu_ms * 1_000_000.0 / f64::from(RAY_COUNT),
        mrays_per_second: f64::from(RAY_COUNT) / (gpu_ms * 1_000.0),
        hit_count,
        candidate_count,
        rejected_candidate_count,
        generated_candidate_count,
        confirmed_candidate_count,
        committed_candidate_count,
        traversal_exhausted_count: results
            .iter()
            .filter(|result| result.traversal_exhausted != 0)
            .count() as u32,
        committed_disagreement_count: results
            .iter()
            .filter(|result| result.committed_disagreement != 0)
            .count() as u32,
        correctness: CorrectnessResult {
            reference_ray_count: reference.len() as u32,
            false_positive_count,
            committed_false_positive_count: if mode == TraversalMode::SoftwareDda {
                0
            } else {
                false_positive_count
            },
            false_negative_count,
            wrong_voxel_count,
            wrong_face_count,
            wrong_normal_count,
            primitive_mapping_mismatch_count,
            hit_t_mismatch_count,
            max_hit_t_error,
            hit_t_tolerance: HIT_T_TOLERANCE,
            first_mismatches,
        },
    }
}

fn extract_geometry_profiled(occupancy: &[u32]) -> (ExtractedGeometry, f64, f64) {
    let triangle_started = Instant::now();
    let (vertices, indices, faces, surface_voxels) = extract_exposed_faces(occupancy);
    let triangle_extraction_host_ms = triangle_started.elapsed().as_secs_f64() * 1_000.0;
    let aabb_started = Instant::now();
    let aabbs = surface_voxels
        .iter()
        .map(|&index| {
            let [x, y, z] = voxel_coordinate(index);
            Aabb {
                min_x: x as f32,
                min_y: y as f32,
                min_z: z as f32,
                max_x: (x + 1) as f32,
                max_y: (y + 1) as f32,
                max_z: (z + 1) as f32,
            }
        })
        .collect();
    let aabb_extraction_host_ms = aabb_started.elapsed().as_secs_f64() * 1_000.0;
    (
        ExtractedGeometry {
            vertices,
            indices,
            faces,
            aabbs,
            surface_voxels,
        },
        triangle_extraction_host_ms,
        aabb_extraction_host_ms,
    )
}

fn extract_exposed_faces(occupancy: &[u32]) -> (Vec<Vertex>, Vec<u32>, Vec<FaceData>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut faces = Vec::new();
    let mut surface_voxels = Vec::new();
    for z in 0..WORLD_DIMENSION {
        for y in 0..WORLD_DIMENSION {
            for x in 0..WORLD_DIMENSION {
                let voxel = voxel_index(x, y, z);
                if occupancy[voxel as usize] == 0 {
                    continue;
                }
                let mut surface = false;
                for face_index in 0..6 {
                    if face_is_exposed(occupancy, x, y, z, face_index) {
                        surface = true;
                        append_face(
                            &mut vertices,
                            &mut indices,
                            [x as f32, y as f32, z as f32],
                            face_index,
                        );
                        faces.push(FaceData {
                            voxel_index: voxel,
                            face_index,
                            normal_code: face_index,
                            padding: 0,
                        });
                    }
                }
                if surface {
                    surface_voxels.push(voxel);
                }
            }
        }
    }
    (vertices, indices, faces, surface_voxels)
}

fn face_is_exposed(occupancy: &[u32], x: u32, y: u32, z: u32, face: u32) -> bool {
    let direction = match face {
        0 => [-1, 0, 0],
        1 => [1, 0, 0],
        2 => [0, -1, 0],
        3 => [0, 1, 0],
        4 => [0, 0, -1],
        5 => [0, 0, 1],
        _ => unreachable!(),
    };
    let neighbor = [
        x as i32 + direction[0],
        y as i32 + direction[1],
        z as i32 + direction[2],
    ];
    neighbor
        .iter()
        .any(|&coordinate| coordinate < 0 || coordinate >= WORLD_DIMENSION as i32)
        || occupancy
            [voxel_index(neighbor[0] as u32, neighbor[1] as u32, neighbor[2] as u32) as usize]
            == 0
}

fn append_face(vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, [x, y, z]: [f32; 3], face: u32) {
    let corners = match face {
        0 => [
            [x, y, z],
            [x, y, z + 1.0],
            [x, y + 1.0, z + 1.0],
            [x, y + 1.0, z],
        ],
        1 => [
            [x + 1.0, y, z],
            [x + 1.0, y + 1.0, z],
            [x + 1.0, y + 1.0, z + 1.0],
            [x + 1.0, y, z + 1.0],
        ],
        2 => [
            [x, y, z],
            [x + 1.0, y, z],
            [x + 1.0, y, z + 1.0],
            [x, y, z + 1.0],
        ],
        3 => [
            [x, y + 1.0, z],
            [x, y + 1.0, z + 1.0],
            [x + 1.0, y + 1.0, z + 1.0],
            [x + 1.0, y + 1.0, z],
        ],
        4 => [
            [x, y, z],
            [x, y + 1.0, z],
            [x + 1.0, y + 1.0, z],
            [x + 1.0, y, z],
        ],
        5 => [
            [x, y, z + 1.0],
            [x + 1.0, y, z + 1.0],
            [x + 1.0, y + 1.0, z + 1.0],
            [x, y + 1.0, z + 1.0],
        ],
        _ => unreachable!(),
    };
    let base = vertices.len() as u32;
    vertices.extend(corners.map(|position| Vertex { position }));
    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn make_volume(kind: VolumeKind) -> Vec<u32> {
    let mut occupancy = vec![0u32; WORLD_DIMENSION.pow(3) as usize];
    let density = match kind {
        VolumeKind::Sparse => Some(5),
        VolumeKind::Dense => Some(75),
        VolumeKind::ShellCavity => None,
    };
    if let Some(density_percent) = density {
        for z in 0..WORLD_DIMENSION {
            for y in 0..WORLD_DIMENSION {
                for x in 0..WORLD_DIMENSION {
                    if hash_voxel(x, y, z) % 10_000 < density_percent * 100 {
                        occupancy[voxel_index(x, y, z) as usize] = 1;
                    }
                }
            }
        }
    }
    add_correctness_fixture(&mut occupancy);
    occupancy
}

fn add_correctness_fixture(occupancy: &mut [u32]) {
    for z in 24..=40 {
        for y in 24..=40 {
            for x in 24..=40 {
                let shell = x == 24 || x == 40 || y == 24 || y == 40 || z == 24 || z == 40;
                occupancy[voxel_index(x, y, z) as usize] = u32::from(shell);
            }
        }
    }
    for coordinate in 0..WORLD_DIMENSION {
        occupancy
            [voxel_index(coordinate, (coordinate + 3) % WORLD_DIMENSION, coordinate) as usize] = 1;
    }
    for [x, y, z] in [[0, 0, 0], [63, 63, 63], [0, 31, 31], [63, 31, 31]] {
        occupancy[voxel_index(x, y, z) as usize] = 1;
    }
}

fn benchmark_rays() -> Vec<BulletRay> {
    let mut rays = Vec::with_capacity(RAY_COUNT as usize);
    for y in 0..RAY_GRID {
        for x in 0..RAY_GRID {
            // Break the rational alignment between the launch grid and voxel lattice, then
            // deterministically reject the rare ray that still crosses a voxel edge/corner.
            // Explicit rays below cover boundary and grazing behavior without ambiguous face
            // ownership; the throughput population therefore has a unique identity oracle.
            let ray = (0..32)
                .find_map(|salt| {
                    let jitter = hash_voxel(x ^ salt, y, x ^ y ^ salt.rotate_left(7));
                    let jitter_x = ((jitter & 0xffff) as f32 + 0.5) / 65_536.0;
                    let jitter_y = ((jitter >> 16) as f32 + 0.5) / 65_536.0;
                    let jitter_z = ((hash_voxel(y, x ^ salt, x.wrapping_add(y)) & 0xffff) as f32
                        + 0.5)
                        / 65_536.0;
                    let target = [
                        (x as f32 + 0.125 + 0.75 * jitter_x) * WORLD_DIMENSION as f32
                            / RAY_GRID as f32,
                        (y as f32 + 0.125 + 0.75 * jitter_y) * WORLD_DIMENSION as f32
                            / RAY_GRID as f32,
                        31.75 + 0.5 * jitter_z,
                    ];
                    let ray = make_ray([32.25, 31.75, -48.0], target);
                    (!ray_has_near_voxel_edge(ray)).then_some(ray)
                })
                .expect("deterministic jitter must find a non-degenerate throughput ray");
            rays.push(ray);
        }
    }
    let cases = [
        make_direction_ray([-8.0, 0.5, 0.5], [1.0, 0.0, 0.0]),
        make_direction_ray([72.0, 63.5, 63.5], [-1.0, 0.0, 0.0]),
        make_direction_ray([0.5, -8.0, 0.5], [0.0, 1.0, 0.0]),
        make_direction_ray([0.5, 0.5, -8.0], [0.0, 0.0, 1.0]),
        make_direction_ray([-8.0, -7.25, -6.5], [1.0, 0.97, 1.03]),
        make_direction_ray([-8.0, 23.9999, 31.5], [1.0, 0.0, 0.0]),
        make_direction_ray([-8.0, 24.0001, 31.5], [1.0, 0.0, 0.0]),
        make_direction_ray([32.5, 32.25, 32.75], [1.0, 0.17, 0.31]),
        make_direction_ray([32.5, 32.25, 32.75], [-0.23, 1.0, 0.11]),
        make_direction_ray([32.5, 32.25, 32.75], [0.19, -0.37, 1.0]),
        make_direction_ray([-8.0, 31.0001, 31.0003], [1.0, 0.0, 0.0]),
        make_direction_ray([72.0, 31.9997, 31.5002], [-1.0, 0.0, 0.0]),
    ];
    rays[..cases.len()].copy_from_slice(&cases);
    rays
}

fn ray_has_near_voxel_edge(ray: BulletRay) -> bool {
    const MIN_BOUNDARY_SEPARATION: f64 = 1.0e-3;
    let Some((entry, exit, _)) = ray_box(ray, [0.0; 3], [WORLD_DIMENSION as f32; 3]) else {
        return false;
    };
    let origin = ray.origin.map(f64::from);
    let direction = ray.direction.map(f64::from);
    let world_near: [f64; 3] = std::array::from_fn(|axis| {
        let bound = if direction[axis] < 0.0 {
            f64::from(WORLD_DIMENSION)
        } else {
            0.0
        };
        (bound - origin[axis]) / direction[axis]
    });
    let entry_axis = world_near
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| f64::total_cmp(left, right))
        .map(|(axis, _)| axis)
        .expect("a three-axis ray has an entry axis");
    let exact_entry = world_near[entry_axis];
    for axis in 0..3 {
        if axis == entry_axis {
            continue;
        }
        let coordinate = origin[axis] + direction[axis] * exact_entry;
        if (coordinate - coordinate.round()).abs() < MIN_BOUNDARY_SEPARATION {
            return true;
        }
    }
    let mut voxel: [i32; 3] = std::array::from_fn(|axis| {
        (origin[axis] + direction[axis] * (f64::from(entry) + 1.0e-4)).floor() as i32
    });
    let step = direction.map(|value| if value > 0.0 { 1 } else { -1 });
    let delta_t = direction.map(|value| {
        if value == 0.0 {
            f64::INFINITY
        } else {
            (1.0 / value).abs()
        }
    });
    let mut next_t: [f64; 3] = std::array::from_fn(|axis| {
        if direction[axis] == 0.0 {
            f64::INFINITY
        } else {
            let boundary = if step[axis] > 0 {
                voxel[axis] + 1
            } else {
                voxel[axis]
            };
            (f64::from(boundary) - origin[axis]) / direction[axis]
        }
    });
    for _ in 0..MAX_DDA_STEPS {
        if voxel
            .iter()
            .any(|&value| value < 0 || value >= WORLD_DIMENSION as i32)
        {
            return false;
        }
        let mut ordered = next_t;
        ordered.sort_by(f64::total_cmp);
        if ordered[1] - ordered[0] < MIN_BOUNDARY_SEPARATION {
            return true;
        }
        let axis = next_t
            .iter()
            .position(|&value| value == ordered[0])
            .expect("one DDA axis must have the minimum boundary time");
        if ordered[0] > f64::from(exit) {
            return false;
        }
        voxel[axis] += step[axis];
        next_t[axis] += delta_t[axis];
    }
    true
}

fn make_ray(origin: [f32; 3], target: [f32; 3]) -> BulletRay {
    make_direction_ray(
        origin,
        [
            target[0] - origin[0],
            target[1] - origin[1],
            target[2] - origin[2],
        ],
    )
}

fn make_direction_ray(origin: [f32; 3], direction: [f32; 3]) -> BulletRay {
    let length =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt();
    BulletRay {
        origin,
        t_min: 0.0,
        direction: [
            direction[0] / length,
            direction[1] / length,
            direction[2] / length,
        ],
        t_max: 256.0,
    }
}

fn cpu_reference(occupancy: &[u32], rays: &[BulletRay]) -> Vec<CpuHit> {
    rays.iter()
        .map(|&ray| cpu_trace_grid(occupancy, ray))
        .collect()
}

fn cpu_trace_grid(occupancy: &[u32], ray: BulletRay) -> CpuHit {
    let Some((entry, exit, mut current_face)) = ray_box(ray, [0.0; 3], [WORLD_DIMENSION as f32; 3])
    else {
        return miss(ray);
    };
    let mut current_t = entry;
    let position = [
        ray.origin[0] + ray.direction[0] * (current_t + 1.0e-4),
        ray.origin[1] + ray.direction[1] * (current_t + 1.0e-4),
        ray.origin[2] + ray.direction[2] * (current_t + 1.0e-4),
    ];
    let mut voxel = position.map(|value| value.floor() as i32);
    if voxel
        .iter()
        .any(|&value| value < 0 || value >= WORLD_DIMENSION as i32)
    {
        return miss(ray);
    }
    let step = ray.direction.map(|value| {
        if value > 0.0 {
            1
        } else if value < 0.0 {
            -1
        } else {
            0
        }
    });
    let delta_t: [f32; 3] = std::array::from_fn(|axis| {
        if step[axis] == 0 {
            1.0e30
        } else {
            (1.0 / ray.direction[axis]).abs()
        }
    });
    let mut next_t: [f32; 3] = std::array::from_fn(|axis| {
        if step[axis] == 0 {
            1.0e30
        } else {
            let boundary = if step[axis] > 0 {
                voxel[axis] + 1
            } else {
                voxel[axis]
            };
            (boundary as f32 - ray.origin[axis]) / ray.direction[axis]
        }
    });
    for _ in 0..MAX_DDA_STEPS {
        if voxel
            .iter()
            .any(|&value| value < 0 || value >= WORLD_DIMENSION as i32)
            || current_t > exit
        {
            break;
        }
        let index = voxel_index(voxel[0] as u32, voxel[1] as u32, voxel[2] as u32);
        if occupancy[index as usize] != 0 {
            return CpuHit {
                hit_t: current_t,
                voxel_index: index,
                face_index: current_face,
                normal_code: current_face,
            };
        }
        let advance_t = next_t.iter().copied().fold(f32::INFINITY, f32::min);
        let advance_axis = next_t
            .iter()
            .position(|&value| value == advance_t)
            .expect("finite DDA advance axis");
        for axis in 0..3 {
            if next_t[axis] == advance_t {
                voxel[axis] += step[axis];
                next_t[axis] += delta_t[axis];
            }
        }
        current_t = advance_t;
        current_face = entry_face(advance_axis, ray.direction[advance_axis]);
    }
    miss(ray)
}

fn miss(ray: BulletRay) -> CpuHit {
    CpuHit {
        hit_t: ray.t_max,
        voxel_index: INVALID_INDEX,
        face_index: INVALID_INDEX,
        normal_code: INVALID_INDEX,
    }
}

fn ray_box(ray: BulletRay, minimum: [f32; 3], maximum: [f32; 3]) -> Option<(f32, f32, u32)> {
    let mut raw_entry = f32::NEG_INFINITY;
    let mut exit = ray.t_max;
    let mut face = INVALID_INDEX;
    for axis in 0..3 {
        let direction = if ray.direction[axis].abs() < 1.0e-20 {
            if ray.direction[axis] < 0.0 {
                -1.0e-20
            } else {
                1.0e-20
            }
        } else {
            ray.direction[axis]
        };
        let t0 = (minimum[axis] - ray.origin[axis]) / direction;
        let t1 = (maximum[axis] - ray.origin[axis]) / direction;
        let near = t0.min(t1);
        if near > raw_entry {
            raw_entry = near;
            face = entry_face(axis, ray.direction[axis]);
        }
        exit = exit.min(t0.max(t1));
    }
    let entry = ray.t_min.max(raw_entry);
    if raw_entry < ray.t_min {
        face = INVALID_INDEX;
    }
    (entry <= exit).then_some((entry, exit, face))
}

fn entry_face(axis: usize, direction: f32) -> u32 {
    axis as u32 * 2 + u32::from(direction < 0.0)
}

fn voxel_index(x: u32, y: u32, z: u32) -> u32 {
    x + WORLD_DIMENSION * (y + WORLD_DIMENSION * z)
}

fn voxel_coordinate(index: u32) -> [u32; 3] {
    let plane = WORLD_DIMENSION * WORLD_DIMENSION;
    let z = index / plane;
    let remainder = index - z * plane;
    let y = remainder / WORLD_DIMENSION;
    [remainder - y * WORLD_DIMENSION, y, z]
}

fn hash_voxel(x: u32, y: u32, z: u32) -> u32 {
    let mut value = x.wrapping_mul(0x8da6_b343)
        ^ y.wrapping_mul(0xd816_3841)
        ^ z.wrapping_mul(0xcb1a_b31f)
        ^ 0x9e37_79b9;
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value.wrapping_mul(0x846c_a68b) ^ (value >> 16)
}

fn occupied_voxel_count(occupancy: &[u32]) -> u32 {
    occupancy.iter().filter(|&&value| value != 0).count() as u32
}

fn typed_buffer<T: Pod>(
    device: &Device,
    allocator: Allocator,
    values: &[T],
    usage: vk::BufferUsageFlags,
    location: MemoryLocation,
) -> Result<Buffer> {
    anyhow::ensure!(!values.is_empty(), "tracer-bullet buffer cannot be empty");
    let buffer = Buffer::new_sized(
        device.clone(),
        allocator,
        BufferUsage::from_flags(usage),
        location,
        std::mem::size_of_val(values) as u64,
    );
    buffer.fill_with_raw_u8(bytemuck::cast_slice(values))?;
    Ok(buffer)
}

fn static_live_resource_bytes(
    build: &StaticBuildEvidence,
    occupancy_bytes: u64,
    ray_bytes: u64,
    result_bytes: u64,
) -> u64 {
    occupancy_bytes
        + ray_bytes
        + result_bytes
        + build.triangle_metadata_bytes
        + build.aabb_metadata_bytes
        + build.triangle_blas.acceleration_structure_bytes
        + build.triangle_tlas.acceleration_structure_bytes
        + build.aabb_blas.acceleration_structure_bytes
        + build.aabb_tlas.acceleration_structure_bytes
}

fn build_peak_accounted_bytes(
    build: &StaticBuildEvidence,
    occupancy_bytes: u64,
    ray_bytes: u64,
    result_bytes: u64,
) -> u64 {
    static_live_resource_bytes(build, occupancy_bytes, ray_bytes, result_bytes)
        + build.triangle_vertex_input_bytes
        + build.triangle_index_input_bytes
        + build.aabb_input_bytes
        + build.triangle_blas.scratch_bytes
        + build.triangle_tlas.scratch_bytes
        + build.aabb_blas.scratch_bytes
        + build.aabb_tlas.scratch_bytes
}

fn log_volume_summary(
    kind: VolumeKind,
    build: &StaticBuildEvidence,
    samples: &[TraversalSample],
    triangle_primitives: u32,
    aabb_primitives: u32,
) {
    let median = |mode| {
        let mut values = samples
            .iter()
            .filter(|sample| sample.mode == mode)
            .map(|sample| sample.gpu_ms)
            .collect::<Vec<_>>();
        values.sort_by(f64::total_cmp);
        (values[values.len() / 2 - 1] + values[values.len() / 2]) * 0.5
    };
    let software = median(TraversalMode::SoftwareDda);
    let aabb = median(TraversalMode::VoxelAabbExact);
    let triangles = median(TraversalMode::ExposedFaceTriangles);
    log::info!(
        "[RTX_STATIC_TRACER_BULLET][PROTOTYPE][RESULT] volume={} rays={} triangles={} aabbs={} triangle_blas_gpu_ms={:.6} aabb_blas_gpu_ms={:.6} software_ms={:.6} aabb_ms={:.6} triangle_ms={:.6} triangle_speedup={:.4} aabb_speedup={:.4}",
        kind.name(),
        RAY_COUNT,
        triangle_primitives,
        aabb_primitives,
        build.triangle_blas.gpu_build_ms,
        build.aabb_blas.gpu_build_ms,
        software,
        aabb,
        triangles,
        software / triangles,
        software / aabb,
    );
}

fn machine_identity(vulkan_ctx: &VulkanContext) -> Result<MachineIdentity> {
    let instance = vulkan_ctx.instance().as_raw();
    let physical_device = vulkan_ctx.physical_device().as_raw();
    let properties = unsafe { instance.get_physical_device_properties(physical_device) };
    let mut id_properties = vk::PhysicalDeviceIDProperties::default();
    let mut properties2 = vk::PhysicalDeviceProperties2::default().push_next(&mut id_properties);
    let mut as_features = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
    let mut ray_query_features = vk::PhysicalDeviceRayQueryFeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut as_features)
        .push_next(&mut ray_query_features);
    unsafe {
        instance.get_physical_device_properties2(physical_device, &mut properties2);
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    let extensions = unsafe { instance.enumerate_device_extension_properties(physical_device) }?;
    let has_extension = |expected: &CStr| {
        extensions.iter().any(|extension| {
            (unsafe { CStr::from_ptr(extension.extension_name.as_ptr()) }) == expected
        })
    };
    Ok(MachineIdentity {
        device_name: unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
        vendor_id: properties.vendor_id,
        device_id: properties.device_id,
        device_type: format!("{:?}", properties.device_type),
        api_version: format!(
            "{}.{}.{}",
            vk::api_version_major(properties.api_version),
            vk::api_version_minor(properties.api_version),
            vk::api_version_patch(properties.api_version),
        ),
        driver_version_raw: properties.driver_version,
        device_uuid_hex: hex_bytes(&id_properties.device_uuid),
        driver_uuid_hex: hex_bytes(&id_properties.driver_uuid),
        timestamp_period_ns: properties.limits.timestamp_period,
        acceleration_structure_extension: has_extension(vk::KHR_ACCELERATION_STRUCTURE_NAME),
        ray_query_extension: has_extension(vk::KHR_RAY_QUERY_NAME),
        ray_tracing_pipeline_extension: has_extension(vk::KHR_RAY_TRACING_PIPELINE_NAME),
        acceleration_structure_feature: as_features.acceleration_structure == vk::TRUE,
        ray_query_feature: ray_query_features.ray_query == vk::TRUE,
    })
}

fn device_local_heap_usage_bytes(vulkan_ctx: &VulkanContext) -> u64 {
    let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
    let mut properties = vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget);
    unsafe {
        vulkan_ctx
            .instance()
            .as_raw()
            .get_physical_device_memory_properties2(
                vulkan_ctx.physical_device().as_raw(),
                &mut properties,
            );
    }
    (0..properties.memory_properties.memory_heap_count as usize)
        .filter(|&index| {
            properties.memory_properties.memory_heaps[index]
                .flags
                .contains(vk::MemoryHeapFlags::DEVICE_LOCAL)
        })
        .map(|index| budget.heap_usage[index])
        .sum()
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposed_faces_map_two_triangles_back_to_the_surface_voxel() {
        let mut occupancy = vec![0; WORLD_DIMENSION.pow(3) as usize];
        occupancy[voxel_index(3, 4, 5) as usize] = 1;

        let (vertices, indices, faces, surface_voxels) = extract_exposed_faces(&occupancy);

        assert_eq!(surface_voxels, vec![voxel_index(3, 4, 5)]);
        assert_eq!(faces.len(), 6);
        assert_eq!(vertices.len(), 24);
        assert_eq!(indices.len(), 36);
        assert!(faces
            .iter()
            .all(|face| face.voxel_index == voxel_index(3, 4, 5)));
    }

    #[test]
    fn fixed_sample_order_balances_all_three_paths() {
        let order = sample_order();

        assert_eq!(order.len(), 36);
        for mode in TraversalMode::ALL {
            assert_eq!(order.iter().filter(|&&sample| sample == mode).count(), 12);
        }
    }

    #[test]
    fn cpu_reference_covers_hits_misses_faces_and_cavity() {
        let occupancy = make_volume(VolumeKind::ShellCavity);
        let reference = cpu_reference(&occupancy, &benchmark_rays());

        assert!(reference.iter().any(|hit| hit.voxel_index == INVALID_INDEX));
        assert!(reference.iter().any(|hit| hit.voxel_index != INVALID_INDEX));
        assert!(reference
            .iter()
            .filter(|hit| hit.voxel_index != INVALID_INDEX)
            .all(|hit| hit.face_index < 6 && hit.normal_code == hit.face_index));
    }

    #[test]
    fn binary_layout_matches_shader_contract() {
        assert_eq!(std::mem::size_of::<Vertex>(), 12);
        assert_eq!(std::mem::size_of::<Aabb>(), 24);
        assert_eq!(std::mem::size_of::<FaceData>(), 16);
        assert_eq!(std::mem::size_of::<BulletRay>(), 32);
        assert_eq!(std::mem::size_of::<BulletResult>(), 48);
        assert_eq!(std::mem::size_of::<BulletPushConstants>(), 16);
    }
}
