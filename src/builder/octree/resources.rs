use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule};
use ash::vk;
use glam::UVec3;

pub struct OctreeBuilderResources {
    pub frag_list: Buffer,

    pub octree_data: Buffer,
    pub voxel_dim_indirect: Buffer,
    pub frag_list_maker_info: Buffer,
    pub frag_list_build_result: Buffer,

    pub octree_build_info: Buffer,
    pub voxel_count_indirect: Buffer,
    pub alloc_number_indirect: Buffer,
    pub octree_alloc_info: Buffer,
    pub counter: Buffer,
    pub octree_build_result: Buffer,

    pub octree_data_single: Buffer,
}

impl OctreeBuilderResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        max_voxel_dim_per_chunk: UVec3,
        octree_buffer_pool_size: u64,
        frag_list_init_buffers_sm: &ShaderModule,
        frag_list_maker_sm: &ShaderModule,
        octree_init_buffers_sm: &ShaderModule,
    ) -> Self {
        let max_possible_voxel_count = (max_voxel_dim_per_chunk.x
            * max_voxel_dim_per_chunk.y
            * max_voxel_dim_per_chunk.z) as u64;
        let frag_list_buf_layout = frag_list_maker_sm
            .get_buffer_layout("B_FragmentList")
            .unwrap();
        let buf_size = frag_list_buf_layout.get_size_bytes() * max_possible_voxel_count;
        log::debug!("Fragment list buffer size: {} MB", buf_size / 1024 / 1024);

        // uninitialized for now, but is guaranteed to be filled by shader before use
        let frag_list = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            buf_size as _,
        );

        let octree_data = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            octree_buffer_pool_size,
        );

        let voxel_dim_indirect_layout = frag_list_init_buffers_sm
            .get_buffer_layout("B_VoxelDimIndirect")
            .unwrap();
        let voxel_dim_indirect = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            voxel_dim_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let frag_list_maker_info_layout = frag_list_maker_sm
            .get_buffer_layout("U_FragListMakerInfo")
            .unwrap();
        let frag_list_maker_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            frag_list_maker_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let frag_list_build_result = frag_list_init_buffers_sm
            .get_buffer_layout("B_FragListBuildResult")
            .unwrap();
        let frag_list_build_result = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            frag_list_build_result.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        //

        let octree_build_info_layout = octree_init_buffers_sm
            .get_buffer_layout("B_OctreeBuildInfo")
            .unwrap();
        let octree_build_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            octree_build_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let voxel_count_indirect_layout = octree_init_buffers_sm
            .get_buffer_layout("B_VoxelCountIndirect")
            .unwrap();
        let voxel_count_indirect = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            voxel_count_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let alloc_number_indirect_layout = octree_init_buffers_sm
            .get_buffer_layout("B_AllocNumberIndirect")
            .unwrap();
        let alloc_number_indirect = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            alloc_number_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let counter_layout = octree_init_buffers_sm
            .get_buffer_layout("B_Counter")
            .unwrap();
        let counter = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            counter_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let octree_alloc_info_layout = octree_init_buffers_sm
            .get_buffer_layout("B_OctreeAllocInfo")
            .unwrap();
        let octree_alloc_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            octree_alloc_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let octree_build_result_layout = octree_init_buffers_sm
            .get_buffer_layout("B_OctreeBuildResult")
            .unwrap();
        let octree_build_result = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            octree_build_result_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuToCpu,
        );

        let single_octree_buffer_size = 100 * 1024 * 1024; // 100 MB
        let octree_data_single = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            single_octree_buffer_size as _,
        );

        Self {
            frag_list,

            octree_data,

            voxel_dim_indirect,
            frag_list_maker_info,
            frag_list_build_result,

            octree_build_info,
            voxel_count_indirect,
            alloc_number_indirect,
            octree_alloc_info,
            counter,
            octree_build_result,

            octree_data_single,
        }
    }
}
