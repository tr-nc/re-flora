mod resources;
use ash::vk;
use glam::UVec3;
pub use resources::*;

use super::PlainBuilderResources;
use crate::{
    geom::UAabb3,
    util::ShaderCompiler,
    vkn::{
        Allocator, Buffer, ClearValue, CommandBuffer, ComputePipeline, DescriptorPool,
        DescriptorSet, PlainMemberTypeWithData, ShaderModule, StructMemberDataBuilder,
        StructMemberDataReader, VulkanContext, WriteDescriptorSet,
    },
};

pub struct SurfaceBuilder {
    vulkan_ctx: VulkanContext,
    resources: SurfaceResources,

    make_surface_ppl: ComputePipeline,
    make_surface_ds_0: DescriptorSet,
    flexible_descriptor_pool: DescriptorPool,

    chunk_bound: UAabb3,
    voxel_dim_per_chunk: UVec3,
}

impl SurfaceBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        plain_builder_resources: &PlainBuilderResources,
        voxel_dim_per_chunk: UVec3,
        chunk_bound: UAabb3,
        grass_instances_capacity_per_chunk: u64,
    ) -> Self {
        let fixed_descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();
        let flexible_descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let make_surface_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/surface/make_surface.comp",
            "main",
        )
        .unwrap();

        let resources = SurfaceResources::new(
            vulkan_ctx.device().clone(),
            allocator,
            voxel_dim_per_chunk,
            &make_surface_sm,
            chunk_bound,
            grass_instances_capacity_per_chunk,
        );

        let make_surface_ppl = ComputePipeline::new(vulkan_ctx.device(), &make_surface_sm);

        let make_surface_ds_0 = create_make_surface_ds_0(
            &vulkan_ctx,
            &make_surface_ppl,
            &resources,
            plain_builder_resources,
            &fixed_descriptor_pool,
        );

        return Self {
            vulkan_ctx,
            resources,

            make_surface_ppl,
            make_surface_ds_0,

            flexible_descriptor_pool,

            chunk_bound,
            voxel_dim_per_chunk,
        };

        fn create_make_surface_ds_0(
            vulkan_ctx: &VulkanContext,
            make_surface_ppl: &ComputePipeline,
            resources: &SurfaceResources,
            plain_builder_resources: &PlainBuilderResources,
            fixed_pool: &DescriptorPool,
        ) -> DescriptorSet {
            let make_surface_ds_0 = DescriptorSet::new(
                vulkan_ctx.device().clone(),
                &make_surface_ppl
                    .get_layout()
                    .get_descriptor_set_layouts()
                    .get(&0)
                    .unwrap(),
                fixed_pool.clone(),
            );
            make_surface_ds_0.perform_writes(&mut [
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
            return make_surface_ds_0;
        }
    }

    fn create_make_surface_ds_1(
        vulkan_ctx: &VulkanContext,
        make_surface_ppl: &ComputePipeline,
        resources: &SurfaceResources,
        chunk_id: UVec3,
        flexible_pool: &DescriptorPool,
    ) -> DescriptorSet {
        flexible_pool.reset().unwrap();
        let make_surface_ds_1 = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &make_surface_ppl
                .get_layout()
                .get_descriptor_set_layouts()
                .get(&1)
                .unwrap(),
            flexible_pool.clone(),
        );
        make_surface_ds_1.perform_writes(&mut [WriteDescriptorSet::new_buffer_write(
            0,
            &resources
                .chunk_raster_resources
                .get(&chunk_id)
                .unwrap()
                .grass_instances,
        )]);
        return make_surface_ds_1;
    }

    /// Returns active_voxel_len
    pub fn build_surface(&mut self, chunk_id: UVec3) -> u32 {
        if !self.chunk_bound.in_bound(chunk_id) {
            return 0;
        }

        let atlas_read_offset = chunk_id * self.voxel_dim_per_chunk;
        let atlas_read_dim = self.voxel_dim_per_chunk;

        let device = self.vulkan_ctx.device();

        update_make_surface_info(
            &self.resources.make_surface_info,
            atlas_read_offset,
            atlas_read_dim,
            true, // is_crossing_boundary,
        );

        update_make_surface_result(&self.resources.make_surface_result, 0, 0);

        let make_surface_ds_1 = Self::create_make_surface_ds_1(
            &self.vulkan_ctx,
            &self.make_surface_ppl,
            &self.resources,
            chunk_id,
            &self.flexible_descriptor_pool,
        );

        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        self.resources.surface.get_image().record_clear(
            &cmdbuf,
            Some(vk::ImageLayout::GENERAL),
            0,
            ClearValue::UInt([0, 0, 0, 0]),
        );

        self.make_surface_ppl.record_bind(&cmdbuf);
        self.make_surface_ppl.record_bind_descriptor_sets(
            &cmdbuf,
            &[self.make_surface_ds_0.clone(), make_surface_ds_1],
            0,
        );
        self.make_surface_ppl.record_dispatch(
            &cmdbuf,
            [
                self.voxel_dim_per_chunk.x,
                self.voxel_dim_per_chunk.y,
                self.voxel_dim_per_chunk.z,
            ],
        );

        cmdbuf.end();

        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);

        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        let (active_voxel_len, grass_instance_len) =
            get_result(&self.resources.make_surface_result);

        self.resources
            .chunk_raster_resources
            .get_mut(&chunk_id)
            .unwrap()
            .grass_instances_len = grass_instance_len;

        return active_voxel_len;

        fn update_make_surface_info(
            make_surface_info: &Buffer,
            atlas_read_offset: UVec3,
            atlas_read_dim: UVec3,
            is_crossing_boundary: bool,
        ) {
            let data = StructMemberDataBuilder::from_buffer(&make_surface_info)
                .set_field(
                    "atlas_read_offset",
                    PlainMemberTypeWithData::UVec3(atlas_read_offset.to_array()),
                )
                .unwrap()
                .set_field(
                    "atlas_read_dim",
                    PlainMemberTypeWithData::UVec3(atlas_read_dim.to_array()),
                )
                .unwrap()
                .set_field(
                    "is_crossing_boundary",
                    PlainMemberTypeWithData::UInt(if is_crossing_boundary { 1 } else { 0 }),
                )
                .unwrap()
                .build();
            make_surface_info.fill_with_raw_u8(&data).unwrap();
        }

        fn update_make_surface_result(
            make_surface_result: &Buffer,
            active_voxel_len: u32,
            grass_instance_len: u32,
        ) {
            let data = StructMemberDataBuilder::from_buffer(&make_surface_result)
                .set_field(
                    "active_voxel_len",
                    PlainMemberTypeWithData::UInt(active_voxel_len),
                )
                .unwrap()
                .set_field(
                    "grass_instance_len",
                    PlainMemberTypeWithData::UInt(grass_instance_len),
                )
                .unwrap()
                .build();
            make_surface_result.fill_with_raw_u8(&data).unwrap();
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
