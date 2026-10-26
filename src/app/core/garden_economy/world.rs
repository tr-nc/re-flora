use super::*;
use crate::tracer::TerrainRayQuery;
use crate::{
    app::core::{planting::AuthoredFloraPlacementBatch, App},
    tree_gen::TreeDesc,
};
use glam::Vec3;
use std::time::Instant;

impl App {
    pub(in crate::app::core) fn harvest_pointed_apple(&mut self) -> Result<()> {
        let Some(pick) = self.pointer_picking.active else {
            return Ok(());
        };
        self.harvest_apple_ray(pick.ray, pick.hit.map(|h| h.distance))?;
        Ok(())
    }
    pub(in crate::app::core) fn harvest_apple_ray(
        &mut self,
        ray: TerrainRayQuery,
        occluder: Option<f32>,
    ) -> Result<bool> {
        let started = Instant::now();
        let Some(pick) = self
            .terrain_physics
            .pick_ripe_apple(ray, occluder, &self.tracer)?
        else {
            let collected = self.harvest_ground_plant(ray)?;
            if !collected {
                self.garden_feedback =
                    Some("Aim at a ripe apple, mature carrot or special plant within reach".into());
            }
            return Ok(collected);
        };
        let key = pick.key;
        let physics = &mut self.terrain_physics;
        if !self
            .garden_economy
            .collect(key, || physics.take_harvest_apple(key))?
        {
            return Ok(false);
        }
        // Ownership already transferred. A failed presentation refresh cannot refund
        // the world fruit or allow a repeated click to credit the same identity.
        self.terrain_physics
            .publish_harvest_rendering(&mut self.tracer)?;
        let trees = self.terrain_physics.take_attached_fruit_refresh_trees();
        self.refresh_attached_tree_fruits(&trees)?;
        self.garden_feedback = Some(format!(
            "{} an apple · {} stored",
            if pick.loose { "Picked up" } else { "Harvested" },
            self.garden_economy.apples
        ));
        log::info!(
            "[GARDEN_ECONOMY] harvested_tree={} fruit={} stored={} elapsed_ms={:.3}",
            key.tree,
            key.fruit,
            self.garden_economy.apples,
            started.elapsed().as_secs_f64() * 1000.
        );
        Ok(true)
    }
    pub(in crate::app::core) fn apply_shop_action(&mut self, action: ShopAction) {
        let result = match action {
            ShopAction::Buy(id) => self
                .garden_economy
                .buy(&id)
                .map(|_| format!("Bought {}", product(&id).unwrap().label)),
            ShopAction::SellCrop(id, n) => self
                .garden_economy
                .sell_crop(&id, n)
                .map(|coins| format!("Sold {n} {id} for {coins} coins")),
            ShopAction::SellApples(n) => self.garden_economy.sell_apples(n).map(|coins| {
                format!(
                    "Sold {n} apple{} for {coins} coins",
                    if n == 1 { "" } else { "s" }
                )
            }),
        };
        self.garden_feedback = Some(match result {
            Ok(text) => text,
            Err(e) => e.to_string(),
        });
    }
    pub(in crate::app::core) fn plant_owned_tree(
        &mut self,
        desc: TreeDesc,
        position: Vec3,
    ) -> Result<u32> {
        let supply = if self.infinite_materials_enabled() {
            None
        } else {
            Some(
                self.garden_economy
                    .planting_supply(PlantVariety::Tree(desc.species))
                    .ok_or_else(|| anyhow::anyhow!("Buy this sapling in Shop (B)"))?,
            )
        };
        let id = self.plant_tree_with_maturity(
            desc,
            position,
            self.debug_settings
                .adjustables
                .tree_planting_instant_mature
                .value,
        )?;
        if let Some(p) = supply {
            self.garden_economy.consume_committed_plant(&p.id);
        }
        Ok(id)
    }
    pub(in crate::app::core) fn plant_owned_flora(
        &mut self,
        selection: FloraPaintSelection,
        position: Vec3,
    ) -> Result<bool> {
        let supply = self
            .garden_economy
            .planting_supply(PlantVariety::Flora(selection))
            .ok_or_else(|| anyhow::anyhow!("Buy seeds or seedlings in Shop (B)"))?;
        let Some(anchor) = self.resolve_plantable_contact(position)? else {
            return Ok(false);
        };
        let seed = rand::random::<u32>();
        let species_index = match selection {
            FloraPaintSelection::GrassMix => seed % 2,
            FloraPaintSelection::Species(i) => i,
        };
        let growth = if matches!(supply.goods, PlantGoods::Seedling(_)) {
            96
        } else {
            0
        };
        let now = self.time_info.time_since_start_duration().as_millis() as u32;
        let base = anchor.base_world_vox();
        let chunk = base / crate::app::core::VOXEL_DIM_PER_CHUNK;
        if species_index < species::MODEL_FLOWER_FIRST_SPECIES {
            if !self
                .surface_builder
                .try_insert_finite_grass(species_index, base, growth, now)?
            {
                return Ok(false);
            }
            self.garden_economy.consume_committed_plant(&supply.id);
        } else {
            let mut batch = AuthoredFloraPlacementBatch::new();
            if !self.try_place_authored_flora(&mut batch, species_index, anchor, growth, now, seed)
            {
                return Ok(false);
            }
            // The canonical plant is owned now, even if later GPU publication fails.
            self.garden_economy.consume_committed_plant(&supply.id);
            self.finish_authored_flora_placement(batch)?;
        }
        self.track_growing_flora_chunk(chunk);
        self.tracer.invalidate_vegetation_response_history();
        log::info!(
            "[GARDEN_ECONOMY] planted={} root={:?} growth={} remaining={}",
            supply.id,
            base,
            growth,
            self.garden_economy.count(&supply.id)
        );
        Ok(true)
    }
}
