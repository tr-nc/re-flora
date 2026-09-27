//! Opt-in real-render capture fixture. Normal play never enters this module.
//! Uses production particles, flight, lighting and the existing denoiser capture.
use super::App;
use crate::particles::{ParticleRenderKind, ParticleSpawn, STANDARD_PARTICLE_SIZE};
use anyhow::{bail, Result};
use glam::{Vec3, Vec4};

pub(super) struct FallenLeafReview {
    fixture: bool,
    frame: u32,
    model_review: Option<String>,
}

impl FallenLeafReview {
    pub fn from_env() -> Result<Option<Self>> {
        let model_review = std::env::var("RE_FLORA_LEAF_MODEL_REVIEW").ok();
        if model_review.as_ref().is_some_and(|m| m != "ab" && m != "b") {
            bail!("RE_FLORA_LEAF_MODEL_REVIEW must be ab (live toggle/resolution sweep) or b (fixed 16px candidate)");
        }
        let Ok(mode) = std::env::var("RE_FLORA_FALLEN_LEAF_REVIEW") else {
            if model_review.is_some() {
                bail!("RE_FLORA_LEAF_MODEL_REVIEW requires RE_FLORA_FALLEN_LEAF_REVIEW=fixture");
            }
            return Ok(None);
        };
        if model_review.is_some() && mode != "fixture" {
            bail!("leaf model review requires the bounded fixture, not natural emission");
        }
        let fixture = match mode.as_str() {
            "fixture" => true,
            "natural" => false,
            _ => bail!("RE_FLORA_FALLEN_LEAF_REVIEW must be fixture or natural; retired A/B modes are no longer supported"),
        };
        Ok(Some(Self {
            fixture,
            frame: 0,
            model_review,
        }))
    }
}

impl App {
    pub(super) fn prepare_fallen_leaf_review(&mut self) {
        let Some(review) = &mut self.fallen_leaf_review else {
            return;
        };
        let frame = review.frame;
        let fixture = review.fixture;
        review.frame += 1;
        if let Some(mode) = &review.model_review {
            // Diagnostic only; the real saved-field-bound controls own normal play.
            let (enabled, resolution) = if mode == "b" {
                (true, 16)
            } else {
                match frame / 30 {
                    0 | 4 => (false, 16),
                    1 => (true, 8),
                    2 => (true, 16),
                    3 => (true, 64),
                    _ => (true, 16),
                }
            };
            self.debug_settings.adjustables.falling_leaf_mesh.value = enabled;
            self.debug_settings
                .adjustables
                .falling_leaf_pixel_resolution
                .value = resolution;
            self.debug_settings
                .adjustables
                .falling_leaf_size_scale
                .value = if mode == "ab" {
                match frame / 30 {
                    4 => 2.,
                    6 => 0.25,
                    7 => 4.,
                    _ => 1.,
                }
            } else {
                1.
            };
        }
        if frame == 0 && fixture {
            let camera = Vec3::new(1., 1.55, 1.8);
            let target = Vec3::new(1., 1.55, 1.4);
            self.camera_control.apply_snapshot_mode(true);
            self.camera_control.set_orbit_focus(target);
            assert!(self.tracer.set_camera_pose_looking_at(camera, target));
            self.set_manual_time_of_day(0.45);
            self.debug_settings.adjustables.auto_daynight_cycle.value = false;
            for index in 0..8 {
                // Standard square size and repeatable initial conditions. Seed
                // variations initialize angular state; they never animate RGB.
                let color = self.debug_settings.adjustables.leaves_bottom_color.value;
                let spawn = ParticleSpawn {
                    position: Vec3::new(
                        1. + (index as f32 - 3.5) * 0.025,
                        1.64 + (index % 2) as f32 * 0.015,
                        1.47,
                    ),
                    color: Vec4::new(
                        color.r() as f32 / 255.,
                        color.g() as f32 / 255.,
                        color.b() as f32 / 255.,
                        1.,
                    ),
                    size: STANDARD_PARTICLE_SIZE,
                    lifetime: 30.,
                    speed_noise_offset: index as f32 * 137.,
                    drift_strength: 0.5,
                    drift_direction: Vec3::X,
                    ..ParticleSpawn::default()
                };
                assert!(self.particle_system.spawn(spawn).is_some());
            }
            log::info!("[LEAF_FLIGHT_REVIEW] fixture=8-production-leaf-particles size={} camera={camera:?} fixed_hz=60 saved_config_unchanged=true", STANDARD_PARTICLE_SIZE);
        }
    }

    pub(super) fn log_fallen_leaf_review(&self) {
        let Some(review) = &self.fallen_leaf_review else {
            return;
        };
        if !review.frame.is_multiple_of(30) {
            return;
        }
        let leaves = self
            .particle_snapshots
            .iter()
            .filter(|p| p.kind == ParticleRenderKind::Leaf)
            .count();
        for (index, leaf) in self
            .particle_snapshots
            .iter()
            .filter(|p| p.kind == ParticleRenderKind::Leaf)
            .take(8)
            .enumerate()
        {
            log::info!("[LEAF_FLIGHT_REVIEW] frame={} geometry={} leaves={} sample={} position={:?} velocity={:?} normal={:?}",
                review.frame,
                if self.debug_settings.adjustables.falling_leaf_mesh.value { "shared-3d-model" } else { "screen-facing" },
                leaves, index, leaf.position_ws, leaf.velocity,
                leaf.leaf_orientation.map(|q| q * Vec3::Z));
        }
    }
}
