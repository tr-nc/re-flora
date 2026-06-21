use super::App;
use crate::app::world_edits::TerrainBrushEdit;
use crate::flora::species;
use glam::{Vec2, Vec3};
use std::time::Instant;

#[derive(Debug)]
pub(super) struct AuthoredFloraBench {
    samples: u32,
    next_sample: u32,
    results: Vec<f32>,
}

impl AuthoredFloraBench {
    pub(super) fn new(samples: u32) -> Self {
        Self {
            samples: samples.max(1),
            next_sample: 0,
            results: Vec::new(),
        }
    }

    pub(super) fn run_next(app: &mut App) -> bool {
        let Some(mut bench) = app.authored_flora_bench.take() else {
            return false;
        };

        let done = bench.run_sample(app);
        if !done {
            app.authored_flora_bench = Some(bench);
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
        app.player_tools.flora_paint_selection_index = 1;
        let center_xz = authored_flora_bench_center(sample - 1);
        let center_y = app.query_terrain_height_cpu(center_xz);
        let center = Vec3::new(center_xz.x, center_y, center_xz.y);
        let edit = TerrainBrushEdit {
            start: center,
            end: center,
            radius: super::TERRAIN_EDIT_DEFAULT_RADIUS,
        };

        let start = Instant::now();
        match app.apply_surface_flora_regeneration(edit, sample, true) {
            Ok(()) => {
                let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
                self.results.push(elapsed_ms);
                log::info!(
                    "[PERF][AUTHORED_FLORA_BENCH] sample {}/{} paint_step {:.3}ms species {} center {:.3},{:.3},{:.3}",
                    sample,
                    self.samples,
                    elapsed_ms,
                    species::flora_paint_selection_label(app.current_flora_paint_selection()),
                    center.x,
                    center.y,
                    center.z,
                );
            }
            Err(err) => {
                log::error!("[PERF][AUTHORED_FLORA_BENCH] sample {sample} failed: {err}");
                self.log_summary();
                return true;
            }
        }

        false
    }

    fn log_summary(&self) {
        if self.results.is_empty() {
            log::info!("[PERF][AUTHORED_FLORA_BENCH_SUMMARY] samples 0");
            return;
        }

        let sum = self.results.iter().sum::<f32>();
        let avg = sum / self.results.len() as f32;
        let max = self
            .results
            .iter()
            .copied()
            .fold(0.0_f32, |acc, value| acc.max(value));
        let mut sorted = self.results.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let p95_idx = ((sorted.len() as f32 * 0.95).ceil() as usize).saturating_sub(1);
        let p95 = sorted[p95_idx.min(sorted.len() - 1)];

        log::info!(
            "[PERF][AUTHORED_FLORA_BENCH_SUMMARY] samples {} avg {:.3}ms p95 {:.3}ms max {:.3}ms",
            self.results.len(),
            avg,
            p95,
            max,
        );
    }
}

fn authored_flora_bench_center(sample_index: u32) -> Vec2 {
    // Spread samples across the default 256-voxel world so repeated releases exercise
    // authored placement over varied terrain, not only one brush neighborhood.
    let grid_x = sample_index % 5;
    let grid_z = (sample_index / 5) % 5;
    let cycle = sample_index / 25;
    let x_vox = 36.0 + grid_x as f32 * 46.0 + (cycle % 3) as f32 * 3.0;
    let z_vox = 36.0 + grid_z as f32 * 46.0 + ((cycle + 1) % 3) as f32 * 3.0;
    Vec2::new(x_vox / 256.0, z_vox / 256.0)
}
