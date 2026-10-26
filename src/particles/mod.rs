pub(crate) mod bees;
mod butterfly_behavior;
mod butterfly_flight;
pub(crate) mod fireflies;
pub use butterfly_behavior::ButterflyForagingSettings;
mod butterfly_presentation;
pub(crate) mod butterfly_wingbeat;
pub(crate) mod flower_visits;
pub(crate) use butterfly_presentation::ButterflyFrame;
pub use butterfly_wingbeat::ButterflyWingbeatPose;
pub mod emitters;
mod leaf_flight;
pub mod system;

pub use butterfly_flight::{
    ButterflyFlightSettings, ButterflyFlightTuning, ButterflyFlightVariant,
};
pub use emitters::{
    ButterflyEmitter, ButterflyEmitterDesc, ButterflySpawnSource, FallenLeafEmitter,
    LeafEmitterDesc, ParticleEmitter,
};
pub use system::{
    MotionMode, ParticleForces, ParticleHandle, ParticleRenderKind, ParticleSnapshot,
    ParticleSpawn, ParticleSystem, ParticleTickStep, ParticleUpdateConfig,
    INITIAL_PARTICLE_CAPACITY, STANDARD_PARTICLE_SIZE,
};
