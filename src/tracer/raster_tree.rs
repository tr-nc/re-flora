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
    pub axis_aligned: bool,
    pub vertices: Vec<RasterTreeVertex>,
    pub indices: Vec<u32>,
    cells: BTreeMap<[u32; 3], (Vec3, [bool; 6])>,
    pub solid_cells: std::collections::BTreeSet<[u32; 3]>,
    bindings: Vec<Option<(u32, SkinBinding)>>,
    pub cell_vertex_indices: Vec<u32>,
}

impl RasterTreeMesh {
    pub fn with_axis_aligned(axis_aligned: bool) -> Self {
        Self {
            axis_aligned,
            ..Self::default()
        }
    }
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
                    self.solid_cells.insert(cell.to_array());
                    let faces = NEIGHBORS.map(|n| !solid(cell.as_ivec3() + n));
                    if !self.axis_aligned && !faces.into_iter().any(|v| v) {
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
                    self.cells.insert(
                        cell.to_array(),
                        (normal, if self.axis_aligned { [true; 6] } else { faces }),
                    );
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
        self.cell_vertex_indices = vec![0; TREE_CELL_CAPACITY];
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
            self.cell_vertex_indices[slot] = base;
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
    /// Smooth mode shares corner bindings; axis-aligned mode shares one center
    /// binding per cube, intentionally allowing neighboring cubes to separate.
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
            let rest = if self.axis_aligned {
                vertex.center
            } else {
                vertex.position
            };
            let point = (Vec3::from_array(rest) - origin) * 256.;
            let key = rest.map(f32::to_bits);
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

    /// Compile resident GPU data only when topology/representation changes.
    /// Aligned blocks have one binding and one rest record, not eight copies.
    pub fn gpu_skin(&self) -> Result<GpuTreeSkin> {
        let group_size = if self.axis_aligned { 8 } else { 1 };
        ensure!(
            self.bindings.len() == self.vertices.len(),
            "missing GPU tree bindings"
        );
        ensure!(
            self.vertices.len() % group_size == 0,
            "incomplete GPU tree block"
        );
        let mut result = GpuTreeSkin::default();
        let mut palette = BTreeMap::new();
        for (vertices, bindings) in self
            .vertices
            .chunks_exact(group_size)
            .zip(self.bindings.chunks_exact(group_size))
        {
            let (tree, binding) =
                bindings[0].ok_or_else(|| anyhow::anyhow!("unbound GPU tree vertex"))?;
            ensure!(
                bindings.iter().all(|b| *b == bindings[0]),
                "inconsistent GPU block binding"
            );
            ensure!(
                binding.weight.is_finite() && (0. ..=1.).contains(&binding.weight),
                "invalid GPU skin weight"
            );
            let mut index = |branch| {
                *palette.entry((tree, branch)).or_insert_with(|| {
                    result.branches.push((tree, branch));
                    result.branches.len() as u32 // zero is the identity/root parent
                })
            };
            let child = index(binding.branch);
            let parent = binding.parent.map(&mut index).unwrap_or(0);
            result
                .bindings
                .push([child, parent, binding.weight.to_bits(), 0]);
            let vertex = vertices[0];
            result.rest.extend([
                Vec3::from(vertex.position).extend(1.).to_array(),
                Vec3::from(vertex.center).extend(1.).to_array(),
                Vec3::from(vertex.normal).extend(0.).to_array(),
            ]);
        }
        ensure!(
            result.branches.len() < super::tree_scene::MAX_TREE_VERTICES,
            "GPU tree pose capacity exceeded"
        );
        Ok(result)
    }

    pub fn posed_surface<'a>(
        &self,
        poses: impl Fn(u32) -> Option<&'a [BranchPose]>,
    ) -> Result<PosedTreeSurface> {
        ensure!(
            self.bindings.len() == self.vertices.len(),
            "missing tree bindings"
        );
        let mut positions = Vec::with_capacity(if self.axis_aligned {
            0
        } else {
            self.vertices.len()
        });

        let mut block_centers = self
            .axis_aligned
            .then(|| Vec::with_capacity(self.vertices.len() / 8));
        // Axis-aligned blocks share a center binding across all eight corners.
        // Smooth surfaces retain their per-corner bindings and normal transforms.
        let group_size = if self.axis_aligned { 8 } else { 1 };
        ensure!(
            self.vertices.len() % group_size == 0,
            "incomplete tree vertex group"
        );
        for (group, bindings) in self
            .vertices
            .chunks_exact(group_size)
            .zip(self.bindings.chunks_exact(group_size))
        {
            let vertex = &group[0];
            let (tree_id, binding) =
                bindings[0].ok_or_else(|| anyhow::anyhow!("unbound tree vertex"))?;
            let transform = binding
                .transform(poses(tree_id).ok_or_else(|| anyhow::anyhow!("tree pose missing"))?)?;
            if let Some(centers) = block_centers.as_mut() {
                debug_assert!(bindings.iter().all(|b| *b == bindings[0]));
                let center = Vec3::from_array(vertex.center);
                let posed_center = transform.point(center);
                centers.push(posed_center);
            } else {
                positions.push(transform.point(Vec3::from_array(vertex.position)));
            }
        }
        Ok(match block_centers {
            Some(centers) => PosedTreeSurface::Blocks { centers },
            None => PosedTreeSurface::Triangles { positions },
        })
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
            posed.vertex_count() == self.vertices.len(),
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
                ids.map(|i| posed.position(i)),
                ids.map(|i| Vec3::from_array(self.vertices[i].position)),
            ) {
                if nearest.is_none_or(|previous| hit.distance < previous.distance) {
                    nearest = Some(hit);
                }
            }
        }
        Ok(nearest)
    }

    pub fn max_displacement(&self, surface: &PosedTreeSurface) -> f32 {
        self.vertices
            .iter()
            .zip(surface.positions())
            .map(|(rest, posed)| Vec3::from(rest.position).distance(posed))
            .fold(0., f32::max)
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }
}

/// CPU interaction geometry is compact and representation-specific. Normals
/// belong to the GPU shading surface; physics needs no duplicate normal array.
pub enum PosedTreeSurface {
    Blocks { centers: Vec<Vec3> },
    Triangles { positions: Vec<Vec3> },
}

impl PosedTreeSurface {
    pub fn vertex_count(&self) -> usize {
        match self {
            Self::Blocks { centers } => centers.len() * 8,
            Self::Triangles { positions } => positions.len(),
        }
    }
    pub fn position(&self, index: usize) -> Vec3 {
        match self {
            Self::Blocks { centers } => {
                centers[index / 8] + (VOXEL_VERTICES[index % 8].as_vec3() - Vec3::splat(0.5)) / 256.
            }
            Self::Triangles { positions } => positions[index],
        }
    }
    pub fn positions(&self) -> impl Iterator<Item = Vec3> + '_ {
        (0..self.vertex_count()).map(|i| self.position(i))
    }
    pub fn is_finite(&self) -> bool {
        match self {
            Self::Blocks { centers } => centers.iter().all(|p| p.is_finite()),
            Self::Triangles { positions } => positions.iter().all(|p| p.is_finite()),
        }
    }
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

#[derive(Default)]
pub struct GpuTreeSkin {
    pub rest: Vec<[f32; 4]>,
    pub bindings: Vec<[u32; 4]>,
    pub branches: Vec<(u32, usize)>,
    pub poses: Vec<BranchPose>,
}

pub struct RasterTreeGeometry {
    pub skin: GpuTreeSkin,
    pub indices: Resource<Buffer>,
    pub index_count: u32,
    pub revision: Option<u32>,
    pub enabled: bool,
    pub color_draws: u64,
    pub rest_mesh: RasterTreeMesh,
    pub scene: super::tree_scene::TreeScene,
    pub attachments: Vec<super::tree_scene::TreeAttachment>,
    pub attachment_poses: Vec<BranchPose>,
    pub previous_attachment_poses: Vec<BranchPose>,
    pub attachment_dt: f32,
    pub posed_surface: Option<PosedTreeSurface>,
}

impl RasterTreeGeometry {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        Self {
            skin: GpuTreeSkin::default(),
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
            scene: super::tree_scene::TreeScene::default(),
            attachments: Vec::new(),
            attachment_poses: Vec::new(),
            previous_attachment_poses: Vec::new(),
            attachment_dt: 0.,
            posed_surface: None,
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
    pub fn raycast(&self, origin: Vec3, direction: Vec3) -> Option<SurfaceHit> {
        let posed = self.posed_surface.as_ref()?;
        let direction = direction.normalize_or_zero();
        let mut nearest: Option<SurfaceHit> = None;
        for index in self.scene.ray_candidates(origin, direction) {
            let triangle = self.scene.primitives[index as usize];
            if triangle[3] == 1 {
                let base = triangle[0] as usize;
                if let Some((distance, normal)) = super::tree_scene::intersect_box(
                    origin,
                    direction,
                    posed.position(base),
                    posed.position(triangle[1] as usize),
                ) {
                    if nearest.is_none_or(|hit: SurfaceHit| distance < hit.distance) {
                        let world_position = origin + direction * distance;
                        nearest = Some(SurfaceHit {
                            distance,
                            normal,
                            world_position,
                            rest_position: world_position - posed.position(base)
                                + Vec3::from(self.rest_mesh.vertices[base].position),
                        });
                    }
                }
                continue;
            }
            let ids = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            if let Some(hit) = intersect_surface_triangle(
                origin,
                direction,
                ids.map(|i| posed.position(i)),
                ids.map(|i| Vec3::from(self.rest_mesh.vertices[i].position)),
            ) {
                if nearest.is_none_or(|previous| hit.distance < previous.distance) {
                    nearest = Some(hit);
                }
            }
        }
        nearest
    }

    /// Hidden-app contract: compare every GPU result with the exact CPU surface
    /// retained for physics, including normals and the disabled-wind rest pose.
    pub fn validate_gpu_surface(&self, data: &[[f32; 4]]) -> Result<()> {
        ensure!(
            data.len() == self.rest_mesh.vertices.len() * 2,
            "GPU tree surface size mismatch"
        );
        let mut max_position_error = 0.0_f32;
        let mut max_normal_error = 0.0_f32;
        for (i, vertex) in self.rest_mesh.vertices.iter().enumerate() {
            let position = self
                .posed_surface
                .as_ref()
                .map_or(Vec3::from(vertex.position), |s| s.position(i));
            let normal = if self.posed_surface.is_some() && !self.rest_mesh.axis_aligned {
                let [branch, parent, weight, _] = self.skin.bindings[i];
                SkinBinding {
                    branch: branch as usize,
                    parent: Some(parent as usize),
                    weight: f32::from_bits(weight),
                }
                .transform(&self.skin.poses)?
                .normal(Vec3::from(vertex.normal))
            } else {
                Vec3::from(vertex.normal)
            };
            let gpu_position = Vec3::from_slice(&data[i * 2]);
            let gpu_normal = Vec3::from_slice(&data[i * 2 + 1]);
            ensure!(
                gpu_position.is_finite() && gpu_normal.is_finite(),
                "nonfinite GPU tree vertex {i}"
            );
            max_position_error = max_position_error.max(position.distance(gpu_position));
            max_normal_error = max_normal_error.max(normal.distance(gpu_normal));
        }
        ensure!(
            max_position_error < 2e-6 && max_normal_error < 2e-4,
            "GPU skin mismatch position={max_position_error} normal={max_normal_error}"
        );
        log::info!("[TREE][GPU_SKIN] vertices={} elements={} bones={} aligned={} active={} position_error={} normal_error={}",
            self.rest_mesh.vertices.len(), self.skin.bindings.len(), self.skin.branches.len(),
            self.rest_mesh.axis_aligned, self.posed_surface.is_some(), max_position_error, max_normal_error);
        Ok(())
    }

    /// Both color and shadow draws use the same resident surface. Aligned
    /// blocks instance a single cube; smooth mode retains its exposed-face topology.
    pub fn draw_counts(&self) -> (u32, u32) {
        if self.rest_mesh.axis_aligned {
            (
                CUBE_INDICES.len() as u32,
                self.rest_mesh.vertices.len() as u32 / 8,
            )
        } else {
            (self.index_count, 1)
        }
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
        let draw_indices = if mesh.axis_aligned {
            CUBE_INDICES.as_slice()
        } else {
            &mesh.indices
        };
        let indices = Self::buffer(
            device,
            allocator,
            vk::BufferUsageFlags::INDEX_BUFFER,
            std::mem::size_of_val(draw_indices),
        );
        if count > 0 {
            indices.fill(draw_indices)?;
        }
        self.skin = mesh.gpu_skin()?;
        self.indices = Resource::new(indices);
        let positions: Vec<_> = mesh
            .vertices
            .iter()
            .map(|v| Vec3::from(v.position))
            .collect();
        self.scene = if mesh.axis_aligned {
            let bounds: Vec<_> = (0..positions.len())
                .step_by(8)
                .map(|base| [base as u32, base as u32 + 6])
                .collect();
            super::tree_scene::TreeScene::boxes(&bounds, &positions)?
        } else {
            super::tree_scene::TreeScene::new(&mesh.indices, &positions)?
        };
        self.posed_surface = None;
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
    fn gpu_skin_compiles_one_record_per_block_and_deduplicates_tree_bones() {
        let mut mesh = RasterTreeMesh::with_axis_aligned(true);
        let binding = SkinBinding {
            branch: 2,
            parent: Some(1),
            weight: 0.25,
        };
        for tree in [7, 7, 9] {
            mesh.vertices
                .extend(VOXEL_VERTICES.map(|v| RasterTreeVertex {
                    position: (v.as_vec3() / 256.).to_array(),
                    center: [0.5 / 256.; 3],
                    normal: Vec3::Y.to_array(),
                }));
            mesh.bindings.extend([Some((tree, binding)); 8]);
        }
        let skin = mesh.gpu_skin().unwrap();
        assert_eq!(skin.rest.len(), 9);
        assert_eq!(skin.branches, [(7, 2), (7, 1), (9, 2), (9, 1)]);
        assert_eq!(
            skin.bindings,
            [
                [1, 2, 0.25_f32.to_bits(), 0],
                [1, 2, 0.25_f32.to_bits(), 0],
                [3, 4, 0.25_f32.to_bits(), 0]
            ]
        );
        mesh.axis_aligned = false;
        assert_eq!(mesh.gpu_skin().unwrap().bindings.len(), 24);
        mesh.bindings[0] = Some((
            7,
            SkinBinding {
                parent: None,
                ..binding
            },
        ));
        assert_eq!(mesh.gpu_skin().unwrap().bindings[0][1], 0);
        mesh.axis_aligned = true;
        assert!(mesh.gpu_skin().is_err()); // No silently mixed bindings within a block.
        mesh.bindings[0] = None;
        assert!(mesh.gpu_skin().is_err());
    }

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
        let positions: Vec<_> = surface.positions().collect();
        for (vertex, position) in mesh.vertices.iter().zip(&positions) {
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
        let posed = ids.map(|i| surface.position(i));
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
    fn axis_aligned_blocks_include_hidden_faces_and_translate_rigidly() {
        use crate::{
            tree_gen::{pose::TreePose, TreeDesc},
            wind_field::WindFieldFrame,
        };
        let tree = Tree::new(TreeDesc::default());
        let bytes = vec![5; 512];
        let mut mesh = RasterTreeMesh::with_axis_aligned(true);
        mesh.append_region(UVec3::ZERO, UVec3::splat(8), &bytes, tree.trunks())
            .unwrap();
        mesh.finish().unwrap();
        assert!(mesh.cell_count() > 0);
        assert_eq!(mesh.cell_count(), mesh.solid_cells.len());
        assert_eq!(mesh.indices.len(), mesh.cell_count() * 36);
        mesh.bind_tree(17, Vec3::ZERO, &tree).unwrap();
        let mut pose = TreePose::new(tree.branches(), Vec3::ZERO).unwrap();
        for _ in 0..120 {
            pose.advance(&WindFieldFrame::uniform(glam::Vec2::X * 8.), 1. / 60.)
                .unwrap();
        }
        let surface = mesh.posed_surface(|_| Some(pose.branches())).unwrap();
        assert!(mesh.max_displacement(&surface) > 0.);
        let positions: Vec<_> = surface.positions().collect();
        let PosedTreeSurface::Blocks { centers } = &surface else {
            panic!("expected compact boxes")
        };
        assert_eq!(centers.len() * 8, positions.len());
        // Equivalence to the old independent per-vertex path protects batching
        // across neighboring cells with different skeleton attachments.
        for ((vertex, binding), position) in
            mesh.vertices.iter().zip(&mesh.bindings).zip(&positions)
        {
            let (_, binding) = binding.unwrap();
            let transform = binding.transform(pose.branches()).unwrap();
            let center = Vec3::from(vertex.center);
            assert!(position.abs_diff_eq(
                Vec3::from(vertex.position) + (transform.point(center) - center),
                1e-7
            ));
        }

        for (rest, posed) in mesh.vertices.chunks_exact(8).zip(positions.chunks_exact(8)) {
            let offset = posed[0] - Vec3::from(rest[0].position);
            for (v, p) in rest.iter().zip(posed) {
                assert!((*p - Vec3::from(v.position)).abs_diff_eq(offset, 1e-7));
            }
        }
        let top = positions.iter().max_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
        let origin = *top + Vec3::new(-0.001, 0.1, -0.001);
        // Query a cube directly as well as all faces: barycentric mapping remains
        // a translation back into the authoritative editable rest volume.
        let ids = &mesh.indices[0..3];
        let target = ids
            .iter()
            .map(|&i| surface.position(i as usize))
            .sum::<Vec3>()
            / 3.;
        let offset =
            surface.position(ids[0] as usize) - Vec3::from(mesh.vertices[ids[0] as usize].position);
        let hit = intersect_surface_triangle(
            target - Vec3::Y * 0.001,
            Vec3::Y,
            ids.iter()
                .map(|&i| surface.position(i as usize))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            ids.iter()
                .map(|&i| Vec3::from(mesh.vertices[i as usize].position))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        )
        .unwrap();
        assert!(hit
            .rest_position
            .abs_diff_eq(hit.world_position - offset, 1e-6));
        assert!(mesh.raycast(&surface, origin, -Vec3::Y).is_ok());
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
