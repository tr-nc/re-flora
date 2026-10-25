//! Soil-bag application policy. Geometry editing, soil amendment and falling FX are separate.
mod review;

use super::player_tools::{ContinuousTerrainToolAction, PlayerTool};
use super::voxel_backpack::BackpackVoxel;
use super::*;
use crate::app::world_edits::TerrainBrushEdit;
use crate::terrain_deposition::{DepositionRequest, DepositionResult};

impl App {
    pub(super) fn soil_deposition_radius(&self) -> f32 {
        self.debug_settings
            .adjustables
            .soil_deposition_spread_radius
            .value
            / 256.
    }

    pub(super) fn terrain_tool_preview_radius(&self) -> f32 {
        if self.player_tools.selected_tool() == PlayerTool::Soil {
            // A zero-spread grain still needs a visible one-voxel target, not a zero-sized ring.
            self.soil_deposition_radius().max(0.5 / 256.)
        } else {
            self.player_tools.terrain_edit_radius
        }
    }

    pub(super) fn try_soil_deposition(&mut self, now: Instant) {
        let action = ContinuousTerrainToolAction::SoilDeposit;
        if !self.terrain_edit_pointer_available()
            || self.player_tools.selected_tool() != PlayerTool::Soil
        {
            self.soil_bag_preview
                .observe_target(input::TerrainEditHoverObservation::Unavailable, now);
            self.player_tools.interrupt_stroke(action);
            self.stop_terrain_edit_loop_sound();
            return;
        }
        if !self.player_tools.stroke_ready(action, now, Duration::ZERO) {
            return;
        }
        // A missing/rejected surface must break the path and budget; never catch up across air.
        let observation = self.terrain_edit_hover_observation();
        // This is the captured, current resolved edit ray. Renew only the bag's
        // presentation anchor before publication invalidates hover; never use
        // that anchor in the deposition path or budget below.
        self.soil_bag_preview.observe_target(observation, now);
        let center = match observation.hit() {
            Some(hit) if hit.is_editable => hit.center,
            _ => {
                self.soil_bag_preview.allow_flow = false;
                self.player_tools.defer_stroke(action, now);
                self.stop_terrain_edit_loop_sound();
                return;
            }
        };
        let rate = self.debug_settings.adjustables.soil_deposition_rate.value;
        let batch_size = self
            .debug_settings
            .adjustables
            .soil_deposition_batch_size
            .value;
        let (volume, seed) = self
            .player_tools
            .soil_deposition_budget(now, rate, batch_size);
        let edit = self.player_tools.stroke_edit_with_radius(
            action,
            center,
            self.soil_deposition_radius(),
        );
        let measure_hold = self.soil_performance_bench.is_some();
        let started = measure_hold.then(Instant::now);
        let deposition = self.apply_soil_deposition(edit, volume, seed);
        if let (Some(started), Ok(result)) = (started, deposition.as_ref()) {
            self.note_soil_performance_hold(
                result,
                volume,
                started.elapsed().as_secs_f64() * 1000.,
            );
        }
        match deposition {
            Ok(result) if result.added > 0 => {
                self.soil_bag_preview.note_added(result.added);
                self.start_terrain_edit_loop_sound(center);
                self.player_tools.record_stroke_dab(action, now, center);
            }
            Ok(_) => {
                // Keep fractional event credit until the next whole batch is due.
                self.player_tools.record_stroke_dab(action, now, center);
                self.stop_terrain_edit_loop_sound();
                if volume > 0 {
                    self.soil_bag_preview.allow_flow = false;
                    self.player_tools.interrupt_stroke(action);
                }
            }
            Err(error) => {
                self.soil_bag_preview.allow_flow = false;
                self.player_tools.interrupt_stroke(action);
                self.stop_terrain_edit_loop_sound();
                log::error!("Failed to deposit soil: {error:#}");
            }
        }
    }

    pub(super) fn apply_soil_deposition(
        &mut self,
        edit: TerrainBrushEdit,
        volume: u32,
        seed: u32,
    ) -> Result<DepositionResult> {
        let rooftop = self.rooftop_scene.is_some();
        let unlimited = self.infinite_materials_enabled();
        let volume = if unlimited {
            volume
        } else {
            volume.min(self.voxel_backpack.count(BackpackVoxel::Dirt))
        };
        let clump_radius = self
            .debug_settings
            .adjustables
            .soil_deposition_clump_radius
            .value;
        let request = DepositionRequest {
            start: edit.start * 256.,
            end: edit.end * 256.,
            radius: edit.radius * 256.,
            volume,
            clump_radius,
            repose_slope: self
                .debug_settings
                .adjustables
                .soil_deposition_repose_slope
                .value,
            seed,
        };
        // Material supply must not change geometry bounds or manufacture a model floor.
        let (legal, floor) = if rooftop {
            (
                UAabb3::new(rooftop_scene::SOIL_MIN, rooftop_scene::SOIL_MAX),
                Some(rooftop_scene::SOIL_MIN.y),
            )
        } else {
            (
                INITIAL_EDITABLE_TERRAIN_BOUNDS.voxel_bounds(VOXEL_DIM_PER_CHUNK),
                None,
            )
        };
        let fast_path = self
            .debug_settings
            .adjustables
            .soil_deposition_fast_path
            .value;
        let result = self
            .plain_builder
            .deposit_soil(request, legal, floor, fast_path)?;
        if let Some(changed) = result.changed {
            // Debit the authoritative write, even if a downstream publication later fails.
            // Existing soil is never fertilized, replaced or debited.
            if !unlimited {
                self.voxel_backpack
                    .withdraw(BackpackVoxel::Dirt, result.added);
            }
            let bound = UAabb3::new(
                changed.min().saturating_sub(UVec3::ONE),
                (changed.max() + UVec3::ONE)
                    .min(self.world_chunk_dim * VOXEL_DIM_PER_CHUNK - UVec3::ONE),
            );
            self.surface_builder
                .remove_flora_buried_by_voxels(&result.deposited_voxels)?;
            self.publish_visible_terrain(
                VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                    bound,
                )])?
                .context("soil deposition has changed terrain")?,
            )?;
            if !rooftop {
                self.observe_player_terrain_publication_for_connectivity(bound);
            }
        }
        Ok(result)
    }
}
