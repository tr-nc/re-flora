use super::App;
use crate::ddgi::{
    DdgiAtlasValidationStats, DdgiBuildToken, DdgiDebugView, DdgiFieldIdentity, DdgiFieldStage,
};
use crate::environment_lighting::DDGI_AUTHORED_SKY_MODEL_IDENTITY;
use crate::tracer::DdgiRuntimeStatus;
use anyhow::{ensure, Context, Result};
use re_flora_vkn::{Buffer, BufferUsage, CommandBuffer, Extent2D, MemoryLocation};
use std::io::Write;
use std::path::Path;

const CAPTURE_MAGIC: &[u8; 8] = b"RFIRR001";
const CAPTURE_VERSION: u32 = 4;
const CAPTURE_CHANNEL_COUNT: u32 = 4;
const CAPTURE_PLANE_COUNT: u32 = 2;
const CAPTURE_HEADER_BYTE_COUNT: usize = 124;
const DDGI_BACKEND_ID: u32 = 1;
const CAPTURE_STAGE_SEED_SKY: u32 = 1;
const CAPTURE_STAGE_SINGLE_BOUNCE: u32 = 2;
const CAPTURE_STAGE_FEEDBACK: u32 = 3;
const CAPTURE_STAGE_CONVERGED: u32 = 4;
const CAPTURE_STAGE_NON_CONVERGED: u32 = 5;
const CAPTURE_PUBLICATION_PUBLISHED: u32 = 1;
const CAPTURE_BATCH_ORDER_FORWARD: u32 = 0;
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
    fn from_runtime(
        runtime: DdgiRuntimeStatus,
        build_token: DdgiBuildToken,
        published: DdgiFieldIdentity,
        validation: DdgiAtlasValidationStats,
        radiance_model_identity: u64,
    ) -> Result<Self> {
        let field = published.field();
        ensure!(
            runtime.active_token_serial == Some(build_token.serial())
                && runtime.active_terrain_revision == Some(field.geometry_revision())
                && runtime.active_radiance_revision == Some(field.radiance_revision())
                && runtime.active_spacing_voxels == field.spacing_voxels()
                && runtime.active_published_field == Some(published),
            "DDGI runtime status does not match the authoritative published field: runtime={runtime:?} published={published:?}"
        );
        ensure!(
            build_token.terrain_revision() == field.geometry_revision()
                && build_token.spacing_voxels() == field.spacing_voxels(),
            "DDGI build token does not own the published field: token={build_token:?} published={published:?}"
        );
        let source = published.source();
        Ok(Self {
            geometry_revision: field.geometry_revision(),
            radiance_revision: field.radiance_revision(),
            radiance_model_identity,
            build_token_serial: build_token.serial(),
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
            publication_state: CAPTURE_PUBLICATION_PUBLISHED,
            batch_order: CAPTURE_BATCH_ORDER_FORWARD,
            max_abs_delta: validation.max_absolute_rgb_delta,
            max_rel_delta: validation.max_relative_rgb_delta,
            nonfinite_count: validation.non_finite_count,
            valid_count: validation.valid_texel_count,
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
        let runtime = self.tracer.ddgi_runtime_status();
        let active = self.tracer.environment_probe_status();
        let build_token = active
            .build_token
            .context("cannot capture DDGI without an active build token")?;
        let published = active
            .published_field
            .context("cannot capture DDGI before a complete field is published")?;
        let validation = active
            .last_atlas_validation
            .context("cannot capture DDGI without validation for the published iteration")?;
        let metadata = CaptureMetadata::from_runtime(
            runtime,
            build_token,
            published,
            validation,
            DDGI_AUTHORED_SKY_MODEL_IDENTITY,
        )?;
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
            spacing_voxels: published.field().spacing_voxels(),
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
            "[ENV_IRRADIANCE_CAPTURE] saved path={} extent={}x{} backend={} spacing_voxels={} view={} samples={} geometry_revision={} radiance_revision={} radiance_model_identity={} build_token_serial={} field_serial={} transport_stage={} transport_iteration={} source_stage={} source_iteration={} source_field_serial={} source_radiance_revision={} publication_state={} batch_order={} max_abs_delta={} max_rel_delta={} nonfinite_count={} valid_count={} format=float4-linear-rgb-hit+float4-world-xyz-exact-direct-sun-visibility",
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
        DdgiAtlasValidationStats, DdgiBuildKind, DdgiBuildToken, DdgiFieldIdentity, DdgiFieldKey,
        DdgiFieldStage, DdgiRefreshState, DdgiVolumeStage,
    };
    use crate::tracer::DdgiRuntimeStatus;

    #[test]
    fn capture_header_is_fixed_width_and_self_describing() {
        assert_eq!(CAPTURE_MAGIC.len(), 8);
        assert_eq!(CAPTURE_VERSION, 4);
        assert_eq!(CAPTURE_CHANNEL_COUNT, 4);
        assert_eq!(CAPTURE_PLANE_COUNT, 2);
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
        let status = DdgiRuntimeStatus {
            active_token_serial: Some(9001),
            active_terrain_revision: Some(41),
            active_spacing_voxels: 16,
            active_stage: DdgiVolumeStage::Ready,
            active_published_field: Some(published),
            active_radiance_revision: Some(17),
            target_terrain_revision: Some(41),
            staging_token_serial: None,
            staging_kind: None,
            staging_stage: None,
            staging_complete_field: None,
            staging_building_field: None,
            staging_radiance_revision: None,
            staging_terrain_revision: None,
            staging_spacing_voxels: None,
            staging_published_field: None,
            staging_filtered_probe_count: 0,
            staging_probe_count: 0,
            coordinator_state: DdgiRefreshState::Idle,
            queued_density_spacing_voxels: None,
            full_domain_invalidation_fail_closed: false,
        };

        let metadata = CaptureMetadata::from_runtime(
            status,
            token,
            published,
            validation,
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
        assert_eq!(metadata.max_abs_delta, 0.0125);
        assert_eq!(metadata.max_rel_delta, 0.025);
        assert_eq!(metadata.nonfinite_count, 0);
        assert_eq!(metadata.valid_count, 314_432);

        let mismatch = CaptureMetadata::from_runtime(
            DdgiRuntimeStatus {
                active_radiance_revision: Some(18),
                ..status
            },
            token,
            published,
            validation,
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
        assert!(tracer.contains("environmentIrradiance, terrainHit"));
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
}
