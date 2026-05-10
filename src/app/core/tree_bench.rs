use super::App;
use std::time::Instant;

#[derive(Debug)]
pub(super) struct TreeBench {
    samples: u32,
    next_sample: u32,
    active_sample: Option<TreeBenchActiveSample>,
    results: Vec<f32>,
}

#[derive(Debug)]
struct TreeBenchActiveSample {
    sample: u32,
    start: Instant,
    tree_height: f32,
    seed: u64,
}

impl TreeBench {
    pub(super) fn new(samples: u32) -> Self {
        Self {
            samples: samples.max(1),
            next_sample: 0,
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
                "[PERF][TREE_BENCH] sample {}/{} replace_deferred_total {:.2}ms tree_height {:.2} seed {}",
                active.sample,
                self.samples,
                elapsed_ms,
                active.tree_height,
                active.seed,
            );
        }

        if self.next_sample >= self.samples {
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
        tree_desc.tree_height = 4.0 + t * 8.0;
        tree_desc.seed = 122 + sample as u64;
        app.debug_tree_desc = tree_desc;

        let start = Instant::now();
        match app.replace_single_tree_deferred(app.debug_tree_desc.clone(), app.debug_tree_pos) {
            Ok(()) => {
                let enqueue_elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
                log::info!(
                    "[PERF][TREE_BENCH] sample {}/{} enqueue {:.2}ms tree_height {:.2} seed {}",
                    sample,
                    self.samples,
                    enqueue_elapsed_ms,
                    app.debug_tree_desc.tree_height,
                    app.debug_tree_desc.seed,
                );
                self.active_sample = Some(TreeBenchActiveSample {
                    sample,
                    start,
                    tree_height: app.debug_tree_desc.tree_height,
                    seed: app.debug_tree_desc.seed,
                });
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
