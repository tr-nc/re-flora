use crate::{
    flora::species,
    geom::{Aabb3, UAabb3},
    resource::Resource,
    vkn::{Allocator, Buffer, BufferUsage, Device, Extent3D, ImageDesc, ShaderModule, Texture},
};
use ash::vk;
use glam::{UVec3, Vec3};
use resource_container_derive::ResourceContainer;
use std::collections::HashMap;

const MAX_FLORA_INSTANCES_PER_SPECIES: u64 = 40_000;

pub type Instance = crate::generated::gpu_structs::ManualFloraInstances;

pub struct InstanceResource {
    pub instances_buf: Resource<Buffer>,
    pub instances_len: u32,
}

impl InstanceResource {
    pub fn new(device: Device, allocator: Allocator, max_instances: u64) -> Self {
        let instance_size = std::mem::size_of::<Instance>();
        let instances_buf = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(
                vk::BufferUsageFlags::VERTEX_BUFFER
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            gpu_allocator::MemoryLocation::CpuToGpu,
            instance_size as u64 * max_instances,
        );

        Self {
            instances_buf: Resource::new(instances_buf),
            instances_len: 0,
        }
    }
}

pub struct TreeLeavesInstance {
    pub aabb: Aabb3,
    pub chunk_world_offset: UVec3,
    pub resources: InstanceResource,
}

impl TreeLeavesInstance {
    pub fn new(
        _tree_id: u32,
        aabb: Aabb3,
        chunk_world_offset: UVec3,
        device: Device,
        allocator: Allocator,
    ) -> Self {
        let resources = InstanceResource::new(device, allocator, 10000);
        Self {
            aabb,
            chunk_world_offset,
            resources,
        }
    }
}

pub struct FloraInstanceResources {
    #[allow(dead_code)]
    pub chunk_id: UVec3,
    pub chunk_world_offset: UVec3,
    pub resources: Vec<InstanceResource>,
}

impl FloraInstanceResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        chunk_id: UVec3,
        voxel_dim_per_chunk: UVec3,
    ) -> Self {
        let species_count = species::species_count();
        let mut resources = Vec::with_capacity(species_count);
        for _ in 0..species_count {
            resources.push(InstanceResource::new(
                device.clone(),
                allocator.clone(),
                MAX_FLORA_INSTANCES_PER_SPECIES,
            ));
        }
        Self {
            chunk_id,
            chunk_world_offset: chunk_id * voxel_dim_per_chunk,
            resources,
        }
    }

    pub fn get(&self, species_index: usize) -> &InstanceResource {
        &self.resources[species_index]
    }

    pub fn get_mut(&mut self, species_index: usize) -> &mut InstanceResource {
        &mut self.resources[species_index]
    }

    pub fn iter(&self) -> impl Iterator<Item = &InstanceResource> {
        self.resources.iter()
    }

    #[allow(dead_code)]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut InstanceResource> {
        self.resources.iter_mut()
    }
}

pub struct InstanceResources {
    pub chunk_flora_instances: Vec<(Aabb3, FloraInstanceResources)>,
    pub leaves_instances: HashMap<u32, TreeLeavesInstance>,
}

impl InstanceResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        chunk_dim: UAabb3,
        voxel_dim_per_chunk: UVec3,
    ) -> Self {
        /// A margin is added becaues the boundary grasses can sway out of the chunk to a certain extent.
        fn compute_chunk_world_aabb(chunk_id: UVec3, margin: f32) -> Aabb3 {
            let chunk_min = chunk_id.as_vec3();
            let chunk_max = chunk_min + Vec3::ONE;

            // add margin for grass swaying
            let min_with_margin = chunk_min - Vec3::splat(margin);
            let max_with_margin = chunk_max + Vec3::splat(margin);

            Aabb3::new(min_with_margin, max_with_margin)
        }

        let mut chunk_flora_instances = Vec::new();
        for x in chunk_dim.min().x..chunk_dim.max().x {
            for y in chunk_dim.min().y..chunk_dim.max().y {
                for z in chunk_dim.min().z..chunk_dim.max().z {
                    let chunk_offset = UVec3::new(x, y, z);
                    let chunk_aabb = compute_chunk_world_aabb(chunk_offset, 0.2);
                    let flora_resources = FloraInstanceResources::new(
                        device.clone(),
                        allocator.clone(),
                        chunk_offset,
                        voxel_dim_per_chunk,
                    );
                    chunk_flora_instances.push((chunk_aabb, flora_resources));
                }
            }
        }

        Self {
            chunk_flora_instances,
            leaves_instances: HashMap::new(),
        }
    }

    /// A margin is added to cover the leaf radius.
    /// We don't input the actual leaf radius just for simplicity.
    pub fn compute_leaves_aabb(leaf_positions: &[Vec3], margin: f32) -> Aabb3 {
        if leaf_positions.is_empty() {
            return Aabb3::new(Vec3::ZERO, Vec3::ZERO);
        }
        let aabb = Aabb3::from_points(leaf_positions);

        // add margin to cover leaf radius
        let min_with_margin = aabb.min() - Vec3::splat(margin);
        let max_with_margin = aabb.max() + Vec3::splat(margin);

        Aabb3::new(min_with_margin, max_with_margin)
    }
}

#[derive(ResourceContainer)]
pub struct SurfaceResources {
    pub surface: Resource<Texture>,
    pub make_surface_info: Resource<Buffer>,
    pub make_surface_result: Resource<Buffer>,
    pub clear_occupancy_info: Resource<Buffer>,
    pub instances_to_occupancy_info: Resource<Buffer>,
    pub edit_occupancy_info: Resource<Buffer>,
    pub occupancy_to_instances_info: Resource<Buffer>,
    pub occupancy_to_instances_result: Resource<Buffer>,
    pub occupancy_data: Resource<Texture>,
    pub instances: InstanceResources,
}

#[allow(clippy::too_many_arguments)]
impl SurfaceResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        voxel_dim_per_chunk: UVec3,
        make_surface_sm: &ShaderModule,
        clear_occupancy_sm: &ShaderModule,
        instances_to_occupancy_sm: &ShaderModule,
        edit_occupancy_sm: &ShaderModule,
        occupancy_to_instances_sm: &ShaderModule,
        chunk_dim: UAabb3,
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

        let occupancy_desc = ImageDesc {
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
        let occupancy_data = Texture::new(
            device.clone(),
            allocator.clone(),
            &occupancy_desc,
            &sam_desc,
        );

        let make_surface_info_layout = make_surface_sm
            .get_buffer_layout("U_MakeSurfaceInfo")
            .unwrap();
        let make_surface_info = Buffer::from_uniform_layout(
            device.clone(),
            allocator.clone(),
            make_surface_info_layout.clone(),
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

        let clear_occupancy_info_layout = clear_occupancy_sm
            .get_buffer_layout("U_ClearOccupancyInfo")
            .unwrap();
        let clear_occupancy_info = Buffer::from_uniform_layout(
            device.clone(),
            allocator.clone(),
            clear_occupancy_info_layout.clone(),
        );

        let instances_to_occupancy_info_layout = instances_to_occupancy_sm
            .get_buffer_layout("U_InstancesToOccupancyInfo")
            .unwrap();
        let instances_to_occupancy_info = Buffer::from_uniform_layout(
            device.clone(),
            allocator.clone(),
            instances_to_occupancy_info_layout.clone(),
        );

        let edit_occupancy_info_layout = edit_occupancy_sm
            .get_buffer_layout("U_EditOccupancyInfo")
            .unwrap();
        let edit_occupancy_info = Buffer::from_uniform_layout(
            device.clone(),
            allocator.clone(),
            edit_occupancy_info_layout.clone(),
        );

        let occupancy_to_instances_info_layout = occupancy_to_instances_sm
            .get_buffer_layout("U_OccupancyToInstancesInfo")
            .unwrap();
        let occupancy_to_instances_info = Buffer::from_uniform_layout(
            device.clone(),
            allocator.clone(),
            occupancy_to_instances_info_layout.clone(),
        );

        let occupancy_to_instances_result_layout = occupancy_to_instances_sm
            .get_buffer_layout("B_OccupancyToInstancesResult")
            .unwrap();
        let occupancy_to_instances_result = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            occupancy_to_instances_result_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let instances = InstanceResources::new(
            device.clone(),
            allocator.clone(),
            chunk_dim,
            voxel_dim_per_chunk,
        );

        Self {
            surface: Resource::new(surface),
            make_surface_info: Resource::new(make_surface_info),
            make_surface_result: Resource::new(make_surface_result),
            clear_occupancy_info: Resource::new(clear_occupancy_info),
            instances_to_occupancy_info: Resource::new(instances_to_occupancy_info),
            edit_occupancy_info: Resource::new(edit_occupancy_info),
            occupancy_to_instances_info: Resource::new(occupancy_to_instances_info),
            occupancy_to_instances_result: Resource::new(occupancy_to_instances_result),
            occupancy_data: Resource::new(occupancy_data),
            instances,
        }
    }
}
