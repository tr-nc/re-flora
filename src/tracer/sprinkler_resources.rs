use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Vec3};
use re_flora_vkn::vk;
use re_flora_vkn::{Allocator, Buffer, BufferUsage, Device, MemoryLocation};

use crate::{
    resource::Resource,
    tracer::voxel_geometry::{CUBE_INDICES, VOXEL_VERTICES},
};

pub const SPRINKLER_RENDER_CAPACITY: usize = 256;
const VOXEL_SCALE: f32 = 1.0 / 256.0;
const PEDESTAL_VOXEL_COUNT: usize = 3;
const CAP_VOXEL_COUNT: usize = 5;
const MODEL_GRID_OFFSET_VOXELS: Vec3 = Vec3::new(-0.5, 0.0, -0.5);
const PEDESTAL_COLOR_SRGB: Vec3 = Vec3::new(0.62, 0.64, 0.63);
const CAP_COLOR_SRGB: Vec3 = Vec3::new(1.0, 0.22, 0.015);
const CAP_EDGE_EXPANSION_VOXELS: f32 = 0.2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SprinklerVertex {
    position: [f32; 3],
    voxel_center: [f32; 3],
    shading_normal: [f32; 3],
    color_srgb: [f32; 3],
    animation_direction: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SprinklerInstanceGpu {
    base_position: [f32; 3],
}

pub struct SprinklerRendererResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
    pub instances: Resource<Buffer>,
    pub instance_count: u32,
}

impl SprinklerRendererResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let (vertices_data, indices_data) = build_sprinkler_mesh();
        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of_val(vertices_data.as_slice()) as u64,
        );
        vertices.fill(&vertices_data).unwrap();

        let indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of_val(indices_data.as_slice()) as u64,
        );
        indices.fill(&indices_data).unwrap();

        let instances = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (std::mem::size_of::<SprinklerInstanceGpu>() * SPRINKLER_RENDER_CAPACITY) as u64,
        );

        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len: indices_data.len() as u32,
            instances: Resource::new(instances),
            instance_count: 0,
        }
    }

    pub fn upload(&mut self, base_positions: &[Vec3]) -> Result<()> {
        ensure!(
            base_positions.len() <= SPRINKLER_RENDER_CAPACITY,
            "sprinkler render capacity exceeded: {} > {}",
            base_positions.len(),
            SPRINKLER_RENDER_CAPACITY,
        );
        let instances = base_positions
            .iter()
            .map(|position| SprinklerInstanceGpu {
                base_position: position.to_array(),
            })
            .collect::<Vec<_>>();
        if !instances.is_empty() {
            self.instances.fill(&instances)?;
        }
        self.instance_count = instances.len() as u32;
        Ok(())
    }
}

fn build_sprinkler_mesh() -> (Vec<SprinklerVertex>, Vec<u32>) {
    let voxel_count = PEDESTAL_VOXEL_COUNT + CAP_VOXEL_COUNT;
    let mut vertices = Vec::with_capacity(voxel_count * VOXEL_VERTICES.len());
    let mut indices = Vec::with_capacity(voxel_count * CUBE_INDICES.len());

    // One-voxel-wide stem, three voxels high.
    for y in 0..3 {
        append_voxel(
            &mut vertices,
            &mut indices,
            IVec3::new(0, y, 0),
            PEDESTAL_COLOR_SRGB,
            Vec3::ZERO,
        );
    }

    // Centered 3x3 cross: remove all four corners from the square. The entire cap moves
    // vertically while all four arms expand together at the same phase.
    for x in -1..=1 {
        for z in -1..=1 {
            if x != 0 && z != 0 {
                continue;
            }
            let outward_direction = match (x, z) {
                (_, -1) => -Vec3::Z,
                (-1, _) => -Vec3::X,
                (_, 1) => Vec3::Z,
                (1, _) => Vec3::X,
                _ => Vec3::ZERO,
            };
            append_voxel(
                &mut vertices,
                &mut indices,
                IVec3::new(x, 3, z),
                CAP_COLOR_SRGB,
                Vec3::Y + outward_direction * CAP_EDGE_EXPANSION_VOXELS,
            );
        }
    }

    (vertices, indices)
}

fn append_voxel(
    vertices: &mut Vec<SprinklerVertex>,
    indices: &mut Vec<u32>,
    voxel_min: IVec3,
    color_srgb: Vec3,
    animation_direction: Vec3,
) {
    let voxel_min_voxels = voxel_min.as_vec3() + MODEL_GRID_OFFSET_VOXELS;
    let voxel_center_voxels = voxel_min_voxels + Vec3::splat(0.5);
    let voxel_center = voxel_center_voxels * VOXEL_SCALE;
    let shading_normal = voxel_center_voxels.normalize_or_zero();
    let base = vertices.len() as u32;

    vertices.extend(VOXEL_VERTICES.map(|offset| SprinklerVertex {
        position: ((voxel_min_voxels + offset.as_vec3()) * VOXEL_SCALE).to_array(),
        voxel_center: voxel_center.to_array(),
        shading_normal: shading_normal.to_array(),
        color_srgb: color_srgb.to_array(),
        animation_direction: animation_direction.to_array(),
    }));
    indices.extend(CUBE_INDICES.map(|index| base + index));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprinkler_mesh_has_stable_voxel_counts() {
        let (vertices, indices) = build_sprinkler_mesh();
        let voxel_count = PEDESTAL_VOXEL_COUNT + CAP_VOXEL_COUNT;

        assert_eq!(vertices.len(), voxel_count * VOXEL_VERTICES.len());
        assert_eq!(indices.len(), voxel_count * CUBE_INDICES.len());
        let max_y = vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert_eq!(max_y, 4.0 / 256.0);
    }

    #[test]
    fn sprinkler_stem_and_cross_cap_are_centered() {
        let (vertices, _) = build_sprinkler_mesh();
        let pedestal_vertex_count = PEDESTAL_VOXEL_COUNT * VOXEL_VERTICES.len();
        let (pedestal, cap) = vertices.split_at(pedestal_vertex_count);

        let axis_bounds = |vertices: &[SprinklerVertex], axis: usize| {
            vertices
                .iter()
                .map(|vertex| vertex.position[axis])
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
                    (min.min(value), max.max(value))
                })
        };
        let (stem_min_x, stem_max_x) = axis_bounds(pedestal, 0);
        let (stem_min_z, stem_max_z) = axis_bounds(pedestal, 2);
        let (cap_min_x, cap_max_x) = axis_bounds(cap, 0);
        let (cap_min_z, cap_max_z) = axis_bounds(cap, 2);

        assert_eq!((stem_min_x, stem_max_x), (-0.5 / 256.0, 0.5 / 256.0));
        assert_eq!((stem_min_z, stem_max_z), (-0.5 / 256.0, 0.5 / 256.0));
        assert_eq!((cap_min_x, cap_max_x), (-1.5 / 256.0, 1.5 / 256.0));
        assert_eq!((cap_min_z, cap_max_z), (-1.5 / 256.0, 1.5 / 256.0));
    }

    #[test]
    fn sprinkler_voxels_share_one_shading_normal() {
        let (vertices, _) = build_sprinkler_mesh();

        for voxel in vertices.chunks_exact(VOXEL_VERTICES.len()) {
            let first = voxel[0];
            assert!(voxel.iter().all(|vertex| {
                vertex.voxel_center == first.voxel_center
                    && vertex.shading_normal == first.shading_normal
                    && vertex.color_srgb == first.color_srgb
                    && vertex.animation_direction == first.animation_direction
            }));
            let normal = Vec3::from_array(first.shading_normal);
            assert!((normal.length() - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn sprinkler_cap_lifts_and_expands_in_sync() {
        let (vertices, _) = build_sprinkler_mesh();
        let vertices_per_voxel = VOXEL_VERTICES.len();
        let animated_voxels = vertices
            .chunks_exact(vertices_per_voxel)
            .filter(|voxel| voxel[0].animation_direction != Vec3::ZERO.to_array())
            .collect::<Vec<_>>();

        assert_eq!(animated_voxels.len(), CAP_VOXEL_COUNT);
        assert!(animated_voxels
            .iter()
            .all(|voxel| voxel[0].animation_direction[1] == 1.0));
        let expanding_voxels = animated_voxels
            .iter()
            .filter(|voxel| {
                voxel[0].animation_direction[0] != 0.0 || voxel[0].animation_direction[2] != 0.0
            })
            .count();
        assert_eq!(expanding_voxels, 4);
        assert!(animated_voxels.iter().all(|voxel| {
            voxel[0].animation_direction[0].abs() <= CAP_EDGE_EXPANSION_VOXELS
                && voxel[0].animation_direction[2].abs() <= CAP_EDGE_EXPANSION_VOXELS
        }));
    }

    #[test]
    fn sprinkler_vertex_layout_matches_shader_locations() {
        assert_eq!(std::mem::size_of::<SprinklerVertex>(), 15 * 4);
        assert_eq!(std::mem::size_of::<SprinklerInstanceGpu>(), 3 * 4);
    }
}
