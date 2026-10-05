//! Small private adapter around production GPU voxelization, surface extraction
//! and Contree building. No second voxelizer or writes to the garden's atlas.
use crate::{
    builder::{ContreeBuilder, VOXEL_TYPE_LIMESTONE, VOXEL_TYPE_ROCK},
    generated::gpu_structs::{MakeSurfaceInfo, ModelVoxelizeInfo},
    resource::Resource,
    stone_models::{voxel as plan, StoneKind, StoneMesh},
    voxel_material::VoxelMaterialMode,
};
use anyhow::{ensure, Result};
use glam::UVec3;
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, ClearValue, ColorClearValue, CommandBuffer,
    ComputePipeline, DescriptorPool, Extent3D, ImageDesc, MemoryLocation, ShaderModule, Texture,
    TextureLayout, TextureRegion, VulkanContext,
};
use resource_container_derive::ResourceContainer;
use std::time::Instant;

#[derive(ResourceContainer)]
struct AtlasResources {
    chunk_atlas: Resource<Texture>,
    model_voxelize_info: Resource<Buffer>,
    model_triangles: Resource<Buffer>,
    solid_workgroup_flags: Resource<Buffer>,
    surface: Resource<Texture>,
    make_surface_info: Resource<Buffer>,
    make_surface_result: Resource<Buffer>,
    surface_active_brick_flags: Resource<Buffer>,
    surface_active_brick_indices: Resource<Buffer>,
}
struct ContreeGuard(ContreeBuilder);
impl Drop for ContreeGuard {
    fn drop(&mut self) {
        // Production allocation also queues a managed CPU-cache GPU readback.
        // Observe that submitted job before dropping its buffers, then close and
        // join the decoder. We never publish this preview as garden CPU terrain.
        if let Err(error) = self.0.discard_active_cpu_chunk_cache_job() {
            log::error!("[STONE_VOXEL] private Contree readback retirement failed: {error:#}");
        }
        self.0.shutdown_cpu_chunk_cache_worker();
    }
}
pub(super) struct Volume {
    pub nodes: Buffer,
    pub leaves: Buffer,
    pub offsets: [u32; 2],
}
pub(super) fn material(kind: StoneKind) -> u32 {
    match kind {
        StoneKind::Slab => VOXEL_TYPE_LIMESTONE,
        StoneKind::Rock => VOXEL_TYPE_ROCK,
    }
}
impl Volume {
    pub fn new(ctx: &VulkanContext, allocator: &Allocator, mesh: &StoneMesh) -> Result<Self> {
        let start = Instant::now();
        let device = ctx.device();
        let voxel_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/model_voxelize.comp",
            "main",
        )
        .map_err(anyhow::Error::msg)?;
        let surface_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/surface/make_surface.comp",
            "main",
        )
        .map_err(anyhow::Error::msg)?;
        let uniform = |sm: &ShaderModule, name: &str| {
            Buffer::from_uniform_layout(
                device.clone(),
                allocator.clone(),
                sm.get_buffer_layout(name).unwrap().clone(),
            )
        };
        let buffer = |size: u64| {
            Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(
                    vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                ),
                MemoryLocation::CpuToGpu,
                size,
            )
        };
        let texture = |format| {
            Texture::new(
                device.clone(),
                allocator.clone(),
                &ImageDesc {
                    extent: Extent3D::new(plan::DIM, plan::DIM, plan::DIM),
                    format,
                    usage: vk::ImageUsageFlags::STORAGE
                        | vk::ImageUsageFlags::TRANSFER_DST
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                    initial_layout: TextureLayout::UNDEFINED,
                    aspect: vk::ImageAspectFlags::COLOR,
                    ..Default::default()
                },
                &Default::default(),
            )
        };
        let tris = plan::triangles(mesh);
        let bricks = (plan::DIM / 4).pow(3);
        let resources = AtlasResources {
            chunk_atlas: Resource::new(texture(vk::Format::R8_UINT)),
            model_voxelize_info: Resource::new(uniform(&voxel_sm, "U_ModelVoxelizeInfo")),
            model_triangles: Resource::new(buffer(std::mem::size_of_val(tris.as_slice()) as u64)),
            solid_workgroup_flags: Resource::new(buffer(
                u64::from((plan::DIM / 8).pow(3).div_ceil(32)) * 4,
            )),
            surface: Resource::new(texture(vk::Format::R32_UINT)),
            make_surface_info: Resource::new(uniform(&surface_sm, "U_MakeSurfaceInfo")),
            make_surface_result: Resource::new(buffer(16)),
            surface_active_brick_flags: Resource::new(buffer(u64::from(bricks.div_ceil(32)) * 4)),
            surface_active_brick_indices: Resource::new(buffer(u64::from(bricks) * 4)),
        };
        resources.model_triangles.fill(&tris)?;
        resources
            .model_voxelize_info
            .fill_uniform(&ModelVoxelizeInfo {
                offset: [0; 3],
                triangle_count: tris.len() as u32,
                dim: [plan::DIM; 3],
                fill_voxel_type: material(mesh.spec.kind),
                position_vox: plan::ATLAS_PIVOT_VOX.to_array(),
                surface_thickness_vox: plan::SURFACE_THICKNESS,
            })?;
        resources.make_surface_info.fill_uniform(&MakeSurfaceInfo {
            atlas_read_offset: [0; 3],
            _pad0: [0; 4],
            atlas_read_dim: [plan::DIM; 3],
            is_crossing_boundary: 0,
        })?;
        let pool = DescriptorPool::new(device)?;
        let voxel_ppl = ComputePipeline::new(device, &voxel_sm, &pool, &[&resources]);
        let surface_ppl = ComputePipeline::new(device, &surface_sm, &pool, &[&resources]);
        let cmd = CommandBuffer::new(device, ctx.command_pool());
        cmd.begin(false);
        for tex in [&resources.chunk_atlas, &resources.surface] {
            tex.get_image().record_clear(
                &cmd,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::UInt([0; 4])),
            );
        }
        for buf in [
            &resources.solid_workgroup_flags,
            &resources.make_surface_result,
            &resources.surface_active_brick_flags,
        ] {
            buf.record_fill(&cmd, 0, buf.get_size_bytes(), 0);
        }
        cmd.use_buffer(&resources.model_voxelize_info, BufferUse::HostWrite);
        cmd.use_buffer(&resources.model_triangles, BufferUse::HostWrite);
        cmd.use_buffer(&resources.make_surface_info, BufferUse::HostWrite);
        let extent = Extent3D::new(plan::DIM, plan::DIM, plan::DIM);
        voxel_ppl.record(&cmd, extent, None);
        surface_ppl.record(&cmd, extent, None);
        cmd.end();
        let _complete = cmd
            .submit_gpu_job(
                &ctx.get_general_queue(),
                "stone.private_model_voxel_surface",
            )?
            .wait_complete()?;
        let mut readback = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            u64::from(plan::DIM.pow(3)),
        );
        resources.chunk_atlas.get_image().copy_image_to_buffer(
            &mut readback,
            &ctx.get_general_queue(),
            ctx.command_pool(),
            TextureLayout::GENERAL,
            0,
            TextureRegion {
                offset: [0; 3],
                extent,
            },
        );
        let (occupied, checked) =
            plan::validate_atlas(mesh, &readback.read_back()?, material(mesh.spec.kind))?;
        let sizes = ContreeBuilder::pool_sizes_for_chunk_dim(UVec3::ONE, UVec3::splat(plan::DIM));
        let mut contree = ContreeGuard(ContreeBuilder::new(
            ctx.clone(),
            allocator.clone(),
            &resources,
            UVec3::ONE,
            UVec3::splat(plan::DIM),
            sizes.node_pool_size_in_bytes,
            sizes.leaf_pool_size_in_bytes,
            VoxelMaterialMode::Standard,
        ));
        let build_job = contree.0.submit_build_and_alloc(UVec3::ZERO)?;
        let build = contree.0.finish_build_and_alloc(build_job)?;
        let offsets = build
            .scene_offsets
            .ok_or_else(|| anyhow::anyhow!("stone Contree is empty"))?;
        let (node_bytes, leaf_bytes) = (build.node_bytes, build.leaf_bytes);
        ensure!(
            node_bytes > 0 && leaf_bytes > 0,
            "stone surface publication has no Contree records"
        );
        log::info!("[STONE_VOXEL] source={:016x} kernel=model_voxelize surface=make_surface visibility=Contree dim={} density={} occupied={} cpu_reference_samples={} nodes={} leaves={} build_ms={:.3} garden_atlas_writes=0",mesh.fingerprint(),plan::DIM,plan::VOXELS_PER_UNIT,occupied,checked,node_bytes/12,leaf_bytes/4,start.elapsed().as_secs_f64()*1000.);
        Ok(Self {
            nodes: (*contree.0.get_resources().contree_node_data).clone(),
            leaves: (*contree.0.get_resources().contree_leaf_data).clone(),
            offsets: [offsets.0 as u32, offsets.1 as u32],
        })
    }
}
