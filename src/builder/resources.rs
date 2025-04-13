use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule, Texture, TextureDesc};
use ash::vk;
use glam::UVec3;

pub struct BuilderResources {
    pub raw_voxels_tex: Texture,
    pub raw_voxels: Buffer,
    pub chunk_build_info: Buffer,
    pub fragment_list_info: Buffer,
    pub octree_build_info: Buffer,
    pub voxel_count_indirect: Buffer,
    pub alloc_number_indirect: Buffer,
    pub octree_alloc_info: Buffer,
    pub counter: Buffer,
    pub octree_build_result: Buffer,
    pub octree_data: Buffer,
    pub fragment_list: Buffer,
}

impl BuilderResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        chunk_init_sm: &ShaderModule,
        frag_list_maker_sm: &ShaderModule,
        octree_init_buffers_sm: &ShaderModule,
        chunk_res: UVec3,
    ) -> Self {
        let raw_voxels_tex = Self::create_weight_tex(device.clone(), allocator.clone(), chunk_res);

        let raw_voxels_size: u64 = chunk_res.x as u64 * chunk_res.y as u64 * chunk_res.z as u64;
        let raw_voxels = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            raw_voxels_size,
        );

        let chunk_build_info_layout = chunk_init_sm.get_buffer_layout("U_ChunkBuildInfo").unwrap();
        let chunk_build_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            chunk_build_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let fragment_list_info_layout = frag_list_maker_sm
            .get_buffer_layout("B_FragmentListInfo")
            .unwrap();
        let fragment_list_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            fragment_list_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let octree_build_info_layout = octree_init_buffers_sm
            .get_buffer_layout("B_OctreeBuildInfo")
            .unwrap();
        let octree_build_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            octree_build_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let voxel_count_indirect_layout = octree_init_buffers_sm
            .get_buffer_layout("B_VoxelCountIndirect")
            .unwrap();
        let voxel_count_indirect = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            voxel_count_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let alloc_number_indirect_layout = octree_init_buffers_sm
            .get_buffer_layout("B_AllocNumberIndirect")
            .unwrap();
        let alloc_number_indirect = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            alloc_number_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let counter_layout = octree_init_buffers_sm
            .get_buffer_layout("B_Counter")
            .unwrap();
        let counter = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            counter_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let octree_alloc_info_layout = octree_init_buffers_sm
            .get_buffer_layout("B_OctreeAllocInfo")
            .unwrap();
        let octree_alloc_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            octree_alloc_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let octree_build_result_layout = octree_init_buffers_sm
            .get_buffer_layout("B_OctreeBuildResult")
            .unwrap();
        let octree_build_result = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            octree_build_result_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let max_possible_voxel_count = chunk_res.x * chunk_res.y * chunk_res.z;
        let fragment_list_buf_layout = frag_list_maker_sm
            .get_buffer_layout("B_FragmentList")
            .unwrap();
        let buf_size = fragment_list_buf_layout.get_size() * max_possible_voxel_count;
        log::debug!("Fragment list buffer size: {} MB", buf_size / 1024 / 1024);

        // uninitialized for now, but is guarenteed to be filled by shader before use
        let fragment_list = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            buf_size as _,
        );

        let one_giga = 1 * 1024 * 1024 * 1024;
        log::debug!(
            "Octree data buffer size: {} GB",
            one_giga / 1024 / 1024 / 1024
        );
        let octree_data = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            one_giga as _,
        );

        Self {
            raw_voxels_tex,
            raw_voxels,
            chunk_build_info,
            fragment_list_info,
            octree_build_info,
            voxel_count_indirect,
            alloc_number_indirect,
            counter,
            octree_alloc_info,
            octree_build_result,
            octree_data,
            fragment_list,
        }
    }

    fn create_weight_tex(device: Device, allocator: Allocator, chunk_res: UVec3) -> Texture {
        let tex_desc = TextureDesc {
            extent: chunk_res.to_array(),
            format: vk::Format::R8_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }
}
