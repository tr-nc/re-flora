//! One tree-owned hierarchy pose, in world units, for render and interaction consumers.
//! Rest topology uses generator voxels; the conversion happens only at construction.
use anyhow::{ensure, Result};
use glam::{Quat, Vec3};

use crate::{branch_skeleton::BranchSegment, wind_field::WindFieldFrame};

#[derive(Clone, Copy, Debug)]
pub struct BranchPose {
    pub rotation: Quat,
    pub translation: Vec3,
}

impl BranchPose {
    pub fn transform_point(self, rest_world: Vec3) -> Vec3 {
        self.rotation * rest_world + self.translation
    }

    pub fn inverse_point(self, world: Vec3) -> Vec3 {
        self.rotation.conjugate() * (world - self.translation)
    }
}

#[derive(Clone, Debug)]
struct Joint {
    parent: Option<usize>,
    start: Vec3,
    end: Vec3,
    compliance: f32,
    frequency: f32,
    angle: Vec3,
    velocity: Vec3,
}

/// This value belongs to one canonical tree generation. Replacing the tree creates
/// a new pose even when its branch count happens to match the old generation.
#[derive(Clone, Debug)]
pub struct TreePose {
    joints: Vec<Joint>,
    branches: Vec<BranchPose>,
    revision: u64,
}

impl TreePose {
    pub fn new(topology: &[BranchSegment], origin_world: Vec3) -> Result<Self> {
        ensure!(origin_world.is_finite(), "nonfinite tree origin");
        let mut joints = Vec::with_capacity(topology.len());
        for (index, branch) in topology.iter().enumerate() {
            ensure!(
                branch.start.is_finite() && branch.end.is_finite(),
                "nonfinite tree branch"
            );
            if let Some(parent) = branch.parent {
                ensure!(parent < index, "tree parent must precede child");
                ensure!(
                    branch.start.distance(topology[parent].end) < 1e-4,
                    "tree child must attach to parent endpoint"
                );
            }
            joints.push(Joint {
                parent: branch.parent,
                start: origin_world + branch.start / 256.,
                end: origin_world + branch.end / 256.,
                // Art-directed angular compliance: thick basal axes bend less,
                // but every nonzero branch participates. No second wind source.
                compliance: 0.012 * (1. + branch.level.min(8) as f32 * 0.7),
                frequency: 0.65 + 0.14 * branch.level.min(8) as f32,
                angle: Vec3::ZERO,
                velocity: Vec3::ZERO,
            });
        }
        Ok(Self {
            branches: vec![
                BranchPose {
                    rotation: Quat::IDENTITY,
                    translation: Vec3::ZERO
                };
                joints.len()
            ],
            joints,
            revision: 0,
        })
    }

    pub fn branches(&self) -> &[BranchPose] {
        &self.branches
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// dt is elapsed simulation time, not an absolute wall clock. Invalid input
    /// leaves the last published pose untouched. Consumers share this publication.
    pub fn advance(&mut self, wind: &WindFieldFrame, dt: f32) -> Result<()> {
        ensure!(dt.is_finite() && dt >= 0., "invalid tree pose timestep");
        ensure!(
            wind.domain
                .iter()
                .chain(wind.cells.iter().flatten())
                .all(|v| v.is_finite()),
            "nonfinite tree wind frame"
        );
        if dt == 0. {
            return Ok(());
        }
        // Exact damped-oscillator integration for a constant target. Substeps
        // resolve changes in the sampled position and inherited parent rotation.
        let steps = (dt.min(0.25) * 120.).ceil().max(1.) as usize;
        let h = dt.min(0.25) / steps as f32;
        for _ in 0..steps {
            for index in 0..self.joints.len() {
                let joint = &mut self.joints[index];
                let parent = joint
                    .parent
                    .map(|p| self.branches[p])
                    .unwrap_or(BranchPose {
                        rotation: Quat::IDENTITY,
                        translation: Vec3::ZERO,
                    });
                let sample = self.branches[index].transform_point((joint.start + joint.end) * 0.5);
                let local_wind = parent.rotation.conjugate() * wind.sample_world(sample);
                let axis = (joint.end - joint.start).normalize_or_zero();
                let target = (axis.cross(local_wind) * joint.compliance).clamp_length_max(0.22);
                advance_spring(
                    &mut joint.angle,
                    &mut joint.velocity,
                    target,
                    joint.frequency,
                    h,
                );
                let rotation = (parent.rotation * Quat::from_scaled_axis(joint.angle)).normalize();
                // Child pivot is the parent's deformed endpoint, never its rest endpoint.
                let pivot = parent.transform_point(joint.start);
                self.branches[index] = BranchPose {
                    rotation,
                    translation: pivot - rotation * joint.start,
                };
            }
        }
        self.revision += 1;
        Ok(())
    }
}

fn advance_spring(angle: &mut Vec3, velocity: &mut Vec3, target: Vec3, hz: f32, dt: f32) {
    let omega = std::f32::consts::TAU * hz;
    let decay = 0.38 * omega;
    let oscillation = omega * (1.0_f32 - 0.38 * 0.38).sqrt();
    let displacement = *angle - target;
    let c = (oscillation * dt).cos();
    let s = (oscillation * dt).sin() / oscillation;
    let envelope = (-decay * dt).exp();
    *angle = target + envelope * (displacement * c + (*velocity + decay * displacement) * s);
    *velocity = envelope * (*velocity * c - (decay * *velocity + omega * omega * displacement) * s);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch_skeleton::BranchSegmentRole;
    use glam::Vec2;
    fn topology() -> Vec<BranchSegment> {
        vec![
            BranchSegment {
                parent: None,
                start: Vec3::ZERO,
                end: Vec3::Y * 64.,
                level: 0,
                role: BranchSegmentRole::MainAxis,
            },
            BranchSegment {
                parent: Some(0),
                start: Vec3::Y * 64.,
                end: Vec3::new(32., 96., 0.),
                level: 1,
                role: BranchSegmentRole::Lateral,
            },
            BranchSegment {
                parent: Some(0),
                start: Vec3::Y * 64.,
                end: Vec3::new(-32., 96., 0.),
                level: 1,
                role: BranchSegmentRole::Lateral,
            },
        ]
    }
    #[test]
    fn wind_keeps_roots_joints_lengths_and_inverse_coordinates() {
        let origin = Vec3::new(1., 0.4, 0.7);
        let topology = topology();
        let mut pose = TreePose::new(&topology, origin).unwrap();
        let wind = WindFieldFrame::uniform(Vec2::new(8., 2.));
        for _ in 0..240 {
            pose.advance(&wind, 1. / 60.).unwrap();
        }
        assert!(
            pose.branches()[0]
                .transform_point(origin + Vec3::Y * 0.25)
                .x
                > origin.x + 0.01
        );
        for (i, branch) in topology.iter().enumerate() {
            let rest_start = origin + branch.start / 256.;
            let rest_end = origin + branch.end / 256.;
            let transform = pose.branches()[i];
            let start = transform.transform_point(rest_start);
            let end = transform.transform_point(rest_end);
            assert!((start.distance(end) - rest_start.distance(rest_end)).abs() < 1e-6);
            if let Some(parent) = branch.parent {
                assert!(start.distance(pose.branches()[parent].transform_point(rest_start)) < 1e-6);
            } else {
                assert!(start.distance(origin) < 1e-6);
            }
            let surface = rest_end + Vec3::new(0.013, -0.002, 0.007);
            assert!(
                transform
                    .inverse_point(transform.transform_point(surface))
                    .distance(surface)
                    < 1e-6
            );
        }
    }
    #[test]
    fn no_wind_stays_at_rest_and_stopped_wind_settles() {
        let mut pose = TreePose::new(&topology(), Vec3::ZERO).unwrap();
        for _ in 0..60 {
            pose.advance(&WindFieldFrame::default(), 1. / 60.).unwrap();
        }
        assert!(pose
            .branches()
            .iter()
            .all(|p| p.rotation == Quat::IDENTITY && p.translation == Vec3::ZERO));
        for _ in 0..120 {
            pose.advance(&WindFieldFrame::uniform(Vec2::X * 3.), 1. / 60.)
                .unwrap();
        }
        assert!(pose.joints[0].angle.length() > 0.01);
        for _ in 0..1200 {
            pose.advance(&WindFieldFrame::default(), 1. / 60.).unwrap();
        }
        assert!(pose
            .joints
            .iter()
            .all(|j| j.angle.length() < 1e-6 && j.velocity.length() < 1e-6));
    }
    #[test]
    fn invalid_topology_and_inputs_do_not_publish() {
        let mut bad = topology();
        bad[0].parent = Some(1);
        assert!(TreePose::new(&bad, Vec3::ZERO).is_err());
        let mut pose = TreePose::new(&topology(), Vec3::ZERO).unwrap();
        assert!(pose.advance(&WindFieldFrame::default(), f32::NAN).is_err());
        assert!(pose
            .advance(&WindFieldFrame::uniform(Vec2::splat(f32::NAN)), 0.01)
            .is_err());
        assert_eq!(pose.revision(), 0);
    }
    #[test]
    fn zero_length_branch_and_strong_wind_remain_finite() {
        let mut branches = topology();
        branches[1].end = branches[1].start;
        let mut pose = TreePose::new(&branches, Vec3::ZERO).unwrap();
        for _ in 0..300 {
            pose.advance(&WindFieldFrame::uniform(Vec2::splat(100.)), 1. / 30.)
                .unwrap();
        }
        assert!(pose
            .branches()
            .iter()
            .all(|p| p.rotation.is_finite() && p.translation.is_finite()));
    }
}
