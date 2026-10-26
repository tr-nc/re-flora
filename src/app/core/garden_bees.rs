//! Durable tree/post attachments, transient solitary-bee residents and shared
//! model presentation. Terrain owns support; this module never stamps hive voxels.
use super::{player_tools::PlayerTool, App};
use crate::{
    builder::{VOXEL_TYPE_CHERRY_WOOD, VOXEL_TYPE_MASK, VOXEL_TYPE_OAK_WOOD},
    geom::UAabb3,
    particles::{
        bees::{Bees, HiveSite, HIVE_SCALE, MAX_HIVES},
        ButterflyFrame,
    },
};
use anyhow::{ensure, Result};
use glam::{Quat, UVec3, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod review;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub(super) enum Anchor {
    Tree(u32),
    Pole(u32),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Hive {
    id: u32,
    anchor: Anchor,
    support: [u32; 3],
    facing: [f32; 3],
}
impl Hive {
    fn rest_site(&self) -> HiveSite {
        let facing = Vec3::from(self.facing);
        HiveSite {
            id: self.id,
            position: (UVec3::from(self.support).as_vec3() + Vec3::splat(0.5)) / 256.
                + facing * (0.5 / 256. + 0.3 * HIVE_SCALE),
            rotation: Quat::from_rotation_y(facing.x.atan2(facing.z)),
        }
    }
    fn bound(&self) -> UAabb3 {
        let center = self.rest_site().position * 256.;
        UAabb3::new(
            (center - Vec3::splat(0.9 * HIVE_SCALE * 256.))
                .floor()
                .max(Vec3::ZERO)
                .as_uvec3(),
            (center + Vec3::splat(0.9 * HIVE_SCALE * 256.))
                .ceil()
                .as_uvec3(),
        )
    }
    fn space_clear(&self, bytes: &[u8]) -> bool {
        let bound = self.bound();
        let dim = bound.max() - bound.min();
        if bytes.len() != dim.element_product() as usize {
            return false;
        }
        let index = |p: UVec3| {
            let q = p - bound.min();
            (q.x + dim.x * (q.y + dim.y * q.z)) as usize
        };
        let expected = match self.anchor {
            Anchor::Tree(_) => VOXEL_TYPE_CHERRY_WOOD,
            Anchor::Pole(_) => VOXEL_TYPE_OAK_WOOD,
        };
        if u32::from(bytes[index(self.support.into())] & VOXEL_TYPE_MASK) != expected {
            return false;
        }
        let site = self.rest_site();
        for z in 0..dim.z {
            for y in 0..dim.y {
                for x in 0..dim.x {
                    let p = bound.min() + UVec3::new(x, y, z);
                    let local = site.rotation.conjugate()
                        * ((p.as_vec3() + Vec3::splat(0.5)) / 256. - site.position)
                        / HIVE_SCALE;
                    if local.cmpge(Vec3::new(-0.6, -0.5, -0.22)).all()
                        && local.cmple(Vec3::new(0.6, 0.85, 0.4)).all()
                        && bytes[index(p)] & VOXEL_TYPE_MASK != 0
                    {
                        return false;
                    }
                }
            }
        }
        true
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct HiveSnapshot {
    next_id: u32,
    hives: Vec<Hive>,
}
impl Default for HiveSnapshot {
    fn default() -> Self {
        Self {
            next_id: 1,
            hives: Vec::new(),
        }
    }
}
impl HiveSnapshot {
    pub(super) fn validate(
        &self,
        world: UVec3,
        tree: impl Fn(u32) -> bool,
        pole: impl Fn(u32) -> bool,
    ) -> Result<()> {
        ensure!(
            self.next_id > 0 && self.hives.len() <= MAX_HIVES,
            "invalid bee-house count"
        );
        let mut ids = BTreeSet::new();
        let mut anchors = BTreeSet::new();
        for h in &self.hives {
            let n = Vec3::from(h.facing);
            let p = UVec3::from(h.support);
            ensure!(
                h.id > 0 && h.id < self.next_id && ids.insert(h.id) && anchors.insert(h.anchor),
                "duplicate/invalid bee-house identity"
            );
            ensure!(
                n.is_finite()
                    && (n.length_squared() - 1.).abs() < 1e-3
                    && n.y.abs() < 1e-4
                    && p.cmpge(UVec3::splat(9)).all()
                    && p.cmplt(world).all()
                    && (p + UVec3::splat(13)).cmplt(world).all(),
                "invalid bee-house position/orientation"
            );
            ensure!(
                match h.anchor {
                    Anchor::Tree(id) => tree(id),
                    Anchor::Pole(id) => pole(id),
                },
                "missing bee-house host"
            );
        }
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct GardenBees {
    pub(super) saved: HiveSnapshot,
    pub(super) swarm: Bees,
    pub(super) sites: Vec<HiveSite>,
    dirty: BTreeSet<u32>,
    bindings: BTreeMap<u32, Option<(u64, crate::tree_gen::skin::SkinBinding)>>,
    review: Option<(String, f32, bool)>,
}
impl GardenBees {
    pub(super) fn restore(&mut self, saved: HiveSnapshot) {
        self.saved = saved;
        self.dirty = self.saved.hives.iter().map(|h| h.id).collect();
        self.bindings.clear();
        self.sites.clear();
        self.swarm.clear();
    }
    pub(super) fn observe_edit(&mut self, bound: UAabb3) {
        for h in &self.saved.hives {
            let occupied = h.bound();
            if occupied.min().cmplt(bound.max()).all() && bound.min().cmplt(occupied.max()).all() {
                self.dirty.insert(h.id);
            }
        }
    }
}
impl App {
    pub(super) fn update_hive_anchors(&mut self) -> Result<()> {
        self.update_garden_fence_anchors()?;
        let mut removed = BTreeSet::new();
        if !self.garden_bees.dirty.is_empty() {
            let hives: Vec<_> = self
                .garden_bees
                .saved
                .hives
                .iter()
                .filter(|h| self.garden_bees.dirty.contains(&h.id))
                .collect();
            let regions: Vec<_> = hives
                .iter()
                .map(|h| (h.bound().min(), h.bound().max() - h.bound().min()))
                .collect();
            let data = if regions.is_empty() {
                Vec::new()
            } else {
                self.plain_builder.read_chunk_atlas_regions(&regions)?
            };
            for (h, bytes) in hives.iter().zip(data) {
                if !h.space_clear(&bytes) {
                    removed.insert(h.id);
                }
            }
            self.garden_bees.dirty.clear();
        }
        self.garden_bees.sites.clear();
        for h in &self.garden_bees.saved.hives {
            if removed.contains(&h.id) {
                continue;
            }
            let mut site = h.rest_site();
            let valid = match h.anchor {
                Anchor::Pole(id) => self.garden_fence.post_owns(id, h.support.into()),
                Anchor::Tree(id) => {
                    if let Some((position, rotation)) = self.trees.resolve_hive_pose(
                        id,
                        h.support.into(),
                        site.position,
                        Vec3::from(h.facing),
                        self.tracer.raster_trees.gpu_wind_active,
                        self.garden_bees.bindings.entry(h.id).or_default(),
                    ) {
                        site.position = position;
                        site.rotation = rotation;
                        true
                    } else {
                        false
                    }
                }
            };
            if valid {
                self.garden_bees.sites.push(site);
            } else {
                removed.insert(h.id);
            }
        }
        if !removed.is_empty() {
            self.garden_bees
                .saved
                .hives
                .retain(|h| !removed.contains(&h.id));
            self.garden_bees
                .bindings
                .retain(|id, _| !removed.contains(id));
            self.garden_bees
                .swarm
                .update(0., &self.garden_bees.sites, &[], false);
            log::info!(
                "[BEE_GARDEN] retired_houses={} remaining={} support_lost=true",
                removed.len(),
                self.garden_bees.sites.len()
            );
        }
        Ok(())
    }
    fn place_bee_house(&mut self, anchor: Anchor, contact: Vec3, facing: Vec3) -> Result<u32> {
        ensure!(
            contact.is_finite() && facing.is_finite() && facing.length_squared() > 0.5,
            "aim at the side of a tree or pole"
        );
        self.update_hive_anchors()?;
        ensure!(
            self.garden_bees.saved.hives.len() < MAX_HIVES,
            "at most {MAX_HIVES} bee houses"
        );
        ensure!(
            !self
                .garden_bees
                .saved
                .hives
                .iter()
                .any(|h| h.anchor == anchor),
            "this tree or pole already has a bee house"
        );
        let facing = Vec3::new(facing.x, 0., facing.z).normalize_or_zero();
        let point = contact * 256. - facing * 0.51;
        ensure!(
            point.cmpge(Vec3::splat(9.)).all(),
            "bee house needs space above ground and inside the garden"
        );
        let root = point.floor().as_uvec3();
        let id = self.garden_bees.saved.next_id;
        let next = id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("bee-house identities exhausted"))?;
        let h = Hive {
            id,
            anchor,
            support: root.to_array(),
            facing: facing.to_array(),
        };
        HiveSnapshot {
            next_id: next,
            hives: vec![h.clone()],
        }
        .validate(
            self.world_chunk_dim * 256,
            |id| self.trees.contains(id),
            |id| self.garden_fence.saved.contains_post(id),
        )?;
        ensure!(
            !self
                .garden_bees
                .sites
                .iter()
                .any(|s| s.position.distance(h.rest_site().position) < 1.8 * HIVE_SCALE),
            "another bee house is too close"
        );
        if let Anchor::Pole(id) = anchor {
            ensure!(self.garden_fence.post_owns(id, root), "not a pole surface");
        }
        if let Anchor::Tree(id) = anchor {
            ensure!(
                self.trees
                    .resolve_hive_pose(id, root, h.rest_site().position, facing, false, &mut None)
                    .is_some(),
                "not this tree's wood"
            );
        }
        let bound = h.bound();
        let dim = bound.max() - bound.min();
        let cells = self
            .plain_builder
            .read_chunk_atlas_region(bound.min(), dim)?;
        ensure!(
            h.space_clear(&cells),
            "bee house needs exposed host wood and a clear entrance/body"
        );
        self.garden_bees.saved.hives.push(h);
        self.garden_bees.saved.next_id = next;
        self.garden_bees.dirty.insert(id);
        self.update_hive_anchors()?;
        log::info!("[BEE_GARDEN] placed={id} anchor={anchor:?} support={root:?} cached_model=true terrain_voxels=false");
        Ok(id)
    }
    pub(super) fn try_bee_house_action(&mut self, remove: bool) {
        if self.player_tools.selected_tool() != PlayerTool::Hive
            || !self.terrain_edit_pointer_available()
            || !self.terrain_persistence.can_start_operation()
        {
            return;
        }
        let result = (|| -> Result<()> {
            let pick = self
                .pointer_picking
                .active
                .ok_or_else(|| anyhow::anyhow!("aim at a tree or pole"))?;
            self.update_hive_anchors()?;
            if remove {
                let limit = pick.hit.map_or(2., |h| h.distance + 0.02).min(2.);
                let hit = self
                    .garden_bees
                    .sites
                    .iter()
                    .filter_map(|s| {
                        let t = (s.position - pick.ray.origin).dot(pick.ray.direction);
                        (t >= 0.
                            && t <= limit
                            && (pick.ray.origin + pick.ray.direction * t).distance(s.position)
                                < 1.1 * HIVE_SCALE)
                            .then_some((t, s.id))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                let (_, id) =
                    hit.ok_or_else(|| anyhow::anyhow!("aim at a bee house to remove it"))?;
                self.garden_bees.saved.hives.retain(|h| h.id != id);
                self.garden_bees.bindings.remove(&id);
                self.update_hive_anchors()?;
                self.garden_feedback = Some("Removed bee house".into());
                return Ok(());
            }
            let hit = pick
                .hit
                .ok_or_else(|| anyhow::anyhow!("aim at a tree or pole"))?;
            ensure!(hit.distance <= 2., "move closer to the tree or pole");
            let facing =
                Vec3::new(-pick.ray.direction.x, 0., -pick.ray.direction.z).normalize_or_zero();
            let contact = hit.tree_rest_position().unwrap_or(hit.world_position);
            let cell = (contact * 256. - facing * 0.51)
                .floor()
                .max(Vec3::ZERO)
                .as_uvec3();
            let tree = self.picked_tree_id(hit).or_else(|| {
                (hit.voxel_type == VOXEL_TYPE_CHERRY_WOOD)
                    .then(|| self.trees.hive_tree_at(cell))
                    .flatten()
            });
            let anchor =
                if let Some(id) = tree {
                    Anchor::Tree(id)
                } else {
                    Anchor::Pole(self.garden_fence.post_at(cell).ok_or_else(|| {
                        anyhow::anyhow!("bee houses need a real tree or garden pole")
                    })?)
                };
            self.place_bee_house(anchor, contact, facing)?;
            self.garden_feedback = Some("Mounted bee house · plant flowers nearby".into());
            Ok(())
        })();
        if let Err(error) = result {
            self.garden_feedback = Some(error.to_string());
        }
    }
    pub(super) fn append_bee_garden_models(&mut self, frame: ButterflyFrame) {
        if self.render_flags.enable_particles {
            // Sample the raw presentation clock, not the already quantized butterfly frame.
            let bee_frame = ButterflyFrame::at(
                self.butterfly_presentation_time(),
                self.debug_settings.adjustables.bee_animation_fps.value,
            );
            self.garden_bees.swarm.append(
                bee_frame,
                self.debug_settings.adjustables.bee_size_scale.value,
                &mut self.particle_snapshots,
            );
        }
        crate::particles::bees::append_hives(
            &self.garden_bees.sites,
            frame,
            &mut self.particle_snapshots,
        );
    }
    pub(super) fn update_hive_models_without_particles(&mut self) {
        self.garden_bees.swarm.clear();
        self.particle_snapshots.clear();
        let frame = ButterflyFrame::at(
            self.butterfly_presentation_time(),
            self.debug_settings
                .adjustables
                .butterfly_animation_fps
                .value,
        );
        self.append_bee_garden_models(frame);
        self.upload_particle_snapshots();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mounting_space_checks_support_burial_and_nearby_edits_without_polling() {
        for facing in [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z] {
            let h = Hive {
                id: 1,
                anchor: Anchor::Tree(3),
                support: [100, 100, 100],
                facing: facing.to_array(),
            };
            let bound = h.bound();
            let dim = bound.max() - bound.min();
            let index = |p: UVec3| {
                let q = p - bound.min();
                (q.x + dim.x * (q.y + dim.y * q.z)) as usize
            };
            let mut bytes = vec![0; dim.element_product() as usize];
            let root = UVec3::from(h.support);
            bytes[index(root)] = VOXEL_TYPE_CHERRY_WOOD as u8;
            assert!(h.space_clear(&bytes));
            let front = (root.as_vec3() + facing).as_uvec3();
            bytes[index(front)] = VOXEL_TYPE_OAK_WOOD as u8;
            assert!(!h.space_clear(&bytes));
            let mut garden = GardenBees::default();
            garden.saved = HiveSnapshot {
                next_id: 2,
                hives: vec![h],
            };
            garden.observe_edit(UAabb3::new(UVec3::ZERO, UVec3::ONE));
            assert!(garden.dirty.is_empty());
            garden.observe_edit(UAabb3::new(front, front + UVec3::ONE));
            assert_eq!(garden.dirty.len(), 1);
            bytes[index(front)] = 0;
            bytes[index(root)] = 0;
            assert!(!garden.saved.hives[0].space_clear(&bytes));
        }
    }
    #[test]
    fn saved_houses_are_bounded_unique_and_have_live_anchor_identities() {
        let hive = Hive {
            id: 1,
            anchor: Anchor::Tree(3),
            support: [100, 100, 100],
            facing: [0., 0., 1.],
        };
        let mut snapshot = HiveSnapshot {
            next_id: 2,
            hives: vec![hive],
        };
        snapshot
            .validate(UVec3::splat(512), |id| id == 3, |_| false)
            .unwrap();
        assert!(snapshot
            .validate(UVec3::splat(512), |_| false, |_| false)
            .is_err());
        let restored: HiveSnapshot =
            serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
        assert_eq!(restored.hives[0].anchor, Anchor::Tree(3));
        snapshot.hives.push(snapshot.hives[0].clone());
        assert!(snapshot
            .validate(UVec3::splat(512), |_| true, |_| true)
            .is_err());
        snapshot.hives.pop();
        snapshot.hives[0].facing = [f32::NAN, 0., 1.];
        assert!(snapshot
            .validate(UVec3::splat(512), |_| true, |_| true)
            .is_err());
    }
}
