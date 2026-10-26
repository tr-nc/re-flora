//! Opt-in native fixtures, not manual interaction or taxonomic realism approval.
use super::*;
use crate::app::{
    core::planting::AuthoredFloraPlacementBatch,
    world_edits::{TerrainBrushEdit, VoxelEdit, WorldEditTransaction},
};
use anyhow::Context;
impl App {
    pub(in crate::app::core) fn seed_bee_garden_review(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_BEE_REVIEW") else {
            return Ok(());
        };
        ensure!(
            [
                "garden",
                "load",
                "edit",
                "cycle",
                "flowers-removed",
                "particles-off",
                "without",
                "cap",
                "tree-delete",
                "tree-view",
                "native",
                "bury"
            ]
            .contains(&mode.as_str()),
            "unknown bee review mode"
        );
        self.set_manual_time_of_day(0.45);
        if mode == "load" {
            self.update_hive_anchors()?;
            ensure!(
                self.garden_bees.saved.hives.len() == 2 && self.garden_bees.saved.next_id == 3,
                "bee-house save did not retain both anchors"
            );
            log::info!("[BEE_REVIEW] loaded_houses=2 tree_and_pole=true verified=true");
        } else {
            for id in self.capture_tree_snapshot().ids().collect::<Vec<_>>() {
                self.remove_tree(id)?;
            }
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            let base = self
                .query_terrain_ray_cpu(Vec3::new(0.82, 3., 1.22), Vec3::NEG_Y)
                .context("tree ground")?
                .position;
            let mut desc = crate::tree_gen::TreeSpecies::AppleTree.default_desc();
            desc.branching.seed = 907;
            let tree = self.plant_growing_tree(desc, base)?;
            self.advance_tree_growth(900.001)?;
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            let ground = self
                .query_terrain_ray_cpu(Vec3::new(1.08, 3., 1.22), Vec3::NEG_Y)
                .context("pole ground")?
                .position;
            let pole = self.place_garden_post(ground, true)?;
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            // The thin, curved leader need not intersect a ray at the planted
            // x coordinate. Select an actual exposed rest-atlas contact instead.
            let lo = (base * 256.).floor().as_uvec3() - UVec3::new(24, 0, 24);
            let dim = UVec3::new(49, 42, 49);
            let trunk = self.plain_builder.read_chunk_atlas_region(lo, dim)?;
            let mut contacts = Vec::new();
            for y in 5..40 {
                for x in 1..48 {
                    for z in 1..48 {
                        let index = (x + 49 * (y + 42 * z)) as usize;
                        if u32::from(trunk[index] & VOXEL_TYPE_MASK) != VOXEL_TYPE_CHERRY_WOOD {
                            continue;
                        }
                        for (offset, n) in [
                            (1, Vec3::X),
                            (-1, -Vec3::X),
                            (2058, Vec3::Z),
                            (-2058, -Vec3::Z),
                        ] {
                            if trunk[(index as i32 + offset) as usize] & VOXEL_TYPE_MASK == 0 {
                                contacts.push((
                                    ((lo + UVec3::new(x, y, z)).as_vec3()
                                        + Vec3::splat(0.5)
                                        + n * 0.5)
                                        / 256.,
                                    n,
                                ));
                            }
                        }
                    }
                }
            }
            let score = |(p, n): (Vec3, Vec3)| {
                p.distance_squared(base + Vec3::Y * (9. / 256.)) + (1. - n.z) * 0.0001
            };
            contacts.sort_by(|a, b| score(*a).total_cmp(&score(*b)));
            let contact = contacts.first().context("exposed tree trunk for hive")?.0;
            let origin = UVec3::new(128, 80, 256);
            let size = UVec3::new(192, 128, 128);
            let before = self.plain_builder.read_chunk_atlas_region(origin, size)?;
            if mode != "without" {
                let mut placed = false;
                for &(p, n) in contacts.iter().take(512) {
                    if self.place_bee_house(Anchor::Tree(tree), p, n).is_ok() {
                        let root = (p * 256. - n * 0.51).floor().as_uvec3();
                        ensure!(
                            self.trees.hive_tree_at(root) == Some(tree),
                            "voxel-fallback mount lost canonical tree ownership"
                        );
                        placed = true;
                        break;
                    }
                }
                ensure!(placed, "no clear mounting face on the review tree");
                self.place_bee_house(
                    Anchor::Pole(pole),
                    self.garden_fence.review_post_contact(pole).unwrap(),
                    Vec3::Z,
                )
                .context("pole house")?;
                ensure!(
                    self.place_bee_house(Anchor::Tree(tree), contact, Vec3::Z)
                        .is_err()
                        && self.garden_bees.saved.hives.len() == 2,
                    "duplicate hive admitted"
                );
                ensure!(
                    self.place_bee_house(Anchor::Tree(99999), contact, Vec3::Z)
                        .is_err(),
                    "missing host admitted"
                );
            }
            ensure!(
                before == self.plain_builder.read_chunk_atlas_region(origin, size)?,
                "hive model modified terrain"
            );
            let species = crate::flora::models::FLOWER_REGISTRY
                .iter()
                .position(|(id, _, _)| *id == "oxeye-daisy")
                .context("daisy registry")? as u32
                + crate::flora::species::MODEL_FLOWER_FIRST_SPECIES;
            let mut batch = AuthoredFloraPlacementBatch::new();
            for (i, x) in [0.72, 0.80, 0.89, 0.98, 1.07, 1.16].into_iter().enumerate() {
                let p = self
                    .query_terrain_ray_cpu(Vec3::new(x, 3., 1.38), Vec3::NEG_Y)
                    .context("flower ground")?
                    .position;
                self.apply_surface_flora_removal(TerrainBrushEdit {
                    start: p,
                    end: p,
                    radius: 2. / 256.,
                })?;
                let anchor = self
                    .resolve_plantable_contact(p)?
                    .context("daisy contact")?;
                ensure!(
                    self.try_place_authored_flora(
                        &mut batch,
                        species,
                        anchor,
                        255,
                        0,
                        900 + i as u32
                    ),
                    "daisy insertion failed"
                );
            }
            self.finish_authored_flora_placement(batch)?;
            self.debug_settings
                .adjustables
                .flora_growth_override_enabled
                .value = true;
            self.debug_settings.adjustables.flora_growth_override.value = 1.;
            if mode == "edit" {
                let houses = self.garden_bees.saved.hives.clone();
                for h in houses {
                    let p = UVec3::from(h.support);
                    let bound = UAabb3::new(p, p + UVec3::ONE);
                    self.execute_world_edit(WorldEditTransaction::terrain_change(
                        vec![VoxelEdit::ClearVoxelRegion(
                            crate::app::world_edits::ClearVoxelRegionEdit {
                                offset: p,
                                dim: UVec3::ONE,
                            },
                        )],
                        bound,
                    ))?;
                }
                self.update_hive_anchors()?;
                ensure!(
                    self.garden_bees.saved.hives.is_empty() && self.garden_bees.saved.next_id == 3,
                    "host editing left stale hive identity"
                );
                log::info!(
                    "[BEE_REVIEW] edited_supports=2 remaining_houses=0 next_id=3 verified=true"
                );
            }
            if mode == "bury" {
                let h = self.garden_bees.saved.hives[0].clone();
                let p = (Vec3::from_array(h.support.map(|v| v as f32))
                    + Vec3::from_array(h.facing))
                .as_uvec3();
                self.execute_world_edit(WorldEditTransaction::terrain_change(
                    vec![VoxelEdit::RestoreAtlasRegion {
                        origin: p,
                        dim: UVec3::ONE,
                        data: vec![crate::builder::VOXEL_TYPE_ROCK as u8].into(),
                    }],
                    UAabb3::new(p, p + UVec3::ONE),
                ))?;
                self.update_hive_anchors()?;
                ensure!(
                    self.garden_bees.saved.hives.len() == 1
                        && self.garden_bees.saved.hives[0].anchor == Anchor::Pole(pole),
                    "buried house survived"
                );
                ensure!(
                    self.plain_builder.read_chunk_atlas_region(p, UVec3::ONE)?[0] & VOXEL_TYPE_MASK
                        == crate::builder::VOXEL_TYPE_ROCK as u8,
                    "house retirement erased the new rock"
                );
                log::info!("[BEE_REVIEW] burial_retired_house=true rock_preserved=true remaining_pole_house=1 verified=true");
            }
            if mode == "cap" {
                let p = self.garden_fence.review_post_cap(pole).unwrap();
                self.execute_world_edit(WorldEditTransaction::terrain_change(
                    vec![VoxelEdit::ClearVoxelRegion(
                        crate::app::world_edits::ClearVoxelRegionEdit {
                            offset: p,
                            dim: UVec3::ONE,
                        },
                    )],
                    UAabb3::new(p, p + UVec3::ONE),
                ))?;
                self.update_hive_anchors()?;
                ensure!(
                    self.garden_bees.saved.hives.len() == 1
                        && self.garden_bees.saved.hives[0].anchor == Anchor::Tree(tree)
                        && !self.garden_fence.saved.contains_post(pole),
                    "deleted post cap retained its hive"
                );
                log::info!("[BEE_REVIEW] cap_removed=true pole_identity_retired=true remaining_tree_house=1 verified=true");
            }
            if mode == "tree-delete" {
                self.remove_tree(tree)?;
                self.update_hive_anchors()?;
                ensure!(
                    self.garden_bees.saved.hives.len() == 1
                        && self.garden_bees.saved.hives[0].anchor == Anchor::Pole(pole),
                    "deleted tree retained its hive"
                );
                log::info!("[BEE_REVIEW] tree_removed=true remaining_pole_house=1 verified=true");
            }
            log::info!("[BEE_REVIEW] houses={} flowers=6 atlas_unchanged_by_houses=true original_cached_models=true",self.garden_bees.saved.hives.len());
        }
        if mode == "particles-off" {
            self.render_flags.enable_particles = false;
        }
        let target = Vec3::new(0.94, 0.58, 1.28);
        self.camera_control.apply_snapshot_mode(true);
        self.camera_control.set_orbit_focus(target);
        self.tracer
            .set_camera_pose_looking_at(target + Vec3::new(0.12, 0.15, 0.46), target);
        if mode == "native" {
            self.debug_settings
                .adjustables
                .butterfly_cache_enabled
                .value = false;
        }
        if mode == "tree-view" {
            let site = self.garden_bees.sites[0];
            self.render_flags.enable_leaves = false;
            self.debug_settings.tree.render_leaves = false;
            self.render_flags.enable_flora = false;
            self.debug_settings.adjustables.fruit_cycle.value = 0.;
            self.terrain_physics.set_fruit_cycle(0., &mut self.tracer)?;
            self.tracer.set_camera_pose_looking_at(
                site.position + site.rotation * Vec3::Z * 0.16 + Vec3::Y * 0.03,
                site.position,
            );
        }
        self.reset_camera_movement_input();
        self.garden_bees.review = Some((mode, 0., false));
        Ok(())
    }
    pub(in crate::app::core) fn review_bee_garden(&mut self) -> Result<()> {
        let Some((mode, mut start, mut switched)) = self.garden_bees.review.clone() else {
            return Ok(());
        };
        let now = self.time_info.time_since_start_duration().as_secs_f32();
        if start == 0. {
            start = now;
            self.garden_bees.review = Some((mode.clone(), start, switched));
        }
        let elapsed = now - start;
        if !switched && elapsed > 6. && matches!(mode.as_str(), "cycle" | "flowers-removed") {
            ensure!(
                self.garden_bees.swarm.visits > 0,
                "bees never reached GPU flower contacts"
            );
            if mode == "cycle" {
                self.set_manual_time_of_day(0.08);
            } else {
                let points: Vec<_> = self
                    .surface_builder
                    .authored_flora_base_positions()
                    .map(|(_, p)| (p.as_vec3() + Vec3::splat(0.5)) / 256.)
                    .collect();
                for p in points {
                    self.apply_surface_flora_removal(TerrainBrushEdit {
                        start: p,
                        end: p,
                        radius: 3. / 256.,
                    })?;
                }
            }
            switched = true;
            self.garden_bees.review = Some((mode.clone(), start, true));
        }
        if elapsed > 14. {
            let bees = &self.garden_bees.swarm;
            match mode.as_str() {
                "tree-view" => ensure!(
                    self.garden_bees.saved.hives.len() == 2,
                    "diagnostic view lost houses"
                ),
                "garden" | "load" | "cap" | "tree-delete" | "native" | "bury" => ensure!(
                    bees.visits > 0 && bees.returns > 0,
                    "incomplete bee foraging cycle"
                ),
                "cycle" | "flowers-removed" => ensure!(
                    switched && bees.outside() == 0 && bees.returns > 0,
                    "bees did not return to their nest"
                ),
                "particles-off" => ensure!(
                    bees.len() == 0
                        && self
                            .particle_snapshots
                            .iter()
                            .filter(|p| p.kind == crate::particles::ParticleRenderKind::Hive)
                            .count()
                            == 2,
                    "particle toggle hid persistent houses"
                ),
                "edit" | "without" => ensure!(bees.len() == 0, "bees without a house"),
                _ => unreachable!(),
            }
            log::info!("[BEE_REVIEW] mode={mode} houses={} residents={} outside={} visits={} returns={} lost_hosts={} verified=true",self.garden_bees.sites.len(),bees.len(),bees.outside(),bees.visits,bees.returns,bees.lost_hosts);
            self.garden_bees.review = None;
        }
        Ok(())
    }
}
