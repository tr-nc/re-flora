use super::App;
use crate::app::world_edits::{TreeAddOptions, TreePlacement};
use glam::{Vec2, Vec3};
use std::time::{Duration, Instant};

// Opt-in diagnostic only: retain normal rendering and the real Grow publication path.
// No GUI settings are changed; --tree-bench-samples is the final tree count (up to 9).
#[derive(Debug)]
struct TreeScaleDiagnostic {
    trees: u32,
    maximum: u32,
    phase_start: Option<Instant>,
}

impl TreeScaleDiagnostic {
    fn new(maximum: u32) -> Self {
        Self {
            trees: 1,
            maximum: maximum.clamp(1, 9),
            phase_start: None,
        }
    }

    fn step(&mut self, app: &mut App) -> bool {
        if app.render_start_time.is_none() {
            return false;
        }
        let Some(start) = self.phase_start else {
            self.phase_start = Some(Instant::now());
            log::info!(
                "[DEBUG-tree-scale] phase trees=1 frame={}",
                app.time_info.total_frame_count()
            );
            return false;
        };
        let elapsed = start.elapsed();
        if elapsed >= Duration::from_millis(500) {
            let timing = app.frame_timing_snapshot;
            log::info!(
                "[DEBUG-tree-scale] sample trees={} frame={} total_ms={:.3} gpu_present_ms={:.3} untracked_cpu_ms={:.3}",
                self.trees, timing.frame, timing.total_ms, timing.gpu_present_ms, timing.untracked_cpu_ms,
            );
        }
        if elapsed < Duration::from_secs(2) {
            return false;
        }
        if self.trees == self.maximum {
            log::info!("[DEBUG-tree-scale] completed trees={}", self.trees);
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
        let mut desc = app.debug_settings.tree.desc.clone();
        desc.branching.seed = 122 + u64::from(self.trees);
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
            "[DEBUG-tree-scale] phase trees={} frame={} placement_ms={:.3} position={position:?}",
            self.trees,
            app.time_info.total_frame_count(),
            publication.elapsed().as_secs_f64() * 1000.0,
        );
        false
    }
}

#[derive(Debug)]
pub(super) struct TreeBench {
    samples: u32,
    next_sample: u32,
    results: Vec<f32>,
    scale_diagnostic: Option<TreeScaleDiagnostic>,
}

impl TreeBench {
    pub(super) fn new(samples: u32) -> Self {
        Self {
            samples: samples.max(1),
            next_sample: 0,
            results: Vec::new(),
            scale_diagnostic: std::env::var_os("RE_FLORA_TREE_SCALE_DIAGNOSTIC")
                .map(|_| TreeScaleDiagnostic::new(samples)),
        }
    }

    pub(super) fn run_next(app: &mut App) -> bool {
        let Some(mut bench) = app.launch_owners.tree_bench.take() else {
            return false;
        };

        let done = match &mut bench.scale_diagnostic {
            Some(diagnostic) => diagnostic.step(app),
            None => bench.run_sample(app),
        };
        if !done {
            app.launch_owners.tree_bench = Some(bench);
        }
        done
    }

    fn run_sample(&mut self, app: &mut App) -> bool {
        if self.next_sample >= self.samples {
            self.log_summary();
            return true;
        }

        self.next_sample += 1;
        let sample = self.next_sample;

        let mut tree_desc = app.debug_settings.tree.desc.clone();
        let t = if self.samples <= 1 {
            0.0
        } else {
            (sample - 1) as f32 / (self.samples - 1) as f32
        };
        tree_desc.branching.initial_length = 32.0 + t * 64.0;
        tree_desc.branching.seed = 122;
        app.debug_settings.tree.desc = tree_desc;

        let start = Instant::now();
        match app.replace_single_tree(app.debug_settings.tree.desc.clone(), app.debug_tree_pos) {
            Ok(()) => {
                let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
                self.results.push(elapsed_ms);
                log::info!(
                    "[PERF][TREE_BENCH] sample {}/{} visible_publication_total {:.2}ms initial_length {:.2} seed {}",
                    sample,
                    self.samples,
                    elapsed_ms,
                    app.debug_settings.tree.desc.branching.initial_length,
                    app.debug_settings.tree.desc.branching.seed,
                );
            }
            Err(err) => {
                log::error!("[PERF][TREE_BENCH] sample {sample} failed: {err}");
                self.log_summary();
                return true;
            }
        }

        false
    }

    fn log_summary(&self) {
        if self.results.is_empty() {
            log::info!("[PERF][TREE_BENCH_SUMMARY] samples 0");
            return;
        }

        let sum = self.results.iter().sum::<f32>();
        let avg = sum / self.results.len() as f32;
        let max = self
            .results
            .iter()
            .copied()
            .fold(0.0_f32, |acc, value| acc.max(value));

        log::info!(
            "[PERF][TREE_BENCH_SUMMARY] samples {} avg {:.2}ms max {:.2}ms",
            self.results.len(),
            avg,
            max,
        );
    }
}
