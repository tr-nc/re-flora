use crate::{
    resource::Resource,
    vkn::{Allocator, Buffer, BufferUsage, Device, Extent3D, ImageDesc, ShaderModule, Texture},
};
use ash::vk;
use glam::UVec3;
use resource_container_derive::ResourceContainer;

#[derive(ResourceContainer)]
pub struct PlainBuilderResources {
    pub chunk_atlas: Resource<Texture>,
    pub free_atlas: Resource<Texture>,

    pub region_info: Resource<Buffer>,
    pub region_indirect: Resource<Buffer>,
    pub heightmap: Resource<Buffer>,
    pub chunk_modify_info: Resource<Buffer>,
    pub edit_stats: Resource<Buffer>,
    pub edit_removal_candidates: Resource<Buffer>,
    pub edit_removal_sample: Resource<Buffer>,
    pub round_cones: Resource<Buffer>,
    pub cuboids: Resource<Buffer>,
    pub spheres: Resource<Buffer>,
    pub trunk_bvh_nodes: Resource<Buffer>,
}

impl PlainBuilderResources {
    pub fn new(
        device: &Device,
        allocator: Allocator,
        plain_atlas_dim: UVec3,
        free_atlas_dim: UVec3,
        buffer_setup_sm: &ShaderModule,
        chunk_modify_sm: &ShaderModule,
        chunk_modify_sample_sm: &ShaderModule,
        heightmap_sm: &ShaderModule,
    ) -> Self {
        let tex_desc = ImageDesc {
            extent: Extent3D::new(plain_atlas_dim.x, plain_atlas_dim.y, plain_atlas_dim.z),
            format: vk::Format::R8_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let chunk_atlas = Texture::new(device.clone(), allocator.clone(), &tex_desc, &sam_desc);

        let free_atlas_tex_desc = ImageDesc {
            extent: Extent3D::new(free_atlas_dim.x, free_atlas_dim.y, free_atlas_dim.z),
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
        let chunk_modify_info = Buffer::from_uniform_layout(
            device.clone(),
            allocator.clone(),
            chunk_modify_info_layout.clone(),
        );

        let edit_stats_layout = chunk_modify_sm.get_buffer_layout("B_EditStats").unwrap();
        let edit_stats = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            edit_stats_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let edit_removal_candidates_layout = chunk_modify_sm
            .get_buffer_layout("B_EditRemovalCandidates")
            .unwrap();
        let edit_removal_candidates = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            edit_removal_candidates_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
            super::EDIT_REMOVAL_CANDIDATE_CAPACITY,
        );

        let edit_removal_sample_layout = chunk_modify_sample_sm
            .get_buffer_layout("B_EditRemovalSample")
            .unwrap();
        let edit_removal_sample = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            edit_removal_sample_layout.clone(),
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
            100000,
        ); // less than 1 MB though, don't worry about the size

        let trunk_bvh_nodes_layout = chunk_modify_sm.get_buffer_layout("B_BvhNodes").unwrap();
        let trunk_bvh_nodes = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            trunk_bvh_nodes_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
            100000,
        ); // less than 1 MB though, don't worry about the size

        let cuboids_layout = chunk_modify_sm.get_buffer_layout("B_Cuboids").unwrap();
        let cuboids = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            cuboids_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
            100000,
        ); // less than 1 MB though, don't worry about the size

        let spheres_layout = chunk_modify_sm.get_buffer_layout("B_Spheres").unwrap();
        let spheres = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            spheres_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
            100000,
        ); // less than 1 MB though, don't worry about the size

        let region_info_layout = buffer_setup_sm.get_buffer_layout("U_RegionInfo").unwrap();
        let region_info = Buffer::from_uniform_layout(
            device.clone(),
            allocator.clone(),
            region_info_layout.clone(),
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

        let heightmap_layout = heightmap_sm.get_buffer_layout("B_Heightmap").unwrap();
        let heightmap_entry_count = (plain_atlas_dim.x * plain_atlas_dim.z) as usize;
        let heightmap = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            heightmap_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
            heightmap_entry_count as u64,
        );

        Self {
            chunk_atlas: Resource::new(chunk_atlas),
            free_atlas: Resource::new(free_atlas),
            chunk_modify_info: Resource::new(chunk_modify_info),
            edit_stats: Resource::new(edit_stats),
            edit_removal_candidates: Resource::new(edit_removal_candidates),
            edit_removal_sample: Resource::new(edit_removal_sample),
            round_cones: Resource::new(round_cones),
            cuboids: Resource::new(cuboids),
            spheres: Resource::new(spheres),
            trunk_bvh_nodes: Resource::new(trunk_bvh_nodes),
            region_info: Resource::new(region_info),
            region_indirect: Resource::new(region_indirect),
            heightmap: Resource::new(heightmap),
        }
    }
}
