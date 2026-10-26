//! Ground-planted voxel nests own their recipe and style; terrain owns their solid cells.
//! Bee residents are transient. No tree/post attachments or wind skinning.
use super::{player_tools::PlayerTool, App};
use crate::{
    app::world_edits::WorldEditTransaction,
    builder::{VOXEL_TYPE_DIRT, VOXEL_TYPE_MASK, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND},
    geom::UAabb3,
    particles::{
        bees::{Bees, HiveSite, MAX_HIVES},
        ButterflyFrame,
    },
};
use anyhow::{ensure, Result};
use glam::{UVec3, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod geometry;
mod review;
use geometry::HiveStyle;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Hive {
    id: u32,
    base: [u32; 3],
    style: HiveStyle,
    quarter_turn: u8,
    #[serde(default = "legacy_hive_scale")]
    scale: f32,
}
fn legacy_hive_scale() -> f32 {
    1.0
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "SavedHives")]
pub(super) struct HiveSnapshot {
    next_id: u32,
    hives: Vec<Hive>,
}
// Decode-only migration: old model attachments never owned any terrain cells.
// Retiring them must not remove or change their tree/post hosts.
#[derive(Deserialize)]
struct SavedHives {
    next_id: u32,
    hives: Vec<SavedHive>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum SavedHive {
    Voxel(Hive),
    Retired {
        #[serde(rename = "anchor")]
        _anchor: RetiredAnchor,
        #[serde(rename = "support")]
        _support: [u32; 3],
        #[serde(rename = "facing")]
        _facing: [f32; 3],
    },
}
#[derive(Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
enum RetiredAnchor {
    Tree(u32),
    Pole(u32),
}
impl From<SavedHives> for HiveSnapshot {
    fn from(saved: SavedHives) -> Self {
        Self {
            next_id: saved.next_id,
            hives: saved
                .hives
                .into_iter()
                .filter_map(|h| match h {
                    SavedHive::Voxel(h) => Some(h),
                    SavedHive::Retired {
                        _anchor: RetiredAnchor::Tree(id) | RetiredAnchor::Pole(id),
                        ..
                    } => {
                        log::info!(
                            "[BEE_GARDEN] retired_legacy_attachment host={id} host_unchanged=true"
                        );
                        None
                    }
                })
                .collect(),
        }
    }
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
    pub(super) fn validate(&self, world: UVec3) -> Result<()> {
        ensure!(
            self.next_id > 0 && self.hives.len() <= MAX_HIVES,
            "invalid hive population"
        );
        let mut ids = BTreeSet::new();
        let mut occupied = BTreeSet::new();
        for h in &self.hives {
            ensure!(
                h.id > 0 && h.id < self.next_id && ids.insert(h.id),
                "invalid hive identity"
            );
            ensure!(
                h.scale.is_finite() && (0.4..=2.0).contains(&h.scale),
                "invalid hive scale"
            );
            let base = UVec3::from(h.base);
            let (below, above) = h.bound_margins();
            ensure!(
                h.quarter_turn < 4
                    && base.cmpge(below).all()
                    && base.cmplt(world).all()
                    && (world - base).cmpge(above).all(),
                "hive outside world"
            );
            ensure!(
                h.cells().keys().all(|p| occupied.insert(*p)),
                "overlapping hive recipes"
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
    active: BTreeSet<u32>,
    review_started: Option<f32>,
}
impl GardenBees {
    pub(super) fn restore(&mut self, saved: HiveSnapshot) {
        self.dirty = saved.hives.iter().map(|h| h.id).collect();
        self.saved = saved;
        self.active.clear();
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
        let hives: Vec<_> = self
            .garden_bees
            .saved
            .hives
            .iter()
            .filter(|h| self.garden_bees.dirty.contains(&h.id))
            .collect();
        if !hives.is_empty() {
            let regions: Vec<_> = hives
                .iter()
                .map(|h| {
                    let b = h.bound();
                    (b.min(), b.max() - b.min())
                })
                .collect();
            let bytes = self.plain_builder.read_chunk_atlas_regions(&regions)?;
            for (h, bytes) in hives.iter().zip(bytes) {
                if h.intact(&bytes) {
                    self.garden_bees.active.insert(h.id);
                } else {
                    self.garden_bees.active.remove(&h.id);
                }
            }
        }
        self.garden_bees.dirty.clear();
        self.garden_bees.sites = self
            .garden_bees
            .saved
            .hives
            .iter()
            .filter(|h| self.garden_bees.active.contains(&h.id))
            .map(Hive::site)
            .collect();
        self.garden_bees
            .swarm
            .update(0., &self.garden_bees.sites, &[], false);
        Ok(())
    }
    fn place_voxel_hive(&mut self, contact: Vec3, facing: Vec3) -> Result<u32> {
        self.update_hive_anchors()?;
        ensure!(
            contact.is_finite() && facing.is_finite(),
            "invalid hive placement"
        );
        ensure!(
            self.garden_bees.saved.hives.len() < MAX_HIVES,
            "at most {MAX_HIVES} hives"
        );
        let id = self.garden_bees.saved.next_id;
        let next_id = id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("hive identities exhausted"))?;
        let p = contact * 256.;
        let base = Vec3::new(p.x.floor(), p.y.ceil(), p.z.floor());
        ensure!(
            base.cmpge(Vec3::ZERO).all()
                && base.cmplt((self.world_chunk_dim * 256).as_vec3()).all(),
            "hive outside garden"
        );
        let quarter_turn = if facing.x.abs() > facing.z.abs() {
            if facing.x > 0. {
                1
            } else {
                3
            }
        } else if facing.z < 0. {
            2
        } else {
            0
        };
        let h = Hive {
            id,
            base: base.as_uvec3().to_array(),
            quarter_turn,
            style: HiveStyle::Artificial,
            scale: self.debug_settings.adjustables.hive_size_scale.value,
        };
        let mut candidate = self.garden_bees.saved.clone();
        candidate.next_id = next_id;
        candidate.hives.push(h.clone());
        candidate.validate(self.world_chunk_dim * 256)?;
        let bound = h.bound();
        let dim = bound.max() - bound.min();
        let before = self
            .plain_builder
            .read_chunk_atlas_region(bound.min(), dim)?;
        let at = |p: UVec3| {
            let q = p - bound.min();
            u32::from(before[(q.x + dim.x * (q.y + dim.y * q.z)) as usize] & VOXEL_TYPE_MASK)
        };
        ensure!(
            matches!(
                at(UVec3::from(h.base) - UVec3::Y),
                VOXEL_TYPE_DIRT | VOXEL_TYPE_ROCK | VOXEL_TYPE_SAND
            ),
            "place the hive on soil or rock"
        );
        let cells = h.cells();
        ensure!(
            cells.keys().all(|p| at((*p).into()) == 0) && at(h.entrance_cell()) == 0,
            "hive or entrance obstructed"
        );
        let mut materials = BTreeMap::<u32, BTreeSet<[u32; 3]>>::new();
        for (p, material) in cells {
            materials.entry(material).or_default().insert(p);
        }
        let edits = materials
            .iter()
            .map(|(m, cells)| super::garden_fence::stamp(cells, *m))
            .collect::<Result<Vec<_>>>()?;
        self.execute_world_edit(WorldEditTransaction::terrain_change(edits, bound))?;
        self.observe_player_terrain_publication_for_connectivity(bound);
        self.garden_bees.saved = candidate;
        self.garden_bees.dirty.insert(id);
        self.update_hive_anchors()?;
        log::info!(
            "[BEE_GARDEN] placed={id} style={:?} actual_voxels=true integrated_post=true",
            h.style
        );
        Ok(id)
    }
    fn remove_voxel_hive(&mut self, id: u32) -> Result<()> {
        let h = self
            .garden_bees
            .saved
            .hives
            .iter()
            .find(|h| h.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("hive no longer exists"))?;
        let bound = h.bound();
        let dim = bound.max() - bound.min();
        let bytes = self
            .plain_builder
            .read_chunk_atlas_region(bound.min(), dim)?;
        // Preserve unrelated terrain and replacements of a different material.
        let cells = h
            .cells()
            .into_iter()
            .filter_map(|(p, material)| {
                let q = UVec3::from(p) - bound.min();
                (u32::from(bytes[(q.x + dim.x * (q.y + dim.y * q.z)) as usize] & VOXEL_TYPE_MASK)
                    == material)
                    .then_some(p)
            })
            .collect::<BTreeSet<_>>();
        if !cells.is_empty() {
            self.execute_world_edit(WorldEditTransaction::terrain_change(
                vec![super::garden_fence::stamp(&cells, 0)?],
                bound,
            ))?;
            self.observe_player_terrain_publication_for_connectivity(bound);
        }
        self.garden_bees.saved.hives.retain(|h| h.id != id);
        self.garden_bees.active.remove(&id);
        self.update_hive_anchors()?;
        Ok(())
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
                .ok_or_else(|| anyhow::anyhow!("aim at the ground"))?;
            let hit = pick
                .hit
                .ok_or_else(|| anyhow::anyhow!("aim at the ground"))?;
            ensure!(hit.distance <= 2., "move closer");
            if remove {
                let cell = (hit.world_position * 256. + pick.ray.direction * 0.05)
                    .floor()
                    .max(Vec3::ZERO)
                    .as_uvec3()
                    .to_array();
                let id = self
                    .garden_bees
                    .saved
                    .hives
                    .iter()
                    .find(|h| h.cells().contains_key(&cell))
                    .map(|h| h.id)
                    .ok_or_else(|| anyhow::anyhow!("aim at a hive or its integrated post"))?;
                self.remove_voxel_hive(id)?;
                self.garden_feedback = Some("Removed hive and post".into());
            } else {
                ensure!(
                    hit.tree_primitive.is_none(),
                    "place the hive on the ground, not on a tree"
                );
                let snapshot = self.contree_builder.cpu_ray_query_snapshot();
                let normal = super::stone_placement::surface_normal(
                    hit.world_position,
                    pick.ray.direction,
                    |o, d| snapshot.query_terrain_ray_cpu(o, d).map(|h| h.position),
                );
                ensure!(
                    normal.is_some_and(|n| n.y >= 0.75),
                    "aim at an upward-facing ground surface"
                );
                self.place_voxel_hive(hit.world_position, -pick.ray.direction)?;
                self.garden_feedback = Some("Planted hive · plant flowers nearby".into());
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.garden_feedback = Some(error.to_string());
        }
    }
    pub(super) fn append_bee_garden_models(&mut self, _frame: ButterflyFrame) {
        if self.render_flags.enable_particles {
            let frame = ButterflyFrame::at(
                self.butterfly_presentation_time(),
                self.debug_settings.adjustables.bee_animation_fps.value,
            );
            self.garden_bees
                .swarm
                .append(frame, &mut self.particle_snapshots);
        }
    }
    pub(super) fn update_hive_models_without_particles(&mut self) {
        self.garden_bees.swarm.clear();
        self.particle_snapshots.clear();
        self.upload_particle_snapshots();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn styles_survive_round_trip_and_old_attachments_retire_without_hosts() {
        let saved = HiveSnapshot {
            next_id: 3,
            hives: vec![
                Hive {
                    id: 1,
                    base: [100; 3],
                    style: HiveStyle::Artificial,
                    quarter_turn: 0,
                    scale: 0.6,
                },
                Hive {
                    id: 2,
                    base: [150; 3],
                    style: HiveStyle::Natural,
                    quarter_turn: 3,
                    scale: 1.0,
                },
            ],
        };
        saved.validate(UVec3::splat(512)).unwrap();
        let restored: HiveSnapshot =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(restored.hives, saved.hives);
        let mut old = serde_json::to_value(&saved).unwrap();
        for h in old["hives"].as_array_mut().unwrap() {
            h.as_object_mut().unwrap().remove("scale");
        }
        let old: HiveSnapshot = serde_json::from_value(old).unwrap();
        assert!(old.hives.iter().all(|h| h.scale == 1.0));
        let mut invalid = saved.clone();
        for scale in [f32::NAN, 0., 3.] {
            invalid.hives[0].scale = scale;
            assert!(invalid.validate(UVec3::splat(512)).is_err());
        }
        let legacy: HiveSnapshot = serde_json::from_str(r#"{"next_id":9,"hives":[{"id":1,"anchor":{"kind":"tree","id":4},"support":[100,100,100],"facing":[0,0,1]},{"id":2,"anchor":{"kind":"pole","id":5},"support":[150,100,100],"facing":[0,0,1]}]}"#).unwrap();
        assert!(legacy.hives.is_empty());
        assert_eq!(legacy.next_id, 9);
        for base in [[u32::MAX; 3], [0; 3], [500; 3]] {
            let mut bad = saved.clone();
            bad.hives[0].base = base;
            assert!(bad.validate(UVec3::splat(512)).is_err());
        }
        let mut overlap = saved.clone();
        overlap.hives[1].base = overlap.hives[0].base;
        assert!(overlap.validate(UVec3::splat(512)).is_err());
    }
}
