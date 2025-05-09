use ash::vk;
use glam::UVec3;

use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule, Texture, TextureDesc};

pub struct PlainBuilderResources {
    pub chunk_atlas: Texture,
    pub free_atlas: Texture,

    pub region_info: Buffer,
    pub region_indirect: Buffer,
    pub chunk_modify_info: Buffer,
    pub leaf_write_info: Buffer,
    pub round_cones: Buffer,
    pub trunk_bvh_nodes: Buffer,
}

impl PlainBuilderResources {
    pub fn new(
        device: &Device,
        allocator: Allocator,
        plain_atlas_dim: UVec3,
        free_atlas_dim: UVec3,
        buffer_setup_sm: &ShaderModule,
        chunk_modify_sm: &ShaderModule,
        leaf_write_sm: &ShaderModule,
    ) -> Self {
        let tex_desc = TextureDesc {
            extent: plain_atlas_dim.to_array(),
            format: vk::Format::R8_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let chunk_atlas = Texture::new(device.clone(), allocator.clone(), &tex_desc, &sam_desc);

        let free_atlas_tex_desc = TextureDesc {
            extent: free_atlas_dim.to_array(),
            format: vk::Format::R8_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let free_atlas = Texture::new(
            device.clone(),
            allocator.clone(),
            &free_atlas_tex_desc,
            &Default::default(),
        );

        let chunk_modify_info_layout = chunk_modify_sm
            .get_buffer_layout("U_ChunkModifyInfo")
            .unwrap();
        let chunk_modify_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            chunk_modify_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let leaf_write_info_layout = leaf_write_sm.get_buffer_layout("U_LeafWriteInfo").unwrap();
        let leaf_write_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            leaf_write_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let round_cones_layout = chunk_modify_sm.get_buffer_layout("B_RoundCones").unwrap();
        let round_cones = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            round_cones_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
            10000,
        ); // less than 1 MB though, don't worry about the size

        let trunk_bvh_nodes_layout = chunk_modify_sm.get_buffer_layout("B_BvhNodes").unwrap();
        let trunk_bvh_nodes = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            trunk_bvh_nodes_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
            10000,
        ); // less than 1 MB though, don't worry about the size

        let region_info_layout = buffer_setup_sm.get_buffer_layout("U_RegionInfo").unwrap();
        let region_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            region_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let region_indirect_layout = buffer_setup_sm
            .get_buffer_layout("B_RegionIndirect")
            .unwrap();
        let region_indirect = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            region_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        return Self {
            chunk_atlas,
            free_atlas,
            chunk_modify_info,
            leaf_write_info,
            round_cones,
            trunk_bvh_nodes,
            region_info,
            region_indirect,
        };
    }
}
