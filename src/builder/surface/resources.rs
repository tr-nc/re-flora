use crate::{
    flora::species,
    geom::{Aabb3, UAabb3},
    resource::Resource,
};
use anyhow::Result;
use glam::{UVec3, Vec3};
use re_flora_vkn::vk;
use re_flora_vkn::{
    Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, DescriptorResource, Device, Extent3D,
    ImageDesc, MemoryLocation, ResourceContainer, ResourceLookup, ShaderModule, Texture,
    TextureLayout,
};
use resource_container_derive::ResourceContainer;
use std::{collections::HashMap, sync::Arc};

pub const MAX_FLORA_INSTANCES_PER_SPECIES: u32 = 40_000;

pub type Instance = crate::generated::gpu_structs::ManualFloraInstances;
pub type TreeLeafInstance = crate::generated::gpu_structs::TreeLeafInstances;
pub type TreeLeafShadowInstance = crate::generated::gpu_structs::TreeLeafShadowInstances;

pub struct InstanceResource {
    pub instances_buf: Resource<Buffer>,
}

pub struct TreeLeafInstanceResource {
    // Visible geometry and shadow proxies deliberately use separate streams: coarsening shadow
    // bandwidth must not change visible foliage geometry, LOD, or its compact instance stride.
    pub instances_buf: Resource<Buffer>,
    pub instances_len: u32,
    // Immutable publication identity: replacement allocates a new Arc, never mutates in place.
    pub response_instances: Arc<[TreeLeafInstance]>,
    pub shadow_instances_buf: Resource<Buffer>,
    pub shadow_instances_len: u32,
    pub shadow_response_sources: Resource<Buffer>,
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
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::TRANSFER_SRC,
            ),
            MemoryLocation::CpuToGpu,
            instance_size as u64 * max_instances,
        );

        Self {
            instances_buf: Resource::new(instances_buf),
        }
    }
}

impl TreeLeafInstanceResource {
    pub fn new(
        device: Device,
        allocator: Allocator,
        max_instances: u64,
        max_shadow_instances: u64,
    ) -> Self {
        let instance_size = std::mem::size_of::<TreeLeafInstance>();
        let instances_buf = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::VERTEX_BUFFER
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::CpuToGpu,
            instance_size as u64 * max_instances,
        );
        let shadow_instance_size = std::mem::size_of::<TreeLeafShadowInstance>();
        let shadow_instances_buf = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::CpuToGpu,
            shadow_instance_size as u64 * max_shadow_instances,
        );
        let shadow_response_sources = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            max_instances.max(1) * 8,
        );

        Self {
            instances_buf: Resource::new(instances_buf),
            instances_len: 0,
            response_instances: Arc::from([]),
            shadow_instances_buf: Resource::new(shadow_instances_buf),
            shadow_instances_len: 0,
            shadow_response_sources: Resource::new(shadow_response_sources),
        }
    }
}

pub struct TreeLeavesInstance {
    pub palette_srgb: Option<[Vec3; 2]>,
    pub appearance: crate::tree_gen::TreeAppearance,
    pub aabb: Aabb3,
    pub chunk_world_offset: UVec3,
    pub resources: TreeLeafInstanceResource,
}

impl TreeLeavesInstance {
    pub fn new_with_capacity(
        _tree_id: u32,
        aabb: Aabb3,
        chunk_world_offset: UVec3,
        device: Device,
        allocator: Allocator,
        max_instances: u64,
        max_shadow_instances: u64,
    ) -> Self {
        let resources = TreeLeafInstanceResource::new(
            device,
            allocator,
            max_instances.max(1),
            max_shadow_instances.max(1),
        );
        Self {
            palette_srgb: None,
            appearance: Default::default(),
            aabb,
            chunk_world_offset,
            resources,
        }
    }
}

pub struct FloraInstanceResources {
    pub(super) grass_audio: super::grass_audio::GrassAudioCache,
    /// Same species-major ordering as the authored draw streams. No ordinary grass state.
    pub authored_response_instances: Vec<super::AuthoredFloraInstance>,
    #[allow(dead_code)]
    pub chunk_id: UVec3,
    pub chunk_world_offset: UVec3,
    pub resource: InstanceResource,
    /// One four-bit ordinary-grass growth-potential level per local voxel (15 means unrestricted).
    pub grass_growth_potential_levels: Resource<Buffer>,
    /// CPU-authored field retained at the same publication boundary as its GPU upload.
    pub(super) grass_growth_potential_cpu: Vec<u32>,
    /// Committed tall/short draw streams in CPU-readable memory. GPU writers copy
    /// here in their existing submission; readers use the existing completion fence.
    pub(super) grass_instances_readback: Buffer,
    pub plant_hydration: Resource<Buffer>,
    /// Species-major live roots and their persistent water store. Reordered by
    /// root identity on publication, never by mutable draw index.
    pub hydration: Vec<Vec<(u32, f32)>>,
    pub hydration_roots: Vec<(u32, usize, usize)>,
    pub hydration_roots_dirty: bool,
    pub(super) soil_cover_roots: [std::collections::HashSet<u32>; 2],
    pub(super) soil_cover_changed_roots: std::collections::HashSet<u32>,
    pub root_water_reserve: std::collections::BTreeMap<u32, f64>,
    species_instance_len: [u32; species::MAX_FLORA_SPECIES],
}

impl ResourceContainer for FloraInstanceResources {
    fn resolve_resource(&self, name: &str) -> ResourceLookup<'_> {
        match name {
            "flora_instances" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.resource.instances_buf))
            }
            "plant_hydration_values" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.plant_hydration))
            }
            "grass_growth_potential_levels" => ResourceLookup::Unique(DescriptorResource::Buffer(
                &self.grass_growth_potential_levels,
            )),
            _ => ResourceLookup::Missing,
        }
    }
}

impl FloraInstanceResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        chunk_id: UVec3,
        voxel_dim_per_chunk: UVec3,
    ) -> Self {
        let species_count = species::species_count();
        let max_instances = u64::from(MAX_FLORA_INSTANCES_PER_SPECIES) * species_count as u64;
        let resource =
            InstanceResource::new(device.clone(), allocator.clone(), max_instances.max(1));
        let voxel_count = u64::from(voxel_dim_per_chunk.x)
            * u64::from(voxel_dim_per_chunk.y)
            * u64::from(voxel_dim_per_chunk.z);
        let grass_growth_potential_word_count = voxel_count.div_ceil(8);
        let plant_hydration = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            max_instances * 4,
        );
        plant_hydration
            .fill_range_with_raw_u8(
                0,
                bytemuck::cast_slice(&vec![
                    crate::flora::hydration::DEFAULT_HYDRATION;
                    max_instances as usize
                ]),
            )
            .expect("initialize plant hydration");
        let grass_instances_readback = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            u64::from(MAX_FLORA_INSTANCES_PER_SPECIES) * 2 * std::mem::size_of::<Instance>() as u64,
        );
        let grass_growth_potential_cpu = vec![u32::MAX; grass_growth_potential_word_count as usize];
        let grass_growth_potential_levels = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::CpuToGpu,
            grass_growth_potential_word_count * std::mem::size_of::<u32>() as u64,
        );
        grass_growth_potential_levels
            .fill_range_with_raw_u8(0, bytemuck::cast_slice(&grass_growth_potential_cpu))
            .expect("initialize grass growth-potential field");
        Self {
            chunk_id,
            grass_audio: Default::default(),
            authored_response_instances: Vec::new(),
            plant_hydration: Resource::new(plant_hydration),
            hydration: vec![Vec::new(); species_count],
            hydration_roots: Vec::new(),
            hydration_roots_dirty: true,
            soil_cover_roots: Default::default(),
            soil_cover_changed_roots: Default::default(),
            root_water_reserve: Default::default(),
            chunk_world_offset: chunk_id * voxel_dim_per_chunk,
            resource,
            grass_growth_potential_levels: Resource::new(grass_growth_potential_levels),
            grass_growth_potential_cpu,
            grass_instances_readback,
            species_instance_len: [0; species::MAX_FLORA_SPECIES],
        }
    }

    pub(super) fn publish_grass_growth_potential(&mut self, words: Vec<u32>) -> Result<()> {
        self.grass_growth_potential_levels
            .fill_with_raw_u8(bytemuck::cast_slice(&words))?;
        self.grass_growth_potential_cpu = words;
        self.grass_audio.shape_changed();
        Ok(())
    }

    /// Record after any GPU writer, before its existing fence. Structural edits
    /// can change counts on the GPU, so copy both bounded species capacities.
    pub(super) fn record_grass_readback(&self, cmdbuf: &CommandBuffer) {
        self.resource.instances_buf.record_copy_to_buffer(
            cmdbuf,
            &self.grass_instances_readback,
            self.grass_instances_readback.get_size_bytes(),
            0,
            0,
        );
        cmdbuf.use_buffer(&self.grass_instances_readback, BufferUse::HostRead);
    }

    pub(super) fn read_grass_instances(&self, species: usize) -> Result<Vec<Instance>> {
        anyhow::ensure!(species < 2, "readback contains only ordinary grass");
        anyhow::ensure!(
            self.grass_audio.pending_builds == 0,
            "grass stream is not committed"
        );
        let count = self.species_len(species);
        if count == 0 {
            return Ok(Vec::new());
        }
        let bytes = self.grass_instances_readback.read_back_range(
            u64::from(Self::species_offset(species)) * std::mem::size_of::<Instance>() as u64,
            u64::from(count) * std::mem::size_of::<Instance>() as u64,
        )?;
        Ok(bytes
            .chunks_exact(std::mem::size_of::<Instance>())
            .map(bytemuck::pod_read_unaligned)
            .collect())
    }

    pub fn species_offset(species_index: usize) -> u32 {
        assert!(
            species_index < species::MAX_FLORA_SPECIES,
            "flora species index {} exceeds max {}",
            species_index,
            species::MAX_FLORA_SPECIES
        );
        species_index as u32 * MAX_FLORA_INSTANCES_PER_SPECIES
    }

    pub fn species_len(&self, species_index: usize) -> u32 {
        self.species_instance_len[species_index]
    }

    pub fn set_species_len(&mut self, species_index: usize, len: u32) {
        self.species_instance_len[species_index] = len;
        if species_index < 2 {
            self.grass_audio.structure = self.grass_audio.structure.wrapping_add(1);
        }
    }

    /// Append one admitted grass root without reading or rewriting its neighbors.
    /// Draw count and CPU ownership change only after both hidden-tail uploads succeed.
    pub(super) fn try_append_grass(&mut self, species: usize, instance: Instance) -> Result<bool> {
        anyhow::ensure!(species < 2, "finite grass append requires ordinary grass");
        let root = instance.packed_local_pos & 0xffffff;
        let count = self.species_len(species);
        if count >= MAX_FLORA_INSTANCES_PER_SPECIES
            || self
                .soil_cover_roots
                .iter()
                .any(|roots| roots.contains(&root))
        {
            return Ok(false);
        }
        let index = u64::from(Self::species_offset(species) + count);
        let hydration = crate::flora::hydration::DEFAULT_HYDRATION;
        self.resource.instances_buf.fill_range_with_raw_u8(
            index * std::mem::size_of::<Instance>() as u64,
            bytemuck::bytes_of(&instance),
        )?;
        self.plant_hydration
            .fill_range_with_raw_u8(index * 4, bytemuck::bytes_of(&hydration))?;
        self.grass_instances_readback.fill_range_with_raw_u8(
            index * std::mem::size_of::<Instance>() as u64,
            bytemuck::bytes_of(&instance),
        )?;
        self.hydration[species].push((root, hydration));
        self.hydration_roots_dirty = true;
        self.soil_cover_roots[species].insert(root);
        self.soil_cover_changed_roots.insert(root);
        self.set_species_len(species, count + 1);
        Ok(true)
    }

    pub fn write_species_instances(
        &mut self,
        species_index: usize,
        instances: &[Instance],
    ) -> Result<()> {
        if species_index >= species::MAX_FLORA_SPECIES {
            return Err(anyhow::anyhow!(
                "flora species index {} exceeds max {}",
                species_index,
                species::MAX_FLORA_SPECIES
            ));
        }
        if instances.len() > MAX_FLORA_INSTANCES_PER_SPECIES as usize {
            return Err(anyhow::anyhow!(
                "authored flora species {} has {} instances, max is {}",
                species_index,
                instances.len(),
                MAX_FLORA_INSTANCES_PER_SPECIES
            ));
        }

        let instance_size = std::mem::size_of::<Instance>();
        let species_offset = Self::species_offset(species_index) as usize;
        let byte_start = species_offset * instance_size;
        let byte_capacity = MAX_FLORA_INSTANCES_PER_SPECIES as usize * instance_size;
        let buffer_size = self.resource.instances_buf.get_size_bytes() as usize;
        if byte_start + byte_capacity > buffer_size {
            return Err(anyhow::anyhow!(
                "flora species {} byte range [{}..{}) exceeds buffer size {}",
                species_index,
                byte_start,
                byte_start + byte_capacity,
                buffer_size
            ));
        }

        let instance_bytes = bytemuck::cast_slice(instances);
        if !instance_bytes.is_empty() {
            self.resource
                .instances_buf
                .fill_range_with_raw_u8(byte_start as u64, instance_bytes)?;
            if species_index < 2 {
                self.grass_instances_readback
                    .fill_range_with_raw_u8(byte_start as u64, instance_bytes)?;
            }
        }
        self.set_species_len(species_index, instances.len() as u32);
        self.reconcile_hydration(species_index, instances)?;
        Ok(())
    }

    pub fn reconcile_hydration(&mut self, species: usize, instances: &[Instance]) -> Result<()> {
        self.hydration_roots_dirty = true;
        if species < species::MODEL_FLOWER_FIRST_SPECIES as usize {
            let next: std::collections::HashSet<_> = instances
                .iter()
                .map(|i| i.packed_local_pos & 0xffffff)
                .collect();
            self.soil_cover_changed_roots.extend(
                self.soil_cover_roots[species]
                    .symmetric_difference(&next)
                    .copied(),
            );
            self.soil_cover_roots[species] = next;
        }
        let previous: HashMap<_, _> = self.hydration[species].iter().copied().collect();
        self.hydration[species] = instances
            .iter()
            .map(|i| {
                let root = i.packed_local_pos & 0xffffff;
                (
                    root,
                    previous
                        .get(&root)
                        .copied()
                        .unwrap_or(crate::flora::hydration::DEFAULT_HYDRATION),
                )
            })
            .collect();
        self.upload_hydration(species)
    }

    pub fn upload_hydration(&self, species: usize) -> Result<()> {
        if species < 2 {
            self.grass_audio.shape_changed();
        }
        let values: Vec<_> = self.hydration[species].iter().map(|p| p.1).collect();
        if !values.is_empty() {
            self.plant_hydration.fill_range_with_raw_u8(
                u64::from(Self::species_offset(species)) * 4,
                bytemuck::cast_slice(&values),
            )?;
        }
        Ok(())
    }

    /// Called after a GPU edit has compacted/rebuilt the ordinary grass stream.
    pub fn reconcile_grass_hydration(&mut self) -> Result<()> {
        for species in 0..species::MODEL_FLOWER_FIRST_SPECIES as usize {
            let instances = self.read_grass_instances(species)?;
            self.reconcile_hydration(species, &instances)?;
        }
        Ok(())
    }

    pub fn total_instance_len(&self) -> u32 {
        self.species_instance_len
            .iter()
            .fold(0_u32, |acc, len| acc.saturating_add(*len))
    }
}

pub struct InstanceResources {
    pub chunk_flora_instances: Vec<(Aabb3, FloraInstanceResources)>,
    pub leaves_instances: HashMap<u32, TreeLeavesInstance>,
    pub apple_instances: HashMap<u32, TreeLeavesInstance>,
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
            apple_instances: HashMap::new(),
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
    pub surface_active_brick_flags: Resource<Buffer>,
    pub surface_active_brick_indices: Resource<Buffer>,
    pub surface_solid_workgroup_indices: Resource<Buffer>,
    pub surface_solid_workgroup_dispatch_indirect: Resource<Buffer>,
    pub active_surface_flora_dispatch_indirect: Resource<Buffer>,
    pub clear_occupancy_info: Resource<Buffer>,
    pub instances_to_occupancy_info: Resource<Buffer>,
    pub edit_occupancy_info: Resource<Buffer>,
    pub occupancy_edit_dispatch: Resource<Buffer>,
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
        prepare_sparse_surface_dispatch_sm: &ShaderModule,
        clear_occupancy_sm: &ShaderModule,
        instances_to_occupancy_sm: &ShaderModule,
        edit_occupancy_sm: &ShaderModule,
        occupancy_to_instances_sm: &ShaderModule,
        update_flora_growth_sm: &ShaderModule,
        prepare_active_surface_flora_dispatch_sm: &ShaderModule,
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
            initial_layout: TextureLayout::UNDEFINED,
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
            initial_layout: TextureLayout::UNDEFINED,
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
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::CpuToGpu,
        );

        let active_brick_dim = voxel_dim_per_chunk / 4;
        let active_brick_count =
            active_brick_dim.x as u64 * active_brick_dim.y as u64 * active_brick_dim.z as u64;
        let active_brick_flag_count = active_brick_count.div_ceil(32);
        let surface_active_brick_flags = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            active_brick_flag_count * std::mem::size_of::<u32>() as u64,
        );

        let surface_active_brick_indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::GpuOnly,
            active_brick_count * std::mem::size_of::<u32>() as u64,
        );

        let solid_workgroup_dim = (voxel_dim_per_chunk + UVec3::splat(7)) / 8;
        let solid_workgroup_count = solid_workgroup_dim.x as u64
            * solid_workgroup_dim.y as u64
            * solid_workgroup_dim.z as u64;
        let surface_solid_workgroup_indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::GpuOnly,
            solid_workgroup_count * std::mem::size_of::<u32>() as u64,
        );

        let surface_solid_workgroup_dispatch_layout = prepare_sparse_surface_dispatch_sm
            .get_buffer_layout("B_SurfaceSolidWorkgroupDispatchIndirect")
            .unwrap();
        let surface_solid_workgroup_dispatch_indirect = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            surface_solid_workgroup_dispatch_layout.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::INDIRECT_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
        );

        let active_surface_flora_dispatch_layout = prepare_active_surface_flora_dispatch_sm
            .get_buffer_layout("B_ActiveSurfaceFloraDispatchIndirect")
            .unwrap();
        let active_surface_flora_dispatch_indirect = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            active_surface_flora_dispatch_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            MemoryLocation::GpuOnly,
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

        let occupancy_edit_dispatch = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            edit_occupancy_sm
                .get_buffer_layout("B_OccupancyEditDispatch")
                .unwrap()
                .clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::INDIRECT_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::CpuToGpu,
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
            .or_else(|_| update_flora_growth_sm.get_buffer_layout("B_OccupancyToInstancesResult"))
            .unwrap();
        let occupancy_to_instances_result = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            occupancy_to_instances_result_layout.clone(),
            BufferUsage::empty(),
            MemoryLocation::CpuToGpu,
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
            surface_active_brick_flags: Resource::new(surface_active_brick_flags),
            surface_active_brick_indices: Resource::new(surface_active_brick_indices),
            surface_solid_workgroup_indices: Resource::new(surface_solid_workgroup_indices),
            surface_solid_workgroup_dispatch_indirect: Resource::new(
                surface_solid_workgroup_dispatch_indirect,
            ),
            active_surface_flora_dispatch_indirect: Resource::new(
                active_surface_flora_dispatch_indirect,
            ),
            clear_occupancy_info: Resource::new(clear_occupancy_info),
            instances_to_occupancy_info: Resource::new(instances_to_occupancy_info),
            edit_occupancy_info: Resource::new(edit_occupancy_info),
            occupancy_edit_dispatch: Resource::new(occupancy_edit_dispatch),
            occupancy_to_instances_info: Resource::new(occupancy_to_instances_info),
            occupancy_to_instances_result: Resource::new(occupancy_to_instances_result),
            occupancy_data: Resource::new(occupancy_data),
            instances,
        }
    }
}
