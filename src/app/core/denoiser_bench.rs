use crate::DenoiserBenchOptions;
use anyhow::{Context, Result};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

const REPORT_VERSION: u32 = 3;
const NOTICEABLE_DELTA_8BIT: u8 = 8;
pub(super) const CAMERA_STRAFE_PER_FRAME_WORLD: f32 = 0.003;
pub(super) const CAMERA_FORWARD_PER_FRAME_WORLD: f32 = 0.001;
pub(super) const CAMERA_YAW_PER_FRAME_RADIANS: f32 = 0.0025;

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
    mean_frame_spatial_gradient_8bit: f64,
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
    fresh_samples: bool,
    camera_motion: bool,
    camera_strafe_per_frame_world: f32,
    camera_forward_per_frame_world: f32,
    camera_yaw_per_frame_radians: f32,
    capture_seconds: f64,
    aggregate: AggregateMetrics,
    keyframe_paths: &'a [String],
    transitions: &'a [TransitionMetrics],
}

struct MotionKeyframe {
    frame: u32,
    label: &'static str,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

pub(super) struct DenoiserBench {
    options: DenoiserBenchOptions,
    presented_frames: u32,
    captured_frames: u32,
    width: u32,
    height: u32,
    previous_luma: Option<Vec<u8>>,
    luma_sum: Vec<u32>,
    transitions: Vec<TransitionMetrics>,
    motion_keyframes: Vec<MotionKeyframe>,
    keyframe_paths: Vec<String>,
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
            luma_sum: Vec::new(),
            transitions: Vec::new(),
            motion_keyframes: Vec::new(),
            keyframe_paths: Vec::new(),
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

    pub(super) fn camera_motion_frame(&self) -> Option<(u32, bool)> {
        (self.options.camera_motion && self.should_capture()).then_some((
            self.captured_frames,
            self.captured_frames + 1 == self.options.capture_frames,
        ))
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

        if self.options.camera_motion {
            if let Some(label) = keyframe_label(self.captured_frames, self.options.capture_frames) {
                self.motion_keyframes.push(MotionKeyframe {
                    frame: self.captured_frames,
                    label,
                    width,
                    height,
                    rgba: rgba.to_vec(),
                });
            }
        }

        let current_luma = rgba_to_luma(rgba);
        if self.luma_sum.is_empty() {
            self.luma_sum.resize(current_luma.len(), 0);
        }
        for (sum, &luma) in self.luma_sum.iter_mut().zip(&current_luma) {
            *sum += u32::from(luma);
        }
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

    fn write_report(&mut self) -> Result<()> {
        self.keyframe_paths = self
            .motion_keyframes
            .iter()
            .map(|keyframe| {
                let path = self.write_keyframe(keyframe)?;
                Ok(path.display().to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        let aggregate = aggregate_metrics(
            &self.transitions,
            mean_frame_spatial_gradient(
                &self.luma_sum,
                self.width,
                self.height,
                self.captured_frames,
            ),
        );
        let report = DenoiserBenchReport {
            version: REPORT_VERSION,
            width: self.width,
            height: self.height,
            warmup_frames: self.options.warmup_frames,
            captured_frames: self.captured_frames,
            transition_count: self.transitions.len(),
            noticeable_delta_threshold_8bit: NOTICEABLE_DELTA_8BIT,
            fresh_samples: false,
            camera_motion: self.options.camera_motion,
            camera_strafe_per_frame_world: if self.options.camera_motion {
                CAMERA_STRAFE_PER_FRAME_WORLD
            } else {
                0.0
            },
            camera_forward_per_frame_world: if self.options.camera_motion {
                CAMERA_FORWARD_PER_FRAME_WORLD
            } else {
                0.0
            },
            camera_yaw_per_frame_radians: if self.options.camera_motion {
                CAMERA_YAW_PER_FRAME_RADIANS
            } else {
                0.0
            },
            capture_seconds: self
                .capture_started
                .map(|started| started.elapsed().as_secs_f64())
                .unwrap_or_default(),
            aggregate,
            keyframe_paths: &self.keyframe_paths,
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
            "[DENOISER_BENCH] complete mode=raw camera_motion={} transitions={} mean_delta={:.4} p95_mean={:.4} p99_mean={:.4} noticeable_ratio={:.6} keyframes={} report={}",
            self.options.camera_motion,
            self.transitions.len(),
            report.aggregate.mean_abs_luma_delta_8bit,
            report.aggregate.mean_p95_abs_luma_delta_8bit,
            report.aggregate.mean_p99_abs_luma_delta_8bit,
            report.aggregate.mean_noticeable_pixel_ratio,
            self.keyframe_paths.len(),
            path.display()
        );
        Ok(())
    }

    fn write_keyframe(&self, keyframe: &MotionKeyframe) -> Result<PathBuf> {
        let report_path = Path::new(&self.options.report_path);
        let report_stem = report_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("denoiser-bench");
        let path =
            report_path.with_file_name(format!("{report_stem}.motion-{}.png", keyframe.label));
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| {
                format!("create benchmark keyframe directory {}", parent.display())
            })?;
        }
        let image =
            image::RgbaImage::from_raw(keyframe.width, keyframe.height, keyframe.rgba.clone())
                .context("construct benchmark keyframe image")?;
        image
            .save(&path)
            .with_context(|| format!("write benchmark keyframe {}", path.display()))?;
        log::info!(
            "[DENOISER_BENCH] retained motion keyframe frame={} label={} path={}",
            keyframe.frame,
            keyframe.label,
            path.display()
        );
        Ok(path)
    }
}

fn keyframe_label(frame: u32, frame_count: u32) -> Option<&'static str> {
    let last = frame_count.saturating_sub(1);
    let one_third = last / 3;
    let two_thirds = last.saturating_mul(2) / 3;
    match frame {
        0 => Some("start"),
        frame if frame == one_third => Some("one-third"),
        frame if frame == two_thirds => Some("two-thirds"),
        frame if frame == last => Some("end"),
        _ => None,
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

fn mean_frame_spatial_gradient(luma_sum: &[u32], width: u32, height: u32, frame_count: u32) -> f64 {
    if width == 0 || height == 0 || frame_count == 0 {
        return 0.0;
    }
    let width = width as usize;
    let height = height as usize;
    assert_eq!(luma_sum.len(), width * height);

    let mut gradient_sum = 0u64;
    let mut edge_count = 0u64;
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if x + 1 < width {
                gradient_sum += u64::from(luma_sum[index].abs_diff(luma_sum[index + 1]));
                edge_count += 1;
            }
            if y + 1 < height {
                gradient_sum += u64::from(luma_sum[index].abs_diff(luma_sum[index + width]));
                edge_count += 1;
            }
        }
    }
    gradient_sum as f64 / edge_count.max(1) as f64 / f64::from(frame_count)
}

fn aggregate_metrics(
    transitions: &[TransitionMetrics],
    mean_frame_spatial_gradient_8bit: f64,
) -> AggregateMetrics {
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
        mean_frame_spatial_gradient_8bit,
    }
}

#[cfg(test)]
mod tests {
    use super::{analyze_transition, keyframe_label, mean_frame_spatial_gradient, rgba_to_luma};

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

    #[test]
    fn spatial_gradient_uses_the_mean_frame() {
        let two_frame_luma_sum = vec![0, 20, 40, 60];
        let gradient = mean_frame_spatial_gradient(&two_frame_luma_sum, 2, 2, 2);
        assert_eq!(gradient, 15.0);
    }

    #[test]
    fn motion_keyframes_cover_sequence_endpoints_and_thirds() {
        let labels: Vec<_> = (0..64)
            .filter_map(|frame| keyframe_label(frame, 64).map(|label| (frame, label)))
            .collect();
        assert_eq!(
            labels,
            vec![
                (0, "start"),
                (21, "one-third"),
                (42, "two-thirds"),
                (63, "end"),
            ]
        );
    }
}
