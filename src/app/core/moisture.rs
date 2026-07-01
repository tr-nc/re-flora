use super::{App, CHUNK_DIM, VOXEL_DIM_PER_CHUNK};
use crate::app::world_edits::TerrainBrushEdit;
use crate::util::BENCH;
use anyhow::Result;
use glam::UVec3;
use std::time::Instant;
use verdarium_vkn::CommandBuffer;

const SPRINKLER_MOISTURE_RADIUS: f32 = 0.30;
const SPRINKLER_MOISTURE_PER_SECOND: f32 = 1.35;
const WATERING_BRUSH_MOISTURE_PER_DAB: f32 = 0.68;
const TERRAIN_MOISTURE_DRY_PROBABILITY_PER_CHUNK_VISIT: f32 = 0.01;
const TERRAIN_MOISTURE_DRY_CHUNKS_PER_FRAME: usize = 1;

impl App {
    pub(super) fn has_terrain_moisture_dry_chunks(&self) -> bool {
        Self::terrain_moisture_dry_chunk_count() > 0
    }

    pub(super) fn record_terrain_moisture_dry_chunks(&mut self, cmdbuf: &CommandBuffer) -> usize {
        let mut recorded_count = 0;
        for _ in 0..TERRAIN_MOISTURE_DRY_CHUNKS_PER_FRAME {
            let Some(chunk_id) = self.next_terrain_moisture_dry_chunk() else {
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
                    "[PERF][MOISTURE_DRY] chunk={:?} atlas_offset={:?} atlas_dim={:?} probability={:.3} next_cursor={} record_ms={:.3}",
                    chunk_id,
                    atlas_offset,
                    VOXEL_DIM_PER_CHUNK,
                    TERRAIN_MOISTURE_DRY_PROBABILITY_PER_CHUNK_VISIT,
                    self.moisture_dry_chunk_cursor,
                    dry_record_elapsed.as_secs_f64() * 1000.0,
                );
            }
        }
        recorded_count
    }

    fn next_terrain_moisture_dry_chunk(&mut self) -> Option<UVec3> {
        let chunk_count = Self::terrain_moisture_dry_chunk_count();
        if chunk_count == 0 {
            return None;
        }

        let chunk_index = self.moisture_dry_chunk_cursor % chunk_count;
        self.moisture_dry_chunk_cursor = (chunk_index + 1) % chunk_count;
        Some(Self::terrain_moisture_dry_chunk_from_index(chunk_index))
    }

    fn terrain_moisture_dry_chunk_count() -> u32 {
        CHUNK_DIM
            .x
            .saturating_mul(CHUNK_DIM.y)
            .saturating_mul(CHUNK_DIM.z)
    }

    fn terrain_moisture_dry_chunk_from_index(mut chunk_index: u32) -> UVec3 {
        let z = chunk_index % CHUNK_DIM.z;
        chunk_index /= CHUNK_DIM.z;
        let y = chunk_index % CHUNK_DIM.y;
        let x = chunk_index / CHUNK_DIM.y;
        UVec3::new(x, y, z)
    }

    pub(super) fn record_sprinkler_moisture(&mut self, cmdbuf: &CommandBuffer, dt: f32) -> usize {
        if dt <= 0.0 || self.sprinkler_records.is_empty() {
            return 0;
        }

        let record_start = self.perf_logging.then(Instant::now);
        let amount = SPRINKLER_MOISTURE_PER_SECOND * dt;
        let sprinkler_positions = self
            .sprinkler_records
            .iter()
            .map(|sprinkler| sprinkler.base_position)
            .collect::<Vec<_>>();
        let mut recorded_count = 0;
        for base_position in sprinkler_positions {
            if self
                .plain_builder
                .record_terrain_moisture_brush(
                    cmdbuf,
                    base_position,
                    base_position,
                    SPRINKLER_MOISTURE_RADIUS,
                    amount,
                )
                .is_some()
            {
                recorded_count += 1;
            }
        }

        if let Some(record_start) = record_start {
            let record_elapsed = record_start.elapsed();
            BENCH
                .lock()
                .unwrap()
                .record("sprinkler_moisture_record", record_elapsed);
            log::info!(
                "[PERF][SPRINKLER_MOISTURE] sprinklers={} recorded={} amount={:.4} record_ms={:.3}",
                self.sprinkler_records.len(),
                recorded_count,
                amount,
                record_elapsed.as_secs_f64() * 1000.0,
            );
        }

        recorded_count
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
