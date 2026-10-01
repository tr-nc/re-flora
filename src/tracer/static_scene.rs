//! Immutable, conventional triangle meshes for fixed scene objects.
//! Uploaded only during scene setup (the caller must establish GPU idleness).
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Vec3, Vec4};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct StaticSceneVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

#[derive(Default)]
pub struct StaticSceneMesh {
    pub vertices: Vec<StaticSceneVertex>,
    pub indices: Vec<u32>,
}

impl StaticSceneMesh {
    /// Boxes are authored model primitives, not voxel occupancy or micro-cube instances.
    pub fn append_box(&mut self, min: Vec3, max: Vec3, color: Vec4) {
        let corners = [
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(min.x, max.y, max.z),
        ];
        for (face, normal) in [
            ([1, 2, 6, 5], Vec3::X),
            ([4, 7, 3, 0], Vec3::NEG_X),
            ([3, 7, 6, 2], Vec3::Y),
            ([4, 0, 1, 5], Vec3::NEG_Y),
            ([5, 6, 7, 4], Vec3::Z),
            ([0, 3, 2, 1], Vec3::NEG_Z),
        ] {
            let base = self.vertices.len() as u32;
            self.vertices.extend(face.map(|i| StaticSceneVertex {
                position: corners[i].to_array(),
                normal: normal.to_array(),
                color: color.to_array(),
            }));
            self.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    pub fn collision_geometry(&self) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        (
            self.vertices
                .iter()
                .map(|v| Vec3::from_array(v.position))
                .collect(),
            self.indices
                .chunks_exact(3)
                .map(|t| [t[0], t[1], t[2]])
                .collect(),
        )
    }
}

pub(super) struct StaticSceneResources {
    pub vertices: Buffer,
    pub indices: Buffer,
    pub index_count: u32,
    /// First index and centroid for translucent triangles, sorted per view at draw time.
    pub translucent_triangles: Vec<(u32, Vec3)>,
}

impl StaticSceneResources {
    pub fn new(device: Device, allocator: Allocator, mesh: &StaticSceneMesh) -> Result<Self> {
        ensure!(
            !mesh.vertices.is_empty() && !mesh.indices.is_empty(),
            "empty static scene mesh"
        );
        ensure!(
            mesh.indices.len() <= u32::MAX as usize && mesh.indices.len().is_multiple_of(3),
            "invalid static scene triangle count"
        );
        ensure!(
            mesh.indices
                .iter()
                .all(|&i| (i as usize) < mesh.vertices.len()),
            "invalid static scene index"
        );
        ensure!(
            mesh.vertices
                .iter()
                .all(|v| Vec3::from_array(v.position).is_finite()
                    && Vec3::from_array(v.normal).is_finite()
                    && Vec4::from_array(v.color).is_finite()),
            "non-finite static scene vertex"
        );
        // Keep opaque depth-writing geometry first; glass must blend after it.
        let mut sorted_indices = Vec::with_capacity(mesh.indices.len());
        let mut glass = Vec::new();
        for triangle in mesh.indices.chunks_exact(3) {
            if triangle
                .iter()
                .any(|&i| mesh.vertices[i as usize].color[3] < 1.0)
            {
                glass.push(triangle);
            } else {
                sorted_indices.extend_from_slice(triangle);
            }
        }
        let index_count = sorted_indices.len() as u32;
        let mut translucent_triangles = Vec::with_capacity(glass.len());
        for triangle in glass {
            let center = triangle
                .iter()
                .map(|&i| Vec3::from_array(mesh.vertices[i as usize].position))
                .sum::<Vec3>()
                / 3.0;
            translucent_triangles.push((sorted_indices.len() as u32, center));
            sorted_indices.extend_from_slice(triangle);
        }
        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of_val(mesh.vertices.as_slice()) as u64,
        );
        let indices = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of_val(mesh.indices.as_slice()) as u64,
        );
        vertices.fill(&mesh.vertices)?;
        indices.fill(&sorted_indices)?;
        Ok(Self {
            vertices,
            indices,
            index_count,
            translucent_triangles,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boxes_are_outward_wound_flat_shaded_triangles() {
        let mut mesh = StaticSceneMesh::default();
        mesh.append_box(Vec3::ZERO, Vec3::ONE, Vec4::ONE);
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
        for tri in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| mesh.vertices[i as usize]);
            let cross = (Vec3::from_array(b.position) - Vec3::from_array(a.position))
                .cross(Vec3::from_array(c.position) - Vec3::from_array(a.position));
            assert!(cross.dot(Vec3::from_array(a.normal)) > 0.0);
        }
        assert_eq!(std::mem::size_of::<StaticSceneVertex>(), 40);
    }
}
