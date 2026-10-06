//! Native fixture for the retained combined stem appearance and camera paths.
//! Uses the same saved tuning as Debug without writing settings.
use super::App;
use crate::flora::models::StemExperiment;
use anyhow::{ensure, Result};
use glam::{Quat, Vec3};

fn policy(_mode: &str, _phase: u32) -> StemExperiment {
    StemExperiment {
        pixelized: true,
        surface_cells: true,
        ..StemExperiment::default()
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
        s.flower_stem_model_resolution.value = p.model_resolution;
        s.flower_stem_radius_scale.value = p.radius_scale;
        s.flower_stem_test_branches.value = p.branches;
        let target = self.flower_model_review.as_ref().unwrap().target.unwrap();
        let step = (frame % 24) as f32 / 23.;
        let offset = Vec3::new(0., 0.14, 0.43);
        let sweep = mode == "stems";
        let (camera, focus) = if sweep && phase >= 12 {
            (
                target + offset * (0.8 - step * 0.9),
                target + Vec3::new(0., -0.02, -0.1),
            )
        } else if sweep && phase >= 8 {
            (target + offset * (1. + step * 3.), target)
        } else if sweep && phase >= 4 {
            (
                target + Quat::from_rotation_y((step - 0.5) * 0.8) * offset,
                target,
            )
        } else {
            (
                target + offset * if mode.ends_with("-far") { 4. } else { 1. },
                target,
            )
        };
        ensure!(
            self.tracer.set_camera_pose_looking_at(camera, focus),
            "stem review camera"
        );
        if frame.is_multiple_of(24) && ((sweep && frame < 16 * 24) || frame == 0) {
            log::info!("[STEM_REVIEW_PHASE] phase={phase} pixelized={} surface_cells={} branches={} model_resolution={} wind=live saved=false", p.pixelized, p.surface_cells, p.branches, p.model_resolution);
        }
        if frame.is_multiple_of(8) && sweep && frame < 16 * 24 {
            log::info!(
                "[STEM_REVIEW_CAMERA] phase={phase} eye={:?} focus={:?}",
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
    fn review_keeps_combined_appearance_with_each_camera_path() {
        for phase in 0..16 {
            let p = policy("stems", phase);
            assert!(p.pixelized);
            assert!(p.surface_cells);
            assert_eq!(policy("stem-model", 0), policy("stem-model-far", 0));
        }
        assert_eq!(policy("stem-combined", 0), policy("stems", 3));
    }
}
