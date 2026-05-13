//! Tiny local MLS-MPM water simulation.
//!
//! The initial implementation is intentionally bounded to a fixed world-space
//! test box so the solver cost and failure modes stay predictable.

pub mod collider;
pub mod mls_mpm;
pub mod pond;

pub use pond::{PondWaterConfig, PondWaterSim, WaterParticle};
