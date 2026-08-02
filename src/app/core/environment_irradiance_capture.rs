use super::environment_lighting_test_scene::{RadianceCaptureCheckpoint, RadianceCaptureRequest};
use super::App;
use crate::ddgi::{DdgiCaptureCheckpoint, DdgiDebugView, DdgiFieldIdentity, DdgiFieldStage};
use crate::environment_lighting::{DdgiRadianceSnapshot, DDGI_AUTHORED_SKY_MODEL_IDENTITY};
use anyhow::{ensure, Context, Result};
use re_flora_vkn::{Buffer, BufferUsage, CommandBuffer, Extent2D, MemoryLocation};
use std::io::Write;
use std::path::{Path, PathBuf};

const CAPTURE_MAGIC: &[u8; 8] = b"RFIRR001";
const CAPTURE_VERSION: u32 = 5;
const CAPTURE_CHANNEL_COUNT: u32 = 4;
const CAPTURE_PLANE_COUNT: u32 = 3;
const CAPTURE_HEADER_BYTE_COUNT: usize = 124;
const DDGI_BACKEND_ID: u32 = 1;
const CAPTURE_STAGE_SEED_SKY: u32 = 1;
const CAPTURE_STAGE_SINGLE_BOUNCE: u32 = 2;
const CAPTURE_STAGE_FEEDBACK: u32 = 3;
const CAPTURE_STAGE_CONVERGED: u32 = 4;
const CAPTURE_STAGE_NON_CONVERGED: u32 = 5;
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
    transport_stage: u32,
    transport_iteration: u32,
    source_stage: u32,
    source_iteration: u32,
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
            transport_stage: encode_transport_stage(field.stage()),
            transport_iteration: field.iteration(),
            source_stage: source
                .map(|source| encode_transport_stage(source.stage()))
                .unwrap_or(CAPTURE_UNKNOWN_U32),
            source_iteration: source
                .map(|source| source.iteration())
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
            self.transport_stage,
            self.transport_iteration,
            self.source_stage,
            self.source_iteration,
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

fn encode_transport_stage(stage: DdgiFieldStage) -> u32 {
    match stage {
        DdgiFieldStage::SeedSky => CAPTURE_STAGE_SEED_SKY,
        DdgiFieldStage::SingleBounce => CAPTURE_STAGE_SINGLE_BOUNCE,
        DdgiFieldStage::Feedback => CAPTURE_STAGE_FEEDBACK,
        DdgiFieldStage::Converged => CAPTURE_STAGE_CONVERGED,
        DdgiFieldStage::NonConverged => CAPTURE_STAGE_NON_CONVERGED,
    }
}

pub(super) struct EnvironmentIrradianceCaptureReadback {
    path: String,
    extent: Extent2D,
    spacing_voxels: u32,
    debug_view: DdgiDebugView,
    metadata: CaptureMetadata,
    radiance_evidence: Option<RadianceCaptureEvidence>,
    buffer: Buffer,
}

impl EnvironmentIrradianceCaptureReadback {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) fn radiance_checkpoint(&self) -> Option<RadianceCaptureCheckpoint> {
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
        "{{\"field_serial\":{},\"geometry_revision\":{},\"radiance_revision\":{},\"spacing_voxels\":{},\"transport_stage\":\"{:?}\",\"transport_iteration\":{},\"source_field_serial\":{},\"source_radiance_revision\":{}}}",
        key.serial(),
        key.geometry_revision(),
        key.radiance_revision(),
        key.spacing_voxels(),
        key.stage(),
        key.iteration(),
        source.map_or(0, |source| source.serial()),
        source.map_or(0, |source| source.radiance_revision()),
    )
}

fn snapshot_json(snapshot: Option<DdgiRadianceSnapshot>) -> String {
    let Some(snapshot) = snapshot else {
        return "null".to_owned();
    };
    format!(
        "{{\"sun_direction\":[{},{},{}],\"sun_color\":[{},{},{}],\"sun_luminance\":{},\"terrain_ray_origin_offset_world\":{}}}",
        snapshot.sun_direction.x,
        snapshot.sun_direction.y,
        snapshot.sun_direction.z,
        snapshot.sun_color.x,
        snapshot.sun_color.y,
        snapshot.sun_color.z,
        snapshot.sun_luminance,
        snapshot.terrain_ray_origin_offset_world,
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

impl App {
    pub(super) fn prepare_environment_irradiance_capture_readback(
        &self,
        base_path: String,
    ) -> Result<EnvironmentIrradianceCaptureReadback> {
        let radiance_request = self
            .environment_lighting_test_scene
            .as_ref()
            .and_then(|scene| scene.radiance_capture_request());
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

        let extent = self.tracer.environment_irradiance_capture_extent();
        let byte_count = u64::from(extent.width)
            * u64::from(extent.height)
            * std::mem::size_of::<[f32; 4]>() as u64
            * u64::from(CAPTURE_PLANE_COUNT);
        let checkpoint = self
            .tracer
            .ddgi_capture_checkpoint()
            .context("cannot capture DDGI before the requested field checkpoint is resident")?;
        let metadata =
            CaptureMetadata::from_checkpoint(checkpoint, DDGI_AUTHORED_SKY_MODEL_IDENTITY)?;
        let radiance_evidence = radiance_request
            .map(|request| {
                let capture_frame = self.time_info.total_frame_count();
                if let Some(mutation_frame) = request.mutation_frame {
                    ensure!(
                        capture_frame == mutation_frame + 1,
                        "{} capture frame {} is not mutation frame {} + 1",
                        request.checkpoint.label(),
                        capture_frame,
                        mutation_frame,
                    );
                }
                let status = self.tracer.ddgi_status();
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
                    live_radiance_revision: self.tracer.ddgi_live_radiance_revision(),
                    live_snapshot: self
                        .tracer
                        .ddgi_live_radiance_snapshot()
                        .context("radiance evidence requires the live renderer snapshot")?,
                    latest_radiance_revision: self.tracer.ddgi_latest_radiance_revision(),
                    active_field,
                    building_field: status.builder().building_field,
                    builder_latched_radiance_revision: status.builder().radiance_revision,
                    builder_latched_snapshot: self.tracer.ddgi_builder_radiance_snapshot(),
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
        let allocator = self
            .tracer
            .get_screen_output_tex()
            .get_image()
            .get_allocator()
            .clone();
        let buffer = Buffer::new_sized(
            self.vulkan_ctx.device().clone(),
            allocator,
            BufferUsage::transfer_dst(),
            MemoryLocation::GpuToCpu,
            byte_count,
        );
        Ok(EnvironmentIrradianceCaptureReadback {
            path,
            extent,
            spacing_voxels: checkpoint.field.field().spacing_voxels(),
            debug_view: self.tracer.ddgi_debug_view(),
            metadata,
            radiance_evidence,
            buffer,
        })
    }

    pub(super) fn record_environment_irradiance_capture_readback(
        &self,
        cmdbuf: &CommandBuffer,
        readback: &EnvironmentIrradianceCaptureReadback,
    ) {
        self.tracer
            .record_environment_irradiance_capture_readback(cmdbuf, &readback.buffer);
    }

    pub(super) fn write_environment_irradiance_capture_readback(
        readback: EnvironmentIrradianceCaptureReadback,
    ) -> Result<()> {
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
            "[ENV_IRRADIANCE_CAPTURE] saved path={} extent={}x{} backend={} spacing_voxels={} view={} samples={} geometry_revision={} radiance_revision={} radiance_model_identity={} build_token_serial={} field_serial={} transport_stage={} transport_iteration={} source_stage={} source_iteration={} source_field_serial={} source_radiance_revision={} publication_state={} batch_order={} max_abs_delta={} max_rel_delta={} nonfinite_count={} valid_count={} format=float4-linear-rgb-hit+float4-world-xyz-exact-direct-sun-visibility+float4-direct-light-rgb-hit",
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
            readback.metadata.transport_stage,
            readback.metadata.transport_iteration,
            readback.metadata.source_stage,
            readback.metadata.source_iteration,
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
        DdgiFieldStage,
    };

    #[test]
    fn capture_header_is_fixed_width_and_self_describing() {
        assert_eq!(CAPTURE_MAGIC.len(), 8);
        assert_eq!(CAPTURE_VERSION, 5);
        assert_eq!(CAPTURE_CHANNEL_COUNT, 4);
        assert_eq!(CAPTURE_PLANE_COUNT, 3);
        assert_eq!(CAPTURE_HEADER_BYTE_COUNT, 124);
    }

    #[test]
    fn capture_metadata_uses_authoritative_published_terminal_identity() {
        let token = DdgiBuildToken::for_test(9001, 41, 16, DdgiBuildKind::Terrain);
        let source = DdgiFieldKey::new(88, 41, 17, 16, DdgiFieldStage::Feedback, 5).unwrap();
        let published = DdgiFieldIdentity::new(
            DdgiFieldKey::new(89, 41, 17, 16, DdgiFieldStage::Converged, 6).unwrap(),
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
        assert_eq!(metadata.transport_stage, CAPTURE_STAGE_CONVERGED);
        assert_eq!(metadata.transport_iteration, 6);
        assert_eq!(metadata.source_stage, CAPTURE_STAGE_FEEDBACK);
        assert_eq!(metadata.source_iteration, 5);
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

        assert!(tracer.contains("sampleDdgiUnpublishedCaptureEnvironment("));
        assert!(tracer.contains("environmentCaptureIrradiance = captureResult.irradiance"));
        assert!(tracer.contains("environmentCaptureIrradiance, terrainHit"));
        assert!(tracer.contains("color = environmentIrradiance * albedo"));
        assert!(query.contains("[[vk::binding(27, 0)]]\nSampler2D ddgi_irradiance_atlas"));
        assert!(query.contains("[[vk::binding(34, 0)]]\nSampler2D ddgi_capture_irradiance_atlas"));
        let capture_query = query
            .split_once("public DdgiQueryResult sampleDdgiUnpublishedCaptureEnvironment(")
            .expect("capture-only DDGI query must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("capture-only query must remain isolated")
            .0;
        assert!(capture_query.contains("query.ready = 1u"));
        assert!(capture_query.contains("query.invalidation_enabled = 0u"));
        assert!(capture_query.contains("ddgi_capture_irradiance_atlas"));
    }
}
