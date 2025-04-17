use super::chunk_data_builder::ChunkDataBuilder;
use super::frag_list_builder::FragListBuilder;
use super::octree_builder::OctreeBuilder;
use super::Resources;
use crate::util::ShaderCompiler;
use crate::util::Timer;
use crate::vkn::Allocator;
use crate::vkn::Buffer;
use crate::vkn::CommandPool;
use crate::vkn::DescriptorPool;
use crate::vkn::VulkanContext;
use glam::IVec3;
use glam::UVec3;

pub struct Builder {
    vulkan_context: VulkanContext,
    resources: Resources,

    voxel_dim: UVec3,
    chunk_dim: UVec3,

    chunk_data_builder: ChunkDataBuilder,
    frag_list_builder: FragListBuilder,
    octree_builder: OctreeBuilder,
}

impl Builder {
    pub fn new(
        vulkan_context: VulkanContext,
        allocator: Allocator,
        command_pool: &CommandPool,
        shader_compiler: &ShaderCompiler,
        voxel_dim: UVec3,
        chunk_dim: UVec3,
        octree_buffer_size: u64,
    ) -> Self {
        if voxel_dim.x != voxel_dim.y || voxel_dim.y != voxel_dim.z {
            log::error!("Dimension must be equal in all dimensions");
        }
        if voxel_dim.x & (voxel_dim.x - 1) != 0 {
            log::error!("Dimension must be a power of 2");
        }

        let descriptor_pool = DescriptorPool::a_big_one(vulkan_context.device()).unwrap();

        let resources = Resources::new(
            vulkan_context.device().clone(),
            allocator.clone(),
            shader_compiler,
            voxel_dim,
            chunk_dim.x * chunk_dim.y * chunk_dim.z,
            octree_buffer_size,
        );

        let chunk_data_builder = ChunkDataBuilder::new(
            &vulkan_context,
            shader_compiler,
            descriptor_pool.clone(),
            &resources,
        );

        let frag_list_builder = FragListBuilder::new(
            &vulkan_context,
            command_pool,
            shader_compiler,
            descriptor_pool.clone(),
            &resources,
        );

        let octree_builder = OctreeBuilder::new(
            &vulkan_context,
            shader_compiler,
            descriptor_pool.clone(),
            &resources,
            octree_buffer_size,
        );

        Self {
            vulkan_context,
            resources,
            voxel_dim,
            chunk_dim,
            chunk_data_builder,
            frag_list_builder,
            octree_builder,
        }
    }

    // current benchmark results:
    // 14:14:38.672Z INFO  [re_flora::builder::builder] Average chunk init time: 3.806937ms
    // 14:14:38.673Z INFO  [re_flora::builder::builder] Average fragment list time: 951.318µs
    // 14:14:38.673Z INFO  [re_flora::builder::builder] Average octree time: 325.893µs

    pub fn init_chunks(&mut self, command_pool: &CommandPool) {
        // first init raw chunk data
        for i in 0..self.chunk_dim.x {
            for j in 0..self.chunk_dim.y {
                for k in 0..self.chunk_dim.z {
                    let chunk_pos = IVec3::new(i as i32, j as i32, k as i32);
                    self.chunk_data_builder.build(
                        &self.vulkan_context,
                        command_pool,
                        &self.resources,
                        self.voxel_dim,
                        chunk_pos,
                    );
                }
            }
        }

        // then init fragment list and octree
        for i in 0..self.chunk_dim.x {
            for j in 0..self.chunk_dim.y {
                for k in 0..self.chunk_dim.z {
                    let chunk_pos = IVec3::new(i as i32, j as i32, k as i32);
                    self.build_octree(command_pool, chunk_pos);
                }
            }
        }

        // benchmark
        let benchmark_chunk_pos = IVec3::new(1, 0, 1);
        let timer = Timer::new();
        const BUILD_TIMES: u32 = 1000;
        for i in 0..BUILD_TIMES {
            self.frag_list_builder.build(
                &self.vulkan_context,
                &self.resources,
                benchmark_chunk_pos,
                self.chunk_data_builder.get_offset_table(),
            );
        }
        log::debug!(
            "Average fragment list time: {:?}",
            timer.elapsed() / BUILD_TIMES
        );

        self.build_octree(command_pool, benchmark_chunk_pos);
    }

    fn build_octree(&mut self, command_pool: &CommandPool, chunk_pos: IVec3) {
        self.frag_list_builder.build(
            &self.vulkan_context,
            &self.resources,
            chunk_pos,
            self.chunk_data_builder.get_offset_table(),
        );

        let fragment_list_len = self.frag_list_builder.get_fraglist_length(&self.resources);
        if fragment_list_len == 0 {
            log::debug!("Fragment list for chunk {:?} is empty", chunk_pos);
            return;
        } else {
            log::debug!(
                "Fragment list for chunk {:?} has {} fragments",
                chunk_pos,
                fragment_list_len
            );
        }

        self.octree_builder.build(
            &self.vulkan_context,
            command_pool,
            &self.resources,
            fragment_list_len,
            self.voxel_dim,
        );
    }

    pub fn get_octree_data(&self) -> &Buffer {
        self.resources.octree_data()
    }
}
