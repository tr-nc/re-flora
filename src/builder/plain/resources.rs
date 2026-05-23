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
    pub solid_workgroup_flags: Resource<Buffer>,

    pub region_info: Resource<Buffer>,
    pub region_indirect: Resource<Buffer>,
    pub heightmap: Resource<Buffer>,
    pub chunk_modify_info: Resource<Buffer>,
    pub chunk_solid_sample_info: Resource<Buffer>,
    pub chunk_solid_samples: Resource<Buffer>,
    pub edit_stats: Resource<Buffer>,
    pub edit_removal_candidates: Resource<Buffer>,
    pub edit_removal_sample: Resource<Buffer>,
    pub round_cones: Resource<Buffer>,
    pub cuboids: Resource<Buffer>,
    pub spheres: Resource<Buffer>,
    pub model_voxelize_info: Resource<Buffer>,
    pub model_triangles: Resource<Buffer>,
    pub trunk_bvh_nodes: Resource<Buffer>,
}

impl PlainBuilderResources {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &Device,
        allocator: Allocator,
        plain_atlas_dim: UVec3,
        free_atlas_dim: UVec3,
        buffer_setup_sm: &ShaderModule,
        chunk_modify_sm: &ShaderModule,
        chunk_modify_sample_sm: &ShaderModule,
        chunk_solid_sample_sm: &ShaderModule,
        model_voxelize_sm: &ShaderModule,
        heightmap_sm: &ShaderModule,
    ) -> Self {
        let tex_desc = ImageDesc {
            extent: Extent3D::new(plain_atlas_dim.x, plain_atlas_dim.y, plain_atlas_dim.z),
            format: vk::Format::R8_UINT,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC,
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

        let solid_workgroup_grid = (plain_atlas_dim + UVec3::splat(7)) / 8;
        let solid_workgroup_count = solid_workgroup_grid.x as u64
            * solid_workgroup_grid.y as u64
            * solid_workgroup_grid.z as u64;
        let solid_workgroup_flag_words = solid_workgroup_count.div_ceil(u32::BITS as u64);
        let solid_workgroup_flags = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            solid_workgroup_flag_words * std::mem::size_of::<u32>() as u64,
        );

        let chunk_modify_info_layout = chunk_modify_sm
            .get_buffer_layout("U_ChunkModifyInfo")
            .unwrap();
        let chunk_modify_info = Buffer::from_uniform_layout(
            device.clone(),
            allocator.clone(),
            chunk_modify_info_layout.clone(),
        );

        let chunk_solid_sample_info_layout = chunk_solid_sample_sm
            .get_buffer_layout("U_ChunkSolidSampleInfo")
            .unwrap();
        let chunk_solid_sample_info = Buffer::from_uniform_layout(
            device.clone(),
            allocator.clone(),
            chunk_solid_sample_info_layout.clone(),
        );

        let chunk_solid_samples = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuToCpu,
            std::mem::size_of::<u32>() as u64 * super::CHUNK_SOLID_SAMPLE_CAPACITY,
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
        let edit_removal_candidates = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_reflect_descriptor_type(
                edit_removal_candidates_layout.descriptor_type,
            ),
            gpu_allocator::MemoryLocation::CpuToGpu,
            std::mem::size_of::<[f32; 4]>() as u64 * super::EDIT_REMOVAL_CANDIDATE_CAPACITY,
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

        let model_voxelize_info_layout = model_voxelize_sm
            .get_buffer_layout("U_ModelVoxelizeInfo")
            .unwrap();
        let model_voxelize_info = Buffer::from_uniform_layout(
            device.clone(),
            allocator.clone(),
            model_voxelize_info_layout.clone(),
        );

        let model_triangles = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::CpuToGpu,
            std::mem::size_of::<[f32; 4]>() as u64 * 300000,
        );

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
            solid_workgroup_flags: Resource::new(solid_workgroup_flags),
            chunk_modify_info: Resource::new(chunk_modify_info),
            chunk_solid_sample_info: Resource::new(chunk_solid_sample_info),
            chunk_solid_samples: Resource::new(chunk_solid_samples),
            edit_stats: Resource::new(edit_stats),
            edit_removal_candidates: Resource::new(edit_removal_candidates),
            edit_removal_sample: Resource::new(edit_removal_sample),
            round_cones: Resource::new(round_cones),
            cuboids: Resource::new(cuboids),
            spheres: Resource::new(spheres),
            model_voxelize_info: Resource::new(model_voxelize_info),
            model_triangles: Resource::new(model_triangles),
            trunk_bvh_nodes: Resource::new(trunk_bvh_nodes),
            region_info: Resource::new(region_info),
            region_indirect: Resource::new(region_indirect),
            heightmap: Resource::new(heightmap),
        }
    }
}
