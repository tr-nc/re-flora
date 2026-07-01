use super::App;
use crate::app::world_edits::TerrainBrushEdit;
use anyhow::Result;

const SPRINKLER_MOISTURE_RADIUS: f32 = 0.30;
const SPRINKLER_MOISTURE_PER_SECOND: f32 = 1.35;
const WATERING_BRUSH_MOISTURE_PER_DAB: f32 = 0.68;
const TERRAIN_MOISTURE_DRY_INTERVAL_WORLD_TICKS: u32 = 20;
const TERRAIN_MOISTURE_DRY_PROBABILITY_PER_VOXEL: f32 = 0.10;

impl App {
    pub(super) fn update_terrain_moisture_drying(&mut self, world_tick_steps: u32) {
        self.moisture_dry_tick_accumulator = self
            .moisture_dry_tick_accumulator
            .saturating_add(world_tick_steps);
        while self.moisture_dry_tick_accumulator >= TERRAIN_MOISTURE_DRY_INTERVAL_WORLD_TICKS {
            self.moisture_dry_tick_accumulator -= TERRAIN_MOISTURE_DRY_INTERVAL_WORLD_TICKS;
            if let Err(err) = self
                .plain_builder
                .apply_terrain_moisture_dry_tick(TERRAIN_MOISTURE_DRY_PROBABILITY_PER_VOXEL)
            {
                log::error!("Failed to dry terrain moisture in terrain atlas: {}", err);
                break;
            }
        }
    }

    pub(super) fn update_sprinkler_moisture(&mut self, dt: f32) {
        if dt <= 0.0 || self.sprinkler_records.is_empty() {
            return;
        }

        let amount = SPRINKLER_MOISTURE_PER_SECOND * dt;
        let sprinkler_positions = self
            .sprinkler_records
            .iter()
            .map(|sprinkler| sprinkler.base_position)
            .collect::<Vec<_>>();
        for base_position in sprinkler_positions {
            if let Err(err) = self.plain_builder.apply_terrain_moisture_brush(
                base_position,
                base_position,
                SPRINKLER_MOISTURE_RADIUS,
                amount,
            ) {
                log::error!(
                    "Failed to write sprinkler moisture into terrain atlas: {}",
                    err
                );
            }
        }
    }

    pub(super) fn add_watering_brush_moisture(&mut self, edit: TerrainBrushEdit) -> Result<()> {
        self.plain_builder.apply_terrain_moisture_brush(
            edit.start,
            edit.end,
            edit.radius,
            WATERING_BRUSH_MOISTURE_PER_DAB,
        )?;
        Ok(())
    }
}
