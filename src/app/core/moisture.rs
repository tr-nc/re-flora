use super::{App, CHUNK_DIM, VOXEL_DIM_PER_CHUNK};
use crate::app::world_edits::TerrainBrushEdit;
use crate::util::{ChunkPopMode, BENCH};
use anyhow::Result;
use std::time::Instant;
use verdarium_vkn::CommandBuffer;

const SPRINKLER_MOISTURE_RADIUS: f32 = 0.30;
const SPRINKLER_MOISTURE_PER_SECOND: f32 = 1.35;
const WATERING_BRUSH_MOISTURE_PER_DAB: f32 = 0.68;
const TERRAIN_MOISTURE_DRY_ENQUEUE_INTERVAL_WORLD_TICKS: u32 = 20;
const TERRAIN_MOISTURE_DRY_PROBABILITY_PER_CHUNK_VISIT: f32 = 0.02;
const TERRAIN_MOISTURE_DRY_CHUNKS_PER_FRAME: usize = 1;

impl App {
    pub(super) fn update_terrain_moisture_drying(&mut self, world_tick_steps: u32) {
        self.enqueue_terrain_moisture_dry_chunks(world_tick_steps);
    }

    fn enqueue_terrain_moisture_dry_chunks(&mut self, world_tick_steps: u32) {
        self.moisture_dry_tick_accumulator = self
            .moisture_dry_tick_accumulator
            .saturating_add(world_tick_steps);
        while self.moisture_dry_tick_accumulator
            >= TERRAIN_MOISTURE_DRY_ENQUEUE_INTERVAL_WORLD_TICKS
        {
            self.moisture_dry_tick_accumulator -= TERRAIN_MOISTURE_DRY_ENQUEUE_INTERVAL_WORLD_TICKS;
            for x in 0..CHUNK_DIM.x {
                for y in 0..CHUNK_DIM.y {
                    for z in 0..CHUNK_DIM.z {
                        self.moisture_dry_chunks.push(glam::uvec3(x, y, z));
                    }
                }
            }
        }
    }

    pub(super) fn has_pending_terrain_moisture_dry_chunks(&self) -> bool {
        !self.moisture_dry_chunks.is_empty()
    }

    pub(super) fn record_terrain_moisture_dry_chunks(&mut self, cmdbuf: &CommandBuffer) -> usize {
        let mut recorded_count = 0;
        for _ in 0..TERRAIN_MOISTURE_DRY_CHUNKS_PER_FRAME {
            let Some(chunk_id) = self.moisture_dry_chunks.pop(ChunkPopMode::Fifo) else {
                return recorded_count;
            };
            let atlas_offset = chunk_id * VOXEL_DIM_PER_CHUNK;
            let dry_record_start = self.perf_logging.then(Instant::now);
            if !self.plain_builder.record_terrain_moisture_dry_region(
                cmdbuf,
                atlas_offset,
                VOXEL_DIM_PER_CHUNK,
                TERRAIN_MOISTURE_DRY_PROBABILITY_PER_CHUNK_VISIT,
            ) {
                continue;
            }
            recorded_count += 1;
            if let Some(dry_record_start) = dry_record_start {
                let dry_record_elapsed = dry_record_start.elapsed();
                BENCH
                    .lock()
                    .unwrap()
                    .record("terrain_moisture_dry_record", dry_record_elapsed);
                log::info!(
                    "[PERF][MOISTURE_DRY] chunk={:?} atlas_offset={:?} atlas_dim={:?} probability={:.3} pending_after={} record_ms={:.3}",
                    chunk_id,
                    atlas_offset,
                    VOXEL_DIM_PER_CHUNK,
                    TERRAIN_MOISTURE_DRY_PROBABILITY_PER_CHUNK_VISIT,
                    self.moisture_dry_chunks.len(),
                    dry_record_elapsed.as_secs_f64() * 1000.0,
                );
            }
        }
        recorded_count
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
