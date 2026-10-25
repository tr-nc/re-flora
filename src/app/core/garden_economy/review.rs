//! Opt-in native fixtures. These exercise live ownership APIs, not synthetic UI clicks.
use super::*;
use crate::{app::core::App, tracer::TerrainRayQuery, tree_gen::TreeSpecies};
use glam::{Vec2, Vec3};

fn final_coins() -> u32 {
    PRICES.starting_coins
        - 2 * product("sapling/apple_tree").unwrap().price
        - PRICES.flower_seed_packet_price
        - PRICES.flower_seedling_price
        - PRICES.grass_packet_price
        + PRICES.apple_sell_price
}
impl App {
    pub(in crate::app::core) fn seed_garden_economy_review(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_ECONOMY_REVIEW") else {
            return Ok(());
        };
        let original = self.debug_settings.adjustables.infinite_materials.value;
        self.debug_settings.adjustables.infinite_materials.value = false;
        let check = (|| -> Result<()> {
            if mode == "load" {
                ensure!(
                    self.garden_economy.coins == final_coins() && self.garden_economy.apples == 1,
                    "wallet or apples changed across reload"
                );
                ensure!(
                    self.garden_economy.count("sapling/apple_tree") == 1
                        && self.garden_economy.count("seeds/sunflower")
                            == PRICES.flower_seed_packet_size - 1
                        && self.garden_economy.count("seeds/grass_mix")
                            == PRICES.grass_packet_size - 1,
                    "purchased plants changed across reload"
                );
                ensure!(
                    self.surface_builder
                        .capture_flora_snapshot(
                            self.time_info.time_since_start_duration().as_millis() as u32
                        )?
                        .counts()
                        == (3, 2),
                    "planted seed, seedling or grass missing after reload"
                );
                let receipts = self.garden_economy.harvest_receipts().collect::<Vec<_>>();
                ensure!(
                    receipts.len() == 2,
                    "harvest identities changed across reload"
                );
                for key in receipts {
                    ensure!(
                        !self.terrain_physics.take_harvest_apple(key),
                        "collected apple respawned on load"
                    );
                }
                log::info!("[GARDEN_ECONOMY_REVIEW] loaded=true coins={} apples=1 harvest_receipts=2 verified=true",self.garden_economy.coins);
                return Ok(());
            }
            for id in self.capture_tree_snapshot().ids().collect::<Vec<_>>() {
                self.remove_tree(id)?;
            }
            self.garden_economy = GardenEconomy::default();
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            let ground = self.query_terrain_height_cpu(Vec2::new(0.82, 1.22));
            let mut desc = TreeSpecies::AppleTree.default_desc();
            desc.branching.seed = 907;
            let pos = Vec3::new(0.82, ground, 1.22);
            ensure!(
                self.plant_owned_tree(desc.clone(), pos).is_err()
                    && self.capture_tree_snapshot().count() == 0,
                "empty stock planted a tree"
            );
            self.garden_economy.buy("sapling/apple_tree")?;
            self.plant_owned_tree(desc.clone(), pos)?;
            ensure!(
                self.garden_economy.count("sapling/apple_tree") == 0,
                "tree placement did not consume one sapling"
            );
            self.advance_tree_growth(900.)?;
            self.garden_economy.buy("sapling/apple_tree")?;
            let before = self.garden_economy.clone();
            ensure!(
                self.plant_owned_tree(desc.clone(), Vec3::splat(f32::NAN))
                    .is_err()
                    && self.garden_economy == before,
                "failed planting spent stock"
            );
            let free_pos = Vec3::new(
                1.45,
                self.query_terrain_height_cpu(Vec2::new(1.45, 1.45)),
                1.45,
            );
            self.debug_settings.adjustables.infinite_materials.value = true;
            desc.branching.seed = 908;
            self.plant_owned_tree(desc, free_pos)?;
            ensure!(
                self.garden_economy == before,
                "debug supply changed real inventory"
            );
            self.debug_settings.adjustables.infinite_materials.value = false;
            self.garden_economy.buy("seeds/sunflower")?;
            self.garden_economy.buy("seedling/sunflower")?;
            self.garden_economy.buy("seeds/grass_mix")?;
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            let flower = species::species()
                .iter()
                .position(|s| s.key == "sunflower")
                .unwrap() as u32;
            let flower = FloraPaintSelection::Species(flower);
            // Retain the complete query contact, like the real pointer path.
            // The CPU DDA sanitizes a vertical direction; a height-only result
            // combined with the original x/z can belong to a different column.
            let mut points = [Vec3::ZERO; 3];
            for (point, x) in points.iter_mut().zip([1.07, 1.11, 1.15]) {
                *point = self
                    .query_terrain_ray_cpu(Vec3::new(x, 3., 1.27), Vec3::NEG_Y)
                    .ok_or_else(|| anyhow::anyhow!("review needs plantable ground"))?
                    .position;
            }
            for point in points {
                self.apply_surface_flora_removal(crate::app::world_edits::TerrainBrushEdit {
                    start: point,
                    end: point,
                    radius: 2. / 256.,
                })?;
            }
            ensure!(
                self.plant_owned_flora(flower, points[0])?,
                "seedling did not plant"
            );
            ensure!(
                self.garden_economy.count("seedling/sunflower") == 0,
                "seedling was not consumed"
            );
            let before = self.garden_economy.clone();
            ensure!(
                !self.plant_owned_flora(flower, points[0])? && self.garden_economy == before,
                "duplicate flower spent a seed"
            );
            ensure!(
                self.plant_owned_flora(flower, points[1])?,
                "seed did not plant"
            );
            ensure!(
                self.plant_owned_flora(FloraPaintSelection::GrassMix, points[2])?,
                "grass seed did not plant"
            );
            let before = self.garden_economy.clone();
            ensure!(
                !self.plant_owned_flora(FloraPaintSelection::GrassMix, points[2])?
                    && self.garden_economy == before,
                "duplicate grass spent a seed"
            );
            self.terrain_physics.set_fruit_cycle(0., &mut self.tracer)?;
            ensure!(
                self.terrain_physics
                    .review_harvest_target(&self.tracer, false)?
                    .is_none(),
                "unripe fruit was harvestable"
            );
            self.debug_settings.adjustables.fruit_cycle.value = 1.;
            self.terrain_physics.set_fruit_cycle(1., &mut self.tracer)?;
            let changed = self.terrain_physics.take_attached_fruit_refresh_trees();
            self.refresh_attached_tree_fruits(&changed)?;
            if mode == "before" {
                return Ok(());
            }
            if mode == "fallen" {
                self.terrain_physics.request_diagnostic_fruit_kicks();
                self.garden_fallen_review_frames = Some(0);
                return Ok(());
            }
            for _ in 0..2 {
                let (_, center) = self
                    .terrain_physics
                    .review_harvest_target(&self.tracer, false)?
                    .ok_or_else(|| anyhow::anyhow!("review needs ripe apples"))?;
                let offset = Vec3::new(0.1, 0.25, 0.3);
                let ray = TerrainRayQuery {
                    origin: center + offset,
                    direction: -offset.normalize(),
                };
                let before = self.garden_economy.clone();
                ensure!(
                    !self.harvest_apple_ray(ray, Some(0.001))? && self.garden_economy == before,
                    "occluded fruit was collected"
                );
                let key = self
                    .terrain_physics
                    .pick_ripe_apple(ray, None, &self.tracer)?
                    .ok_or_else(|| anyhow::anyhow!("ray missed review apple"))?
                    .key;
                ensure!(
                    self.harvest_apple_ray(ray, None)?,
                    "ripe apple was not collected"
                );
                ensure!(
                    self.garden_economy.harvested(key)
                        && !self.terrain_physics.take_harvest_apple(key),
                    "retired apple still exists"
                );
                ensure!(
                    !self.garden_economy.collect(key, || panic!(
                        "duplicate receipt must not take the world fruit"
                    ))?,
                    "duplicate harvest credited"
                );
            }
            self.apply_shop_action(ShopAction::SellApples(1));
            ensure!(
                self.garden_economy.coins == final_coins() && self.garden_economy.apples == 1,
                "sale accounting mismatch"
            );
            let before = self.garden_economy.clone();
            ensure!(
                self.garden_economy.sell_apples(2).is_err()
                    && self.garden_economy.buy("sapling/apple_tree").is_err()
                    && self.garden_economy == before,
                "rejected trade changed storage"
            );
            self.debug_settings.adjustables.tree_age.value = 1.;
            self.update_all_tree_ages_from_gui()?;
            for key in self.garden_economy.harvest_receipts().collect::<Vec<_>>() {
                ensure!(
                    !self.terrain_physics.take_harvest_apple(key),
                    "rebuilding trees respawned harvested apples"
                );
            }
            log::info!("[GARDEN_ECONOMY_REVIEW] coins={} apples={} harvest_receipts=2 saplings_owned=1 flower_seeds={} grass_seeds={} failed_and_duplicate_planting_preserved=true debug_supply_preserved=true verified=true",self.garden_economy.coins,self.garden_economy.apples,self.garden_economy.count("seeds/sunflower"),self.garden_economy.count("seeds/grass_mix"));
            Ok(())
        })();
        self.debug_settings.adjustables.infinite_materials.value =
            if mode == "shop" { false } else { original };
        check?;
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        let ground = self.query_terrain_height_cpu(Vec2::new(0.65, 1.22));
        let target = Vec3::new(0.86, ground + 0.075, 1.22);
        self.camera_control.apply_snapshot_mode(true);
        self.camera_control.set_orbit_focus(target);
        self.tracer
            .set_camera_pose_looking_at(target + Vec3::new(0.10, 0.12, 0.34), target);
        self.reset_camera_movement_input();
        self.shop_open = mode == "shop";
        Ok(())
    }
    pub(in crate::app::core) fn review_fallen_apple_collection(&mut self) -> Result<()> {
        let Some(frame) = self.garden_fallen_review_frames.as_mut() else {
            return Ok(());
        };
        *frame += 1;
        if *frame < 60 {
            return Ok(());
        }
        let Some((_, center)) = self
            .terrain_physics
            .review_harvest_target(&self.tracer, true)?
        else {
            return Ok(());
        };
        let ray = TerrainRayQuery {
            origin: center + Vec3::Z * 0.025,
            direction: Vec3::NEG_Z,
        };
        let Some(pick) = self
            .terrain_physics
            .pick_ripe_apple(ray, None, &self.tracer)?
        else {
            return Ok(());
        };
        if !pick.loose {
            return Ok(());
        }
        let key = pick.key;
        ensure!(
            self.harvest_apple_ray(ray, None)?,
            "fallen apple ray missed"
        );
        ensure!(
            !self.terrain_physics.take_harvest_apple(key) && self.garden_economy.apples == 1,
            "fallen apple retirement mismatch"
        );
        self.garden_fallen_review_frames = None;
        log::info!("[GARDEN_ECONOMY_REVIEW] fallen_collected=true stored=1 dynamic_owner_retired=true verified=true");
        Ok(())
    }
}
