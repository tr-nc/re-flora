use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule, Texture, TextureDesc};
use ash::vk;
use glam::UVec3;

pub struct ContreeBuilderResources {
    pub frag_img: Texture,

    pub contree_data: Buffer,
    pub voxel_dim_indirect: Buffer,
    pub frag_img_maker_info: Buffer,
    pub frag_img_build_result: Buffer,

    pub contree_data_single: Buffer,
}

impl ContreeBuilderResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        voxel_dim_per_chunk: UVec3,
        contree_buffer_pool_size: u64,
        frag_img_init_buffers_sm: &ShaderModule,
        frag_img_maker_sm: &ShaderModule,
    ) -> Self {
        let frag_img_desc = TextureDesc {
            extent: voxel_dim_per_chunk.to_array(),
            format: vk::Format::R32_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let frag_img = Texture::new(device.clone(), allocator.clone(), &frag_img_desc, &sam_desc);

        let contree_data = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            contree_buffer_pool_size,
        );

        let voxel_dim_indirect_layout = frag_img_init_buffers_sm
            .get_buffer_layout("B_VoxelDimIndirect")
            .unwrap();
        let voxel_dim_indirect = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            voxel_dim_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let frag_img_maker_info_layout = frag_img_maker_sm
            .get_buffer_layout("U_FragImgMakerInfo")
            .unwrap();
        let frag_img_maker_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            frag_img_maker_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let frag_img_build_result = frag_img_init_buffers_sm
            .get_buffer_layout("B_FragImgBuildResult")
            .unwrap();
        let frag_img_build_result = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            frag_img_build_result.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let single_contree_buffer_size = 100 * 1024 * 1024; // 100 MB
        let contree_data_single = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            single_contree_buffer_size as _,
        );

        Self {
            frag_img,

            contree_data,

            voxel_dim_indirect,
            frag_img_maker_info,
            frag_img_build_result,

            contree_data_single,
        }
    }
}
