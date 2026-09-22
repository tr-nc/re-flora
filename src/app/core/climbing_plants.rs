//! One opt-in playable wall vine, independent of tree/grass ownership and snapshot history.
use super::App;
use crate::app::world_edits::{VoxelEdit, WorldEditTransaction};
use crate::builder::{
    ContreeCpuVoxelBlock, ContreeCpuVoxelBlockExport, ContreeCpuVoxelSourceDependency,
};
use crate::builder::{VOXEL_TYPE_EMPTY, VOXEL_TYPE_LIMESTONE};
use crate::climbing_plants::{Plant, Terrain};
use crate::geom::{build_bvh, Cuboid, UAabb3};
use crate::tracer::DynamicFruitRenderInstance;
use anyhow::Result;
use glam::{IVec3, Quat, UVec3, Vec3};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct ClimbingPlants {
    plant: Option<Plant>,
    created: bool,
    pub reset_requested: bool,
    pub focus_requested: bool,
    pub disconnect_root_requested: bool,
    pub refill_tip_requested: bool,
    accumulator: f32,
    instances: Vec<DynamicFruitRenderInstance>,
    anchor_chunks: HashMap<UVec3, Vec<usize>>,
    dirty_anchors: HashSet<usize>,
    review_edited: bool,
    last_dependencies: Vec<ContreeCpuVoxelSourceDependency>,
    review_before_edit: Option<Plant>,
    review_ticks: u32,
    review_multiple: bool,
    review_reported: bool,
    review_root_cut: bool,
    review_root_verified: bool,
    review_refilled: bool,
    review_refill_observed: bool,
    refill_cell: Option<UVec3>,
}
impl ClimbingPlants {
    pub fn has_history(&self) -> bool {
        self.plant.is_some()
    }
    pub fn observe_edit(&mut self, bound: UAabb3) {
        let Some(plant) = &self.plant else {
            return;
        };
        let lo = bound.min() / 256;
        let hi = bound.max() / 256;
        for (chunk, ids) in &self.anchor_chunks {
            if chunk.cmpge(lo).all() && chunk.cmple(hi).all() {
                for &id in ids {
                    let (min, max) = contact_bounds(plant, id);
                    if min.cmple(bound.max()).all() && max.cmpge(bound.min()).all() {
                        self.dirty_anchors.insert(id);
                    }
                }
            }
        }
    }
    /// World replacement drops history and prevents silently authoring a new wall on load.
    pub fn replace_world(&mut self) {
        *self = Self {
            created: true,
            ..Self::default()
        };
    }
    fn index_anchors(&mut self) {
        self.anchor_chunks.clear();
        if let Some(plant) = &self.plant {
            for id in 0..plant.anchors.len() {
                let (min, max) = contact_bounds(plant, id);
                let lo = min / 256;
                let hi = max / 256;
                for z in lo.z..=hi.z {
                    for y in lo.y..=hi.y {
                        for x in lo.x..=hi.x {
                            self.anchor_chunks
                                .entry(UVec3::new(x, y, z))
                                .or_default()
                                .push(id);
                        }
                    }
                }
            }
        }
    }
}
// Fixed support identity plus the stem's exposed contact footprint. Occluding a contact
// from the neighbouring chunk is just as relevant as deleting its support cell.
fn contact_bounds(plant: &Plant, id: usize) -> (UVec3, UVec3) {
    let anchor = &plant.anchors[id];
    let min = (anchor.position - Vec3::splat(plant.radius))
        .min(anchor.cell.as_vec3())
        .max(Vec3::ZERO)
        .floor()
        .as_uvec3();
    let max = (anchor.position + Vec3::splat(plant.radius))
        .max(anchor.cell.as_vec3())
        .floor()
        .as_uvec3();
    (min, max)
}
struct Patch {
    block: ContreeCpuVoxelBlock,
    fresh: bool,
}
impl Terrain for Patch {
    fn voxel(&self, cell: IVec3) -> Option<u8> {
        if cell.cmplt(IVec3::ZERO).any() {
            return None;
        }
        let cell = cell.as_uvec3();
        if cell.cmplt(self.block.voxel_min).any() {
            return None;
        }
        let c = cell - self.block.voxel_min;
        let d = self.block.dim;
        if c.cmpge(d).any() {
            return None;
        }
        Some(self.block.voxel_types[((c.z * d.y + c.y) * d.x + c.x) as usize])
    }
    fn current(&self) -> bool {
        self.fresh
    }
}

fn wall_edit(min: UVec3, max: UVec3, material: u32) -> Result<WorldEditTransaction> {
    let cuboid = Cuboid::from_min_max(min.as_vec3(), max.as_vec3());
    let bvh_nodes = build_bvh(&[cuboid.aabb()], &[0]).map_err(anyhow::Error::msg)?;
    Ok(WorldEditTransaction::terrain_change(
        vec![VoxelEdit::StampCuboids {
            bvh_nodes,
            cuboids: vec![cuboid],
            voxel_type: material,
            atlas_state_write: Default::default(),
        }],
        UAabb3::new(min, max),
    ))
}

impl App {
    pub(super) fn update_climbing_plants(&mut self, steps: u32, tick_seconds: f32) -> Result<()> {
        let review = std::env::var_os("RE_FLORA_CLIMBING_REVIEW").is_some();
        let enabled = self.debug_settings.adjustables.climbing_enabled.value || review;
        if !enabled {
            self.tracer.show_climbing_plant_geometry(&[])?;
            return Ok(());
        }
        if !self.terrain_persistence.allows_world_updates() {
            return Ok(());
        }
        if self.climbing_plants.reset_requested || !self.climbing_plants.created {
            self.climbing_plants = ClimbingPlants {
                created: true,
                focus_requested: true,
                ..Default::default()
            };
            self.execute_world_edit(wall_edit(
                UVec3::new(224, 192, 300),
                UVec3::new(288, 300, 306),
                VOXEL_TYPE_LIMESTONE,
            )?)?;
            log::info!("[CLIMBING] authored editable review wall; history is session-only");
        }
        if self.climbing_plants.focus_requested {
            self.climbing_plants.focus_requested = false;
            let target = Vec3::new(256., 244., 308.) / 256.;
            self.camera_control.apply_snapshot_mode(true);
            self.camera_control.set_orbit_focus(target);
            self.tracer
                .set_camera_pose_looking_at(target + Vec3::new(0., 0.02, 0.65), target);
            self.reset_camera_movement_input();
        }
        if std::mem::take(&mut self.climbing_plants.refill_tip_requested) {
            if let Some(plant) = &self.climbing_plants.plant {
                let tip = plant.nodes.last().unwrap().position;
                let cell = tip.floor().as_uvec3();
                let min = UVec3::new(cell.x.saturating_sub(1), cell.y.saturating_sub(2), cell.z);
                let max = UVec3::new(cell.x + 2, cell.y + 3, (tip.z + plant.radius).ceil() as u32)
                    .min(super::CHUNK_DIM * super::VOXEL_DIM_PER_CHUNK);
                anyhow::ensure!(
                    max.cmpgt(min).all(),
                    "vine tip is outside editable refill bounds"
                );
                self.climbing_plants.refill_cell = Some(min);
                self.execute_world_edit(wall_edit(min, max, VOXEL_TYPE_LIMESTONE)?)?;
                log::info!("[CLIMBING] real refill through tip bounds={min:?}..{max:?}");
            }
        }
        let source = self.contree_builder.cpu_voxel_source_snapshot();

        let (min, max) = if let Some(plant) = &self.climbing_plants.plant {
            let mut min = plant.nodes[0].position;
            let mut max = min;
            for node in &plant.nodes {
                min = min.min(node.position);
                max = max.max(node.position);
            }
            for anchor in &plant.anchors {
                min = min.min(anchor.cell.as_vec3());
                max = max.max(anchor.cell.as_vec3());
            }
            (
                (min - Vec3::splat(8.)).max(Vec3::ZERO).floor().as_uvec3(),
                (max + Vec3::splat(9.))
                    .ceil()
                    .as_uvec3()
                    .min(super::CHUNK_DIM * super::VOXEL_DIM_PER_CHUNK),
            )
        } else {
            (UVec3::new(250, 194, 298), UVec3::new(262, 208, 325))
        };
        let ContreeCpuVoxelBlockExport::Ready(block) = source.export_voxel_block(min, max - min)?
        else {
            return Ok(());
        };
        // Publication/cache readiness is distinct from revision equality. No world mutation can
        // interleave this synchronous query/commit, and all overlapping chunks are dependencies.
        let latest = self.contree_builder.cpu_voxel_source_snapshot();
        let fresh = block
            .source_dependencies
            .iter()
            .all(|d| latest.is_chunk_voxel_cache_ready(*d));
        if !fresh {
            return Ok(());
        }
        let patch = Patch { block, fresh };
        if self.climbing_plants.plant.is_none() {
            for z in (299..324).rev() {
                let cell = IVec3::new(255, 198, z);
                if let Some(material) = patch
                    .voxel(cell)
                    .filter(|v| *v == VOXEL_TYPE_LIMESTONE as u8)
                {
                    let position = cell.as_vec3() + Vec3::new(0.5, 0.5, 1.8);
                    if crate::climbing_plants::clear_segment(&patch, position, position, 0.65)
                        == Some(true)
                    {
                        self.climbing_plants.plant =
                            Some(Plant::seed(position, Vec3::Z, cell, material, 42));
                        log::info!(
                            "[CLIMBING] seed=42 position={position:?} dependencies={}",
                            patch.block.source_dependencies.len()
                        );
                    }
                    break;
                }
            }
        }
        if review && self.climbing_plants.review_edited {
            self.climbing_plants.review_ticks += 1;
        }
        let Some(plant) = &mut self.climbing_plants.plant else {
            return Ok(());
        };
        if self.climbing_plants.disconnect_root_requested {
            self.climbing_plants.disconnect_root_requested = false;
            plant.disconnect_root();
            log::info!("[CLIMBING] root disconnected; growth stopped, wall attachments retained");
        }
        if review
            && self.climbing_plants.review_refilled
            && !self.climbing_plants.review_refill_observed
        {
            let overlapping = plant.nodes.iter().any(|n| {
                crate::climbing_plants::clear_segment(&patch, n.position, n.position, plant.radius)
                    == Some(false)
            });
            if overlapping {
                self.climbing_plants.review_refill_observed = true;
                log::info!(
                    "[CLIMBING][REVIEW] refill overlap observed in authoritative current terrain"
                );
            }
        }
        let before_nodes = plant.nodes.len();

        let before_anchors = plant.anchors.iter().filter(|a| a.attached).count();
        // Events narrow phase contact cells; dependency polling catches later cache publication.
        for dependency in &patch.block.source_dependencies {
            if !self.climbing_plants.last_dependencies.contains(dependency) {
                if let Some(ids) = self
                    .climbing_plants
                    .anchor_chunks
                    .get(&dependency.chunk_idx)
                {
                    self.climbing_plants.dirty_anchors.extend(ids);
                }
            }
        }
        self.climbing_plants
            .last_dependencies
            .clone_from(&patch.block.source_dependencies);
        let topology_before: Vec<_> = plant
            .nodes
            .iter()
            .map(|n| (n.parent, n.rest_length))
            .collect();
        let released = plant.revalidate(&patch, self.climbing_plants.dirty_anchors.iter().copied());

        self.climbing_plants.dirty_anchors.clear();
        if released {
            log::info!(
                "[CLIMBING] support released attached={}->{} nodes={} stable_topology={}",
                before_anchors,
                plant.anchors.iter().filter(|a| a.attached).count(),
                plant.nodes.len(),
                topology_before
                    == plant
                        .nodes
                        .iter()
                        .map(|n| (n.parent, n.rest_length))
                        .collect::<Vec<_>>()
            );
        }
        let dt = steps as f32 * tick_seconds;
        self.climbing_plants.accumulator +=
            dt * self.debug_settings.adjustables.climbing_speed.value;
        let quanta = if review {
            1
        } else {
            self.climbing_plants.accumulator.floor().min(8.) as u32
        };
        if !review {
            self.climbing_plants.accumulator -= quanta as f32;
        }
        for _ in 0..quanta {
            plant.grow(
                &patch,
                self.debug_settings.adjustables.climbing_spacing.value,
            );
        }
        for _ in 0..if review { 1 } else { steps.min(8) } {
            plant.relax(&patch);
        }
        if before_nodes / 16 != plant.nodes.len() / 16 {
            log::info!(
                "[CLIMBING] growth nodes={} attached={} tips={} finite={}",
                plant.nodes.len(),
                plant.anchors.iter().filter(|a| a.attached).count(),
                plant.tips.len(),
                plant.nodes.iter().all(|n| n.position.is_finite())
            );
        }
        let mut instances = std::mem::take(&mut self.climbing_plants.instances);
        instances.clear();
        instances.reserve(plant.nodes.len() * 2);
        for (id, node) in plant.nodes.iter().enumerate() {
            if let Some(parent) = node.parent {
                let start = plant.nodes[parent].position;
                let delta = node.position - start;
                let rotation = Quat::from_rotation_arc(Vec3::Y, delta.normalize_or_zero());
                instances.push(block_instance(
                    (start + node.position) * 0.5,
                    rotation,
                    Vec3::new(0.9, delta.length(), 0.9),
                    Vec3::new(0.22, 0.32, 0.07),
                ));
                if id % 3 == 0 {
                    let side = if id % 2 == 0 { -1. } else { 1. };
                    instances.push(block_instance(
                        node.position + rotation * Vec3::new(side * 1.8, 0., 0.3),
                        rotation * Quat::from_rotation_z(side * 0.6),
                        Vec3::new(3.8, 2.5, 0.55),
                        Vec3::new(0.12 + (id % 5) as f32 * 0.015, 0.38, 0.09),
                    ));
                }
            }
        }
        if self.debug_settings.adjustables.climbing_show_anchors.value {
            for a in &plant.anchors {
                instances.push(block_instance(
                    plant.nodes[a.node].position + Vec3::Z,
                    Quat::IDENTITY,
                    Vec3::splat(1.6),
                    if a.attached {
                        Vec3::new(1., 0.8, 0.1)
                    } else {
                        Vec3::new(1., 0.1, 0.1)
                    },
                ));
            }
        }
        if review
            && self.climbing_plants.review_multiple
            && self.climbing_plants.review_ticks >= 220
            && !self.climbing_plants.review_reported
        {
            if let Some(before) = &self.climbing_plants.review_before_edit {
                let stable = plant.nodes.len() >= before.nodes.len()
                    && plant
                        .nodes
                        .iter()
                        .zip(&before.nodes)
                        .all(|(a, b)| a.parent == b.parent && a.rest_length == b.rest_length)
                    && plant
                        .anchors
                        .iter()
                        .zip(&before.anchors)
                        .all(|(a, b)| a.node == b.node && a.cell == b.cell);
                let max_motion = plant
                    .nodes
                    .iter()
                    .zip(&before.nodes)
                    .map(|(a, b)| a.position.distance(b.position))
                    .fold(0.0f32, f32::max);
                let max_length_error = plant
                    .nodes
                    .iter()
                    .filter_map(|n| {
                        n.parent.map(|p| {
                            (n.position.distance(plant.nodes[p].position) - n.rest_length).abs()
                        })
                    })
                    .fold(0.0f32, f32::max);
                let unaffected = before
                    .anchors
                    .iter()
                    .filter(|a| a.attached && a.cell.y < 244)
                    .all(|a| {
                        plant
                            .anchors
                            .iter()
                            .any(|b| b.node == a.node && b.attached && b.position == a.position)
                    });
                let collision_clear = plant.nodes.iter().all(|n| {
                    n.parent.is_none_or(|p| {
                        crate::climbing_plants::clear_segment(
                            &patch,
                            plant.nodes[p].position,
                            n.position,
                            plant.radius,
                        ) == Some(true)
                    })
                });
                let refill_retained = self
                    .climbing_plants
                    .refill_cell
                    .is_some_and(|c| patch.voxel(c.as_ivec3()) == Some(VOXEL_TYPE_LIMESTONE as u8));
                anyhow::ensure!(
                    self.climbing_plants.review_refill_observed && refill_retained,
                    "review must retain the actual overlapping refill while removing supports"
                );
                anyhow::ensure!(
                    collision_clear
                        && stable
                        && unaffected
                        && max_length_error < 0.0021
                        && plant.nodes.iter().all(|n| n.position.is_finite()),
                    "climbing review invariant failed"
                );
                log::info!("[CLIMBING][REVIEW] verified stable_ids={stable} unaffected_supports={unaffected} collision_clear={collision_clear} refill_retained={refill_retained} finite=true max_length_error={max_length_error:.6} max_motion_voxels={max_motion:.4} nodes={} attached={}",plant.nodes.len(),plant.anchors.iter().filter(|a|a.attached).count());
            }
            self.climbing_plants.review_reported = true;
        }
        if review
            && self.climbing_plants.review_root_cut
            && self.climbing_plants.review_ticks >= 250
            && !self.climbing_plants.review_root_verified
        {
            let before = self.climbing_plants.review_before_edit.as_ref().unwrap();
            let drop = before.nodes[0].position.y - plant.nodes[0].position.y;
            anyhow::ensure!(
                !plant.root_connected()
                    && plant.nodes.len() == before.nodes.len()
                    && plant.anchors.iter().all(|a| !a.attached)
                    && drop > 1.0,
                "root cut review failed: drop={drop}"
            );
            log::info!("[CLIMBING][REVIEW] root_cut=true growth_stopped=true attached=0 root_drop_voxels={drop:.4}");
            self.climbing_plants.review_root_verified = true;
        }
        let review_edit = review && !self.climbing_plants.review_edited && plant.nodes.len() >= 65;
        let edit_cell = plant.anchors.get(2).map(|a| a.cell.as_uvec3());
        self.tracer.show_climbing_plant_geometry(&instances)?;
        self.climbing_plants.instances = instances;
        self.climbing_plants.index_anchors();
        if review_edit {
            if let Some(c) = edit_cell {
                self.climbing_plants.review_edited = true;
                self.climbing_plants.review_before_edit = self.climbing_plants.plant.clone();
                self.execute_world_edit(wall_edit(
                    c - UVec3::new(4, 4, 3),
                    c + UVec3::new(5, 5, 3),
                    VOXEL_TYPE_EMPTY,
                )?)?;
                log::info!("[CLIMBING][REVIEW] real terrain edit at {c:?}");
            }
        }

        if review
            && self.climbing_plants.review_edited
            && !self.climbing_plants.review_refilled
            && self.climbing_plants.review_ticks >= 60
        {
            self.climbing_plants.review_refilled = true;
            self.climbing_plants.refill_tip_requested = true;
        }
        if review && self.climbing_plants.review_reported && !self.climbing_plants.review_root_cut {
            self.climbing_plants.review_root_cut = true;
            self.climbing_plants.review_before_edit = self.climbing_plants.plant.clone();
            self.climbing_plants.disconnect_root_requested = true;
            self.execute_world_edit(wall_edit(
                UVec3::new(224, 192, 299),
                UVec3::new(288, 300, 308),
                VOXEL_TYPE_EMPTY,
            )?)?;
            log::info!("[CLIMBING][REVIEW] disconnect root and remove remaining wall supports");
        }
        if review
            && self.climbing_plants.review_edited
            && !self.climbing_plants.review_multiple
            && self.climbing_plants.review_ticks >= 100
        {
            self.climbing_plants.review_multiple = true;
            self.climbing_plants.review_before_edit = self.climbing_plants.plant.clone();
            self.execute_world_edit(wall_edit(
                UVec3::new(224, 244, 299),
                UVec3::new(288, 300, 306),
                VOXEL_TYPE_EMPTY,
            )?)?;
            log::info!("[CLIMBING][REVIEW] real terrain edit removed several upper supports, retaining refill");
        }
        Ok(())
    }
}
fn block_instance(
    position: Vec3,
    rotation: Quat,
    dimensions: Vec3,
    color: Vec3,
) -> DynamicFruitRenderInstance {
    let mut instance = DynamicFruitRenderInstance::new(position / 256., rotation, 1.);
    instance.dimensions = dimensions / 256.;
    instance.color = color;
    instance
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::test_cpu_voxel_source_snapshot;

    #[test]
    fn local_edit_index_and_world_replacement_preserve_unrelated_state() {
        let plant = Plant::seed(
            Vec3::new(256.5, 10.5, 20.8),
            Vec3::Z,
            IVec3::new(256, 10, 19),
            1,
            42,
        );
        let mut runtime = ClimbingPlants {
            plant: Some(plant.clone()),
            created: true,
            ..Default::default()
        };
        runtime.index_anchors();
        runtime.observe_edit(UAabb3::new(
            UVec3::new(300, 10, 19),
            UVec3::new(310, 20, 25),
        ));
        assert!(runtime.dirty_anchors.is_empty());
        assert_eq!(runtime.plant.as_ref(), Some(&plant));
        runtime.observe_edit(UAabb3::new(
            UVec3::new(255, 10, 19),
            UVec3::new(257, 12, 20),
        ));
        assert_eq!(runtime.dirty_anchors, HashSet::from([0]));
        runtime.replace_world();
        assert!(runtime.plant.is_none());
        assert!(runtime.created); // load must not silently respawn the authored wall
        assert!(runtime.anchor_chunks.is_empty());
    }

    #[test]
    fn cross_chunk_export_dependencies_reject_changed_and_pending_sources() {
        let a = UVec3::ZERO;
        let b = UVec3::X;
        let grid = UVec3::new(2, 1, 1);
        let dim = UVec3::splat(256);
        let source = test_cpu_voxel_source_snapshot(grid, dim, &[], &[], &[(a, 1), (b, 1)], &[]);
        let ContreeCpuVoxelBlockExport::Ready(block) = source
            .export_voxel_block(UVec3::new(255, 1, 1), UVec3::new(2, 2, 2))
            .unwrap()
        else {
            panic!("absent chunks are authoritative empty");
        };
        assert_eq!(block.source_dependencies.len(), 2);
        assert!(block
            .source_dependencies
            .iter()
            .all(|d| source.is_chunk_voxel_cache_ready(*d)));
        let changed = test_cpu_voxel_source_snapshot(grid, dim, &[], &[], &[(a, 1), (b, 2)], &[]);
        assert!(!block
            .source_dependencies
            .iter()
            .all(|d| changed.is_chunk_voxel_cache_ready(*d)));
        let pending = test_cpu_voxel_source_snapshot(grid, dim, &[b], &[], &[(a, 1), (b, 1)], &[b]);
        assert!(!block
            .source_dependencies
            .iter()
            .all(|d| pending.is_chunk_voxel_cache_ready(*d)));
        assert!(matches!(
            pending
                .export_voxel_block(UVec3::new(255, 1, 1), UVec3::new(2, 2, 2))
                .unwrap(),
            ContreeCpuVoxelBlockExport::NotReady(_)
        ));
        let patch = Patch { block, fresh: true };
        assert_eq!(patch.voxel(IVec3::new(255, 1, 1)), Some(0));
        assert_eq!(patch.voxel(IVec3::new(256, 1, 1)), Some(0));
        assert_eq!(patch.voxel(IVec3::new(257, 1, 1)), None);
    }
    #[test]
    fn contact_exposure_indexes_neighbouring_chunk_and_precise_refill_bounds() {
        let plant = Plant::seed(
            Vec3::new(255.8, 10.5, 255.8),
            Vec3::Z,
            IVec3::new(255, 10, 254),
            1,
            42,
        );
        let mut runtime = ClimbingPlants {
            plant: Some(plant),
            ..Default::default()
        };
        runtime.index_anchors();
        assert!(runtime.anchor_chunks.contains_key(&UVec3::new(1, 0, 1)));
        runtime.observe_edit(UAabb3::new(
            UVec3::new(256, 10, 256),
            UVec3::new(257, 11, 257),
        ));
        assert_eq!(runtime.dirty_anchors, HashSet::from([0]));
        runtime.dirty_anchors.clear();
        runtime.observe_edit(UAabb3::new(
            UVec3::new(260, 10, 260),
            UVec3::new(261, 11, 261),
        ));
        assert!(runtime.dirty_anchors.is_empty());
    }
}
