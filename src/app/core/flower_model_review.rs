//! Opt-in hidden Release fixture. Plants via production placement and switches
//! the real saved-field inputs in memory; never writes settings or terrain.
use super::{
    planting::{AuthoredFloraPlacementBatch, PlantableSurfaceAnchor},
    App,
};
use anyhow::{ensure, Result};
use glam::{UVec2, Vec3};

pub(super) struct FlowerModelReview {
    mode: String,
    frame: u32,
    target: Option<Vec3>,
    last_anchor: Option<PlantableSurfaceAnchor>,
}
impl FlowerModelReview {
    pub fn from_env() -> Result<Option<Self>> {
        let Ok(mode) = std::env::var("RE_FLORA_FLOWER_MODEL_REVIEW") else {
            return Ok(None);
        };
        ensure!(["a", "b", "ab", "controls"].contains(&mode.as_str()),
            "RE_FLORA_FLOWER_MODEL_REVIEW must be a, b, ab, or controls (saved-field shape/view/allocation sweep)");
        Ok(Some(Self {
            mode,
            frame: 0,
            target: None,
            last_anchor: None,
        }))
    }
}
impl App {
    pub(super) fn prepare_flower_model_review(&mut self) -> Result<()> {
        let Some(review) = &mut self.flower_model_review else {
            return Ok(());
        };
        let frame = review.frame;
        review.frame += 1;
        let controls = review.mode == "controls";
        let phase = if controls {
            (frame / 24).min(26)
        } else if review.mode == "ab" {
            (frame / 24).min(8)
        } else {
            0
        };
        let control = control_settings(phase);
        // Legacy a/b/ab inputs now exercise the selected heads-only renderer.
        let heads = true;
        let resolution = if controls {
            control.resolution
        } else {
            match phase {
                1 => 8,
                3 => 64,
                _ => 32,
            }
        };
        let settings = &mut self.debug_settings.adjustables;
        settings.model_flower_pixel_resolution.value = resolution;
        settings.model_flower_view_count.value = if controls {
            control.views
        } else if phase == 5 {
            8
        } else {
            16
        };
        settings.model_flower_head_scale.value = if controls {
            control.shape.head_scale
        } else {
            1.
        };
        settings.model_flower_height_scale.value = if controls {
            control.shape.height_scale
        } else {
            1.
        };
        if controls {
            settings.model_flower_height_variance.value = control.shape.height_variance;
            settings.model_flower_voxel_scale.value = control.shape.voxel_scale;
        }
        settings.model_flower_size_scale.value = if controls {
            control.size_scale
        } else if phase == 7 {
            0.5
        } else if phase == 8 {
            2.0
        } else {
            1.0
        };
        // Color-only phases exercise the live saved palette without changing
        // source geometry or the pixelated head/calyx material.
        if controls && phase >= 19 {
            let (bottom, tip) = if phase == 19 {
                (egui::Color32::BLUE, egui::Color32::RED)
            } else {
                (egui::Color32::RED, egui::Color32::BLUE)
            };
            settings.model_flower_stem_bottom_color.value = bottom;
            settings.model_flower_stem_tip_color.value = tip;
            if frame == phase * 24 {
                log::info!(
                    "[FLOWER_STEM_COLORS] phase={phase} bottom={bottom:?} tip={tip:?} saved=false"
                );
            }
        }
        settings.model_pixel_view_count.value = if !controls && phase == 5 { 8 } else { 16 };
        settings.flora_growth_override_enabled.value = true;
        settings.flora_growth_override.value = if controls {
            if phase == 17 {
                0.55
            } else {
                1.
            }
        } else if phase == 7 {
            0.25
        } else {
            1.0
        };
        settings.auto_daynight_cycle.value = false;
        if frame == 0 {
            self.select_item_panel_slot(super::ui_style::STAFF_SLOT_INDEX);
            let mut batch = AuthoredFloraPlacementBatch::new();
            let mut center = Vec3::ZERO;
            for model in 0..crate::flora::models::MODEL_COUNT {
                let species = crate::flora::MODEL_FLOWER_FIRST_SPECIES + model as u32;
                let x = 166 + (model as u32 % 4) * 34;
                let z = 342 + (model as u32 / 4) * 34;
                let anchor = self
                    .resolve_plantable_surface_column(UVec2::new(x, z))
                    .map_err(|e| anyhow::anyhow!("flower fixture column ({x},{z}): {e:?}"))?;
                center += anchor.base_center_vox() / 256.0;
                self.flower_model_review.as_mut().unwrap().last_anchor = Some(anchor);
                ensure!(
                    self.try_place_authored_flora(
                        &mut batch,
                        species,
                        anchor,
                        255,
                        0,
                        model as u32 * 137
                    ),
                    "flower fixture planting failed for species {species}"
                );
                log::info!(
                    "[FLOWER_REVIEW_PLANT] species={species} root={:?}",
                    anchor.base_world_vox()
                );
            }
            self.finish_authored_flora_placement(batch)?;
            center /= crate::flora::models::MODEL_COUNT as f32;
            let target = center + Vec3::Y * 0.035;
            let camera = target + Vec3::new(0., 0.24, 0.57);
            self.camera_control.apply_snapshot_mode(true);
            self.camera_control.set_orbit_focus(target);
            ensure!(
                self.tracer.set_camera_pose_looking_at(camera, target),
                "flower fixture camera"
            );
            self.flower_model_review.as_mut().unwrap().target = Some(target);
            self.set_manual_time_of_day(0.45);
            log::info!("[FLOWER_REVIEW] planted=8 placement=production saved=false target={target:?} camera={camera:?}");
        }
        if (!controls && phase == 3 && frame == 72) || (controls && phase == 17 && frame == 408) {
            if let Some(resize) = &mut self.resize_lifecycle_test {
                // Replay the existing extent test after flowers have actually
                // submitted draws, not just during loading.
                resize.requested = 0;
                resize.next_request_frame = 0;
                resize.complete = false;
                log::info!("[FLOWER_REVIEW_RESIZE] after_submitted_frames={frame}");
            }
        }
        if !controls && phase >= 7 && frame == phase * 24 {
            let anchor = self
                .flower_model_review
                .as_ref()
                .unwrap()
                .last_anchor
                .unwrap();
            let species = crate::flora::species_count() as u32 - 1;
            if phase == 7 {
                let point = anchor.base_center_vox() / 256.0;
                let removed = self.surface_builder.remove_authored_flora_for_brush(
                    anchor.base_world_vox() / 256,
                    point,
                    point,
                    1.0 / 256.0,
                )?;
                ensure!(removed == 1, "fixture must remove exactly one plant");
                log::info!("[FLOWER_REVIEW_LIFETIME] removed=1 species={species}");
            } else {
                let mut batch = AuthoredFloraPlacementBatch::new();
                let now = self.time_info.time_since_start_duration().as_millis() as u32;
                ensure!(
                    self.try_place_authored_flora(&mut batch, species, anchor, 255, now, 959),
                    "fixture replant"
                );
                self.finish_authored_flora_placement(batch)?;
                log::info!(
                    "[FLOWER_REVIEW_LIFETIME] replanted=1 species={species} spawn_time_ms={now}"
                );
            }
        }
        // Other opt-in startup scenes may set a camera once after planting.
        // Keep this explicit review's camera authoritative throughout capture.
        if let Some(target) = self.flower_model_review.as_ref().unwrap().target {
            self.camera_control.apply_snapshot_mode(true);
            self.tracer
                .set_camera_pose_looking_at(target + Vec3::new(0., 0.24, 0.57), target);
            self.reset_camera_movement_input();
        }
        if frame.is_multiple_of(24) && frame / 24 <= if controls { 26 } else { 8 } {
            if controls {
                log::info!("[FLOWER_CONTROLS_PHASE] phase={phase} heads_only={heads} resolution={resolution} views={} head_scale={} height_mean={} height_variance={} voxel_scale={} size={} frame={frame} saved=false", control.views, control.shape.head_scale, control.shape.height_scale, control.shape.height_variance, control.shape.voxel_scale, control.size_scale);
            } else {
                log::info!("[FLOWER_REVIEW_PHASE] phase={phase} heads_only={heads} resolution={resolution} frame={frame} saved=false");
            }
        }
        Ok(())
    }
}

/// Bounded production-input sweep. Large view counts deliberately use 8px;
/// maximum simultaneous VRAM stress and visual/performance approval are separate.
fn control_settings(phase: u32) -> crate::flora::models::Settings {
    use crate::flora::models::{Settings, Shape};
    let mut s = Settings::default();
    s.shape = match phase {
        1 | 2 => Shape {
            head_scale: 2.,
            height_scale: 1.,
            ..Shape::default()
        },
        3 => Shape {
            head_scale: 2.,
            height_scale: 2.,
            ..Shape::default()
        },
        4 | 5 => Shape {
            head_scale: 1.,
            height_scale: 2.,
            ..Shape::default()
        },
        12 | 13 => Shape {
            head_scale: 0.25,
            height_scale: 4.,
            ..Shape::default()
        },
        14 | 15 => Shape {
            head_scale: 4.,
            height_scale: 0.25,
            ..Shape::default()
        },
        21 => Shape {
            voxel_scale: 0.9,
            ..Shape::default()
        },
        23 => Shape {
            voxel_scale: 0.9,
            height_variance: 0.09,
            ..Shape::default()
        },
        24 => Shape {
            height_scale: 0.25,
            height_variance: 1.,
            voxel_scale: 0.2,
            ..Shape::default()
        },
        25 => Shape::MAX,
        _ => Shape::default(),
    };
    s.resolution = match phase {
        6..=9 => 8,
        10 => 64,
        _ => 32,
    };
    s.views = match phase {
        7 => 512,
        8 => 37,
        9 | 10 => 8,
        _ => 16,
    };
    s.size_scale = if phase == 17 { 2. } else { 1. };
    s
}
