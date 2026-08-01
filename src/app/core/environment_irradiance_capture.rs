use super::App;
use crate::ddgi::{
    DdgiAtlasValidationStats, DdgiBuildKind, DdgiDebugView, DdgiTransportFieldIdentity,
    DdgiTransportIterationIdentity, DdgiTransportStage,
};
use crate::environment_lighting::DDGI_AUTHORED_SKY_MODEL_IDENTITY;
use crate::tracer::DdgiRuntimeStatus;
use anyhow::{ensure, Context, Result};
use re_flora_vkn::{Buffer, BufferUsage, CommandBuffer, Extent2D, MemoryLocation};
use std::io::Write;
use std::path::Path;

const CAPTURE_MAGIC: &[u8; 8] = b"RFIRR001";
const CAPTURE_VERSION: u32 = 3;
const CAPTURE_CHANNEL_COUNT: u32 = 4;
const CAPTURE_PLANE_COUNT: u32 = 2;
const CAPTURE_HEADER_BYTE_COUNT: usize = 108;
const DDGI_BACKEND_ID: u32 = 1;
const CAPTURE_STAGE_SEED_SKY: u32 = 1;
const CAPTURE_STAGE_SINGLE_BOUNCE: u32 = 2;
const CAPTURE_STAGE_FEEDBACK: u32 = 3;
const CAPTURE_STAGE_CONVERGED: u32 = 4;
const CAPTURE_STAGE_NON_CONVERGED: u32 = 5;
const CAPTURE_PUBLICATION_PUBLISHED: u32 = 1;
const CAPTURE_FNV1A64_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const CAPTURE_FNV1A64_PRIME: u64 = 0x100000001b3;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CaptureMetadata {
    geometry_revision: u32,
    radiance_revision: u32,
    radiance_model_identity: u64,
    token_serial: u64,
    transport_stage: u32,
    transport_iteration: u32,
    source_stage: u32,
    source_iteration: u32,
    source_identity: u64,
    publication_state: u32,
    max_abs_delta: f32,
    max_rel_delta: f32,
    nonfinite_count: u32,
    valid_count: u32,
}

impl CaptureMetadata {
    fn from_runtime(
        runtime: DdgiRuntimeStatus,
        published: DdgiTransportIterationIdentity,
        validation: DdgiAtlasValidationStats,
        radiance_model_identity: u64,
    ) -> Result<Self> {
        let token = published
            .build_token
            .context("published DDGI iteration has no build token")?;
        let transport = runtime
            .active_transport_stage
            .context("published DDGI runtime has no transport stage")?;
        let source = published
            .source
            .context("published DDGI iteration has no immutable source field")?;
        ensure!(
            runtime.active_token_serial == Some(token.serial())
                && runtime.active_terrain_revision == Some(published.geometry_revision)
                && runtime.active_radiance_revision == Some(published.radiance_revision)
                && runtime.active_spacing_voxels == published.spacing_voxels
                && runtime.active_published_slot == Some(published.destination.slot)
                && runtime.active_published_iteration == Some(published.iteration),
            "DDGI runtime status does not match the authoritative published iteration: runtime={runtime:?} published={published:?}"
        );
        ensure!(
            published.destination.build_token == published.build_token
                && published.destination.geometry_revision == published.geometry_revision
                && published.destination.radiance_revision == published.radiance_revision
                && published.destination.spacing_voxels == published.spacing_voxels
                && published.destination.stage == published.stage
                && published.destination.iteration == published.iteration,
            "DDGI published destination identity is inconsistent: {published:?}"
        );
        match transport {
            DdgiTransportStage::Converged { iteration }
            | DdgiTransportStage::NonConverged { iteration } => ensure!(
                published.stage == DdgiTransportStage::Feedback { iteration }
                    && published.iteration == iteration,
                "terminal DDGI runtime stage does not name its published feedback iteration: runtime={transport:?} published={published:?}"
            ),
            _ => ensure!(
                transport == published.stage && transport.iteration() == published.iteration,
                "DDGI runtime transport stage does not match published iteration: runtime={transport:?} published={published:?}"
            ),
        }
        let (transport_stage, transport_iteration) = encode_transport_stage(transport);
        let (source_stage, source_iteration) = encode_transport_stage(source.stage);
        Ok(Self {
            geometry_revision: published.geometry_revision,
            radiance_revision: published.radiance_revision,
            radiance_model_identity,
            token_serial: token.serial(),
            transport_stage,
            transport_iteration,
            source_stage,
            source_iteration,
            source_identity: source_field_fingerprint(source),
            publication_state: CAPTURE_PUBLICATION_PUBLISHED,
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
        writer.write_all(&self.token_serial.to_le_bytes())?;
        for value in [
            self.transport_stage,
            self.transport_iteration,
            self.source_stage,
            self.source_iteration,
        ] {
            writer.write_all(&value.to_le_bytes())?;
        }
        writer.write_all(&self.source_identity.to_le_bytes())?;
        writer.write_all(&self.publication_state.to_le_bytes())?;
        writer.write_all(&self.max_abs_delta.to_le_bytes())?;
        writer.write_all(&self.max_rel_delta.to_le_bytes())?;
        writer.write_all(&self.nonfinite_count.to_le_bytes())?;
        writer.write_all(&self.valid_count.to_le_bytes())?;
        Ok(())
    }
}

fn encode_transport_stage(stage: DdgiTransportStage) -> (u32, u32) {
    match stage {
        DdgiTransportStage::SeedSky => (CAPTURE_STAGE_SEED_SKY, 0),
        DdgiTransportStage::SingleBounce => (CAPTURE_STAGE_SINGLE_BOUNCE, 1),
        DdgiTransportStage::Feedback { iteration } => (CAPTURE_STAGE_FEEDBACK, iteration),
        DdgiTransportStage::Converged { iteration } => (CAPTURE_STAGE_CONVERGED, iteration),
        DdgiTransportStage::NonConverged { iteration } => (CAPTURE_STAGE_NON_CONVERGED, iteration),
    }
}

/// Stable on-disk fingerprint of every logical source-field identity component. Atlas slot is
/// included because ping-pong ownership is part of the immutable source named by an iteration.
fn source_field_fingerprint(field: DdgiTransportFieldIdentity) -> u64 {
    let mut hash = CAPTURE_FNV1A64_OFFSET_BASIS;
    let (token_present, token_serial, token_terrain, token_spacing, token_kind) =
        match field.build_token {
            Some(token) => (
                1,
                token.serial(),
                token.terrain_revision(),
                token.spacing_voxels(),
                match token.kind() {
                    DdgiBuildKind::Terrain => 1,
                    DdgiBuildKind::Density => 2,
                },
            ),
            None => (0, 0, 0, 0, 0),
        };
    let (stage, stage_iteration) = encode_transport_stage(field.stage);
    for value in [
        token_present,
        token_serial,
        u64::from(token_terrain),
        u64::from(token_spacing),
        token_kind,
        u64::from(field.geometry_revision),
        u64::from(field.radiance_revision),
        u64::from(field.spacing_voxels),
        u64::from(stage),
        u64::from(stage_iteration),
        u64::from(field.iteration),
        field.slot as u64,
    ] {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(CAPTURE_FNV1A64_PRIME);
        }
    }
    hash
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
        let published = active
            .published_iteration
            .context("cannot capture DDGI before a complete iteration is published")?;
        let validation = active
            .last_atlas_validation
            .context("cannot capture DDGI without validation for the published iteration")?;
        let metadata = CaptureMetadata::from_runtime(
            runtime,
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
            spacing_voxels: published.spacing_voxels,
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
            "[ENV_IRRADIANCE_CAPTURE] saved path={} extent={}x{} backend={} spacing_voxels={} view={} samples={} geometry_revision={} radiance_revision={} radiance_model_identity={} token_serial={} transport_stage={} transport_iteration={} source_stage={} source_iteration={} source_identity={} publication_state={} max_abs_delta={} max_rel_delta={} nonfinite_count={} valid_count={} format=float4-linear-rgb-hit+float4-world-xyz-exact-direct-sun-visibility",
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
            readback.metadata.token_serial,
            readback.metadata.transport_stage,
            readback.metadata.transport_iteration,
            readback.metadata.source_stage,
            readback.metadata.source_iteration,
            readback.metadata.source_identity,
            readback.metadata.publication_state,
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
        DdgiAtlasValidationStats, DdgiBuildKind, DdgiBuildToken, DdgiIrradianceSlot,
        DdgiRefreshState, DdgiTransportFieldIdentity, DdgiTransportIterationIdentity,
        DdgiTransportStage, DdgiVolumeStage,
    };
    use crate::tracer::DdgiRuntimeStatus;

    #[test]
    fn capture_header_is_fixed_width_and_self_describing() {
        assert_eq!(CAPTURE_MAGIC.len(), 8);
        assert_eq!(CAPTURE_VERSION, 3);
        assert_eq!(CAPTURE_CHANNEL_COUNT, 4);
        assert_eq!(CAPTURE_PLANE_COUNT, 2);
        assert_eq!(CAPTURE_HEADER_BYTE_COUNT, 108);
    }

    #[test]
    fn capture_metadata_uses_authoritative_published_terminal_identity() {
        let token = DdgiBuildToken::for_test(9001, 41, 16, DdgiBuildKind::Terrain);
        let source = DdgiTransportFieldIdentity {
            build_token: Some(token),
            geometry_revision: 41,
            radiance_revision: 17,
            spacing_voxels: 16,
            stage: DdgiTransportStage::Feedback { iteration: 5 },
            iteration: 5,
            slot: DdgiIrradianceSlot::Atlas0,
        };
        let destination = DdgiTransportFieldIdentity {
            stage: DdgiTransportStage::Feedback { iteration: 6 },
            iteration: 6,
            slot: DdgiIrradianceSlot::Atlas1,
            ..source
        };
        let published = DdgiTransportIterationIdentity {
            build_token: Some(token),
            geometry_revision: 41,
            radiance_revision: 17,
            spacing_voxels: 16,
            stage: DdgiTransportStage::Feedback { iteration: 6 },
            iteration: 6,
            source: Some(source),
            destination,
        };
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
            active_transport_stage: Some(DdgiTransportStage::Converged { iteration: 6 }),
            active_radiance_revision: Some(17),
            active_published_slot: Some(DdgiIrradianceSlot::Atlas1),
            active_published_iteration: Some(6),
            target_terrain_revision: Some(41),
            staging_token_serial: None,
            staging_kind: None,
            staging_stage: None,
            staging_transport_stage: None,
            staging_building_transport_stage: None,
            staging_radiance_revision: None,
            staging_terrain_revision: None,
            staging_spacing_voxels: None,
            staging_published_slot: None,
            staging_published_iteration: None,
            staging_filtered_probe_count: 0,
            staging_probe_count: 0,
            coordinator_state: DdgiRefreshState::Idle,
            queued_density_spacing_voxels: None,
            full_domain_invalidation_fail_closed: false,
        };

        let metadata = CaptureMetadata::from_runtime(
            status,
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
        assert_eq!(metadata.token_serial, 9001);
        assert_eq!(metadata.transport_stage, CAPTURE_STAGE_CONVERGED);
        assert_eq!(metadata.transport_iteration, 6);
        assert_eq!(metadata.source_stage, CAPTURE_STAGE_FEEDBACK);
        assert_eq!(metadata.source_iteration, 5);
        assert_eq!(metadata.source_identity, source_field_fingerprint(source));
        assert_ne!(metadata.source_identity, 9001);
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
