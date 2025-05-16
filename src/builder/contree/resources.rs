use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule};
use ash::vk;
use glam::UVec3;

pub struct ContreeBuilderResources {
    pub contree_build_info: Buffer,
    pub contree_build_state: Buffer,
    pub level_dispatch_indirect: Buffer,
    pub concat_dispatch_indirect: Buffer,
    pub counter_for_levels: Buffer,
    pub node_offset_for_levels: Buffer,
    pub sparse_nodes: Buffer,
    pub dense_nodes: Buffer,
    pub leaf_data: Buffer,

    pub node_data: Buffer,
    pub contree_build_result: Buffer,
}

impl ContreeBuilderResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        max_voxel_dim_per_chunk: UVec3,
        node_pool_size_in_bytes: u64,
        leaf_pool_size_in_bytes: u64,
        contree_buffer_setup_sm: &ShaderModule,
        leaf_write_sm: &ShaderModule,
        tree_write_sm: &ShaderModule,
        last_buffer_update_sm: &ShaderModule,
    ) -> Self {
        let contree_build_info_layout = contree_buffer_setup_sm
            .get_buffer_layout("U_ContreeBuildInfo")
            .unwrap();
        let contree_build_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            contree_build_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let contree_build_state_layout = contree_buffer_setup_sm
            .get_buffer_layout("B_ContreeBuildState")
            .unwrap();
        let contree_build_state = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            contree_build_state_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let level_dispatch_indirect_layout = contree_buffer_setup_sm
            .get_buffer_layout("B_LevelDispatchIndirect")
            .unwrap();
        let level_dispatch_indirect = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            level_dispatch_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let concat_dispatch_indirect_layout = last_buffer_update_sm
            .get_buffer_layout("B_ConcatDispatchIndirect")
            .unwrap();
        let concat_dispatch_indirect = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            concat_dispatch_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let max_level = log_4(max_voxel_dim_per_chunk.x) + 1;

        // level 0 needs 64^0 (1) node, level 1 needs 64^1 (64) nodes, level 2 needs 64^2 (4096) nodes, etc.
        // notice that the leaf layer shouldn't be counted here, because it takes only 1 uint32
        // this limit is applied to both the sparse nodes and dense nodes
        let nodes_len_max: usize = {
            // 64.pow(max_level) may overflow u32 for large max_level, so do this in usize
            let total = 64usize.pow((max_level - 1) as u32);
            (total - 1) / 63
        };

        let counter_for_levels = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            (max_level - 1) as u64 * std::mem::size_of::<u32>() as u64,
        );

        let node_offset_for_levels = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            (max_level - 1) as u64 * std::mem::size_of::<u32>() as u64,
        );

        let sparse_nodes_layout = leaf_write_sm.get_buffer_layout("B_SparseNodes").unwrap();
        let sparse_nodes = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            sparse_nodes_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
            nodes_len_max as u64,
        );

        let dense_nodes_layout = tree_write_sm.get_buffer_layout("B_DenseNodes").unwrap();
        let dense_nodes = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            dense_nodes_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
            nodes_len_max as u64,
        );

        let leaf_data = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            leaf_pool_size_in_bytes,
        );

        // let node_data_layout = buffer_concat.get_buffer_layout("B_NodeData").unwrap();
        // let contree_data = Buffer::from_buffer_layout_arraylike(
        //     device.clone(),
        //     allocator.clone(),
        //     node_data_layout.clone(),
        //     BufferUsage::empty(),
        //     gpu_allocator::MemoryLocation::GpuOnly,
        //     node_pool_size_in_bytes,
        // );

        let node_data = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            node_pool_size_in_bytes,
        );

        let contree_build_result_layout = contree_buffer_setup_sm
            .get_buffer_layout("B_ContreeBuildResult")
            .unwrap();
        let contree_build_result = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            contree_build_result_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuToCpu,
        );

        return Self {
            contree_build_info,
            contree_build_state,
            level_dispatch_indirect,
            concat_dispatch_indirect,
            counter_for_levels,
            node_offset_for_levels,
            sparse_nodes,
            dense_nodes,
            leaf_data,

            node_data,
            contree_build_result,
        };

        fn log_4(n: u32) -> u32 {
            // trailing_zeros gives 2*k, so divide by 2:
            n.trailing_zeros() / 2
        }
    }
}
