//! Opt-in real-game visual/lifecycle fixture. No settings or terrain snapshots are saved.
use super::{planting::AuthoredFloraPlacementBatch, App};
use crate::app::world_edits::TerrainBrushEdit;
use anyhow::{ensure, Result};
use glam::{UVec2, Vec3};

pub(super) struct Review {
    mode: String,
    frame: u32,
    base: Option<Vec3>,
    control_step: usize,
    control_frames: u32,
    control_baseline: Option<crate::tracer::ModelSurfaceCacheOptions>,
}

impl Review {
    pub fn from_env() -> Result<Option<Self>> {
        let Ok(mode) = std::env::var("RE_FLORA_SPRINKLER_REVIEW") else {
            return Ok(None);
        };
        ensure!(
            matches!(mode.as_str(), "near" | "garden" | "orbit" | "lifecycle" | "pixels-8" | "pixels-16" | "pixels-32" | "controls"),
            "RE_FLORA_SPRINKLER_REVIEW must be near, garden, orbit, lifecycle, pixels-8, pixels-16, pixels-32 or controls"
        );
        Ok(Some(Self {
            mode,
            frame: 0,
            base: None,
            control_step: 0,
            control_frames: 0,
            control_baseline: None,
        }))
    }
}

impl App {
    pub(super) fn prepare_sprinkler_review(&mut self) -> Result<()> {
        if !self.terrain_persistence.allows_world_updates() {
            return Ok(());
        }
        let Some(mut review) = self.sprinkler_review.take() else {
            return Ok(());
        };
        if review.frame == 0 {
            ensure!(
                self.sprinklers.is_empty(),
                "sprinkler review needs an empty fixture"
            );
            let anchor = self
                .resolve_plantable_surface_column(UVec2::new(196, 360))
                .map_err(|e| anyhow::anyhow!("sprinkler review surface: {e:?}"))?;
            let base = anchor.surface_position_ws();
            self.apply_sprinkler_placement(base)?;
            let mut plants = AuthoredFloraPlacementBatch::new();
            for (index, column) in [
                UVec2::new(180, 346),
                UVec2::new(211, 345),
                UVec2::new(216, 367),
                UVec2::new(176, 365),
            ]
            .into_iter()
            .enumerate()
            {
                let anchor = self
                    .resolve_plantable_surface_column(column)
                    .map_err(|e| anyhow::anyhow!("sprinkler review flower: {e:?}"))?;
                ensure!(
                    self.try_place_authored_flora(
                        &mut plants,
                        crate::flora::MODEL_FLOWER_FIRST_SPECIES + index as u32,
                        anchor,
                        255,
                        0,
                        index as u32 * 137
                    ),
                    "sprinkler review planting"
                );
            }
            self.finish_authored_flora_placement(plants)?;
            review.base = Some(base);
            self.set_manual_time_of_day(0.43);
            self.debug_settings.adjustables.auto_daynight_cycle.value = false;
            if let Some(n) = review.mode.strip_prefix("pixels-") {
                self.debug_settings
                    .adjustables
                    .sprinkler_cache_enabled
                    .value = true;
                self.debug_settings
                    .adjustables
                    .sprinkler_pixel_resolution
                    .value = n.parse()?;
            }
            review.control_baseline =
                Some(super::render_frame_input::model_cache_inputs(&self.debug_settings).surfaces);
            log::info!("[SPRINKLER_REVIEW] mode={} base={base:?} placement=production raster=true saved=false", review.mode);
        }
        let base = review.base.unwrap();
        let target = base + Vec3::Y * (3. / 256.);
        let offset = match review.mode.as_str() {
            "garden" => Vec3::new(0.20, 0.19, 0.32),
            "orbit" => {
                let angle = review.frame as f32 * 0.012;
                Vec3::new(angle.sin() * 0.12, 0.065, angle.cos() * 0.12)
            }
            _ => Vec3::new(0.055, 0.045, 0.080),
        };
        self.camera_control.apply_snapshot_mode(true);
        self.camera_control.set_orbit_focus(target);
        ensure!(
            self.tracer
                .set_camera_pose_looking_at(target + offset, target),
            "sprinkler review camera"
        );
        self.reset_camera_movement_input();
        if review.mode == "lifecycle" {
            if review.frame == 40 {
                self.apply_sprinkler_placement(base + Vec3::X * 0.035)?;
                self.apply_sprinkler_placement(base - Vec3::X * 0.035)?;
                ensure!(
                    self.sprinklers.len() == 3 && self.tracer.sprinkler_instance_count() == 3,
                    "sprinkler instances must publish together with gameplay records"
                );
            }
            if review.frame == 120 {
                ensure!(
                    self.remove_sprinklers_in_brush(TerrainBrushEdit {
                        start: base,
                        end: base,
                        radius: 0.06,
                    })? == 3,
                    "sprinkler brush removal"
                );
                ensure!(
                    self.sprinklers.is_empty() && self.tracer.sprinkler_instance_count() == 0,
                    "removal must retire rendering and water emitters"
                );
            }
            if review.frame == 150 {
                self.apply_sprinkler_placement(base)?;
            }
        }
        if review.mode == "controls" {
            const STEPS: [(bool, u32); 7] = [
                (true, 8),
                (true, 16),
                (true, 32),
                (false, 32),
                (true, 24),
                (true, 8),
                (true, 32),
            ];
            if let Some(&(enabled, resolution)) = STEPS.get(review.control_step) {
                let gui = &mut self.debug_settings.adjustables;
                gui.sprinkler_cache_enabled.value = enabled;
                gui.sprinkler_pixel_resolution.value = resolution;
                let inputs =
                    super::render_frame_input::model_cache_inputs(&self.debug_settings).surfaces;
                for kind in 0..4 {
                    ensure!(
                        inputs.for_kind(kind) == review.control_baseline.unwrap().for_kind(kind),
                        "sprinkler control changed another model's sampling"
                    );
                }
                if review.control_frames > 0
                    && self.tracer.sprinkler_cache_status() == Some((enabled, resolution))
                {
                    review.control_frames += 1;
                    if review.control_frames >= 12 {
                        log::info!("[SPRINKLER_CACHE_REVIEW] step={} cached={enabled} resolution={resolution} other_models_unchanged=true",
                            review.control_step);
                        review.control_step += 1;
                        review.control_frames = 0;
                        if review.control_step == STEPS.len() {
                            log::info!("[SPRINKLER_CACHE_REVIEW] complete steps=7 saved=false");
                        }
                    }
                } else {
                    review.control_frames = 1;
                }
            }
        }
        if review.frame == 180 {
            ensure!(
                self.sprinklers.len() == 1 && self.tracer.sprinkler_instance_count() == 1,
                "sprinkler review final publication"
            );
            log::info!(
                "[SPRINKLER_REVIEW] passed mode={} frames=180 instances=1 saved=false",
                review.mode
            );
        }
        review.frame += 1;
        self.sprinkler_review = Some(review);
        Ok(())
    }
}
