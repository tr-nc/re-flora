//! One continuous wood mesh for color, shadows, scene queries and exact collision.
//! No terrain occupancy, voxel surface extraction, or alternate tree representation.
use crate::{
    resource::Resource,
    tree_gen::{
        mesh::WoodMesh,
        pose::BranchPose,
        skin::{intersect_surface_triangle, SkinBinding, SurfaceHit},
    },
};
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use std::collections::BTreeMap;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RasterTreeVertex {
    position: [f32; 3],
    normal: [f32; 3],
}
#[derive(Clone, Default)]
pub struct RasterTreeMesh {
    pub vertices: Vec<RasterTreeVertex>,
    pub indices: Vec<u32>,
    bindings: Vec<(u32, SkinBinding)>,
}
impl RasterTreeMesh {
    pub fn append(&mut self, tree: u32, origin: Vec3, mesh: &WoodMesh) -> Result<()> {
        let base = u32::try_from(self.vertices.len())?;
        ensure!(
            mesh.vertices.len() <= (u32::MAX - base) as usize,
            "wood vertex address space exhausted"
        );
        self.vertices
            .extend(mesh.vertices.iter().map(|v| RasterTreeVertex {
                position: (origin + v.position / 256.).to_array(),
                normal: v.normal.to_array(),
            }));
        self.bindings
            .extend(mesh.vertices.iter().map(|v| (tree, v.binding)));
        self.indices.extend(mesh.indices.iter().map(|i| base + i));
        Ok(())
    }
    pub fn gpu_skin(&self) -> Result<GpuTreeSkin> {
        ensure!(
            self.bindings.len() == self.vertices.len(),
            "missing tree bindings"
        );
        let mut result = GpuTreeSkin::default();
        let mut palette = BTreeMap::new();
        for (vertex, &(tree, binding)) in self.vertices.iter().zip(&self.bindings) {
            let mut index = |branch| {
                *palette.entry((tree, branch)).or_insert_with(|| {
                    result.branches.push((tree, branch));
                    result.branches.len() as u32
                })
            };
            let child = index(binding.branch);
            let parent = binding.parent.map(&mut index).unwrap_or(0);
            result
                .bindings
                .push([child, parent, binding.weight.to_bits(), 0]);
            result.rest.extend([
                Vec3::from(vertex.position).extend(1.).to_array(),
                Vec3::from(vertex.normal).extend(0.).to_array(),
            ]);
        }
        ensure!(
            result.branches.len() < super::tree_scene::MAX_TREE_VERTICES,
            "tree pose storage exhausted"
        );
        Ok(result)
    }
    pub fn rest_surface(&self) -> PosedTreeSurface {
        PosedTreeSurface {
            positions: self
                .vertices
                .iter()
                .map(|v| Vec3::from(v.position))
                .collect(),
        }
    }
    pub fn posed_surface<'a>(
        &self,
        poses: impl Fn(u32) -> Option<&'a [BranchPose]>,
    ) -> Result<PosedTreeSurface> {
        let positions = self
            .vertices
            .iter()
            .zip(&self.bindings)
            .map(|(v, &(tree, binding))| {
                Ok(binding
                    .transform(poses(tree).ok_or_else(|| anyhow::anyhow!("missing tree pose"))?)?
                    .point(Vec3::from(v.position)))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(PosedTreeSurface { positions })
    }
    pub fn max_displacement(&self, surface: &PosedTreeSurface) -> f32 {
        self.vertices
            .iter()
            .zip(surface.positions())
            .map(|(v, p)| Vec3::from(v.position).distance(p))
            .fold(0., f32::max)
    }
    pub fn rest_fingerprint(&self) -> u64 {
        bytemuck::cast_slice::<_, u8>(&self.vertices)
            .iter()
            .chain(bytemuck::cast_slice::<_, u8>(&self.indices))
            .fold(0xcbf29ce484222325, |h, b| {
                (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
            })
    }
    pub fn validate_lighting_cache(&self, data: &[[f32; 4]]) -> Result<()> {
        ensure!(
            data.len() == self.vertices.len(),
            "tree lighting cache size mismatch"
        );
        ensure!(
            data.iter()
                .all(|v| v.iter().all(|c| c.is_finite() && *c >= 0.)),
            "invalid tree irradiance"
        );
        Ok(())
    }
}
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

pub fn tree_attachment_hash(p: [u32; 3]) -> u32 {
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
    pub enabled: bool,
    pub wind_enabled: bool,
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
            indices: Resource::new(Self::index_buffer(device, allocator, 4)),
            index_count: 0,
            enabled: true,
            wind_enabled: false,
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
    fn index_buffer(device: Device, allocator: Allocator, bytes: usize) -> Buffer {
        Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
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
            let t = self.scene.primitives[index as usize];
            let ids = [t[0] as usize, t[1] as usize, t[2] as usize];
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
    pub fn validate_gpu_surface(&self, data: &[[f32; 4]]) -> Result<()> {
        ensure!(
            data.len() == self.rest_mesh.vertices.len() * 2,
            "GPU tree surface size mismatch"
        );
        let mut max_position_error = 0.0f32;
        let mut max_normal_error = 0.0f32;
        for (i, v) in self.rest_mesh.vertices.iter().enumerate() {
            let position = self
                .posed_surface
                .as_ref()
                .map_or(Vec3::from(v.position), |s| s.position(i));
            let normal = if self.wind_enabled {
                let [child, parent, weight, _] = self.skin.bindings[i];
                SkinBinding {
                    branch: child as usize,
                    parent: Some(parent as usize),
                    weight: f32::from_bits(weight),
                }
                .transform(&self.skin.poses)?
                .normal(Vec3::from(v.normal))
            } else {
                Vec3::from(v.normal)
            };
            let gp = Vec3::from_slice(&data[i * 2]);
            let gn = Vec3::from_slice(&data[i * 2 + 1]);
            ensure!(
                gp.is_finite() && gn.is_finite(),
                "nonfinite GPU tree vertex"
            );
            max_position_error = max_position_error.max(gp.distance(position));
            max_normal_error = max_normal_error.max(gn.distance(normal));
        }
        ensure!(
            max_position_error < 2e-6 && max_normal_error < 2e-4,
            "GPU skin mismatch position={max_position_error} normal={max_normal_error}"
        );
        log::info!(
            "[TREE][GPU_SKIN] vertices={} bones={} wind={} position_error={} normal_error={}",
            self.rest_mesh.vertices.len(),
            self.skin.branches.len(),
            self.wind_enabled,
            max_position_error,
            max_normal_error
        );
        Ok(())
    }
    pub fn draw_counts(&self) -> (u32, u32) {
        (self.index_count, 1)
    }
    /// All fallible CPU preparation precedes replacement; caller owns frame fences.
    pub fn upload(
        &mut self,
        device: Device,
        allocator: Allocator,
        mesh: &RasterTreeMesh,
    ) -> Result<()> {
        let count = u32::try_from(mesh.indices.len())?;
        let skin = mesh.gpu_skin()?;
        let positions = mesh.rest_surface();
        let scene = super::tree_scene::TreeScene::new(&mesh.indices, &positions.positions)?;
        let refit = scene.refit_schedule();
        let indices = Self::index_buffer(
            device,
            allocator,
            std::mem::size_of_val(mesh.indices.as_slice()),
        );
        if count > 0 {
            indices.fill(&mesh.indices)?;
        }
        self.skin = skin;
        self.scene = scene;
        self.refit = refit;
        self.indices = Resource::new(indices);
        self.rest_mesh = mesh.clone();
        self.posed_surface = Some(positions);
        self.index_count = count;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_adapter_preserves_welded_indices_and_deduplicates_bones() {
        let tree = crate::tree_gen::Tree::new(crate::tree_gen::TreeDesc::default());
        let wood = WoodMesh::from_tree(&tree).unwrap();
        let mut mesh = RasterTreeMesh::default();
        mesh.append(7, Vec3::ONE, &wood).unwrap();
        mesh.append(9, Vec3::ZERO, &wood).unwrap();
        assert_eq!(mesh.vertices.len(), wood.vertices.len() * 2);
        assert_eq!(&mesh.indices[..wood.indices.len()], &wood.indices);
        let skin = mesh.gpu_skin().unwrap();
        assert_eq!(skin.rest.len(), mesh.vertices.len() * 2);
        let unique: std::collections::BTreeSet<_> = skin.branches.iter().copied().collect();
        assert_eq!(unique.len(), skin.branches.len());
        assert_eq!(
            mesh.vertices[0].position,
            (Vec3::ONE + wood.vertices[0].position / 256.).to_array()
        );
    }
}
