use super::chunk_data_builder::ChunkDataBuilder;
use super::frag_list_builder::FragListBuilder;
use super::octree_builder::OctreeBuilder;
use super::Resources;
use crate::util::FirstFitAllocator;
use crate::util::ShaderCompiler;
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

    first_fit_allocator: FirstFitAllocator,
}

impl Builder {
    pub fn new(
        vulkan_context: VulkanContext,
        allocator: Allocator,
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

        let first_fit_allocator = FirstFitAllocator::new(octree_buffer_size);

        Self {
            vulkan_context,
            resources,
            voxel_dim,
            chunk_dim,
            chunk_data_builder,
            frag_list_builder,
            octree_builder,
            first_fit_allocator,
        }
    }

    // current benchmark results:
    // 14:14:38.672Z INFO  [re_flora::builder::builder] Average chunk init time: 3.806937ms
    // 14:14:38.673Z INFO  [re_flora::builder::builder] Average fragment list time: 951.318µs
    // 14:14:38.673Z INFO  [re_flora::builder::builder] Average octree time: 1.006229ms

    pub fn init_chunks(&mut self, command_pool: &CommandPool) {
        // first init raw chunk data
        for i in 0..self.chunk_dim.x {
            for j in 0..self.chunk_dim.y {
                for k in 0..self.chunk_dim.z {
                    let chunk_pos = IVec3::new(i as i32, j as i32, k as i32);
                    self.init_chunk_data(command_pool, chunk_pos);
                }
            }
        }

        // then init fragment list and octree
        for i in 0..self.chunk_dim.x {
            for j in 0..self.chunk_dim.y {
                for k in 0..self.chunk_dim.z {
                    let chunk_pos = IVec3::new(i as i32, j as i32, k as i32);
                    self.make_octree(command_pool, chunk_pos);
                }
            }
        }
    }

    fn init_chunk_data(&mut self, command_pool: &CommandPool, chunk_pos: IVec3) {
        self.chunk_data_builder
            .update_uniforms(&self.resources, self.voxel_dim, chunk_pos);
        self.chunk_data_builder.init_chunk_by_noise(
            &self.vulkan_context,
            command_pool,
            self.voxel_dim,
        );
    }

    fn make_octree(&mut self, command_pool: &CommandPool, chunk_pos: IVec3) {
        self.make_chunk_frag_list_by_raw_data(command_pool, chunk_pos);

        let fragment_list_size = self.frag_list_builder.get_fraglist_length(&self.resources);
        if fragment_list_size == 0 {
            return;
        }
        self.make_octree_by_frag_list(command_pool, fragment_list_size);
    }

    fn make_chunk_frag_list_by_raw_data(&mut self, command_pool: &CommandPool, chunk_pos: IVec3) {
        self.chunk_data_builder
            .update_frag_list_maker_info_buf(&self.resources, self.voxel_dim);

        /// idx ranges from 0-3 in three dimensions
        fn serialize(idx: UVec3) -> u32 {
            return idx.x + idx.y * 3 + idx.z * 9;
        }

        const NEIGHBOR_COUNT: usize = 3 * 3 * 3;
        let mut offsets: [u32; NEIGHBOR_COUNT] = [0; NEIGHBOR_COUNT];
        for i in -1..=1 {
            for j in -1..=1 {
                for k in -1..=1 {
                    let neighbor_pos = chunk_pos + IVec3::new(i, j, k);

                    let offset = if let Some(offset) =
                        self.chunk_data_builder.get_chunk_offset(neighbor_pos)
                    {
                        offset
                    } else {
                        0xFFFFFFFF
                    };

                    let serialized_idx =
                        serialize(UVec3::new((i + 1) as u32, (j + 1) as u32, (k + 1) as u32));
                    offsets[serialized_idx as usize] = offset;
                }
            }
        }

        self.resources
            .neighbor_info()
            .fill_with_raw_u32(&offsets)
            .unwrap();

        self.frag_list_builder
            .reset_fragment_list_info_buf(&self.resources);
        self.frag_list_builder
            .make_frag_list(&self.vulkan_context, command_pool, self.voxel_dim);
    }

    fn make_octree_by_frag_list(&mut self, command_pool: &CommandPool, frag_list_size: u32) {
        if frag_list_size == 0 {
            log::error!("Fragment list size is 0, and should be skipped");
            return;
        }

        self.octree_builder.update_octree_build_info_buf(
            &self.resources,
            self.voxel_dim,
            frag_list_size,
        );
        self.octree_builder.make_octree_by_frag_list(
            &self.vulkan_context,
            command_pool,
            &self.resources,
            self.voxel_dim,
        );
    }

    pub fn get_octree_data(&self) -> &Buffer {
        self.resources.octree_data()
    }
}
