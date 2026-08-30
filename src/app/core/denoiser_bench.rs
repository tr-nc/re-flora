use crate::{CameraDenoiserOptions, CameraMotion, DenoiserCaptureOptions, FoliageDenoiserOptions};
use anyhow::{Context, Result};
use serde::Serialize;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

const REPORT_VERSION: u32 = 5;
const NOTICEABLE_DELTA_8BIT: u8 = 8;
const FOLIAGE_SHADOW_BENCH_FRAME_SECONDS: f32 = 1.0 / 60.0;
const FOLIAGE_STRUCTURE_SAMPLE_SCALE: u32 = 2;
pub(super) const CAMERA_STRAFE_PER_FRAME_WORLD: f32 = 0.003;
pub(super) const CAMERA_FORWARD_PER_FRAME_WORLD: f32 = 0.001;
pub(super) const CAMERA_YAW_PER_FRAME_RADIANS: f32 = 0.0025;

#[derive(Clone, Debug, Serialize)]
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
    scene: &'static str,
    source_width: u32,
    source_height: u32,
    analysis_x: u32,
    analysis_y: u32,
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
    fixed_animation_step_seconds: f32,
    capture_seconds: f64,
    aggregate: AggregateMetrics,
    luma_sequence_path: &'a str,
    luma_frame_bytes: usize,
    structure_analysis_x: u32,
    structure_analysis_y: u32,
    structure_analysis_width: u32,
    structure_analysis_height: u32,
    structure_sample_scale: u32,
    structure_sample_width: u32,
    structure_sample_height: u32,
    structure_luma_sequence_path: &'a str,
    structure_luma_frame_bytes: usize,
    keyframe_paths: &'a [String],
    transitions: &'a [TransitionMetrics],
}

struct CapturedKeyframe {
    label: &'static str,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AnalysisRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

pub(super) struct DenoiserBench {
    capture: DenoiserCaptureOptions,
    mode: DenoiserMode,
    presented_frames: u32,
    captured_frames: u32,
    source_width: u32,
    source_height: u32,
    analysis_region: Option<AnalysisRegion>,
    structure_analysis_region: Option<AnalysisRegion>,
    previous_luma: Option<Vec<u8>>,
    luma_sum: Vec<u32>,
    captured_luma: Vec<u8>,
    captured_structure_luma: Vec<u8>,
    transitions: Vec<TransitionMetrics>,
    keyframes: Vec<CapturedKeyframe>,
    keyframe_paths: Vec<String>,
    luma_sequence_path: String,
    structure_luma_sequence_path: String,
    capture_started: Option<Instant>,
}

struct PreparedDenoiserFrame {
    delta: DenoiserFrameDelta,
    report_publication: Option<ReportPublication>,
    complete: bool,
}

impl PreparedDenoiserFrame {
    #[cfg(test)]
    fn report_publication_mut(&mut self) -> Option<&mut ReportPublication> {
        self.report_publication.as_mut()
    }
}

struct DenoiserFrameDelta {
    source_width: u32,
    source_height: u32,
    analysis_region: AnalysisRegion,
    structure_analysis_region: Option<AnalysisRegion>,
    current_luma: Vec<u8>,
    structure_luma: Vec<u8>,
    transition: Option<TransitionMetrics>,
    keyframe: Option<CapturedKeyframe>,
    capture_started: Option<Instant>,
}

struct ReportPublication {
    staging_directory: Option<tempfile::TempDir>,
    staged_artifacts: Vec<StagedDenoiserArtifact>,
    final_artifact_directory: PathBuf,
    staged_report: Option<tempfile::NamedTempFile>,
    staged_report_fingerprint: FileFingerprint,
    report_path: PathBuf,
    published_paths: PublishedDenoiserPaths,
}

impl ReportPublication {
    #[cfg(test)]
    fn first_staged_artifact_path(&self) -> &Path {
        &self
            .staged_artifacts
            .first()
            .expect("foliage report must stage at least one artifact")
            .staged_path
    }
}

struct StagedDenoiserArtifact {
    staged_path: PathBuf,
    fingerprint: FileFingerprint,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileFingerprint {
    byte_len: u64,
    hash: u64,
}

struct PublishedDenoiserPaths {
    luma_sequence_path: String,
    structure_luma_sequence_path: String,
    keyframe_paths: Vec<String>,
}

#[derive(Debug)]
pub(super) struct DenoiserFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DenoiserCaptureStep {
    Skip,
    Record { frame: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CameraFrameMotion {
    Fixed,
    Scripted { capture_frame: u32, is_last: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FixedVisualFrame {
    pub(super) frame_delta_seconds: f32,
    pub(super) visual_time_seconds: f32,
}

#[derive(Debug)]
pub(super) struct DenoiserFramePermit {
    presented_frame: u32,
    captured_frame: u32,
}

#[derive(Debug)]
pub(super) enum DenoiserFrameCommand {
    Inactive,
    Camera(CameraDenoiserCommand),
    Foliage(FoliageDenoiserCommand),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CameraDenoiserPresentation {
    Fixed {
        capture: DenoiserCaptureStep,
    },
    Scripted {
        capture: DenoiserCaptureStep,
        capture_frame: u32,
        is_last: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FoliageDenoiserPresentation {
    pub(super) capture: DenoiserCaptureStep,
    pub(super) timeline: FixedVisualFrame,
}

#[derive(Debug)]
pub(super) struct CameraDenoiserCommand {
    presentation: CameraDenoiserPresentation,
    permit: DenoiserFramePermit,
}

impl CameraDenoiserCommand {
    pub(super) fn presentation(&self) -> CameraDenoiserPresentation {
        self.presentation
    }

    pub(super) fn into_run(self) -> DenoiserFrameRun {
        DenoiserFrameRun::Camera(CameraDenoiserRun {
            capture: match self.presentation {
                CameraDenoiserPresentation::Fixed { capture }
                | CameraDenoiserPresentation::Scripted { capture, .. } => capture,
            },
            permit: self.permit,
        })
    }
}

#[derive(Debug)]
pub(super) struct FoliageDenoiserCommand {
    presentation: FoliageDenoiserPresentation,
    permit: DenoiserFramePermit,
}

impl FoliageDenoiserCommand {
    pub(super) fn presentation(&self) -> FoliageDenoiserPresentation {
        self.presentation
    }

    pub(super) fn into_run(self) -> DenoiserFrameRun {
        DenoiserFrameRun::Foliage(FoliageDenoiserRun {
            capture: self.presentation.capture,
            permit: self.permit,
        })
    }
}

#[derive(Debug)]
pub(super) struct CameraDenoiserRun {
    capture: DenoiserCaptureStep,
    permit: DenoiserFramePermit,
}

#[derive(Debug)]
pub(super) struct FoliageDenoiserRun {
    capture: DenoiserCaptureStep,
    permit: DenoiserFramePermit,
}

#[derive(Debug)]
pub(super) enum DenoiserFrameRun {
    Inactive,
    Camera(CameraDenoiserRun),
    Foliage(FoliageDenoiserRun),
}

impl DenoiserFrameCommand {
    pub(super) fn into_run(self) -> DenoiserFrameRun {
        match self {
            Self::Inactive => DenoiserFrameRun::Inactive,
            Self::Camera(command) => command.into_run(),
            Self::Foliage(command) => command.into_run(),
        }
    }
}

#[derive(Debug)]
pub(super) enum DenoiserReadbackOutcome {
    NotRequested,
    Frame(DenoiserFrame),
    Failed(anyhow::Error),
}

#[derive(Debug)]
pub(super) enum DenoiserFrameCompletion {
    Inactive(DenoiserReadbackOutcome),
    Camera {
        capture: DenoiserCaptureStep,
        permit: DenoiserFramePermit,
        readback: DenoiserReadbackOutcome,
    },
    Foliage {
        capture: DenoiserCaptureStep,
        permit: DenoiserFramePermit,
        readback: DenoiserReadbackOutcome,
    },
}

impl DenoiserFrameRun {
    pub(super) fn complete(self, readback: DenoiserReadbackOutcome) -> DenoiserFrameCompletion {
        match self {
            Self::Inactive => DenoiserFrameCompletion::Inactive(readback),
            Self::Camera(CameraDenoiserRun { capture, permit }) => {
                DenoiserFrameCompletion::Camera {
                    capture,
                    permit,
                    readback,
                }
            }
            Self::Foliage(FoliageDenoiserRun { capture, permit }) => {
                DenoiserFrameCompletion::Foliage {
                    capture,
                    permit,
                    readback,
                }
            }
        }
    }
}

impl DenoiserFrame {
    pub(super) fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DenoiserMode {
    Camera(CameraMotion),
    FoliageShadow,
}

impl DenoiserMode {
    fn label(self) -> &'static str {
        match self {
            Self::Camera(_) => "camera-snapshot",
            Self::FoliageShadow => "foliage-shadow",
        }
    }

    fn is_foliage_shadow(self) -> bool {
        matches!(self, Self::FoliageShadow)
    }

    fn has_scripted_camera_motion(self) -> bool {
        matches!(self, Self::Camera(CameraMotion::Scripted))
    }
}

impl DenoiserBench {
    pub(super) fn new_camera(options: CameraDenoiserOptions) -> Self {
        Self::new(options.capture, DenoiserMode::Camera(options.camera_motion))
    }

    pub(super) fn new_foliage(options: FoliageDenoiserOptions) -> Self {
        Self::new(options.capture, DenoiserMode::FoliageShadow)
    }

    fn new(capture: DenoiserCaptureOptions, mode: DenoiserMode) -> Self {
        Self {
            capture,
            mode,
            presented_frames: 0,
            captured_frames: 0,
            source_width: 0,
            source_height: 0,
            analysis_region: None,
            structure_analysis_region: None,
            previous_luma: None,
            luma_sum: Vec::new(),
            captured_luma: Vec::new(),
            captured_structure_luma: Vec::new(),
            transitions: Vec::new(),
            keyframes: Vec::new(),
            keyframe_paths: Vec::new(),
            luma_sequence_path: String::new(),
            structure_luma_sequence_path: String::new(),
            capture_started: None,
        }
    }

    pub(super) fn should_capture(&self) -> bool {
        self.presented_frames >= self.capture.warmup_frames
            && self.captured_frames < self.capture.capture_frames
    }

    pub(super) fn mark_frame_presented(&mut self) {
        self.presented_frames += 1;
    }

    pub(super) fn fixed_frame_delta_seconds(&self) -> Option<f32> {
        self.mode
            .is_foliage_shadow()
            .then_some(FOLIAGE_SHADOW_BENCH_FRAME_SECONDS)
    }

    pub(super) fn camera_motion_frame(&self) -> Option<(u32, bool)> {
        (self.mode.has_scripted_camera_motion() && self.should_capture()).then_some((
            self.captured_frames,
            self.captured_frames + 1 == self.capture.capture_frames,
        ))
    }

    fn capture_step(&self) -> DenoiserCaptureStep {
        if self.should_capture() {
            DenoiserCaptureStep::Record {
                frame: self.captured_frames,
            }
        } else {
            DenoiserCaptureStep::Skip
        }
    }

    fn frame_permit(&self) -> DenoiserFramePermit {
        DenoiserFramePermit {
            presented_frame: self.presented_frames,
            captured_frame: self.captured_frames,
        }
    }

    pub(super) fn begin_camera_frame(&self) -> CameraDenoiserCommand {
        debug_assert!(matches!(self.mode, DenoiserMode::Camera(_)));
        let motion = self.camera_motion_frame().map_or(
            CameraFrameMotion::Fixed,
            |(capture_frame, is_last)| CameraFrameMotion::Scripted {
                capture_frame,
                is_last,
            },
        );
        let capture = self.capture_step();
        let presentation = match motion {
            CameraFrameMotion::Fixed => CameraDenoiserPresentation::Fixed { capture },
            CameraFrameMotion::Scripted {
                capture_frame,
                is_last,
            } => CameraDenoiserPresentation::Scripted {
                capture,
                capture_frame,
                is_last,
            },
        };
        CameraDenoiserCommand {
            presentation,
            permit: self.frame_permit(),
        }
    }

    pub(super) fn begin_foliage_frame(&self) -> FoliageDenoiserCommand {
        debug_assert!(self.mode.is_foliage_shadow());
        let frame_delta_seconds = FOLIAGE_SHADOW_BENCH_FRAME_SECONDS;
        FoliageDenoiserCommand {
            presentation: FoliageDenoiserPresentation {
                capture: self.capture_step(),
                timeline: FixedVisualFrame {
                    frame_delta_seconds,
                    visual_time_seconds: self.presented_frames as f32 * frame_delta_seconds,
                },
            },
            permit: self.frame_permit(),
        }
    }

    pub(super) fn finish_camera_frame(
        &mut self,
        completion: DenoiserFrameCompletion,
    ) -> Result<bool> {
        let DenoiserFrameCompletion::Camera {
            capture,
            permit,
            readback,
        } = completion
        else {
            anyhow::bail!("denoiser frame completion does not belong to the camera owner")
        };
        debug_assert!(matches!(self.mode, DenoiserMode::Camera(_)));
        self.finish_frame(capture, permit, readback)
    }

    pub(super) fn finish_foliage_frame(
        &mut self,
        completion: DenoiserFrameCompletion,
    ) -> Result<bool> {
        let DenoiserFrameCompletion::Foliage {
            capture,
            permit,
            readback,
            ..
        } = completion
        else {
            anyhow::bail!("denoiser frame completion does not belong to the foliage owner")
        };
        debug_assert!(self.mode.is_foliage_shadow());
        self.finish_frame(capture, permit, readback)
    }

    fn finish_frame(
        &mut self,
        capture: DenoiserCaptureStep,
        permit: DenoiserFramePermit,
        readback: DenoiserReadbackOutcome,
    ) -> Result<bool> {
        anyhow::ensure!(
            (self.presented_frames, self.captured_frames)
                == (permit.presented_frame, permit.captured_frame),
            "stale denoiser frame completion"
        );

        let complete = match (capture, readback) {
            (DenoiserCaptureStep::Skip, DenoiserReadbackOutcome::NotRequested) => false,
            (DenoiserCaptureStep::Record { .. }, DenoiserReadbackOutcome::Frame(frame)) => {
                self.record_completed_frame(frame)?
            }
            (_, DenoiserReadbackOutcome::Failed(error)) => return Err(error),
            (DenoiserCaptureStep::Skip, DenoiserReadbackOutcome::Frame(_)) => {
                anyhow::bail!("uncalled denoiser capture produced a frame")
            }
            (DenoiserCaptureStep::Record { .. }, DenoiserReadbackOutcome::NotRequested) => {
                anyhow::bail!("requested denoiser capture produced no frame")
            }
        };
        self.mark_frame_presented();
        Ok(complete)
    }

    pub(super) fn record_frame(&mut self, width: u32, height: u32, rgba: &[u8]) -> Result<bool> {
        let prepared = self.prepare_frame(width, height, rgba)?;
        self.publish_and_commit(prepared)
    }

    fn prepare_frame(&self, width: u32, height: u32, rgba: &[u8]) -> Result<PreparedDenoiserFrame> {
        let expected_len = width as usize * height as usize * 4;
        anyhow::ensure!(
            rgba.len() == expected_len,
            "benchmark frame is {} bytes, expected {} for {}x{} RGBA",
            rgba.len(),
            expected_len,
            width,
            height
        );
        let analysis_region = analysis_region(self.mode, width, height);
        let structure_analysis_region = foliage_structure_analysis_region(self.mode, width, height);
        if self.captured_frames != 0 {
            anyhow::ensure!(
                (width, height) == (self.source_width, self.source_height),
                "benchmark extent changed from {}x{} to {}x{}",
                self.source_width,
                self.source_height,
                width,
                height
            );
            anyhow::ensure!(
                self.analysis_region == Some(analysis_region),
                "benchmark analysis region changed during capture"
            );
            anyhow::ensure!(
                self.structure_analysis_region == structure_analysis_region,
                "benchmark structure analysis region changed during capture"
            );
        }

        let keyframe = if self.mode.has_scripted_camera_motion() || self.mode.is_foliage_shadow() {
            keyframe_label(self.captured_frames, self.capture.capture_frames).map(|label| {
                CapturedKeyframe {
                    label,
                    width,
                    height,
                    rgba: rgba.to_vec(),
                }
            })
        } else {
            None
        };

        let current_luma = rgba_region_to_luma(rgba, width, analysis_region);
        let structure_luma = if self.mode.is_foliage_shadow() {
            let structure_region = structure_analysis_region
                .context("foliage shadow benchmark requires a structure analysis region")?;
            let structure_luma = rgba_region_to_luma(rgba, width, structure_region);
            box_downsample_luma(
                &structure_luma,
                structure_region.width,
                structure_region.height,
                FOLIAGE_STRUCTURE_SAMPLE_SCALE,
            )
        } else {
            Vec::new()
        };
        let transition = self.previous_luma.as_ref().map(|previous_luma| {
            analyze_transition(
                previous_luma,
                &current_luma,
                self.captured_frames - 1,
                self.captured_frames,
            )
        });
        let delta = DenoiserFrameDelta {
            source_width: width,
            source_height: height,
            analysis_region,
            structure_analysis_region,
            current_luma,
            structure_luma,
            transition,
            keyframe,
            capture_started: (self.captured_frames == 0).then(Instant::now),
        };
        let complete = self.captured_frames + 1 == self.capture.capture_frames;
        let report_publication = complete
            .then(|| self.stage_report_publication(&delta))
            .transpose()?;
        Ok(PreparedDenoiserFrame {
            delta,
            report_publication,
            complete,
        })
    }

    pub(super) fn record_completed_frame(&mut self, frame: DenoiserFrame) -> Result<bool> {
        self.record_frame(frame.width, frame.height, &frame.rgba)
    }

    fn publish_and_commit(&mut self, mut prepared: PreparedDenoiserFrame) -> Result<bool> {
        let published_paths = prepared
            .report_publication
            .take()
            .map(ReportPublication::publish)
            .transpose()?;
        let complete = prepared.complete;
        self.commit_prepared_frame(prepared.delta, published_paths);
        Ok(complete)
    }

    fn commit_prepared_frame(
        &mut self,
        delta: DenoiserFrameDelta,
        published_paths: Option<PublishedDenoiserPaths>,
    ) {
        if self.captured_frames == 0 {
            self.source_width = delta.source_width;
            self.source_height = delta.source_height;
            self.analysis_region = Some(delta.analysis_region);
            self.structure_analysis_region = delta.structure_analysis_region;
            self.capture_started = delta.capture_started;
            log::info!(
                "[DENOISER_BENCH] warmup complete scene={} after {} presented frames; capturing {} frames at {}x{} analysis={}x{}+{},{}",
                self.mode.label(),
                self.presented_frames,
                self.capture.capture_frames,
                delta.source_width,
                delta.source_height,
                delta.analysis_region.width,
                delta.analysis_region.height,
                delta.analysis_region.x,
                delta.analysis_region.y,
            );
        }
        if self.mode.is_foliage_shadow() {
            self.captured_luma.extend_from_slice(&delta.current_luma);
            self.captured_structure_luma
                .extend_from_slice(&delta.structure_luma);
        }
        if self.luma_sum.is_empty() {
            self.luma_sum.resize(delta.current_luma.len(), 0);
        }
        for (sum, &luma) in self.luma_sum.iter_mut().zip(&delta.current_luma) {
            *sum += u32::from(luma);
        }
        if let Some(transition) = delta.transition {
            self.transitions.push(transition);
        }
        if let Some(keyframe) = delta.keyframe {
            self.keyframes.push(keyframe);
        }
        self.previous_luma = Some(delta.current_luma);
        self.captured_frames += 1;

        if let Some(paths) = published_paths {
            self.luma_sequence_path = paths.luma_sequence_path;
            self.structure_luma_sequence_path = paths.structure_luma_sequence_path;
            self.keyframe_paths = paths.keyframe_paths;
            let analysis_region = self
                .analysis_region
                .expect("published denoiser frame must retain its analysis region");
            let aggregate = aggregate_metrics(
                &self.transitions,
                mean_frame_spatial_gradient(
                    &self.luma_sum,
                    analysis_region.width,
                    analysis_region.height,
                    self.captured_frames,
                ),
            );
            log::info!(
                "[DENOISER_BENCH] complete mode=raw scene={} camera_motion={} transitions={} mean_delta={:.4} p95_mean={:.4} p99_mean={:.4} noticeable_ratio={:.6} keyframes={} report={}",
                self.mode.label(),
                self.mode.has_scripted_camera_motion(),
                self.transitions.len(),
                aggregate.mean_abs_luma_delta_8bit,
                aggregate.mean_p95_abs_luma_delta_8bit,
                aggregate.mean_p99_abs_luma_delta_8bit,
                aggregate.mean_noticeable_pixel_ratio,
                self.keyframe_paths.len(),
                self.capture.report_path,
            );
        }
    }

    fn stage_report_publication(&self, delta: &DenoiserFrameDelta) -> Result<ReportPublication> {
        let report_path = PathBuf::from(&self.capture.report_path);
        let report_parent = report_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(report_parent).with_context(|| {
            format!(
                "create benchmark report directory {}",
                report_parent.display()
            )
        })?;
        let staging_directory = tempfile::Builder::new()
            .prefix(".denoiser-stage-")
            .tempdir_in(report_parent)
            .context("create denoiser report staging directory")?;
        let generation = staging_directory
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("generation")
            .trim_start_matches(".denoiser-stage-");
        let report_stem = report_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("denoiser-bench");
        let final_artifact_directory =
            report_path.with_file_name(format!("{report_stem}.artifacts-{generation}"));
        anyhow::ensure!(
            !final_artifact_directory.exists(),
            "denoiser artifact generation already exists: {}",
            final_artifact_directory.display()
        );

        let mut staged_artifacts = Vec::new();
        let mut luma_sequence_path = String::new();
        let mut structure_luma_sequence_path = String::new();
        if self.mode.is_foliage_shadow() {
            let staged_path = staging_directory.path().join("receiver-luma-u8.bin");
            write_chunks(
                &staged_path,
                [&self.captured_luma[..], &delta.current_luma[..]],
            )?;
            luma_sequence_path = final_artifact_directory
                .join("receiver-luma-u8.bin")
                .display()
                .to_string();
            staged_artifacts.push(StagedDenoiserArtifact::capture(staged_path)?);

            let staged_path = staging_directory.path().join("structure-luma-u8.bin");
            write_chunks(
                &staged_path,
                [&self.captured_structure_luma[..], &delta.structure_luma[..]],
            )?;
            structure_luma_sequence_path = final_artifact_directory
                .join("structure-luma-u8.bin")
                .display()
                .to_string();
            staged_artifacts.push(StagedDenoiserArtifact::capture(staged_path)?);
        }

        let mut keyframe_paths = Vec::new();
        for keyframe in self.keyframes.iter().chain(delta.keyframe.iter()) {
            let file_name = format!("frame-{}.png", keyframe.label);
            let staged_path = staging_directory.path().join(&file_name);
            image::save_buffer(
                &staged_path,
                &keyframe.rgba,
                keyframe.width,
                keyframe.height,
                image::ColorType::Rgba8,
            )
            .with_context(|| format!("stage benchmark keyframe {}", staged_path.display()))?;
            fs::File::open(&staged_path)
                .and_then(|file| file.sync_all())
                .with_context(|| format!("sync benchmark keyframe {}", staged_path.display()))?;
            keyframe_paths.push(
                final_artifact_directory
                    .join(file_name)
                    .display()
                    .to_string(),
            );
            staged_artifacts.push(StagedDenoiserArtifact::capture(staged_path)?);
        }

        let captured_frames = self.captured_frames + 1;
        let mut prospective_luma_sum = if self.luma_sum.is_empty() {
            vec![0; delta.current_luma.len()]
        } else {
            self.luma_sum.clone()
        };
        for (sum, &luma) in prospective_luma_sum.iter_mut().zip(&delta.current_luma) {
            *sum += u32::from(luma);
        }
        let mut prospective_transitions = self.transitions.clone();
        if let Some(transition) = &delta.transition {
            prospective_transitions.push(transition.clone());
        }
        let aggregate = aggregate_metrics(
            &prospective_transitions,
            mean_frame_spatial_gradient(
                &prospective_luma_sum,
                delta.analysis_region.width,
                delta.analysis_region.height,
                captured_frames,
            ),
        );
        let structure_region = delta.structure_analysis_region.unwrap_or(AnalysisRegion {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        let structure_sample_width = structure_region.width / FOLIAGE_STRUCTURE_SAMPLE_SCALE;
        let structure_sample_height = structure_region.height / FOLIAGE_STRUCTURE_SAMPLE_SCALE;
        let report = DenoiserBenchReport {
            version: REPORT_VERSION,
            scene: self.mode.label(),
            source_width: delta.source_width,
            source_height: delta.source_height,
            analysis_x: delta.analysis_region.x,
            analysis_y: delta.analysis_region.y,
            width: delta.analysis_region.width,
            height: delta.analysis_region.height,
            warmup_frames: self.capture.warmup_frames,
            captured_frames,
            transition_count: prospective_transitions.len(),
            noticeable_delta_threshold_8bit: NOTICEABLE_DELTA_8BIT,
            fresh_samples: false,
            camera_motion: self.mode.has_scripted_camera_motion(),
            camera_strafe_per_frame_world: if self.mode.has_scripted_camera_motion() {
                CAMERA_STRAFE_PER_FRAME_WORLD
            } else {
                0.0
            },
            camera_forward_per_frame_world: if self.mode.has_scripted_camera_motion() {
                CAMERA_FORWARD_PER_FRAME_WORLD
            } else {
                0.0
            },
            camera_yaw_per_frame_radians: if self.mode.has_scripted_camera_motion() {
                CAMERA_YAW_PER_FRAME_RADIANS
            } else {
                0.0
            },
            fixed_animation_step_seconds: self.fixed_frame_delta_seconds().unwrap_or(0.0),
            capture_seconds: self
                .capture_started
                .or(delta.capture_started)
                .map(|started| started.elapsed().as_secs_f64())
                .unwrap_or_default(),
            aggregate,
            luma_sequence_path: &luma_sequence_path,
            luma_frame_bytes: delta.analysis_region.width as usize
                * delta.analysis_region.height as usize,
            structure_analysis_x: structure_region.x,
            structure_analysis_y: structure_region.y,
            structure_analysis_width: structure_region.width,
            structure_analysis_height: structure_region.height,
            structure_sample_scale: if self.mode.is_foliage_shadow() {
                FOLIAGE_STRUCTURE_SAMPLE_SCALE
            } else {
                0
            },
            structure_sample_width,
            structure_sample_height,
            structure_luma_sequence_path: &structure_luma_sequence_path,
            structure_luma_frame_bytes: structure_sample_width as usize
                * structure_sample_height as usize,
            keyframe_paths: &keyframe_paths,
            transitions: &prospective_transitions,
        };
        let serialized = toml::to_string_pretty(&report).context("serialize denoiser report")?;
        let mut staged_report = tempfile::Builder::new()
            .prefix(".denoiser-report-")
            .tempfile_in(report_parent)
            .context("create staged denoiser report")?;
        staged_report
            .write_all(serialized.as_bytes())
            .context("write staged denoiser report")?;
        staged_report
            .as_file_mut()
            .sync_all()
            .context("sync staged denoiser report")?;
        let staged_report_fingerprint = file_fingerprint(staged_report.path())?;

        Ok(ReportPublication {
            staging_directory: Some(staging_directory),
            staged_artifacts,
            final_artifact_directory,
            staged_report: Some(staged_report),
            staged_report_fingerprint,
            report_path,
            published_paths: PublishedDenoiserPaths {
                luma_sequence_path,
                structure_luma_sequence_path,
                keyframe_paths,
            },
        })
    }
}

impl StagedDenoiserArtifact {
    fn capture(staged_path: PathBuf) -> Result<Self> {
        Ok(Self {
            fingerprint: file_fingerprint(&staged_path)?,
            staged_path,
        })
    }
}

impl ReportPublication {
    fn validate(&self) -> Result<()> {
        for artifact in &self.staged_artifacts {
            anyhow::ensure!(
                file_fingerprint(&artifact.staged_path)? == artifact.fingerprint,
                "staged denoiser artifact changed before publication: {}",
                artifact.staged_path.display()
            );
        }
        let staged_report = self
            .staged_report
            .as_ref()
            .context("staged denoiser report was already consumed")?;
        anyhow::ensure!(
            file_fingerprint(staged_report.path())? == self.staged_report_fingerprint,
            "staged denoiser report changed before publication: {}",
            staged_report.path().display()
        );
        Ok(())
    }

    fn publish(mut self) -> Result<PublishedDenoiserPaths> {
        self.validate()?;
        let staging_directory = self
            .staging_directory
            .take()
            .context("denoiser artifact staging directory was already consumed")?;
        fs::rename(staging_directory.path(), &self.final_artifact_directory).with_context(
            || {
                format!(
                    "publish denoiser artifacts {}",
                    self.final_artifact_directory.display()
                )
            },
        )?;
        drop(staging_directory);

        let staged_report = self
            .staged_report
            .take()
            .context("staged denoiser report was already consumed")?;
        if let Err(error) = staged_report.persist(&self.report_path) {
            let publish_error = error.error;
            drop(error.file);
            fs::remove_dir_all(&self.final_artifact_directory).with_context(|| {
                format!(
                    "remove unpublished denoiser artifacts {} after report failure {publish_error}",
                    self.final_artifact_directory.display()
                )
            })?;
            return Err(anyhow::anyhow!(
                "publish denoiser report {}: {publish_error}",
                self.report_path.display()
            ));
        }
        Ok(self.published_paths)
    }
}

fn write_chunks<'a>(path: &Path, chunks: impl IntoIterator<Item = &'a [u8]>) -> Result<()> {
    let mut file = fs::File::create(path)
        .with_context(|| format!("create staged denoiser artifact {}", path.display()))?;
    for chunk in chunks {
        file.write_all(chunk)
            .with_context(|| format!("write staged denoiser artifact {}", path.display()))?;
    }
    file.sync_all()
        .with_context(|| format!("sync staged denoiser artifact {}", path.display()))
}

fn file_fingerprint(path: &Path) -> Result<FileFingerprint> {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut file = fs::File::open(path)
        .with_context(|| format!("open staged denoiser file {}", path.display()))?;
    let mut buffer = [0u8; 64 * 1024];
    let mut byte_len = 0u64;
    let mut hash = FNV_OFFSET;
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("hash staged denoiser file {}", path.display()))?;
        if read == 0 {
            break;
        }
        byte_len += read as u64;
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    Ok(FileFingerprint { byte_len, hash })
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

fn analysis_region(mode: DenoiserMode, width: u32, height: u32) -> AnalysisRegion {
    match mode {
        DenoiserMode::Camera(_) => AnalysisRegion {
            x: 0,
            y: 0,
            width,
            height,
        },
        DenoiserMode::FoliageShadow => {
            // The fixed camera places the deterministic grass patch and its canopy projection in
            // this resolution-independent rectangle. Excluding visible canopy and bare terrain
            // keeps the metric specific to the moving receiver named by the benchmark.
            let x = width.saturating_mul(3) / 10;
            let y = height.saturating_mul(3) / 10;
            AnalysisRegion {
                x,
                y,
                width: (width.saturating_mul(3) / 10).max(1),
                height: (height.saturating_mul(11) / 20).max(1),
            }
        }
    }
}

fn foliage_structure_analysis_region(
    mode: DenoiserMode,
    width: u32,
    height: u32,
) -> Option<AnalysisRegion> {
    if !mode.is_foliage_shadow() {
        return None;
    }
    // The bare terrain left of the grass receiver contains the projected crown silhouette and
    // internal openings without visible-leaf or grass shading. Keep the extent divisible by the
    // sample scale so every stored sample represents the same 4x4 screen footprint.
    let sample_scale = FOLIAGE_STRUCTURE_SAMPLE_SCALE;
    let tile_extent = sample_scale * 16;
    let rounded_down = |value: u32, multiple: u32| value / multiple * multiple;
    Some(AnalysisRegion {
        x: 0,
        y: rounded_down(height / 6, sample_scale),
        width: rounded_down(width.saturating_mul(3) / 10, tile_extent).max(tile_extent),
        height: rounded_down(height.saturating_mul(3) / 4, tile_extent).max(tile_extent),
    })
}

fn rgba_region_to_luma(rgba: &[u8], source_width: u32, region: AnalysisRegion) -> Vec<u8> {
    let row_stride = source_width as usize * 4;
    let region_row_bytes = region.width as usize * 4;
    let mut luma = Vec::with_capacity(region.width as usize * region.height as usize);
    for y in region.y..region.y + region.height {
        let row_start = y as usize * row_stride + region.x as usize * 4;
        luma.extend(rgba_to_luma(&rgba[row_start..row_start + region_row_bytes]));
    }
    luma
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

fn box_downsample_luma(luma: &[u8], width: u32, height: u32, scale: u32) -> Vec<u8> {
    assert!(scale > 0);
    assert_eq!(width % scale, 0);
    assert_eq!(height % scale, 0);
    assert_eq!(luma.len(), width as usize * height as usize);
    let output_width = width / scale;
    let output_height = height / scale;
    let sample_count = scale * scale;
    let mut output = Vec::with_capacity(output_width as usize * output_height as usize);
    for output_y in 0..output_height {
        for output_x in 0..output_width {
            let mut sum = 0u32;
            for y in 0..scale {
                let row = (output_y * scale + y) * width + output_x * scale;
                for x in 0..scale {
                    sum += u32::from(luma[(row + x) as usize]);
                }
            }
            output.push(((sum + sample_count / 2) / sample_count) as u8);
        }
    }
    output
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
    use super::{
        analysis_region, analyze_transition, box_downsample_luma,
        foliage_structure_analysis_region, keyframe_label, mean_frame_spatial_gradient,
        rgba_region_to_luma, rgba_to_luma, AnalysisRegion, DenoiserBench, DenoiserMode,
    };
    use crate::{DenoiserCaptureOptions, FoliageDenoiserOptions};

    #[test]
    fn report_publication_failure_leaves_no_partial_files_or_owner_progress() {
        static_assertions::assert_not_impl_any!(DenoiserBench: Clone, Copy);

        let output = tempfile::tempdir().unwrap();
        let report_path = output.path().join("report.toml");
        std::fs::create_dir(&report_path).unwrap();
        let mut bench = foliage_bench(&report_path);

        let error = bench
            .record_frame(64, 64, &vec![80; 64 * 64 * 4])
            .expect_err("a directory at the report path must reject atomic publication");

        assert!(error.to_string().contains("publish denoiser report"));
        assert_eq!(bench.presented_frames, 0);
        assert_eq!(bench.captured_frames, 0);
        assert!(bench.previous_luma.is_none());
        assert!(bench.luma_sum.is_empty());
        assert!(bench.captured_luma.is_empty());
        assert!(bench.captured_structure_luma.is_empty());
        assert!(bench.transitions.is_empty());
        assert!(bench.keyframes.is_empty());
        assert_eq!(directory_entries(output.path()), vec!["report.toml"]);
    }

    #[test]
    fn truncated_staged_artifact_is_rejected_and_cleaned_before_owner_commit() {
        let output = tempfile::tempdir().unwrap();
        let report_path = output.path().join("report.toml");
        let mut bench = foliage_bench(&report_path);
        let mut prepared = bench.prepare_frame(64, 64, &vec![96; 64 * 64 * 4]).unwrap();
        let staged_path = prepared
            .report_publication_mut()
            .expect("the final frame must stage a report")
            .first_staged_artifact_path()
            .to_owned();
        let original_len = std::fs::metadata(&staged_path).unwrap().len();
        std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&staged_path)
            .unwrap();
        assert_ne!(std::fs::metadata(&staged_path).unwrap().len(), original_len);

        let error = bench
            .publish_and_commit(prepared)
            .expect_err("truncated staged bytes must fail validation");

        assert!(error
            .to_string()
            .contains("staged denoiser artifact changed"));
        assert_eq!(bench.captured_frames, 0);
        assert!(bench.previous_luma.is_none());
        assert!(bench.keyframes.is_empty());
        assert!(directory_entries(output.path()).is_empty());
    }

    #[test]
    fn successful_report_manifest_publishes_one_complete_artifact_generation() {
        let output = tempfile::tempdir().unwrap();
        let report_path = output.path().join("report.toml");
        let mut bench = foliage_bench(&report_path);

        assert!(bench.record_frame(64, 64, &vec![112; 64 * 64 * 4]).unwrap());

        assert_eq!(bench.captured_frames, 1);
        assert_eq!(bench.keyframes.len(), 1);
        let serialized = std::fs::read_to_string(&report_path).unwrap();
        let report: toml::Value = toml::from_str(&serialized).unwrap();
        for key in ["luma_sequence_path", "structure_luma_sequence_path"] {
            let path = std::path::Path::new(report[key].as_str().unwrap());
            assert!(path.is_file(), "published report path is missing: {path:?}");
        }
        for path in report["keyframe_paths"].as_array().unwrap() {
            assert!(std::path::Path::new(path.as_str().unwrap()).is_file());
        }
        let entries = directory_entries(output.path());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1], "report.toml");
        assert!(entries[0].starts_with("report.artifacts-"));
    }

    fn foliage_bench(report_path: &std::path::Path) -> DenoiserBench {
        DenoiserBench::new_foliage(FoliageDenoiserOptions {
            capture: DenoiserCaptureOptions {
                report_path: report_path.display().to_string(),
                warmup_frames: 0,
                capture_frames: 1,
            },
        })
    }

    fn directory_entries(path: &std::path::Path) -> Vec<String> {
        let mut entries = std::fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }

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

    #[test]
    fn foliage_analysis_region_tracks_the_fixed_grass_receiver() {
        assert_eq!(
            analysis_region(DenoiserMode::FoliageShadow, 1920, 1080),
            AnalysisRegion {
                x: 576,
                y: 324,
                width: 576,
                height: 594,
            }
        );
    }

    #[test]
    fn foliage_structure_region_tracks_bare_terrain_and_has_uniform_samples() {
        assert_eq!(
            foliage_structure_analysis_region(DenoiserMode::FoliageShadow, 1920, 1080,),
            Some(AnalysisRegion {
                x: 0,
                y: 180,
                width: 576,
                height: 800,
            })
        );
    }

    #[test]
    fn structure_luma_downsample_is_an_exact_box_average() {
        let luma = (0..64).collect::<Vec<_>>();
        assert_eq!(box_downsample_luma(&luma, 8, 8, 4), vec![14, 18, 46, 50]);
    }

    #[test]
    fn rgba_region_conversion_excludes_pixels_outside_the_receiver_roi() {
        let rgba = [
            255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        ];
        let luma = rgba_region_to_luma(
            &rgba,
            4,
            AnalysisRegion {
                x: 1,
                y: 0,
                width: 2,
                height: 2,
            },
        );
        assert_eq!(luma, vec![0, 255, 54, 255]);
    }
}
