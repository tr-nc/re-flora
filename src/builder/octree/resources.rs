use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule, Texture, TextureDesc};
use ash::vk;
use glam::UVec3;

pub struct Resources {
    pub fragment_list: Buffer,

    pub octree_data: Buffer,
    pub octree_offset_atlas_tex: Texture,
    pub scene_bvh_nodes: Buffer,

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

impl Resources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        voxel_dim: UVec3,
        visible_chunk_dim: UVec3,
        octree_buffer_size: u64,
        frag_init_buffers_sm: &ShaderModule,
        frag_list_maker_sm: &ShaderModule,
        octree_init_buffers_sm: &ShaderModule,
        tracer_sm: &ShaderModule,
    ) -> Self {
        let max_possible_voxel_count = (voxel_dim.x * voxel_dim.y * voxel_dim.z) as u64;
        let fragment_list_buf_layout = frag_list_maker_sm
            .get_buffer_layout("B_FragmentList")
            .unwrap();
        let buf_size = fragment_list_buf_layout.get_size_bytes() * max_possible_voxel_count;
        log::debug!("Fragment list buffer size: {} MB", buf_size / 1024 / 1024);

        // uninitialized for now, but is guaranteed to be filled by shader before use
        let fragment_list = Buffer::new_sized(
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
            octree_buffer_size,
        );

        let octree_offset_atlas_tex_desc = TextureDesc {
            extent: visible_chunk_dim.to_array(),
            format: vk::Format::R32_UINT, // TODO: maybe extend this into 64 bit later for more octree data
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let octree_offset_atlas_tex = Texture::new(
            device.clone(),
            allocator.clone(),
            &octree_offset_atlas_tex_desc,
            &Default::default(),
        );

        let scene_bvh_nodes_layout = tracer_sm.get_buffer_layout("B_BvhNodes").unwrap();
        let scene_bvh_nodes = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            scene_bvh_nodes_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::CpuToGpu,
            10000,
        ); // less than 1 MB though, don't worry about the size

        let voxel_dim_indirect_layout = frag_init_buffers_sm
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

        let frag_list_build_result = frag_init_buffers_sm
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
            fragment_list,

            octree_data,
            octree_offset_atlas_tex,
            scene_bvh_nodes,

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
