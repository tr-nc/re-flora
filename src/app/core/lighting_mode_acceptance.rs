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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LightingModeAcceptanceIdentity {
    pub camera_pose_bits: [u32; 6],
    pub render_extent: [u32; 2],
    pub screen_extent: [u32; 2],
    pub extent_generation: u64,
    pub visible_terrain_revision: u32,
    pub ddgi_field_serial: u64,
    pub ddgi_geometry_revision: u32,
    pub ddgi_radiance_revision: u32,
    pub ddgi_spacing_voxels: u32,
    pub ddgi_update_epoch: u32,
    pub ddgi_source_field_serial: u64,
    pub ddgi_source_geometry_revision: u32,
    pub ddgi_source_radiance_revision: u32,
    pub ddgi_source_update_epoch: u32,
    pub authored_lighting_revision: u64,
    pub local_lighting_revision: u64,
    pub visual_time_bits: u32,
    pub sampling_serial: u32,
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

    pub(super) fn effective_controls(
        &self,
        gui: EffectiveLightingControls,
    ) -> EffectiveLightingControls {
        self.phase.controls(gui)
    }

    pub(super) fn is_active(&self) -> bool {
        !matches!(
            self.phase,
            LightingModeAcceptancePhase::Inactive | LightingModeAcceptancePhase::Complete
        )
    }

    pub(super) fn fixed_visual_time_seconds(&self, gui_time: f32) -> f32 {
        self.is_active()
            .then_some(FIXED_VISUAL_TIME_SECONDS)
            .unwrap_or(gui_time)
    }

    pub(super) fn fixed_sampling_serial(&self, frame_serial: u32) -> u32 {
        self.is_active()
            .then_some(FIXED_SAMPLING_SERIAL)
            .unwrap_or(frame_serial)
    }

    pub(super) fn effective_dither_strength(&self, gui_dither: f32) -> f32 {
        self.is_active().then_some(0.0).unwrap_or(gui_dither)
    }

    pub(super) fn effective_frame_delta_seconds(&self, frame_delta: f32) -> f32 {
        self.is_active().then_some(0.0).unwrap_or(frame_delta)
    }

    pub(super) fn effective_time_of_day(&self, time_of_day: f32) -> f32 {
        self.is_active()
            .then_some(FIXED_TIME_OF_DAY)
            .unwrap_or(time_of_day)
    }

    pub(super) fn effective_path_max_bounces(&self, gui_value: u32) -> u32 {
        self.is_active().then_some(2).unwrap_or(gui_value)
    }

    pub(super) fn effective_path_ambient_light(&self, gui_value: [f32; 3]) -> [f32; 3] {
        self.is_active().then_some([0.0; 3]).unwrap_or(gui_value)
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
    let extent = capture.identity.render_extent;
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
        camera_pose_bits: identity.camera_pose_bits,
        render_extent: identity.render_extent,
        screen_extent: identity.screen_extent,
        extent_generation: identity.extent_generation,
        visible_terrain_revision: identity.visible_terrain_revision,
        ddgi_field_serial: identity.ddgi_field_serial,
        ddgi_geometry_revision: identity.ddgi_geometry_revision,
        ddgi_radiance_revision: identity.ddgi_radiance_revision,
        ddgi_spacing_voxels: identity.ddgi_spacing_voxels,
        ddgi_update_epoch: identity.ddgi_update_epoch,
        ddgi_source_field_serial: identity.ddgi_source_field_serial,
        ddgi_source_geometry_revision: identity.ddgi_source_geometry_revision,
        ddgi_source_radiance_revision: identity.ddgi_source_radiance_revision,
        ddgi_source_update_epoch: identity.ddgi_source_update_epoch,
        authored_lighting_revision: identity.authored_lighting_revision,
        local_lighting_revision: identity.local_lighting_revision,
        visual_time_bits: identity.visual_time_bits,
        sampling_serial: identity.sampling_serial,
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
        let gui = EffectiveLightingControls::from_gui(true, false);
        let inactive = LightingModeAcceptanceRuntime::new(None);
        assert_eq!(inactive.effective_controls(gui), gui);

        let options = LightingModeAcceptanceOptions {
            artifact_path: "target/r13-e2.rflma".into(),
        };
        let active = LightingModeAcceptanceRuntime::new(Some(&options));
        assert_eq!(
            active.effective_controls(gui),
            LightingModeAcceptancePhase::A.controls(gui)
        );
    }

    fn identity(revision: u32) -> LightingModeAcceptanceIdentity {
        LightingModeAcceptanceIdentity {
            camera_pose_bits: [1, 2, 3, 4, 5, 6],
            render_extent: [960, 540],
            screen_extent: [1920, 1080],
            extent_generation: 7,
            visible_terrain_revision: revision,
            ddgi_field_serial: 11,
            ddgi_geometry_revision: revision,
            ddgi_radiance_revision: 13,
            ddgi_spacing_voxels: 32,
            ddgi_update_epoch: 9,
            ddgi_source_field_serial: 10,
            ddgi_source_geometry_revision: revision,
            ddgi_source_radiance_revision: 13,
            ddgi_source_update_epoch: 8,
            authored_lighting_revision: 17,
            local_lighting_revision: 19,
            visual_time_bits: 0,
            sampling_serial: 23,
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
}
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
