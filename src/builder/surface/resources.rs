use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule, Texture, ImageDesc};
use ash::vk;
use glam::UVec3;

pub struct SurfaceResources {
    pub surface: Texture,
    pub make_surface_info: Buffer,
    pub voxel_dim_indirect: Buffer,
    pub make_surface_result: Buffer,
    pub grass_instances: Buffer,
}

impl SurfaceResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        max_voxel_dim_per_chunk: UVec3,
        grass_instances_pool_len: u64,
        buffer_setup: &ShaderModule,
        make_surface: &ShaderModule,
    ) -> Self {
        let surface_desc = ImageDesc {
            extent: max_voxel_dim_per_chunk.to_array(),
            format: vk::Format::R32_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let surface = Texture::new(device.clone(), allocator.clone(), &surface_desc, &sam_desc);

        let voxel_dim_indirect_layout = buffer_setup
            .get_buffer_layout("B_VoxelDimIndirect")
            .unwrap();
        let voxel_dim_indirect = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            voxel_dim_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let make_surface_info_layout = buffer_setup.get_buffer_layout("U_MakeSurfaceInfo").unwrap();
        let make_surface_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            make_surface_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let make_surface_result_layout = buffer_setup
            .get_buffer_layout("B_MakeSurfaceResult")
            .unwrap();
        let make_surface_result = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            make_surface_result_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let grass_instances_layout = make_surface.get_buffer_layout("B_GrassInstances").unwrap();
        let grass_instances = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            grass_instances_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
            grass_instances_pool_len,
        );

        return Self {
            surface,
            make_surface_info,
            voxel_dim_indirect,
            make_surface_result,
            grass_instances,
        };
    }
}
