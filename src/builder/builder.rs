use super::BuilderResources;
use super::Chunk;
use crate::util::compiler::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::Allocator;
use crate::vkn::BufferBuilder;
use crate::vkn::CommandPool;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::Device;
use crate::vkn::MemoryBarrier;
use crate::vkn::PipelineBarrier;
use crate::vkn::ShaderModule;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::IVec3;
use glam::UVec3;
use std::collections::HashMap;

pub struct Builder {
    vulkan_context: VulkanContext,

    resources: BuilderResources,

    chunk_init_sm: ShaderModule,
    frag_list_maker_sm: ShaderModule,

    chunk_init_ppl: ComputePipeline,
    frag_list_maker_ppl: ComputePipeline,

    chunk_shared_ds: DescriptorSet,
    chunk_init_ds: DescriptorSet,
    frag_list_maker_ds: DescriptorSet,

    octree_init_buffers_sm: ShaderModule,
    octree_init_node_sm: ShaderModule,
    octree_tag_node_sm: ShaderModule,
    octree_alloc_node_sm: ShaderModule,
    octree_modify_args_sm: ShaderModule,

    octree_init_buffers_ppl: ComputePipeline,
    octree_init_node_ppl: ComputePipeline,
    octree_tag_node_ppl: ComputePipeline,
    octree_alloc_node_ppl: ComputePipeline,
    octree_modify_args_ppl: ComputePipeline,

    octree_shared_ds: DescriptorSet,
    octree_init_buffers_ds: DescriptorSet,
    octree_init_node_ds: DescriptorSet,
    octree_tag_node_ds: DescriptorSet,
    octree_alloc_node_ds: DescriptorSet,
    octree_modify_args_ds: DescriptorSet,

    chunk_res: UVec3,
    chunks: HashMap<IVec3, Chunk>,
}

impl Builder {
    pub fn new(
        vulkan_context: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        chunk_res: UVec3,
    ) -> Self {
        if chunk_res.x != chunk_res.y || chunk_res.y != chunk_res.z {
            log::error!("Resolution must be equal in all dimensions");
        }
        if chunk_res.x & (chunk_res.x - 1) != 0 {
            log::error!("Resolution must be a power of 2");
        }

        let chunk_init_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/chunk_init.comp",
            "main",
        )
        .unwrap();
        let chunk_init_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &chunk_init_sm);

        let frag_list_maker_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/frag_list_maker.comp",
            "main",
        )
        .unwrap();
        let frag_list_maker_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &frag_list_maker_sm);

        let octree_init_buffers_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree_init_buffers.comp",
            "main",
        )
        .unwrap();
        let octree_init_buffers_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_init_buffers_sm);

        let octree_init_node_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree_init_node.comp",
            "main",
        )
        .unwrap();
        let octree_init_node_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_init_node_sm);

        let octree_tag_node_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree_tag_node.comp",
            "main",
        )
        .unwrap();
        let octree_tag_node_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_tag_node_sm);

        let octree_alloc_node_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree_alloc_node.comp",
            "main",
        )
        .unwrap();
        let octree_alloc_node_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_alloc_node_sm);

        let octree_modify_args_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree_modify_args.comp",
            "main",
        )
        .unwrap();
        let octree_modify_args_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_modify_args_sm);

        let descriptor_pool = DescriptorPool::a_big_one(vulkan_context.device()).unwrap();

        let resources = BuilderResources::new(
            vulkan_context.device().clone(),
            allocator.clone(),
            &chunk_init_sm,
            &frag_list_maker_sm,
            &octree_init_buffers_sm,
            chunk_res,
        );

        let (chunk_shared_ds, chunk_init_ds, frag_list_maker_ds) =
            Self::create_frag_builder_descriptor_sets(
                descriptor_pool.clone(),
                vulkan_context.device().clone(),
                &chunk_init_ppl,
                &frag_list_maker_ppl,
                &resources,
            );

        let (
            octree_shared_ds,
            octree_init_buffers_ds,
            octree_init_node_ds,
            octree_tag_node_ds,
            octree_alloc_node_ds,
            octree_modify_args_ds,
        ) = Self::create_octree_builder_descriptor_sets(
            descriptor_pool.clone(),
            vulkan_context.device().clone(),
            &octree_init_buffers_ppl,
            &octree_init_node_ppl,
            &octree_tag_node_ppl,
            &octree_alloc_node_ppl,
            &octree_modify_args_ppl,
            &resources,
        );

        Self {
            vulkan_context,
            resources,

            chunk_init_sm,
            frag_list_maker_sm,

            chunk_init_ppl,
            frag_list_maker_ppl,

            chunk_shared_ds,
            chunk_init_ds,
            frag_list_maker_ds,

            octree_init_buffers_sm,
            octree_init_node_sm,
            octree_tag_node_sm,
            octree_alloc_node_sm,
            octree_modify_args_sm,

            octree_init_buffers_ppl,
            octree_init_node_ppl,
            octree_tag_node_ppl,
            octree_alloc_node_ppl,
            octree_modify_args_ppl,

            octree_shared_ds,
            octree_init_buffers_ds,
            octree_init_node_ds,
            octree_tag_node_ds,
            octree_alloc_node_ds,
            octree_modify_args_ds,

            chunk_res,
            chunks: HashMap::new(),
        }
    }

    pub fn init_chunk(&mut self, command_pool: &CommandPool, chunk_pos: IVec3) {
        self.update_chunk_build_info_buf(self.chunk_res, chunk_pos);
        self.reset_fragment_list_info_buf();

        self.write_block_tex_by_noise(command_pool, self.chunk_res);
        self.fill_fragment_list_from_block_tex(command_pool, self.chunk_res);

        let fragment_list_len = self.get_fraglist_length();
        self.update_octree_build_info_buf(self.chunk_res, fragment_list_len);

        self.make_octree_by_frag_list(command_pool);

        let chunk = Chunk {
            res: self.chunk_res,
            pos: chunk_pos,
            data: vec![], // the data is not sent back to CPU for now
        };
        self.chunks.insert(chunk_pos, chunk);
    }

    fn create_frag_builder_descriptor_sets(
        descriptor_pool: DescriptorPool,
        device: Device,
        chunk_init_ppl: &ComputePipeline,
        frag_list_maker_ppl: &ComputePipeline,
        resources: &BuilderResources,
    ) -> (DescriptorSet, DescriptorSet, DescriptorSet) {
        // this set is shared between all pipelines
        let shared_ds = DescriptorSet::new(
            device.clone(),
            &chunk_init_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        shared_ds.perform_writes(&[WriteDescriptorSet::new_buffer_write(
            0,
            &resources.chunk_build_info,
        )]);

        let chunk_init_ds = DescriptorSet::new(
            device.clone(),
            &chunk_init_ppl.get_layout().get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        chunk_init_ds.perform_writes(&[WriteDescriptorSet::new_texture_write(
            0,
            vk::DescriptorType::STORAGE_IMAGE,
            &resources.blocks_tex,
            vk::ImageLayout::GENERAL,
        )]);

        let frag_list_maker_ds = DescriptorSet::new(
            device.clone(),
            &frag_list_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        frag_list_maker_ds.perform_writes(&[
            WriteDescriptorSet::new_texture_write(
                0,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.blocks_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_buffer_write(1, &resources.fragment_list_info),
            WriteDescriptorSet::new_buffer_write(2, &resources.fragment_list),
        ]);

        (shared_ds, chunk_init_ds, frag_list_maker_ds)
    }

    fn create_octree_builder_descriptor_sets(
        descriptor_pool: DescriptorPool,
        device: Device,
        octree_init_buffers_ppl: &ComputePipeline,
        octree_init_node_ppl: &ComputePipeline,
        octree_tag_node_ppl: &ComputePipeline,
        octree_alloc_node_ppl: &ComputePipeline,
        octree_modify_args_ppl: &ComputePipeline,
        resources: &BuilderResources,
    ) -> (
        DescriptorSet,
        DescriptorSet,
        DescriptorSet,
        DescriptorSet,
        DescriptorSet,
        DescriptorSet,
    ) {
        let shared_ds = DescriptorSet::new(
            device.clone(),
            &octree_init_buffers_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        shared_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, &resources.octree_build_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.octree_alloc_info),
        ]);

        let init_buffers_ds = DescriptorSet::new(
            device.clone(),
            &octree_init_buffers_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        init_buffers_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, &resources.voxel_count_indirect),
            WriteDescriptorSet::new_buffer_write(1, &resources.alloc_number_indirect),
            WriteDescriptorSet::new_buffer_write(2, &resources.counter),
            WriteDescriptorSet::new_buffer_write(3, &resources.octree_build_result),
        ]);

        let init_node_ds = DescriptorSet::new(
            device.clone(),
            &octree_init_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        init_node_ds.perform_writes(&[WriteDescriptorSet::new_buffer_write(
            0,
            &resources.octree_data,
        )]);

        let tag_node_ds = DescriptorSet::new(
            device.clone(),
            &octree_tag_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        tag_node_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, &resources.octree_data),
            WriteDescriptorSet::new_buffer_write(1, &resources.fragment_list),
        ]);

        let alloc_node_ds = DescriptorSet::new(
            device.clone(),
            &octree_alloc_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        alloc_node_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, &resources.octree_data),
            WriteDescriptorSet::new_buffer_write(1, &resources.fragment_list),
            WriteDescriptorSet::new_buffer_write(2, &resources.counter),
        ]);

        let modify_args_ds = DescriptorSet::new(
            device.clone(),
            &octree_modify_args_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        modify_args_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, &resources.counter),
            WriteDescriptorSet::new_buffer_write(1, &resources.octree_build_result),
            WriteDescriptorSet::new_buffer_write(2, &resources.alloc_number_indirect),
        ]);

        return (
            shared_ds,
            init_buffers_ds,
            init_node_ds,
            tag_node_ds,
            alloc_node_ds,
            modify_args_ds,
        );
    }

    fn update_chunk_build_info_buf(&mut self, resolution: UVec3, chunk_pos: IVec3) {
        // modify the uniform buffer to guide the chunk generation
        let chunk_build_info_layout = BufferBuilder::from_layout(
            self.chunk_init_sm
                .get_buffer_layout("U_ChunkBuildInfo")
                .unwrap(),
        );
        let chunk_build_info_data = chunk_build_info_layout
            .set_uvec3("chunk_res", resolution.to_array())
            .set_ivec3("chunk_pos", chunk_pos.to_array())
            .to_raw_data();
        self.resources
            .chunk_build_info
            .fill_raw(&chunk_build_info_data)
            .expect("Failed to fill buffer data");
    }

    fn update_octree_build_info_buf(&mut self, resolution: UVec3, fragment_list_length: u32) {
        let octree_build_info_layout = BufferBuilder::from_layout(
            self.octree_init_buffers_sm
                .get_buffer_layout("B_OctreeBuildInfo")
                .unwrap(),
        );
        assert!(resolution.x == resolution.y && resolution.y == resolution.z);
        let octree_build_info_data = octree_build_info_layout
            .set_uint("chunk_res_xyz", resolution.x as u32)
            .set_uint("fragment_list_len", fragment_list_length)
            .to_raw_data();
        self.resources
            .octree_build_info
            .fill_raw(&octree_build_info_data)
            .expect("Failed to fill buffer data");
    }

    fn reset_fragment_list_info_buf(&self) {
        // reset the fragment list info buffer
        let fragment_list_info_layout = BufferBuilder::from_layout(
            self.frag_list_maker_sm
                .get_buffer_layout("B_FragmentListInfo")
                .unwrap(),
        );
        let fragment_list_info_data = fragment_list_info_layout
            .set_uint("fragment_list_len", 0)
            .to_raw_data();
        self.resources
            .fragment_list_info
            .fill_raw(&fragment_list_info_data)
            .expect("Failed to fill buffer data");
    }

    /// Ask the builder to write the block texture from noise, chunk build info buffer must be ready before calling.
    fn write_block_tex_by_noise(&mut self, command_pool: &CommandPool, resolution: UVec3) {
        execute_one_time_command(
            self.vulkan_context.device(),
            command_pool,
            &self.vulkan_context.get_general_queue(),
            |cmdbuf| {
                self.resources
                    .blocks_tex
                    .get_image()
                    .record_transition_barrier(cmdbuf, vk::ImageLayout::GENERAL);

                self.chunk_init_ppl.record_bind(cmdbuf);
                self.chunk_init_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.chunk_shared_ds),
                    0,
                );
                self.chunk_init_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.chunk_init_ds),
                    1,
                );
                self.chunk_init_ppl
                    .record_dispatch(cmdbuf, resolution.to_array());
            },
        );
    }

    /// Generate the chunk data and fill the fragment list buffer.
    fn fill_fragment_list_from_block_tex(&mut self, command_pool: &CommandPool, resolution: UVec3) {
        execute_one_time_command(
            self.vulkan_context.device(),
            command_pool,
            &self.vulkan_context.get_general_queue(),
            |cmdbuf| {
                self.resources
                    .blocks_tex
                    .get_image()
                    .record_transition_barrier(cmdbuf, vk::ImageLayout::GENERAL);

                self.frag_list_maker_ppl.record_bind(cmdbuf);
                self.frag_list_maker_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.chunk_shared_ds),
                    0,
                );
                self.frag_list_maker_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.frag_list_maker_ds),
                    1,
                );
                self.frag_list_maker_ppl
                    .record_dispatch(cmdbuf, resolution.to_array());
            },
        );
    }

    fn get_fraglist_length(&self) -> u32 {
        // TODO: can we store buffer layout inside buffer?
        let raw_data = self.resources.fragment_list_info.fetch_raw().unwrap();
        let fragment_list_info_layout = self
            .frag_list_maker_sm
            .get_buffer_layout("B_FragmentListInfo")
            .unwrap();
        let data_to_fetch = BufferBuilder::from_layout(fragment_list_info_layout).set_raw(raw_data);
        data_to_fetch.get_uint("fragment_list_len").unwrap()
    }

    fn make_octree_by_frag_list(&mut self, command_pool: &CommandPool) {
        let device = self.vulkan_context.device();

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

        fn log2(x: u32) -> u32 {
            31 - x.leading_zeros()
        }

        execute_one_time_command(
            self.vulkan_context.device(),
            command_pool,
            &self.vulkan_context.get_general_queue(),
            |cmdbuf| {
                self.octree_init_buffers_ppl.record_bind(cmdbuf);
                self.octree_init_buffers_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.octree_shared_ds),
                    0,
                );
                self.octree_init_buffers_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.octree_init_buffers_ds),
                    1,
                );
                self.octree_init_buffers_ppl
                    .record_dispatch(cmdbuf, [1, 1, 1]);
                shader_access_pipeline_barrier.record_insert(device, cmdbuf);
                indirect_access_pipeline_barrier.record_insert(device, cmdbuf);

                let voxel_level_count = log2(self.chunk_res.x);
                log::debug!("Voxel level count: {}", voxel_level_count);

                for i in 0..voxel_level_count {
                    self.octree_init_node_ppl.record_bind(cmdbuf);
                    self.octree_init_node_ppl.record_bind_descriptor_sets(
                        cmdbuf,
                        std::slice::from_ref(&self.octree_init_node_ds),
                        1,
                    );
                    self.octree_init_node_ppl
                        .record_dispatch_indirect(cmdbuf, &self.resources.alloc_number_indirect);

                    // shader_access_pipeline_barrier.record_insert(device, cmdbuf);

                    // self.octree_tag_node_ppl.record_bind(cmdbuf);
                    // self.octree_tag_node_ppl.record_bind_descriptor_sets(
                    //     cmdbuf,
                    //     std::slice::from_ref(&self.octree_tag_node_ds),
                    //     1,
                    // );
                    // self.octree_tag_node_ppl
                    //     .record_dispatch_indirect(cmdbuf, &self.resources.fragment_list);

                    // // if not last level
                    // if i != voxel_level_count - 1 {
                    //     shader_access_pipeline_barrier.record_insert(device, cmdbuf);

                    //     self.octree_alloc_node_ppl.record_bind(cmdbuf);
                    //     self.octree_alloc_node_ppl.record_bind_descriptor_sets(
                    //         cmdbuf,
                    //         std::slice::from_ref(&self.octree_alloc_node_ds),
                    //         1,
                    //     );
                    //     self.octree_alloc_node_ppl
                    //         .record_dispatch_indirect(cmdbuf, &self.resources.counter);

                    //     shader_access_pipeline_barrier.record_insert(device, cmdbuf);

                    //     self.octree_modify_args_ppl.record_bind(cmdbuf);
                    //     self.octree_modify_args_ppl.record_bind_descriptor_sets(
                    //         cmdbuf,
                    //         std::slice::from_ref(&self.octree_modify_args_ds),
                    //         1,
                    //     );
                    //     self.octree_modify_args_ppl
                    //         .record_dispatch(cmdbuf, [1, 1, 1]);

                    //     shader_access_pipeline_barrier.record_insert(device, cmdbuf);
                    //     indirect_access_pipeline_barrier.record_insert(device, cmdbuf);
                    // }
                }
            },
        );
    }
}
