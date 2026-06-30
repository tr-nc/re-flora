use super::App;
use crate::app::world_edits::TerrainBrushEdit;
use anyhow::Result;

const SPRINKLER_MOISTURE_RADIUS: f32 = 0.30;
const SPRINKLER_MOISTURE_PER_SECOND: f32 = 1.35;
const WATERING_BRUSH_MOISTURE_PER_DAB: f32 = 0.68;

impl App {
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
