mod resources;
use anyhow::Result;
use ash::vk;
use glam::UVec3;
pub use resources::*;

use super::PlainBuilderResources;
use crate::{
    geom::UAabb3,
    util::ShaderCompiler,
    vkn::{
        Allocator, Buffer, ClearValue, CommandBuffer, ComputePipeline, DescriptorPool,
        DescriptorSet, Extent3D, PlainMemberTypeWithData, ShaderModule, StructMemberDataBuilder,
        StructMemberDataReader, VulkanContext, WriteDescriptorSet,
    },
};

pub struct SurfaceBuilder {
    vulkan_ctx: VulkanContext,
    resources: SurfaceResources,

    #[allow(dead_code)]
    fixed_pool: DescriptorPool,
    #[allow(dead_code)]
    flexible_pool: DescriptorPool,
    flexible_sets: Vec<DescriptorSet>,

    make_surface_ppl: ComputePipeline,

    chunk_bound: UAabb3,
    voxel_dim_per_chunk: UVec3,
}

impl SurfaceBuilder {
    fn update_make_surface_ds_0(
        ds: &DescriptorSet,
        resources: &SurfaceResources,
        plain_builder_resources: &PlainBuilderResources,
    ) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.make_surface_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.make_surface_result),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.surface,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &plain_builder_resources.chunk_atlas,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        plain_builder_resources: &PlainBuilderResources,
        voxel_dim_per_chunk: UVec3,
        chunk_bound: UAabb3,
        grass_instances_capacity_per_chunk: u64,
    ) -> Self {
        let device = vulkan_ctx.device();
        let fixed_pool = DescriptorPool::new(device).unwrap();
        let flexible_pool = DescriptorPool::new(device).unwrap();

        let make_surface_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/make_surface.comp",
            "main",
        )
        .unwrap();

        let resources = SurfaceResources::new(
            device.clone(),
            allocator,
            voxel_dim_per_chunk,
            &make_surface_sm,
            chunk_bound,
            grass_instances_capacity_per_chunk,
        );

        let make_surface_ppl = ComputePipeline::new(device, &make_surface_sm);

        let make_surface_ds_0 = fixed_pool
            .allocate_set(&make_surface_ppl.get_layout().get_descriptor_set_layouts()[&0])
            .unwrap();

        Self::update_make_surface_ds_0(&make_surface_ds_0, &resources, plain_builder_resources);

        let grass_instance_set = flexible_pool
            .allocate_set(&make_surface_ppl.get_layout().get_descriptor_set_layouts()[&1])
            .unwrap();

        make_surface_ppl.set_descriptor_sets(vec![make_surface_ds_0, grass_instance_set.clone()]);

        Self {
            vulkan_ctx,
            resources,
            fixed_pool,
            flexible_pool,
            flexible_sets: vec![grass_instance_set],
            make_surface_ppl,
            chunk_bound,
            voxel_dim_per_chunk,
        }
    }

    fn update_grass_instance_set(&self, grass_instance_set: &DescriptorSet, chunk_id: UVec3) {
        grass_instance_set.perform_writes(&mut [WriteDescriptorSet::new_buffer_write(
            0,
            &self
                .resources
                .instances
                .chunk_grass_instances
                .get(&chunk_id)
                .unwrap()
                .grass_instances,
        )]);
    }

    /// Returns active_voxel_len
    pub fn build_surface(&mut self, chunk_id: UVec3) -> Result<u32> {
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

        update_make_surface_result(&self.resources.make_surface_result, 0, 0)?;

        self.update_grass_instance_set(&self.flexible_sets[0], chunk_id);

        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        self.resources.surface.get_image().record_clear(
            &cmdbuf,
            Some(vk::ImageLayout::GENERAL),
            0,
            ClearValue::UInt([0, 0, 0, 0]),
        );

        let extent = Extent3D {
            width: self.voxel_dim_per_chunk.x,
            height: self.voxel_dim_per_chunk.y,
            depth: self.voxel_dim_per_chunk.z,
        };

        self.make_surface_ppl.record(&cmdbuf, extent, None);

        cmdbuf.end();

        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);

        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        let (active_voxel_len, grass_instance_len) =
            get_result(&self.resources.make_surface_result);

        self.resources
            .instances
            .chunk_grass_instances
            .get_mut(&chunk_id)
            .unwrap()
            .grass_instances_len = grass_instance_len;

        return Ok(active_voxel_len);

        fn update_make_surface_info(
            make_surface_info: &Buffer,
            atlas_read_offset: UVec3,
            atlas_read_dim: UVec3,
            is_crossing_boundary: bool,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&make_surface_info)
                .set_field(
                    "atlas_read_offset",
                    PlainMemberTypeWithData::UVec3(atlas_read_offset.to_array()),
                )
                .set_field(
                    "atlas_read_dim",
                    PlainMemberTypeWithData::UVec3(atlas_read_dim.to_array()),
                )
                .set_field(
                    "is_crossing_boundary",
                    PlainMemberTypeWithData::UInt(if is_crossing_boundary { 1 } else { 0 }),
                )
                .build()?;
            make_surface_info.fill_with_raw_u8(&data)?;
            Ok(())
        }

        fn update_make_surface_result(
            make_surface_result: &Buffer,
            active_voxel_len: u32,
            grass_instance_len: u32,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&make_surface_result)
                .set_field(
                    "active_voxel_len",
                    PlainMemberTypeWithData::UInt(active_voxel_len),
                )
                .set_field(
                    "grass_instance_len",
                    PlainMemberTypeWithData::UInt(grass_instance_len),
                )
                .build()?;
            make_surface_result.fill_with_raw_u8(&data)?;
            Ok(())
        }

        /// Returns: (active_voxel_len, grass_instance_len)
        fn get_result(frag_img_build_result: &Buffer) -> (u32, u32) {
            let layout = &frag_img_build_result.get_layout().unwrap().root_member;
            let raw_data = frag_img_build_result.read_back().unwrap();
            let reader = StructMemberDataReader::new(layout, &raw_data);

            let active_voxel_len = if let PlainMemberTypeWithData::UInt(val) =
                reader.get_field("active_voxel_len").unwrap()
            {
                val
            } else {
                panic!("Expected UInt type for active_voxel_len")
            };
            let grass_instance_len = if let PlainMemberTypeWithData::UInt(val) =
                reader.get_field("grass_instance_len").unwrap()
            {
                val
            } else {
                panic!("Expected UInt type for grass_instance_len")
            };
            (active_voxel_len, grass_instance_len)
        }
    }

    pub fn get_resources(&self) -> &SurfaceResources {
        &self.resources
    }
}
