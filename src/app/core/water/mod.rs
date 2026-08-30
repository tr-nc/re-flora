mod coordinator;
mod runtime;
mod settings;
mod simulation;
mod terrain;

pub(super) use coordinator::{WaterPhase, WaterRuntime};
#[cfg(test)]
pub(super) use settings::EXPERIENCE_PARTICLE_COUNT;
pub(super) use settings::{
    WaterLaunchRequest, EXPERIENCE_INITIAL_FLUID_MAX_WS, EXPERIENCE_INITIAL_FLUID_MIN_WS,
};
pub(super) use simulation::WaterEditSoak;
