use crate::tracer::{LightingModeProductionLayers, LightingModeProductionReadback, Tracer};
use crate::LightingModeAcceptanceOptions;
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const ARTIFACT_MAGIC: &[u8; 8] = b"RFLMA01\0";
const ARTIFACT_SCHEMA: &str = "re-flora-lighting-mode-acceptance-v1";
const CALIBRATION_ID: &str = "r13-e2-production-v1";
const FIXTURE_ID: &str = "foliage-shadow-r13-e2-v1";
const FIXED_VISUAL_TIME_SECONDS: f32 = 0.0;
const FIXED_TIME_OF_DAY: f32 = 0.47;
const FIXED_SAMPLING_SERIAL: u32 = 0x5246_1302;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerrainLightingMode {
    Ddgi,
    PathTracingReference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RasterLightingMode {
    Ddgi,
    Legacy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EffectiveLightingControls {
    pub terrain: TerrainLightingMode,
    pub raster: RasterLightingMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum PlannedFrameValue<T> {
    Live,
    Fixed(T),
}

impl<T: Copy> PlannedFrameValue<T> {
    pub(super) const fn resolve(self, live: T) -> T {
        match self {
            Self::Live => live,
            Self::Fixed(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LightingModeAcceptanceFramePlan {
    pub visual_time_seconds: PlannedFrameValue<f32>,
    pub frame_delta_seconds: PlannedFrameValue<f32>,
    pub time_of_day: PlannedFrameValue<f32>,
    pub sampling_serial: PlannedFrameValue<u32>,
    pub dither_strength_lsb: PlannedFrameValue<f32>,
    pub path_tracing_max_bounces: PlannedFrameValue<u32>,
    pub path_tracing_ambient_light: PlannedFrameValue<[f32; 3]>,
    pub lighting_controls: PlannedFrameValue<EffectiveLightingControls>,
}

impl LightingModeAcceptanceFramePlan {
    fn live() -> Self {
        Self {
            visual_time_seconds: PlannedFrameValue::Live,
            frame_delta_seconds: PlannedFrameValue::Live,
            time_of_day: PlannedFrameValue::Live,
            sampling_serial: PlannedFrameValue::Live,
            dither_strength_lsb: PlannedFrameValue::Live,
            path_tracing_max_bounces: PlannedFrameValue::Live,
            path_tracing_ambient_light: PlannedFrameValue::Live,
            lighting_controls: PlannedFrameValue::Live,
        }
    }

    fn fixed(controls: EffectiveLightingControls) -> Self {
        Self {
            visual_time_seconds: PlannedFrameValue::Fixed(FIXED_VISUAL_TIME_SECONDS),
            frame_delta_seconds: PlannedFrameValue::Fixed(0.0),
            time_of_day: PlannedFrameValue::Fixed(FIXED_TIME_OF_DAY),
            sampling_serial: PlannedFrameValue::Fixed(FIXED_SAMPLING_SERIAL),
            dither_strength_lsb: PlannedFrameValue::Fixed(0.0),
            path_tracing_max_bounces: PlannedFrameValue::Fixed(2),
            path_tracing_ambient_light: PlannedFrameValue::Fixed([0.0; 3]),
            lighting_controls: PlannedFrameValue::Fixed(controls),
        }
    }
}

impl EffectiveLightingControls {
    pub(super) const fn new(terrain: TerrainLightingMode, raster: RasterLightingMode) -> Self {
        Self { terrain, raster }
    }

    pub(super) const fn from_gui(
        path_tracing_reference: bool,
        raster_flora_ddgi_lighting: bool,
    ) -> Self {
        Self {
            terrain: if path_tracing_reference {
                TerrainLightingMode::PathTracingReference
            } else {
                TerrainLightingMode::Ddgi
            },
            raster: if raster_flora_ddgi_lighting {
                RasterLightingMode::Ddgi
            } else {
                RasterLightingMode::Legacy
            },
        }
    }

    pub(super) const fn path_tracing_reference(self) -> bool {
        matches!(self.terrain, TerrainLightingMode::PathTracingReference)
    }

    pub(super) const fn raster_flora_ddgi_lighting(self) -> bool {
        matches!(self.raster, RasterLightingMode::Ddgi)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LightingModeAcceptancePhase {
    Inactive,
    A,
    B,
    C,
    D,
    Complete,
}

impl LightingModeAcceptancePhase {
    pub(super) const fn controls(
        self,
        gui: EffectiveLightingControls,
    ) -> EffectiveLightingControls {
        match self {
            Self::Inactive => gui,
            Self::A => {
                EffectiveLightingControls::new(TerrainLightingMode::Ddgi, RasterLightingMode::Ddgi)
            }
            Self::B => EffectiveLightingControls::new(
                TerrainLightingMode::PathTracingReference,
                RasterLightingMode::Ddgi,
            ),
            Self::C => EffectiveLightingControls::new(
                TerrainLightingMode::PathTracingReference,
                RasterLightingMode::Legacy,
            ),
            Self::D => EffectiveLightingControls::new(
                TerrainLightingMode::Ddgi,
                RasterLightingMode::Legacy,
            ),
            Self::Complete => EffectiveLightingControls::new(
                TerrainLightingMode::Ddgi,
                RasterLightingMode::Legacy,
            ),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::Inactive => "inactive",
            Self::Complete => "complete",
        }
    }

    fn frame_plan(self) -> LightingModeAcceptanceFramePlan {
        if matches!(self, Self::A | Self::B | Self::C | Self::D) {
            LightingModeAcceptanceFramePlan::fixed(self.controls(EffectiveLightingControls::new(
                TerrainLightingMode::Ddgi,
                RasterLightingMode::Ddgi,
            )))
        } else {
            LightingModeAcceptanceFramePlan::live()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LightingModeAcceptanceSceneObservation {
    pub camera_pose_bits: [u32; 6],
    pub visible_terrain_revision: u32,
    pub visual_time_bits: u32,
    pub sampling_serial: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LightingModeAcceptanceRenderObservation {
    pub render_extent: [u32; 2],
    pub screen_extent: [u32; 2],
    pub extent_generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LightingModeAcceptanceDdgiFieldObservation {
    pub serial: u64,
    pub geometry_revision: u32,
    pub radiance_revision: u32,
    pub update_epoch: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LightingModeAcceptanceDdgiObservation {
    pub published: LightingModeAcceptanceDdgiFieldObservation,
    pub source: LightingModeAcceptanceDdgiFieldObservation,
    pub spacing_voxels: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LightingModeAcceptanceLightingObservation {
    pub authored_revision: u64,
    pub local_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LightingModeAcceptanceIdentity {
    pub scene: LightingModeAcceptanceSceneObservation,
    pub render: LightingModeAcceptanceRenderObservation,
    pub ddgi: LightingModeAcceptanceDdgiObservation,
    pub lighting: LightingModeAcceptanceLightingObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LightingModeAcceptanceError {
    IdentityDrift,
    CaptureAlreadyPending,
    UnexpectedCapture,
}

pub(super) struct PendingLightingModeCapture {
    phase: LightingModeAcceptancePhase,
    identity: LightingModeAcceptanceIdentity,
    readback: LightingModeProductionReadback,
}

struct CapturedLightingModePhase {
    phase: LightingModeAcceptancePhase,
    identity: LightingModeAcceptanceIdentity,
    layers: LightingModeProductionLayers,
}

pub(super) struct LightingModeAcceptanceRuntime {
    artifact_path: Option<PathBuf>,
    phase: LightingModeAcceptancePhase,
    baseline_identity: Option<LightingModeAcceptanceIdentity>,
    phase_settled: bool,
    capture_pending: bool,
    captures: Vec<CapturedLightingModePhase>,
}

impl LightingModeAcceptanceRuntime {
    pub(super) fn new(options: Option<&LightingModeAcceptanceOptions>) -> Self {
        Self {
            artifact_path: options.map(|options| options.artifact_path.clone()),
            phase: if options.is_some() {
                LightingModeAcceptancePhase::A
            } else {
                LightingModeAcceptancePhase::Inactive
            },
            baseline_identity: None,
            phase_settled: false,
            capture_pending: false,
            captures: Vec::with_capacity(4),
        }
    }

    pub(super) fn is_active(&self) -> bool {
        !matches!(
            self.phase,
            LightingModeAcceptancePhase::Inactive | LightingModeAcceptancePhase::Complete
        )
    }

    pub(super) fn frame_plan(&self) -> LightingModeAcceptanceFramePlan {
        self.phase.frame_plan()
    }

    pub(super) fn record_if_ready(
        &mut self,
        identity: Option<LightingModeAcceptanceIdentity>,
        tracer: &Tracer,
        cmdbuf: &re_flora_vkn::CommandBuffer,
    ) -> Result<Option<PendingLightingModeCapture>> {
        let Some(identity) = identity else {
            return Ok(None);
        };
        let Some(phase) = self
            .claim_capture(identity)
            .map_err(|error| anyhow::anyhow!("lighting acceptance rejected frame: {error:?}"))?
        else {
            return Ok(None);
        };
        let readback = tracer.prepare_lighting_mode_production_readback();
        tracer.record_lighting_mode_production_readback(cmdbuf, &readback);
        Ok(Some(PendingLightingModeCapture {
            phase,
            identity,
            readback,
        }))
    }

    pub(super) fn complete(&mut self, pending: PendingLightingModeCapture) -> Result<bool> {
        let layers = pending.readback.read()?;
        self.complete_capture(pending.phase, pending.identity)
            .map_err(|error| anyhow::anyhow!("lighting acceptance rejected readback: {error:?}"))?;
        log::info!(
            "[LIGHTING_MODE_ACCEPTANCE] captured phase={} terrain_bytes={} depth_bytes={} raster_bytes={}",
            pending.phase.label(),
            layers.terrain_rgbe.len(),
            layers.terrain_depth.len(),
            layers.raster_rgba.len(),
        );
        self.captures.push(CapturedLightingModePhase {
            phase: pending.phase,
            identity: pending.identity,
            layers,
        });
        if !self.is_complete() {
            return Ok(false);
        }
        self.write_artifact()?;
        Ok(true)
    }

    pub(super) fn claim_capture(
        &mut self,
        identity: LightingModeAcceptanceIdentity,
    ) -> Result<Option<LightingModeAcceptancePhase>, LightingModeAcceptanceError> {
        if matches!(
            self.phase,
            LightingModeAcceptancePhase::Inactive | LightingModeAcceptancePhase::Complete
        ) {
            return Ok(None);
        }
        match self.baseline_identity {
            Some(baseline) if baseline != identity => {
                return Err(LightingModeAcceptanceError::IdentityDrift);
            }
            None => self.baseline_identity = Some(identity),
            Some(_) => {}
        }
        if self.capture_pending {
            return Err(LightingModeAcceptanceError::CaptureAlreadyPending);
        }
        if !std::mem::replace(&mut self.phase_settled, true) {
            return Ok(None);
        }
        self.capture_pending = true;
        Ok(Some(self.phase))
    }

    pub(super) fn complete_capture(
        &mut self,
        phase: LightingModeAcceptancePhase,
        identity: LightingModeAcceptanceIdentity,
    ) -> Result<(), LightingModeAcceptanceError> {
        if !self.capture_pending || phase != self.phase || self.baseline_identity != Some(identity)
        {
            return Err(LightingModeAcceptanceError::UnexpectedCapture);
        }
        self.capture_pending = false;
        self.phase_settled = false;
        self.phase = match self.phase {
            LightingModeAcceptancePhase::A => LightingModeAcceptancePhase::B,
            LightingModeAcceptancePhase::B => LightingModeAcceptancePhase::C,
            LightingModeAcceptancePhase::C => LightingModeAcceptancePhase::D,
            LightingModeAcceptancePhase::D => LightingModeAcceptancePhase::Complete,
            LightingModeAcceptancePhase::Inactive | LightingModeAcceptancePhase::Complete => {
                return Err(LightingModeAcceptanceError::UnexpectedCapture);
            }
        };
        Ok(())
    }

    pub(super) fn is_complete(&self) -> bool {
        self.phase == LightingModeAcceptancePhase::Complete
    }

    fn write_artifact(&self) -> Result<()> {
        let path = self
            .artifact_path
            .as_deref()
            .context("completed lighting acceptance has no artifact path")?;
        anyhow::ensure!(
            self.captures.len() == 4,
            "lighting artifact requires four captures"
        );
        let binary_identity = executable_identity()?;
        let mut payload = Vec::new();
        let phases = self
            .captures
            .iter()
            .map(|capture| phase_manifest(capture, binary_identity, &mut payload))
            .collect::<Vec<_>>();
        let manifest = ArtifactManifest {
            schema: ARTIFACT_SCHEMA,
            calibration: CALIBRATION_ID,
            phase_count: phases.len(),
            phases,
        };
        let manifest = toml::to_string(&manifest)?;
        write_atomic_artifact(path, manifest.as_bytes(), &payload)?;
        log::info!(
            "[LIGHTING_MODE_ACCEPTANCE] artifact={} calibration={} payload_bytes={}",
            path.display(),
            CALIBRATION_ID,
            payload.len(),
        );
        Ok(())
    }
}

#[derive(Serialize)]
struct ArtifactManifest<'a> {
    schema: &'a str,
    calibration: &'a str,
    phase_count: usize,
    phases: Vec<PhaseManifest>,
}

#[derive(Serialize)]
struct PhaseManifest {
    label: &'static str,
    terrain_mode: &'static str,
    raster_mode: &'static str,
    binary_identity: String,
    fixture: &'static str,
    camera_pose_bits: [u32; 6],
    render_extent: [u32; 2],
    screen_extent: [u32; 2],
    extent_generation: u64,
    visible_terrain_revision: u32,
    ddgi_field_serial: u64,
    ddgi_geometry_revision: u32,
    ddgi_radiance_revision: u32,
    ddgi_spacing_voxels: u32,
    ddgi_update_epoch: u32,
    ddgi_source_field_serial: u64,
    ddgi_source_geometry_revision: u32,
    ddgi_source_radiance_revision: u32,
    ddgi_source_update_epoch: u32,
    authored_lighting_revision: u64,
    local_lighting_revision: u64,
    visual_time_bits: u32,
    sampling_serial: u32,
    layers: Vec<LayerManifest>,
}

#[derive(Serialize)]
struct LayerManifest {
    kind: &'static str,
    format: &'static str,
    width: u32,
    height: u32,
    offset: usize,
    length: usize,
    fnv1a64: String,
}

fn phase_manifest(
    capture: &CapturedLightingModePhase,
    binary_identity: u64,
    payload: &mut Vec<u8>,
) -> PhaseManifest {
    let controls = capture.phase.controls(EffectiveLightingControls::new(
        TerrainLightingMode::Ddgi,
        RasterLightingMode::Ddgi,
    ));
    let extent = capture.identity.render.render_extent;
    let mut layers = Vec::with_capacity(3);
    for (kind, format, bytes) in [
        ("terrain_rgbe", "R32_UINT", &capture.layers.terrain_rgbe),
        ("terrain_depth", "R32_SFLOAT", &capture.layers.terrain_depth),
        ("raster_rgba", "R8G8B8A8_UNORM", &capture.layers.raster_rgba),
    ] {
        let offset = payload.len();
        payload.extend_from_slice(bytes);
        layers.push(LayerManifest {
            kind,
            format,
            width: extent[0],
            height: extent[1],
            offset,
            length: bytes.len(),
            fnv1a64: format!("{:016x}", fnv1a64(bytes)),
        });
    }
    let identity = capture.identity;
    PhaseManifest {
        label: capture.phase.label(),
        terrain_mode: match controls.terrain {
            TerrainLightingMode::Ddgi => "ddgi",
            TerrainLightingMode::PathTracingReference => "path-reference",
        },
        raster_mode: match controls.raster {
            RasterLightingMode::Ddgi => "ddgi",
            RasterLightingMode::Legacy => "legacy",
        },
        binary_identity: format!("fnv1a64:{binary_identity:016x}"),
        fixture: FIXTURE_ID,
        camera_pose_bits: identity.scene.camera_pose_bits,
        render_extent: identity.render.render_extent,
        screen_extent: identity.render.screen_extent,
        extent_generation: identity.render.extent_generation,
        visible_terrain_revision: identity.scene.visible_terrain_revision,
        ddgi_field_serial: identity.ddgi.published.serial,
        ddgi_geometry_revision: identity.ddgi.published.geometry_revision,
        ddgi_radiance_revision: identity.ddgi.published.radiance_revision,
        ddgi_spacing_voxels: identity.ddgi.spacing_voxels,
        ddgi_update_epoch: identity.ddgi.published.update_epoch,
        ddgi_source_field_serial: identity.ddgi.source.serial,
        ddgi_source_geometry_revision: identity.ddgi.source.geometry_revision,
        ddgi_source_radiance_revision: identity.ddgi.source.radiance_revision,
        ddgi_source_update_epoch: identity.ddgi.source.update_epoch,
        authored_lighting_revision: identity.lighting.authored_revision,
        local_lighting_revision: identity.lighting.local_revision,
        visual_time_bits: identity.scene.visual_time_bits,
        sampling_serial: identity.scene.sampling_serial,
        layers,
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}

fn executable_identity() -> Result<u64> {
    let path = std::env::current_exe()?;
    Ok(fnv1a64(&fs::read(path)?))
}

fn write_atomic_artifact(path: &Path, manifest: &[u8], payload: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    anyhow::ensure!(
        parent.is_dir(),
        "artifact parent does not exist: {}",
        parent.display()
    );
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(ARTIFACT_MAGIC)?;
    temporary.write_all(&(manifest.len() as u64).to_le_bytes())?;
    temporary.write_all(manifest)?;
    temporary.write_all(payload)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::process::Command;

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct FrameInputs {
        visual_time_seconds: f32,
        frame_delta_seconds: f32,
        time_of_day: f32,
        sampling_serial: u32,
        dither_strength_lsb: f32,
        path_tracing_max_bounces: u32,
        path_tracing_ambient_light: [f32; 3],
        lighting_controls: EffectiveLightingControls,
    }

    fn resolve(plan: LightingModeAcceptanceFramePlan, live: FrameInputs) -> FrameInputs {
        FrameInputs {
            visual_time_seconds: plan.visual_time_seconds.resolve(live.visual_time_seconds),
            frame_delta_seconds: plan.frame_delta_seconds.resolve(live.frame_delta_seconds),
            time_of_day: plan.time_of_day.resolve(live.time_of_day),
            sampling_serial: plan.sampling_serial.resolve(live.sampling_serial),
            dither_strength_lsb: plan.dither_strength_lsb.resolve(live.dither_strength_lsb),
            path_tracing_max_bounces: plan
                .path_tracing_max_bounces
                .resolve(live.path_tracing_max_bounces),
            path_tracing_ambient_light: plan
                .path_tracing_ambient_light
                .resolve(live.path_tracing_ambient_light),
            lighting_controls: plan.lighting_controls.resolve(live.lighting_controls),
        }
    }

    #[test]
    fn inactive_acceptance_preserves_gui_lighting_controls() {
        let gui = EffectiveLightingControls::new(
            TerrainLightingMode::PathTracingReference,
            RasterLightingMode::Legacy,
        );

        assert_eq!(LightingModeAcceptancePhase::Inactive.controls(gui), gui);
    }

    #[test]
    fn acceptance_phases_define_the_fixed_two_by_two_matrix() {
        let gui = EffectiveLightingControls::new(
            TerrainLightingMode::PathTracingReference,
            RasterLightingMode::Legacy,
        );

        assert_eq!(
            LightingModeAcceptancePhase::A.controls(gui),
            EffectiveLightingControls::new(TerrainLightingMode::Ddgi, RasterLightingMode::Ddgi,)
        );
        assert_eq!(
            LightingModeAcceptancePhase::B.controls(gui),
            EffectiveLightingControls::new(
                TerrainLightingMode::PathTracingReference,
                RasterLightingMode::Ddgi,
            )
        );
        assert_eq!(
            LightingModeAcceptancePhase::C.controls(gui),
            EffectiveLightingControls::new(
                TerrainLightingMode::PathTracingReference,
                RasterLightingMode::Legacy,
            )
        );
        assert_eq!(
            LightingModeAcceptancePhase::D.controls(gui),
            EffectiveLightingControls::new(TerrainLightingMode::Ddgi, RasterLightingMode::Legacy,)
        );
    }

    #[test]
    fn runtime_only_overrides_controls_when_acceptance_was_requested() {
        let inputs = FrameInputs {
            visual_time_seconds: 12.5,
            frame_delta_seconds: 1.0 / 60.0,
            time_of_day: 0.75,
            sampling_serial: 23,
            dither_strength_lsb: 0.25,
            path_tracing_max_bounces: 7,
            path_tracing_ambient_light: [0.1, 0.2, 0.3],
            lighting_controls: EffectiveLightingControls::from_gui(true, false),
        };
        let inactive = LightingModeAcceptanceRuntime::new(None);
        assert_eq!(resolve(inactive.frame_plan(), inputs), inputs);

        let options = LightingModeAcceptanceOptions {
            artifact_path: "target/r13-e2.rflma".into(),
        };
        let active = LightingModeAcceptanceRuntime::new(Some(&options));
        assert_eq!(
            resolve(active.frame_plan(), inputs),
            FrameInputs {
                visual_time_seconds: FIXED_VISUAL_TIME_SECONDS,
                frame_delta_seconds: 0.0,
                time_of_day: FIXED_TIME_OF_DAY,
                sampling_serial: FIXED_SAMPLING_SERIAL,
                dither_strength_lsb: 0.0,
                path_tracing_max_bounces: 2,
                path_tracing_ambient_light: [0.0; 3],
                lighting_controls: LightingModeAcceptancePhase::A
                    .controls(inputs.lighting_controls),
            }
        );
    }

    #[test]
    fn every_active_phase_owns_the_complete_fixed_frame_bundle() {
        let live = FrameInputs {
            visual_time_seconds: 12.5,
            frame_delta_seconds: 1.0 / 60.0,
            time_of_day: 0.75,
            sampling_serial: 23,
            dither_strength_lsb: 0.25,
            path_tracing_max_bounces: 7,
            path_tracing_ambient_light: [0.1, 0.2, 0.3],
            lighting_controls: EffectiveLightingControls::from_gui(true, false),
        };
        for phase in [
            LightingModeAcceptancePhase::A,
            LightingModeAcceptancePhase::B,
            LightingModeAcceptancePhase::C,
            LightingModeAcceptancePhase::D,
        ] {
            assert_eq!(
                resolve(phase.frame_plan(), live),
                FrameInputs {
                    visual_time_seconds: FIXED_VISUAL_TIME_SECONDS,
                    frame_delta_seconds: 0.0,
                    time_of_day: FIXED_TIME_OF_DAY,
                    sampling_serial: FIXED_SAMPLING_SERIAL,
                    dither_strength_lsb: 0.0,
                    path_tracing_max_bounces: 2,
                    path_tracing_ambient_light: [0.0; 3],
                    lighting_controls: phase.controls(live.lighting_controls),
                },
                "{}",
                phase.label(),
            );
        }
    }

    #[test]
    fn production_app_constructs_one_plan_and_has_no_primitive_runtime_bypass() {
        let core = include_str!("mod.rs");

        assert_eq!(core.matches(".frame_plan(").count(), 1);
        for removed_primitive in [
            "effective_controls(",
            "fixed_visual_time_seconds(",
            "fixed_sampling_serial(",
            "effective_dither_strength(",
            "effective_frame_delta_seconds(",
            "effective_time_of_day(",
            "effective_path_max_bounces(",
            "effective_path_ambient_light(",
        ] {
            assert!(!core.contains(removed_primitive), "{removed_primitive}");
        }
    }

    fn identity(revision: u32) -> LightingModeAcceptanceIdentity {
        LightingModeAcceptanceIdentity {
            scene: LightingModeAcceptanceSceneObservation {
                camera_pose_bits: [1, 2, 3, 4, 5, 6],
                visible_terrain_revision: revision,
                visual_time_bits: 0,
                sampling_serial: FIXED_SAMPLING_SERIAL,
            },
            render: LightingModeAcceptanceRenderObservation {
                render_extent: [960, 540],
                screen_extent: [1920, 1080],
                extent_generation: 7,
            },
            ddgi: LightingModeAcceptanceDdgiObservation {
                published: LightingModeAcceptanceDdgiFieldObservation {
                    serial: 11,
                    geometry_revision: revision,
                    radiance_revision: 13,
                    update_epoch: 9,
                },
                source: LightingModeAcceptanceDdgiFieldObservation {
                    serial: 10,
                    geometry_revision: revision,
                    radiance_revision: 13,
                    update_epoch: 8,
                },
                spacing_voxels: 32,
            },
            lighting: LightingModeAcceptanceLightingObservation {
                authored_revision: 17,
                local_revision: 19,
            },
        }
    }

    #[test]
    fn runtime_settles_then_claims_each_phase_in_fixed_order() {
        let options = LightingModeAcceptanceOptions {
            artifact_path: "target/r13-e2.rflma".into(),
        };
        let mut runtime = LightingModeAcceptanceRuntime::new(Some(&options));
        let identity = identity(5);

        for phase in [
            LightingModeAcceptancePhase::A,
            LightingModeAcceptancePhase::B,
            LightingModeAcceptancePhase::C,
            LightingModeAcceptancePhase::D,
        ] {
            assert_eq!(runtime.claim_capture(identity).unwrap(), None);
            assert_eq!(runtime.claim_capture(identity).unwrap(), Some(phase));
            runtime.complete_capture(phase, identity).unwrap();
        }
        assert!(runtime.is_complete());
    }

    #[test]
    fn runtime_fails_closed_when_identity_drifts_between_phases() {
        let options = LightingModeAcceptanceOptions {
            artifact_path: "target/r13-e2.rflma".into(),
        };
        let mut runtime = LightingModeAcceptanceRuntime::new(Some(&options));

        assert_eq!(runtime.claim_capture(identity(5)).unwrap(), None);
        assert_eq!(
            runtime.claim_capture(identity(6)).unwrap_err(),
            LightingModeAcceptanceError::IdentityDrift
        );
    }

    #[test]
    fn runtime_fails_closed_when_any_observation_group_drifts() {
        let baseline = identity(5);
        let mut scene = baseline;
        scene.scene.camera_pose_bits[0] ^= 1;
        let mut render = baseline;
        render.render.extent_generation += 1;
        let mut ddgi_published = baseline;
        ddgi_published.ddgi.published.serial += 1;
        let mut ddgi_source = baseline;
        ddgi_source.ddgi.source.update_epoch += 1;
        let mut lighting = baseline;
        lighting.lighting.local_revision += 1;

        for drifted in [scene, render, ddgi_published, ddgi_source, lighting] {
            let options = LightingModeAcceptanceOptions {
                artifact_path: "target/r13-e2.rflma".into(),
            };
            let mut runtime = LightingModeAcceptanceRuntime::new(Some(&options));
            assert_eq!(runtime.claim_capture(baseline).unwrap(), None);
            assert_eq!(
                runtime.claim_capture(drifted).unwrap_err(),
                LightingModeAcceptanceError::IdentityDrift
            );
        }
    }

    #[test]
    fn grouped_observation_serializes_the_existing_flat_v1_identity() {
        let capture = CapturedLightingModePhase {
            phase: LightingModeAcceptancePhase::A,
            identity: identity(5),
            layers: LightingModeProductionLayers {
                terrain_rgbe: vec![0; 4],
                terrain_depth: vec![0; 4],
                raster_rgba: vec![0; 4],
            },
        };
        let manifest = phase_manifest(&capture, 0x1234, &mut Vec::new());
        let value = toml::Value::try_from(&manifest).unwrap();
        let table = value.as_table().unwrap();
        let actual_keys = table.keys().map(String::as_str).collect::<BTreeSet<_>>();
        let expected_keys = [
            "authored_lighting_revision",
            "binary_identity",
            "camera_pose_bits",
            "ddgi_field_serial",
            "ddgi_geometry_revision",
            "ddgi_radiance_revision",
            "ddgi_source_field_serial",
            "ddgi_source_geometry_revision",
            "ddgi_source_radiance_revision",
            "ddgi_source_update_epoch",
            "ddgi_spacing_voxels",
            "ddgi_update_epoch",
            "extent_generation",
            "fixture",
            "label",
            "layers",
            "local_lighting_revision",
            "raster_mode",
            "render_extent",
            "sampling_serial",
            "screen_extent",
            "terrain_mode",
            "visible_terrain_revision",
            "visual_time_bits",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        assert_eq!(actual_keys, expected_keys);

        assert_eq!(table["label"].as_str(), Some("A"));
        assert_eq!(table["terrain_mode"].as_str(), Some("ddgi"));
        assert_eq!(table["raster_mode"].as_str(), Some("ddgi"));
        assert_eq!(
            table["binary_identity"].as_str(),
            Some("fnv1a64:0000000000001234")
        );
        assert_eq!(table["fixture"].as_str(), Some("foliage-shadow-r13-e2-v1"));
        for (key, expected) in [
            ("extent_generation", 7),
            ("visible_terrain_revision", 5),
            ("ddgi_field_serial", 11),
            ("ddgi_geometry_revision", 5),
            ("ddgi_radiance_revision", 13),
            ("ddgi_spacing_voxels", 32),
            ("ddgi_update_epoch", 9),
            ("ddgi_source_field_serial", 10),
            ("ddgi_source_geometry_revision", 5),
            ("ddgi_source_radiance_revision", 13),
            ("ddgi_source_update_epoch", 8),
            ("authored_lighting_revision", 17),
            ("local_lighting_revision", 19),
            ("visual_time_bits", 0),
            ("sampling_serial", i64::from(FIXED_SAMPLING_SERIAL)),
        ] {
            assert_eq!(table[key].as_integer(), Some(expected), "{key}");
        }
        assert_eq!(table["camera_pose_bits"].as_array().unwrap().len(), 6);
        assert_eq!(table["camera_pose_bits"].to_string(), "[1, 2, 3, 4, 5, 6]");
        assert_eq!(table["render_extent"].to_string(), "[960, 540]");
        assert_eq!(table["screen_extent"].to_string(), "[1920, 1080]");

        let layers = table["layers"].as_array().unwrap();
        assert_eq!(layers.len(), 3);
        for (layer, kind, format, offset) in [
            (&layers[0], "terrain_rgbe", "R32_UINT", 0),
            (&layers[1], "terrain_depth", "R32_SFLOAT", 4),
            (&layers[2], "raster_rgba", "R8G8B8A8_UNORM", 8),
        ] {
            let layer = layer.as_table().unwrap();
            assert_eq!(
                layer.keys().map(String::as_str).collect::<BTreeSet<_>>(),
                ["fnv1a64", "format", "height", "kind", "length", "offset", "width"]
                    .into_iter()
                    .collect()
            );
            assert_eq!(layer["kind"].as_str(), Some(kind));
            assert_eq!(layer["format"].as_str(), Some(format));
            assert_eq!(layer["width"].as_integer(), Some(960));
            assert_eq!(layer["height"].as_integer(), Some(540));
            assert_eq!(layer["offset"].as_integer(), Some(offset));
            assert_eq!(layer["length"].as_integer(), Some(4));
            assert_eq!(layer["fnv1a64"].as_str(), Some("4d25767f9dce13f5"));
        }
    }

    #[test]
    fn rust_producer_artifact_passes_the_official_python_analyzer() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("producer-golden.rflma");
        let mut golden_identity = identity(2);
        golden_identity.render.render_extent = [20, 1];
        golden_identity.render.screen_extent = [40, 2];
        golden_identity.render.extent_generation = 1;
        golden_identity.ddgi.published.serial = 3;
        golden_identity.ddgi.published.radiance_revision = 4;
        golden_identity.ddgi.published.update_epoch = 8;
        golden_identity.ddgi.source.serial = 2;
        golden_identity.ddgi.source.radiance_revision = 4;
        golden_identity.ddgi.source.update_epoch = 7;
        golden_identity.lighting.authored_revision = 4;
        golden_identity.lighting.local_revision = 5;

        let depth = 0.5_f32
            .to_le_bytes()
            .into_iter()
            .cycle()
            .take(80)
            .collect::<Vec<_>>();
        let terrain_changed = [1, 0, 0, 0]
            .into_iter()
            .cycle()
            .take(80)
            .collect::<Vec<_>>();
        let raster_ddgi = [0, 0, 0, 255]
            .into_iter()
            .cycle()
            .take(80)
            .collect::<Vec<_>>();
        let raster_legacy = [1, 0, 0, 255]
            .into_iter()
            .cycle()
            .take(80)
            .collect::<Vec<_>>();
        let captures = [
            (
                LightingModeAcceptancePhase::A,
                vec![0; 80],
                raster_ddgi.clone(),
            ),
            (
                LightingModeAcceptancePhase::B,
                terrain_changed.clone(),
                raster_ddgi,
            ),
            (
                LightingModeAcceptancePhase::C,
                terrain_changed,
                raster_legacy.clone(),
            ),
            (LightingModeAcceptancePhase::D, vec![0; 80], raster_legacy),
        ]
        .into_iter()
        .map(
            |(phase, terrain_rgbe, raster_rgba)| CapturedLightingModePhase {
                phase,
                identity: golden_identity,
                layers: LightingModeProductionLayers {
                    terrain_rgbe,
                    terrain_depth: depth.clone(),
                    raster_rgba,
                },
            },
        )
        .collect::<Vec<_>>();
        let mut payload = Vec::new();
        let phases = captures
            .iter()
            .map(|capture| phase_manifest(capture, 0x0123_4567_89ab_cdef, &mut payload))
            .collect();
        let manifest = toml::to_string(&ArtifactManifest {
            schema: ARTIFACT_SCHEMA,
            calibration: CALIBRATION_ID,
            phase_count: 4,
            phases,
        })
        .unwrap();
        write_atomic_artifact(&artifact, manifest.as_bytes(), &payload).unwrap();

        let analyzer = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("scripts/analyze_lighting_mode_acceptance.py");
        let output = Command::new("python3")
            .arg(analyzer)
            .arg(&artifact)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = String::from_utf8(output.stdout).unwrap();
        for field in [
            r#""schema": "re-flora-lighting-mode-acceptance-v1""#,
            r#""calibration": "r13-e2-production-v1""#,
            r#""verdict": "GREEN""#,
            r#""terrain_changed_ab": 20"#,
            r#""raster_changed_ad": 20"#,
        ] {
            assert!(result.contains(field), "missing {field} in {result}");
        }
    }

    #[test]
    fn production_raster_trace_uses_the_frozen_acceptance_visual_time() {
        let core = include_str!("mod.rs");
        let trace_call = core
            .split(".record_trace_after_shadow_prepass(")
            .nth(1)
            .expect("production trace call must remain wired")
            .split("flora_color_tables")
            .next()
            .expect("trace time must precede flora color tables");

        assert!(trace_call.contains("visual_time_since_start"));
        assert!(!trace_call.contains("self.time_info.time_since_start()"));
    }
}
