//! Retained opt-in reproduction, not a rendering mode or performance fix.
use super::super::App;
use crate::app::world_edits::{TreeAddOptions, TreePlacement};
use glam::{Vec2, Vec3};
use std::time::{Duration, Instant};

// Opt-in diagnostic only: retain normal rendering and the real Grow publication path.
// No GUI settings are changed; --tree-bench-samples is the final tree count (up to 12).
// Preserve the original nine-tree workload prefix for historical comparisons.
const OFFSETS: [Vec2; 11] = [
    Vec2::new(0.48, 0.0),
    Vec2::new(0.0, 0.48),
    Vec2::new(-0.48, 0.0),
    Vec2::new(0.0, -0.48),
    Vec2::new(0.36, 0.36),
    Vec2::new(-0.36, 0.36),
    Vec2::new(0.36, -0.36),
    Vec2::new(-0.36, -0.36),
    Vec2::new(0.72, 0.0),
    Vec2::new(-0.72, 0.0),
    Vec2::new(0.0, 0.72),
];
#[derive(Debug)]
pub(super) struct TreeScaleDiagnostic {
    trees: u32,
    maximum: u32,
    phase_start: Option<Instant>,
    conifer: bool,
    first_seed: u64,
    validate_gpu: bool,
    final_hold: Duration,
}

impl TreeScaleDiagnostic {
    pub(super) fn new(maximum: u32) -> Self {
        Self {
            trees: 1,
            final_hold: Duration::from_secs_f64(
                std::env::var("RE_FLORA_TREE_SCALE_FINAL_SECONDS")
                    .map(|value| {
                        value
                            .parse()
                            .expect("diagnostic final hold must be seconds")
                    })
                    .unwrap_or(2.),
            ),
            maximum: maximum.clamp(1, OFFSETS.len() as u32 + 1),
            phase_start: None,
            conifer: std::env::var("RE_FLORA_TREE_SCALE_SPECIES").as_deref() == Ok("conifer"),
            validate_gpu: std::env::var_os("RE_FLORA_TREE_SCALE_VALIDATE_GPU").is_some(),
            first_seed: std::env::var("RE_FLORA_TREE_SCALE_SEED")
                .map(|value| value.parse().expect("diagnostic seed must be an integer"))
                .unwrap_or(123),
        }
    }

    pub(super) fn step(&mut self, app: &mut App) -> bool {
        if app.render_start_time.is_none() {
            return false;
        }
        let Some(start) = self.phase_start else {
            self.phase_start = Some(Instant::now());
            log::info!(
                "[TREE_SCALE_DIAG] phase trees=1 frame={}",
                app.time_info.total_frame_count()
            );
            return false;
        };
        let elapsed = start.elapsed();
        if elapsed >= Duration::from_millis(500) {
            let timing = app.frame_timing_snapshot;
            log::info!(
                "[TREE_SCALE_DIAG] sample trees={} frame={} total_ms={:.3} gpu_present_ms={:.3} untracked_cpu_ms={:.3}",
                self.trees, timing.frame, timing.total_ms, timing.gpu_present_ms, timing.untracked_cpu_ms,
            );
        }
        let hold = if self.trees == self.maximum {
            self.final_hold
        } else {
            Duration::from_secs(2)
        };
        if elapsed < hold {
            return false;
        }
        if self.validate_gpu {
            app.validate_tree_poses()
                .expect("tree scale GPU hierarchy agreement");
            app.validate_tree_surface_pose()
                .expect("tree scale finite CPU surface");
            app.tracer
                .validate_gpu_tree_surface()
                .expect("tree scale GPU surface/BVH agreement");
            app.tracer
                .validate_gpu_tree_lighting(true)
                .expect("tree scale GPU lighting agreement");
            if app.tracer.raster_trees.gpu_wind_active {
                app.validate_tree_surface_queries(false)
                    .expect("tree scale collision/query agreement");
            }
            log::info!("[TREE_SCALE_DIAG] validated trees={}", self.trees);
        }
        if self.trees == self.maximum {
            log::info!("[TREE_SCALE_DIAG] completed trees={}", self.trees);
            return true;
        }
        let horizontal = Vec2::new(app.debug_tree_pos.x, app.debug_tree_pos.z)
            + OFFSETS[(self.trees - 1) as usize];
        let position = Vec3::new(
            horizontal.x,
            app.query_terrain_height_cpu(horizontal),
            horizontal.y,
        );
        let species = if self.conifer {
            crate::tree_gen::TreeSpecies::Conifer
        } else {
            crate::tree_gen::TreeSpecies::AppleTree
        };
        let mut desc = species.mature_desc(&app.debug_settings.tree.desc);
        if self.conifer {
            desc.axis_model = app.debug_settings.tree.conifer_axis_model;
        }
        desc.branching.seed = self.first_seed + u64::from(self.trees - 1);
        let publication = Instant::now();
        app.add_tree(
            desc,
            TreePlacement::World(position),
            TreeAddOptions::default().with_new_id(),
        )
        .expect("tree scale diagnostic placement");
        self.trees += 1;
        self.phase_start = Some(Instant::now());
        log::info!(
            "[TREE_SCALE_DIAG] phase trees={} frame={} placement_ms={:.3} position={position:?}",
            self.trees,
            app.time_info.total_frame_count(),
            publication.elapsed().as_secs_f64() * 1000.0,
        );
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn twelve_tree_replay_preserves_original_prefix_and_has_distinct_placements() {
        assert_eq!(OFFSETS.len(), 11);
        assert_eq!(OFFSETS[0], Vec2::new(0.48, 0.));
        assert_eq!(OFFSETS[7], Vec2::new(-0.36, -0.36));
        for (i, offset) in OFFSETS.iter().enumerate() {
            assert!(offset.is_finite() && *offset != Vec2::ZERO);
            assert!(!OFFSETS[..i].contains(offset));
        }
    }
}
