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
#[cfg_attr(not(test), allow(dead_code))]
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
    pub authored_lighting_revision: u64,
    pub local_lighting_revision: u64,
    pub visual_time_bits: u32,
    pub sampling_serial: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum LightingModeAcceptanceError {
    IdentityDrift,
    CaptureAlreadyPending,
    UnexpectedCapture,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct LightingModeAcceptanceRuntime {
    _artifact_path: Option<PathBuf>,
    phase: LightingModeAcceptancePhase,
    baseline_identity: Option<LightingModeAcceptanceIdentity>,
    phase_settled: bool,
    capture_pending: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
impl LightingModeAcceptanceRuntime {
    pub(super) fn new(options: Option<&LightingModeAcceptanceOptions>) -> Self {
        Self {
            _artifact_path: options.map(|options| options.artifact_path.clone()),
            phase: if options.is_some() {
                LightingModeAcceptancePhase::A
            } else {
                LightingModeAcceptancePhase::Inactive
            },
            baseline_identity: None,
            phase_settled: false,
            capture_pending: false,
        }
    }

    pub(super) fn effective_controls(
        &self,
        gui: EffectiveLightingControls,
    ) -> EffectiveLightingControls {
        self.phase.controls(gui)
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
use crate::LightingModeAcceptanceOptions;
use std::path::PathBuf;
