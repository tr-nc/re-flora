//! Opt-in real-render capture fixture. Normal play never enters this module.
//! Uses production particles, flight, lighting and the existing denoiser capture.
use super::App;
use crate::particles::{ParticleRenderKind, ParticleSpawn, STANDARD_PARTICLE_SIZE};
use anyhow::{bail, Result};
use glam::{Vec3, Vec4};

pub(super) struct FallenLeafReview {
    mode: String,
    frame: u32,
}

impl FallenLeafReview {
    pub fn from_env() -> Result<Option<Self>> {
        let Ok(mode) = std::env::var("RE_FLORA_FALLEN_LEAF_REVIEW") else {
            return Ok(None);
        };
        if !matches!(mode.as_str(), "a" | "b" | "switch" | "natural-b") {
            bail!("RE_FLORA_FALLEN_LEAF_REVIEW must be a, b, switch or natural-b");
        }
        Ok(Some(Self { mode, frame: 0 }))
    }
}

impl App {
    pub(super) fn prepare_fallen_leaf_review(&mut self) {
        let Some(review) = &mut self.fallen_leaf_review else {
            return;
        };
        let frame = review.frame;
        let fixture = review.mode != "natural-b";
        let enabled = match review.mode.as_str() {
            "a" => false,
            "switch" => (120..240).contains(&frame),
            _ => true,
        };
        self.debug_settings.adjustables.fallen_leaf_flight.value = enabled;
        review.frame += 1;
        if frame == 0 && fixture {
            let camera = Vec3::new(1., 1.55, 1.8);
            let target = Vec3::new(1., 1.55, 1.4);
            self.camera_control.apply_snapshot_mode(true);
            self.camera_control.set_orbit_focus(target);
            assert!(self.tracer.set_camera_pose_looking_at(camera, target));
            self.set_manual_time_of_day(0.45);
            self.debug_settings.adjustables.auto_daynight_cycle.value = false;
            for index in 0..8 {
                // Same square size and fixed initial conditions in A and B. Seed
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
            log::info!("[LEAF_FLIGHT_REVIEW] frame={} variant={} leaves={} sample={} position={:?} velocity={:?} normal={:?}",
                review.frame, if leaf.leaf_orientation.is_some() { "B" } else { "A" },
                leaves, index, leaf.position_ws, leaf.velocity,
                leaf.leaf_orientation.map(|q| q * Vec3::Z));
        }
    }
}
