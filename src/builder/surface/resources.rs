use crate::vkn::{
    Allocator, Buffer, BufferUsage, Device, Extent3D, ImageDesc, ShaderModule, Texture,
};
use ash::vk;
use glam::UVec3;
use std::collections::HashMap;

pub struct ChunkRasterResources {
    pub chunk_id: UVec3,
    pub grass_instances: Buffer,
    pub grass_instances_len: u32,
}

impl ChunkRasterResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        make_surface_sm: &ShaderModule,
        chunk_id: UVec3,
        grass_instances_capacity: u64,
    ) -> Self {
        log::debug!("capacity: {}", grass_instances_capacity);
        let grass_instances_layout = make_surface_sm
            .get_buffer_layout("B_GrassInstances")
            .unwrap();
        let grass_instances = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            grass_instances_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            grass_instances_capacity,
        );
        Self {
            chunk_id,
            grass_instances,
            grass_instances_len: 0,
        }
    }
}

pub struct SurfaceResources {
    pub surface: Texture,
    pub make_surface_info: Buffer,
    pub make_surface_result: Buffer,
    pub chunk_raster_resources: HashMap<UVec3, ChunkRasterResources>,
}

impl SurfaceResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        voxel_dim_per_chunk: UVec3,
        make_surface_sm: &ShaderModule,
        chunk_dim: UVec3,
        grass_instances_capacity_per_chunk: u64,
    ) -> Self {
        let surface_desc = ImageDesc {
            extent: Extent3D::new(
                voxel_dim_per_chunk.x,
                voxel_dim_per_chunk.y,
                voxel_dim_per_chunk.z,
            ),
            format: vk::Format::R32_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let surface = Texture::new(device.clone(), allocator.clone(), &surface_desc, &sam_desc);

        let make_surface_info_layout = make_surface_sm
            .get_buffer_layout("U_MakeSurfaceInfo")
            .unwrap();
        let make_surface_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            make_surface_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let make_surface_result_layout = make_surface_sm
            .get_buffer_layout("B_MakeSurfaceResult")
            .unwrap();
        let make_surface_result = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            make_surface_result_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let mut chunk_raster_resources = HashMap::new();
        for x in 0..chunk_dim.x {
            for y in 0..chunk_dim.y {
                for z in 0..chunk_dim.z {
                    let chunk_offset = UVec3::new(x, y, z);
                    chunk_raster_resources.insert(
                        chunk_offset,
                        ChunkRasterResources::new(
                            device.clone(),
                            allocator.clone(),
                            make_surface_sm,
                            chunk_offset,
                            grass_instances_capacity_per_chunk,
                        ),
                    );
                }
            }
        }

        return Self {
            surface,
            make_surface_info,
            make_surface_result,
            chunk_raster_resources,
        };
    }
}
