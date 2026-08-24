use crate::{
    geom::{RoundCone, RoundConeClearanceIndex},
    util::cluster_positions,
};
use glam::Vec3;

const LEGACY_CLUSTER_DISTANCE_WORLD: f32 = 0.08;
const VOXELS_PER_WORLD_UNIT: f32 = 256.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyBranchEndpointSample {
    pub position_tree_voxels: Vec3,
    pub cluster_members: u32,
    pub clearance_voxels: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegacyBranchEndpointLayout {
    samples: Vec<LegacyBranchEndpointSample>,
}

impl LegacyBranchEndpointLayout {
    pub const MAX_SAMPLES: usize = 8;

    /// Reproduce the pre-canopy audio layout for diagnostics only.
    ///
    /// This deliberately retains the old greedy first-member clustering and largest-eight
    /// selection so comparisons describe the shipped path instead of a cleaned-up straw man.
    pub fn build(branch_endpoints_tree_voxels: &[Vec3], trunks: &[RoundCone]) -> Self {
        let mut clusters = cluster_positions(
            branch_endpoints_tree_voxels,
            LEGACY_CLUSTER_DISTANCE_WORLD * VOXELS_PER_WORLD_UNIT,
        );
        if clusters.len() > Self::MAX_SAMPLES {
            clusters.sort_by(|left, right| right.items_count.cmp(&left.items_count));
            clusters.truncate(Self::MAX_SAMPLES);
        }
        let clearance = RoundConeClearanceIndex::new(trunks);
        let samples = clusters
            .into_iter()
            .map(|cluster| LegacyBranchEndpointSample {
                position_tree_voxels: cluster.pos,
                cluster_members: cluster.items_count,
                clearance_voxels: clearance.minimum_clearance(cluster.pos),
            })
            .collect();
        Self { samples }
    }

    pub fn samples(&self) -> &[LegacyBranchEndpointSample] {
        &self.samples
    }

    pub fn selected_member_count(&self) -> u32 {
        self.samples
            .iter()
            .map(|sample| sample.cluster_members)
            .sum()
    }

    pub fn below_clearance_count(&self, required_clearance_voxels: f32) -> usize {
        self.samples
            .iter()
            .filter(|sample| sample.clearance_voxels < required_clearance_voxels)
            .count()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanopyAudioTrajectoryPhase {
    Settle,
    ForwardOrbit,
    OcclusionBoundaryHold,
    ReverseOrbit,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanopyAudioDiagnosticPose {
    pub phase: CanopyAudioTrajectoryPhase,
    pub position_world: Vec3,
    pub target_world: Vec3,
}

/// Fixed forward orbit, stationary boundary hold, and exact reverse orbit around one tree.
pub fn canopy_audio_diagnostic_pose(
    tree_origin_world: Vec3,
    elapsed_seconds: f32,
) -> CanopyAudioDiagnosticPose {
    const SETTLE_END: f32 = 1.0;
    const FORWARD_END: f32 = 5.0;
    const HOLD_END: f32 = 6.0;
    const REVERSE_END: f32 = 10.0;
    const START_ANGLE: f32 = -std::f32::consts::FRAC_PI_2;
    const FULL_ORBIT: f32 = std::f32::consts::TAU;

    let elapsed_seconds = elapsed_seconds.max(0.0);
    let (phase, angle) = if elapsed_seconds < SETTLE_END {
        (CanopyAudioTrajectoryPhase::Settle, START_ANGLE)
    } else if elapsed_seconds < FORWARD_END {
        let progress = (elapsed_seconds - SETTLE_END) / (FORWARD_END - SETTLE_END);
        (
            CanopyAudioTrajectoryPhase::ForwardOrbit,
            START_ANGLE + FULL_ORBIT * progress,
        )
    } else if elapsed_seconds < HOLD_END {
        (
            CanopyAudioTrajectoryPhase::OcclusionBoundaryHold,
            START_ANGLE + FULL_ORBIT,
        )
    } else if elapsed_seconds < REVERSE_END {
        let progress = (elapsed_seconds - HOLD_END) / (REVERSE_END - HOLD_END);
        (
            CanopyAudioTrajectoryPhase::ReverseOrbit,
            START_ANGLE + FULL_ORBIT * (1.0 - progress),
        )
    } else {
        (CanopyAudioTrajectoryPhase::Complete, START_ANGLE)
    };
    let target_world = tree_origin_world + Vec3::Y * 0.38;
    let position_world = target_world + Vec3::new(angle.cos() * 0.72, 0.0, angle.sin() * 0.72);
    CanopyAudioDiagnosticPose {
        phase,
        position_world,
        target_world,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audio::CanopyAcousticDescriptor,
        tree_gen::{Tree, TreeDesc},
    };

    #[test]
    fn legacy_layout_reproduces_branch_endpoint_selection_and_measures_clearance() {
        let endpoints = (0..10)
            .map(|index| Vec3::new(index as f32 * 30.0, 0.0, 0.0))
            .collect::<Vec<_>>();
        let trunks = [RoundCone::new(2.0, Vec3::NEG_Y, 2.0, Vec3::Y)];

        let layout = LegacyBranchEndpointLayout::build(&endpoints, &trunks);

        assert_eq!(layout.samples().len(), 8);
        assert_eq!(layout.selected_member_count(), 8);
        assert_eq!(layout.samples()[0].position_tree_voxels, endpoints[0]);
        assert!(layout.samples()[0].clearance_voxels < 0.0);
        assert_eq!(layout.below_clearance_count(2.0), 1);
    }

    #[test]
    fn fixed_generated_tree_exposes_legacy_wood_overlap_but_canopy_samples_are_clear() {
        let mut desc = TreeDesc::default();
        desc.branching.seed = 122;
        let tree = Tree::new(desc.clone());
        let legacy =
            LegacyBranchEndpointLayout::build(tree.relative_leaf_positions(), tree.trunks());
        let canopy = CanopyAcousticDescriptor::build(
            1,
            Vec3::ZERO,
            desc.branching.seed,
            tree.relative_leaf_placements(),
            tree.trunks(),
        );

        assert!(
            legacy.below_clearance_count(CanopyAcousticDescriptor::MIN_WOOD_CLEARANCE_VOXELS) > 0
        );
        assert!(canopy
            .samples()
            .iter()
            .all(|sample| sample.clearance_voxels()
                >= CanopyAcousticDescriptor::MIN_WOOD_CLEARANCE_VOXELS));
    }

    #[test]
    fn diagnostic_trajectory_retraces_the_forward_orbit_after_a_stationary_hold() {
        let tree = Vec3::new(1.0, 0.4, 1.0);
        let forward_start = canopy_audio_diagnostic_pose(tree, 1.0);
        let forward_end = canopy_audio_diagnostic_pose(tree, 5.0);
        let held = canopy_audio_diagnostic_pose(tree, 5.5);
        let reverse_end = canopy_audio_diagnostic_pose(tree, 10.0);

        assert_eq!(
            forward_start.phase,
            CanopyAudioTrajectoryPhase::ForwardOrbit
        );
        assert_eq!(
            forward_end.phase,
            CanopyAudioTrajectoryPhase::OcclusionBoundaryHold
        );
        assert_eq!(held.position_world, forward_end.position_world);
        assert_eq!(reverse_end.phase, CanopyAudioTrajectoryPhase::Complete);
        assert!(
            forward_start
                .position_world
                .distance(reverse_end.position_world)
                < 1.0e-6
        );
    }
}
