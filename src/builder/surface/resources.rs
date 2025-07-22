use crate::{
    geom::{Aabb3, UAabb3},
    vkn::{Allocator, Buffer, BufferUsage, Device, Extent3D, ImageDesc, ShaderModule, Texture},
};
use ash::vk;
use glam::{UVec3, Vec3};
use std::collections::HashMap;

pub struct GrassInstanceResources {
    #[allow(dead_code)]
    pub chunk_id: UVec3,
    pub grass_instances: Buffer,
    pub grass_instances_len: u32,
}

impl GrassInstanceResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        make_surface_sm: &ShaderModule,
        chunk_id: UVec3,
        grass_instances_capacity: u64,
    ) -> Self {
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

pub struct InstanceResources {
    pub chunk_grass_instances: Vec<(Aabb3, GrassInstanceResources)>,
}

impl InstanceResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        make_surface_sm: &ShaderModule,
        chunk_dim: UAabb3,
        grass_instances_capacity_per_chunk: u64,
    ) -> Self {
        let mut chunk_grass_instances = Vec::new();
        for x in chunk_dim.min().x..chunk_dim.max().x {
            for y in chunk_dim.min().y..chunk_dim.max().y {
                for z in chunk_dim.min().z..chunk_dim.max().z {
                    let chunk_offset = UVec3::new(x, y, z);
                    let chunk_aabb = Self::compute_chunk_world_aabb(chunk_offset, 0.2); // Use grass sway margin
                    let grass_resources = GrassInstanceResources::new(
                        device.clone(),
                        allocator.clone(),
                        make_surface_sm,
                        chunk_offset,
                        grass_instances_capacity_per_chunk,
                    );
                    chunk_grass_instances.push((chunk_aabb, grass_resources));
                }
            }
        }

        Self {
            chunk_grass_instances,
        }
    }

    fn compute_chunk_world_aabb(chunk_id: UVec3, margin: f32) -> Aabb3 {
        let chunk_min = chunk_id.as_vec3();
        let chunk_max = chunk_min + Vec3::ONE;

        // add margin for grass swaying
        let min_with_margin = chunk_min - Vec3::splat(margin);
        let max_with_margin = chunk_max + Vec3::splat(margin);

        Aabb3::new(min_with_margin, max_with_margin)
    }
}

pub struct SurfaceResources {
    pub surface: Texture,
    pub make_surface_info: Buffer,
    pub make_surface_result: Buffer,
    pub instances: InstanceResources,
}

impl SurfaceResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        voxel_dim_per_chunk: UVec3,
        make_surface_sm: &ShaderModule,
        chunk_dim: UAabb3,
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

        let instances = InstanceResources::new(
            device.clone(),
            allocator.clone(),
            make_surface_sm,
            chunk_dim,
            grass_instances_capacity_per_chunk,
        );

        return Self {
            surface,
            make_surface_info,
            make_surface_result,
            instances,
        };
    }
}
