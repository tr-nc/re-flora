//! Tiny local MLS-MPM water simulation.
//!
//! This crate is intentionally split out of the main game crate. MLS-MPM spends
//! most of its time in small math-heavy P2G/G2P transfer loops; those loops are
//! dramatically slower in an unoptimized debug build. Keeping the solver in its
//! own crate lets the game stay debuggable while Cargo can compile only this
//! crate with release-like optimization in the dev profile.
//!
//! The initial implementation is intentionally bounded to a fixed world-space
//! test box so the solver cost and failure modes stay predictable.

pub mod collider;
pub mod mls_mpm;
pub mod pond;

pub use collider::{WaterTerrainColliderChunk, WaterTerrainColliderSet};
pub use pond::{
    DebugWaterSpawnResult, DebugWaterSpawnSkipReason, PondWaterConfig, PondWaterSim,
};
