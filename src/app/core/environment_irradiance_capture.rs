use super::App;
use crate::ddgi::{DdgiCaptureCheckpoint, DdgiDebugView, DdgiFieldStage};
use crate::environment_lighting::DDGI_AUTHORED_SKY_MODEL_IDENTITY;
use anyhow::{ensure, Context, Result};
use re_flora_vkn::{Buffer, BufferUsage, CommandBuffer, Extent2D, MemoryLocation};
use std::io::Write;
use std::path::Path;

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
    buffer: Buffer,
}

impl App {
    pub(super) fn prepare_environment_irradiance_capture_readback(
        &self,
        path: String,
    ) -> Result<EnvironmentIrradianceCaptureReadback> {
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
        for contract in [
            "1.0 / 256.0",
            "0.25",
            "0.75",
            "1.0 / 512.0",
            "receiver.center_position, receiver.normal, sunDirection",
            "shadowHit.is_hit ? 0.0 : 1.0",
        ] {
            assert!(
                exact_shadow.contains(contract),
                "capture exact-shadow helper lost contract `{contract}`"
            );
            assert!(
                probe_trace.contains(contract),
                "probe exact-shadow path lost contract `{contract}`"
            );
        }
    }

    #[test]
    fn capture_shader_writes_direct_light_without_ddgi_environment() {
        let tracer = include_str!("../../../shader/slang/tracer.slang");

        let direct_lighting = tracer
            .split_once("float3 directLighting(")
            .expect("terrain direct-light function must exist")
            .1
            .split_once("float depthFromWorldPosition(")
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
