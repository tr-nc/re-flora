use crate::vkn::{Allocator, Buffer, Device, ShaderModule, Texture, TextureDesc};
use ash::vk;
use glam::UVec3;

pub struct BuilderResources {
    pub blocks_tex: Texture,
    pub chunk_build_info: Buffer,
    pub fragment_list_info: Buffer,
    pub octree_build_info: Buffer,
    pub voxel_count_indirect: Buffer,
    pub alloc_number_indirect: Buffer,
    pub counter: Buffer,
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
        let blocks_tex = Self::create_weight_tex(device.clone(), allocator.clone(), chunk_res);

        let chunk_build_info_layout = chunk_init_sm.get_buffer_layout("U_ChunkBuildInfo").unwrap();
        let chunk_build_info = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            gpu_allocator::MemoryLocation::CpuToGpu,
            chunk_build_info_layout.get_size() as _,
        );

        let fragment_list_info_layout = frag_list_maker_sm
            .get_buffer_layout("B_FragmentListInfo")
            .unwrap();
        let fragment_list_info = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::STORAGE_BUFFER,
            gpu_allocator::MemoryLocation::CpuToGpu,
            fragment_list_info_layout.get_size() as _,
        );

        let octree_build_info_layout = octree_init_buffers_sm
            .get_buffer_layout("B_OctreeBuildInfo")
            .unwrap();
        let octree_build_info = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::STORAGE_BUFFER,
            gpu_allocator::MemoryLocation::GpuOnly,
            octree_build_info_layout.get_size() as _,
        );

        let voxel_count_indirect_layout = octree_init_buffers_sm
            .get_buffer_layout("B_VoxelCountIndirect")
            .unwrap();
        let voxel_count_indirect = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER,
            gpu_allocator::MemoryLocation::GpuOnly,
            voxel_count_indirect_layout.get_size() as _,
        );

        let alloc_number_indirect_layout = octree_init_buffers_sm
            .get_buffer_layout("B_AllocNumberIndirect")
            .unwrap();
        let alloc_number_indirect = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER,
            gpu_allocator::MemoryLocation::GpuOnly,
            alloc_number_indirect_layout.get_size() as _,
        );

        let counter_layout = octree_init_buffers_sm
            .get_buffer_layout("B_Counter")
            .unwrap();
        let counter = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::STORAGE_BUFFER,
            gpu_allocator::MemoryLocation::GpuOnly,
            counter_layout.get_size() as _,
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
            vk::BufferUsageFlags::STORAGE_BUFFER,
            gpu_allocator::MemoryLocation::GpuOnly,
            buf_size as _,
        );

        Self {
            blocks_tex,
            chunk_build_info,
            fragment_list_info,
            octree_build_info,
            voxel_count_indirect,
            alloc_number_indirect,
            counter,
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
