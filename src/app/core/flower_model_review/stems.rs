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
        surface_cell_scale: if sweep && phase == 13 { 4. } else { 1. },
        radius_scale: if sweep && phase == 10 {
            0.25
        } else if sweep && phase == 13 {
            2.
        } else {
            0.7
        },
        branches: !(sweep && phase == 13),
        freeze_motion: !(sweep && (8..=9).contains(&phase)),
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
        s.flower_stem_surface_cell_scale.value = p.surface_cell_scale;
        s.flower_stem_radius_scale.value = p.radius_scale;
        s.flower_stem_test_branches.value = p.branches;
        s.flower_stem_freeze_motion.value = p.freeze_motion;
        let target = self.flower_model_review.as_ref().unwrap().target.unwrap();
        let sweep = mode == "stems";
        let step = (frame % 24) as f32 / 23.;
        let offset = Vec3::new(0., 0.14, 0.43);
        let (camera, focus, motion) = if sweep && (4..=9).contains(&phase) {
            (
                target + Quat::from_rotation_y((step - 0.5) * 0.8) * offset,
                target,
                "orbit",
            )
        } else if sweep && phase == 14 {
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
        if (sweep && frame % 24 == 0 && frame <= 15 * 24) || (!sweep && frame == 0) {
            log::info!("[STEM_REVIEW_PHASE] phase={phase} enabled={} method={} motion={motion} resolution={} branches={} freeze={} saved=false", p.enabled, p.sampling, p.direction_resolution, p.branches, p.freeze_motion);
        }
        if frame.is_multiple_of(8) && frame < if sweep { 16 * 24 } else { 24 } {
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
