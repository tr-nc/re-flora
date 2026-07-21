use crate::DenoiserBenchOptions;
use anyhow::{Context, Result};
use serde::Serialize;
use std::{fs, path::Path, time::Instant};

const REPORT_VERSION: u32 = 1;
const NOTICEABLE_DELTA_8BIT: u8 = 8;

#[derive(Debug, Serialize)]
struct TransitionMetrics {
    from_frame: u32,
    to_frame: u32,
    mean_abs_luma_delta_8bit: f64,
    p95_abs_luma_delta_8bit: u8,
    p99_abs_luma_delta_8bit: u8,
    max_abs_luma_delta_8bit: u8,
    noticeable_pixel_ratio: f64,
}

#[derive(Debug, Serialize)]
struct AggregateMetrics {
    mean_abs_luma_delta_8bit: f64,
    mean_p95_abs_luma_delta_8bit: f64,
    mean_p99_abs_luma_delta_8bit: f64,
    max_abs_luma_delta_8bit: u8,
    mean_noticeable_pixel_ratio: f64,
    max_transition_mean_abs_luma_delta_8bit: f64,
}

#[derive(Debug, Serialize)]
struct DenoiserBenchReport<'a> {
    version: u32,
    width: u32,
    height: u32,
    warmup_frames: u32,
    captured_frames: u32,
    transition_count: usize,
    noticeable_delta_threshold_8bit: u8,
    capture_seconds: f64,
    aggregate: AggregateMetrics,
    transitions: &'a [TransitionMetrics],
}

pub(super) struct DenoiserBench {
    options: DenoiserBenchOptions,
    presented_frames: u32,
    captured_frames: u32,
    width: u32,
    height: u32,
    previous_luma: Option<Vec<u8>>,
    transitions: Vec<TransitionMetrics>,
    capture_started: Option<Instant>,
}

impl DenoiserBench {
    pub(super) fn new(options: DenoiserBenchOptions) -> Self {
        Self {
            options,
            presented_frames: 0,
            captured_frames: 0,
            width: 0,
            height: 0,
            previous_luma: None,
            transitions: Vec::new(),
            capture_started: None,
        }
    }

    pub(super) fn should_capture(&self) -> bool {
        self.presented_frames >= self.options.warmup_frames
            && self.captured_frames < self.options.capture_frames
    }

    pub(super) fn mark_frame_presented(&mut self) {
        self.presented_frames += 1;
    }

    pub(super) fn record_frame(&mut self, width: u32, height: u32, rgba: &[u8]) -> Result<bool> {
        let expected_len = width as usize * height as usize * 4;
        anyhow::ensure!(
            rgba.len() == expected_len,
            "benchmark frame is {} bytes, expected {} for {}x{} RGBA",
            rgba.len(),
            expected_len,
            width,
            height
        );
        if self.captured_frames == 0 {
            self.width = width;
            self.height = height;
            self.capture_started = Some(Instant::now());
            log::info!(
                "[DENOISER_BENCH] warmup complete after {} presented frames; capturing {} frames at {}x{}",
                self.presented_frames,
                self.options.capture_frames,
                width,
                height
            );
        } else {
            anyhow::ensure!(
                (width, height) == (self.width, self.height),
                "benchmark extent changed from {}x{} to {}x{}",
                self.width,
                self.height,
                width,
                height
            );
        }

        let current_luma = rgba_to_luma(rgba);
        if let Some(previous_luma) = &self.previous_luma {
            self.transitions.push(analyze_transition(
                previous_luma,
                &current_luma,
                self.captured_frames - 1,
                self.captured_frames,
            ));
        }
        self.previous_luma = Some(current_luma);
        self.captured_frames += 1;

        if self.captured_frames == self.options.capture_frames {
            self.write_report()?;
            return Ok(true);
        }
        Ok(false)
    }

    fn write_report(&self) -> Result<()> {
        let aggregate = aggregate_metrics(&self.transitions);
        let report = DenoiserBenchReport {
            version: REPORT_VERSION,
            width: self.width,
            height: self.height,
            warmup_frames: self.options.warmup_frames,
            captured_frames: self.captured_frames,
            transition_count: self.transitions.len(),
            noticeable_delta_threshold_8bit: NOTICEABLE_DELTA_8BIT,
            capture_seconds: self
                .capture_started
                .map(|started| started.elapsed().as_secs_f64())
                .unwrap_or_default(),
            aggregate,
            transitions: &self.transitions,
        };
        let serialized = toml::to_string_pretty(&report).context("serialize denoiser report")?;
        let path = Path::new(&self.options.report_path);
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| {
                format!("create benchmark report directory {}", parent.display())
            })?;
        }
        fs::write(path, serialized)
            .with_context(|| format!("write denoiser report {}", path.display()))?;
        log::info!(
            "[DENOISER_BENCH] complete transitions={} mean_delta={:.4} p95_mean={:.4} p99_mean={:.4} noticeable_ratio={:.6} report={}",
            self.transitions.len(),
            report.aggregate.mean_abs_luma_delta_8bit,
            report.aggregate.mean_p95_abs_luma_delta_8bit,
            report.aggregate.mean_p99_abs_luma_delta_8bit,
            report.aggregate.mean_noticeable_pixel_ratio,
            path.display()
        );
        Ok(())
    }
}

fn rgba_to_luma(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .map(|pixel| {
            let weighted =
                54u32 * pixel[0] as u32 + 183u32 * pixel[1] as u32 + 19u32 * pixel[2] as u32;
            ((weighted + 128) >> 8) as u8
        })
        .collect()
}

fn analyze_transition(
    previous: &[u8],
    current: &[u8],
    from_frame: u32,
    to_frame: u32,
) -> TransitionMetrics {
    assert_eq!(previous.len(), current.len());
    let mut deltas: Vec<u8> = previous
        .iter()
        .zip(current)
        .map(|(&before, &after)| before.abs_diff(after))
        .collect();
    let sum: u64 = deltas.iter().map(|&delta| delta as u64).sum();
    let noticeable_count = deltas
        .iter()
        .filter(|&&delta| delta >= NOTICEABLE_DELTA_8BIT)
        .count();
    deltas.sort_unstable();
    let sample_count = deltas.len().max(1);
    TransitionMetrics {
        from_frame,
        to_frame,
        mean_abs_luma_delta_8bit: sum as f64 / sample_count as f64,
        p95_abs_luma_delta_8bit: percentile(&deltas, 0.95),
        p99_abs_luma_delta_8bit: percentile(&deltas, 0.99),
        max_abs_luma_delta_8bit: deltas.last().copied().unwrap_or_default(),
        noticeable_pixel_ratio: noticeable_count as f64 / sample_count as f64,
    }
}

fn percentile(sorted: &[u8], percentile: f64) -> u8 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() as f64 * percentile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[index]
}

fn aggregate_metrics(transitions: &[TransitionMetrics]) -> AggregateMetrics {
    let count = transitions.len().max(1) as f64;
    AggregateMetrics {
        mean_abs_luma_delta_8bit: transitions
            .iter()
            .map(|metric| metric.mean_abs_luma_delta_8bit)
            .sum::<f64>()
            / count,
        mean_p95_abs_luma_delta_8bit: transitions
            .iter()
            .map(|metric| metric.p95_abs_luma_delta_8bit as f64)
            .sum::<f64>()
            / count,
        mean_p99_abs_luma_delta_8bit: transitions
            .iter()
            .map(|metric| metric.p99_abs_luma_delta_8bit as f64)
            .sum::<f64>()
            / count,
        max_abs_luma_delta_8bit: transitions
            .iter()
            .map(|metric| metric.max_abs_luma_delta_8bit)
            .max()
            .unwrap_or_default(),
        mean_noticeable_pixel_ratio: transitions
            .iter()
            .map(|metric| metric.noticeable_pixel_ratio)
            .sum::<f64>()
            / count,
        max_transition_mean_abs_luma_delta_8bit: transitions
            .iter()
            .map(|metric| metric.mean_abs_luma_delta_8bit)
            .fold(0.0, f64::max),
    }
}

#[cfg(test)]
mod tests {
    use super::{analyze_transition, rgba_to_luma};

    #[test]
    fn identical_frames_have_zero_temporal_delta() {
        let luma = rgba_to_luma(&[10, 20, 30, 255, 200, 100, 50, 255]);
        let metrics = analyze_transition(&luma, &luma, 0, 1);
        assert_eq!(metrics.mean_abs_luma_delta_8bit, 0.0);
        assert_eq!(metrics.p99_abs_luma_delta_8bit, 0);
        assert_eq!(metrics.noticeable_pixel_ratio, 0.0);
    }

    #[test]
    fn transition_metrics_report_tail_and_noticeable_pixels() {
        let before = vec![0; 100];
        let mut after = before.clone();
        after[98] = 8;
        after[99] = 100;
        let metrics = analyze_transition(&before, &after, 4, 5);
        assert_eq!(metrics.p95_abs_luma_delta_8bit, 0);
        assert_eq!(metrics.p99_abs_luma_delta_8bit, 8);
        assert_eq!(metrics.max_abs_luma_delta_8bit, 100);
        assert_eq!(metrics.noticeable_pixel_ratio, 0.02);
    }
}
