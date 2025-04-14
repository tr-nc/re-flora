use super::BuilderResources;
use super::Chunk;
use crate::util::compiler::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::Allocator;
use crate::vkn::Buffer;
use crate::vkn::BufferBuilder;
use crate::vkn::CommandPool;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::MemoryBarrier;
use crate::vkn::PipelineBarrier;
use crate::vkn::ShaderModule;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::IVec3;
use glam::UVec3;
use std::collections::HashMap;

pub struct ChunkInitBuilder {
    chunk_init_ppl: ComputePipeline,
    chunk_shared_ds: DescriptorSet,
    chunk_init_ds: DescriptorSet,
}

impl ChunkInitBuilder {
    fn new(
        vulkan_context: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        descriptor_pool: DescriptorPool,
        resources: &BuilderResources,
    ) -> Self {
        let chunk_init_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/chunk_init/chunk_init.comp",
            "main",
        )
        .unwrap();
        let chunk_init_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &chunk_init_sm);

        let chunk_shared_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &chunk_init_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        chunk_shared_ds.perform_writes(&[WriteDescriptorSet::new_buffer_write(
            0,
            resources.chunk_build_info(),
        )]);

        let chunk_init_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &chunk_init_ppl.get_layout().get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        chunk_init_ds.perform_writes(&[WriteDescriptorSet::new_buffer_write(
            0,
            resources.raw_voxels(),
        )]);

        Self {
            chunk_init_ppl,
            chunk_shared_ds,
            chunk_init_ds,
        }
    }

    fn update_chunk_build_info_buf(
        &self,
        resources: &BuilderResources,
        resolution: UVec3,
        chunk_pos: IVec3,
    ) {
        // modify the uniform buffer to guide the chunk generation
        let chunk_build_info_data = BufferBuilder::from_struct_buffer(resources.chunk_build_info())
            .unwrap()
            .set_uvec3("chunk_res", resolution.to_array())
            .set_ivec3("chunk_pos", chunk_pos.to_array())
            .to_raw_data();
        resources
            .chunk_build_info()
            .fill_raw(&chunk_build_info_data)
            .expect("Failed to fill buffer data");
    }

    fn init_chunk_by_noise(
        &self,
        vulkan_context: &VulkanContext,
        command_pool: &CommandPool,
        resolution: UVec3,
    ) {
        execute_one_time_command(
            vulkan_context.device(),
            command_pool,
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
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
}

pub struct FragListBuilder {
    frag_list_maker_ppl: ComputePipeline,
    frag_list_maker_ds: DescriptorSet,
}

impl FragListBuilder {
    fn new(
        vulkan_context: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        descriptor_pool: DescriptorPool,
        resources: &BuilderResources,
    ) -> Self {
        let frag_list_maker_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/frag_list_maker/frag_list_maker.comp",
            "main",
        )
        .unwrap();
        let frag_list_maker_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &frag_list_maker_sm);

        let frag_list_maker_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &frag_list_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        frag_list_maker_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.raw_voxels()),
            WriteDescriptorSet::new_buffer_write(1, resources.fragment_list_info()),
            WriteDescriptorSet::new_buffer_write(2, resources.fragment_list()),
        ]);

        Self {
            frag_list_maker_ppl,
            frag_list_maker_ds,
        }
    }

    fn reset_fragment_list_info_buf(&self, resources: &BuilderResources) {
        let fragment_list_info_data =
            BufferBuilder::from_struct_buffer(resources.fragment_list_info())
                .unwrap()
                .set_uint("fragment_list_len", 0)
                .to_raw_data();
        resources
            .fragment_list_info()
            .fill_raw(&fragment_list_info_data)
            .expect("Failed to fill buffer data");
    }

    fn make_frag_list(
        &self,
        vulkan_context: &VulkanContext,
        command_pool: &CommandPool,
        chunk_shared_ds: &DescriptorSet,
        resolution: UVec3,
    ) {
        execute_one_time_command(
            vulkan_context.device(),
            command_pool,
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                self.frag_list_maker_ppl.record_bind(cmdbuf);
                self.frag_list_maker_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(chunk_shared_ds),
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

    fn get_fraglist_length(&self, resources: &BuilderResources) -> u32 {
        let raw_data = resources.fragment_list_info().fetch_raw().unwrap();

        BufferBuilder::from_struct_buffer(resources.fragment_list_info())
            .unwrap()
            .set_raw(raw_data)
            .get_uint("fragment_list_len")
            .unwrap()
    }
}

pub struct OctreeBuilder {
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
}

impl OctreeBuilder {
    fn new(
        vulkan_context: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        descriptor_pool: DescriptorPool,
        resources: &BuilderResources,
    ) -> Self {
        let octree_init_buffers_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree/init_buffers.comp",
            "main",
        )
        .unwrap();
        let octree_init_buffers_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_init_buffers_sm);

        let octree_init_node_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree/init_node.comp",
            "main",
        )
        .unwrap();
        let octree_init_node_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_init_node_sm);

        let octree_tag_node_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree/tag_node.comp",
            "main",
        )
        .unwrap();
        let octree_tag_node_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_tag_node_sm);

        let octree_alloc_node_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree/alloc_node.comp",
            "main",
        )
        .unwrap();
        let octree_alloc_node_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_alloc_node_sm);

        let octree_modify_args_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree/modify_args.comp",
            "main",
        )
        .unwrap();
        let octree_modify_args_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_modify_args_sm);

        let shared_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_init_buffers_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        shared_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.octree_build_info()),
            WriteDescriptorSet::new_buffer_write(1, resources.octree_alloc_info()),
        ]);

        let init_buffers_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_init_buffers_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        init_buffers_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.voxel_count_indirect()),
            WriteDescriptorSet::new_buffer_write(1, resources.alloc_number_indirect()),
            WriteDescriptorSet::new_buffer_write(2, resources.counter()),
            WriteDescriptorSet::new_buffer_write(3, resources.octree_build_result()),
        ]);

        let init_node_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_init_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        init_node_ds.perform_writes(&[WriteDescriptorSet::new_buffer_write(
            0,
            resources.octree_data(),
        )]);

        let tag_node_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_tag_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        tag_node_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.octree_data()),
            WriteDescriptorSet::new_buffer_write(1, resources.fragment_list()),
        ]);

        let alloc_node_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_alloc_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        alloc_node_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.octree_data()),
            WriteDescriptorSet::new_buffer_write(1, resources.fragment_list()),
            WriteDescriptorSet::new_buffer_write(2, resources.counter()),
        ]);

        let modify_args_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_modify_args_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        modify_args_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.counter()),
            WriteDescriptorSet::new_buffer_write(1, resources.octree_build_result()),
            WriteDescriptorSet::new_buffer_write(2, resources.alloc_number_indirect()),
        ]);

        Self {
            octree_init_buffers_ppl,
            octree_init_node_ppl,
            octree_tag_node_ppl,
            octree_alloc_node_ppl,
            octree_modify_args_ppl,
            octree_shared_ds: shared_ds,
            octree_init_buffers_ds: init_buffers_ds,
            octree_init_node_ds: init_node_ds,
            octree_tag_node_ds: tag_node_ds,
            octree_alloc_node_ds: alloc_node_ds,
            octree_modify_args_ds: modify_args_ds,
        }
    }

    fn update_octree_build_info_buf(
        &self,
        resources: &BuilderResources,
        resolution: UVec3,
        fragment_list_length: u32,
    ) {
        assert!(resolution.x == resolution.y && resolution.y == resolution.z);
        let octree_build_info_data =
            BufferBuilder::from_struct_buffer(resources.octree_build_info())
                .unwrap()
                .set_uint("chunk_res_xyz", resolution.x as u32)
                .set_uint("fragment_list_len", fragment_list_length)
                .to_raw_data();
        resources
            .octree_build_info()
            .fill_raw(&octree_build_info_data)
            .expect("Failed to fill buffer data");
    }

    fn make_octree_by_frag_list(
        &self,
        vulkan_context: &VulkanContext,
        command_pool: &CommandPool,
        resources: &BuilderResources,
        chunk_res: UVec3,
    ) {
        let device = vulkan_context.device();

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
            vulkan_context.device(),
            command_pool,
            &vulkan_context.get_general_queue(),
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

                let voxel_level_count = log2(chunk_res.x);

                for i in 0..voxel_level_count {
                    self.octree_init_node_ppl.record_bind(cmdbuf);
                    self.octree_init_node_ppl.record_bind_descriptor_sets(
                        cmdbuf,
                        std::slice::from_ref(&self.octree_init_node_ds),
                        1,
                    );
                    self.octree_init_node_ppl
                        .record_dispatch_indirect(cmdbuf, resources.alloc_number_indirect());

                    shader_access_pipeline_barrier.record_insert(device, cmdbuf);

                    self.octree_tag_node_ppl.record_bind(cmdbuf);
                    self.octree_tag_node_ppl.record_bind_descriptor_sets(
                        cmdbuf,
                        std::slice::from_ref(&self.octree_tag_node_ds),
                        1,
                    );
                    self.octree_tag_node_ppl
                        .record_dispatch_indirect(cmdbuf, resources.voxel_count_indirect());

                    // if not last level
                    if i != voxel_level_count - 1 {
                        shader_access_pipeline_barrier.record_insert(device, cmdbuf);

                        self.octree_alloc_node_ppl.record_bind(cmdbuf);
                        self.octree_alloc_node_ppl.record_bind_descriptor_sets(
                            cmdbuf,
                            std::slice::from_ref(&self.octree_alloc_node_ds),
                            1,
                        );
                        self.octree_alloc_node_ppl
                            .record_dispatch_indirect(cmdbuf, resources.alloc_number_indirect());

                        shader_access_pipeline_barrier.record_insert(device, cmdbuf);

                        self.octree_modify_args_ppl.record_bind(cmdbuf);
                        self.octree_modify_args_ppl.record_bind_descriptor_sets(
                            cmdbuf,
                            std::slice::from_ref(&self.octree_modify_args_ds),
                            1,
                        );
                        self.octree_modify_args_ppl
                            .record_dispatch(cmdbuf, [1, 1, 1]);

                        shader_access_pipeline_barrier.record_insert(device, cmdbuf);
                        indirect_access_pipeline_barrier.record_insert(device, cmdbuf);
                    }
                }
            },
        );
    }
}

pub struct Builder {
    vulkan_context: VulkanContext,
    resources: BuilderResources,
    chunk_res: UVec3,
    chunks: HashMap<IVec3, Chunk>,

    chunk_init_builder: ChunkInitBuilder,
    frag_list_builder: FragListBuilder,
    octree_builder: OctreeBuilder,
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

        let descriptor_pool = DescriptorPool::a_big_one(vulkan_context.device()).unwrap();

        let resources = BuilderResources::new(
            vulkan_context.device().clone(),
            allocator.clone(),
            shader_compiler,
            chunk_res,
        );

        let chunk_init_builder = ChunkInitBuilder::new(
            &vulkan_context,
            shader_compiler,
            descriptor_pool.clone(),
            &resources,
        );

        let frag_list_builder = FragListBuilder::new(
            &vulkan_context,
            shader_compiler,
            descriptor_pool.clone(),
            &resources,
        );

        let octree_builder = OctreeBuilder::new(
            &vulkan_context,
            shader_compiler,
            descriptor_pool.clone(),
            &resources,
        );

        Self {
            vulkan_context,
            resources,
            chunk_res,
            chunks: HashMap::new(),
            chunk_init_builder,
            frag_list_builder,
            octree_builder,
        }
    }

    // previous benchmark results:
    // 14:14:38.672Z INFO  [re_flora::builder::builder] Average chunk init time: 3.806937ms
    // 14:14:38.673Z INFO  [re_flora::builder::builder] Average fragment list time: 1.147109ms
    // 14:14:38.673Z INFO  [re_flora::builder::builder] Average octree time: 1.006229ms

    pub fn init_chunk(&mut self, command_pool: &CommandPool, chunk_pos: IVec3) {
        // Chunk initialization
        self.chunk_init_builder.update_chunk_build_info_buf(
            &self.resources,
            self.chunk_res,
            chunk_pos,
        );
        self.chunk_init_builder.init_chunk_by_noise(
            &self.vulkan_context,
            command_pool,
            self.chunk_res,
        );

        // Fragment list building
        self.frag_list_builder
            .reset_fragment_list_info_buf(&self.resources);
        self.frag_list_builder.make_frag_list(
            &self.vulkan_context,
            command_pool,
            &self.chunk_init_builder.chunk_shared_ds,
            self.chunk_res,
        );

        // Octree building
        let fragment_list_len = self.frag_list_builder.get_fraglist_length(&self.resources);
        self.octree_builder.update_octree_build_info_buf(
            &self.resources,
            self.chunk_res,
            fragment_list_len,
        );
        self.octree_builder.make_octree_by_frag_list(
            &self.vulkan_context,
            command_pool,
            &self.resources,
            self.chunk_res,
        );

        let chunk = Chunk {
            res: self.chunk_res,
            pos: chunk_pos,
            data: vec![], // the data is not sent back to CPU for now
        };
        self.chunks.insert(chunk_pos, chunk);
    }

    pub fn get_octree_data(&self) -> &Buffer {
        self.resources.octree_data()
    }
}
