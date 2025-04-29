use super::Resources;
use crate::geom::BvhNode;
use crate::geom::RoundCone;
use crate::util::AtlasAllocator;
use crate::util::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::ClearValue;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::PlainMemberTypeWithData;
use crate::vkn::ShaderModule;
use crate::vkn::StructMemberDataBuilder;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::UVec3;
use glam::Vec3;

pub struct ChunkWriter {
    chunk_init_ppl: ComputePipeline,
    chunk_modify_ppl: ComputePipeline,
    leaf_write_ppl: ComputePipeline,

    chunk_init_ds: DescriptorSet,
    chunk_modify_ds: DescriptorSet,
    leaf_write_ds: DescriptorSet,

    free_atlas_allocator: AtlasAllocator,
}

impl ChunkWriter {
    pub fn new(
        vulkan_context: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        descriptor_pool: DescriptorPool,
        resources: &Resources,
        free_atlas_dim: UVec3,
    ) -> Self {
        let chunk_init_ppl = ComputePipeline::from_shader_module(
            vulkan_context.device(),
            &ShaderModule::from_glsl(
                vulkan_context.device(),
                &shader_compiler,
                "shader/builder/chunk_writer/chunk_init.comp",
                "main",
            )
            .unwrap(),
        );

        let chunk_init_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &chunk_init_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        chunk_init_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.chunk_init_info()),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                resources.chunk_atlas(),
                vk::ImageLayout::GENERAL,
            ),
        ]);

        let chunk_modify_ppl = ComputePipeline::from_shader_module(
            vulkan_context.device(),
            &ShaderModule::from_glsl(
                vulkan_context.device(),
                &shader_compiler,
                "shader/builder/chunk_writer/chunk_modify.comp",
                "main",
            )
            .unwrap(),
        );
        let chunk_modify_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &chunk_modify_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        chunk_modify_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.chunk_modify_info()),
            WriteDescriptorSet::new_buffer_write(1, resources.trunk_bvh_nodes()),
            WriteDescriptorSet::new_buffer_write(2, resources.round_cones()),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                resources.chunk_atlas(),
                vk::ImageLayout::GENERAL,
            ),
        ]);

        let leaf_write_ppl = ComputePipeline::from_shader_module(
            vulkan_context.device(),
            &ShaderModule::from_glsl(
                vulkan_context.device(),
                &shader_compiler,
                "shader/builder/chunk_writer/leaf_write.comp",
                "main",
            )
            .unwrap(),
        );
        let leaf_write_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &leaf_write_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        leaf_write_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.leaf_write_info()),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                resources.free_atlas(),
                vk::ImageLayout::GENERAL,
            ),
        ]);

        init_atlas(vulkan_context, resources);
        fn init_atlas(vulkan_context: &VulkanContext, resources: &Resources) {
            execute_one_time_command(
                vulkan_context.device(),
                vulkan_context.command_pool(),
                &vulkan_context.get_general_queue(),
                |cmdbuf| {
                    resources.chunk_atlas().get_image().record_clear(
                        cmdbuf,
                        Some(vk::ImageLayout::GENERAL),
                        ClearValue::UInt([0, 0, 0, 0]),
                    );
                    resources.free_atlas().get_image().record_clear(
                        cmdbuf,
                        Some(vk::ImageLayout::GENERAL),
                        ClearValue::UInt([0, 0, 0, 0]),
                    );
                },
            );
        }

        let free_atlas_allocator = AtlasAllocator::new(free_atlas_dim);

        Self {
            chunk_init_ppl,
            chunk_modify_ppl,
            leaf_write_ppl,

            chunk_init_ds,
            chunk_modify_ds,
            leaf_write_ds,

            free_atlas_allocator,
        }
    }

    pub fn chunk_init(
        &mut self,
        vulkan_context: &VulkanContext,
        resources: &Resources,
        voxel_dim: UVec3,
        chunk_pos: UVec3,
    ) {
        update_buffers(resources, chunk_pos);

        execute_one_time_command(
            vulkan_context.device(),
            vulkan_context.command_pool(),
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                resources
                    .chunk_atlas()
                    .get_image()
                    .record_transition_barrier(cmdbuf, vk::ImageLayout::GENERAL);

                self.chunk_init_ppl.record_bind(cmdbuf);
                self.chunk_init_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.chunk_init_ds),
                    0,
                );
                self.chunk_init_ppl
                    .record_dispatch(cmdbuf, voxel_dim.to_array());
            },
        );

        fn update_buffers(resources: &Resources, chunk_pos: UVec3) {
            let data = StructMemberDataBuilder::from_buffer(&resources.chunk_init_info())
                .set_field(
                    "chunk_pos",
                    PlainMemberTypeWithData::UVec3(chunk_pos.to_array()),
                )
                .unwrap()
                .get_data_u8();
            resources.chunk_init_info().fill_with_raw_u8(&data).unwrap();
        }
    }

    pub fn create_leaf(
        &mut self,
        vulkan_context: &VulkanContext,
        resources: &Resources,
        leaf_color: Vec3,
        leaf_chunk_dim: UVec3,
    ) -> UVec3 {
        let allocation = self.free_atlas_allocator.allocate(leaf_chunk_dim).unwrap();
        let _id = allocation.id; // TODO: maybe store this later to modify

        update_buffers(resources, leaf_color, allocation.offset, allocation.dim);

        execute_one_time_command(
            vulkan_context.device(),
            vulkan_context.command_pool(),
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                self.leaf_write_ppl.record_bind(cmdbuf);
                self.leaf_write_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.leaf_write_ds),
                    0,
                );
                self.leaf_write_ppl
                    .record_dispatch(cmdbuf, leaf_chunk_dim.to_array());
            },
        );

        return allocation.offset;

        fn update_buffers(
            resources: &Resources,
            leaf_color: Vec3,
            atlas_write_offset: UVec3,
            atlas_write_dim: UVec3,
        ) {
            let data = StructMemberDataBuilder::from_buffer(&resources.leaf_write_info())
                .set_field(
                    "leaf_color",
                    PlainMemberTypeWithData::Vec3(leaf_color.to_array()),
                )
                .unwrap()
                .set_field(
                    "atlas_write_offset",
                    PlainMemberTypeWithData::UVec3(atlas_write_offset.to_array()),
                )
                .unwrap()
                .set_field(
                    "atlas_write_dim",
                    PlainMemberTypeWithData::UVec3(atlas_write_dim.to_array()),
                )
                .unwrap()
                .get_data_u8();
            resources.leaf_write_info().fill_with_raw_u8(&data).unwrap();
        }
    }

    pub fn chunk_modify(
        &mut self,
        vulkan_context: &VulkanContext,
        resources: &Resources,
        voxel_dim: UVec3,
        chunk_pos: UVec3,
        bvh_nodes: &[BvhNode],
        round_cones: &[RoundCone],
    ) {
        update_buffers(resources, chunk_pos, round_cones, bvh_nodes);

        execute_one_time_command(
            vulkan_context.device(),
            vulkan_context.command_pool(),
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                self.chunk_modify_ppl.record_bind(cmdbuf);
                self.chunk_modify_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.chunk_modify_ds),
                    0,
                );
                self.chunk_modify_ppl
                    .record_dispatch(cmdbuf, voxel_dim.to_array());
            },
        );

        fn update_buffers(
            resources: &Resources,
            chunk_pos: UVec3,
            round_cones: &[RoundCone],
            bvh_nodes: &[BvhNode],
        ) {
            update_chunk_modify_info(resources, chunk_pos, 1);
            update_round_cones(resources, round_cones);
            update_trunk_bvh_nodes(resources, bvh_nodes);

            fn update_chunk_modify_info(
                resources: &Resources,
                chunk_pos: UVec3,
                fill_voxel_type: u32,
            ) {
                let data = StructMemberDataBuilder::from_buffer(resources.chunk_modify_info())
                    .set_field(
                        "chunk_pos",
                        PlainMemberTypeWithData::UVec3(chunk_pos.to_array()),
                    )
                    .unwrap()
                    .set_field(
                        "fill_voxel_type",
                        PlainMemberTypeWithData::UInt(fill_voxel_type),
                    )
                    .unwrap()
                    .get_data_u8();
                resources
                    .chunk_modify_info()
                    .fill_with_raw_u8(&data)
                    .unwrap();
            }

            fn update_round_cones(resources: &Resources, round_cones: &[RoundCone]) {
                for i in 0..round_cones.len() {
                    let round_cone = &round_cones[i];
                    let data = StructMemberDataBuilder::from_buffer(resources.round_cones())
                        .set_field(
                            "data.center_a",
                            PlainMemberTypeWithData::Vec3(round_cone.center_a().to_array()),
                        )
                        .unwrap()
                        .set_field(
                            "data.center_b",
                            PlainMemberTypeWithData::Vec3(round_cone.center_b().to_array()),
                        )
                        .unwrap()
                        .set_field(
                            "data.radius_a",
                            PlainMemberTypeWithData::Float(round_cone.radius_a()),
                        )
                        .unwrap()
                        .set_field(
                            "data.radius_b",
                            PlainMemberTypeWithData::Float(round_cone.radius_b()),
                        )
                        .unwrap()
                        .get_data_u8();
                    resources
                        .round_cones()
                        .fill_element_with_raw_u8(&data, i as u64)
                        .unwrap();
                }
            }

            fn update_trunk_bvh_nodes(resources: &Resources, bvh_nodes: &[BvhNode]) {
                for i in 0..bvh_nodes.len() {
                    let bvh_node = &bvh_nodes[i];

                    let combined_offset: u32 = if bvh_node.is_leaf {
                        let primitive_idx = bvh_node.data_offset;
                        0x8000_0000 | primitive_idx
                    } else {
                        bvh_node.left
                    };
                    let data = StructMemberDataBuilder::from_buffer(resources.trunk_bvh_nodes())
                        .set_field(
                            "data.aabb_min",
                            PlainMemberTypeWithData::Vec3(bvh_node.aabb.min().to_array()),
                        )
                        .unwrap()
                        .set_field(
                            "data.aabb_max",
                            PlainMemberTypeWithData::Vec3(bvh_node.aabb.max().to_array()),
                        )
                        .unwrap()
                        .set_field(
                            "data.offset",
                            PlainMemberTypeWithData::UInt(combined_offset),
                        )
                        .unwrap()
                        .get_data_u8();
                    resources
                        .trunk_bvh_nodes()
                        .fill_element_with_raw_u8(&data, i as u64)
                        .unwrap();
                }
            }
        }
    }
}
