//! Retained opt-in reproduction, not a rendering mode or performance fix.
use super::super::App;
use crate::app::world_edits::{TreeAddOptions, TreePlacement};
use glam::{Vec2, Vec3};
use std::time::{Duration, Instant};

// Opt-in diagnostic only: retain normal rendering and the real Grow publication path.
// No GUI settings are changed; --tree-bench-samples is the final tree count (up to 9).
#[derive(Debug)]
pub(super) struct TreeScaleDiagnostic {
    trees: u32,
    maximum: u32,
    phase_start: Option<Instant>,
    conifer: bool,
    first_seed: u64,
    validate_gpu: bool,
}

impl TreeScaleDiagnostic {
    pub(super) fn new(maximum: u32) -> Self {
        Self {
            trees: 1,
            maximum: maximum.clamp(1, 9),
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
        if elapsed < Duration::from_secs(2) {
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
            if app.tracer.raster_trees.posed_surface.is_some() {
                app.validate_tree_surface_queries(false)
                    .expect("tree scale collision/query agreement");
            }
            log::info!("[TREE_SCALE_DIAG] validated trees={}", self.trees);
        }
        if self.trees == self.maximum {
            log::info!("[TREE_SCALE_DIAG] completed trees={}", self.trees);
            return true;
        }
        const OFFSETS: [Vec2; 8] = [
            Vec2::new(0.48, 0.0),
            Vec2::new(0.0, 0.48),
            Vec2::new(-0.48, 0.0),
            Vec2::new(0.0, -0.48),
            Vec2::new(0.36, 0.36),
            Vec2::new(-0.36, 0.36),
            Vec2::new(0.36, -0.36),
            Vec2::new(-0.36, -0.36),
        ];
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
            crate::tree_gen::TreeSpecies::GardenTree
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
