use super::environment_lighting_test_scene::{
    EnvironmentLightingTestScene, RadianceCaptureCheckpoint, RadianceCaptureRequest,
};
use crate::ddgi::{
    DdgiCaptureCheckpoint, DdgiDebugView, DdgiFieldIdentity, DdgiFieldState, DdgiRefreshState,
    DdgiVolumeStage,
};
use crate::environment_lighting::{DdgiRadianceSnapshot, DDGI_AUTHORED_SKY_MODEL_IDENTITY};
use crate::tracer::Tracer;
use anyhow::{ensure, Context, Result};
use re_flora_vkn::{Buffer, BufferUsage, CommandBuffer, Extent2D, MemoryLocation, VulkanContext};
use std::io::Write;
use std::path::{Path, PathBuf};

const CAPTURE_MAGIC: &[u8; 8] = b"RFIRR001";
const CAPTURE_VERSION: u32 = 6;
const CAPTURE_CHANNEL_COUNT: u32 = 4;
const CAPTURE_PLANE_COUNT: u32 = 3;
const CAPTURE_HEADER_BYTE_COUNT: usize = 124;
const DDGI_BACKEND_ID: u32 = 1;
const CAPTURE_STATE_CONVERGING: u32 = 1;
const CAPTURE_STATE_CONVERGED: u32 = 2;
#[cfg(test)]
const CAPTURE_PUBLICATION_PUBLISHED: u32 = 1;
const CAPTURE_UNKNOWN_U32: u32 = u32::MAX;
const CAPTURE_UNKNOWN_U64: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CaptureMetadata {
    geometry_revision: u32,
    radiance_revision: u32,
    radiance_model_identity: u64,
    build_token_serial: u64,
    field_serial: u64,
    lifecycle_state: u32,
    update_epoch: u32,
    source_state: u32,
    source_update_epoch: u32,
    source_field_serial: u64,
    source_radiance_revision: u32,
    publication_state: u32,
    batch_order: u32,
    max_abs_delta: f32,
    max_rel_delta: f32,
    nonfinite_count: u32,
    valid_count: u32,
}

impl CaptureMetadata {
    fn from_checkpoint(
        checkpoint: DdgiCaptureCheckpoint,
        radiance_model_identity: u64,
    ) -> Result<Self> {
        let field = checkpoint.field.field();
        ensure!(
            checkpoint.build_token.terrain_revision() == field.geometry_revision()
                && checkpoint.build_token.spacing_voxels() == field.spacing_voxels(),
            "DDGI build token does not own the captured field: checkpoint={checkpoint:?}"
        );
        let source = checkpoint.field.source();
        Ok(Self {
            geometry_revision: field.geometry_revision(),
            radiance_revision: field.radiance_revision(),
            radiance_model_identity,
            build_token_serial: checkpoint.build_token.serial(),
            field_serial: field.serial(),
            lifecycle_state: encode_lifecycle_state(field.state()),
            update_epoch: field.update_epoch(),
            source_state: source
                .map(|source| encode_lifecycle_state(source.state()))
                .unwrap_or(CAPTURE_UNKNOWN_U32),
            source_update_epoch: source
                .map(|source| source.update_epoch())
                .unwrap_or(CAPTURE_UNKNOWN_U32),
            source_field_serial: source
                .map(|source| source.serial())
                .unwrap_or(CAPTURE_UNKNOWN_U64),
            source_radiance_revision: source
                .map(|source| source.radiance_revision())
                .unwrap_or(CAPTURE_UNKNOWN_U32),
            publication_state: checkpoint.publication as u32,
            batch_order: checkpoint.batch_order.as_u32(),
            max_abs_delta: checkpoint.validation.max_absolute_rgb_delta,
            max_rel_delta: checkpoint.validation.max_relative_rgb_delta,
            nonfinite_count: checkpoint.validation.non_finite_count,
            valid_count: checkpoint.validation.valid_texel_count,
        })
    }

    fn write_to(self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.geometry_revision.to_le_bytes())?;
        writer.write_all(&self.radiance_revision.to_le_bytes())?;
        writer.write_all(&self.radiance_model_identity.to_le_bytes())?;
        writer.write_all(&self.build_token_serial.to_le_bytes())?;
        writer.write_all(&self.field_serial.to_le_bytes())?;
        for value in [
            self.lifecycle_state,
            self.update_epoch,
            self.source_state,
            self.source_update_epoch,
        ] {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&self.source_field_serial.to_le_bytes())?;
        for value in [
            self.source_radiance_revision,
            self.publication_state,
            self.batch_order,
        ] {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&self.max_abs_delta.to_le_bytes())?;
        writer.write_all(&self.max_rel_delta.to_le_bytes())?;
        writer.write_all(&self.nonfinite_count.to_le_bytes())?;
        writer.write_all(&self.valid_count.to_le_bytes())?;
        Ok(())
    }
}

fn encode_lifecycle_state(state: DdgiFieldState) -> u32 {
    match state {
        DdgiFieldState::Converging => CAPTURE_STATE_CONVERGING,
        DdgiFieldState::Converged => CAPTURE_STATE_CONVERGED,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum CaptureState {
    #[default]
    Waiting,
    Recording,
    Complete,
}

pub(super) struct EnvironmentIrradianceCaptureRuntime {
    base_path: Option<String>,
    state: CaptureState,
}

pub(super) struct PendingEnvironmentIrradianceCapture {
    path: String,
    extent: Extent2D,
    spacing_voxels: u32,
    debug_view: DdgiDebugView,
    metadata: CaptureMetadata,
    radiance_evidence: Option<RadianceCaptureEvidence>,
    buffer: Buffer,
}

impl PendingEnvironmentIrradianceCapture {
    fn path(&self) -> &str {
        &self.path
    }

    fn radiance_checkpoint(&self) -> Option<RadianceCaptureCheckpoint> {
        self.radiance_evidence
            .map(|evidence| evidence.request.checkpoint)
    }
}

#[derive(Clone, Copy, Debug)]
struct RadianceCaptureEvidence {
    request: RadianceCaptureRequest,
    capture_frame: u64,
    live_radiance_revision: u32,
    live_snapshot: DdgiRadianceSnapshot,
    latest_radiance_revision: Option<u32>,
    active_field: DdgiFieldIdentity,
    building_field: Option<DdgiFieldIdentity>,
    builder_latched_radiance_revision: Option<u32>,
    builder_latched_snapshot: Option<DdgiRadianceSnapshot>,
}

fn radiance_capture_path(base: &str, checkpoint: RadianceCaptureCheckpoint) -> PathBuf {
    if checkpoint == RadianceCaptureCheckpoint::Final {
        return PathBuf::from(base);
    }
    let base = Path::new(base);
    let stem = base
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("radiance-capture");
    let extension = base.extension().and_then(|extension| extension.to_str());
    let file_name = match extension {
        Some(extension) => format!("{stem}.{}.{extension}", checkpoint.label()),
        None => format!("{stem}.{}", checkpoint.label()),
    };
    base.with_file_name(file_name)
}

fn field_json(field: Option<DdgiFieldIdentity>) -> String {
    let Some(field) = field else {
        return "null".to_owned();
    };
    let key = field.field();
    let source = field.source();
    format!(
        "{{\"field_serial\":{},\"geometry_revision\":{},\"radiance_revision\":{},\"spacing_voxels\":{},\"lifecycle_state\":\"{:?}\",\"update_epoch\":{},\"source_field_serial\":{},\"source_radiance_revision\":{}}}",
        key.serial(),
        key.geometry_revision(),
        key.radiance_revision(),
        key.spacing_voxels(),
        key.state(),
        key.update_epoch(),
        source.map_or(0, |source| source.serial()),
        source.map_or(0, |source| source.radiance_revision()),
    )
}

fn snapshot_json(snapshot: Option<DdgiRadianceSnapshot>) -> String {
    let Some(snapshot) = snapshot else {
        return "null".to_owned();
    };
    format!(
        "{{\"sun_direction\":[{},{},{}],\"sun_color\":[{},{},{}],\"sun_luminance\":{},\"terrain_ray_origin_offset_world\":{},\"ddgi_receiver_visibility_bias_world\":{}}}",
        snapshot.sun_direction.x,
        snapshot.sun_direction.y,
        snapshot.sun_direction.z,
        snapshot.sun_color.x,
        snapshot.sun_color.y,
        snapshot.sun_color.z,
        snapshot.sun_luminance,
        snapshot.terrain_ray_origin_offset_world,
        snapshot.ddgi_receiver_visibility_bias_world,
    )
}

fn option_u32_json(value: Option<u32>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

impl RadianceCaptureEvidence {
    fn write(self, capture_path: &str) -> Result<()> {
        let identity_path = format!("{capture_path}.identity.json");
        let mut file = std::fs::File::create(&identity_path)
            .with_context(|| format!("create {identity_path}"))?;
        let mutation_frame = self
            .request
            .mutation_frame
            .map_or_else(|| "null".to_owned(), |frame| frame.to_string());
        write!(
            file,
            "{{\n  \"schema\": \"re-flora-ddgi-radiance-capture-v1\",\n  \"checkpoint\": \"{}\",\n  \"mutation_frame\": {},\n  \"capture_frame\": {},\n  \"live_radiance_revision\": {},\n  \"live_snapshot\": {},\n  \"latest_radiance_revision\": {},\n  \"active_field\": {},\n  \"building_field\": {},\n  \"builder_latched_radiance_revision\": {},\n  \"builder_latched_snapshot\": {}\n}}\n",
            self.request.checkpoint.label(),
            mutation_frame,
            self.capture_frame,
            self.live_radiance_revision,
            snapshot_json(Some(self.live_snapshot)),
            option_u32_json(self.latest_radiance_revision),
            field_json(Some(self.active_field)),
            field_json(self.building_field),
            option_u32_json(self.builder_latched_radiance_revision),
            snapshot_json(self.builder_latched_snapshot),
        )?;
        file.flush()?;
        log::info!(
            "[ENV_IRRADIANCE_CAPTURE] radiance identity saved path={} checkpoint={} mutation_frame={:?} capture_frame={} live_radiance_revision={} active_field_serial={} active_radiance_revision={} latest_radiance_revision={:?}",
            identity_path,
            self.request.checkpoint.label(),
            self.request.mutation_frame,
            self.capture_frame,
            self.live_radiance_revision,
            self.active_field.field().serial(),
            self.active_field.field().radiance_revision(),
            self.latest_radiance_revision,
        );
        Ok(())
    }
}

impl EnvironmentIrradianceCaptureRuntime {
    pub(super) fn new(base_path: Option<String>) -> Self {
        Self {
            base_path,
            state: CaptureState::Waiting,
        }
    }

    pub(super) fn is_enabled(&self) -> bool {
        self.base_path.is_some()
    }

    pub(super) fn record_if_ready(
        &mut self,
        tracer: &Tracer,
        vulkan_ctx: &VulkanContext,
        cmdbuf: &CommandBuffer,
        test_scene: Option<&EnvironmentLightingTestScene>,
        capture_frame: u64,
    ) -> Result<Option<PendingEnvironmentIrradianceCapture>> {
        let Some(base_path) = self.base_path.clone() else {
            return Ok(None);
        };
        if self.state != CaptureState::Waiting {
            return Ok(None);
        }

        let test_scene_ready =
            test_scene.is_none_or(EnvironmentLightingTestScene::is_capture_ready);
        let target_scene_ready = tracer
            .ddgi_capture_target()
            .update_epoch()
            .is_some_and(|epoch| epoch == 0)
            || test_scene_ready;
        let inflight_target_revision =
            test_scene.and_then(EnvironmentLightingTestScene::inflight_capture_target_revision);
        let inflight_checkpoint_ready = inflight_target_revision.is_none_or(|target_revision| {
            let runtime = tracer.ddgi_runtime_status();
            matches!(
                runtime.coordinator(),
                DdgiRefreshState::BuildingTerrain {
                    candidate,
                    latest_terrain_revision,
                } if candidate.terrain_revision() == target_revision
                    && latest_terrain_revision == target_revision
            ) && runtime.target_terrain_revision() == Some(target_revision)
                && runtime
                    .active()
                    .relocated_terrain_revision
                    .is_some_and(|active_revision| active_revision != target_revision)
                && runtime.staging().is_some_and(|staging| {
                    staging.build_token.is_some() && staging.stage != DdgiVolumeStage::Ready
                })
                && runtime.active_consumers_are_available()
        });
        if !target_scene_ready
            || !inflight_checkpoint_ready
            || tracer.ddgi_capture_checkpoint().is_none()
        {
            return Ok(None);
        }

        if let Some(target_revision) = inflight_target_revision {
            let runtime = tracer.ddgi_runtime_status();
            let staging = runtime
                .staging()
                .expect("ready in-flight checkpoint must retain staging work");
            log::info!(target: "re_flora::app::core::environment_irradiance_capture",
                "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] recording active_terrain_revision={:?} target_terrain_revision={} staging_token_serial={:?} staging_stage={:?} staging_progress={}/{} coordinator={:?} invalidation=stale-active",
                runtime.active().relocated_terrain_revision,
                target_revision,
                runtime.staging_token().map(|token| token.serial()),
                staging.stage,
                staging.filtered_probe_count,
                staging.grid.probe_count(),
                runtime.coordinator(),
            );
        }

        let readback =
            Self::prepare_readback(base_path, tracer, vulkan_ctx, test_scene, capture_frame)?;
        tracer.record_environment_irradiance_capture_readback(cmdbuf, &readback.buffer);
        self.state = CaptureState::Recording;
        log::info!(
            "[ENV_IRRADIANCE_CAPTURE] recording backend=ddgi path={}",
            readback.path(),
        );
        Ok(Some(readback))
    }

    fn prepare_readback(
        base_path: String,
        tracer: &Tracer,
        vulkan_ctx: &VulkanContext,
        test_scene: Option<&EnvironmentLightingTestScene>,
        capture_frame: u64,
    ) -> Result<PendingEnvironmentIrradianceCapture> {
        let radiance_request =
            test_scene.and_then(EnvironmentLightingTestScene::radiance_capture_request);
        let path = radiance_request.map_or_else(
            || PathBuf::from(&base_path),
            |request| radiance_capture_path(&base_path, request.checkpoint),
        );
        let path = path.to_string_lossy().into_owned();
        let output_path = Path::new(&path);
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                anyhow::bail!("parent directory does not exist: {}", parent.display());
            }
        }

        let extent = tracer.environment_irradiance_capture_extent();
        let byte_count = u64::from(extent.width)
            * u64::from(extent.height)
            * std::mem::size_of::<[f32; 4]>() as u64
            * u64::from(CAPTURE_PLANE_COUNT);
        let checkpoint = tracer
            .ddgi_capture_checkpoint()
            .context("cannot capture DDGI before the requested field checkpoint is resident")?;
        let metadata =
            CaptureMetadata::from_checkpoint(checkpoint, DDGI_AUTHORED_SKY_MODEL_IDENTITY)?;
        let radiance_evidence = radiance_request
            .map(|request| {
                if let Some(mutation_frame) = request.mutation_frame {
                    ensure!(
                        capture_frame == mutation_frame + 1,
                        "{} capture frame {} is not mutation frame {} + 1",
                        request.checkpoint.label(),
                        capture_frame,
                        mutation_frame,
                    );
                }
                let status = tracer.ddgi_runtime_status();
                let active = status.active();
                let active_field = active
                    .published_field
                    .context("radiance evidence requires a published active field")?;
                ensure!(
                    checkpoint.field == active_field,
                    "radiance evidence checkpoint {:?} is not active field {:?}",
                    checkpoint.field,
                    active_field,
                );
                let evidence = RadianceCaptureEvidence {
                    request,
                    capture_frame,
                    live_radiance_revision: tracer.ddgi_live_radiance_revision(),
                    live_snapshot: tracer
                        .ddgi_live_radiance_snapshot()
                        .context("radiance evidence requires the live renderer snapshot")?,
                    latest_radiance_revision: tracer.ddgi_latest_radiance_revision(),
                    active_field,
                    building_field: status.builder().building_field,
                    builder_latched_radiance_revision: status.builder().radiance_revision,
                    builder_latched_snapshot: tracer.ddgi_builder_radiance_snapshot(),
                };
                let active_revision = evidence.active_field.field().radiance_revision();
                match request.checkpoint {
                    RadianceCaptureCheckpoint::Baseline => {
                        ensure!(
                            evidence.live_radiance_revision == active_revision
                                && evidence.latest_radiance_revision == Some(active_revision),
                            "baseline live/latest revision does not match active field"
                        );
                    }
                    RadianceCaptureCheckpoint::R2NextFrame => {
                        let expected_live = active_revision.wrapping_add(1).max(1);
                        let building = evidence
                            .building_field
                            .context("r2 next-frame evidence requires in-flight r2")?;
                        ensure!(
                            evidence.live_radiance_revision == expected_live
                                && evidence.latest_radiance_revision == Some(expected_live)
                                && building.field().radiance_revision() == expected_live
                                && evidence.builder_latched_radiance_revision
                                    == Some(expected_live)
                                && evidence.builder_latched_snapshot == Some(evidence.live_snapshot),
                            "r2 next-frame live/builder snapshot identity mismatch"
                        );
                    }
                    RadianceCaptureCheckpoint::R4NextFrame => {
                        let building = evidence
                            .building_field
                            .context("r4 next-frame evidence requires immutable in-flight r2")?;
                        let r2_revision = building.field().radiance_revision();
                        let expected_live = r2_revision.wrapping_add(2).max(1);
                        ensure!(
                            active_revision.wrapping_add(1).max(1) == r2_revision
                                && evidence.live_radiance_revision == expected_live
                                && evidence.latest_radiance_revision == Some(expected_live)
                                && evidence.builder_latched_radiance_revision == Some(r2_revision)
                                && evidence.builder_latched_snapshot.is_some()
                                && evidence.builder_latched_snapshot != Some(evidence.live_snapshot),
                            "r4 next-frame did not preserve r2 snapshot while latest r4 coalesced"
                        );
                    }
                    RadianceCaptureCheckpoint::Final => {
                        ensure!(
                            evidence.live_radiance_revision == active_revision
                                && evidence.latest_radiance_revision == Some(active_revision),
                            "final live/latest revision does not match active field"
                        );
                    }
                }
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] checkpoint={} mutation_frame={:?} capture_frame={} active_field_serial={} active_radiance_revision={} building_field_serial={} builder_latched_radiance_revision={:?} live_radiance_revision={} latest_radiance_revision={:?}",
                    request.checkpoint.label(),
                    request.mutation_frame,
                    capture_frame,
                    evidence.active_field.field().serial(),
                    active_revision,
                    evidence.building_field.map_or(0, |field| field.field().serial()),
                    evidence.builder_latched_radiance_revision,
                    evidence.live_radiance_revision,
                    evidence.latest_radiance_revision,
                );
                Ok::<_, anyhow::Error>(evidence)
            })
            .transpose()?;
        let allocator = tracer
            .get_screen_output_tex()
            .get_image()
            .get_allocator()
            .clone();
        let buffer = Buffer::new_sized(
            vulkan_ctx.device().clone(),
            allocator,
            BufferUsage::transfer_dst(),
            MemoryLocation::GpuToCpu,
            byte_count,
        );
        Ok(PendingEnvironmentIrradianceCapture {
            path,
            extent,
            spacing_voxels: checkpoint.field.field().spacing_voxels(),
            debug_view: tracer.ddgi_debug_view(),
            metadata,
            radiance_evidence,
            buffer,
        })
    }

    fn transition_after_capture(&mut self, sequence_complete: bool) -> bool {
        self.state = if sequence_complete {
            CaptureState::Complete
        } else {
            CaptureState::Waiting
        };
        sequence_complete
    }

    pub(super) fn complete(
        &mut self,
        readback: PendingEnvironmentIrradianceCapture,
        test_scene: Option<&mut EnvironmentLightingTestScene>,
    ) -> Result<bool> {
        debug_assert_eq!(self.state, CaptureState::Recording);
        let radiance_checkpoint = readback.radiance_checkpoint();
        self.state = CaptureState::Complete;
        Self::write_readback(readback)?;
        let sequence_complete = if let Some(checkpoint) = radiance_checkpoint {
            test_scene
                .context("radiance capture completion lost its test scene")?
                .complete_radiance_capture(checkpoint)
        } else {
            true
        };
        Ok(self.transition_after_capture(sequence_complete))
    }

    fn write_readback(readback: PendingEnvironmentIrradianceCapture) -> Result<()> {
        let raw = readback.buffer.read_back()?;
        let expected_bytes = readback.extent.width as usize
            * readback.extent.height as usize
            * std::mem::size_of::<[f32; 4]>()
            * CAPTURE_PLANE_COUNT as usize;
        if raw.len() != expected_bytes {
            anyhow::bail!(
                "capture byte count mismatch: got {}, expected {}",
                raw.len(),
                expected_bytes,
            );
        }

        let mut file = std::fs::File::create(&readback.path)
            .with_context(|| format!("create {}", readback.path))?;
        let mut header = Vec::with_capacity(CAPTURE_HEADER_BYTE_COUNT);
        header.write_all(CAPTURE_MAGIC)?;
        for value in [
            CAPTURE_VERSION,
            readback.extent.width,
            readback.extent.height,
            CAPTURE_CHANNEL_COUNT,
            DDGI_BACKEND_ID,
            readback.spacing_voxels,
            readback.debug_view.as_u32(),
            CAPTURE_PLANE_COUNT,
        ] {
            header.write_all(&value.to_le_bytes())?;
        }
        readback.metadata.write_to(&mut header)?;
        if header.len() != CAPTURE_HEADER_BYTE_COUNT {
            anyhow::bail!(
                "capture header byte count mismatch: got {}, expected {}",
                header.len(),
                CAPTURE_HEADER_BYTE_COUNT,
            );
        }
        file.write_all(&header)?;
        file.write_all(&raw)?;
        file.flush()?;
        if let Some(evidence) = readback.radiance_evidence {
            evidence.write(&readback.path)?;
        }
        log::info!(
            "[ENV_IRRADIANCE_CAPTURE] saved path={} extent={}x{} backend={} spacing_voxels={} view={} samples={} geometry_revision={} radiance_revision={} radiance_model_identity={} build_token_serial={} field_serial={} lifecycle_state={} update_epoch={} source_state={} source_update_epoch={} source_field_serial={} source_radiance_revision={} publication_state={} batch_order={} max_abs_delta={} max_rel_delta={} nonfinite_count={} valid_count={} format=float4-linear-rgb-hit+float4-world-xyz-exact-direct-sun-visibility+float4-direct-light-rgb-hit",
            readback.path,
            readback.extent.width,
            readback.extent.height,
            "ddgi",
            readback.spacing_voxels,
            readback.debug_view.label(),
            readback.extent.width * readback.extent.height,
            readback.metadata.geometry_revision,
            readback.metadata.radiance_revision,
            readback.metadata.radiance_model_identity,
            readback.metadata.build_token_serial,
            readback.metadata.field_serial,
            readback.metadata.lifecycle_state,
            readback.metadata.update_epoch,
            readback.metadata.source_state,
            readback.metadata.source_update_epoch,
            readback.metadata.source_field_serial,
            readback.metadata.source_radiance_revision,
            readback.metadata.publication_state,
            readback.metadata.batch_order,
            readback.metadata.max_abs_delta,
            readback.metadata.max_rel_delta,
            readback.metadata.nonfinite_count,
            readback.metadata.valid_count,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddgi::{
        DdgiAtlasValidationStats, DdgiBatchOrder, DdgiBuildKind, DdgiBuildToken,
        DdgiCaptureCheckpoint, DdgiCapturePublication, DdgiFieldIdentity, DdgiFieldKey,
        DdgiFieldState,
    };

    #[test]
    fn capture_runtime_rearms_only_for_an_incomplete_sequence() {
        let disabled = EnvironmentIrradianceCaptureRuntime::new(None);
        assert!(!disabled.is_enabled());

        let mut runtime =
            EnvironmentIrradianceCaptureRuntime::new(Some("capture.rfirr".to_owned()));
        assert!(runtime.is_enabled());
        runtime.state = CaptureState::Recording;
        assert!(!runtime.transition_after_capture(false));
        assert_eq!(runtime.state, CaptureState::Waiting);
        runtime.state = CaptureState::Recording;
        assert!(runtime.transition_after_capture(true));
        assert_eq!(runtime.state, CaptureState::Complete);
    }

    #[test]
    fn capture_header_is_fixed_width_and_self_describing() {
        assert_eq!(CAPTURE_MAGIC.len(), 8);
        assert_eq!(CAPTURE_VERSION, 6);
        assert_eq!(CAPTURE_CHANNEL_COUNT, 4);
        assert_eq!(CAPTURE_PLANE_COUNT, 3);
        assert_eq!(CAPTURE_HEADER_BYTE_COUNT, 124);
    }

    #[test]
    fn capture_metadata_uses_authoritative_published_terminal_identity() {
        let token = DdgiBuildToken::for_test(9001, 41, 16, DdgiBuildKind::Terrain);
        let source = DdgiFieldKey::new(88, 41, 17, 16, DdgiFieldState::Converging, 5).unwrap();
        let published = DdgiFieldIdentity::new(
            DdgiFieldKey::new(89, 41, 17, 16, DdgiFieldState::Converged, 6).unwrap(),
            Some(source),
        )
        .unwrap();
        let validation = DdgiAtlasValidationStats {
            max_absolute_rgb_delta: 0.0125,
            max_relative_rgb_delta: 0.025,
            non_finite_count: 0,
            negative_rgb_texel_count: 0,
            valid_texel_count: 314_432,
            scanned_stored_texel_count: 491_300,
        };
        let checkpoint = DdgiCaptureCheckpoint {
            build_token: token,
            field: published,
            validation,
            publication: DdgiCapturePublication::Published,
            batch_order: DdgiBatchOrder::Reverse,
        };
        let metadata = CaptureMetadata::from_checkpoint(
            checkpoint,
            crate::environment_lighting::DDGI_AUTHORED_SKY_MODEL_IDENTITY,
        )
        .unwrap();

        assert_eq!(metadata.geometry_revision, 41);
        assert_eq!(metadata.radiance_revision, 17);
        assert_eq!(
            metadata.radiance_model_identity,
            crate::environment_lighting::DDGI_AUTHORED_SKY_MODEL_IDENTITY
        );
        assert_eq!(metadata.build_token_serial, 9001);
        assert_eq!(metadata.field_serial, 89);
        assert_eq!(metadata.lifecycle_state, CAPTURE_STATE_CONVERGED);
        assert_eq!(metadata.update_epoch, 6);
        assert_eq!(metadata.source_state, CAPTURE_STATE_CONVERGING);
        assert_eq!(metadata.source_update_epoch, 5);
        assert_eq!(metadata.source_field_serial, 88);
        assert_eq!(metadata.source_radiance_revision, 17);
        assert_eq!(metadata.publication_state, CAPTURE_PUBLICATION_PUBLISHED);
        assert_eq!(metadata.batch_order, DdgiBatchOrder::Reverse.as_u32());
        assert_eq!(metadata.max_abs_delta, 0.0125);
        assert_eq!(metadata.max_rel_delta, 0.025);
        assert_eq!(metadata.nonfinite_count, 0);
        assert_eq!(metadata.valid_count, 314_432);

        let mismatched_token = DdgiBuildToken::for_test(9002, 42, 16, DdgiBuildKind::Terrain);
        let mismatch = CaptureMetadata::from_checkpoint(
            DdgiCaptureCheckpoint {
                build_token: mismatched_token,
                ..checkpoint
            },
            crate::environment_lighting::DDGI_AUTHORED_SKY_MODEL_IDENTITY,
        );
        assert!(
            mismatch.is_err(),
            "runtime/published identity drift must fail closed"
        );
    }

    #[test]
    fn capture_shader_writes_world_plane_with_probe_exact_shadow_contract() {
        let tracer = include_str!("../../../shader/slang/tracer.slang");
        let exact_shadow = include_str!("../../../shader/slang/ddgi_exact_sun_visibility.slang");
        let probe_trace = include_str!("../../../shader/slang/ddgi_probe_trace.slang");
        let ray_origin = include_str!("../../../shader/slang/terrain_ray_origin.slang");

        assert!(tracer.contains("import ddgi_exact_sun_visibility;"));
        assert!(tracer.contains("captureIndex + width * height"));
        assert!(tracer.contains("environmentCaptureIrradiance, terrainHit"));
        assert!(tracer.contains("environmentCaptureWorld"));
        let capture_visibility = tracer
            .split_once("if (shading_info.environment_irradiance_capture_enabled != 0u)")
            .expect("capture-only exact visibility block must exist")
            .1
            .split_once("environmentCaptureWorld =")
            .expect("world capture assignment must follow exact visibility")
            .0;
        assert!(!capture_visibility.contains("cosine"));
        assert!(!capture_visibility.contains("sunAboveHorizon"));
        assert!(capture_visibility.contains("ddgiExactTerrainSunVisibility("));
        assert!(capture_visibility.contains("gui_input.terrain_ray_origin_offset_world"));
        for contract in ["import terrain_ray_origin;", "terrainRayOriginAlongNormal("] {
            assert!(
                exact_shadow.contains(contract),
                "capture exact-shadow helper lost shared origin contract `{contract}`"
            );
            assert!(
                probe_trace.contains(contract),
                "probe exact-shadow path lost shared origin contract `{contract}`"
            );
        }
        assert!(exact_shadow.contains("originOffsetWorld"));
        assert!(probe_trace.contains("ddgi_radiance_sun.terrain_ray_origin_offset_world"));
        for contract in [
            "1.0 / 256.0",
            "terrainVoxelSurfacePositionAlongNormal(",
            "normalDirection * max(0.0, offsetWorld)",
        ] {
            assert!(
                ray_origin.contains(contract),
                "shared terrain ray origin lost contract `{contract}`"
            );
        }
        assert!(exact_shadow.contains("shadowHit.is_hit ? 0.0 : 1.0"));
        assert!(probe_trace.contains("shadowHit.is_hit ? 0.0 : 1.0"));
    }

    #[test]
    fn capture_shader_writes_direct_light_without_ddgi_environment() {
        let tracer = include_str!("../../../shader/slang/tracer.slang");

        let direct_lighting = tracer
            .split_once("float3 directLighting(")
            .expect("terrain direct-light function must exist")
            .1
            .split_once("static const uint PATH_TRACING_MAX_BOUNCES")
            .expect("terrain direct-light function must remain isolated")
            .0;
        assert!(!direct_lighting.contains("Ddgi"));
        assert!(!direct_lighting.contains("environmentIrradiance"));
        assert!(!direct_lighting.contains("sampleDdgi"));
        assert!(tracer.contains("out float3 directLight"));
        assert!(tracer.contains("directLight = directLighting("));
        assert!(tracer.contains("color += directLight"));
        assert!(tracer.contains("captureIndex + 2 * width * height"));
        assert!(tracer.contains("float4(directLight, terrainHit)"));
    }

    #[test]
    fn unpublished_s0_capture_uses_a_private_atlas_without_changing_consumers() {
        let tracer = include_str!("../../../shader/slang/tracer.slang");
        let query = include_str!("../../../shader/slang/ddgi_query.slang");

        assert!(tracer.contains("sampleDdgiUnpublishedTerrainSmoothEnvironment("));
        assert!(tracer.contains("environmentCaptureIrradiance = captureResult.irradiance"));
        assert!(tracer.contains("environmentCaptureIrradiance, terrainHit"));
        assert!(tracer.contains("color = environmentIrradiance * albedo"));
        assert!(query.contains("[[vk::binding(27, 0)]]\nSampler2D ddgi_irradiance_atlas"));
        assert!(query.contains("[[vk::binding(34, 0)]]\nSampler2D ddgi_capture_irradiance_atlas"));
        let capture_query = query
            .split_once("public DdgiQueryResult sampleDdgiUnpublishedTerrainSmoothEnvironment(")
            .expect("capture-only DDGI query must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiDiffuseEnvironment(")
            .expect("capture-only query must remain isolated")
            .0;
        assert!(capture_query.contains("query.ready = 1u"));
        assert!(capture_query.contains("query.invalidation_enabled = 0u"));
        assert!(capture_query.contains("ddgi_capture_irradiance_atlas"));
    }
}
