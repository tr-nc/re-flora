use super::chunk_data_builder::ChunkDataBuilder;
use super::frag_list_builder::FragListBuilder;
use super::octree_builder::OctreeBuilder;
use super::ExternalSharedResources;
use super::Resources;
use crate::util::ShaderCompiler;
use crate::util::Timer;
use crate::vkn::Allocator;
use crate::vkn::DescriptorPool;
use crate::vkn::VulkanContext;
use glam::UVec3;

pub struct Builder {
    vulkan_context: VulkanContext,
    resources: Resources,

    voxel_dim: UVec3,
    chunk_dim: UVec3,
    visible_chunk_dim: UVec3,

    chunk_data_builder: ChunkDataBuilder,
    frag_list_builder: FragListBuilder,
    octree_builder: OctreeBuilder,
}

impl Builder {
    pub fn new(
        vulkan_context: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        voxel_dim: UVec3,
        chunk_dim: UVec3,
        visible_chunk_dim: UVec3,
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
            chunk_dim,
            visible_chunk_dim,
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
            visible_chunk_dim,
            chunk_data_builder,
            frag_list_builder,
            octree_builder,
        }
    }

    // current benchmark results:
    // 14:14:38.672Z INFO  [re_flora::builder::builder] Average chunk init time: 4.794448ms
    // 14:14:38.673Z INFO  [re_flora::builder::builder] Average fragment + octree time: 1.551188ms

    pub fn init_chunks(&mut self) {
        let chunk_positions = {
            let mut positions = Vec::new();
            for i in 0..self.chunk_dim.x {
                for j in 0..self.chunk_dim.y {
                    for k in 0..self.chunk_dim.z {
                        positions.push(UVec3::new(i, j, k));
                    }
                }
            }
            positions
        };

        // first init raw chunk data
        let timer = Timer::new();
        for chunk_pos in chunk_positions.iter() {
            self.chunk_data_builder.build(
                &self.vulkan_context,
                &self.resources,
                self.voxel_dim,
                *chunk_pos,
            );
        }
        log::debug!(
            "Average chunk init time: {:?}",
            timer.elapsed() / (self.chunk_dim.x * self.chunk_dim.y * self.chunk_dim.z)
        );

        // then init fragment list and octree
        let timer = Timer::new();
        for chunk_pos in chunk_positions.iter() {
            self.build_octree(*chunk_pos);
        }
        log::debug!(
            "Average octree time: {:?}",
            timer.elapsed() / (self.chunk_dim.x * self.chunk_dim.y * self.chunk_dim.z)
        );

        self.octree_builder.update_octree_offset_atlas(
            &self.vulkan_context,
            &self.resources,
            self.visible_chunk_dim,
        );
    }

    fn build_octree(&mut self, chunk_pos: UVec3) {
        self.frag_list_builder
            .build(&self.vulkan_context, &self.resources, chunk_pos);

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
            &self.resources,
            fragment_list_len,
            chunk_pos,
            self.voxel_dim,
        );
    }

    pub fn get_external_shared_resources(&self) -> &ExternalSharedResources {
        &self.resources.external_shared_resources
    }
}
