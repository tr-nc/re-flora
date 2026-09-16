//! Static raster comparison compiled from published terrain, never from guessed occupancy.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, UVec3, Vec3};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use std::collections::BTreeMap;

use super::voxel_geometry::{CUBE_INDICES, VOXEL_VERTICES};
use crate::{
    geom::RoundCone,
    resource::Resource,
    tree_gen::{
        pose::BranchPose,
        skin::{intersect_surface_triangle, SkinBinding, SurfaceHit},
        Tree,
    },
};

pub const TREE_CELL_CAPACITY: usize = 1 << 18;
const NEIGHBORS: [IVec3; 6] = [
    IVec3::NEG_Y,
    IVec3::Y,
    IVec3::NEG_Z,
    IVec3::Z,
    IVec3::NEG_X,
    IVec3::X,
];

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RasterTreeVertex {
    position: [f32; 3],
    center: [f32; 3],
    normal: [f32; 3],
}

#[derive(Clone, Default)]
pub struct RasterTreeMesh {
    pub vertices: Vec<RasterTreeVertex>,
    pub indices: Vec<u32>,
    cells: BTreeMap<[u32; 3], (Vec3, [bool; 6])>,
    bindings: Vec<Option<(u32, SkinBinding)>>,
}

impl RasterTreeMesh {
    /// `bytes` includes a two-voxel halo for the same radius-two normal estimator as terrain.
    pub fn append_region(
        &mut self,
        origin: UVec3,
        dim: UVec3,
        bytes: &[u8],
        cones: &[RoundCone],
    ) -> Result<()> {
        ensure!(
            bytes.len() == dim.as_u64vec3().element_product() as usize,
            "tree atlas region size mismatch"
        );
        let sample = |world: IVec3| -> u8 {
            let p = world - origin.as_ivec3();
            if p.cmplt(IVec3::ZERO).any() || p.cmpge(dim.as_ivec3()).any() {
                return 0;
            }
            bytes[(p.x as u32 + dim.x * (p.y as u32 + dim.y * p.z as u32)) as usize] & 15
        };
        // Same solid predicate as surface_extraction.slang.
        let solid = |p: IVec3| sample(p) != 0;
        for z in 0..dim.z {
            for y in 0..dim.y {
                for x in 0..dim.x {
                    let cell = origin + UVec3::new(x, y, z);
                    let center = cell.as_vec3() + Vec3::splat(0.5);
                    if sample(cell.as_ivec3()) != 5
                        || !cones.iter().any(|c| c.signed_distance(center) < 0.0)
                    {
                        continue;
                    }
                    let faces = NEIGHBORS.map(|n| !solid(cell.as_ivec3() + n));
                    if !faces.into_iter().any(|v| v) {
                        continue;
                    }
                    let mut moment = IVec3::ZERO;
                    for dz in -2..=2 {
                        for dy in -2..=2 {
                            for dx in -2..=2 {
                                let offset = IVec3::new(dx, dy, dz);
                                if solid(cell.as_ivec3() + offset) {
                                    moment += offset;
                                }
                            }
                        }
                    }
                    let normal = if moment == IVec3::ZERO {
                        Vec3::Y
                    } else {
                        -moment.as_vec3().normalize()
                    };
                    self.cells.insert(cell.to_array(), (normal, faces));
                }
            }
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Vec<[u32; 4]>> {
        ensure!(
            self.cells.len() < TREE_CELL_CAPACITY / 2,
            "static tree comparison exceeds {} surface cells",
            TREE_CELL_CAPACITY / 2
        );
        let mut table = vec![[0u32; 4]; TREE_CELL_CAPACITY];
        self.vertices.clear();
        self.indices.clear();
        for (&cell, &(normal, faces)) in &self.cells {
            let hash = tree_cell_hash(cell) as usize;
            let slot = (0..64)
                .map(|i| (hash + i) & (TREE_CELL_CAPACITY - 1))
                .find(|&s| table[s][3] == 0);
            let slot = slot
                .ok_or_else(|| anyhow::anyhow!("static tree cell lookup probe budget exhausted"))?;
            table[slot] = [cell[0], cell[1], cell[2], pack_normal_oct16(normal) + 1];
            let min = UVec3::from_array(cell).as_vec3();
            let base = u32::try_from(self.vertices.len())?;
            self.vertices
                .extend(VOXEL_VERTICES.map(|v| RasterTreeVertex {
                    position: ((min + v.as_vec3()) / 256.0).to_array(),
                    center: ((min + Vec3::splat(0.5)) / 256.0).to_array(),
                    normal: normal.to_array(),
                }));
            for (face, visible) in faces.into_iter().enumerate() {
                if visible {
                    self.indices.extend(
                        CUBE_INDICES[face * 6..face * 6 + 6]
                            .iter()
                            .map(|&i| base + i),
                    );
                }
            }
        }
        self.bindings = vec![None; self.vertices.len()];
        Ok(table)
    }
    /// Coincident voxel corners bind from the same rest coordinate. Never choose
    /// a joint independently from each voxel center, which would open cracks.
    pub fn bind_tree(&mut self, tree_id: u32, origin: Vec3, tree: &Tree) -> Result<()> {
        ensure!(
            self.bindings.len() == self.vertices.len(),
            "finish tree mesh before binding"
        );
        let mut corners = BTreeMap::new();
        for (vertex, binding) in self.vertices.iter().zip(&mut self.bindings) {
            if binding.is_some() {
                continue;
            }
            let center = (Vec3::from_array(vertex.center) - origin) * 256.;
            if !tree.trunks().iter().any(|c| c.signed_distance(center) < 0.) {
                continue;
            }
            let point = (Vec3::from_array(vertex.position) - origin) * 256.;
            let key = vertex.position.map(f32::to_bits);
            let skin = if let Some(skin) = corners.get(&key) {
                *skin
            } else {
                let skin = SkinBinding::at_rest_position(tree, point)?;
                corners.insert(key, skin);
                skin
            };
            *binding = Some((tree_id, skin));
        }
        Ok(())
    }

    pub fn posed_surface<'a>(
        &self,
        poses: impl Fn(u32) -> Option<&'a [BranchPose]>,
    ) -> Result<PosedTreeSurface> {
        ensure!(
            self.bindings.len() == self.vertices.len(),
            "missing tree bindings"
        );
        let mut positions = Vec::with_capacity(self.vertices.len());
        let mut normals = Vec::with_capacity(self.vertices.len());
        for (vertex, binding) in self.vertices.iter().zip(&self.bindings) {
            let (tree_id, binding) =
                binding.ok_or_else(|| anyhow::anyhow!("unbound tree vertex"))?;
            let transform = binding
                .transform(poses(tree_id).ok_or_else(|| anyhow::anyhow!("tree pose missing"))?)?;
            positions.push(transform.point(Vec3::from_array(vertex.position)));
            normals.push(transform.normal(Vec3::from_array(vertex.normal)));
        }
        Ok(PosedTreeSurface { positions, normals })
    }

    /// Exact surface query used to validate rest-coordinate editing. The future
    /// scene acceleration structure must preserve this barycentric hit contract.
    pub fn raycast(
        &self,
        posed: &PosedTreeSurface,
        origin: Vec3,
        direction: Vec3,
    ) -> Result<Option<SurfaceHit>> {
        ensure!(
            posed.positions.len() == self.vertices.len(),
            "surface topology mismatch"
        );
        let mut nearest: Option<SurfaceHit> = None;
        for triangle in self.indices.chunks_exact(3) {
            let ids = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            if let Some(hit) = intersect_surface_triangle(
                origin,
                direction,
                ids.map(|i| posed.positions[i]),
                ids.map(|i| Vec3::from_array(self.vertices[i].position)),
            ) {
                if nearest.is_none_or(|previous| hit.distance < previous.distance) {
                    nearest = Some(hit);
                }
            }
        }
        Ok(nearest)
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }
}

pub struct PosedTreeSurface {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
}

fn pack_normal_oct16(normal: Vec3) -> u32 {
    let p = normal / normal.abs().element_sum().max(1e-8);
    let mut x = p.x;
    let mut y = p.y;
    if p.z < 0.0 {
        x = (1.0 - p.y.abs()) * if p.x >= 0.0 { 1.0 } else { -1.0 };
        y = (1.0 - p.x.abs()) * if p.y >= 0.0 { 1.0 } else { -1.0 };
    }
    let q = |v: f32| ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u32;
    q(x) | (q(y) << 8)
}

pub fn tree_cell_hash(p: [u32; 3]) -> u32 {
    let h = p[0].wrapping_mul(73856093) ^ p[1].wrapping_mul(19349663) ^ p[2].wrapping_mul(83492791);
    h ^ (h >> 16)
}

pub struct RasterTreeGeometry {
    pub vertices: Resource<Buffer>,
    pub shadow_vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub index_count: u32,
    pub revision: Option<u32>,
    pub enabled: bool,
    pub color_draws: u64,
    pub rest_mesh: RasterTreeMesh,
}

impl RasterTreeGeometry {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        Self {
            vertices: Resource::new(Self::buffer(
                device.clone(),
                allocator.clone(),
                vk::BufferUsageFlags::VERTEX_BUFFER,
                4,
            )),
            shadow_vertices: Resource::new(Self::buffer(
                device.clone(),
                allocator.clone(),
                vk::BufferUsageFlags::VERTEX_BUFFER,
                4,
            )),
            indices: Resource::new(Self::buffer(
                device,
                allocator,
                vk::BufferUsageFlags::INDEX_BUFFER,
                4,
            )),
            index_count: 0,
            revision: None,
            enabled: false,
            color_draws: 0,
            rest_mesh: RasterTreeMesh::default(),
        }
    }
    fn buffer(
        device: Device,
        allocator: Allocator,
        usage: vk::BufferUsageFlags,
        bytes: usize,
    ) -> Buffer {
        Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(usage),
            MemoryLocation::CpuToGpu,
            bytes.max(4) as u64,
        )
    }
    /// Caller has quiesced frames before atlas readback and replacement.
    pub fn upload(
        &mut self,
        device: Device,
        allocator: Allocator,
        mesh: &RasterTreeMesh,
        revision: u32,
    ) -> Result<()> {
        let count = u32::try_from(mesh.indices.len())?;
        let vertices = Self::buffer(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            std::mem::size_of_val(mesh.vertices.as_slice()),
        );
        let shadow_positions = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position)
            .collect::<Vec<_>>();
        let shadow_vertices = Self::buffer(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            std::mem::size_of_val(shadow_positions.as_slice()),
        );
        let indices = Self::buffer(
            device,
            allocator,
            vk::BufferUsageFlags::INDEX_BUFFER,
            std::mem::size_of_val(mesh.indices.as_slice()),
        );
        if count > 0 {
            vertices.fill(&mesh.vertices)?;
            shadow_vertices.fill(&shadow_positions)?;
            indices.fill(&mesh.indices)?;
        }
        self.vertices = Resource::new(vertices);
        self.shadow_vertices = Resource::new(shadow_vertices);
        self.indices = Resource::new(indices);
        self.rest_mesh = mesh.clone();
        self.index_count = count;
        self.revision = Some(revision);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adjacent_voxels_have_no_internal_faces_and_regions_deduplicate() {
        let dim = UVec3::splat(8);
        let mut bytes = vec![0; 512];
        for x in [3, 4] {
            bytes[x + 8 * (3 + 8 * 3)] = 5;
        }
        let cones = [RoundCone::new(4.0, Vec3::splat(3.5), 4.0, Vec3::splat(3.5))];
        let mut mesh = RasterTreeMesh::default();
        for _ in 0..2 {
            mesh.append_region(UVec3::ZERO, dim, &bytes, &cones)
                .unwrap();
        }
        let table = mesh.finish().unwrap();
        assert_eq!(mesh.cell_count(), 2);
        assert_eq!(mesh.vertices.len(), 16);
        assert_eq!(mesh.indices.len(), 10 * 6);
        assert_eq!(table.iter().filter(|e| e[3] != 0).count(), 2);
        // An edit removing a tree voxel is read from the published atlas, not regenerated from cones.
        bytes[4 + 8 * (3 + 8 * 3)] = 0;
        let mut edited = RasterTreeMesh::default();
        edited
            .append_region(UVec3::ZERO, dim, &bytes, &cones)
            .unwrap();
        edited.finish().unwrap();
        assert_eq!(edited.cell_count(), 1);
        assert_eq!(edited.indices.len(), 36);
    }
    #[test]
    fn neighboring_blocks_share_posed_corners_and_hits_recover_rest_surface() {
        use crate::{
            tree_gen::{pose::TreePose, TreeDesc},
            wind_field::WindFieldFrame,
        };
        use glam::Vec2;
        let tree = Tree::new(TreeDesc::default());
        let mut bytes = vec![0; 512];
        for x in [3, 4] {
            bytes[x + 8 * (3 + 8 * 3)] = 5;
        }
        let mut mesh = RasterTreeMesh::default();
        mesh.append_region(UVec3::ZERO, UVec3::splat(8), &bytes, tree.trunks())
            .unwrap();
        mesh.finish().unwrap();
        assert_eq!(mesh.cell_count(), 2);
        mesh.bind_tree(17, Vec3::ZERO, &tree).unwrap();
        let mut pose = TreePose::new(tree.branches(), Vec3::ZERO).unwrap();
        for _ in 0..120 {
            pose.advance(&WindFieldFrame::uniform(Vec2::X * 8.), 1. / 60.)
                .unwrap();
        }
        let surface = mesh
            .posed_surface(|id| (id == 17).then_some(pose.branches()))
            .unwrap();
        let mut corners = BTreeMap::new();
        let mut shared = 0;
        for (vertex, position) in mesh.vertices.iter().zip(&surface.positions) {
            let key = vertex.position.map(f32::to_bits);
            if let Some(previous) = corners.insert(key, *position) {
                assert_eq!(previous, *position);
                shared += 1;
            }
        }
        assert_eq!(shared, 4);
        let ids = [
            mesh.indices[0] as usize,
            mesh.indices[1] as usize,
            mesh.indices[2] as usize,
        ];
        let posed = ids.map(|i| surface.positions[i]);
        let target = (posed[0] + posed[1] + posed[2]) / 3.;
        let normal = (posed[1] - posed[0]).cross(posed[2] - posed[0]).normalize();
        let hit = mesh
            .raycast(&surface, target + normal * 0.001, -normal)
            .unwrap()
            .unwrap();
        let rest = ids.map(|i| Vec3::from_array(mesh.vertices[i].position));
        assert!(
            hit.rest_position
                .distance((rest[0] + rest[1] + rest[2]) / 3.)
                < 1e-6
        );
        assert!(hit.world_position.distance(target) < 1e-6);
    }

    #[test]
    fn other_wood_and_surrounding_terrain_are_not_replaced() {
        let mut bytes = vec![0; 512];
        bytes[3 + 8 * (3 + 8 * 3)] = 5;
        bytes[4 + 8 * (3 + 8 * 3)] = 2;
        bytes[7 + 8 * (3 + 8 * 3)] = 5;
        let mut mesh = RasterTreeMesh::default();
        mesh.append_region(
            UVec3::ZERO,
            UVec3::splat(8),
            &bytes,
            &[RoundCone::new(0.6, Vec3::splat(3.5), 0.6, Vec3::splat(3.5))],
        )
        .unwrap();
        mesh.finish().unwrap();
        assert_eq!(mesh.cell_count(), 1);
        assert_eq!(mesh.indices.len(), 30);
    }
}
