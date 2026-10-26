//! Native carrot growth, rendering and ownership review. No persistent save writes.
use super::*;
use crate::app::core::{garden_economy::GardenEconomy, planting::AuthoredFloraPlacementBatch};
use crate::flora::species;
use glam::UVec2;

impl App {
    pub(super) fn review_carrots(&mut self) -> Result<()> {
        if std::env::var_os("RE_FLORA_CARROT_REVIEW").is_none()
            || !self.terrain_persistence.allows_world_updates()
        {
            return Ok(());
        }
        let frame = self.vacuum.carrot_review;
        self.vacuum.carrot_review += 1;
        let carrot = species::species()
            .iter()
            .position(|s| s.key == "carrot")
            .unwrap() as u32;
        if frame == 0 {
            self.debug_settings.adjustables.auto_daynight_cycle.value = false;
            self.set_manual_time_of_day(0.43);
            self.debug_settings
                .adjustables
                .flora_growth_override_enabled
                .value = true;
            self.debug_settings.adjustables.flora_growth_override.value = 1.;
            self.material_supply_diagnostic_override = Some(false);
            self.garden_economy = GardenEconomy::default();
            self.garden_economy.buy("seeds/carrot")?;
            let anchor = self
                .resolve_plantable_surface_column(UVec2::new(274, 325))
                .unwrap();
            anyhow::ensure!(
                self.plant_owned_flora(
                    species::FloraPaintSelection::Species(carrot),
                    anchor.surface_position_ws()
                )?,
                "carrot finite seed placement"
            );
            anyhow::ensure!(
                self.garden_economy.count("seeds/carrot") == 4,
                "carrot seed debit"
            );
            let target = Vec3::new(
                274.5 / 256.,
                anchor.surface_position_ws().y + 0.015,
                325.5 / 256.,
            );
            self.camera_control.apply_snapshot_mode(true);
            self.camera_control.set_orbit_focus(target);
            self.tracer
                .set_camera_pose_looking_at(target + Vec3::new(0., 0.10, 0.18), target);
            let chunk = anchor.base_world_vox() / 256;
            let beam = Beam {
                origin: target + Vec3::Y * 0.2,
                direction: Vec3::NEG_Y,
            };
            anyhow::ensure!(
                self.collect_vacuum_beam(beam)? == 0,
                "immature carrot harvested"
            );
            self.surface_builder
                .update_flora_growth_for_chunk(chunk, 128)?;
            let plant = self
                .surface_builder
                .authored_plants()
                .find(|p| p.species_index == carrot)
                .unwrap();
            anyhow::ensure!(
                plant.growth_progress == 128,
                "carrot did not advance canonical growth"
            );
            let snapshot = self.surface_builder.capture_flora_snapshot(0)?;
            self.surface_builder.restore_flora_snapshot(&snapshot, 0)?;
            anyhow::ensure!(
                self.surface_builder
                    .authored_plants()
                    .any(|p| p.response_id == plant.response_id && p.growth_progress == 128),
                "carrot partial growth lost on reload"
            );
            self.surface_builder
                .update_flora_growth_for_chunk(chunk, 127)?;
            // Leave a mature rosette and younger plants for a real in-game image.
            let mut batch = AuthoredFloraPlacementBatch::new();
            for (x, growth) in [(262, 90), (286, 170)] {
                let anchor = self
                    .resolve_plantable_surface_column(UVec2::new(x, 325))
                    .unwrap();
                anyhow::ensure!(
                    self.try_place_authored_flora(&mut batch, carrot, anchor, growth, 0, x),
                    "carrot age fixture"
                );
            }
            self.finish_authored_flora_placement(batch)?;
            log::info!("[CARROT_REVIEW] planted finite=true growth=0->128->255 partial_reload=true immature_protected=true");
        }
        if frame == 120 {
            anyhow::ensure!(
                self.surface_builder
                    .authored_plants()
                    .any(|p| p.species_index == carrot
                        && p.base_world_vox.x == 262
                        && p.growth_progress > 90),
                "normal app queue did not advance the growing carrot"
            );
            let plant = self
                .surface_builder
                .authored_plants()
                .find(|p| p.species_index == carrot && p.base_world_vox.x == 274)
                .unwrap();
            let p = plant_collection_point(plant);
            let beam = Beam {
                origin: p + Vec3::Y * 0.2,
                direction: Vec3::NEG_Y,
            };
            anyhow::ensure!(
                self.collect_vacuum_beam(beam)? == 1,
                "mature carrot vacuum transfer"
            );
            anyhow::ensure!(
                self.garden_economy.crops.get("carrot") == Some(&1),
                "carrot produce credit"
            );
            anyhow::ensure!(
                self.collect_vacuum_beam(beam)? == 0,
                "duplicate or premature crop transfer"
            );
            let snapshot = self.surface_builder.capture_flora_snapshot(0)?;
            self.surface_builder.restore_flora_snapshot(&snapshot, 0)?;
            let restored: GardenEconomy =
                serde_json::from_slice(&serde_json::to_vec(&self.garden_economy)?)?;
            restored.validate()?;
            anyhow::ensure!(
                restored == self.garden_economy && self.collect_vacuum_beam(beam)? == 0,
                "harvest lost on reload"
            );
            let coins = self.garden_economy.coins;
            self.garden_economy.sell_crop("carrot", 1)?;
            anyhow::ensure!(
                self.garden_economy.coins == coins + GardenEconomy::crop_price("carrot").unwrap(),
                "carrot sale"
            );
            let other = self
                .surface_builder
                .authored_plants()
                .find(|p| p.species_index == carrot)
                .unwrap();
            self.surface_builder
                .update_flora_growth_for_chunk(other.base_world_vox / 256, 255)?;
            let p = plant_collection_point(other);
            anyhow::ensure!(
                self.harvest_apple_ray(
                    TerrainRayQuery {
                        origin: p + Vec3::Y * 0.08,
                        direction: Vec3::NEG_Y
                    },
                    None
                )?,
                "manual crop harvest"
            );
            log::info!("[CARROT_REVIEW] passed growth=true scheduled_growth=true vacuum=true harvest=true sale=true save_reload=true nectar=false");
        }
        Ok(())
    }
}
