//! Opt-in native fixture, no save/config writes. Uses production ownership,
//! collection visibility, inventory, GPU publication and flora snapshot restore.
use super::*;
use crate::app::core::{garden_economy::GardenEconomy, planting::AuthoredFloraPlacementBatch};
use crate::flora::species;
use glam::UVec2;
impl App {
    pub(super) fn review_vacuum(&mut self) -> Result<()> {
        if std::env::var_os("RE_FLORA_VACUUM_REVIEW").is_none()
            || !self.terrain_persistence.allows_world_updates()
        {
            return Ok(());
        }
        let frame = self.vacuum.review;
        self.vacuum.review += 1;
        if frame == 0 {
            self.debug_settings.adjustables.auto_daynight_cycle.value = false;
            self.set_manual_time_of_day(0.43);
            self.debug_settings
                .adjustables
                .flora_growth_override_enabled
                .value = true;
            self.debug_settings.adjustables.flora_growth_override.value = 1.;
            let mut batch = AuthoredFloraPlacementBatch::new();
            for x in [264, 274, 284] {
                let anchor = self
                    .resolve_plantable_surface_column(UVec2::new(x, 325))
                    .unwrap();
                anyhow::ensure!(
                    self.try_place_authored_flora(
                        &mut batch,
                        species::MODEL_FLOWER_FIRST_SPECIES,
                        anchor,
                        255,
                        0,
                        x
                    ),
                    "vacuum fixture planting"
                );
            }
            self.finish_authored_flora_placement(batch)?;
            let anchor = self
                .resolve_plantable_surface_column(UVec2::new(274, 333))
                .unwrap();
            anyhow::ensure!(
                self.surface_builder
                    .try_insert_finite_grass(0, anchor.base_world_vox(), 255, 0)?,
                "vacuum fixture grass"
            );
            self.material_supply_diagnostic_override = Some(true);
            let pos = Vec3::new(
                0.82,
                self.query_terrain_height_cpu(Vec2::new(0.82, 1.22)),
                1.22,
            );
            self.plant_owned_tree(crate::tree_gen::TreeSpecies::AppleTree.default_desc(), pos)?;
            self.advance_tree_growth(900.)?;
            self.debug_settings.adjustables.fruit_cycle.value = 0.84;
            self.terrain_physics
                .set_fruit_cycle(0.84, &mut self.tracer)?;
            let target = Vec3::new(
                1.07,
                self.query_terrain_height_cpu(Vec2::new(1.07, 1.27)) + 0.035,
                1.27,
            );
            self.camera_control.apply_snapshot_mode(true);
            self.camera_control.set_orbit_focus(target);
            self.tracer
                .set_camera_pose_looking_at(target + Vec3::new(0., 0.24, 0.35), target);
        }
        if frame == 90 {
            let before = self.surface_builder.capture_flora_snapshot(0)?;
            let grass = before.counts().0 - before.counts().1;
            let trees = self.capture_tree_snapshot().count();
            let ground = self.query_terrain_height_cpu(Vec2::new(1.07, 1.27));
            let beam = Beam {
                origin: Vec3::new(1.07, ground + 0.3, 1.27),
                direction: Vec3::NEG_Y,
            };
            let stock = self.garden_economy.clone();
            anyhow::ensure!(
                self.collect_vacuum_beam(Beam {
                    origin: beam.origin - Vec3::Y * 0.35,
                    direction: Vec3::Y
                })? == 0
                    && self.garden_economy == stock,
                "vacuum crossed solid ground"
            );
            let n = self.collect_vacuum_beam(beam)?;
            anyhow::ensure!(n == 3, "vacuum should batch three plants, got {n}");
            let key = format!(
                "seedling/{}",
                species::species()[species::MODEL_FLOWER_FIRST_SPECIES as usize].key
            );
            anyhow::ensure!(
                self.garden_economy.count(&key) == stock.count(&key) + 3,
                "plant credit mismatch"
            );
            anyhow::ensure!(
                self.collect_vacuum_beam(beam)? == 0,
                "repeated collection duplicated plants"
            );
            let after = self.surface_builder.capture_flora_snapshot(0)?;
            anyhow::ensure!(
                after.counts().0 - after.counts().1 == grass
                    && self.capture_tree_snapshot().count() == trees,
                "vacuum removed grass or tree"
            );
            self.surface_builder.restore_flora_snapshot(&after, 0)?;
            let restored: GardenEconomy =
                serde_json::from_slice(&serde_json::to_vec(&self.garden_economy)?)?;
            restored.validate()?;
            anyhow::ensure!(
                restored == self.garden_economy && self.collect_vacuum_beam(beam)? == 0,
                "vacuum ownership lost on reload"
            );
            if let Some((key, p)) = self
                .terrain_physics
                .review_harvest_target(&self.tracer, false)?
            {
                let before = self.garden_economy.apples;
                let count = self.collect_vacuum_beam(Beam {
                    origin: p + Vec3::Y * 0.05,
                    direction: Vec3::NEG_Y,
                })?;
                anyhow::ensure!(
                    count > 0
                        && self.garden_economy.apples > before
                        && !self.terrain_physics.take_harvest_apple(key),
                    "vacuum ripe apple transfer"
                );
            } else {
                anyhow::bail!("vacuum fixture missing ripe fruit");
            }
            self.select_item_panel_slot(super::super::ui_style::VACUUM_SLOT_INDEX);
            log::info!("[VACUUM_REVIEW] passed batch=3 grass_preserved=true trees_preserved=true occlusion=true duplicate=false save_reload=true apples=true");
        }
        Ok(())
    }
}
