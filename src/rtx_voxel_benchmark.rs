//! Default-off, synthetic hardware ray-query experiment.
//!
//! This is deliberately not a second terrain lifecycle. It owns one immutable synthetic
//! acceleration snapshot at a time, validates it, records measurements, and drops it before
//! production rendering starts.

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use re_flora_vkn::{
    build_aabb_blas_profiled, build_tlas_profiled, execute_one_time_command, khr, vk, AccelStruct,
    Allocator, Buffer, BufferUsage, ComputePipeline, DescriptorPool, Device, Extent3D,
    MemoryLocation, PipelineStage, Resource, ShaderModule, TimestampQueryPool, VulkanContext,
};
use resource_container_derive::ResourceContainer;
use serde::Serialize;
use std::{ffi::CStr, path::Path, time::Instant};

const WORLD_DIMENSION: u32 = 32;
const RAY_GRID: u32 = 256;
const RAY_COUNT: u32 = RAY_GRID * RAY_GRID;
const CANDIDATE_BUDGET: u32 = 4096;
const MAX_DDA_STEPS: u32 = 512;
const INVALID_VOXEL: u32 = u32::MAX;
const DENSITIES_PERCENT: [u32; 3] = [5, 25, 75];
const MACRO_DIMENSIONS: [u32; 3] = [2, 4, 8];
const SAMPLE_ORDER: [TraversalMode; 4] = [
    TraversalMode::Software,
    TraversalMode::Hardware,
    TraversalMode::Hardware,
    TraversalMode::Software,
];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BenchmarkRay {
    origin: [f32; 3],
    t_min: f32,
    direction: [f32; 3],
    t_max: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BenchmarkResult {
    hit_t: f32,
    voxel_index: u32,
    candidate_count: u32,
    rejected_candidate_count: u32,
    committed_candidate_count: u32,
    traversal_exhausted: u32,
    primitive_index: u32,
    padding: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BenchmarkPushConstants {
    mode: u32,
    world_dimension: u32,
    macro_dimension: u32,
    ray_count: u32,
    candidate_budget: u32,
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
struct MacroData {
    xyz: [u32; 3],
    padding: u32,
}

#[derive(ResourceContainer)]
struct BenchmarkResources {
    tlas: Resource<AccelStruct>,
    macro_data: Resource<Buffer>,
    occupancy: Resource<Buffer>,
    rays: Resource<Buffer>,
    results: Resource<Buffer>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TraversalMode {
    Software,
    Hardware,
}

impl TraversalMode {
    fn shader_value(self) -> u32 {
        match self {
            Self::Software => 0,
            Self::Hardware => 1,
        }
    }
}

#[derive(Serialize)]
struct Artifact {
    schema: &'static str,
    fixed_baseline: &'static str,
    generated_at: String,
    command: Vec<String>,
    machine: MachineIdentity,
    workload: WorkloadIdentity,
    coverage: Vec<&'static str>,
    memory_before_bytes: u64,
    configurations: Vec<ConfigurationResult>,
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
    sample_order: Vec<&'static str>,
    density_percent: Vec<u32>,
    macro_dimensions: Vec<u32>,
}

#[derive(Serialize)]
struct ConfigurationResult {
    requested_density_percent: u32,
    actual_occupied_voxels_before: u32,
    actual_occupied_voxels_after: u32,
    macro_dimension: u32,
    initial: PhaseResult,
    after_edit: PhaseResult,
    edit: EditResult,
    peak_device_local_heap_usage_bytes: u64,
    logical_live_resource_bytes: u64,
}

#[derive(Serialize)]
struct PhaseResult {
    occupied_macro_count: u32,
    aabb_input_bytes: u64,
    macro_metadata_bytes: u64,
    blas: AsBuildResult,
    tlas: AsBuildResult,
    samples: Vec<TraversalSample>,
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
    gpu_ms: f64,
    host_wait_ms: f64,
    hit_count: u32,
    candidate_count: u64,
    rejected_candidate_count: u64,
    committed_candidate_count: u64,
    traversal_exhausted_count: u32,
    query_committed_disagreement_count: u32,
    correctness: CorrectnessResult,
}

#[derive(Serialize)]
struct CorrectnessResult {
    reference_ray_count: u32,
    false_positive_count: u32,
    false_negative_count: u32,
    wrong_voxel_count: u32,
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
    expected_hit_t: f32,
    actual_hit_t: f32,
    actual_primitive_index: u32,
    candidate_count: u32,
    rejected_candidate_count: u32,
    committed_candidate_count: u32,
    query_committed_disagreement: bool,
}

#[derive(Serialize)]
struct EditResult {
    cleared_macro_min: [u32; 3],
    cleared_voxel_count: u32,
    topology_changed: bool,
    update_mode: &'static str,
    reason: &'static str,
}

struct PhaseResources {
    pipeline: ComputePipeline,
    resources: BenchmarkResources,
    phase: PhaseResult,
}

pub(crate) fn run(vulkan_ctx: &VulkanContext, allocator: Allocator, output: &Path) -> Result<()> {
    let machine = machine_identity(vulkan_ctx)?;
    anyhow::ensure!(
        machine.acceleration_structure_extension
            && machine.ray_query_extension
            && machine.acceleration_structure_feature
            && machine.ray_query_feature,
        "selected device did not expose the enabled acceleration-structure/ray-query capability"
    );
    log::info!(
        "[RTX_VOXEL][CAPABILITY] device={} as_ext={} ray_query_ext={} ray_pipeline_ext={} as_feature={} ray_query_feature={}",
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
        "shader/experiments/rtx_voxel_benchmark.comp",
        "main",
    )
    .map_err(anyhow::Error::msg)
    .context("load RTX voxel benchmark shader")?;
    let descriptor_pool = DescriptorPool::new_for_hardware_ray_query(vulkan_ctx.device())
        .context("create RTX descriptor pool")?;
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
        u64::from(RAY_COUNT) * std::mem::size_of::<BenchmarkResult>() as u64,
    );

    let mut configurations = Vec::new();
    for requested_density_percent in DENSITIES_PERCENT {
        for macro_dimension in MACRO_DIMENSIONS {
            let mut occupancy = make_occupancy(requested_density_percent);
            add_correctness_fixture(&mut occupancy);
            let actual_occupied_voxels_before = occupied_voxel_count(&occupancy);
            let references_before = cpu_reference(&occupancy, &rays);
            let occupancy_buffer = typed_buffer(
                vulkan_ctx.device(),
                allocator.clone(),
                &occupancy,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                MemoryLocation::CpuToGpu,
            )?;

            let mut initial = build_phase(
                vulkan_ctx,
                allocator.clone(),
                &acc_device,
                &shader,
                &descriptor_pool,
                occupancy_buffer.clone(),
                ray_buffer.clone(),
                result_buffer.clone(),
                &occupancy,
                macro_dimension,
            )?;
            warm_up(
                vulkan_ctx,
                &initial.pipeline,
                macro_dimension,
                TraversalMode::Software,
            );
            warm_up(
                vulkan_ctx,
                &initial.pipeline,
                macro_dimension,
                TraversalMode::Hardware,
            );
            initial.phase.samples = sample_sequence(
                vulkan_ctx,
                &initial.pipeline,
                &initial.resources.results,
                macro_dimension,
                &references_before,
            )?;
            let initial_heap_usage_bytes = device_local_heap_usage_bytes(vulkan_ctx);
            let PhaseResources {
                pipeline: initial_pipeline,
                resources: initial_resources,
                phase: initial_phase,
            } = initial;
            drop((initial_pipeline, initial_resources));

            let cleared_macro_min = [
                (8 / macro_dimension) * macro_dimension,
                (8 / macro_dimension) * macro_dimension,
                (8 / macro_dimension) * macro_dimension,
            ];
            let cleared_voxel_count =
                clear_macro(&mut occupancy, cleared_macro_min, macro_dimension);
            occupancy_buffer
                .fill_with_raw_u8(bytemuck::cast_slice(&occupancy))
                .context("publish edited occupancy")?;
            let actual_occupied_voxels_after = occupied_voxel_count(&occupancy);
            let references_after = cpu_reference(&occupancy, &rays);

            let mut after_edit = build_phase(
                vulkan_ctx,
                allocator.clone(),
                &acc_device,
                &shader,
                &descriptor_pool,
                occupancy_buffer.clone(),
                ray_buffer.clone(),
                result_buffer.clone(),
                &occupancy,
                macro_dimension,
            )?;
            warm_up(
                vulkan_ctx,
                &after_edit.pipeline,
                macro_dimension,
                TraversalMode::Software,
            );
            warm_up(
                vulkan_ctx,
                &after_edit.pipeline,
                macro_dimension,
                TraversalMode::Hardware,
            );
            after_edit.phase.samples = sample_sequence(
                vulkan_ctx,
                &after_edit.pipeline,
                &after_edit.resources.results,
                macro_dimension,
                &references_after,
            )?;

            let peak_device_local_heap_usage_bytes =
                initial_heap_usage_bytes.max(device_local_heap_usage_bytes(vulkan_ctx));
            let logical_live_resource_bytes = logical_resource_bytes(
                &initial_phase,
                u64::from(WORLD_DIMENSION.pow(3)) * 4,
                ray_buffer.get_size_bytes(),
                result_buffer.get_size_bytes(),
            )
            .max(logical_resource_bytes(
                &after_edit.phase,
                u64::from(WORLD_DIMENSION.pow(3)) * 4,
                ray_buffer.get_size_bytes(),
                result_buffer.get_size_bytes(),
            ));
            log_phase_summary(
                requested_density_percent,
                macro_dimension,
                "initial",
                &initial_phase,
            );
            log_phase_summary(
                requested_density_percent,
                macro_dimension,
                "after_edit",
                &after_edit.phase,
            );
            configurations.push(ConfigurationResult {
                requested_density_percent,
                actual_occupied_voxels_before,
                actual_occupied_voxels_after,
                macro_dimension,
                initial: initial_phase,
                after_edit: after_edit.phase,
                edit: EditResult {
                    cleared_macro_min,
                    cleared_voxel_count,
                    topology_changed: true,
                    update_mode: "blas_rebuild_and_tlas_rebuild",
                    reason: "the edit removes one AABB primitive, so primitive count changes and Vulkan UPDATE/refit is not legal",
                },
                peak_device_local_heap_usage_bytes,
                logical_live_resource_bytes,
            });
        }
    }

    let artifact = Artifact {
        schema: "re-flora.rtx-voxel-hardware-ray-query.v1",
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
            sample_order: vec!["software", "hardware", "hardware", "software"],
            density_percent: DENSITIES_PERCENT.to_vec(),
            macro_dimensions: MACRO_DIMENSIONS.to_vec(),
        },
        coverage: vec![
            "surface",
            "cavity",
            "world_boundary",
            "axis_parallel",
            "diagonal",
            "grazing",
            "macro_seam",
            "dynamic_edit",
        ],
        memory_before_bytes,
        configurations,
    };
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create benchmark output directory {}", parent.display()))?;
    }
    std::fs::write(output, toml::to_string_pretty(&artifact)?)
        .with_context(|| format!("write benchmark artifact {}", output.display()))?;
    log::info!(
        "[RTX_VOXEL][COMPLETE] artifact={} configurations={} rays_per_sample={}",
        output.display(),
        artifact.configurations.len(),
        RAY_COUNT,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_phase(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    acc_device: &khr::acceleration_structure::Device,
    shader: &ShaderModule,
    descriptor_pool: &DescriptorPool,
    occupancy: Buffer,
    rays: Buffer,
    results: Buffer,
    occupancy_values: &[u32],
    macro_dimension: u32,
) -> Result<PhaseResources> {
    let (aabbs, macros) = occupied_macros(occupancy_values, macro_dimension);
    let aabb_buffer = typed_buffer(
        vulkan_ctx.device(),
        allocator.clone(),
        &aabbs,
        vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        MemoryLocation::CpuToGpu,
    )?;
    let macro_buffer = typed_buffer(
        vulkan_ctx.device(),
        allocator.clone(),
        &macros,
        vk::BufferUsageFlags::STORAGE_BUFFER,
        MemoryLocation::CpuToGpu,
    )?;
    let blas = build_aabb_blas_profiled(
        vulkan_ctx,
        allocator.clone(),
        acc_device.clone(),
        &aabb_buffer,
        aabbs.len() as u32,
    );
    let instance = vk::AccelerationStructureInstanceKHR {
        transform: vk::TransformMatrixKHR {
            matrix: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        },
        instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xff),
        instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(0, 0),
        acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
            device_handle: blas.acceleration_structure.get_device_address(),
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
    let tlas = build_tlas_profiled(
        vulkan_ctx,
        allocator,
        acc_device.clone(),
        &instance_buffer,
        1,
    );
    let resources = BenchmarkResources {
        tlas: Resource::new(tlas.acceleration_structure.clone()),
        macro_data: Resource::new(macro_buffer),
        occupancy: Resource::new(occupancy),
        rays: Resource::new(rays),
        results: Resource::new(results),
    };
    let pipeline =
        ComputePipeline::new(vulkan_ctx.device(), shader, descriptor_pool, &[&resources]);
    let phase = PhaseResult {
        occupied_macro_count: aabbs.len() as u32,
        aabb_input_bytes: aabb_buffer.get_size_bytes(),
        macro_metadata_bytes: resources.macro_data.get_size_bytes(),
        blas: AsBuildResult {
            primitive_count: aabbs.len() as u32,
            acceleration_structure_bytes: blas.acceleration_structure_bytes,
            scratch_bytes: blas.scratch_bytes,
            host_build_ms: blas.host_build_ms,
            gpu_build_ms: blas.gpu_build_ms,
        },
        tlas: AsBuildResult {
            primitive_count: 1,
            acceleration_structure_bytes: tlas.acceleration_structure_bytes,
            scratch_bytes: tlas.scratch_bytes,
            host_build_ms: tlas.host_build_ms,
            gpu_build_ms: tlas.gpu_build_ms,
        },
        samples: Vec::new(),
    };
    Ok(PhaseResources {
        pipeline,
        resources,
        phase,
    })
}

fn warm_up(
    vulkan_ctx: &VulkanContext,
    pipeline: &ComputePipeline,
    macro_dimension: u32,
    mode: TraversalMode,
) {
    let _ = dispatch(vulkan_ctx, pipeline, macro_dimension, mode);
}

fn sample_sequence(
    vulkan_ctx: &VulkanContext,
    pipeline: &ComputePipeline,
    result_buffer: &Buffer,
    macro_dimension: u32,
    reference: &[CpuHit],
) -> Result<Vec<TraversalSample>> {
    SAMPLE_ORDER
        .into_iter()
        .enumerate()
        .map(|(order_index, mode)| {
            let (gpu_ms, host_wait_ms) = dispatch(vulkan_ctx, pipeline, macro_dimension, mode);
            let bytes = result_buffer
                .read_back()
                .context("read benchmark results")?;
            let results = bytemuck::try_cast_slice::<u8, BenchmarkResult>(&bytes)
                .map_err(|error| anyhow::anyhow!("decode benchmark results: {error}"))?;
            Ok(summarize_sample(
                order_index as u32,
                mode,
                gpu_ms,
                host_wait_ms,
                results,
                reference,
            ))
        })
        .collect()
}

fn dispatch(
    vulkan_ctx: &VulkanContext,
    pipeline: &ComputePipeline,
    macro_dimension: u32,
    mode: TraversalMode,
) -> (f64, f64) {
    let timestamps = TimestampQueryPool::maybe_new(vulkan_ctx, 2, "RTX_VOXEL_TRAVERSAL")
        .expect("RTX benchmark requires compute timestamps");
    let constants = BenchmarkPushConstants {
        mode: mode.shader_value(),
        world_dimension: WORLD_DIMENSION,
        macro_dimension,
        ray_count: RAY_COUNT,
        candidate_budget: CANDIDATE_BUDGET,
    };
    let started = Instant::now();
    execute_one_time_command(
        vulkan_ctx.device(),
        vulkan_ctx.command_pool(),
        &vulkan_ctx.get_general_queue(),
        |cmdbuf| {
            timestamps.record_reset(cmdbuf, 2);
            timestamps.record_timestamp(cmdbuf, PipelineStage::TOP_OF_PIPE, 0);
            pipeline.record(
                cmdbuf,
                Extent3D::new(RAY_COUNT, 1, 1),
                Some(bytemuck::bytes_of(&constants)),
            );
            timestamps.record_timestamp(cmdbuf, PipelineStage::BOTTOM_OF_PIPE, 1);
        },
    );
    let host_wait_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let values = timestamps
        .read_u64(2)
        .expect("RTX traversal timestamp query must be ready after queue idle");
    let gpu_ms = values[1].saturating_sub(values[0]) as f64
        * timestamps.timestamp_period_ns() as f64
        / 1_000_000.0;
    (gpu_ms, host_wait_ms)
}

fn summarize_sample(
    order_index: u32,
    mode: TraversalMode,
    gpu_ms: f64,
    host_wait_ms: f64,
    results: &[BenchmarkResult],
    reference: &[CpuHit],
) -> TraversalSample {
    const TOLERANCE: f32 = 2.0e-3;
    let mut false_positive_count = 0;
    let mut false_negative_count = 0;
    let mut wrong_voxel_count = 0;
    let mut hit_t_mismatch_count = 0;
    let mut max_hit_t_error = 0.0f32;
    let mut first_mismatches = Vec::new();
    for (index, (actual, expected)) in results.iter().zip(reference).enumerate() {
        let actual_hit = actual.voxel_index != INVALID_VOXEL;
        let expected_hit = expected.voxel_index != INVALID_VOXEL;
        if actual_hit && !expected_hit {
            false_positive_count += 1;
        } else if !actual_hit && expected_hit {
            false_negative_count += 1;
        } else if actual_hit && actual.voxel_index != expected.voxel_index {
            wrong_voxel_count += 1;
        }
        if actual_hit && expected_hit {
            let error = (actual.hit_t - expected.hit_t).abs();
            max_hit_t_error = max_hit_t_error.max(error);
            if error > TOLERANCE {
                hit_t_mismatch_count += 1;
            }
        }
        let mismatch = actual_hit != expected_hit
            || (actual_hit && actual.voxel_index != expected.voxel_index)
            || (actual_hit && expected_hit && (actual.hit_t - expected.hit_t).abs() > TOLERANCE);
        if mismatch && first_mismatches.len() < 16 {
            first_mismatches.push(MismatchDetail {
                ray_index: index as u32,
                expected_voxel: expected.voxel_index,
                actual_voxel: actual.voxel_index,
                expected_hit_t: expected.hit_t,
                actual_hit_t: actual.hit_t,
                actual_primitive_index: actual.primitive_index,
                candidate_count: actual.candidate_count,
                rejected_candidate_count: actual.rejected_candidate_count,
                committed_candidate_count: actual.committed_candidate_count,
                query_committed_disagreement: actual.padding != 0,
            });
        }
    }
    TraversalSample {
        order_index,
        mode,
        gpu_ms,
        host_wait_ms,
        hit_count: results
            .iter()
            .filter(|result| result.voxel_index != INVALID_VOXEL)
            .count() as u32,
        candidate_count: results
            .iter()
            .map(|result| u64::from(result.candidate_count))
            .sum(),
        rejected_candidate_count: results
            .iter()
            .map(|result| u64::from(result.rejected_candidate_count))
            .sum(),
        committed_candidate_count: results
            .iter()
            .map(|result| u64::from(result.committed_candidate_count))
            .sum(),
        traversal_exhausted_count: results
            .iter()
            .filter(|result| result.traversal_exhausted != 0)
            .count() as u32,
        query_committed_disagreement_count: results
            .iter()
            .filter(|result| result.padding != 0)
            .count() as u32,
        correctness: CorrectnessResult {
            reference_ray_count: reference.len() as u32,
            false_positive_count,
            false_negative_count,
            wrong_voxel_count,
            hit_t_mismatch_count,
            max_hit_t_error,
            hit_t_tolerance: TOLERANCE,
            first_mismatches,
        },
    }
}

#[derive(Clone, Copy)]
struct CpuHit {
    hit_t: f32,
    voxel_index: u32,
}

fn cpu_reference(occupancy: &[u32], rays: &[BenchmarkRay]) -> Vec<CpuHit> {
    rays.iter()
        .map(|ray| cpu_trace_voxel_range(occupancy, *ray, [0, 0, 0], [WORLD_DIMENSION as i32; 3]))
        .collect()
}

fn cpu_trace_voxel_range(
    occupancy: &[u32],
    ray: BenchmarkRay,
    lower: [i32; 3],
    upper: [i32; 3],
) -> CpuHit {
    let Some((entry, exit)) = ray_box(
        ray,
        [lower[0] as f32, lower[1] as f32, lower[2] as f32],
        [upper[0] as f32, upper[1] as f32, upper[2] as f32],
    ) else {
        return CpuHit {
            hit_t: ray.t_max,
            voxel_index: INVALID_VOXEL,
        };
    };
    let mut current_t = ray.t_min.max(entry);
    let position = [
        ray.origin[0] + ray.direction[0] * (current_t + 1.0e-4),
        ray.origin[1] + ray.direction[1] * (current_t + 1.0e-4),
        ray.origin[2] + ray.direction[2] * (current_t + 1.0e-4),
    ];
    let mut voxel = [0i32; 3];
    let mut step = [0i32; 3];
    let mut delta_t = [0.0f32; 3];
    let mut next_t = [0.0f32; 3];
    for axis in 0..3 {
        voxel[axis] = position[axis].floor() as i32;
        step[axis] = if ray.direction[axis] > 0.0 {
            1
        } else if ray.direction[axis] < 0.0 {
            -1
        } else {
            0
        };
        delta_t[axis] = if step[axis] == 0 {
            1.0e30
        } else {
            (1.0 / ray.direction[axis]).abs()
        };
        let boundary = if step[axis] > 0 {
            voxel[axis] + 1
        } else {
            voxel[axis]
        } as f32;
        next_t[axis] = if step[axis] == 0 {
            1.0e30
        } else {
            (boundary - ray.origin[axis]) / ray.direction[axis]
        };
    }
    if (0..3).any(|axis| voxel[axis] < lower[axis] || voxel[axis] >= upper[axis]) {
        return CpuHit {
            hit_t: ray.t_max,
            voxel_index: INVALID_VOXEL,
        };
    }
    for _ in 0..MAX_DDA_STEPS {
        if (0..3).any(|axis| voxel[axis] < lower[axis] || voxel[axis] >= upper[axis])
            || current_t > exit
        {
            break;
        }
        let index = voxel_index(voxel[0] as u32, voxel[1] as u32, voxel[2] as u32);
        if occupancy[index as usize] != 0 {
            let voxel_exit_t = exit.min(next_t[0].min(next_t[1]).min(next_t[2]));
            if voxel_exit_t > current_t + 1.0e-6 {
                return CpuHit {
                    hit_t: current_t,
                    voxel_index: index,
                };
            }
        }
        let advance_t = next_t[0].min(next_t[1]).min(next_t[2]);
        for axis in 0..3 {
            if next_t[axis] <= advance_t + 1.0e-5 {
                voxel[axis] += step[axis];
                next_t[axis] += delta_t[axis];
            }
        }
        current_t = advance_t;
    }
    CpuHit {
        hit_t: ray.t_max,
        voxel_index: INVALID_VOXEL,
    }
}

fn ray_box(ray: BenchmarkRay, minimum: [f32; 3], maximum: [f32; 3]) -> Option<(f32, f32)> {
    let mut entry = ray.t_min;
    let mut exit = ray.t_max;
    for axis in 0..3 {
        let safe_direction = if ray.direction[axis].abs() < 1.0e-20 {
            if ray.direction[axis] < 0.0 {
                -1.0e-20
            } else {
                1.0e-20
            }
        } else {
            ray.direction[axis]
        };
        let t0 = (minimum[axis] - ray.origin[axis]) / safe_direction;
        let t1 = (maximum[axis] - ray.origin[axis]) / safe_direction;
        entry = entry.max(t0.min(t1));
        exit = exit.min(t0.max(t1));
    }
    (entry <= exit).then_some((entry, exit))
}

fn benchmark_rays() -> Vec<BenchmarkRay> {
    let mut rays = Vec::with_capacity(RAY_COUNT as usize);
    for y in 0..RAY_GRID {
        for x in 0..RAY_GRID {
            let target = [
                (x as f32 + 0.5) * WORLD_DIMENSION as f32 / RAY_GRID as f32,
                (y as f32 + 0.5) * WORLD_DIMENSION as f32 / RAY_GRID as f32,
                16.0,
            ];
            rays.push(make_ray([16.0, 16.0, -24.0], target, 96.0));
        }
    }
    let cases = [
        make_direction_ray([16.5, 16.5, -4.0], [0.0, 0.0, 1.0]),
        make_direction_ray([16.5, 16.5, 16.5], [1.0, 0.0, 0.0]),
        make_direction_ray([-2.0, 0.5, 0.5], [1.0, 0.0, 0.0]),
        make_direction_ray([34.0, 31.5, 31.5], [-1.0, 0.0, 0.0]),
        make_direction_ray([-2.0, -2.0, -2.0], [1.0, 1.0, 1.0]),
        make_direction_ray([-2.0, 5.9999, 5.5], [1.0, 0.0, 0.0]),
        make_direction_ray([-2.0, 5.5, 5.5], [1.0, 0.0, 0.0]),
        make_direction_ray([8.5, -2.0, 8.5], [0.0, 1.0, 0.0]),
        make_direction_ray([15.9999, 16.0, -2.0], [0.0, 0.0, 1.0]),
        make_direction_ray([16.0001, 16.0, -2.0], [0.0, 0.0, 1.0]),
    ];
    rays[..cases.len()].copy_from_slice(&cases);
    rays
}

fn make_ray(origin: [f32; 3], target: [f32; 3], t_max: f32) -> BenchmarkRay {
    make_direction_ray_with_max(
        origin,
        [
            target[0] - origin[0],
            target[1] - origin[1],
            target[2] - origin[2],
        ],
        t_max,
    )
}

fn make_direction_ray(origin: [f32; 3], direction: [f32; 3]) -> BenchmarkRay {
    make_direction_ray_with_max(origin, direction, 96.0)
}

fn make_direction_ray_with_max(origin: [f32; 3], direction: [f32; 3], t_max: f32) -> BenchmarkRay {
    let length =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt();
    BenchmarkRay {
        origin,
        t_min: 0.0,
        direction: [
            direction[0] / length,
            direction[1] / length,
            direction[2] / length,
        ],
        t_max,
    }
}

fn make_occupancy(density_percent: u32) -> Vec<u32> {
    let mut occupancy = vec![0u32; WORLD_DIMENSION.pow(3) as usize];
    for z in 0..WORLD_DIMENSION {
        for y in 0..WORLD_DIMENSION {
            for x in 0..WORLD_DIMENSION {
                let hash = hash_voxel(x, y, z) % 10_000;
                if hash < density_percent * 100 {
                    occupancy[voxel_index(x, y, z) as usize] = 1;
                }
            }
        }
    }
    occupancy
}

fn add_correctness_fixture(occupancy: &mut [u32]) {
    for coordinate in 0..WORLD_DIMENSION {
        set_voxel(
            occupancy,
            coordinate,
            (coordinate + 2) % WORLD_DIMENSION,
            coordinate,
            1,
        );
    }
    for x in 12..=20 {
        for y in 12..=20 {
            for z in 12..=20 {
                let shell = x == 12 || x == 20 || y == 12 || y == 20 || z == 12 || z == 20;
                set_voxel(occupancy, x, y, z, u32::from(shell));
            }
        }
    }
    for seam in [2, 4, 8, 16, 24, 30] {
        set_voxel(occupancy, seam, 5, 5, 1);
    }
    set_voxel(occupancy, 0, 0, 0, 1);
    set_voxel(occupancy, 31, 31, 31, 1);
}

fn occupied_macros(occupancy: &[u32], macro_dimension: u32) -> (Vec<Aabb>, Vec<MacroData>) {
    let mut aabbs = Vec::new();
    let mut macros = Vec::new();
    for z in (0..WORLD_DIMENSION).step_by(macro_dimension as usize) {
        for y in (0..WORLD_DIMENSION).step_by(macro_dimension as usize) {
            for x in (0..WORLD_DIMENSION).step_by(macro_dimension as usize) {
                let occupied = (z..(z + macro_dimension).min(WORLD_DIMENSION)).any(|vz| {
                    (y..(y + macro_dimension).min(WORLD_DIMENSION)).any(|vy| {
                        (x..(x + macro_dimension).min(WORLD_DIMENSION))
                            .any(|vx| occupancy[voxel_index(vx, vy, vz) as usize] != 0)
                    })
                });
                if occupied {
                    aabbs.push(Aabb {
                        min_x: x as f32,
                        min_y: y as f32,
                        min_z: z as f32,
                        max_x: (x + macro_dimension).min(WORLD_DIMENSION) as f32,
                        max_y: (y + macro_dimension).min(WORLD_DIMENSION) as f32,
                        max_z: (z + macro_dimension).min(WORLD_DIMENSION) as f32,
                    });
                    macros.push(MacroData {
                        xyz: [x, y, z],
                        padding: 0,
                    });
                }
            }
        }
    }
    (aabbs, macros)
}

fn clear_macro(occupancy: &mut [u32], minimum: [u32; 3], macro_dimension: u32) -> u32 {
    let mut cleared = 0;
    for z in minimum[2]..(minimum[2] + macro_dimension).min(WORLD_DIMENSION) {
        for y in minimum[1]..(minimum[1] + macro_dimension).min(WORLD_DIMENSION) {
            for x in minimum[0]..(minimum[0] + macro_dimension).min(WORLD_DIMENSION) {
                let value = &mut occupancy[voxel_index(x, y, z) as usize];
                cleared += u32::from(*value != 0);
                *value = 0;
            }
        }
    }
    cleared
}

fn set_voxel(occupancy: &mut [u32], x: u32, y: u32, z: u32, value: u32) {
    occupancy[voxel_index(x, y, z) as usize] = value;
}

fn voxel_index(x: u32, y: u32, z: u32) -> u32 {
    x + WORLD_DIMENSION * (y + WORLD_DIMENSION * z)
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
    anyhow::ensure!(!values.is_empty(), "benchmark buffer cannot be empty");
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

fn logical_resource_bytes(
    phase: &PhaseResult,
    occupancy_bytes: u64,
    ray_bytes: u64,
    result_bytes: u64,
) -> u64 {
    phase.aabb_input_bytes
        + phase.macro_metadata_bytes
        + phase.blas.acceleration_structure_bytes
        + phase.tlas.acceleration_structure_bytes
        + occupancy_bytes
        + ray_bytes
        + result_bytes
}

fn log_phase_summary(density: u32, macro_dimension: u32, phase_name: &str, phase: &PhaseResult) {
    let software_ms = mean_gpu_ms(&phase.samples, TraversalMode::Software);
    let hardware_ms = mean_gpu_ms(&phase.samples, TraversalMode::Hardware);
    let hardware = phase
        .samples
        .iter()
        .find(|sample| matches!(sample.mode, TraversalMode::Hardware))
        .expect("hardware sample");
    log::info!(
        "[RTX_VOXEL][RESULT] density={} macro={} phase={} macros={} blas_gpu_ms={:.6} tlas_gpu_ms={:.6} software_gpu_ms={:.6} hardware_gpu_ms={:.6} speedup={:.4} candidates={} rejected={} false_positive={} false_negative={} wrong_voxel={} exhausted={} query_committed_disagreement={}",
        density,
        macro_dimension,
        phase_name,
        phase.occupied_macro_count,
        phase.blas.gpu_build_ms,
        phase.tlas.gpu_build_ms,
        software_ms,
        hardware_ms,
        software_ms / hardware_ms,
        hardware.candidate_count,
        hardware.rejected_candidate_count,
        hardware.correctness.false_positive_count,
        hardware.correctness.false_negative_count,
        hardware.correctness.wrong_voxel_count,
        hardware.traversal_exhausted_count,
        hardware.query_committed_disagreement_count,
    );
}

fn mean_gpu_ms(samples: &[TraversalSample], mode: TraversalMode) -> f64 {
    let selected = samples
        .iter()
        .filter(|sample| sample.mode == mode)
        .collect::<Vec<_>>();
    selected.iter().map(|sample| sample.gpu_ms).sum::<f64>() / selected.len() as f64
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
    fn fixed_cpu_reference_covers_hits_and_misses() {
        let mut occupancy = make_occupancy(5);
        add_correctness_fixture(&mut occupancy);
        let results = cpu_reference(&occupancy, &benchmark_rays());

        assert!(results.iter().any(|hit| hit.voxel_index == INVALID_VOXEL));
        assert!(results.iter().any(|hit| hit.voxel_index != INVALID_VOXEL));
    }

    #[test]
    fn clearing_one_macro_removes_its_primitive() {
        let mut occupancy = vec![0; WORLD_DIMENSION.pow(3) as usize];
        set_voxel(&mut occupancy, 8, 8, 8, 1);
        set_voxel(&mut occupancy, 20, 20, 20, 1);
        assert_eq!(occupied_macros(&occupancy, 4).0.len(), 2);

        assert_eq!(clear_macro(&mut occupancy, [8, 8, 8], 4), 1);
        assert_eq!(occupied_macros(&occupancy, 4).0.len(), 1);
    }

    #[test]
    fn aabb_stride_matches_vulkan_aabb_layout() {
        assert_eq!(std::mem::size_of::<Aabb>(), 24);
        assert_eq!(std::mem::size_of::<Aabb>() % 8, 0);
        assert_eq!(
            std::mem::size_of::<Aabb>(),
            std::mem::size_of::<vk::AabbPositionsKHR>()
        );
    }
}
