mod resources;
use super::PlainBuilderResources;
use crate::{
    flora::species,
    generated::gpu_structs::{
        ClearOccupancyInfo, EditOccupancyInfo, InstancesToOccupancyInfo, MakeSurfaceInfo,
        OccupancyToInstancesInfo,
    },
    geom::UAabb3,
    util::ShaderCompiler,
    vkn::{
        Buffer, ClearValue, ColorClearValue, CommandBuffer, ComputePipeline, DescriptorPool,
        Extent3D, MemoryBarrier, PipelineBarrier, ShaderModule, VulkanContext, WriteDescriptorSet,
        Fence,
    },
};
use anyhow::Result;
use ash::vk;
use bytemuck::Zeroable;
use glam::{UVec3, Vec3};
pub use resources::*;

#[derive(Copy, Clone, Eq, PartialEq)]
enum OccupancyEditMode {
    Remove = 0,
    Add = 1,
    Trim = 2,
}

#[allow(dead_code)]
pub struct FloraRegenStats {
    pub appended_total: u32,
    pub before_total: u32,
    pub after_total: u32,
    pub dispatch_dim: UVec3,
}

pub struct SurfaceBuilder {
    vulkan_ctx: VulkanContext,
    pub resources: SurfaceResources,

    #[allow(dead_code)]
    pool: DescriptorPool,

    make_surface_ppl: ComputePipeline,
    clear_occupancy_ppl: ComputePipeline,
    instances_to_occupancy_ppl: ComputePipeline,
    edit_occupancy_ppl: ComputePipeline,
    occupancy_to_instances_ppl: ComputePipeline,

    chunk_bound: UAabb3,
    voxel_dim_per_chunk: UVec3,
    flora_species_count: usize,
}

impl SurfaceBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: crate::vkn::Allocator,
        shader_compiler: &ShaderCompiler,
        plain_builder_resources: &PlainBuilderResources,
        voxel_dim_per_chunk: UVec3,
        chunk_bound: UAabb3,
    ) -> Self {
        let device = vulkan_ctx.device();
        species::assert_species_limit();
        let flora_species_count = species::species_count();

        let make_surface_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/make_surface.comp",
            "main",
        )
        .unwrap();

        let clear_occupancy_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/clear_occupancy.comp",
            "main",
        )
        .unwrap();

        let instances_to_occupancy_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/instances_to_occupancy.comp",
            "main",
        )
        .unwrap();

        let edit_occupancy_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/edit_occupancy_sphere.comp",
            "main",
        )
        .unwrap();

        let occupancy_to_instances_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/occupancy_to_flora_instances.comp",
            "main",
        )
        .unwrap();

        let resources = SurfaceResources::new(
            device.clone(),
            allocator,
            voxel_dim_per_chunk,
            &make_surface_sm,
            &clear_occupancy_sm,
            &instances_to_occupancy_sm,
            &edit_occupancy_sm,
            &occupancy_to_instances_sm,
            chunk_bound,
        );

        let pool = DescriptorPool::new(device).unwrap();

        let make_surface_ppl = ComputePipeline::new(
            device,
            &make_surface_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );
        let clear_occupancy_ppl = ComputePipeline::new(
            device,
            &clear_occupancy_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );
        let instances_to_occupancy_ppl = ComputePipeline::new(
            device,
            &instances_to_occupancy_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );
        let edit_occupancy_ppl = ComputePipeline::new(
            device,
            &edit_occupancy_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );
        let occupancy_to_instances_ppl = ComputePipeline::new(
            device,
            &occupancy_to_instances_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );

        Self {
            vulkan_ctx,
            resources,
            pool,
            make_surface_ppl,
            clear_occupancy_ppl,
            instances_to_occupancy_ppl,
            edit_occupancy_ppl,
            occupancy_to_instances_ppl,
            chunk_bound,
            voxel_dim_per_chunk,
            flora_species_count,
        }
    }

    pub fn build_surface(&mut self, chunk_id: UVec3, place_flora: bool) -> Result<u32> {
        if !self.chunk_bound.in_bound(chunk_id) {
            return Err(anyhow::anyhow!("Chunk ID out of bounds"));
        }

        let atlas_read_offset = chunk_id * self.voxel_dim_per_chunk;
        let atlas_read_dim = self.voxel_dim_per_chunk;
        let device = self.vulkan_ctx.device();

        update_make_surface_info(
            &self.resources.make_surface_info,
            atlas_read_offset,
            atlas_read_dim,
            true,
        )?;
        cleanup_make_surface_result(&self.resources.make_surface_result)?;

        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        self.resources.surface.get_image().record_clear(
            &cmdbuf,
            Some(vk::ImageLayout::GENERAL),
            0,
            ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
        );
        self.resources.occupancy_data.get_image().record_clear(
            &cmdbuf,
            Some(vk::ImageLayout::GENERAL),
            0,
            ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
        );

        let extent = Extent3D {
            width: self.voxel_dim_per_chunk.x,
            height: self.voxel_dim_per_chunk.y,
            depth: self.voxel_dim_per_chunk.z,
        };
        self.make_surface_ppl.record(&cmdbuf, extent, None);

        cmdbuf.end();
        let fence = Fence::new(device, false);
        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), Some(&fence));
        self.vulkan_ctx.wait_for_fences(&[fence.as_raw()]).unwrap();

        let active_voxel_len = get_make_surface_result(&self.resources.make_surface_result);
        if place_flora {
            self.seed_and_rebuild_flora_from_surface(chunk_id, 0)?;
        }

        Ok(active_voxel_len)
    }

    pub fn edit_flora_instances(
        &mut self,
        chunk_id: UVec3,
        edit_center: Vec3,
        edit_radius: f32,
        flora_tick: u32,
    ) -> Result<()> {
        let _ = self.run_occupancy_edit(
            chunk_id,
            edit_center,
            edit_radius,
            flora_tick,
            0,
            OccupancyEditMode::Remove,
        )?;
        Ok(())
    }

    pub fn regenerate_flora_instances(
        &mut self,
        chunk_id: UVec3,
        edit_center: Vec3,
        edit_radius: f32,
        flora_tick: u32,
    ) -> Result<FloraRegenStats> {
        self.run_occupancy_edit(
            chunk_id,
            edit_center,
            edit_radius,
            flora_tick,
            0,
            OccupancyEditMode::Add,
        )
    }

    pub fn trim_flora_instances(
        &mut self,
        chunk_id: UVec3,
        edit_center: Vec3,
        edit_radius: f32,
        flora_tick: u32,
        target_age: u32,
    ) -> Result<FloraRegenStats> {
        self.run_occupancy_edit(
            chunk_id,
            edit_center,
            edit_radius,
            flora_tick,
            target_age,
            OccupancyEditMode::Trim,
        )
    }

    fn seed_and_rebuild_flora_from_surface(
        &mut self,
        chunk_id: UVec3,
        flora_tick: u32,
    ) -> Result<()> {
        let chunk_world_offset = chunk_id * self.voxel_dim_per_chunk;
        let center = chunk_world_offset.as_vec3() + self.voxel_dim_per_chunk.as_vec3() * 0.5;
        let radius = self.voxel_dim_per_chunk.as_vec3().length();

        update_clear_occupancy_info(
            &self.resources.clear_occupancy_info,
            self.voxel_dim_per_chunk,
        )?;
        update_edit_occupancy_info(
            &self.resources.edit_occupancy_info,
            center,
            radius,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
            OccupancyEditMode::Add,
            flora_tick,
            0,
        )?;
        update_occupancy_to_instances_info(
            &self.resources.occupancy_to_instances_info,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
        )?;
        cleanup_occupancy_to_instances_result(&self.resources.occupancy_to_instances_result)?;

        let chunk_idx = self.get_chunk_resource_index(chunk_id)?;
        let chunk_resources = &self.resources.instances.chunk_flora_instances[chunk_idx]
            .1
            .resources;
        self.bind_manual_instance_buffers(&self.occupancy_to_instances_ppl, chunk_resources);

        let device = self.vulkan_ctx.device();
        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        self.resources
            .occupancy_data
            .get_image()
            .record_transition_barrier(&cmdbuf, 0, vk::ImageLayout::GENERAL);

        self.clear_occupancy_ppl.record(
            &cmdbuf,
            Extent3D::new(
                self.voxel_dim_per_chunk.x,
                self.voxel_dim_per_chunk.y,
                self.voxel_dim_per_chunk.z,
            ),
            None,
        );

        record_compute_barrier(device, &cmdbuf);

        self.edit_occupancy_ppl.record(
            &cmdbuf,
            Extent3D::new(
                self.voxel_dim_per_chunk.x,
                self.voxel_dim_per_chunk.y,
                self.voxel_dim_per_chunk.z,
            ),
            None,
        );

        record_compute_barrier(device, &cmdbuf);

        self.occupancy_to_instances_ppl.record(
            &cmdbuf,
            Extent3D::new(
                self.voxel_dim_per_chunk.x,
                self.voxel_dim_per_chunk.y,
                self.voxel_dim_per_chunk.z,
            ),
            None,
        );

        cmdbuf.end();
        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        let lengths = get_occupancy_to_instances_result(
            &self.resources.occupancy_to_instances_result,
            self.flora_species_count,
        );
        let chunk_resources_mut = &mut self.resources.instances.chunk_flora_instances[chunk_idx].1;
        for (species_idx, len) in lengths.iter().enumerate() {
            chunk_resources_mut.get_mut(species_idx).instances_len = *len;
        }

        Ok(())
    }

    fn run_occupancy_edit(
        &mut self,
        chunk_id: UVec3,
        edit_center: Vec3,
        edit_radius: f32,
        flora_tick: u32,
        target_age: u32,
        mode: OccupancyEditMode,
    ) -> Result<FloraRegenStats> {
        if !self.chunk_bound.in_bound(chunk_id) {
            return Err(anyhow::anyhow!("Chunk ID out of bounds"));
        }

        let chunk_idx = self.get_chunk_resource_index(chunk_id)?;
        let chunk_world_offset = chunk_id * self.voxel_dim_per_chunk;
        let edit_center_vox = edit_center * 256.0;
        let edit_radius_vox = edit_radius * 256.0;

        let before_total = self.resources.instances.chunk_flora_instances[chunk_idx]
            .1
            .iter()
            .fold(0_u32, |acc, r| acc.saturating_add(r.instances_len));

        let mut species_len = [0_u32; 4];
        let mut max_len = 0_u32;
        for (species_idx, species) in species_len
            .iter_mut()
            .enumerate()
            .take(self.flora_species_count.min(4))
        {
            let len = self.resources.instances.chunk_flora_instances[chunk_idx]
                .1
                .get(species_idx)
                .instances_len;
            *species = len;
            max_len = max_len.max(len);
        }

        update_clear_occupancy_info(
            &self.resources.clear_occupancy_info,
            self.voxel_dim_per_chunk,
        )?;
        update_instances_to_occupancy_info(
            &self.resources.instances_to_occupancy_info,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
            species_len,
        )?;
        update_edit_occupancy_info(
            &self.resources.edit_occupancy_info,
            edit_center_vox,
            edit_radius_vox,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
            mode,
            flora_tick,
            target_age,
        )?;
        update_occupancy_to_instances_info(
            &self.resources.occupancy_to_instances_info,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
        )?;
        cleanup_occupancy_to_instances_result(&self.resources.occupancy_to_instances_result)?;

        let chunk_resources = &self.resources.instances.chunk_flora_instances[chunk_idx]
            .1
            .resources;
        self.bind_manual_instance_buffers(&self.instances_to_occupancy_ppl, chunk_resources);
        self.bind_manual_instance_buffers(&self.occupancy_to_instances_ppl, chunk_resources);

        let device = self.vulkan_ctx.device();
        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        self.resources
            .occupancy_data
            .get_image()
            .record_transition_barrier(&cmdbuf, 0, vk::ImageLayout::GENERAL);

        self.clear_occupancy_ppl.record(
            &cmdbuf,
            Extent3D::new(
                self.voxel_dim_per_chunk.x,
                self.voxel_dim_per_chunk.y,
                self.voxel_dim_per_chunk.z,
            ),
            None,
        );

        if max_len > 0 {
            record_compute_barrier(device, &cmdbuf);
            self.instances_to_occupancy_ppl
                .record(&cmdbuf, Extent3D::new(max_len, 1, 1), None);
        }

        record_compute_barrier(device, &cmdbuf);

        self.edit_occupancy_ppl.record(
            &cmdbuf,
            Extent3D::new(
                self.voxel_dim_per_chunk.x,
                self.voxel_dim_per_chunk.y,
                self.voxel_dim_per_chunk.z,
            ),
            None,
        );

        record_compute_barrier(device, &cmdbuf);

        self.occupancy_to_instances_ppl.record(
            &cmdbuf,
            Extent3D::new(
                self.voxel_dim_per_chunk.x,
                self.voxel_dim_per_chunk.y,
                self.voxel_dim_per_chunk.z,
            ),
            None,
        );

        cmdbuf.end();
        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        let lengths = get_occupancy_to_instances_result(
            &self.resources.occupancy_to_instances_result,
            self.flora_species_count,
        );
        let chunk_resources_mut = &mut self.resources.instances.chunk_flora_instances[chunk_idx].1;
        let mut after_total = 0_u32;
        for (species_idx, len) in lengths.iter().enumerate() {
            chunk_resources_mut.get_mut(species_idx).instances_len = *len;
            after_total = after_total.saturating_add(*len);
        }

        let appended_total = if mode == OccupancyEditMode::Add {
            after_total.saturating_sub(before_total)
        } else {
            0
        };

        Ok(FloraRegenStats {
            appended_total,
            before_total,
            after_total,
            dispatch_dim: self.voxel_dim_per_chunk,
        })
    }

    fn get_chunk_resource_index(&self, chunk_id: UVec3) -> Result<usize> {
        self.resources
            .instances
            .chunk_flora_instances
            .iter()
            .position(|(_, resources)| resources.chunk_id == chunk_id)
            .ok_or_else(|| anyhow::anyhow!("Chunk {:?} has no flora instance resources", chunk_id))
    }

    fn bind_manual_instance_buffers(
        &self,
        pipeline: &ComputePipeline,
        resources: &[InstanceResource],
    ) {
        for (species_index, instance_resource) in resources.iter().enumerate() {
            pipeline.write_descriptor_set(
                1,
                WriteDescriptorSet::new_buffer_write(0, &instance_resource.instances_buf)
                    .with_array_element(species_index as u32),
            );
        }
    }

    pub fn get_resources(&self) -> &SurfaceResources {
        &self.resources
    }
}

fn record_compute_barrier(device: &crate::vkn::Device, cmdbuf: &CommandBuffer) {
    let barrier = PipelineBarrier::new(
        vk::PipelineStageFlags::COMPUTE_SHADER,
        vk::PipelineStageFlags::COMPUTE_SHADER,
        vec![MemoryBarrier::new_shader_access()],
    );
    barrier.record_insert(device, cmdbuf);
}

fn update_make_surface_info(
    make_surface_info: &Buffer,
    atlas_read_offset: UVec3,
    atlas_read_dim: UVec3,
    is_crossing_boundary: bool,
) -> Result<()> {
    make_surface_info.fill_uniform(&MakeSurfaceInfo {
        atlas_read_offset: atlas_read_offset.to_array(),
        atlas_read_dim: atlas_read_dim.to_array(),
        is_crossing_boundary: if is_crossing_boundary { 1 } else { 0 },
        ..MakeSurfaceInfo::zeroed()
    })
}

fn cleanup_make_surface_result(make_surface_result: &Buffer) -> Result<()> {
    let layout = make_surface_result.get_layout().unwrap();
    let buffer_size = layout.root_member.get_size_bytes() as usize;
    let zeroed = vec![0u8; buffer_size];
    make_surface_result.fill_with_raw_u8(&zeroed)?;
    Ok(())
}

fn get_make_surface_result(make_surface_result: &Buffer) -> u32 {
    let raw_data = make_surface_result.read_back().unwrap();
    let total_u32 = raw_data.len() / std::mem::size_of::<u32>();
    let data = unsafe { std::slice::from_raw_parts(raw_data.as_ptr() as *const u32, total_u32) };
    assert!(
        total_u32 >= 1,
        "make_surface_result buffer too small: expected at least 1 u32, got {}",
        total_u32
    );
    data[0]
}

fn update_clear_occupancy_info(clear_occupancy_info: &Buffer, chunk_dim: UVec3) -> Result<()> {
    clear_occupancy_info.fill_uniform(&ClearOccupancyInfo {
        chunk_dim: chunk_dim.to_array(),
        ..ClearOccupancyInfo::zeroed()
    })
}

fn update_instances_to_occupancy_info(
    instances_to_occupancy_info: &Buffer,
    chunk_world_offset: UVec3,
    chunk_dim: UVec3,
    species_instance_len: [u32; 4],
) -> Result<()> {
    instances_to_occupancy_info.fill_uniform(&InstancesToOccupancyInfo {
        chunk_world_offset: chunk_world_offset.to_array(),
        chunk_dim: chunk_dim.to_array(),
        species_instance_len,
        ..InstancesToOccupancyInfo::zeroed()
    })
}

#[allow(clippy::too_many_arguments)]
fn update_edit_occupancy_info(
    edit_occupancy_info: &Buffer,
    edit_center_vox: Vec3,
    edit_radius_vox: f32,
    chunk_world_offset: UVec3,
    chunk_dim: UVec3,
    mode: OccupancyEditMode,
    flora_tick: u32,
    target_age: u32,
) -> Result<()> {
    edit_occupancy_info.fill_uniform(&EditOccupancyInfo {
        edit_center_radius_vox: [
            edit_center_vox.x,
            edit_center_vox.y,
            edit_center_vox.z,
            edit_radius_vox,
        ],
        chunk_world_offset: chunk_world_offset.to_array(),
        chunk_dim: chunk_dim.to_array(),
        mode: mode as u32,
        flora_tick,
        target_age,
        ..EditOccupancyInfo::zeroed()
    })
}

fn update_occupancy_to_instances_info(
    occupancy_to_instances_info: &Buffer,
    chunk_world_offset: UVec3,
    chunk_dim: UVec3,
) -> Result<()> {
    occupancy_to_instances_info.fill_uniform(&OccupancyToInstancesInfo {
        chunk_world_offset: chunk_world_offset.to_array(),
        chunk_dim: chunk_dim.to_array(),
        ..OccupancyToInstancesInfo::zeroed()
    })
}

fn cleanup_occupancy_to_instances_result(result: &Buffer) -> Result<()> {
    let layout = result.get_layout().unwrap();
    let buffer_size = layout.root_member.get_size_bytes() as usize;
    let zeroed = vec![0u8; buffer_size];
    result.fill_with_raw_u8(&zeroed)?;
    Ok(())
}

fn get_occupancy_to_instances_result(result: &Buffer, species_count: usize) -> Vec<u32> {
    let raw_data = result.read_back().unwrap();
    let total_u32 = raw_data.len() / std::mem::size_of::<u32>();
    let data = unsafe { std::slice::from_raw_parts(raw_data.as_ptr() as *const u32, total_u32) };
    assert!(
        total_u32 >= species_count,
        "occupancy_to_instances_result buffer too small: expected at least {} u32s, got {}",
        species_count,
        total_u32
    );
    let mut lengths = Vec::with_capacity(species_count);
    lengths.extend_from_slice(&data[0..species_count]);
    lengths
}
