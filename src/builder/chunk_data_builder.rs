use super::Resources;
use crate::geom::BvhNode;
use crate::geom::RoundCone;
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

pub struct ChunkDataBuilder {
    chunk_init_ppl: ComputePipeline,
    chunk_modify_ppl: ComputePipeline,

    chunk_init_ds: DescriptorSet,
    chunk_modify_ds: DescriptorSet,
}

impl ChunkDataBuilder {
    pub fn new(
        vulkan_context: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        descriptor_pool: DescriptorPool,
        resources: &Resources,
    ) -> Self {
        let chunk_init_ppl = ComputePipeline::from_shader_module(
            vulkan_context.device(),
            &ShaderModule::from_glsl(
                vulkan_context.device(),
                &shader_compiler,
                "shader/builder/chunk_data_builder/chunk_init.comp",
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
                resources.raw_atlas_tex(),
                vk::ImageLayout::GENERAL,
            ),
        ]);

        let chunk_modify_ppl = ComputePipeline::from_shader_module(
            vulkan_context.device(),
            &ShaderModule::from_glsl(
                vulkan_context.device(),
                &shader_compiler,
                "shader/builder/chunk_data_builder/chunk_modify.comp",
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
            WriteDescriptorSet::new_buffer_write(1, resources.bvh_nodes()),
            WriteDescriptorSet::new_buffer_write(2, resources.round_cones()),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                resources.raw_atlas_tex(),
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
                    resources.raw_atlas_tex().get_image().record_clear(
                        cmdbuf,
                        Some(vk::ImageLayout::GENERAL),
                        ClearValue::UInt([0, 0, 0, 0]),
                    );
                },
            );
        }

        Self {
            chunk_init_ppl,
            chunk_modify_ppl,
            chunk_init_ds,
            chunk_modify_ds,
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
                    .raw_atlas_tex()
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
            update_chunk_modify_info(resources, chunk_pos, 1, round_cones.len() as _);
            update_round_cones(resources, round_cones);
            update_bvh_nodes(resources, bvh_nodes);

            fn update_chunk_modify_info(
                resources: &Resources,
                chunk_pos: UVec3,
                fill_voxel_type: u32,
                round_cone_len: u32,
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
                    .set_field(
                        "round_cone_len",
                        PlainMemberTypeWithData::UInt(round_cone_len),
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

            fn update_bvh_nodes(resources: &Resources, bvh_nodes: &[BvhNode]) {
                for i in 0..bvh_nodes.len() {
                    let bvh_node = &bvh_nodes[i];
                    let data = StructMemberDataBuilder::from_buffer(resources.bvh_nodes())
                        .set_field(
                            "aabb_min",
                            PlainMemberTypeWithData::Vec3(bvh_node.aabb.min().to_array()),
                        )
                        .unwrap()
                        .set_field(
                            "aabb_max",
                            PlainMemberTypeWithData::Vec3(bvh_node.aabb.max().to_array()),
                        )
                        .unwrap()
                        .set_field("left", PlainMemberTypeWithData::UInt(bvh_node.left))
                        .unwrap()
                        .set_field("right", PlainMemberTypeWithData::UInt(bvh_node.right))
                        .unwrap()
                        .get_data_u8();
                    resources
                        .bvh_nodes()
                        .fill_element_with_raw_u8(&data, i as u64)
                        .unwrap();
                }
            }
        }
    }
}
