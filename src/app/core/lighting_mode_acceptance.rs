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
        }
    }
}

pub(super) struct LightingModeAcceptanceRuntime {
    _artifact_path: Option<PathBuf>,
    phase: LightingModeAcceptancePhase,
}

impl LightingModeAcceptanceRuntime {
    pub(super) fn new(options: Option<&LightingModeAcceptanceOptions>) -> Self {
        Self {
            _artifact_path: options.map(|options| options.artifact_path.clone()),
            phase: if options.is_some() {
                LightingModeAcceptancePhase::A
            } else {
                LightingModeAcceptancePhase::Inactive
            },
        }
    }

    pub(super) fn effective_controls(
        &self,
        gui: EffectiveLightingControls,
    ) -> EffectiveLightingControls {
        self.phase.controls(gui)
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
}
use crate::LightingModeAcceptanceOptions;
use std::path::PathBuf;
