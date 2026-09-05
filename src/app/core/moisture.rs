use super::{vegetation::SurfaceOccupantClearPath, App, CHUNK_DIM, VOXEL_DIM_PER_CHUNK};
use crate::app::world_edits::TerrainBrushEdit;
use crate::builder::{ContreeBuilder, PlainBuilder};
use crate::util::BENCH;
use anyhow::Result;
use glam::{UVec3, Vec3};
use re_flora_vkn::CommandBuffer;
use std::time::Instant;

const SPRINKLER_MOISTURE_RADIUS: f32 = 0.30;
const SPRINKLER_MOISTURE_PER_SECOND: f32 = 1.35;
const WATERING_BRUSH_MOISTURE_PER_DAB: f32 = 0.68;
const TILLER_BRUSH_SOIL_MIX_STRENGTH: f32 = 0.82;
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

#[derive(Default)]
pub(super) struct TerrainMoistureRuntime {
    dry_chunk_cursor: u32,
    spread_task_cursor: u32,
}

impl TerrainMoistureRuntime {
    pub(super) fn has_chunks(&self) -> bool {
        Self::chunk_count() > 0
    }

    pub(super) fn record_spread(
        &mut self,
        plain_builder: &mut PlainBuilder,
        cmdbuf: &CommandBuffer,
        perf_logging: bool,
    ) -> usize {
        let mut recorded_count = 0;
        for _ in 0..TERRAIN_MOISTURE_SPREAD_CHUNKS_PER_FRAME {
            let Some((chunk_id, axis, pair_parity)) = self.next_spread_task() else {
                return recorded_count;
            };
            let atlas_offset = chunk_id * VOXEL_DIM_PER_CHUNK;
            let spread_record_start = perf_logging.then(Instant::now);
            if !plain_builder.record_terrain_moisture_spread_region(
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
                    self.spread_task_cursor,
                    spread_record_elapsed.as_secs_f64() * 1000.0,
                );
            }
        }
        recorded_count
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_dry(
        &mut self,
        plain_builder: &mut PlainBuilder,
        contree_builder: &ContreeBuilder,
        cmdbuf: &CommandBuffer,
        sun_dir: Vec3,
        direct_shadow_source_mask: u32,
        direct_shadow_available_mask: u32,
        perf_logging: bool,
    ) -> usize {
        let mut recorded_count = 0;
        for _ in 0..TERRAIN_MOISTURE_DRY_CHUNKS_PER_FRAME {
            let Some(chunk_id) = self.next_dry_chunk() else {
                return recorded_count;
            };
            let atlas_offset = chunk_id * VOXEL_DIM_PER_CHUNK;
            let Some(surface_leaf_info) = contree_builder.surface_leaf_dry_info(chunk_id) else {
                continue;
            };
            let dry_record_start = perf_logging.then(Instant::now);
            if !plain_builder.record_terrain_moisture_dry_region(
                cmdbuf,
                atlas_offset,
                VOXEL_DIM_PER_CHUNK,
                TERRAIN_MOISTURE_DRY_PROBABILITY_PER_CHUNK_VISIT,
                sun_dir,
                TERRAIN_MOISTURE_SUNLIT_DRY_PROBABILITY_MULTIPLIER,
                TERRAIN_MOISTURE_RESIDUAL_DRY_PROBABILITY_MULTIPLIER,
                VOXEL_DIM_PER_CHUNK.x as f32,
                direct_shadow_source_mask,
                direct_shadow_available_mask,
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
                    "[PERF][MOISTURE_DRY] chunk={:?} atlas_offset={:?} atlas_dim={:?} surface_leaf_count={} surface_leaf_chunk_info_index={} probability={:.3} sunlit_multiplier={:.2} residual_multiplier={:.2} shadow_source_mask={:#x} shadow_available_mask={:#x} sun_dir={:?} next_cursor={} record_ms={:.3}",
                    chunk_id,
                    atlas_offset,
                    VOXEL_DIM_PER_CHUNK,
                    surface_leaf_info.leaf_count,
                    surface_leaf_info.chunk_info_index,
                    TERRAIN_MOISTURE_DRY_PROBABILITY_PER_CHUNK_VISIT,
                    TERRAIN_MOISTURE_SUNLIT_DRY_PROBABILITY_MULTIPLIER,
                    TERRAIN_MOISTURE_RESIDUAL_DRY_PROBABILITY_MULTIPLIER,
                    direct_shadow_source_mask,
                    direct_shadow_available_mask,
                    sun_dir,
                    self.dry_chunk_cursor,
                    dry_record_elapsed.as_secs_f64() * 1000.0,
                );
            }
        }
        recorded_count
    }

    fn next_dry_chunk(&mut self) -> Option<UVec3> {
        let chunk_count = Self::chunk_count();
        if chunk_count == 0 {
            return None;
        }

        let chunk_index = self.dry_chunk_cursor % chunk_count;
        self.dry_chunk_cursor = (chunk_index + 1) % chunk_count;
        Some(Self::chunk_from_index(chunk_index))
    }

    fn next_spread_task(&mut self) -> Option<(UVec3, u32, u32)> {
        let chunk_count = Self::chunk_count();
        let task_count = chunk_count.saturating_mul(TERRAIN_MOISTURE_SPREAD_PAIR_PHASE_COUNT);
        if task_count == 0 {
            return None;
        }

        let task_index = self.spread_task_cursor % task_count;
        self.spread_task_cursor = (task_index + 1) % task_count;
        let chunk_index = task_index % chunk_count;
        let pair_phase = task_index / chunk_count;
        let axis = TERRAIN_MOISTURE_SPREAD_VERTICAL_AXIS;
        let pair_parity = pair_phase % TERRAIN_MOISTURE_SPREAD_PAIR_PHASE_COUNT;
        Some((Self::chunk_from_index(chunk_index), axis, pair_parity))
    }

    fn chunk_count() -> u32 {
        CHUNK_DIM
            .x
            .saturating_mul(CHUNK_DIM.y)
            .saturating_mul(CHUNK_DIM.z)
    }

    fn chunk_from_index(mut chunk_index: u32) -> UVec3 {
        let z = chunk_index % CHUNK_DIM.z;
        chunk_index /= CHUNK_DIM.z;
        let y = chunk_index % CHUNK_DIM.y;
        let x = chunk_index / CHUNK_DIM.y;
        UVec3::new(x, y, z)
    }
}

impl App {
    pub(super) fn record_sprinkler_moisture(&mut self, cmdbuf: &CommandBuffer, dt: f32) -> usize {
        if dt <= 0.0 || self.sprinklers.is_empty() {
            return 0;
        }

        let record_start = self.perf_logging.then(Instant::now);
        let amount = SPRINKLER_MOISTURE_PER_SECOND * dt;
        let sprinkler_sources = self.sprinklers.moisture_sources();
        let tick_seconds = self.debug_settings.adjustables.world_tick_seconds.value;
        let mut recorded_count = 0;
        for source in sprinkler_sources {
            let spray_axis = source.spray_axis(self.world_clock.flora_tick(), tick_seconds);
            if self
                .plain_builder
                .record_directional_pair_moisture_brush(
                    cmdbuf,
                    source.base_position,
                    spray_axis,
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
                self.sprinklers.len(),
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

    pub(super) fn mix_tiller_brush_soil(&mut self, edit: TerrainBrushEdit) -> Result<()> {
        self.plain_builder.apply_terrain_soil_mix(
            edit.start,
            edit.end,
            edit.radius,
            TILLER_BRUSH_SOIL_MIX_STRENGTH,
        )?;
        self.clear_surface_occupants_in_brush(edit, SurfaceOccupantClearPath::Standalone)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TerrainMoistureRuntime, TERRAIN_MOISTURE_SPREAD_PAIR_PHASE_COUNT,
        TERRAIN_MOISTURE_SPREAD_VERTICAL_AXIS,
    };
    use std::collections::HashSet;

    #[test]
    fn dry_schedule_visits_each_chunk_once_before_wrapping() {
        let mut runtime = TerrainMoistureRuntime::default();
        let chunk_count = TerrainMoistureRuntime::chunk_count();

        let first_cycle = (0..chunk_count)
            .map(|_| runtime.next_dry_chunk().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            first_cycle.iter().copied().collect::<HashSet<_>>().len(),
            chunk_count as usize
        );
        assert_eq!(runtime.next_dry_chunk(), Some(first_cycle[0]));
    }

    #[test]
    fn spread_schedule_visits_both_pair_phases_before_wrapping() {
        let mut runtime = TerrainMoistureRuntime::default();
        let chunk_count = TerrainMoistureRuntime::chunk_count();
        let task_count = chunk_count * TERRAIN_MOISTURE_SPREAD_PAIR_PHASE_COUNT;

        let first_cycle = (0..task_count)
            .map(|_| runtime.next_spread_task().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            first_cycle.iter().copied().collect::<HashSet<_>>().len(),
            task_count as usize
        );
        assert!(first_cycle[..chunk_count as usize]
            .iter()
            .all(
                |(_, axis, parity)| *axis == TERRAIN_MOISTURE_SPREAD_VERTICAL_AXIS && *parity == 0
            ));
        assert!(first_cycle[chunk_count as usize..]
            .iter()
            .all(
                |(_, axis, parity)| *axis == TERRAIN_MOISTURE_SPREAD_VERTICAL_AXIS && *parity == 1
            ));
        assert_eq!(runtime.next_spread_task(), Some(first_cycle[0]));
    }
}
