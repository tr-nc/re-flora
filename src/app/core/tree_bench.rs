use super::App;
use std::time::Instant;

#[derive(Debug)]
pub(super) struct TreeBench {
    samples: u32,
    next_sample: u32,
    mode: TreeBenchMode,
    rapid: bool,
    active_sample: Option<TreeBenchActiveSample>,
    results: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum TreeBenchMode {
    TreeHeight,
    MinTrunkThickness,
}

#[derive(Debug)]
struct TreeBenchActiveSample {
    sample: u32,
    start: Instant,
    initial_length: f32,
    min_trunk_thickness: f32,
    seed: u64,
}

impl TreeBench {
    pub(super) fn new(samples: u32, mode: TreeBenchMode, rapid: bool) -> Self {
        Self {
            samples: samples.max(1),
            next_sample: 0,
            mode,
            rapid,
            active_sample: None,
            results: Vec::new(),
        }
    }

    pub(super) fn run_next(app: &mut App) -> bool {
        let Some(mut bench) = app.tree_bench.take() else {
            return false;
        };

        let done = bench.run_sample(app);
        if !done {
            app.tree_bench = Some(bench);
        }
        done
    }

    fn run_sample(&mut self, app: &mut App) -> bool {
        if let Some(active) = self.active_sample.take() {
            if !app.deferred_chunk_rebuilds_idle() {
                self.active_sample = Some(active);
                return false;
            }

            let elapsed_ms = active.start.elapsed().as_secs_f32() * 1000.0;
            self.results.push(elapsed_ms);
            log::info!(
                "[PERF][TREE_BENCH] sample {}/{} replace_deferred_total {:.2}ms initial_length {:.2} min_trunk_thickness {:.3} seed {}",
                active.sample,
                self.samples,
                elapsed_ms,
                active.initial_length,
                active.min_trunk_thickness,
                active.seed,
            );
        }

        if self.next_sample >= self.samples {
            if self.rapid && !app.deferred_chunk_rebuilds_idle() {
                return false;
            }
            self.log_summary();
            return true;
        }

        self.next_sample += 1;
        let sample = self.next_sample;

        let mut tree_desc = app.debug_tree_desc.clone();
        let t = if self.samples <= 1 {
            0.0
        } else {
            (sample - 1) as f32 / (self.samples - 1) as f32
        };
        match self.mode {
            TreeBenchMode::TreeHeight => {
                tree_desc.branching.initial_length = 32.0 + t * 64.0;
            }
            TreeBenchMode::MinTrunkThickness => {
                tree_desc.trunk_thickness_min = 1.05 + t * 0.95;
            }
        }
        tree_desc.branching.seed = 122;
        app.debug_tree_desc = tree_desc;

        let start = Instant::now();
        match app.replace_single_tree_deferred(app.debug_tree_desc.clone(), app.debug_tree_pos) {
            Ok(()) => {
                let enqueue_elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
                log::info!(
                    "[PERF][TREE_BENCH] sample {}/{} enqueue {:.2}ms initial_length {:.2} min_trunk_thickness {:.3} seed {}",
                    sample,
                    self.samples,
                    enqueue_elapsed_ms,
                    app.debug_tree_desc.branching.initial_length,
                    app.debug_tree_desc.trunk_thickness_min,
                    app.debug_tree_desc.branching.seed,
                );
                if self.rapid {
                    self.results.push(enqueue_elapsed_ms);
                } else {
                    self.active_sample = Some(TreeBenchActiveSample {
                        sample,
                        start,
                        initial_length: app.debug_tree_desc.branching.initial_length,
                        min_trunk_thickness: app.debug_tree_desc.trunk_thickness_min,
                        seed: app.debug_tree_desc.branching.seed,
                    });
                }
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
