use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Vec3};
use re_flora_vkn::vk;
use re_flora_vkn::{Allocator, Buffer, BufferUsage, Device, MemoryLocation};

use crate::resource::Resource;

pub const SPRINKLER_RENDER_CAPACITY: usize = 256;
const VOXEL_SCALE: f32 = 1.0 / 256.0;
const PEDESTAL_VOXEL_COUNT: usize = 8;
const CAP_VOXEL_COUNT: usize = 12;
const PEDESTAL_COLOR_SRGB: Vec3 = Vec3::new(0.018, 0.022, 0.026);
const CAP_COLOR_SRGB: Vec3 = Vec3::new(1.0, 0.22, 0.015);

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SprinklerVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color_srgb: [f32; 3],
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
    let mut vertices = Vec::with_capacity(voxel_count * 6 * 4);
    let mut indices = Vec::with_capacity(voxel_count * 6 * 6);

    // Centered 2x2 stem, two voxels high.
    for y in 0..2 {
        for x in -1..1 {
            for z in -1..1 {
                append_voxel(
                    &mut vertices,
                    &mut indices,
                    IVec3::new(x, y, z),
                    PEDESTAL_COLOR_SRGB,
                );
            }
        }
    }

    // Deterministic 4x4 head with exactly the four corners removed.
    for x in -2..2 {
        for z in -2..2 {
            if (x == -2 || x == 1) && (z == -2 || z == 1) {
                continue;
            }
            append_voxel(
                &mut vertices,
                &mut indices,
                IVec3::new(x, 2, z),
                CAP_COLOR_SRGB,
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
) {
    let min = voxel_min.as_vec3() * VOXEL_SCALE;
    let max = min + Vec3::splat(VOXEL_SCALE);
    let faces = [
        (
            Vec3::X,
            [
                Vec3::new(max.x, min.y, min.z),
                Vec3::new(max.x, max.y, min.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(max.x, min.y, max.z),
            ],
        ),
        (
            -Vec3::X,
            [
                Vec3::new(min.x, min.y, max.z),
                Vec3::new(min.x, max.y, max.z),
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(min.x, min.y, min.z),
            ],
        ),
        (
            Vec3::Y,
            [
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(min.x, max.y, max.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(max.x, max.y, min.z),
            ],
        ),
        (
            -Vec3::Y,
            [
                Vec3::new(min.x, min.y, max.z),
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(max.x, min.y, min.z),
                Vec3::new(max.x, min.y, max.z),
            ],
        ),
        (
            Vec3::Z,
            [
                Vec3::new(max.x, min.y, max.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(min.x, max.y, max.z),
                Vec3::new(min.x, min.y, max.z),
            ],
        ),
        (
            -Vec3::Z,
            [
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(max.x, max.y, min.z),
                Vec3::new(max.x, min.y, min.z),
            ],
        ),
    ];

    for (normal, positions) in faces {
        let base = vertices.len() as u32;
        vertices.extend(positions.map(|position| SprinklerVertex {
            position: position.to_array(),
            normal: normal.to_array(),
            color_srgb: color_srgb.to_array(),
        }));
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprinkler_mesh_has_stable_voxel_counts() {
        let (vertices, indices) = build_sprinkler_mesh();
        let voxel_count = PEDESTAL_VOXEL_COUNT + CAP_VOXEL_COUNT;

        assert_eq!(vertices.len(), voxel_count * 6 * 4);
        assert_eq!(indices.len(), voxel_count * 6 * 6);
        let max_y = vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert_eq!(max_y, 3.0 / 256.0);
    }

    #[test]
    fn sprinkler_vertex_layout_matches_shader_locations() {
        assert_eq!(std::mem::size_of::<SprinklerVertex>(), 9 * 4);
        assert_eq!(std::mem::size_of::<SprinklerInstanceGpu>(), 3 * 4);
    }
}
