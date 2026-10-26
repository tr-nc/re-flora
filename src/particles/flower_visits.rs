//! Simulation-visible flower facts published from the renderer's actual GPU pose.
//! Stable identities do not encode mutable draw slots.
use glam::{Quat, Vec3};

/// Shared cached-model body pose, optionally bound to a flower. The historical
/// name remains; bees and rigid bee houses use the same publication contract.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ButterflyBodyPose {
    pub orientation: Quat,
    pub phase: Option<f32>,
    pub host: Option<FlowerId>,
    pub yaw: f32,
    pub clearance: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct FlowerId {
    pub epoch: u64,
    pub plant: u64,
    pub head: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FlowerTarget {
    pub id: FlowerId,
    pub species: u32,
    pub position: Vec3,
    pub normal: Vec3,
    pub tangent: Vec3,
    pub radius: f32,
    pub velocity: Vec3,
    pub observed_at: f32,
}
