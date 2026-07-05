use super::{App, CHUNK_DIM, VOXEL_DIM_PER_CHUNK};
use crate::app::world_edits::TerrainBrushEdit;
use crate::util::BENCH;
use anyhow::Result;
use glam::{UVec3, Vec3};
use std::time::Instant;
use verdarium_vkn::CommandBuffer;

const SPRINKLER_MOISTURE_RADIUS: f32 = 0.30;
const SPRINKLER_MOISTURE_PER_SECOND: f32 = 1.35;
const WATERING_BRUSH_MOISTURE_PER_DAB: f32 = 0.68;
const FERTILIZER_BRUSH_FERTILITY_PER_DAB: f32 = 0.68;
const TILLER_BRUSH_FERTILITY_MIX_STRENGTH: f32 = 0.82;
const TERRAIN_MOISTURE_DRY_PROBABILITY_PER_CHUNK_VISIT: f32 = 0.002;
const TERRAIN_MOISTURE_SUNLIT_DRY_PROBABILITY_MULTIPLIER: f32 = 12.0;
const TERRAIN_MOISTURE_RESIDUAL_DRY_PROBABILITY_MULTIPLIER: f32 = 64.0;
const TERRAIN_MOISTURE_DRY_CHUNKS_PER_FRAME: usize = 1;
const TERRAIN_MOISTURE_SPREAD_PROBABILITY_PER_PAIR_VISIT: f32 = 0.45;
const TERRAIN_MOISTURE_SPREAD_MOBILITY_EXPONENT: f32 = 2.0;
const TERRAIN_MOISTURE_SPREAD_DOWNWARD_MULTIPLIER: f32 = 2.5;
const TERRAIN_MOISTURE_SPREAD_UPWARD_MULTIPLIER: f32 = 0.45;
const TERRAIN_MOISTURE_SPREAD_VERTICAL_AXIS: u32 = 1;
const TERRAIN_MOISTURE_SPREAD_PAIR_PHASE_COUNT: u32 = 2;
const TERRAIN_MOISTURE_SPREAD_CHUNKS_PER_FRAME: usize = 1;

impl App {
    pub(super) fn has_terrain_moisture_dry_chunks(&self) -> bool {
        Self::terrain_moisture_chunk_count() > 0
    }

    pub(super) fn has_terrain_moisture_spread_chunks(&self) -> bool {
        Self::terrain_moisture_chunk_count() > 0
    }

    pub(super) fn record_terrain_moisture_spread_chunks(
        &mut self,
        cmdbuf: &CommandBuffer,
    ) -> usize {
        let mut recorded_count = 0;
        for _ in 0..TERRAIN_MOISTURE_SPREAD_CHUNKS_PER_FRAME {
            let Some((chunk_id, axis, pair_parity)) = self.next_terrain_moisture_spread_task()
            else {
                return recorded_count;
            };
            let atlas_offset = chunk_id * VOXEL_DIM_PER_CHUNK;
            let spread_record_start = self.perf_logging.then(Instant::now);
            if !self.plain_builder.record_terrain_moisture_spread_region(
                cmdbuf,
                atlas_offset,
                VOXEL_DIM_PER_CHUNK,
                TERRAIN_MOISTURE_SPREAD_PROBABILITY_PER_PAIR_VISIT,
                TERRAIN_MOISTURE_SPREAD_MOBILITY_EXPONENT,
                TERRAIN_MOISTURE_SPREAD_DOWNWARD_MULTIPLIER,
                TERRAIN_MOISTURE_SPREAD_UPWARD_MULTIPLIER,
                axis,
                pair_parity,
            ) {
                continue;
            }
            recorded_count += 1;
            if let Some(spread_record_start) = spread_record_start {
                let spread_record_elapsed = spread_record_start.elapsed();
                BENCH
                    .lock()
                    .unwrap()
                    .record("terrain_moisture_spread_record", spread_record_elapsed);
                log::info!(
                    "[PERF][MOISTURE_SPREAD] chunk={:?} atlas_offset={:?} atlas_dim={:?} probability={:.3} mobility_exponent={:.2} downward_multiplier={:.2} upward_multiplier={:.2} axis={} pair_parity={} next_cursor={} record_ms={:.3}",
                    chunk_id,
                    atlas_offset,
                    VOXEL_DIM_PER_CHUNK,
                    TERRAIN_MOISTURE_SPREAD_PROBABILITY_PER_PAIR_VISIT,
                    TERRAIN_MOISTURE_SPREAD_MOBILITY_EXPONENT,
                    TERRAIN_MOISTURE_SPREAD_DOWNWARD_MULTIPLIER,
                    TERRAIN_MOISTURE_SPREAD_UPWARD_MULTIPLIER,
                    axis,
                    pair_parity,
                    self.moisture_spread_chunk_cursor,
                    spread_record_elapsed.as_secs_f64() * 1000.0,
                );
            }
        }
        recorded_count
    }

    pub(super) fn record_terrain_moisture_dry_chunks(
        &mut self,
        cmdbuf: &CommandBuffer,
        sun_dir: Vec3,
        terrain_shadow_vsm_ready: bool,
    ) -> usize {
        let mut recorded_count = 0;
        for _ in 0..TERRAIN_MOISTURE_DRY_CHUNKS_PER_FRAME {
            let Some(chunk_id) = self.next_terrain_moisture_dry_chunk() else {
                return recorded_count;
            };
            let atlas_offset = chunk_id * VOXEL_DIM_PER_CHUNK;
            let Some(surface_leaf_info) = self.contree_builder.surface_leaf_dry_info(chunk_id)
            else {
                continue;
            };
            let dry_record_start = self.perf_logging.then(Instant::now);
            if !self.plain_builder.record_terrain_moisture_dry_region(
                cmdbuf,
                atlas_offset,
                VOXEL_DIM_PER_CHUNK,
                TERRAIN_MOISTURE_DRY_PROBABILITY_PER_CHUNK_VISIT,
                sun_dir,
                TERRAIN_MOISTURE_SUNLIT_DRY_PROBABILITY_MULTIPLIER,
                TERRAIN_MOISTURE_RESIDUAL_DRY_PROBABILITY_MULTIPLIER,
                VOXEL_DIM_PER_CHUNK.x as f32,
                terrain_shadow_vsm_ready,
                surface_leaf_info.leaf_count,
                surface_leaf_info.chunk_info_index,
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
                    "[PERF][MOISTURE_DRY] chunk={:?} atlas_offset={:?} atlas_dim={:?} surface_leaf_count={} surface_leaf_chunk_info_index={} probability={:.3} sunlit_multiplier={:.2} residual_multiplier={:.2} vsm_ready={} sun_dir={:?} next_cursor={} record_ms={:.3}",
                    chunk_id,
                    atlas_offset,
                    VOXEL_DIM_PER_CHUNK,
                    surface_leaf_info.leaf_count,
                    surface_leaf_info.chunk_info_index,
                    TERRAIN_MOISTURE_DRY_PROBABILITY_PER_CHUNK_VISIT,
                    TERRAIN_MOISTURE_SUNLIT_DRY_PROBABILITY_MULTIPLIER,
                    TERRAIN_MOISTURE_RESIDUAL_DRY_PROBABILITY_MULTIPLIER,
                    terrain_shadow_vsm_ready,
                    sun_dir,
                    self.moisture_dry_chunk_cursor,
                    dry_record_elapsed.as_secs_f64() * 1000.0,
                );
            }
        }
        recorded_count
    }

    fn next_terrain_moisture_dry_chunk(&mut self) -> Option<UVec3> {
        let chunk_count = Self::terrain_moisture_chunk_count();
        if chunk_count == 0 {
            return None;
        }

        let chunk_index = self.moisture_dry_chunk_cursor % chunk_count;
        self.moisture_dry_chunk_cursor = (chunk_index + 1) % chunk_count;
        Some(Self::terrain_moisture_chunk_from_index(chunk_index))
    }

    fn next_terrain_moisture_spread_task(&mut self) -> Option<(UVec3, u32, u32)> {
        let chunk_count = Self::terrain_moisture_chunk_count();
        let task_count = chunk_count.saturating_mul(TERRAIN_MOISTURE_SPREAD_PAIR_PHASE_COUNT);
        if task_count == 0 {
            return None;
        }

        let task_index = self.moisture_spread_chunk_cursor % task_count;
        self.moisture_spread_chunk_cursor = (task_index + 1) % task_count;
        let chunk_index = task_index % chunk_count;
        let pair_phase = task_index / chunk_count;
        let axis = TERRAIN_MOISTURE_SPREAD_VERTICAL_AXIS;
        let pair_parity = pair_phase % TERRAIN_MOISTURE_SPREAD_PAIR_PHASE_COUNT;
        Some((
            Self::terrain_moisture_chunk_from_index(chunk_index),
            axis,
            pair_parity,
        ))
    }

    fn terrain_moisture_chunk_count() -> u32 {
        CHUNK_DIM
            .x
            .saturating_mul(CHUNK_DIM.y)
            .saturating_mul(CHUNK_DIM.z)
    }

    fn terrain_moisture_chunk_from_index(mut chunk_index: u32) -> UVec3 {
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

    pub(super) fn add_fertilizer_brush_fertility(
        &mut self,
        edit: TerrainBrushEdit,
        stroke_seed: u32,
    ) -> Result<()> {
        self.plain_builder.apply_terrain_fertility_brush(
            edit.start,
            edit.end,
            edit.radius,
            FERTILIZER_BRUSH_FERTILITY_PER_DAB,
            stroke_seed,
        )?;
        Ok(())
    }

    pub(super) fn mix_tiller_brush_fertility(&mut self, edit: TerrainBrushEdit) -> Result<()> {
        self.plain_builder.apply_terrain_fertility_mix(
            edit.start,
            edit.end,
            edit.radius,
            TILLER_BRUSH_FERTILITY_MIX_STRENGTH,
        )?;
        Ok(())
    }
}
