//! Static raster comparison compiled from published terrain, never from guessed occupancy.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, UVec3, Vec3};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use std::{collections::BTreeMap, sync::Arc};

use super::voxel_geometry::{CUBE_INDICES, VOXEL_VERTICES};
use crate::{
    geom::{RoundCone, RoundConeClearanceIndex},
    resource::Resource,
    tree_gen::{
        pose::BranchPose,
        skin::{intersect_surface_triangle, RestSkinBinder, SkinBinding, SurfaceHit},
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
    normal_confidence: f32,
}

/// Rest-space bindings depend on the immutable authored tree and its placement,
/// not on terrain occupancy, exposed faces, normals, or the current wind pose.
/// Keep only corners used by this mesh, so edits cannot accumulate an unbounded history.
#[derive(Clone)]
struct TreeBindingCache {
    tree: Arc<Tree>,
    origin: Vec3,
    corners: BTreeMap<[u32; 3], SkinBinding>,
}

#[derive(Clone, Default)]
pub struct RasterTreeMesh {
    pub vertices: Vec<RasterTreeVertex>,
    pub indices: Vec<u32>,
    cells: BTreeMap<[u32; 3], (Vec3, f32, [bool; 6])>,
    pub solid_cells: std::collections::BTreeSet<[u32; 3]>,
    bindings: Vec<Option<(u32, SkinBinding)>>,
    binding_cache: BTreeMap<u32, TreeBindingCache>,
    pub cell_vertex_indices: Vec<u32>,
}

impl RasterTreeMesh {
    /// Exact equality of the facts consumed by rendering, queries, and physics.
    /// Binding memoization is not an observable fact. Never use a fingerprint or
    /// just topology counts here: occupancy and normal-only changes also matter.
    pub(super) fn same_surface(&self, other: &Self) -> bool {
        bytemuck::cast_slice::<_, u8>(&self.vertices)
            == bytemuck::cast_slice::<_, u8>(&other.vertices)
            && self.indices == other.indices
            && self.solid_cells == other.solid_cells
            && self.bindings == other.bindings
            && self.cell_vertex_indices == other.cell_vertex_indices
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
        let wood = RoundConeClearanceIndex::new(cones);
        for z in 0..dim.z {
            for y in 0..dim.y {
                for x in 0..dim.x {
                    let cell = origin + UVec3::new(x, y, z);
                    let center = cell.as_vec3() + Vec3::splat(0.5);
                    if sample(cell.as_ivec3()) != 5 || wood.has_minimum_clearance(center, 0.0) {
                        continue;
                    }
                    self.solid_cells.insert(cell.to_array());
                    let faces = NEIGHBORS.map(|n| !solid(cell.as_ivec3() + n));
                    if !faces.into_iter().any(|v| v) {
                        continue;
                    }
                    let mut estimate = super::voxel_normal::OccupancyNormal::default();
                    for dz in -2..=2 {
                        for dy in -2..=2 {
                            for dx in -2..=2 {
                                let offset = IVec3::new(dx, dy, dz);
                                if solid(cell.as_ivec3() + offset) {
                                    estimate.add(offset);
                                }
                            }
                        }
                    }
                    let (normal, confidence) = estimate.finish();
                    self.cells
                        .insert(cell.to_array(), (normal, confidence, faces));
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
        self.cell_vertex_indices = vec![u32::MAX; TREE_CELL_CAPACITY];
        self.vertices.clear();
        self.indices.clear();
        for (&cell, &(normal, confidence, faces)) in &self.cells {
            let hash = tree_cell_hash(cell) as usize;
            let slot = (0..64)
                .map(|i| (hash + i) & (TREE_CELL_CAPACITY - 1))
                .find(|&s| table[s][3] == 0);
            let slot = slot
                .ok_or_else(|| anyhow::anyhow!("static tree cell lookup probe budget exhausted"))?;
            // Low 17 bits preserve the normal+1 empty sentinel; next six bits
            // identify exposed faces in CUBE_INDICES order. Confidence stays f32
            // in the resident normal's spare W component, without another buffer.
            let face_mask = faces
                .iter()
                .enumerate()
                .fold(0u32, |mask, (i, &visible)| mask | (u32::from(visible) << i));
            table[slot] = [
                cell[0],
                cell[1],
                cell[2],
                (pack_normal_oct16(normal) + 1) | (face_mask << 17),
            ];
            let min = UVec3::from_array(cell).as_vec3();
            let base = u32::try_from(self.vertices.len())?;
            self.cell_vertex_indices[slot] = base;
            self.vertices
                .extend(VOXEL_VERTICES.map(|v| RasterTreeVertex {
                    position: ((min + v.as_vec3()) / 256.0).to_array(),
                    center: ((min + Vec3::splat(0.5)) / 256.0).to_array(),
                    normal: normal.to_array(),
                    normal_confidence: confidence,
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
    /// Shared corners use identical bindings so neighboring cells remain connected.
    /// Reuse only the previous mesh's bindings for the exact same immutable tree
    /// and placement. Ownership is still checked against the current tree in caller
    /// order, including overlapping trees; terrain faces/normals are always rebuilt.
    pub fn bind_tree(
        &mut self,
        tree_id: u32,
        origin: Vec3,
        tree: &Arc<Tree>,
        previous: Option<&Self>,
    ) -> Result<()> {
        ensure!(
            self.bindings.len() == self.vertices.len(),
            "finish tree mesh before binding"
        );
        let previous = previous
            .and_then(|mesh| mesh.binding_cache.get(&tree_id))
            .filter(|cache| Arc::ptr_eq(&cache.tree, tree) && cache.origin == origin);
        let binder = RestSkinBinder::new(tree);
        let mut corners = BTreeMap::new();
        let mut cell_owner = None;
        for (vertex, binding) in self.vertices.iter().zip(&mut self.bindings) {
            if binding.is_some() {
                continue;
            }
            let center_key = vertex.center.map(f32::to_bits);
            let owns_cell = match cell_owner {
                Some((key, owns)) if key == center_key => owns,
                _ => {
                    let center = (Vec3::from_array(vertex.center) - origin) * 256.;
                    let owns = binder.owns_cell(center);
                    cell_owner = Some((center_key, owns));
                    owns
                }
            };
            if !owns_cell {
                continue;
            }
            let rest = vertex.position;
            let point = (Vec3::from_array(rest) - origin) * 256.;
            let key = rest.map(f32::to_bits);
            let skin = if let Some(skin) = corners.get(&key) {
                *skin
            } else {
                let skin = match previous.and_then(|cache| cache.corners.get(&key)) {
                    Some(skin) => *skin,
                    None => binder.bind(point)?,
                };
                corners.insert(key, skin);
                skin
            };
            *binding = Some((tree_id, skin));
        }
        self.binding_cache.insert(
            tree_id,
            TreeBindingCache {
                tree: Arc::clone(tree),
                origin,
                corners,
            },
        );
        Ok(())
    }

    /// Compile resident per-vertex GPU data only when topology changes.
    pub fn gpu_skin(&self) -> Result<GpuTreeSkin> {
        ensure!(
            self.bindings.len() == self.vertices.len(),
            "missing GPU tree bindings"
        );
        let mut result = GpuTreeSkin::default();
        let mut palette = BTreeMap::new();
        for (vertex, binding) in self.vertices.iter().zip(&self.bindings) {
            let (tree, binding) =
                binding.ok_or_else(|| anyhow::anyhow!("unbound GPU tree vertex"))?;
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
            result.rest.extend([
                Vec3::from(vertex.position).extend(1.).to_array(),
                Vec3::from(vertex.center).extend(1.).to_array(),
                Vec3::from(vertex.normal)
                    .extend(vertex.normal_confidence)
                    .to_array(),
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
        let mut positions = Vec::with_capacity(self.vertices.len());
        for (vertex, binding) in self.vertices.iter().zip(&self.bindings) {
            let (tree_id, binding) =
                binding.ok_or_else(|| anyhow::anyhow!("unbound tree vertex"))?;
            let transform = binding
                .transform(poses(tree_id).ok_or_else(|| anyhow::anyhow!("tree pose missing"))?)?;
            positions.push(transform.point(Vec3::from_array(vertex.position)));
        }
        Ok(PosedTreeSurface { positions })
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

    /// Exposed on both sides of at least two axes: actual published one-voxel
    /// cross sections (including isolated cells), not inferred from cone radii.
    pub fn single_voxel_cross_sections(&self) -> usize {
        self.cells
            .values()
            .filter(|(_, _, faces)| {
                faces
                    .chunks_exact(2)
                    .filter(|pair| pair[0] && pair[1])
                    .count()
                    >= 2
            })
            .count()
    }

    /// Deterministic same-platform A/B identity of the compiled rest mesh,
    /// including normals/confidence; excludes changing wind pose and lighting.
    pub fn rest_fingerprint(&self) -> u64 {
        bytemuck::cast_slice::<_, u8>(&self.vertices)
            .iter()
            .chain(bytemuck::cast_slice::<_, u8>(&self.indices))
            .fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            })
    }

    pub fn confidence_counts(&self) -> [usize; 3] {
        let mut counts = [0; 3];
        for &(_, confidence, _) in self.cells.values() {
            counts[if confidence == 0. {
                0
            } else if confidence == 1. {
                2
            } else {
                1
            }] += 1;
        }
        counts
    }

    /// Smoke-only validation of the actual compute output, not just CPU metadata.
    pub fn validate_lighting_cache(&self, data: &[[f32; 4]], hybrid: bool) -> Result<()> {
        ensure!(
            data.len() == TREE_CELL_CAPACITY,
            "tree lighting cache size mismatch"
        );
        for (slot, &base) in self.cell_vertex_indices.iter().enumerate() {
            if base == u32::MAX {
                continue;
            }
            let value = data[slot];
            ensure!(
                value.iter().all(|v| v.is_finite() && *v >= 0.),
                "invalid tree irradiance/confidence at slot {slot}: {value:?}"
            );
            let expected = if hybrid {
                self.vertices[base as usize].normal_confidence
            } else {
                1.
            };
            ensure!(
                (value[3] - expected).abs() < 1e-6,
                "tree lighting toggle/metadata mismatch at {slot}: {} != {expected}",
                value[3]
            );
        }
        Ok(())
    }
}

/// Exact CPU interaction geometry. Normals belong to the GPU shading surface;
/// physics needs no duplicate normal array.
pub struct PosedTreeSurface {
    positions: Vec<Vec3>,
}

impl PosedTreeSurface {
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }
    pub fn position(&self, index: usize) -> Vec3 {
        self.positions[index]
    }
    pub fn positions(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.positions.iter().copied()
    }
    pub fn is_finite(&self) -> bool {
        self.positions.iter().all(|p| p.is_finite())
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
    // A failed publication must not make a later retry look like an unchanged surface.
    pub(super) publication_valid: bool,
    pub(crate) source: super::tree_surface_cache::TreeSurfaceCache,
    pub enabled: bool,
    pub color_draws: u64,
    pub rest_mesh: RasterTreeMesh,
    pub scene: super::tree_scene::TreeScene,
    pub refit: super::tree_scene::TreeRefitSchedule,
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
            publication_valid: false,
            source: super::tree_surface_cache::TreeSurfaceCache::default(),
            enabled: false,
            color_draws: 0,
            rest_mesh: RasterTreeMesh::default(),
            scene: super::tree_scene::TreeScene::default(),
            refit: super::tree_scene::TreeRefitSchedule::default(),
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
    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        candidates: impl IntoIterator<Item = u32>,
    ) -> Option<SurfaceHit> {
        let posed = self.posed_surface.as_ref()?;
        let direction = direction.normalize_or_zero();
        let mut nearest: Option<SurfaceHit> = None;
        for index in candidates {
            let triangle = self.scene.primitives[index as usize];
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
            let normal = if self.posed_surface.is_some() {
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
            ensure!(
                data[i * 2 + 1][3] == vertex.normal_confidence,
                "GPU tree confidence changed under skinning at vertex {i}"
            );
            max_position_error = max_position_error.max(position.distance(gpu_position));
            max_normal_error = max_normal_error.max(normal.distance(gpu_normal));
        }
        ensure!(
            max_position_error < 2e-6 && max_normal_error < 2e-4,
            "GPU skin mismatch position={max_position_error} normal={max_normal_error}"
        );
        log::info!("[TREE][GPU_SKIN] vertices={} elements={} bones={} active={} position_error={} normal_error={}",
            self.rest_mesh.vertices.len(), self.skin.bindings.len(), self.skin.branches.len(),
            self.posed_surface.is_some(), max_position_error, max_normal_error);
        Ok(())
    }

    /// Both color and shadow draws use the same resident exposed-face surface.
    pub fn draw_counts(&self) -> (u32, u32) {
        (self.index_count, 1)
    }

    /// Caller has quiesced frames before atlas readback and replacement.
    pub fn upload(
        &mut self,
        device: Device,
        allocator: Allocator,
        mesh: &RasterTreeMesh,
    ) -> Result<()> {
        let count = u32::try_from(mesh.indices.len())?;
        let draw_indices = mesh.indices.as_slice();
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
        self.scene = super::tree_scene::TreeScene::new(&mesh.indices, &positions)?;
        self.refit = self.scene.refit_schedule();
        self.posed_surface = None;
        self.rest_mesh = mesh.clone();
        self.index_count = count;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn small_tree() -> Arc<Tree> {
        let mut desc = crate::tree_gen::TreeDesc::default();
        desc.branching.iterations = 3;
        Arc::new(Tree::new(desc))
    }

    fn binding_fixture(tree: &Tree, cells: &[(usize, usize, usize)]) -> RasterTreeMesh {
        let mut bytes = vec![0; 512];
        for &(x, y, z) in cells {
            bytes[x + 8 * (y + 8 * z)] = 5;
        }
        let mut mesh = RasterTreeMesh::default();
        mesh.append_region(UVec3::ZERO, UVec3::splat(8), &bytes, tree.trunks())
            .unwrap();
        mesh.finish().unwrap();
        assert!(!mesh.vertices.is_empty());
        mesh
    }

    // Independent reference to the original per-vertex ownership and full cone scan.
    fn reference_bind(mesh: &mut RasterTreeMesh, id: u32, origin: Vec3, tree: &Tree) {
        for (vertex, binding) in mesh.vertices.iter().zip(&mut mesh.bindings) {
            let center = (Vec3::from(vertex.center) - origin) * 256.;
            if binding.is_none() && tree.trunks().iter().any(|c| c.signed_distance(center) < 0.) {
                *binding = Some((
                    id,
                    SkinBinding::at_rest_position(
                        tree,
                        (Vec3::from(vertex.position) - origin) * 256.,
                    )
                    .unwrap(),
                ));
            }
        }
    }

    #[test]
    fn publication_equality_covers_geometry_normals_occupancy_and_skin() {
        let tree = small_tree();
        let mut mesh = binding_fixture(&tree, &[(3, 3, 3), (4, 3, 3)]);
        mesh.bind_tree(7, Vec3::ZERO, &tree, None).unwrap();
        assert!(mesh.same_surface(&mesh.clone()));
        let mut memo_only = mesh.clone();
        memo_only.binding_cache.clear();
        assert!(mesh.same_surface(&memo_only));
        for change in 0..7 {
            let mut other = mesh.clone();
            match change {
                0 => other.vertices[0].normal = [1., 0., 0.],
                1 => other.vertices[0].normal_confidence = 0.5,
                2 => other.vertices[0].position[0] += 1. / 256.,
                3 => other.indices.swap(0, 1),
                4 => {
                    other.solid_cells.insert([3, 2, 3]);
                }
                5 => other.bindings[0].as_mut().unwrap().1.branch += 1,
                6 => other.cell_vertex_indices[0] = 0,
                _ => unreachable!(),
            }
            assert!(!mesh.same_surface(&other), "change {change} must publish");
        }
    }

    #[test]
    fn edited_surface_reuses_rest_bindings_without_reusing_faces_or_normals() {
        let tree = small_tree();
        let mut before = binding_fixture(&tree, &[(3, 3, 3), (4, 3, 3)]);
        before.bind_tree(7, Vec3::ZERO, &tree, None).unwrap();
        // Remove wood and expose another cell: shared corners, new corners, different faces.
        let mut after = binding_fixture(&tree, &[(3, 3, 3), (3, 4, 3)]);
        let mut expected = after.clone();
        reference_bind(&mut expected, 7, Vec3::ZERO, &tree);
        after
            .bind_tree(7, Vec3::ZERO, &tree, Some(&before))
            .unwrap();
        assert_eq!(after.bindings, expected.bindings);
        assert_eq!(
            after.gpu_skin().unwrap().bindings,
            expected.gpu_skin().unwrap().bindings
        );
        assert_ne!(after.rest_fingerprint(), before.rest_fingerprint());
        assert_eq!(after.rest_fingerprint(), expected.rest_fingerprint());
        let corners = &after.binding_cache[&7].corners;
        assert!(corners
            .keys()
            .any(|key| !before.binding_cache[&7].corners.contains_key(key)));
        // Only currently used corners survive, rather than all previously exposed wood.
        assert_eq!(corners.len(), 12);
    }

    #[test]
    fn rest_binding_reuse_requires_same_tree_identity_and_placement() {
        let tree = small_tree();
        let mut before = binding_fixture(&tree, &[(3, 3, 3), (4, 3, 3)]);
        before.bind_tree(7, Vec3::ZERO, &tree, None).unwrap();
        // Sentinel detects any accidental reuse across replacement/movement, even if
        // a replacement happens to generate an identical authored shape.
        for skin in before
            .binding_cache
            .get_mut(&7)
            .unwrap()
            .corners
            .values_mut()
        {
            skin.branch = usize::MAX;
        }
        for (replacement, origin) in [
            (small_tree(), Vec3::ZERO),
            (Arc::clone(&tree), Vec3::X / 256.),
        ] {
            let mut after = binding_fixture(&replacement, &[(3, 3, 3), (4, 3, 3)]);
            let mut expected = after.clone();
            reference_bind(&mut expected, 7, origin, &replacement);
            after
                .bind_tree(7, origin, &replacement, Some(&before))
                .unwrap();
            assert_eq!(after.bindings, expected.bindings);
        }
    }

    #[test]
    fn cached_corners_do_not_override_overlapping_tree_ownership_order() {
        let first = small_tree();
        let second = small_tree();
        let mut before = binding_fixture(&first, &[(3, 3, 3), (4, 3, 3)]);
        before.bind_tree(7, Vec3::ZERO, &first, None).unwrap();
        before.bind_tree(9, Vec3::ZERO, &second, None).unwrap();
        let mut after = binding_fixture(&first, &[(3, 3, 3), (4, 3, 3)]);
        let mut expected = after.clone();
        reference_bind(&mut expected, 9, Vec3::ZERO, &second);
        reference_bind(&mut expected, 7, Vec3::ZERO, &first);
        after
            .bind_tree(9, Vec3::ZERO, &second, Some(&before))
            .unwrap();
        after
            .bind_tree(7, Vec3::ZERO, &first, Some(&before))
            .unwrap();
        assert_eq!(after.bindings, expected.bindings);
        assert!(after.binding_cache[&7].corners.is_empty());
    }

    #[test]
    fn gpu_skin_compiles_per_vertex_records_and_deduplicates_tree_bones() {
        let mut mesh = RasterTreeMesh::default();
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
                    normal_confidence: 0.375,
                }));
            mesh.bindings.extend([Some((tree, binding)); 8]);
        }
        let skin = mesh.gpu_skin().unwrap();
        assert_eq!(skin.rest.len(), 24 * 3);
        assert!(skin.rest.chunks_exact(3).all(|v| v[2][3] == 0.375));
        assert_eq!(skin.branches, [(7, 2), (7, 1), (9, 2), (9, 1)]);
        assert_eq!(skin.bindings.len(), 24);
        assert!(skin.bindings[..16]
            .iter()
            .all(|&b| b == [1, 2, 0.25_f32.to_bits(), 0]));
        assert!(skin.bindings[16..]
            .iter()
            .all(|&b| b == [3, 4, 0.25_f32.to_bits(), 0]));
        mesh.bindings[0] = Some((
            7,
            SkinBinding {
                parent: None,
                ..binding
            },
        ));
        assert_eq!(mesh.gpu_skin().unwrap().bindings[0][1], 0);
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
        assert_eq!(mesh.confidence_counts(), [2, 0, 0]);
        assert_eq!(mesh.single_voxel_cross_sections(), 2);
        assert_eq!(mesh.rest_fingerprint(), mesh.clone().rest_fingerprint());
        for cell in table.iter().filter(|e| e[3] != 0) {
            assert_eq!(((cell[3] >> 17) & 63).count_ones(), 5);
            let center =
                (UVec3::new(cell[0], cell[1], cell[2]).as_vec3() + Vec3::splat(0.5)) / 256.;
            let vertex = mesh
                .vertices
                .iter()
                .find(|v| Vec3::from(v.center) == center)
                .unwrap();
            assert_eq!(
                (cell[3] & 0x1ffff) - 1,
                pack_normal_oct16(Vec3::from(vertex.normal))
            );
        }
        let mut cache = vec![[0.; 4]; TREE_CELL_CAPACITY];
        mesh.validate_lighting_cache(&cache, true).unwrap();
        assert!(mesh.validate_lighting_cache(&cache, false).is_err());
        for &base in mesh.cell_vertex_indices.iter().filter(|&&v| v != u32::MAX) {
            assert!(mesh.vertices[base as usize].normal_confidence == 0.);
        }
        for value in &mut cache {
            value[3] = 1.;
        }
        mesh.validate_lighting_cache(&cache, false).unwrap();
        // An edit removing a tree voxel is read from the published atlas, not regenerated from cones.
        bytes[4 + 8 * (3 + 8 * 3)] = 0;
        let mut edited = RasterTreeMesh::default();
        edited
            .append_region(UVec3::ZERO, dim, &bytes, &cones)
            .unwrap();
        edited.finish().unwrap();
        assert_eq!(edited.cell_count(), 1);
        assert_eq!(edited.indices.len(), 36);
        assert_eq!(edited.single_voxel_cross_sections(), 1);
        assert_ne!(mesh.rest_fingerprint(), edited.rest_fingerprint());
    }
    #[test]
    fn neighboring_blocks_share_posed_corners_and_hits_recover_rest_surface() {
        use crate::{
            tree_gen::{pose::TreePose, TreeDesc},
            wind_field::WindFieldFrame,
        };
        use glam::Vec2;
        let tree = Arc::new(Tree::new(TreeDesc::default()));
        let mut bytes = vec![0; 512];
        for x in [3, 4] {
            bytes[x + 8 * (3 + 8 * 3)] = 5;
        }
        let mut mesh = RasterTreeMesh::default();
        mesh.append_region(UVec3::ZERO, UVec3::splat(8), &bytes, tree.trunks())
            .unwrap();
        mesh.finish().unwrap();
        assert_eq!(mesh.cell_count(), 2);
        mesh.bind_tree(17, Vec3::ZERO, &tree, None).unwrap();
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
    fn smooth_surface_omits_interior_cells_but_tracks_the_full_rest_volume() {
        let tree = Tree::new(crate::tree_gen::TreeDesc::default());
        let mut mesh = RasterTreeMesh::default();
        mesh.append_region(UVec3::ZERO, UVec3::splat(8), &vec![5; 512], tree.trunks())
            .unwrap();
        mesh.finish().unwrap();
        assert!(mesh.cell_count() > 0);
        assert!(mesh.cell_count() < mesh.solid_cells.len());
        assert!(mesh.indices.len() < mesh.cell_count() * 36);
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
