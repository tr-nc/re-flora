//! Internal native review fixture, not a public CLI or a GUI replacement.
//! Uses the real saved controls in memory, production flower placement/tree fruit
//! and the normal mesh-particle upload. Never saves settings or simulation poses.
use super::App;
use crate::particles::{ParticleRenderKind, ParticleSnapshot};
use anyhow::{ensure, Result};
use glam::{Vec3, Vec4};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Views32,
    Views128,
    Views256,
    Sweep,
}

pub(super) struct ModelViewReview {
    mode: Mode,
    frame: u32,
    rotation: f32,
    target: Option<Vec3>,
    surface_review: Option<String>,
}

impl ModelViewReview {
    pub(super) fn from_environment() -> Result<Option<Self>> {
        let Ok(value) = std::env::var("RE_FLORA_MODEL_VIEW_REVIEW") else {
            return Ok(None);
        };
        let surface_review = std::env::var("RE_FLORA_MODEL_SURFACE_REVIEW").ok();
        if let Some(mode) = &surface_review {
            ensure!(matches!(mode.as_str(), "on" | "off" | "apple" | "butterfly" | "flower" | "sweep"), "RE_FLORA_MODEL_SURFACE_REVIEW must be on, off, apple, butterfly, flower or sweep (requires model view fixture)");
        }
        let mode = match value.as_str() {
            "32" => Mode::Views32,
            "128" => Mode::Views128,
            "256" => Mode::Views256,
            "sweep" => Mode::Sweep,
            _ => anyhow::bail!("RE_FLORA_MODEL_VIEW_REVIEW must be 32, 128, 256 or sweep"),
        };
        let rotation = std::env::var("RE_FLORA_MODEL_VIEW_REVIEW_ROTATION")
            .unwrap_or_else(|_| "0.57".to_owned())
            .parse::<f32>()?;
        ensure!(
            rotation.is_finite(),
            "model view review rotation must be finite"
        );
        Ok(Some(Self {
            mode,
            frame: 0,
            rotation,
            target: None,
            surface_review,
        }))
    }

    fn direction_count(&self) -> u32 {
        match self.mode {
            Mode::Views32 => 32,
            Mode::Views128 => 128,
            Mode::Views256 => 256,
            Mode::Sweep => match self.frame / 30 {
                0 => 32,
                1 => 128,
                2 => 256,
                3 => 8,
                4 => 512,
                5 => 256,
                _ => 128,
            },
        }
    }

    fn angle(&self, frame: u32) -> f32 {
        self.rotation
            + if self.mode == Mode::Sweep {
                frame as f32 * 0.009
            } else {
                0.
            }
    }
}

impl App {
    pub(super) fn prepare_model_view_review(&mut self) -> Result<()> {
        let Some(review) = &mut self.model_view_review else {
            return Ok(());
        };
        review.target = self.flower_model_review.as_ref().and_then(|r| r.target());
        let target = review
            .target
            .ok_or_else(|| anyhow::anyhow!("model view fixture flowers are not planted"))?;
        let count = review.direction_count();
        let frame = review.frame;
        let sweep = review.mode == Mode::Sweep;
        let surface_sweep = review.surface_review.as_deref() == Some("sweep");
        let angle = if sweep && frame >= 180 {
            (frame - 180) as f32 * 0.01
        } else {
            0.
        };
        let camera = target + Vec3::new(angle.sin() * 0.57, 0.24, angle.cos() * 0.57);
        let gui = &mut self.debug_settings.adjustables;
        gui.model_pixel_view_count.value = count;
        gui.model_flower_height_variance.value = 0.;
        gui.model_flower_head_scale.value = 1.;
        gui.butterfly_mesh_preview.value = false;
        if let Some(mode) = &review.surface_review {
            let stage = if mode == "sweep" {
                (frame / 60).min(8)
            } else {
                0
            };
            let flags = match mode.as_str() {
                "off" => [false; 3],
                "apple" => [true, false, false],
                "butterfly" => [false, true, false],
                "flower" => [false, false, true],
                "sweep" => match stage {
                    0 => [false; 3],
                    1 => [true, false, false],
                    2 => [true, true, false],
                    7 => [true, false, true],
                    _ => [true; 3],
                },
                _ => [true; 3],
            };
            let resolutions = if mode == "sweep" {
                match stage {
                    4 => [8, 16, 16],
                    5 => [8, 32, 16],
                    6 => [8, 32, 24],
                    _ => [16; 3],
                }
            } else {
                [16; 3]
            };
            gui.apple_cache_enabled.value = flags[0];
            gui.butterfly_cache_enabled.value = flags[1];
            gui.model_flower_cache_enabled.value = flags[2];
            gui.apple_pixel_resolution.value = resolutions[0];
            gui.butterfly_pixel_resolution.value = resolutions[1];
            gui.model_flower_pixel_resolution.value = resolutions[2];
            if frame.is_multiple_of(60) {
                log::info!("[MODEL_SURFACE_REVIEW] stage={stage} enabled={flags:?} resolutions={resolutions:?} saved=false");
            }
        }
        // Retain attached fruit for fixed counts; sweep uses the existing production
        // fruit-cycle handoff to submit the dynamic apple vertex path too.
        gui.fruit_cycle.value = if (sweep || surface_sweep) && frame >= 90 {
            1.
        } else {
            0.7
        };
        self.camera_control.apply_snapshot_mode(true);
        self.camera_control.set_orbit_focus(target);
        ensure!(
            self.tracer.set_camera_pose_looking_at(camera, target),
            "model view fixture camera"
        );
        self.reset_camera_movement_input();
        if (sweep || surface_sweep) && matches!(frame, 90 | 180) {
            let (width, height) = if frame == 90 {
                (1023, 767)
            } else {
                (1280, 720)
            };
            let accepted = self
                .window_state
                .window()
                .request_inner_size(winit::dpi::PhysicalSize::new(width, height));
            if let Some(size) = accepted {
                self.queue_frame_extent(re_flora_vkn::Extent2D::new(size.width, size.height));
            }
            log::info!("[MODEL_VIEW_REVIEW_RESIZE] frame={frame} requested={width}x{height} accepted={accepted:?} saved=false");
        }
        if frame.is_multiple_of(30) && frame <= 210 {
            log::info!("[MODEL_VIEW_REVIEW] frame={frame} count={count} rotation={} camera={camera:?} grid=unchanged saved=false", self.model_view_review.as_ref().unwrap().angle(frame));
            if sweep && frame == 210 {
                log::info!(
                    "[MODEL_VIEW_REVIEW] complete=true counts_rotation_resize=true saved=false"
                );
            }
        }
        self.model_view_review.as_mut().unwrap().frame += 1;
        Ok(())
    }

    pub(super) fn append_model_view_review_particles(&mut self) {
        let Some(review) = &self.model_view_review else {
            return;
        };
        let Some(target) = review.target else {
            return;
        };
        // prepare_model_view_review already advanced the next-frame counter;
        // these poses and reference logs belong to the controls just published.
        let frame = review.frame.saturating_sub(1);
        let angle = review.angle(frame);
        for index in 0..4 {
            let x = (index as f32 - 1.5) * 0.095;
            let heading = angle + index as f32 * 0.35;
            self.particle_snapshots.push(ParticleSnapshot {
                position_ws: target + Vec3::new(x, 0.23, 0.04),
                velocity: Vec3::new(heading.sin(), 0., -heading.cos()) * 0.05,
                color: Vec4::ONE,
                size: 0.08,
                kind: ParticleRenderKind::Butterfly,
                palette_index: index as u32,
                animation_phase_offset: index as f32 * 0.137,
                animation_sample_time: Some(if review.mode == Mode::Sweep {
                    angle
                } else {
                    0.237
                }),
                butterfly_wingbeat: None,
                leaf_orientation: None,
                leaf_shape_seed: None,
                leaf_geometry: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_exercises_live_counts_and_keeps_fixed_comparisons_fixed() {
        let mut review = ModelViewReview {
            mode: Mode::Sweep,
            frame: 0,
            rotation: 0.57,
            target: None,
            surface_review: None,
        };
        for (frame, expected) in [
            (0, 32),
            (30, 128),
            (60, 256),
            (90, 8),
            (120, 512),
            (150, 256),
            (180, 128),
        ] {
            review.frame = frame;
            assert_eq!(review.direction_count(), expected);
        }
        review.mode = Mode::Views128;
        let angle = review.angle(review.frame);
        review.frame += 17;
        assert_eq!(review.angle(review.frame), angle);
        assert_eq!(review.direction_count(), 128);
    }
}
