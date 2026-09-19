//! One tree-owned hierarchy pose, in world units, for render and interaction consumers.
//! Rest topology uses generator voxels; the conversion happens only at construction.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec3};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_POSE_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Global stiffness relative to the authored branch properties. The normalized
/// control has its neutral point at 0.5 and spans two stiffness octaves each way.
#[derive(Clone, Copy, Debug)]
pub struct TreeStiffness {
    pub(super) compliance_scale: f32,
    pub(super) frequency_scale: f32,
}

impl TreeStiffness {
    #[cfg(test)]
    const NEUTRAL: Self = Self {
        compliance_scale: 1.,
        frequency_scale: 1.,
    };

    pub fn from_control(value: f32) -> Result<Self> {
        ensure!(
            value.is_finite() && (0. ..=1.).contains(&value),
            "tree stiffness must be in 0..=1"
        );
        const OCTAVES_EACH_SIDE: f32 = 2.;
        let rigidity = ((value * 2. - 1.) * OCTAVES_EACH_SIDE).exp2();
        // With mass unchanged, static deflection is proportional to 1/k and
        // natural frequency to sqrt(k). Preserve branch ratios and damping.
        Ok(Self {
            compliance_scale: rigidity.recip(),
            frequency_scale: rigidity.sqrt(),
        })
    }
}

/// Shader ABI: immutable world-space joint and its local parent index.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuPoseJoint {
    pub start_frequency: [f32; 4],
    pub end_compliance: [f32; 4],
    pub parent: [u32; 4],
}

/// Compact skeleton publication, never an expanded voxel/vertex readback.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuPoseState {
    pub rotation: [f32; 4],
    pub translation: [f32; 4],
    pub angle: [f32; 4],
    pub velocity: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PoseVersion {
    generation: u64,
    revision: u64,
}

use crate::{branch_skeleton::BranchSegment, wind_field::WindFieldFrame};

impl PoseVersion {
    pub(crate) fn advanced(self) -> Self {
        Self {
            revision: self.revision + 1,
            ..self
        }
    }
}

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
    generation: u64,
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
            generation: NEXT_POSE_GENERATION.fetch_add(1, Ordering::Relaxed),
        })
    }

    pub fn branches(&self) -> &[BranchPose] {
        &self.branches
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn version(&self) -> PoseVersion {
        PoseVersion {
            generation: self.generation,
            revision: self.revision,
        }
    }

    pub(crate) fn gpu_joints(&self) -> Vec<GpuPoseJoint> {
        let mut joints: Vec<GpuPoseJoint> = Vec::with_capacity(self.joints.len());
        for joint in &self.joints {
            let depth = joint.parent.map_or(0, |p| joints[p].parent[1] + 1);
            joints.push(GpuPoseJoint {
                start_frequency: joint.start.extend(joint.frequency).to_array(),
                end_compliance: joint.end.extend(joint.compliance).to_array(),
                parent: [joint.parent.map_or(u32::MAX, |p| p as u32), depth, 0, 0],
            });
        }
        joints
    }

    pub(crate) fn gpu_state(&self) -> Vec<GpuPoseState> {
        self.joints
            .iter()
            .zip(&self.branches)
            .map(|(joint, pose)| GpuPoseState {
                rotation: pose.rotation.to_array(),
                translation: pose.translation.extend(0.).to_array(),
                angle: joint.angle.extend(0.).to_array(),
                velocity: joint.velocity.extend(0.).to_array(),
            })
            .collect()
    }

    /// A result may arrive after an edit/replacement or an explicit diagnostic
    /// pose. Reject that result, including equal-count/equal-topology replacements.
    pub(crate) fn accept_gpu(
        &mut self,
        source: PoseVersion,
        state: &[GpuPoseState],
    ) -> Result<bool> {
        if self.version() != source {
            return Ok(false);
        }
        ensure!(
            state.len() == self.joints.len(),
            "GPU pose topology mismatch"
        );
        for s in state {
            ensure!(
                s.rotation
                    .iter()
                    .chain(&s.translation)
                    .chain(&s.angle)
                    .chain(&s.velocity)
                    .all(|v| v.is_finite()),
                "nonfinite GPU pose publication"
            );
            ensure!(
                (Quat::from_array(s.rotation).length_squared() - 1.).abs() < 1e-4,
                "invalid GPU pose rotation"
            );
        }
        for ((joint, pose), s) in self.joints.iter_mut().zip(&mut self.branches).zip(state) {
            joint.angle = Vec3::from_slice(&s.angle);
            joint.velocity = Vec3::from_slice(&s.velocity);
            *pose = BranchPose {
                rotation: Quat::from_array(s.rotation),
                translation: Vec3::from_slice(&s.translation),
            };
        }
        self.revision += 1;
        Ok(true)
    }

    pub(crate) fn validate_step(wind: &WindFieldFrame, dt: f32) -> Result<()> {
        ensure!(dt.is_finite() && dt >= 0., "invalid tree pose timestep");
        ensure!(
            wind.domain
                .iter()
                .chain(wind.cells.iter().flatten())
                .all(|v| v.is_finite()),
            "nonfinite tree wind frame"
        );
        ensure!(
            wind.domain[3] == 0. || (wind.domain[0] > 0. && wind.domain[1] > 0.),
            "invalid tree wind domain"
        );
        Ok(())
    }

    #[cfg(test)]
    pub fn advance(&mut self, wind: &WindFieldFrame, dt: f32) -> Result<()> {
        self.advance_with_stiffness(wind, dt, TreeStiffness::NEUTRAL)
    }

    /// dt is elapsed simulation time, not an absolute wall clock. Invalid input
    /// leaves the last published pose untouched. Consumers share this publication.
    pub fn advance_with_stiffness(
        &mut self,
        wind: &WindFieldFrame,
        dt: f32,
        stiffness: TreeStiffness,
    ) -> Result<()> {
        Self::validate_step(wind, dt)?;
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
                let target = (axis.cross(local_wind)
                    * (joint.compliance * stiffness.compliance_scale))
                    .clamp_length_max(0.22);
                advance_spring(
                    &mut joint.angle,
                    &mut joint.velocity,
                    target,
                    joint.frequency * stiffness.frequency_scale,
                    h,
                );
                let rotation =
                    (parent.rotation * rotation_from_scaled_axis(joint.angle)).normalize();
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

// The exponential map has a smooth limit at zero. Normalizing a tiny axis
// first loses accuracy when its squared length becomes subnormal, even though
// the original components and the resulting quaternion are representable.
fn rotation_from_scaled_axis(angle: Vec3) -> Quat {
    let theta_squared = angle.length_squared();
    if theta_squared < 1e-6 {
        // sin(theta / 2) / theta and cos(theta / 2), through theta squared.
        let vector = angle * (0.5 - theta_squared / 48.);
        Quat::from_xyzw(vector.x, vector.y, vector.z, 1. - theta_squared / 8.)
    } else {
        Quat::from_scaled_axis(angle)
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
    fn stiffness_midpoint_preserves_current_motion_and_endpoints_change_response() {
        let neutral = TreeStiffness::from_control(0.5).unwrap();
        assert_eq!(neutral.compliance_scale, 1.);
        assert_eq!(neutral.frequency_scale, 1.);
        for invalid in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
            assert!(TreeStiffness::from_control(invalid).is_err());
        }
        let wind = WindFieldFrame::uniform(Vec2::X * 0.5);
        let mut baseline = TreePose::new(&topology(), Vec3::ZERO).unwrap();
        let mut poses = [baseline.clone(), baseline.clone(), baseline.clone()];
        for _ in 0..1200 {
            baseline.advance(&wind, 1. / 60.).unwrap();
            for (pose, control) in poses.iter_mut().zip([0., 0.5, 1.]) {
                pose.advance_with_stiffness(
                    &wind,
                    1. / 60.,
                    TreeStiffness::from_control(control).unwrap(),
                )
                .unwrap();
            }
        }
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&baseline.gpu_state()),
            bytemuck::cast_slice::<_, u8>(&poses[1].gpu_state())
        );
        let angles = poses.each_ref().map(|p| p.joints[0].angle.length());
        assert!(angles[0] > angles[1] * 3.9);
        assert!(angles[1] > angles[2] * 3.9);
        assert_eq!(
            TreeStiffness::from_control(0.).unwrap().frequency_scale,
            0.5
        );
        assert_eq!(TreeStiffness::from_control(1.).unwrap().frequency_scale, 2.);
    }

    #[test]
    fn live_stiffness_change_keeps_pose_state_and_topology() {
        let wind = WindFieldFrame::uniform(Vec2::X);
        let mut pose = TreePose::new(&topology(), Vec3::ZERO).unwrap();
        for _ in 0..240 {
            pose.advance(&wind, 1. / 60.).unwrap();
        }
        let before = pose.gpu_state();
        let version = pose.version();
        let hard = TreeStiffness::from_control(1.).unwrap();
        pose.advance_with_stiffness(&wind, 0., hard).unwrap();
        assert_eq!(pose.version(), version);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&before),
            bytemuck::cast_slice::<_, u8>(&pose.gpu_state())
        );
        pose.advance_with_stiffness(&wind, 1e-5, hard).unwrap();
        assert_eq!(pose.version(), version.advanced());
        assert!(pose.joints[0]
            .angle
            .abs_diff_eq(Vec3::from_slice(&before[0].angle), 1e-5));
        assert!(pose.joints[0].angle.length() > 0.001);
    }

    #[test]
    fn gpu_publication_rejects_replacements_and_is_transactional() {
        let mut pose = TreePose::new(&topology(), Vec3::ZERO).unwrap();
        let source = pose.version();
        let mut reference = pose.clone();
        reference
            .advance(&WindFieldFrame::uniform(Vec2::X * 4.), 1. / 60.)
            .unwrap();
        let states = reference.gpu_state();
        let mut replacement = TreePose::new(&topology(), Vec3::ZERO).unwrap();
        assert!(!replacement.accept_gpu(source, &states).unwrap());
        assert_eq!(replacement.revision(), 0);
        let mut invalid = states.clone();
        invalid[1].velocity[0] = f32::NAN;
        assert!(pose.accept_gpu(source, &invalid).is_err());
        assert_eq!(pose.version(), source);
        assert_eq!(pose.branches()[0].rotation, Quat::IDENTITY);
        assert!(pose.accept_gpu(source, &states).unwrap());
        assert_eq!(pose.version(), source.advanced());
        assert_eq!(
            pose.branches()[1].translation,
            reference.branches()[1].translation
        );
        assert!(!pose.accept_gpu(source, &states).unwrap());
        // Even an explicit CPU diagnostic invalidates an outstanding GPU step.
        let source = pose.version();
        pose.advance(&WindFieldFrame::default(), 0.01).unwrap();
        assert!(!pose.accept_gpu(source, &states).unwrap());
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
    fn small_rotation_preserves_motion_and_matches_exponential_map() {
        for strength in [0., 1e-30, 1e-22, 1e-18, 1e-8, 1e-4, 0.001, 0.22] {
            let axis = Vec3::new(1., -2., 3.).normalize();
            let actual = rotation_from_scaled_axis(axis * strength);
            let expected = Quat::from_axis_angle(axis, strength);
            assert!(actual.is_normalized());
            assert!(actual.abs_diff_eq(expected, 1e-7));
            if strength > 0. {
                assert!(actual.x > 0., "tiny rotations must not be snapped to rest");
            }
        }
    }
    #[test]
    fn tiny_wind_and_long_settling_keep_valid_rotations() {
        for strength in [1e-18, 1e-20, 1e-22, 1e-24, 1e-30] {
            let mut pose = TreePose::new(&topology(), Vec3::ZERO).unwrap();
            let wind = WindFieldFrame::uniform(Vec2::new(strength, strength * 0.5));
            for _ in 0..120 {
                pose.advance(&wind, 1. / 60.).unwrap();
                assert!(pose
                    .branches()
                    .iter()
                    .all(|p| p.rotation.is_normalized() && p.translation.is_finite()));
            }
        }
        let mut pose = TreePose::new(&topology(), Vec3::ZERO).unwrap();
        pose.advance(&WindFieldFrame::uniform(Vec2::X * 3.), 0.25)
            .unwrap();
        for _ in 0..18000 {
            pose.advance(&WindFieldFrame::default(), 1. / 60.).unwrap();
            assert!(pose
                .branches()
                .iter()
                .all(|p| p.rotation.is_normalized() && p.translation.is_finite()));
        }
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
