use crate::{
    flora::species,
    geom::UAabb3,
    particles::{BUTTERFLY_ATLAS_ROW_FOR_VIEW, PARTICLE_CAPACITY, PARTICLE_SPRITE_FRAME_DIM},
    resource::Resource,
    tracer::{
        leaves_construct::{
            generate_indexed_single_voxel_leaf, generate_indexed_voxel_apple,
            generate_voxel_leaf_shape, LeafVoxelShape, DEFAULT_LEAF_INNER_DENSITY,
            DEFAULT_LEAF_INNER_RADIUS, DEFAULT_LEAF_OUTER_DENSITY, DEFAULT_LEAF_OUTER_RADIUS,
        },
        load_butterfly_and_remap,
        voxel_encoding::{
            encode_lookup_pos_key, FloraMeshData, FloraVoxelInfo, FloraVoxelInfoEntry,
            FLORA_VOXEL_LOOKUP_EMPTY_KEY,
        },
        ButterflyPalettePreset, DenoiserResources, ExtentDependentResources, ParticleTextureLayout,
        Vertex, WIND_VOLUME_BUCKET_COUNT,
    },
    util::get_project_root,
};
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, UVec3, Vec3};
use resource_container_derive::ResourceContainer;
use std::{collections::HashMap, path::Path};
use verdarium_vkn::vk;
use verdarium_vkn::{
    Allocator, Buffer, BufferUsage, CurrentPrevious, Device, Extent2D, Extent3D, ImageDesc,
    MemoryLocation, SamplerDesc, ShaderModule, Texture, TextureLayout, TextureRegion,
    VulkanContext,
};

type MeshGenerator = fn(bool) -> anyhow::Result<FloraMeshData>;

pub const WIND_VOLUME_TEXELS_PER_CHUNK: UVec3 = UVec3::splat(10);

#[derive(ResourceContainer)]
pub struct FloraMeshResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
}

impl FloraMeshResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        is_lod_used: bool,
        generator: MeshGenerator,
    ) -> Self {
        let mesh_data = generator(is_lod_used).unwrap();
        Self::from_mesh_data(device, allocator, mesh_data)
    }

    pub fn from_mesh_data(device: Device, allocator: Allocator, mesh_data: FloraMeshData) -> Self {
        Self::from_data(device, allocator, mesh_data.vertices, mesh_data.indices)
    }

    pub fn from_data(
        device: Device,
        allocator: Allocator,
        vertices_data: Vec<Vertex>,
        indices_data: Vec<u32>,
    ) -> Self {
        let indices_len = indices_data.len() as u32;

        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<Vertex>() * vertices_data.len()) as u64,
        );
        vertices.fill(&vertices_data).unwrap();

        let indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<u32>() * indices_data.len()) as u64,
        );
        indices.fill(&indices_data).unwrap();

        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len,
        }
    }
}

const FLORA_VOXEL_LOOKUP_TYPE_COUNT: usize = species::APPLE_RENDER_SPECIES_INDEX as usize + 1;
const FLORA_VOXEL_LOOKUP_ENTRY_CAPACITY: usize = 1 << 21;
const FLORA_VOXEL_LOOKUP_MAX_PROBES: u32 = 64;
const FLORA_VOXEL_LOOKUP_LOAD_FACTOR: usize = 4;

#[derive(Clone, Debug)]
pub struct FloraVoxelLookupTypeData {
    pub entries: Vec<FloraVoxelInfoEntry>,
    pub max_length: u32,
    pub fallback_info: FloraVoxelInfo,
}

impl FloraVoxelLookupTypeData {
    pub fn new(entries: Vec<FloraVoxelInfoEntry>, max_length: u32) -> Self {
        Self {
            entries,
            max_length: max_length.max(1),
            fallback_info: FloraVoxelInfo::fallback(),
        }
    }

    pub fn from_mesh_data(mesh_data: &FloraMeshData) -> Self {
        Self::new(mesh_data.voxel_infos.clone(), mesh_data.max_length)
    }

    pub fn from_leaf_shape(shape: &LeafVoxelShape) -> Self {
        let max_length = shape.max_length.max(1);
        let entries = shape
            .offsets
            .iter()
            .copied()
            .map(|pos| {
                let gradient = (pos.as_vec3().length() / max_length as f32).clamp(0.0, 1.0);
                FloraVoxelInfoEntry {
                    pos,
                    info: FloraVoxelInfo::gradient(gradient),
                }
            })
            .collect();
        Self::new(entries, max_length)
    }
}

#[derive(ResourceContainer)]
pub struct FloraVoxelLookupResources {
    pub flora_voxel_table_descs: Resource<Buffer>,
    pub flora_voxel_infos: Resource<Buffer>,
    type_data: Vec<FloraVoxelLookupTypeData>,
}

impl FloraVoxelLookupResources {
    fn new(device: Device, allocator: Allocator) -> Self {
        let flora_voxel_table_descs = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<[u32; 4]>() * FLORA_VOXEL_LOOKUP_TYPE_COUNT) as u64,
        );
        let flora_voxel_infos = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<[u32; 2]>() * FLORA_VOXEL_LOOKUP_ENTRY_CAPACITY) as u64,
        );
        let mut resources = Self {
            flora_voxel_table_descs: Resource::new(flora_voxel_table_descs),
            flora_voxel_infos: Resource::new(flora_voxel_infos),
            type_data: Self::default_type_data().unwrap(),
        };
        resources.upload().unwrap();
        resources
    }

    pub fn update_type(
        &mut self,
        type_index: usize,
        type_data: FloraVoxelLookupTypeData,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            type_index < self.type_data.len(),
            "flora voxel lookup type index {} exceeds type count {}",
            type_index,
            self.type_data.len()
        );
        self.type_data[type_index] = type_data;
        self.upload()
    }

    fn default_type_data() -> anyhow::Result<Vec<FloraVoxelLookupTypeData>> {
        let mut data =
            vec![FloraVoxelLookupTypeData::new(Vec::new(), 1); FLORA_VOXEL_LOOKUP_TYPE_COUNT];
        for (species_index, desc) in species::species().iter().enumerate() {
            let mesh_data = (desc.mesh_generator)(false)?;
            data[species_index] = FloraVoxelLookupTypeData::from_mesh_data(&mesh_data);
        }

        let leaf_shape = generate_voxel_leaf_shape(
            DEFAULT_LEAF_INNER_DENSITY,
            DEFAULT_LEAF_OUTER_DENSITY,
            DEFAULT_LEAF_INNER_RADIUS,
            DEFAULT_LEAF_OUTER_RADIUS,
        )?;
        data[species::TREE_LEAF_RENDER_SPECIES_INDEX as usize] =
            FloraVoxelLookupTypeData::from_leaf_shape(&leaf_shape);

        let apple_mesh = generate_indexed_voxel_apple(false)?;
        data[species::APPLE_RENDER_SPECIES_INDEX as usize] =
            FloraVoxelLookupTypeData::from_mesh_data(&apple_mesh);

        Ok(data)
    }

    fn upload(&mut self) -> anyhow::Result<()> {
        let mut descs = vec![[0_u32; 4]; FLORA_VOXEL_LOOKUP_TYPE_COUNT];
        let mut entries = vec![
            [
                FLORA_VOXEL_LOOKUP_EMPTY_KEY,
                FloraVoxelInfo::fallback().packed
            ];
            FLORA_VOXEL_LOOKUP_ENTRY_CAPACITY
        ];
        let mut next_offset = 0_usize;

        for (type_index, type_data) in self.type_data.iter().enumerate() {
            let table = build_lookup_table(type_data)?;
            anyhow::ensure!(
                next_offset + table.len() <= FLORA_VOXEL_LOOKUP_ENTRY_CAPACITY,
                "flora voxel lookup tables need {} entries, but capacity is {}",
                next_offset + table.len(),
                FLORA_VOXEL_LOOKUP_ENTRY_CAPACITY
            );

            let offset = next_offset;
            let capacity = table.len();
            if capacity > 0 {
                entries[offset..offset + capacity].copy_from_slice(&table);
            }
            descs[type_index] = [
                offset as u32,
                capacity as u32,
                type_data.fallback_info.packed,
                type_data.max_length.max(1),
            ];
            next_offset += capacity;
        }

        self.flora_voxel_table_descs.fill(&descs)?;
        self.flora_voxel_infos.fill(&entries)?;
        Ok(())
    }
}

fn build_lookup_table(type_data: &FloraVoxelLookupTypeData) -> anyhow::Result<Vec<[u32; 2]>> {
    let mut unique = HashMap::<u32, u32>::new();
    for entry in &type_data.entries {
        let key = encode_lookup_pos_key(entry.pos)?;
        unique.insert(key, entry.info.packed);
    }

    let unique_len = unique.len();
    let mut capacity = unique_len
        .saturating_mul(FLORA_VOXEL_LOOKUP_LOAD_FACTOR)
        .max(1)
        .next_power_of_two();

    loop {
        let mut table = vec![
            [
                FLORA_VOXEL_LOOKUP_EMPTY_KEY,
                FloraVoxelInfo::fallback().packed
            ];
            capacity
        ];
        let mask = capacity as u32 - 1;
        let mut max_probe = 0_u32;
        let mut failed_probe_budget = false;

        for (&key, &info) in &unique {
            let start_slot = flora_lookup_hash(key) & mask;
            let mut inserted = false;
            for probe in 0..FLORA_VOXEL_LOOKUP_MAX_PROBES {
                let slot = ((start_slot + probe) & mask) as usize;
                if table[slot][0] == FLORA_VOXEL_LOOKUP_EMPTY_KEY || table[slot][0] == key {
                    table[slot] = [key, info];
                    max_probe = max_probe.max(probe);
                    inserted = true;
                    break;
                }
            }
            if !inserted {
                failed_probe_budget = true;
                break;
            }
        }

        if !failed_probe_budget && max_probe < FLORA_VOXEL_LOOKUP_MAX_PROBES {
            return Ok(table);
        }
        capacity = capacity
            .checked_mul(2)
            .ok_or_else(|| anyhow::anyhow!("flora voxel lookup table capacity overflow"))?;
    }
}

fn flora_lookup_hash(key: u32) -> u32 {
    let mut x = key ^ 0x9E37_79B9;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}

#[derive(ResourceContainer)]
pub struct LeavesResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
}

impl LeavesResources {
    pub fn new(device: Device, allocator: Allocator, is_lod_used: bool) -> Self {
        // use default parameters for initial leaf generation
        Self::new_with_params(
            device,
            allocator,
            DEFAULT_LEAF_INNER_DENSITY,
            DEFAULT_LEAF_OUTER_DENSITY,
            DEFAULT_LEAF_INNER_RADIUS,
            DEFAULT_LEAF_OUTER_RADIUS,
            is_lod_used,
        )
    }

    pub fn new_with_params(
        device: Device,
        allocator: Allocator,
        inner_density: f32,
        outer_density: f32,
        inner_radius: f32,
        outer_radius: f32,
        is_lod_used: bool,
    ) -> Self {
        // 1. Tree leaves keep the historical hollow-sphere offsets, but those offsets are now
        // uploaded per voxel as instances. The shared mesh is therefore a single voxel whose
        // gradient length matches the configured leaf-shell radius.
        let shape =
            generate_voxel_leaf_shape(inner_density, outer_density, inner_radius, outer_radius)
                .unwrap();
        let mesh_data = generate_indexed_single_voxel_leaf(shape.max_length, is_lod_used).unwrap();
        let (mut vertices_data, mut indices_data) = mesh_data.into_render_data();

        // guard against empty data - create minimal buffers to avoid Vulkan validation errors
        if vertices_data.is_empty() {
            vertices_data.push(Vertex { packed_data: 0 }); // Dummy vertex
        }
        if indices_data.is_empty() {
            indices_data.push(0); // Dummy index
        }

        let indices_len = if indices_data.len() == 1 && indices_data[0] == 0 {
            0 // Don't render anything if this was a dummy index
        } else {
            indices_data.len() as u32
        };

        // 2. Create and fill the vertex buffer.
        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<Vertex>() * vertices_data.len()) as u64,
        );
        vertices.fill(&vertices_data).unwrap();

        // 3. Create and fill the index buffer.
        let indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<u32>() * indices_data.len()) as u64,
        );
        indices.fill(&indices_data).unwrap();

        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ParticleInstanceGpu {
    pub position: [f32; 3],
    pub size: f32,
    pub color: [f32; 4],
    pub tex_index: u32,
}

pub struct ParticleRendererResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
    pub instance_buffer: Resource<Buffer>,
    pub instance_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GlassVertex {
    pub position_ws: [f32; 3],
    pub uv: [f32; 2],
    pub normal_ws: [f32; 3],
    pub face_id: u32,
    pub part_kind: u32,
}

pub struct GlassMeshResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
    pub pane_index_starts: [u32; 4],
    pub pane_index_count: u32,
    pub edge_index_start: u32,
    pub edge_indices_len: u32,
    pub box_min: Vec3,
    pub box_max: Vec3,
}

pub const TERRARIUM_GLASS_TOP_PADDING_WORLD: f32 = 0.08;

impl GlassMeshResources {
    const PART_PANE: u32 = 0;
    const PART_EDGE_BAND: u32 = 1;
    const PART_RIM: u32 = 2;
    const PART_CORNER_BEVEL: u32 = 3;
    const VOXELS_PER_CHUNK_AXIS: f32 = 256.0;
    const GLASS_THICKNESS_VOXELS: f32 = 2.0;

    pub fn new(device: Device, allocator: Allocator, chunk_bound: UAabb3) -> Self {
        let extent = chunk_bound.get_extent();
        let glass_thickness_world = Self::GLASS_THICKNESS_VOXELS / Self::VOXELS_PER_CHUNK_AXIS;
        let inset = glass_thickness_world;
        let top_padding = TERRARIUM_GLASS_TOP_PADDING_WORLD;
        let box_min = Vec3::new(-inset, 0.0, -inset);
        let box_max = Vec3::new(
            extent.width as f32 + inset,
            extent.height as f32 + top_padding,
            extent.depth as f32 + inset,
        );

        let mut vertices_data = Vec::with_capacity(160);
        let mut indices_data = Vec::with_capacity(240);
        let mut pane_index_starts = [0u32; 4];

        let min = box_min;
        let max = box_max;
        let faces = [
            (
                0u32,
                Vec3::new(-1.0, 0.0, 0.0),
                [
                    Vec3::new(min.x, min.y, max.z),
                    Vec3::new(min.x, min.y, min.z),
                    Vec3::new(min.x, max.y, min.z),
                    Vec3::new(min.x, max.y, max.z),
                ],
            ),
            (
                1u32,
                Vec3::new(1.0, 0.0, 0.0),
                [
                    Vec3::new(max.x, min.y, min.z),
                    Vec3::new(max.x, min.y, max.z),
                    Vec3::new(max.x, max.y, max.z),
                    Vec3::new(max.x, max.y, min.z),
                ],
            ),
            (
                2u32,
                Vec3::new(0.0, 0.0, -1.0),
                [
                    Vec3::new(min.x, min.y, min.z),
                    Vec3::new(max.x, min.y, min.z),
                    Vec3::new(max.x, max.y, min.z),
                    Vec3::new(min.x, max.y, min.z),
                ],
            ),
            (
                3u32,
                Vec3::new(0.0, 0.0, 1.0),
                [
                    Vec3::new(max.x, min.y, max.z),
                    Vec3::new(min.x, min.y, max.z),
                    Vec3::new(min.x, max.y, max.z),
                    Vec3::new(max.x, max.y, max.z),
                ],
            ),
        ];

        for (face_id, normal, corners) in faces {
            pane_index_starts[face_id as usize] = indices_data.len() as u32;
            Self::append_quad(
                &mut vertices_data,
                &mut indices_data,
                face_id,
                Self::PART_PANE,
                normal,
                corners,
                Self::full_face_uvs(),
            );
        }

        let edge_index_start = indices_data.len() as u32;
        let edge_uv_width = glass_thickness_world / (box_max.x - box_min.x);
        let bevel_width = glass_thickness_world;
        for (face_id, normal, corners) in faces {
            Self::append_face_edge_bands(
                &mut vertices_data,
                &mut indices_data,
                face_id,
                normal,
                corners,
                edge_uv_width,
            );
        }
        Self::append_corner_bevels(
            &mut vertices_data,
            &mut indices_data,
            box_min,
            box_max,
            bevel_width,
        );
        let edge_indices_len = indices_data.len() as u32 - edge_index_start;

        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<GlassVertex>() * vertices_data.len()) as u64,
        );
        vertices.fill(&vertices_data).unwrap();

        let indices = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<u32>() * indices_data.len()) as u64,
        );
        indices.fill(&indices_data).unwrap();

        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len: indices_data.len() as u32,
            pane_index_starts,
            pane_index_count: 6,
            edge_index_start,
            edge_indices_len,
            box_min,
            box_max,
        }
    }

    fn full_face_uvs() -> [[f32; 2]; 4] {
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }

    fn append_quad(
        vertices_data: &mut Vec<GlassVertex>,
        indices_data: &mut Vec<u32>,
        face_id: u32,
        part_kind: u32,
        normal: Vec3,
        corners: [Vec3; 4],
        uvs: [[f32; 2]; 4],
    ) {
        let base = vertices_data.len() as u32;
        let normal_ws = normal.normalize_or_zero().to_array();
        for (position_ws, uv) in corners.into_iter().zip(uvs) {
            vertices_data.push(GlassVertex {
                position_ws: position_ws.to_array(),
                uv,
                normal_ws,
                face_id,
                part_kind,
            });
        }
        indices_data.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn face_point(corners: [Vec3; 4], u: f32, v: f32) -> Vec3 {
        let bottom = corners[0].lerp(corners[1], u);
        let top = corners[3].lerp(corners[2], u);
        bottom.lerp(top, v)
    }

    fn append_face_edge_bands(
        vertices_data: &mut Vec<GlassVertex>,
        indices_data: &mut Vec<u32>,
        face_id: u32,
        normal: Vec3,
        corners: [Vec3; 4],
        edge_uv_width: f32,
    ) {
        let e = edge_uv_width.clamp(0.0, 0.25);
        let bands = [
            (0.0, 0.0, e, 1.0),
            (1.0 - e, 0.0, 1.0, 1.0),
            (0.0, 0.0, 1.0, e),
            (0.0, 1.0 - e, 1.0, 1.0),
        ];
        for (u0, v0, u1, v1) in bands {
            Self::append_quad(
                vertices_data,
                indices_data,
                face_id,
                Self::PART_EDGE_BAND,
                normal,
                [
                    Self::face_point(corners, u0, v0),
                    Self::face_point(corners, u1, v0),
                    Self::face_point(corners, u1, v1),
                    Self::face_point(corners, u0, v1),
                ],
                [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
            );
        }
    }

    #[allow(dead_code)]
    fn append_horizontal_rims(
        vertices_data: &mut Vec<GlassVertex>,
        indices_data: &mut Vec<u32>,
        box_min: Vec3,
        box_max: Vec3,
        rim_width: f32,
    ) {
        let w = rim_width
            .min((box_max.x - box_min.x) * 0.25)
            .min((box_max.z - box_min.z) * 0.25);
        for (y, normal, face_id) in [
            (box_max.y, Vec3::new(0.0, 1.0, 0.0), 4u32),
            (box_min.y, Vec3::new(0.0, -1.0, 0.0), 5u32),
        ] {
            let strips = [
                [
                    Vec3::new(box_min.x, y, box_min.z),
                    Vec3::new(box_max.x, y, box_min.z),
                    Vec3::new(box_max.x, y, box_min.z + w),
                    Vec3::new(box_min.x, y, box_min.z + w),
                ],
                [
                    Vec3::new(box_max.x, y, box_max.z),
                    Vec3::new(box_min.x, y, box_max.z),
                    Vec3::new(box_min.x, y, box_max.z - w),
                    Vec3::new(box_max.x, y, box_max.z - w),
                ],
                [
                    Vec3::new(box_min.x, y, box_max.z),
                    Vec3::new(box_min.x, y, box_min.z),
                    Vec3::new(box_min.x + w, y, box_min.z),
                    Vec3::new(box_min.x + w, y, box_max.z),
                ],
                [
                    Vec3::new(box_max.x, y, box_min.z),
                    Vec3::new(box_max.x, y, box_max.z),
                    Vec3::new(box_max.x - w, y, box_max.z),
                    Vec3::new(box_max.x - w, y, box_min.z),
                ],
            ];
            for corners in strips {
                Self::append_quad(
                    vertices_data,
                    indices_data,
                    face_id,
                    Self::PART_RIM,
                    normal,
                    corners,
                    Self::full_face_uvs(),
                );
            }
        }
    }

    fn append_corner_bevels(
        vertices_data: &mut Vec<GlassVertex>,
        indices_data: &mut Vec<u32>,
        box_min: Vec3,
        box_max: Vec3,
        bevel_width: f32,
    ) {
        let b = bevel_width
            .min((box_max.x - box_min.x) * 0.2)
            .min((box_max.z - box_min.z) * 0.2);
        let y0 = box_min.y;
        let y1 = box_max.y;
        let bevels = [
            (
                6u32,
                Vec3::new(-1.0, 0.0, -1.0),
                [
                    Vec3::new(box_min.x, y0, box_min.z + b),
                    Vec3::new(box_min.x + b, y0, box_min.z),
                    Vec3::new(box_min.x + b, y1, box_min.z),
                    Vec3::new(box_min.x, y1, box_min.z + b),
                ],
            ),
            (
                7u32,
                Vec3::new(1.0, 0.0, -1.0),
                [
                    Vec3::new(box_max.x - b, y0, box_min.z),
                    Vec3::new(box_max.x, y0, box_min.z + b),
                    Vec3::new(box_max.x, y1, box_min.z + b),
                    Vec3::new(box_max.x - b, y1, box_min.z),
                ],
            ),
            (
                8u32,
                Vec3::new(1.0, 0.0, 1.0),
                [
                    Vec3::new(box_max.x, y0, box_max.z - b),
                    Vec3::new(box_max.x - b, y0, box_max.z),
                    Vec3::new(box_max.x - b, y1, box_max.z),
                    Vec3::new(box_max.x, y1, box_max.z - b),
                ],
            ),
            (
                9u32,
                Vec3::new(-1.0, 0.0, 1.0),
                [
                    Vec3::new(box_min.x + b, y0, box_max.z),
                    Vec3::new(box_min.x, y0, box_max.z - b),
                    Vec3::new(box_min.x, y1, box_max.z - b),
                    Vec3::new(box_min.x + b, y1, box_max.z),
                ],
            ),
        ];
        for (face_id, normal, corners) in bevels {
            Self::append_quad(
                vertices_data,
                indices_data,
                face_id,
                Self::PART_CORNER_BEVEL,
                normal,
                corners,
                Self::full_face_uvs(),
            );
        }
    }
}

impl ParticleRendererResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let instance_capacity = PARTICLE_CAPACITY as u32;
        let (vertices, indices, indices_len) =
            Self::create_particle_mesh(device.clone(), allocator.clone());

        let instance_buffer = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<ParticleInstanceGpu>() as u64) * instance_capacity as u64,
        );

        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len,
            instance_buffer: Resource::new(instance_buffer),
            instance_count: 0,
        }
    }

    fn create_particle_mesh(device: Device, allocator: Allocator) -> (Buffer, Buffer, u32) {
        use crate::tracer::voxel_encoding::append_indexed_cube_data;

        let mut vertices_data = Vec::new();
        let mut indices_data = Vec::new();
        let mut voxel_infos = Vec::new();
        append_indexed_cube_data(
            &mut vertices_data,
            &mut indices_data,
            &mut voxel_infos,
            IVec3::ZERO,
            0,
            IVec3::ZERO,
            1,
            true,
        )
        .unwrap();

        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<Vertex>() * vertices_data.len()) as u64,
        );
        vertices.fill(&vertices_data).unwrap();

        let indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<u32>() * indices_data.len()) as u64,
        );
        indices.fill(&indices_data).unwrap();

        (vertices, indices, indices_data.len() as u32)
    }
}

#[derive(ResourceContainer)]
pub struct TracerUniformResources {
    pub gui_input: Resource<Buffer>,
    pub sun_info: Resource<Buffer>,
    pub shading_info: Resource<Buffer>,
    pub camera_info: Resource<Buffer>,
    pub camera_info_prev_frame: Resource<Buffer>,
    pub env_info: Resource<Buffer>,
    pub starlight_info: Resource<Buffer>,
    pub voxel_colors: Resource<Buffer>,
    pub terrain_edit_preview: Resource<Buffer>,
    pub flora_growth_info: Resource<Buffer>,
    pub god_ray_info: Resource<Buffer>,
    pub post_processing_info: Resource<Buffer>,
}

#[derive(ResourceContainer)]
pub struct ShadowResources {
    pub shadow_camera_info: Resource<Buffer>,
    pub shadow_map_depth_tex: Resource<Texture>,
    pub shadow_map_tex: Resource<Texture>,
    pub shadow_map_tex_for_vsm_ping: Resource<Texture>,
    pub shadow_map_tex_for_vsm_pong: Resource<Texture>,
    pub shadow_map_tex_for_vsm_prev: Resource<Texture>,
    pub cloud_shadow_raw_tex: Resource<Texture>,
    pub cloud_shadow_history_tex: Resource<Texture>,
    pub cloud_shadow_tex: Resource<Texture>,
    pub leaf_shadow_opacity_tex: Resource<Texture>,
    pub leaf_shadow_opacity_prev_tex: Resource<Texture>,
    pub leaf_shadow_opacity_blended_tex: Resource<Texture>,
    pub leaf_shadow_mask_tex: Resource<Texture>,
}

impl ShadowResources {
    pub fn vsm_history(&self) -> CurrentPrevious<&Resource<Texture>> {
        CurrentPrevious::new(
            &self.shadow_map_tex_for_vsm_ping,
            &self.shadow_map_tex_for_vsm_prev,
        )
    }

    pub fn cloud_shadow_history(&self) -> CurrentPrevious<&Resource<Texture>> {
        CurrentPrevious::new(&self.cloud_shadow_tex, &self.cloud_shadow_history_tex)
    }

    pub fn leaf_shadow_history(&self) -> CurrentPrevious<&Resource<Texture>> {
        CurrentPrevious::new(
            &self.leaf_shadow_opacity_blended_tex,
            &self.leaf_shadow_opacity_prev_tex,
        )
    }
}

#[derive(ResourceContainer)]
pub struct WindResources {
    pub wind_volume_info: Resource<Buffer>,
    pub wind_sources: Resource<Buffer>,
    pub wind_volume_tex: Resource<Texture>,
}

#[derive(ResourceContainer)]
pub struct TerrainQueryResources {
    pub player_collider_info: Resource<Buffer>,
    pub player_collision_result: Resource<Buffer>,
    pub terrain_query_count: Resource<Buffer>,
    pub terrain_query_info: Resource<Buffer>,
    pub terrain_query_result: Resource<Buffer>,
}

#[derive(ResourceContainer)]
pub struct TracerTextureResources {
    pub sun_sprite_tex: Resource<Texture>,
    pub particle_lod_tex_lut: Resource<Texture>,
    pub scalar_bn: Resource<Texture>,
    pub unit_vec2_bn: Resource<Texture>,
    pub unit_vec3_bn: Resource<Texture>,
    pub weighted_cosine_bn: Resource<Texture>,
    pub fast_unit_vec3_bn: Resource<Texture>,
    pub fast_weighted_cosine_bn: Resource<Texture>,
}

pub struct TracerMeshResources {
    pub flora_meshes: Vec<FloraMeshResources>,
    pub leaves_resources: LeavesResources,
    pub apple_resources: FloraMeshResources,
    pub flora_meshes_lod: Vec<FloraMeshResources>,
    pub leaves_resources_lod: LeavesResources,
    pub apple_resources_lod: FloraMeshResources,
    pub glass: GlassMeshResources,
}

impl verdarium_vkn::ResourceContainer for TracerMeshResources {
    fn get_buffer(&self, _name: &str) -> Option<&verdarium_vkn::Buffer> {
        None
    }

    fn get_texture(&self, _name: &str) -> Option<&verdarium_vkn::Texture> {
        None
    }

    fn get_resource_names(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

impl TracerUniformResources {
    #[allow(clippy::too_many_arguments)]
    fn new(
        device: Device,
        allocator: Allocator,
        tracer_sm: &ShaderModule,
        composition_sm: &ShaderModule,
        god_ray_sm: &ShaderModule,
        post_processing_sm: &ShaderModule,
        flora_vert_sm: &ShaderModule,
    ) -> Self {
        let layout_buffer = |shader: &ShaderModule, name: &str| {
            Buffer::from_buffer_layout(
                device.clone(),
                allocator.clone(),
                shader.get_buffer_layout(name).unwrap().clone(),
                BufferUsage::empty(),
                MemoryLocation::CpuToGpu,
            )
        };

        Self {
            gui_input: Resource::new(layout_buffer(tracer_sm, "U_GuiInput")),
            sun_info: Resource::new(layout_buffer(tracer_sm, "U_SunInfo")),
            shading_info: Resource::new(layout_buffer(tracer_sm, "U_ShadingInfo")),
            camera_info: Resource::new(layout_buffer(tracer_sm, "U_CameraInfo")),
            camera_info_prev_frame: Resource::new(layout_buffer(
                tracer_sm,
                "U_CameraInfoPrevFrame",
            )),
            env_info: Resource::new(layout_buffer(tracer_sm, "U_EnvInfo")),
            starlight_info: Resource::new(layout_buffer(composition_sm, "U_StarlightInfo")),
            voxel_colors: Resource::new(layout_buffer(tracer_sm, "U_VoxelColors")),
            terrain_edit_preview: Resource::new(layout_buffer(tracer_sm, "U_TerrainEditPreview")),
            flora_growth_info: Resource::new(layout_buffer(flora_vert_sm, "U_FloraGrowthInfo")),
            god_ray_info: Resource::new(layout_buffer(god_ray_sm, "U_GodRayInfo")),
            post_processing_info: Resource::new(layout_buffer(
                post_processing_sm,
                "U_PostProcessingInfo",
            )),
        }
    }
}

impl TerrainQueryResources {
    fn new(
        device: Device,
        allocator: Allocator,
        player_collider_sm: &ShaderModule,
        terrain_query_sm: &ShaderModule,
        max_terrain_queries: u32,
    ) -> Self {
        let layout_buffer = |shader: &ShaderModule, name: &str| {
            Buffer::from_buffer_layout(
                device.clone(),
                allocator.clone(),
                shader.get_buffer_layout(name).unwrap().clone(),
                BufferUsage::empty(),
                MemoryLocation::CpuToGpu,
            )
        };

        let terrain_query_info = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            (max_terrain_queries * 8 * std::mem::size_of::<f32>() as u32) as u64,
        );

        let terrain_query_result = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            (max_terrain_queries * 4 * std::mem::size_of::<f32>() as u32) as u64,
        );

        Self {
            player_collider_info: Resource::new(layout_buffer(
                player_collider_sm,
                "U_PlayerColliderInfo",
            )),
            player_collision_result: Resource::new(layout_buffer(
                player_collider_sm,
                "B_PlayerCollisionResult",
            )),
            terrain_query_count: Resource::new(layout_buffer(
                terrain_query_sm,
                "U_TerrainQueryCount",
            )),
            terrain_query_info: Resource::new(terrain_query_info),
            terrain_query_result: Resource::new(terrain_query_result),
        }
    }
}

impl ShadowResources {
    fn new(
        device: Device,
        allocator: Allocator,
        tracer_shadow_sm: &ShaderModule,
        shadow_map_extent: Extent2D,
        cloud_shadow_extent: Extent2D,
        leaf_shadow_opacity_extent: Extent2D,
    ) -> Self {
        let shadow_camera_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            tracer_shadow_sm
                .get_buffer_layout("U_ShadowCameraInfo")
                .unwrap()
                .clone(),
            BufferUsage::empty(),
            MemoryLocation::CpuToGpu,
        );
        let shadow_map_extent: Extent3D = shadow_map_extent.into();
        let cloud_shadow_extent: Extent3D = cloud_shadow_extent.into();
        let leaf_shadow_opacity_extent: Extent3D = leaf_shadow_opacity_extent.into();
        let leaf_shadow_mask_extent = Extent3D::new(
            (leaf_shadow_opacity_extent.width / 8).max(1),
            (leaf_shadow_opacity_extent.height / 8).max(1),
            1,
        );
        log::info!(
            "[SHADOW] using VSM shadow map {}x{}",
            shadow_map_extent.width,
            shadow_map_extent.height,
        );
        log::info!(
            "[CLOUD_SHADOW] using Beer transmittance map {}x{} with temporal resolve",
            cloud_shadow_extent.width,
            cloud_shadow_extent.height,
        );
        log::info!(
            "[LEAF_SHADOW] using 2D opacity map {}x{}, temporal history, and influence mask {}x{}",
            leaf_shadow_opacity_extent.width,
            leaf_shadow_opacity_extent.height,
            leaf_shadow_mask_extent.width,
            leaf_shadow_mask_extent.height,
        );

        Self {
            shadow_camera_info: Resource::new(shadow_camera_info),
            shadow_map_depth_tex: Resource::new(TracerResources::create_shadow_map_depth_tex(
                device.clone(),
                allocator.clone(),
                shadow_map_extent,
            )),
            shadow_map_tex: Resource::new(TracerResources::create_shadow_map_tex(
                device.clone(),
                allocator.clone(),
                shadow_map_extent,
            )),
            shadow_map_tex_for_vsm_ping: Resource::new(
                TracerResources::create_shadow_map_tex_for_vsm_pingpong(
                    device.clone(),
                    allocator.clone(),
                    shadow_map_extent,
                ),
            ),
            shadow_map_tex_for_vsm_pong: Resource::new(
                TracerResources::create_shadow_map_tex_for_vsm_pingpong(
                    device.clone(),
                    allocator.clone(),
                    shadow_map_extent,
                ),
            ),
            shadow_map_tex_for_vsm_prev: Resource::new(
                TracerResources::create_shadow_map_tex_for_vsm_pingpong(
                    device.clone(),
                    allocator.clone(),
                    shadow_map_extent,
                ),
            ),
            cloud_shadow_raw_tex: Resource::new(TracerResources::create_cloud_shadow_tex(
                device.clone(),
                allocator.clone(),
                cloud_shadow_extent,
            )),
            cloud_shadow_history_tex: Resource::new(TracerResources::create_cloud_shadow_tex(
                device.clone(),
                allocator.clone(),
                cloud_shadow_extent,
            )),
            cloud_shadow_tex: Resource::new(TracerResources::create_cloud_shadow_tex(
                device.clone(),
                allocator.clone(),
                cloud_shadow_extent,
            )),
            leaf_shadow_opacity_tex: Resource::new(
                TracerResources::create_leaf_shadow_opacity_tex(
                    device.clone(),
                    allocator.clone(),
                    leaf_shadow_opacity_extent,
                ),
            ),
            leaf_shadow_opacity_prev_tex: Resource::new(
                TracerResources::create_leaf_shadow_opacity_history_tex(
                    device.clone(),
                    allocator.clone(),
                    leaf_shadow_opacity_extent,
                ),
            ),
            leaf_shadow_opacity_blended_tex: Resource::new(
                TracerResources::create_leaf_shadow_opacity_blended_tex(
                    device.clone(),
                    allocator.clone(),
                    leaf_shadow_opacity_extent,
                ),
            ),
            leaf_shadow_mask_tex: Resource::new(TracerResources::create_leaf_shadow_mask_tex(
                device,
                allocator,
                leaf_shadow_mask_extent,
            )),
        }
    }
}

impl WindResources {
    fn new(
        device: Device,
        allocator: Allocator,
        flora_vert_sm: &ShaderModule,
        chunk_bound: UAabb3,
    ) -> Self {
        let wind_volume_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            flora_vert_sm
                .get_buffer_layout("U_WindVolumeInfo")
                .unwrap()
                .clone(),
            BufferUsage::empty(),
            MemoryLocation::CpuToGpu,
        );
        let wind_sources =
            TracerResources::create_wind_sources_buffer(device.clone(), allocator.clone(), 1);
        let chunk_extent = chunk_bound.get_extent();
        wind_volume_info
            .fill_uniform(&WindVolumeInfoGpu {
                world_chunk_extent: [
                    chunk_extent.width as f32,
                    chunk_extent.height as f32,
                    chunk_extent.depth as f32,
                ],
                _pad0: 0.0,
            })
            .unwrap();

        Self {
            wind_volume_info: Resource::new(wind_volume_info),
            wind_sources: Resource::new(wind_sources),
            wind_volume_tex: Resource::new(TracerResources::create_wind_volume_tex(
                device,
                allocator,
                chunk_bound,
            )),
        }
    }
}

impl TracerTextureResources {
    fn new(vulkan_ctx: &VulkanContext, allocator: Allocator) -> Self {
        Self {
            sun_sprite_tex: Resource::new(TracerResources::create_sun_sprite_tex(
                vulkan_ctx,
                allocator.clone(),
            )),
            particle_lod_tex_lut: Resource::new(TracerResources::create_particle_lod_tex_lut(
                vulkan_ctx,
                allocator.clone(),
            )),
            scalar_bn: Resource::new(TracerResources::create_bn(
                vulkan_ctx,
                allocator.clone(),
                vk::Format::R8_UNORM,
                "stbn/scalar_2d_1d_1d/stbn_scalar_2Dx1Dx1D_128x128x64x1_",
            )),
            unit_vec2_bn: Resource::new(TracerResources::create_bn(
                vulkan_ctx,
                allocator.clone(),
                vk::Format::R8G8_UNORM,
                "stbn/unitvec2_2d_1d/stbn_unitvec2_2Dx1D_128x128x64_",
            )),
            unit_vec3_bn: Resource::new(TracerResources::create_bn(
                vulkan_ctx,
                allocator.clone(),
                vk::Format::R8G8B8A8_UNORM,
                "stbn/unitvec3_2d_1d/stbn_unitvec3_2Dx1D_128x128x64_",
            )),
            weighted_cosine_bn: Resource::new(TracerResources::create_bn(
                vulkan_ctx,
                allocator.clone(),
                vk::Format::R8G8B8A8_UNORM,
                "stbn/unitvec3_cosine_2d_1d/stbn_unitvec3_cosine_2Dx1D_128x128x64_",
            )),
            fast_unit_vec3_bn: Resource::new(TracerResources::create_bn(
                vulkan_ctx,
                allocator.clone(),
                vk::Format::R8G8B8A8_UNORM,
                "fast/unit_vec3/out_",
            )),
            fast_weighted_cosine_bn: Resource::new(TracerResources::create_bn(
                vulkan_ctx,
                allocator,
                vk::Format::R8G8B8A8_UNORM,
                "fast/weighted_cosine/out_",
            )),
        }
    }
}

impl TracerMeshResources {
    fn new(device: Device, allocator: Allocator, chunk_bound: UAabb3) -> Self {
        species::assert_species_limit();
        let flora_meshes = species::species()
            .iter()
            .map(|desc| {
                FloraMeshResources::new(
                    device.clone(),
                    allocator.clone(),
                    false,
                    desc.mesh_generator,
                )
            })
            .collect::<Vec<_>>();
        let leaves_resources = LeavesResources::new(device.clone(), allocator.clone(), false);
        let apple_resources = FloraMeshResources::new(
            device.clone(),
            allocator.clone(),
            false,
            generate_indexed_voxel_apple,
        );
        let flora_meshes_lod = species::species()
            .iter()
            .map(|desc| {
                FloraMeshResources::new(
                    device.clone(),
                    allocator.clone(),
                    true,
                    desc.mesh_generator,
                )
            })
            .collect::<Vec<_>>();
        let leaves_resources_lod = LeavesResources::new(device.clone(), allocator.clone(), true);
        let apple_resources_lod = FloraMeshResources::new(
            device.clone(),
            allocator.clone(),
            true,
            generate_indexed_voxel_apple,
        );
        let glass = GlassMeshResources::new(device, allocator, chunk_bound);

        Self {
            flora_meshes,
            leaves_resources,
            apple_resources,
            flora_meshes_lod,
            leaves_resources_lod,
            apple_resources_lod,
            glass,
        }
    }
}

#[derive(ResourceContainer)]
pub struct TracerResources {
    pub uniforms: TracerUniformResources,
    pub shadow: ShadowResources,
    pub wind: WindResources,
    pub flora_voxel_lookup: FloraVoxelLookupResources,
    pub terrain_query: TerrainQueryResources,
    pub textures: TracerTextureResources,
    pub meshes: TracerMeshResources,
    pub extent_dependent_resources: ExtentDependentResources,
    pub denoiser_resources: DenoiserResources,
}

impl TracerResources {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        tracer_sm: &ShaderModule,
        tracer_shadow_sm: &ShaderModule,
        composition_sm: &ShaderModule,
        temporal_sm: &ShaderModule,
        spatial_sm: &ShaderModule,
        god_ray_sm: &ShaderModule,
        post_processing_sm: &ShaderModule,
        player_collider_sm: &ShaderModule,
        terrain_query_sm: &ShaderModule,
        flora_vert_sm: &ShaderModule,
        chunk_bound: UAabb3,
        rendering_extent: Extent2D,
        screen_extent: Extent2D,
        shadow_map_extent: Extent2D,
        cloud_shadow_extent: Extent2D,
        leaf_shadow_opacity_extent: Extent2D,
        max_terrain_queries: u32,
    ) -> Self {
        let device = vulkan_ctx.device();

        Self {
            uniforms: TracerUniformResources::new(
                device.clone(),
                allocator.clone(),
                tracer_sm,
                composition_sm,
                god_ray_sm,
                post_processing_sm,
                flora_vert_sm,
            ),
            shadow: ShadowResources::new(
                device.clone(),
                allocator.clone(),
                tracer_shadow_sm,
                shadow_map_extent,
                cloud_shadow_extent,
                leaf_shadow_opacity_extent,
            ),
            wind: WindResources::new(
                device.clone(),
                allocator.clone(),
                flora_vert_sm,
                chunk_bound,
            ),
            flora_voxel_lookup: FloraVoxelLookupResources::new(device.clone(), allocator.clone()),
            terrain_query: TerrainQueryResources::new(
                device.clone(),
                allocator.clone(),
                player_collider_sm,
                terrain_query_sm,
                max_terrain_queries,
            ),
            textures: TracerTextureResources::new(vulkan_ctx, allocator.clone()),
            meshes: TracerMeshResources::new(device.clone(), allocator.clone(), chunk_bound),
            extent_dependent_resources: ExtentDependentResources::new(
                device.clone(),
                allocator.clone(),
                rendering_extent,
                screen_extent,
            ),
            denoiser_resources: DenoiserResources::new(
                device.clone(),
                allocator,
                rendering_extent,
                temporal_sm,
                spatial_sm,
            ),
        }
    }

    pub fn on_resize(
        &mut self,
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
        screen_extent: Extent2D,
    ) {
        self.extent_dependent_resources.on_resize(
            device,
            allocator,
            rendering_extent,
            screen_extent,
        );
        self.denoiser_resources.on_resize(rendering_extent);
    }

    fn create_bn(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        format: vk::Format,
        relative_path: &str,
    ) -> Texture {
        const BLUE_NOISE_LEN: u32 = 64;

        let img_desc = ImageDesc {
            extent: Extent3D::new(128, 128, 1),
            array_len: BLUE_NOISE_LEN,
            format,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(vulkan_ctx.device().clone(), allocator, &img_desc, &sam_desc);

        let base_path = get_project_root() + "/assets/texture/noise/";
        for i in 0..BLUE_NOISE_LEN {
            let path = format!("{}{}{}.png", base_path, relative_path, i);
            tex.get_image()
                .load_and_fill(
                    &vulkan_ctx.get_general_queue(),
                    vulkan_ctx.command_pool(),
                    &path,
                    i,
                    Some(TextureLayout::GENERAL),
                )
                .unwrap();
        }
        tex
    }

    fn create_sun_sprite_tex(vulkan_ctx: &VulkanContext, allocator: Allocator) -> Texture {
        const SUN_SPRITE_REL_PATH: &str = "assets/texture/Planets_16x16/Sun.png";

        let path = get_project_root() + "/" + SUN_SPRITE_REL_PATH;
        if !Path::new(&path).exists() {
            panic!("Sun sprite texture missing at '{}'", path);
        }
        let image = image::open(&path).unwrap_or_else(|e| {
            panic!("Failed to open sun sprite texture '{}': {}", path, e);
        });
        let rgba = image.to_rgba8();
        let (w, h) = rgba.dimensions();
        let extent = Extent2D::new(w, h);
        let texels_rgba = rgba.into_raw();

        let img_desc = ImageDesc {
            extent: extent.into(),
            array_len: 1,
            format: vk::Format::R8G8B8A8_SRGB,
            usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(vulkan_ctx.device().clone(), allocator, &img_desc, &sam_desc);

        tex.get_image()
            .fill_with_raw_u8(
                &vulkan_ctx.get_general_queue(),
                vulkan_ctx.command_pool(),
                TextureRegion::from_image(tex.get_image()),
                &texels_rgba,
                0,
                Some(TextureLayout::GENERAL),
            )
            .unwrap();
        tex
    }

    fn create_particle_lod_tex_lut(vulkan_ctx: &VulkanContext, allocator: Allocator) -> Texture {
        const PARTICLE_LOD_TEXTURE_DIR_REL_PATH: &str = "assets/texture/butterfly_16px";
        let frame_dim = PARTICLE_SPRITE_FRAME_DIM;
        let layout = ParticleTextureLayout::new();
        layout.assert_valid();

        let white = [255u8, 255u8, 255u8, 255u8];
        let white_layer = white
            .repeat((frame_dim * frame_dim) as usize)
            .into_iter()
            .collect::<Vec<u8>>();
        let dir_path = get_project_root() + "/" + PARTICLE_LOD_TEXTURE_DIR_REL_PATH;
        let mut atlas_paths = std::fs::read_dir(&dir_path)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
            })
            .collect::<Vec<_>>();
        atlas_paths.sort();

        assert!(
            !atlas_paths.is_empty(),
            "Butterfly atlas not found in '{}'",
            dir_path
        );

        let butterfly_atlas_path = atlas_paths
            .iter()
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case("butterfly.png"))
            })
            .unwrap_or(&atlas_paths[0]);

        if atlas_paths.len() > 1 {
            log::warn!(
                "Found multiple butterfly atlases in '{}', using '{}'",
                dir_path,
                butterfly_atlas_path.display()
            );
        }

        let atlas_path_str = butterfly_atlas_path.to_string_lossy().to_string();
        let atlas_rgba = image::open(butterfly_atlas_path)
            .unwrap_or_else(|e| {
                panic!("Failed to open butterfly atlas '{}': {}", atlas_path_str, e)
            })
            .to_rgba8();
        let (width, height) = atlas_rgba.dimensions();
        let expected_size = frame_dim * 5;
        assert!(
            width == expected_size && height == expected_size,
            "Butterfly atlas must be {}x{}, got {}x{}",
            expected_size,
            expected_size,
            width,
            height
        );

        let mut butterfly_layers = Vec::new();
        for preset_idx in 0..layout.butterfly_preset_count() {
            let preset = ButterflyPalettePreset::from_index(preset_idx);
            let config = preset.config();
            let rgba = load_butterfly_and_remap(butterfly_atlas_path, &config);
            let label = format!("{} ({})", atlas_path_str, preset.name());
            for view in 0..layout.butterfly_view_count() {
                let row = BUTTERFLY_ATLAS_ROW_FOR_VIEW[view as usize];
                if let Some(frames) = Self::extract_row_sequence_layers(
                    &rgba,
                    row,
                    layout.butterfly_frames_per_view(),
                    &label,
                ) {
                    butterfly_layers.extend(frames);
                } else {
                    panic!(
                        "Failed to extract butterfly frames for view {} (row {}) of '{}'",
                        view, row, label
                    );
                }
            }
        }

        let lut_layer_count = layout.total_layer_count();

        let sam_desc = Default::default();
        let img_desc = ImageDesc {
            extent: Extent3D::new(frame_dim, frame_dim, 1),
            array_len: lut_layer_count,
            format: vk::Format::R8G8B8A8_SRGB,
            usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let tex = Texture::new(vulkan_ctx.device().clone(), allocator, &img_desc, &sam_desc);

        Self::fill_particle_lut_layer(vulkan_ctx, &tex, layout.leaf_layer(), &white_layer);
        for (frame_idx, frame_data) in butterfly_layers.iter().enumerate() {
            Self::fill_particle_lut_layer(
                vulkan_ctx,
                &tex,
                layout.butterfly_base_layer() + frame_idx as u32,
                frame_data.as_slice(),
            );
        }

        tex
    }

    fn extract_row_sequence_layers(
        atlas: &image::RgbaImage,
        row: u32,
        target_frame_count: u32,
        source_label: &str,
    ) -> Option<Vec<Vec<u8>>> {
        if target_frame_count == 0 {
            return Some(Vec::new());
        }

        let frame_dim = PARTICLE_SPRITE_FRAME_DIM;
        let (width, height) = atlas.dimensions();
        let row_y = row.saturating_mul(frame_dim);
        if width < frame_dim || height < row_y.saturating_add(frame_dim) {
            log::warn!(
                "Animated texture '{}' is {}x{}; row {} with {}x{} frames is unavailable",
                source_label,
                width,
                height,
                row,
                frame_dim,
                frame_dim
            );
            return None;
        }

        if width % frame_dim != 0 {
            log::warn!(
                "Animated texture '{}' width {} is not divisible by frame size {}; ignoring trailing pixels",
                source_label,
                width,
                frame_dim
            );
        }

        let available_frames = (width / frame_dim).max(1);
        let mut frames = Vec::with_capacity(target_frame_count as usize);
        for target_frame_idx in 0..target_frame_count {
            let src_frame_idx = target_frame_idx.min(available_frames - 1);
            let frame = image::imageops::crop_imm(
                atlas,
                src_frame_idx * frame_dim,
                row_y,
                frame_dim,
                frame_dim,
            )
            .to_image();
            frames.push(Self::to_particle_frame_bytes(frame));
        }
        Some(frames)
    }

    fn to_particle_frame_bytes(frame: image::RgbaImage) -> Vec<u8> {
        frame.into_raw()
    }

    fn fill_particle_lut_layer(vulkan_ctx: &VulkanContext, tex: &Texture, layer: u32, data: &[u8]) {
        tex.get_image()
            .fill_with_raw_u8(
                &vulkan_ctx.get_general_queue(),
                vulkan_ctx.command_pool(),
                TextureRegion::from_image(tex.get_image()),
                data,
                layer,
                Some(TextureLayout::GENERAL),
            )
            .unwrap();
    }

    fn create_shadow_map_depth_tex(
        device: Device,
        allocator: Allocator,
        shadow_map_extent: Extent3D,
    ) -> Texture {
        // keep the raster shadow pass on a real depth image; macOS cannot use
        // D32_SFLOAT as a storage image for the later compute stages.
        let tex_desc = ImageDesc {
            extent: shadow_map_extent,
            format: vk::Format::D32_SFLOAT,
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::DEPTH,
            ..Default::default()
        };
        let sam_desc = Default::default();
        Texture::new(device, allocator, &tex_desc, &sam_desc)
    }

    fn create_shadow_map_tex(
        device: Device,
        allocator: Allocator,
        shadow_map_extent: Extent3D,
    ) -> Texture {
        // the compute shadow path writes and filters a float image so all
        // platforms use the same storage-compatible shadow source.
        let tex_desc = ImageDesc {
            extent: shadow_map_extent,
            format: vk::Format::R32_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        Texture::new(device, allocator, &tex_desc, &sam_desc)
    }

    fn create_shadow_map_tex_for_vsm_pingpong(
        device: Device,
        allocator: Allocator,
        shadow_map_extent: Extent3D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: shadow_map_extent,
            format: vk::Format::R32G32B32A32_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        // Filtered VSM moments should be interpolated at lookup time; using
        // the default nearest sampler makes grass shadows snap by whole texels
        // even after the compute blur has softened the moments.
        let sam_desc = SamplerDesc {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &sam_desc)
    }

    fn create_cloud_shadow_tex(
        device: Device,
        allocator: Allocator,
        cloud_shadow_extent: Extent3D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: cloud_shadow_extent,
            format: vk::Format::R16_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = SamplerDesc {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &sam_desc)
    }

    fn create_leaf_shadow_opacity_tex(
        device: Device,
        allocator: Allocator,
        shadow_map_extent: Extent3D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: shadow_map_extent,
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = SamplerDesc {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &sam_desc)
    }

    fn create_leaf_shadow_opacity_history_tex(
        device: Device,
        allocator: Allocator,
        shadow_map_extent: Extent3D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: shadow_map_extent,
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = SamplerDesc {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &sam_desc)
    }

    fn create_leaf_shadow_opacity_blended_tex(
        device: Device,
        allocator: Allocator,
        shadow_map_extent: Extent3D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: shadow_map_extent,
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = SamplerDesc {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &sam_desc)
    }

    fn create_leaf_shadow_mask_tex(
        device: Device,
        allocator: Allocator,
        extent: Extent3D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent,
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = SamplerDesc {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &sam_desc)
    }

    pub fn create_wind_sources_buffer(
        device: Device,
        allocator: Allocator,
        capacity: usize,
    ) -> Buffer {
        let byte_count =
            (capacity.max(1) * std::mem::size_of::<crate::tracer::WindSourceGpu>()) as u64;
        Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            byte_count,
        )
    }

    fn create_wind_volume_tex(
        device: Device,
        allocator: Allocator,
        chunk_bound: UAabb3,
    ) -> Texture {
        let chunk_extent = chunk_bound.get_extent();
        let tex_desc = ImageDesc {
            extent: Extent3D::new(
                chunk_extent.width * WIND_VOLUME_TEXELS_PER_CHUNK.x * WIND_VOLUME_BUCKET_COUNT,
                chunk_extent.height * WIND_VOLUME_TEXELS_PER_CHUNK.y,
                chunk_extent.depth * WIND_VOLUME_TEXELS_PER_CHUNK.z,
            ),
            format: vk::Format::R16G16_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = SamplerDesc {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &sam_desc)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct WindVolumeInfoGpu {
    world_chunk_extent: [f32; 3],
    _pad0: f32,
}
