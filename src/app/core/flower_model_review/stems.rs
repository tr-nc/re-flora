//! Opt-in native exercise of the same saved fields used by the Debug panel.
//! No settings writes, alternate renderer or diagnostic-only sampling policy.
use super::App;
use crate::flora::models::StemExperiment;
use anyhow::{ensure, Result};
use glam::{Quat, Vec3};

fn policy(mode: &str, phase: u32) -> StemExperiment {
    // -1 is the original pipeline, including a return after experimental draws.
    let method = match mode {
        "stem-original" => -1,
        "stem-continuous" => 0,
        "stem-direction" => 1,
        "stem-surface" => 2,
        mode if mode.starts_with("stem-blocks") => 2,
        mode if mode.starts_with("stem-contract") => 1,
        "stems" => [-1, 0, 1, 2, -1, 0, 1, 2, 1, 2, 0, 1, 1, 2, 1, -1][phase.min(15) as usize],
        _ => unreachable!("validated stem review mode"),
    };
    let sweep = mode == "stems";
    StemExperiment {
        enabled: method >= 0,
        sampling: method.max(0) as u32,
        direction_resolution: if sweep && phase == 11 {
            128
        } else if sweep && phase == 12 {
            2048
        } else {
            512
        },
        object_sampling: if mode == "stem-contract" {
            matches!(phase, 1 | 3 | 5..=10)
        } else {
            mode.starts_with("stem-contract-b")
        },
        object_resolution: if mode == "stem-contract" && phase == 9 {
            32
        } else if mode == "stem-contract" && phase == 10 {
            512
        } else {
            256
        },
        surface_cell_scale: if sweep && phase == 13 { 4. } else { 1. },
        surface_geometry: if mode == "stem-blocks" {
            (1..=6).contains(&phase)
        } else {
            mode.starts_with("stem-blocks-") && mode != "stem-blocks-a"
        },
        geometry_cell_scale: if mode == "stem-blocks-fine"
            || (mode == "stem-blocks" && matches!(phase, 1 | 5))
        {
            0.25
        } else if mode == "stem-blocks-coarse" || (mode == "stem-blocks" && matches!(phase, 3 | 6))
        {
            4.
        } else {
            1.
        },
        radius_scale: if sweep && phase == 10 {
            0.25
        } else if sweep && phase == 13 {
            2.
        } else {
            0.7
        },
        branches: !(sweep && phase == 13),
        freeze_motion: !(sweep && (8..=9).contains(&phase))
            && !(mode == "stem-contract" && phase == 8)
            && !(mode == "stem-blocks" && phase == 4),
    }
}

impl App {
    pub(super) fn prepare_flower_stem_review(
        &mut self,
        mode: &str,
        frame: u32,
        phase: u32,
    ) -> Result<()> {
        let p = policy(mode, phase);
        let s = &mut self.debug_settings.adjustables;
        s.flower_stem_experiment.value = p.enabled;
        s.flower_stem_sampling.value = p.sampling;
        s.flower_stem_direction_resolution.value = p.direction_resolution;
        s.flower_stem_object_sampling.value = p.object_sampling;
        s.flower_stem_object_resolution.value = p.object_resolution;
        s.flower_stem_surface_cell_scale.value = p.surface_cell_scale;
        s.flower_stem_surface_geometry.value = p.surface_geometry;
        s.flower_stem_geometry_cell_scale.value = p.geometry_cell_scale;
        s.flower_stem_radius_scale.value = p.radius_scale;
        s.flower_stem_test_branches.value = p.branches;
        s.flower_stem_freeze_motion.value = p.freeze_motion;
        let target = self.flower_model_review.as_ref().unwrap().target.unwrap();
        let sweep = mode == "stems";
        let contract = mode == "stem-contract";
        let step = (frame % 24) as f32 / 23.;
        let offset = Vec3::new(0., 0.14, 0.43);
        let (camera, focus, motion) = if mode.starts_with("stem-contract") {
            let distance = if mode.ends_with("far") {
                4.0
            } else if contract {
                match phase {
                    2..=3 => 2.0,
                    4..=5 => 4.0,
                    _ => 1.0,
                }
            } else {
                1.0
            };
            let eye = target + offset * distance;
            if contract && phase == 6 {
                let turn = Quat::from_rotation_y((step - 0.5) * 0.16)
                    * Quat::from_rotation_x((step - 0.5) * 0.08);
                (eye, eye + turn * -offset, "turn")
            } else if contract && phase == 7 {
                (
                    target + Quat::from_rotation_y((step - 0.5) * 0.8) * offset,
                    target,
                    "orbit",
                )
            } else {
                (eye, target, "dolly")
            }
        } else if (sweep && (4..=9).contains(&phase)) || (mode == "stem-blocks" && phase == 5) {
            (
                target + Quat::from_rotation_y((step - 0.5) * 0.8) * offset,
                target,
                "orbit",
            )
        } else if (sweep && phase == 14) || (mode == "stem-blocks" && phase == 6) {
            // Cross the front of the fixture to exercise near-plane proxies.
            (
                target + offset * (0.8 - step * 0.9),
                target + Vec3::new(0., -0.02, -0.1),
                "near",
            )
        } else if sweep && phase < 4 {
            let camera = target + offset;
            let turn = Quat::from_rotation_y((step - 0.5) * 0.16)
                * Quat::from_rotation_x((step - 0.5) * 0.08);
            (camera, camera + turn * -offset, "turn")
        } else {
            (target + offset, target, "fixed")
        };
        ensure!(
            self.tracer.set_camera_pose_looking_at(camera, focus),
            "stem review camera"
        );
        let phase_count = if contract {
            12
        } else if sweep {
            16
        } else if mode == "stem-blocks" {
            8
        } else {
            1
        };
        if (phase_count > 1 && frame.is_multiple_of(24) && frame < phase_count * 24)
            || (phase_count == 1 && frame == 0)
        {
            log::info!("[STEM_REVIEW_PHASE] phase={phase} enabled={} method={} motion={motion} resolution={} branches={} freeze={} object={} object_pixels={} geometry={} geometry_cells={} saved=false", p.enabled, p.sampling, p.direction_resolution, p.branches, p.freeze_motion, p.object_sampling, p.object_resolution, p.surface_geometry, p.geometry_cell_scale);
        }
        if contract && frame.is_multiple_of(24) && frame <= 11 * 24 {
            log::info!("[STEM_CONTRACT_PHASE] phase={phase} object={} pixels={} direction={} views={} distance={} saved=false",p.object_sampling,p.object_resolution,p.direction_resolution,s.model_flower_view_count.value,(camera-target).length());
        }
        if frame.is_multiple_of(8) && frame < phase_count * 24 {
            log::info!(
                "[STEM_REVIEW_CAMERA] phase={phase} motion={motion} eye={:?} focus={:?}",
                camera.to_array(),
                focus.to_array()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_review_covers_saved_slider_endpoints_wind_and_return() {
        assert!(!policy("stem-blocks", 0).surface_geometry);
        assert_eq!(policy("stem-blocks", 1).geometry_cell_scale, 0.25);
        assert_eq!(policy("stem-blocks", 3).geometry_cell_scale, 4.);
        assert!(!policy("stem-blocks", 4).freeze_motion);
        for phase in 1..=6 {
            assert!(policy("stem-blocks", phase).surface_geometry_active());
        }
        assert!(!policy("stem-blocks", 7).surface_geometry);
    }

    #[test]
    fn contract_review_pairs_three_distances_and_exercises_live_b() {
        for phase in [0, 2, 4] {
            assert!(!policy("stem-contract", phase).object_sampling);
            assert!(policy("stem-contract", phase + 1).object_sampling);
        }
        assert!(!policy("stem-contract", 8).freeze_motion);
        assert_eq!(policy("stem-contract", 9).object_resolution, 32);
        assert_eq!(policy("stem-contract", 10).object_resolution, 512);
        assert!(!policy("stem-contract", 11).object_sampling);
        assert_eq!(
            policy("stem-contract-b-near", 0),
            policy("stem-contract-b-far", 0)
        );
    }

    #[test]
    fn review_covers_both_cameras_every_mode_and_returns_to_original() {
        for phase in 0..4 {
            assert_eq!(policy("stems", phase).enabled, phase != 0);
            assert_eq!(policy("stems", phase + 4), policy("stems", phase));
        }
        assert!(!policy("stems", 8).freeze_motion);
        assert!(!policy("stems", 9).freeze_motion);
        assert_eq!(policy("stems", 11).direction_resolution, 128);
        assert_eq!(policy("stems", 12).direction_resolution, 2048);
        assert!(!policy("stems", 13).branches);
        assert!(!policy("stems", 15).enabled);
    }
}
