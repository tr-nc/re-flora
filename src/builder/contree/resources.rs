use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule, Texture, TextureDesc};
use ash::vk;
use glam::UVec3;

pub struct ContreeBuilderResources {
    pub frag_img: Texture,

    pub voxel_dim_indirect: Buffer,
    pub frag_img_maker_info: Buffer,
    pub frag_img_build_result: Buffer,
    pub contree_build_info: Buffer,
    pub contree_build_state: Buffer,
    pub level_dispatch_indirect: Buffer,
    pub concat_dispatch_indirect: Buffer,
    pub counter_for_levels: Buffer,
    pub node_offset_for_levels: Buffer,
    pub sparse_nodes: Buffer,
    pub dense_nodes: Buffer,
    pub leaf_data: Buffer,

    pub contree_data: Buffer,
    pub contree_build_result: Buffer,
}

impl ContreeBuilderResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        max_voxel_dim_per_chunk: UVec3,
        contree_buffer_pool_size: u64,
        frag_img_buffer_setup_sm: &ShaderModule,
        contree_buffer_setup_sm: &ShaderModule,
        leaf_write_sm: &ShaderModule,
        tree_write_sm: &ShaderModule,
        last_buffer_update_sm: &ShaderModule,
        buffer_concat: &ShaderModule,
    ) -> Self {
        let voxel_dim_indirect_layout = frag_img_buffer_setup_sm
            .get_buffer_layout("B_VoxelDimIndirect")
            .unwrap();
        let voxel_dim_indirect = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            voxel_dim_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let frag_img_maker_info_layout = frag_img_buffer_setup_sm
            .get_buffer_layout("U_FragImgMakerInfo")
            .unwrap();
        let frag_img_maker_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            frag_img_maker_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let frag_img_build_result = frag_img_buffer_setup_sm
            .get_buffer_layout("B_FragImgBuildResult")
            .unwrap();
        let frag_img_build_result = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            frag_img_build_result.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        // ---

        assert!(
            max_voxel_dim_per_chunk.x == max_voxel_dim_per_chunk.y
                && max_voxel_dim_per_chunk.x == max_voxel_dim_per_chunk.z,
            "ContreeBuilderResources: max_voxel_dim_per_chunk must be a cube"
        );
        assert!(is_power_of_four(max_voxel_dim_per_chunk.x));
        let max_level = log_4(max_voxel_dim_per_chunk.x) + 1;
        log::debug!(
            "max_voxel_dim_per_chunk: {:?}, max_level: {}",
            max_voxel_dim_per_chunk,
            max_level
        );

        // level 0 needs 64^0 (1) node, level 1 needs 64^1 (64) nodes, level 2 needs 64^2 (4096) nodes, etc.
        // notice that the leaf layer shouldn't be counted here, because it takes only 1 uint32
        // this limit is applied to both the sparse nodes and dense nodes
        let nodes_len_max: usize = {
            // 64.pow(max_level) may overflow u32 for large max_level, so do this in usize
            let total = 64usize.pow((max_level - 1) as u32);
            (total - 1) / 63
        };
        log::debug!("nodes_len_max: {}", nodes_len_max);

        let leaf_len_max = max_voxel_dim_per_chunk.x.pow(3) as u32;
        log::debug!("leaf_len_max: {}", leaf_len_max);

        let frag_img_desc = TextureDesc {
            extent: max_voxel_dim_per_chunk.to_array(),
            format: vk::Format::R32_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let frag_img = Texture::new(device.clone(), allocator.clone(), &frag_img_desc, &sam_desc);

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
            leaf_len_max as u64 * std::mem::size_of::<u32>() as u64,
        );

        let contree_data_layout = buffer_concat.get_buffer_layout("B_ContreeData").unwrap();
        let contree_data = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            contree_data_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
            contree_buffer_pool_size,
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
            frag_img,

            voxel_dim_indirect,
            frag_img_maker_info,
            frag_img_build_result,
            contree_build_info,
            contree_build_state,
            level_dispatch_indirect,
            concat_dispatch_indirect,
            counter_for_levels,
            node_offset_for_levels,
            sparse_nodes,
            dense_nodes,
            leaf_data,

            contree_data,
            contree_build_result,
        };

        /// Returns true if `n` is a power of four (1, 4, 16, 64, …).
        ///
        /// Uses two bit-tricks:
        /// 1. `n & (n - 1) == 0` ensures `n` is a power of two (only one bit set).
        /// 2. `0x5555_5555` has 1s in all even bit positions (0,2,4,…).
        ///    Masking with it ensures the single bit of `n` is in an even position.
        fn is_power_of_four(n: u32) -> bool {
            n != 0
        && (n & (n - 1)) == 0         // power of two?
        && (n & 0x5555_5555) != 0 // bit in an even position?
        }

        fn log_4(n: u32) -> u32 {
            // trailing_zeros gives 2*k, so divide by 2:
            n.trailing_zeros() / 2
        }
    }
}
