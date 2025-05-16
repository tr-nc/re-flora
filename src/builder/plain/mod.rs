mod resources;
pub use resources::*;

use crate::util::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::Allocator;
use crate::vkn::Buffer;
use crate::vkn::ClearValue;
use crate::vkn::CommandBuffer;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::MemoryBarrier;
use crate::vkn::PipelineBarrier;
use crate::vkn::PlainMemberTypeWithData;
use crate::vkn::ShaderModule;
use crate::vkn::StructMemberDataBuilder;
use crate::vkn::Texture;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::UVec3;

pub struct PlainBuilder {
    vulkan_ctx: VulkanContext,

    resources: PlainBuilderResources,

    _buffer_setup_ppl: ComputePipeline,
    _chunk_init_ppl: ComputePipeline,
    _chunk_modify_ppl: ComputePipeline,
    _leaf_write_ppl: ComputePipeline,

    _buffer_setup_ds: DescriptorSet,
    _chunk_init_ds: DescriptorSet,
    _chunk_modify_ds: DescriptorSet,
    _leaf_write_ds: DescriptorSet,

    build_cmdbuf: CommandBuffer,
    // free_atlas_allocator: AtlasAllocator,
}

impl PlainBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        shader_compiler: &ShaderCompiler,
        allocator: Allocator,
        plain_atlas_dim: UVec3,
        free_atlas_dim: UVec3,
    ) -> Self {
        // we create a local one
        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let buffer_setup_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/chunk_writer/buffer_setup.comp",
            "main",
        )
        .unwrap();

        let chunk_init_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/chunk_writer/chunk_init.comp",
            "main",
        )
        .unwrap();

        let chunk_modify_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/chunk_writer/chunk_modify.comp",
            "main",
        )
        .unwrap();

        let leaf_write_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/chunk_writer/leaf_write.comp",
            "main",
        )
        .unwrap();

        let resources = PlainBuilderResources::new(
            vulkan_ctx.device(),
            allocator.clone(),
            plain_atlas_dim,
            free_atlas_dim,
            &buffer_setup_sm,
            &chunk_modify_sm,
            &leaf_write_sm,
        );

        let buffer_setup_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &buffer_setup_sm);
        let chunk_init_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &chunk_init_sm);
        let chunk_modify_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &chunk_modify_sm);
        let leaf_write_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &leaf_write_sm);

        let buffer_setup_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &buffer_setup_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        buffer_setup_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.region_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.region_indirect),
        ]);
        let chunk_init_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &chunk_init_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        chunk_init_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.region_info),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.chunk_atlas,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        let chunk_modify_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &chunk_modify_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        chunk_modify_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.chunk_modify_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.trunk_bvh_nodes),
            WriteDescriptorSet::new_buffer_write(2, &resources.round_cones),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.chunk_atlas,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        let leaf_write_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &leaf_write_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        leaf_write_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.leaf_write_info),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.free_atlas,
                vk::ImageLayout::GENERAL,
            ),
        ]);

        init_atlas_images(&vulkan_ctx, &resources);

        let build_cmdbuf = record_build_cmdbuf(
            &vulkan_ctx,
            &resources.chunk_atlas,
            &resources.region_indirect,
            &buffer_setup_ppl,
            &chunk_init_ppl,
            &buffer_setup_ds,
            &chunk_init_ds,
        );

        return Self {
            vulkan_ctx,
            resources,

            _buffer_setup_ppl: buffer_setup_ppl,
            _chunk_init_ppl: chunk_init_ppl,
            _chunk_modify_ppl: chunk_modify_ppl,
            _leaf_write_ppl: leaf_write_ppl,

            _buffer_setup_ds: buffer_setup_ds,
            _chunk_init_ds: chunk_init_ds,
            _chunk_modify_ds: chunk_modify_ds,
            _leaf_write_ds: leaf_write_ds,

            // free_atlas_allocator: AtlasAllocator::new(free_atlas_dim),
            build_cmdbuf,
        };

        fn init_atlas_images(vulkan_context: &VulkanContext, resources: &PlainBuilderResources) {
            execute_one_time_command(
                vulkan_context.device(),
                vulkan_context.command_pool(),
                &vulkan_context.get_general_queue(),
                |cmdbuf| {
                    resources.chunk_atlas.get_image().record_clear(
                        cmdbuf,
                        Some(vk::ImageLayout::GENERAL),
                        ClearValue::UInt([0, 0, 0, 0]),
                    );
                    resources.free_atlas.get_image().record_clear(
                        cmdbuf,
                        Some(vk::ImageLayout::GENERAL),
                        ClearValue::UInt([0, 0, 0, 0]),
                    );
                },
            );
        }

        fn record_build_cmdbuf(
            vulkan_ctx: &VulkanContext,
            chunk_atlas: &Texture,
            region_indirect: &Buffer,
            buffer_setup_ppl: &ComputePipeline,
            chunk_init_ppl: &ComputePipeline,
            buffer_setup_ds: &DescriptorSet,
            chunk_init_ds: &DescriptorSet,
        ) -> CommandBuffer {
            let shader_access_memory_barrier = MemoryBarrier::new_shader_access();
            let indirect_access_memory_barrier = MemoryBarrier::new_indirect_access();

            let shader_access_pipeline_barrier = PipelineBarrier::new(
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vec![shader_access_memory_barrier],
            );
            let indirect_access_pipeline_barrier = PipelineBarrier::new(
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::DRAW_INDIRECT | vk::PipelineStageFlags::COMPUTE_SHADER,
                vec![indirect_access_memory_barrier],
            );

            let cmdbuf = CommandBuffer::new(vulkan_ctx.device(), vulkan_ctx.command_pool());
            cmdbuf.begin(false);

            chunk_atlas
                .get_image()
                .record_transition_barrier(&cmdbuf, vk::ImageLayout::GENERAL);

            buffer_setup_ppl.record_bind(&cmdbuf);
            buffer_setup_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(&buffer_setup_ds),
                0,
            );
            buffer_setup_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);

            shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
            indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

            chunk_init_ppl.record_bind(&cmdbuf);
            chunk_init_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(&chunk_init_ds),
                0,
            );
            chunk_init_ppl.record_dispatch_indirect(&cmdbuf, region_indirect);

            cmdbuf.end();
            return cmdbuf;
        }
    }

    pub fn get_resources(&self) -> &PlainBuilderResources {
        &self.resources
    }

    pub fn chunk_init(&mut self, atlas_offset: UVec3, atlas_dim: UVec3) {
        update_buffers(&self.resources, atlas_offset, atlas_dim);

        self.build_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        fn update_buffers(resources: &PlainBuilderResources, offset: UVec3, dim: UVec3) {
            let data = StructMemberDataBuilder::from_buffer(&resources.region_info)
                .set_field("offset", PlainMemberTypeWithData::UVec3(offset.to_array()))
                .unwrap()
                .set_field("dim", PlainMemberTypeWithData::UVec3(dim.to_array()))
                .unwrap()
                .get_data_u8();
            resources.region_info.fill_with_raw_u8(&data).unwrap();
        }
    }

    // pub fn create_leaf(
    //     &mut self,
    //     vulkan_context: &VulkanContext,
    //     resources: &PlainBuilderResources,
    //     leaf_color: Vec3,
    //     leaf_chunk_dim: UVec3,
    // ) -> UVec3 {
    //     // TODO: debug:
    //     let offset = UVec3::new(0, 0, 0);
    //     let dimension = leaf_chunk_dim;
    //     update_buffers(resources, leaf_color, offset, dimension);

    //     execute_one_time_command(
    //         vulkan_context.device(),
    //         vulkan_context.command_pool(),
    //         &vulkan_context.get_general_queue(),
    //         |cmdbuf| {
    //             self.leaf_write_ppl.record_bind(cmdbuf);
    //             self.leaf_write_ppl.record_bind_descriptor_sets(
    //                 cmdbuf,
    //                 std::slice::from_ref(&self.leaf_write_ds),
    //                 0,
    //             );
    //             self.leaf_write_ppl
    //                 .record_dispatch(cmdbuf, leaf_chunk_dim.to_array());
    //         },
    //     );

    //     return offset;

    //     fn update_buffers(
    //         resources: &PlainBuilderResources,
    //         leaf_color: Vec3,
    //         atlas_write_offset: UVec3,
    //         atlas_write_dim: UVec3,
    //     ) {
    //         let data = StructMemberDataBuilder::from_buffer(&resources.leaf_write_info)
    //             .set_field(
    //                 "leaf_color",
    //                 PlainMemberTypeWithData::Vec3(leaf_color.to_array()),
    //             )
    //             .unwrap()
    //             .set_field(
    //                 "atlas_write_offset",
    //                 PlainMemberTypeWithData::UVec3(atlas_write_offset.to_array()),
    //             )
    //             .unwrap()
    //             .set_field(
    //                 "atlas_write_dim",
    //                 PlainMemberTypeWithData::UVec3(atlas_write_dim.to_array()),
    //             )
    //             .unwrap()
    //             .get_data_u8();
    //         resources.leaf_write_info.fill_with_raw_u8(&data).unwrap();
    //     }
    // }

    // pub fn chunk_modify(
    //     &mut self,
    //     vulkan_context: &VulkanContext,
    //     resources: &PlainBuilderResources,
    //     voxel_dim: UVec3,
    //     chunk_pos: UVec3,
    //     bvh_nodes: &[BvhNode],
    //     round_cones: &[RoundCone],
    // ) {
    //     update_buffers(resources, chunk_pos, round_cones, bvh_nodes);

    //     execute_one_time_command(
    //         vulkan_context.device(),
    //         vulkan_context.command_pool(),
    //         &vulkan_context.get_general_queue(),
    //         |cmdbuf| {
    //             self.chunk_modify_ppl.record_bind(cmdbuf);
    //             self.chunk_modify_ppl.record_bind_descriptor_sets(
    //                 cmdbuf,
    //                 std::slice::from_ref(&self.chunk_modify_ds),
    //                 0,
    //             );
    //             self.chunk_modify_ppl
    //                 .record_dispatch(cmdbuf, voxel_dim.to_array());
    //         },
    //     );

    //     fn update_buffers(
    //         resources: &PlainBuilderResources,
    //         chunk_pos: UVec3,
    //         round_cones: &[RoundCone],
    //         bvh_nodes: &[BvhNode],
    //     ) {
    //         update_chunk_modify_info(resources, chunk_pos, 1);
    //         update_round_cones(resources, round_cones);
    //         update_trunk_bvh_nodes(resources, bvh_nodes);

    //         fn update_chunk_modify_info(
    //             resources: &PlainBuilderResources,
    //             chunk_pos: UVec3,
    //             fill_voxel_type: u32,
    //         ) {
    //             let data = StructMemberDataBuilder::from_buffer(&resources.chunk_modify_info)
    //                 .set_field(
    //                     "chunk_pos",
    //                     PlainMemberTypeWithData::UVec3(chunk_pos.to_array()),
    //                 )
    //                 .unwrap()
    //                 .set_field(
    //                     "fill_voxel_type",
    //                     PlainMemberTypeWithData::UInt(fill_voxel_type),
    //                 )
    //                 .unwrap()
    //                 .get_data_u8();
    //             resources.chunk_modify_info.fill_with_raw_u8(&data).unwrap();
    //         }

    //         fn update_round_cones(resources: &PlainBuilderResources, round_cones: &[RoundCone]) {
    //             for i in 0..round_cones.len() {
    //                 let round_cone = &round_cones[i];
    //                 let data = StructMemberDataBuilder::from_buffer(&resources.round_cones)
    //                     .set_field(
    //                         "data.center_a",
    //                         PlainMemberTypeWithData::Vec3(round_cone.center_a().to_array()),
    //                     )
    //                     .unwrap()
    //                     .set_field(
    //                         "data.center_b",
    //                         PlainMemberTypeWithData::Vec3(round_cone.center_b().to_array()),
    //                     )
    //                     .unwrap()
    //                     .set_field(
    //                         "data.radius_a",
    //                         PlainMemberTypeWithData::Float(round_cone.radius_a()),
    //                     )
    //                     .unwrap()
    //                     .set_field(
    //                         "data.radius_b",
    //                         PlainMemberTypeWithData::Float(round_cone.radius_b()),
    //                     )
    //                     .unwrap()
    //                     .get_data_u8();
    //                 resources
    //                     .round_cones
    //                     .fill_element_with_raw_u8(&data, i as u64)
    //                     .unwrap();
    //             }
    //         }

    //         fn update_trunk_bvh_nodes(resources: &PlainBuilderResources, bvh_nodes: &[BvhNode]) {
    //             for i in 0..bvh_nodes.len() {
    //                 let bvh_node = &bvh_nodes[i];

    //                 let combined_offset: u32 = if bvh_node.is_leaf {
    //                     let primitive_idx = bvh_node.data_offset;
    //                     0x8000_0000 | primitive_idx
    //                 } else {
    //                     bvh_node.left
    //                 };
    //                 let data = StructMemberDataBuilder::from_buffer(&resources.trunk_bvh_nodes)
    //                     .set_field(
    //                         "data.aabb_min",
    //                         PlainMemberTypeWithData::Vec3(bvh_node.aabb.min().to_array()),
    //                     )
    //                     .unwrap()
    //                     .set_field(
    //                         "data.aabb_max",
    //                         PlainMemberTypeWithData::Vec3(bvh_node.aabb.max().to_array()),
    //                     )
    //                     .unwrap()
    //                     .set_field(
    //                         "data.offset",
    //                         PlainMemberTypeWithData::UInt(combined_offset),
    //                     )
    //                     .unwrap()
    //                     .get_data_u8();
    //                 resources
    //                     .trunk_bvh_nodes
    //                     .fill_element_with_raw_u8(&data, i as u64)
    //                     .unwrap();
    //             }
    //         }
    //     }
    // }
}
