mod butterfly_flight;
mod butterfly_presentation;
pub(crate) use butterfly_presentation::ButterflyFrame;
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
    ParticleSpawn, ParticleSystem, ParticleTickStep, ParticleUpdateConfig, PARTICLE_CAPACITY,
    STANDARD_PARTICLE_SIZE,
};
