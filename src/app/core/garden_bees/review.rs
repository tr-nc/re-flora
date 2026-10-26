//! Opt-in real atlas/publication/save fixtures for ground-planted nests.
use super::*;
use crate::app::{core::planting::AuthoredFloraPlacementBatch, world_edits::TerrainBrushEdit};
use anyhow::Context;
impl App {
    pub(in crate::app::core) fn seed_bee_garden_review(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_BEE_REVIEW") else {
            return Ok(());
        };
        ensure!(
            ["voxel", "voxel-edit", "voxel-load", "particles-off"].contains(&mode.as_str()),
            "use voxel, voxel-edit, voxel-load or particles-off bee review"
        );
        self.set_manual_time_of_day(0.45);
        if mode != "voxel-load" {
            for id in self.capture_tree_snapshot().ids().collect::<Vec<_>>() {
                self.remove_tree(id)?;
            }
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            let old_style = self.debug_settings.adjustables.hive_style.value;
            for (style, x) in [(0, 0.82), (1, 1.02)] {
                let ground = self
                    .query_terrain_ray_cpu(Vec3::new(x, 3., 1.22), Vec3::NEG_Y)
                    .context("hive ground")?
                    .position;
                self.debug_settings.adjustables.hive_style.value = style;
                self.place_voxel_hive(ground, Vec3::Z)?;
                let next = self.garden_bees.saved.next_id;
                ensure!(
                    self.place_voxel_hive(ground, Vec3::Z).is_err(),
                    "overlap accepted"
                );
                ensure!(
                    self.garden_bees.saved.next_id == next,
                    "rejected placement consumed identity"
                );
            }
            self.debug_settings.adjustables.hive_style.value = old_style;
            let species = crate::flora::models::FLOWER_REGISTRY
                .iter()
                .position(|(id, _, _)| *id == "oxeye-daisy")
                .context("daisy")? as u32
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
                    "daisy placement"
                );
            }
            self.finish_authored_flora_placement(batch)?;
            self.debug_settings
                .adjustables
                .flora_growth_override_enabled
                .value = true;
            self.debug_settings.adjustables.flora_growth_override.value = 1.;
        }
        self.garden_bees
            .saved
            .validate(self.world_chunk_dim * 256)?;
        self.update_hive_anchors()?;
        ensure!(
            self.garden_bees.sites.len() == 2,
            "two live voxel nests expected"
        );
        ensure!(
            self.garden_bees.saved.hives[0].style == HiveStyle::Artificial
                && self.garden_bees.saved.hives[1].style == HiveStyle::Natural,
            "placement-only style not retained"
        );
        let saved: HiveSnapshot =
            serde_json::from_slice(&serde_json::to_vec(&self.garden_bees.saved)?)?;
        self.garden_bees.restore(saved);
        self.update_hive_anchors()?;
        ensure!(
            self.garden_bees.sites.len() == 2,
            "restore lost voxel nests"
        );
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        for h in &self.garden_bees.saved.hives {
            let base = UVec3::from(h.base).as_vec3();
            let hit = self
                .query_terrain_ray_cpu((base + Vec3::new(0.5, 10.5, 20.)) / 256., Vec3::NEG_Z)
                .context("solid post CPU ray")?;
            ensure!(
                (hit.position.y * 256. - (base.y + 10.5)).abs() < 0.1,
                "post not solid"
            );
        }
        let target = (self.garden_bees.sites[0].position + self.garden_bees.sites[1].position)
            * 0.5
            - Vec3::Y * 0.04;
        self.camera_control.apply_snapshot_mode(true);
        self.camera_control.set_orbit_focus(target);
        self.tracer
            .set_camera_pose_looking_at(target + Vec3::new(0.10, 0.10, 0.43), target);
        self.reset_camera_movement_input();
        if mode == "voxel-edit" {
            let h = self.garden_bees.saved.hives[0].clone();
            let bound = h.bound();
            let dim = bound.max() - bound.min();
            let before = self
                .plain_builder
                .read_chunk_atlas_region(bound.min(), dim)?;
            let base = UVec3::from(h.base);
            self.execute_world_edit(WorldEditTransaction::terrain_change(
                vec![super::super::garden_fence::stamp(
                    &BTreeSet::from([base.to_array()]),
                    0,
                )?],
                UAabb3::new(base, base + UVec3::ONE),
            ))?;
            self.update_hive_anchors()?;
            ensure!(
                self.garden_bees.sites.len() == 1,
                "damaged nest retained residents"
            );
            self.remove_voxel_hive(h.id)?;
            let after = self
                .plain_builder
                .read_chunk_atlas_region(bound.min(), dim)?;
            let owned = h.cells();
            for z in 0..dim.z {
                for y in 0..dim.y {
                    for x in 0..dim.x {
                        let p = bound.min() + UVec3::new(x, y, z);
                        let i = (x + dim.x * (y + dim.y * z)) as usize;
                        if owned.contains_key(&p.to_array()) {
                            ensure!(after[i] & VOXEL_TYPE_MASK == 0, "removal left hive voxels");
                        } else {
                            ensure!(after[i] == before[i], "removal touched unrelated ground");
                        }
                    }
                }
            }
            ensure!(
                self.garden_bees.saved.hives.len() == 1 && self.garden_bees.saved.next_id == 3,
                "removal identity"
            );
            log::info!("[BEE_VOXEL_REVIEW] edit_remove=true ground_preserved=true verified=true");
        } else if mode == "particles-off" {
            self.render_flags.enable_particles = false;
        } else {
            self.garden_bees.review_started =
                Some(self.time_info.time_since_start_duration().as_secs_f32());
        }
        log::info!("[BEE_VOXEL_REVIEW] mode={mode} styles=2 solid_post=true restore=true placement_only=true verified=true");
        Ok(())
    }
    pub(in crate::app::core) fn review_bee_garden(&mut self) -> Result<()> {
        if let Some(start) = self.garden_bees.review_started {
            if self.time_info.time_since_start_duration().as_secs_f32() - start > 20. {
                let bees = &self.garden_bees.swarm;
                ensure!(
                    bees.visits > 0 && bees.returns > 0,
                    "voxel nest foraging cycle incomplete"
                );
                log::info!(
                    "[BEE_VOXEL_REVIEW] visits={} returns={} verified=true",
                    bees.visits,
                    bees.returns
                );
                self.garden_bees.review_started = None;
            }
        }
        Ok(())
    }
}
