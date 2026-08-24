use super::resources::LocalLightingResources;
use crate::generated::gpu_structs::{
    LocalLightVisibilityDiagnosticInfo, LocalLightVisibilityDiagnosticResult,
};
use crate::lighting::LightId;
use anyhow::{ensure, Result};
use glam::Vec3;
use re_flora_vkn::{BufferUse, CommandBuffer, ComputePipeline, Extent3D};

const LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_ABI_VERSION: u32 = 3;
const LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_SPECIFIC: u32 = 1;
const LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_AGGREGATE: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalLightVisibilityDiagnosticTarget {
    Specific(LightId),
    Aggregate,
}

impl LocalLightVisibilityDiagnosticTarget {
    fn gpu_identity(self) -> (u32, u32, u32) {
        match self {
            Self::Specific(id) => (
                LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_SPECIFIC,
                id.slot(),
                id.generation(),
            ),
            Self::Aggregate => (LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_AGGREGATE, 0, 0),
        }
    }

    pub(crate) const fn light_id(self) -> Option<LightId> {
        match self {
            Self::Specific(id) => Some(id),
            Self::Aggregate => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalLightVisibilityDiagnosticRequest {
    pub request_serial: u32,
    pub geometry_revision: u32,
    pub source_revision: u64,
    pub target: LocalLightVisibilityDiagnosticTarget,
    pub receiver_position: Vec3,
    pub receiver_normal: Vec3,
    pub ray_origin_offset_world: f32,
}

impl LocalLightVisibilityDiagnosticRequest {
    fn gpu_info(self) -> LocalLightVisibilityDiagnosticInfo {
        let (mode, light_id_slot, light_id_generation) = self.target.gpu_identity();
        LocalLightVisibilityDiagnosticInfo {
            abi_version: LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_ABI_VERSION,
            request_serial: self.request_serial,
            geometry_revision: self.geometry_revision,
            source_revision_low: self.source_revision as u32,
            source_revision_high: (self.source_revision >> 32) as u32,
            light_id_slot,
            light_id_generation,
            mode,
            receiver_position_and_origin_offset: [
                self.receiver_position.x,
                self.receiver_position.y,
                self.receiver_position.z,
                self.ray_origin_offset_world,
            ],
            receiver_normal_and_reserved: [
                self.receiver_normal.x,
                self.receiver_normal.y,
                self.receiver_normal.z,
                0.0,
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalLightVisibilityDiagnosticEvidence {
    pub request: LocalLightVisibilityDiagnosticRequest,
    pub identity_matches: bool,
    pub selected_light_index: Option<u32>,
    pub irradiance: Vec3,
    pub candidates: u32,
    pub visible: u32,
    pub occluded: u32,
    pub irradiance_luma_q8: u32,
}

impl LocalLightVisibilityDiagnosticEvidence {
    pub fn irradiance_luma(self) -> f32 {
        self.irradiance_luma_q8 as f32 / 256.0
    }
}

fn decode_selected_light_index(
    target: LocalLightVisibilityDiagnosticTarget,
    identity_matches: u32,
    selected_light_index: u32,
) -> Result<(bool, Option<u32>)> {
    ensure!(
        identity_matches <= 1,
        "local-light visibility diagnostic identity flag is invalid"
    );
    ensure!(
        match target {
            LocalLightVisibilityDiagnosticTarget::Specific(_) => {
                (identity_matches == 1 && selected_light_index != u32::MAX)
                    || (identity_matches == 0 && selected_light_index == u32::MAX)
            }
            LocalLightVisibilityDiagnosticTarget::Aggregate => selected_light_index == u32::MAX,
        },
        "local-light diagnostic selected index does not match its identity result"
    );
    Ok((
        identity_matches == 1,
        (selected_light_index != u32::MAX).then_some(selected_light_index),
    ))
}

#[derive(Default)]
pub(crate) struct LocalLightVisibilityDiagnostic {
    next_request_serial: u32,
    queued: Option<LocalLightVisibilityDiagnosticRequest>,
    in_flight: Option<LocalLightVisibilityDiagnosticRequest>,
    published: Option<LocalLightVisibilityDiagnosticEvidence>,
}

impl LocalLightVisibilityDiagnostic {
    pub fn has_queued(&self) -> bool {
        self.queued.is_some()
    }

    pub fn request(
        &mut self,
        resources: &LocalLightingResources,
        geometry_revision: u32,
        source_revision: u64,
        light_id: LightId,
        receiver_position: Vec3,
        receiver_normal: Vec3,
        ray_origin_offset_world: f32,
    ) -> Result<u32> {
        self.request_target(
            resources,
            geometry_revision,
            source_revision,
            LocalLightVisibilityDiagnosticTarget::Specific(light_id),
            receiver_position,
            receiver_normal,
            ray_origin_offset_world,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn request_aggregate(
        &mut self,
        resources: &LocalLightingResources,
        geometry_revision: u32,
        source_revision: u64,
        receiver_position: Vec3,
        receiver_normal: Vec3,
        ray_origin_offset_world: f32,
    ) -> Result<u32> {
        self.request_target(
            resources,
            geometry_revision,
            source_revision,
            LocalLightVisibilityDiagnosticTarget::Aggregate,
            receiver_position,
            receiver_normal,
            ray_origin_offset_world,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn request_target(
        &mut self,
        resources: &LocalLightingResources,
        geometry_revision: u32,
        source_revision: u64,
        target: LocalLightVisibilityDiagnosticTarget,
        receiver_position: Vec3,
        receiver_normal: Vec3,
        ray_origin_offset_world: f32,
    ) -> Result<u32> {
        ensure!(
            self.queued.is_none() && self.in_flight.is_none(),
            "local-light visibility diagnostic already has pending work"
        );
        ensure!(
            receiver_position.is_finite(),
            "diagnostic receiver position must be finite"
        );
        ensure!(
            receiver_normal.is_finite() && receiver_normal.length_squared() > 0.0,
            "diagnostic receiver normal must be finite and nonzero"
        );
        ensure!(
            ray_origin_offset_world.is_finite() && ray_origin_offset_world >= 0.0,
            "diagnostic ray-origin offset must be finite and nonnegative"
        );
        self.next_request_serial = self.next_request_serial.wrapping_add(1).max(1);
        let request = LocalLightVisibilityDiagnosticRequest {
            request_serial: self.next_request_serial,
            geometry_revision,
            source_revision,
            target,
            receiver_position,
            receiver_normal: receiver_normal.normalize(),
            ray_origin_offset_world,
        };
        resources
            .local_light_visibility_diagnostic_info
            .fill_uniform(&request.gpu_info())?;
        self.queued = Some(request);
        Ok(request.request_serial)
    }

    pub fn resolve_readback(&mut self, resources: &LocalLightingResources) -> Result<()> {
        let Some(request) = self.in_flight.take() else {
            return Ok(());
        };
        let bytes = resources
            .local_light_visibility_diagnostic_readback
            .read_back()?;
        ensure!(
            bytes.len() == std::mem::size_of::<LocalLightVisibilityDiagnosticResult>(),
            "local-light visibility diagnostic readback returned {} bytes",
            bytes.len(),
        );
        let result = bytemuck::pod_read_unaligned::<LocalLightVisibilityDiagnosticResult>(&bytes);
        let expected = request.gpu_info();
        ensure!(
            result.abi_version == LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_ABI_VERSION,
            "local-light visibility diagnostic ABI mismatch"
        );
        let (identity_matches, selected_light_index) = decode_selected_light_index(
            request.target,
            result.identity_matches,
            result.selected_light_index,
        )?;
        ensure!(
            result.request_serial == request.request_serial
                && result.geometry_revision == request.geometry_revision
                && result.mode == expected.mode
                && result.source_revision_low == expected.source_revision_low
                && result.source_revision_high == expected.source_revision_high
                && result.light_id_slot == expected.light_id_slot
                && result.light_id_generation == expected.light_id_generation,
            "local-light visibility diagnostic result identity diverged from its request"
        );
        ensure!(
            result.receiver_position_and_origin_offset
                == expected.receiver_position_and_origin_offset
                && result.receiver_normal_and_reserved == expected.receiver_normal_and_reserved,
            "local-light visibility diagnostic changed its fixed receiver inputs",
        );
        ensure!(
            result.candidates == result.visible.saturating_add(result.occluded)
                && match request.target {
                    LocalLightVisibilityDiagnosticTarget::Specific(_) => result.candidates <= 1,
                    LocalLightVisibilityDiagnosticTarget::Aggregate => {
                        result.candidates <= crate::lighting::LOCAL_LIGHT_GPU_CAPACITY as u32
                    }
                },
            "local-light visibility diagnostic visibility partition is invalid"
        );
        ensure!(
            result.identity_matches == 1
                || (result.candidates == 0
                    && result.visible == 0
                    && result.occluded == 0
                    && result.irradiance_luma_q8 == 0
                    && result.irradiance == [0.0; 4]),
            "unmatched local-light diagnostic must publish explicit zero contribution"
        );
        let evidence = LocalLightVisibilityDiagnosticEvidence {
            request,
            identity_matches,
            selected_light_index,
            irradiance: Vec3::new(
                result.irradiance[0],
                result.irradiance[1],
                result.irradiance[2],
            ),
            candidates: result.candidates,
            visible: result.visible,
            occluded: result.occluded,
            irradiance_luma_q8: result.irradiance_luma_q8,
        };
        log::info!(
            "[LOCAL_LIGHT][FIXED_GPU_EVIDENCE] request={} geometry_revision={} source_revision={} target={:?} light_id={:?} identity_matches={} selected_light_index={:?} receiver_world={:?} normal={:?} candidates={} visible={} occluded={} irradiance={:?} irradiance_luma_q8={} irradiance_luma={:.6}",
            request.request_serial,
            request.geometry_revision,
            request.source_revision,
            request.target,
            request.target.light_id(),
            evidence.identity_matches,
            evidence.selected_light_index,
            request.receiver_position,
            request.receiver_normal,
            evidence.candidates,
            evidence.visible,
            evidence.occluded,
            evidence.irradiance,
            evidence.irradiance_luma_q8,
            evidence.irradiance_luma(),
        );
        self.published = Some(evidence);
        Ok(())
    }

    pub fn record(
        &mut self,
        resources: &LocalLightingResources,
        pipeline: &ComputePipeline,
        cmdbuf: &CommandBuffer,
    ) {
        let Some(request) = self.queued.take() else {
            return;
        };
        assert!(self.in_flight.is_none());
        pipeline.record(cmdbuf, Extent3D::new(1, 1, 1), None);
        resources
            .local_light_visibility_diagnostic_result
            .record_copy_to_buffer(
                cmdbuf,
                &resources.local_light_visibility_diagnostic_readback,
                std::mem::size_of::<LocalLightVisibilityDiagnosticResult>() as u64,
                0,
                0,
            );
        cmdbuf.use_buffer(
            &resources.local_light_visibility_diagnostic_readback,
            BufferUse::HostRead,
        );
        self.in_flight = Some(request);
    }

    pub fn published(&self) -> Option<LocalLightVisibilityDiagnosticEvidence> {
        self.published
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::{LocalLight, LocalLightRegistry, PointLight};

    #[test]
    fn gpu_info_preserves_world_values_and_stable_identity() {
        let mut lights = LocalLightRegistry::default();
        let light_id = lights.add(LocalLight::Point(
            PointLight::new(Vec3::ZERO, Vec3::ONE, 1.0, 0.01, 1.0).unwrap(),
        ));
        let request = LocalLightVisibilityDiagnosticRequest {
            request_serial: 7,
            geometry_revision: 11,
            source_revision: 0x1234_5678_9abc_def0,
            target: LocalLightVisibilityDiagnosticTarget::Specific(light_id),
            receiver_position: Vec3::new(0.66, 101.0 / 256.0, 1.18),
            receiver_normal: Vec3::Y,
            ray_origin_offset_world: 0.001,
        };
        let info = request.gpu_info();
        assert_eq!(info.source_revision_low, 0x9abc_def0);
        assert_eq!(info.source_revision_high, 0x1234_5678);
        assert_eq!(info.light_id_slot, 0);
        assert_eq!(info.light_id_generation, 1);
        assert_eq!(info.mode, LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_SPECIFIC);
        assert_eq!(
            info.receiver_position_and_origin_offset,
            [0.66, 101.0 / 256.0, 1.18, 0.001]
        );
        assert_eq!(info.receiver_normal_and_reserved, [0.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn stale_source_revision_suppresses_a_still_found_light_index_without_error() {
        let mut lights = LocalLightRegistry::default();
        let light_id = lights.add(LocalLight::Point(
            PointLight::new(Vec3::ZERO, Vec3::ONE, 1.0, 0.01, 1.0).unwrap(),
        ));
        assert_eq!(
            decode_selected_light_index(
                LocalLightVisibilityDiagnosticTarget::Specific(light_id),
                0,
                u32::MAX,
            )
            .unwrap(),
            (false, None)
        );

        let shader =
            include_str!("../../shader/slang/local_light_visibility_diagnostic.comp.slang");
        assert!(shader.contains("result.selected_light_index = identityMatches && specificMode"));
    }

    #[test]
    fn aggregate_mode_requires_no_selected_identity_and_preserves_fixed_inputs() {
        assert_eq!(
            decode_selected_light_index(
                LocalLightVisibilityDiagnosticTarget::Aggregate,
                1,
                u32::MAX,
            )
            .unwrap(),
            (true, None)
        );
        let request = LocalLightVisibilityDiagnosticRequest {
            request_serial: 3,
            geometry_revision: 4,
            source_revision: 5,
            target: LocalLightVisibilityDiagnosticTarget::Aggregate,
            receiver_position: Vec3::new(0.1, 0.2, 0.3),
            receiver_normal: Vec3::Y,
            ray_origin_offset_world: 0.001,
        };
        let info = request.gpu_info();
        assert_eq!(info.mode, LOCAL_LIGHT_VISIBILITY_DIAGNOSTIC_AGGREGATE);
        assert_eq!((info.light_id_slot, info.light_id_generation), (0, 0));
    }
}
