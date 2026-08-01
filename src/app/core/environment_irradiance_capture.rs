use super::App;
use crate::ddgi::{DdgiDebugView, DdgiTransportStage};
use crate::environment_lighting::DDGI_AUTHORED_SKY_MODEL_IDENTITY;
use crate::tracer::DdgiRuntimeStatus;
use anyhow::{Context, Result};
use re_flora_vkn::{Buffer, BufferUsage, CommandBuffer, Extent2D, MemoryLocation};
use std::io::Write;
use std::path::Path;

const CAPTURE_MAGIC: &[u8; 8] = b"RFIRR001";
const CAPTURE_VERSION: u32 = 3;
const CAPTURE_CHANNEL_COUNT: u32 = 4;
const CAPTURE_PLANE_COUNT: u32 = 2;
const CAPTURE_HEADER_BYTE_COUNT: usize = 108;
const DDGI_BACKEND_ID: u32 = 1;
const CAPTURE_UNKNOWN_U32: u32 = u32::MAX;
const CAPTURE_UNKNOWN_U64: u64 = u64::MAX;
const CAPTURE_UNKNOWN_DELTA: f32 = -1.0;
const CAPTURE_STAGE_SEED_SKY: u32 = 1;
const CAPTURE_STAGE_SINGLE_BOUNCE: u32 = 2;
const CAPTURE_STAGE_FEEDBACK: u32 = 3;
const CAPTURE_STAGE_CONVERGED: u32 = 4;
const CAPTURE_STAGE_NON_CONVERGED: u32 = 5;
const CAPTURE_PUBLICATION_UNPUBLISHED: u32 = 0;
const CAPTURE_PUBLICATION_PUBLISHED: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CaptureConvergenceMetadata {
    pub max_abs_delta: Option<f32>,
    pub max_rel_delta: Option<f32>,
    pub nonfinite_count: Option<u32>,
    pub valid_count: Option<u32>,
}

impl CaptureConvergenceMetadata {
    pub const fn unknown() -> Self {
        Self {
            max_abs_delta: None,
            max_rel_delta: None,
            nonfinite_count: None,
            valid_count: None,
        }
    }
}

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
        radiance_model_identity: u64,
        convergence: CaptureConvergenceMetadata,
    ) -> Self {
        let (transport_stage, transport_iteration) =
            encode_transport_stage(runtime.active_transport_stage);
        let source = runtime
            .active_transport_stage
            .and_then(DdgiTransportStage::immutable_source);
        let (source_stage, source_iteration) = encode_transport_stage(source);
        Self {
            geometry_revision: runtime
                .active_terrain_revision
                .unwrap_or(CAPTURE_UNKNOWN_U32),
            radiance_revision: runtime
                .active_radiance_revision
                .unwrap_or(CAPTURE_UNKNOWN_U32),
            radiance_model_identity,
            token_serial: runtime.active_token_serial.unwrap_or(CAPTURE_UNKNOWN_U64),
            transport_stage,
            transport_iteration,
            source_stage,
            source_iteration,
            source_identity: source
                .and(runtime.active_token_serial)
                .unwrap_or(CAPTURE_UNKNOWN_U64),
            publication_state: if runtime.active_transport_stage.is_some() {
                CAPTURE_PUBLICATION_PUBLISHED
            } else {
                CAPTURE_PUBLICATION_UNPUBLISHED
            },
            max_abs_delta: convergence.max_abs_delta.unwrap_or(CAPTURE_UNKNOWN_DELTA),
            max_rel_delta: convergence.max_rel_delta.unwrap_or(CAPTURE_UNKNOWN_DELTA),
            nonfinite_count: convergence.nonfinite_count.unwrap_or(CAPTURE_UNKNOWN_U32),
            valid_count: convergence.valid_count.unwrap_or(CAPTURE_UNKNOWN_U32),
        }
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

fn encode_transport_stage(stage: Option<DdgiTransportStage>) -> (u32, u32) {
    match stage {
        None => (CAPTURE_UNKNOWN_U32, CAPTURE_UNKNOWN_U32),
        Some(DdgiTransportStage::SeedSky) => (CAPTURE_STAGE_SEED_SKY, 0),
        Some(DdgiTransportStage::SingleBounce) => (CAPTURE_STAGE_SINGLE_BOUNCE, 1),
        Some(DdgiTransportStage::Feedback { iteration }) => (CAPTURE_STAGE_FEEDBACK, iteration),
        Some(DdgiTransportStage::Converged) => (CAPTURE_STAGE_CONVERGED, CAPTURE_UNKNOWN_U32),
        Some(DdgiTransportStage::NonConverged) => {
            (CAPTURE_STAGE_NON_CONVERGED, CAPTURE_UNKNOWN_U32)
        }
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
        let metadata = CaptureMetadata::from_runtime(
            runtime,
            DDGI_AUTHORED_SKY_MODEL_IDENTITY,
            CaptureConvergenceMetadata::unknown(),
        );
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
            spacing_voxels: runtime.active_spacing_voxels,
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
    use crate::ddgi::{DdgiRefreshState, DdgiTransportStage, DdgiVolumeStage};
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
    fn capture_metadata_uses_real_active_identity_and_unknown_convergence() {
        let status = DdgiRuntimeStatus {
            active_token_serial: Some(9001),
            active_terrain_revision: Some(41),
            active_spacing_voxels: 16,
            active_stage: DdgiVolumeStage::Ready,
            active_transport_stage: Some(DdgiTransportStage::SingleBounce),
            active_radiance_revision: Some(17),
            target_terrain_revision: Some(41),
            staging_token_serial: None,
            staging_kind: None,
            staging_stage: None,
            staging_transport_stage: None,
            staging_building_transport_stage: None,
            staging_radiance_revision: None,
            staging_terrain_revision: None,
            staging_spacing_voxels: None,
            staging_filtered_probe_count: 0,
            staging_probe_count: 0,
            coordinator_state: DdgiRefreshState::Idle,
            queued_density_spacing_voxels: None,
            full_domain_invalidation_fail_closed: false,
        };

        let metadata = CaptureMetadata::from_runtime(
            status,
            crate::environment_lighting::DDGI_AUTHORED_SKY_MODEL_IDENTITY,
            CaptureConvergenceMetadata::unknown(),
        );

        assert_eq!(metadata.geometry_revision, 41);
        assert_eq!(metadata.radiance_revision, 17);
        assert_eq!(
            metadata.radiance_model_identity,
            crate::environment_lighting::DDGI_AUTHORED_SKY_MODEL_IDENTITY
        );
        assert_eq!(metadata.token_serial, 9001);
        assert_eq!(metadata.transport_stage, CAPTURE_STAGE_SINGLE_BOUNCE);
        assert_eq!(metadata.transport_iteration, 1);
        assert_eq!(metadata.source_stage, CAPTURE_STAGE_SEED_SKY);
        assert_eq!(metadata.source_iteration, 0);
        assert_eq!(metadata.source_identity, 9001);
        assert_eq!(metadata.publication_state, CAPTURE_PUBLICATION_PUBLISHED);
        assert_eq!(metadata.max_abs_delta, CAPTURE_UNKNOWN_DELTA);
        assert_eq!(metadata.max_rel_delta, CAPTURE_UNKNOWN_DELTA);
        assert_eq!(metadata.nonfinite_count, CAPTURE_UNKNOWN_U32);
        assert_eq!(metadata.valid_count, CAPTURE_UNKNOWN_U32);

        let rebuilding_metadata = CaptureMetadata::from_runtime(
            DdgiRuntimeStatus {
                active_stage: DdgiVolumeStage::Rebuilding,
                ..status
            },
            crate::environment_lighting::DDGI_AUTHORED_SKY_MODEL_IDENTITY,
            CaptureConvergenceMetadata::unknown(),
        );
        assert_eq!(
            rebuilding_metadata.publication_state, CAPTURE_PUBLICATION_PUBLISHED,
            "a published transport field remains consumable during its next iteration"
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
