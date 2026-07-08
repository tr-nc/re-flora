use crate::util::stable_perpendicular_basis;
use glam::Vec3;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use std::f32::consts::PI;

#[derive(Debug, Clone, PartialEq)]
pub struct BranchingDesc {
    pub seed: u64,
    pub iterations: u32,
    /// Normalized position on each recursive axis where lateral branches may start.
    ///
    /// This mirrors Weber-Penn/BaseSize-style controls: 0.0 branches from the root,
    /// while values near 1.0 leave most of the trunk/axis bare before the first branch.
    pub branch_start_fraction: f32,
    /// Normalized position on each recursive axis after which lateral branches stop.
    pub branch_end_fraction: f32,
    pub initial_length: f32,
    pub length_dropoff: f32,
    pub spread: f32,
    pub randomness: f32,
    pub vertical_tendency: f32,
    pub branch_angle_min: f32,
    pub branch_angle_max: f32,
    pub branch_probability: f32,
    pub branch_count_min: u32,
    pub branch_count_max: u32,
    pub segment_length_variation: f32,
    /// When true, branching emits lateral children while also continuing the parent axis.
    /// This lets a trunk/vine keep growing and branch again at later positions.
    pub continue_main_axis: bool,
}

impl BranchingDesc {
    pub fn normalize(&mut self) {
        self.iterations = self.iterations.max(1);
        self.branch_start_fraction = self.branch_start_fraction.clamp(0.0, 1.0);
        self.branch_end_fraction = self
            .branch_end_fraction
            .clamp(self.branch_start_fraction, 1.0);
        self.initial_length = self.initial_length.max(0.0);
        self.length_dropoff = self.length_dropoff.max(0.0);
        self.spread = self.spread.max(0.0);
        self.randomness = self.randomness.max(0.0);
        self.branch_probability = self.branch_probability.clamp(0.0, 1.0);
        if self.branch_count_min > self.branch_count_max {
            self.branch_count_max = self.branch_count_min;
        }
        self.segment_length_variation = self.segment_length_variation.max(0.0);
        if self.branch_angle_min > self.branch_angle_max {
            self.branch_angle_max = self.branch_angle_min;
        }
    }

    pub fn normalized(&self) -> Self {
        let mut desc = self.clone();
        desc.normalize();
        desc
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BranchSegmentRole {
    MainAxis,
    Lateral,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BranchSegment {
    pub start: Vec3,
    pub end: Vec3,
    pub level: u32,
    pub role: BranchSegmentRole,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BranchNode {
    pub pos: Vec3,
    pub level: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BranchSkeleton {
    pub segments: Vec<BranchSegment>,
    pub nodes: Vec<BranchNode>,
}

pub fn generate_branch_skeleton(desc: &BranchingDesc) -> BranchSkeleton {
    let mut rng = StdRng::seed_from_u64(desc.seed);
    generate_branch_skeleton_with_rng(desc, &mut rng)
}

pub fn generate_branch_skeleton_with_rng(
    desc: &BranchingDesc,
    rng: &mut impl RngExt,
) -> BranchSkeleton {
    let desc = desc.normalized();
    let mut skeleton = BranchSkeleton::default();
    recurse(
        Vec3::ZERO,
        Vec3::Y,
        0,
        &desc,
        desc.initial_length,
        &mut skeleton,
        rng,
    );
    skeleton
}

fn recurse(
    pos: Vec3,
    dir: Vec3,
    level: u32,
    desc: &BranchingDesc,
    length: f32,
    skeleton: &mut BranchSkeleton,
    rng: &mut impl RngExt,
) {
    if level >= desc.iterations {
        skeleton.nodes.push(BranchNode { pos, level });
        return;
    }

    skeleton.nodes.push(BranchNode { pos, level });

    let axis_dir = adjusted_axis_direction(dir, level, desc);
    let should_branch = should_emit_lateral_branches(level, desc, rng);

    if should_branch {
        let branch_count = sample_branch_count(desc, rng);

        if desc.continue_main_axis {
            let continuation_dir = add_direction_variation(axis_dir, desc.randomness * 0.2, rng);
            grow_segment(
                pos,
                continuation_dir,
                level,
                BranchSegmentRole::MainAxis,
                desc,
                length,
                skeleton,
                rng,
            );
        }

        for branch_index in 0..branch_count {
            let new_dir =
                calculate_branch_direction(axis_dir, branch_index, branch_count, level, desc, rng);

            grow_segment(
                pos,
                new_dir,
                level,
                BranchSegmentRole::Lateral,
                desc,
                length,
                skeleton,
                rng,
            );
        }

        if branch_count == 0 && !desc.continue_main_axis {
            let continuation_dir = add_direction_variation(axis_dir, desc.randomness * 0.2, rng);
            grow_segment(
                pos,
                continuation_dir,
                level,
                BranchSegmentRole::MainAxis,
                desc,
                length,
                skeleton,
                rng,
            );
        }
    } else {
        let continuation_dir = add_direction_variation(axis_dir, desc.randomness * 0.2, rng);
        grow_segment(
            pos,
            continuation_dir,
            level,
            BranchSegmentRole::MainAxis,
            desc,
            length,
            skeleton,
            rng,
        );
    }
}

fn adjusted_axis_direction(dir: Vec3, level: u32, desc: &BranchingDesc) -> Vec3 {
    let level_factor = (level as f32) / (desc.iterations as f32).max(1.0);
    let vertical_influence = desc.vertical_tendency * level_factor;
    (dir + Vec3::new(0.0, vertical_influence, 0.0)).normalize_or_zero()
}

fn should_emit_lateral_branches(level: u32, desc: &BranchingDesc, rng: &mut impl RngExt) -> bool {
    if !branch_window_contains_level(level, desc) {
        return false;
    }

    level == first_branch_level(desc) || rng.random::<f32>() < desc.branch_probability
}

fn branch_level_fraction(level: u32, iterations: u32) -> f32 {
    if iterations <= 1 {
        0.0
    } else {
        level as f32 / iterations.saturating_sub(1) as f32
    }
}

fn normalized_branch_window(desc: &BranchingDesc) -> (f32, f32) {
    let start = desc.branch_start_fraction.clamp(0.0, 1.0);
    let end = desc.branch_end_fraction.clamp(start, 1.0);
    (start, end)
}

fn first_branch_level(desc: &BranchingDesc) -> u32 {
    let (start, _) = normalized_branch_window(desc);
    (start * desc.iterations.saturating_sub(1) as f32).ceil() as u32
}

fn branch_window_contains_level(level: u32, desc: &BranchingDesc) -> bool {
    let t = branch_level_fraction(level, desc.iterations);
    let (start, end) = normalized_branch_window(desc);
    t >= start && t <= end
}

fn sample_branch_count(desc: &BranchingDesc, rng: &mut impl RngExt) -> u32 {
    let min = desc.branch_count_min.min(desc.branch_count_max);
    let max = desc.branch_count_min.max(desc.branch_count_max);
    if min == max {
        min
    } else {
        rng.random_range(min..=max)
    }
}

fn varied_segment_length(length: f32, desc: &BranchingDesc, rng: &mut impl RngExt) -> f32 {
    let random_factor = rng.random_range(-1.0..=1.0);
    let variation = desc.segment_length_variation.max(0.0);
    length * (1.0 + random_factor * variation).max(0.0)
}

fn grow_segment(
    pos: Vec3,
    dir: Vec3,
    level: u32,
    role: BranchSegmentRole,
    desc: &BranchingDesc,
    length: f32,
    skeleton: &mut BranchSkeleton,
    rng: &mut impl RngExt,
) {
    if dir == Vec3::ZERO {
        return;
    }

    let segment_length = varied_segment_length(length, desc, rng);
    let end_pos = pos + dir * segment_length;

    skeleton.segments.push(BranchSegment {
        start: pos,
        end: end_pos,
        level,
        role,
    });

    recurse(
        end_pos,
        dir,
        level + 1,
        desc,
        length * desc.length_dropoff,
        skeleton,
        rng,
    );
}

fn calculate_branch_direction(
    parent_dir: Vec3,
    branch_index: u32,
    total_branches: u32,
    level: u32,
    desc: &BranchingDesc,
    rng: &mut impl RngExt,
) -> Vec3 {
    let golden_angle = 2.4;
    let around_angle = if total_branches > 1 {
        (branch_index as f32) * (2.0 * PI) / (total_branches as f32) + (level as f32) * golden_angle
    } else {
        rng.random::<f32>() * 2.0 * PI
    };
    let around_angle = add_azimuth_variation(around_angle, desc.randomness, rng);
    let branch_angle = rng.random_range(desc.branch_angle_min..=desc.branch_angle_max);
    let away_angle = spread_adjusted_branch_angle(branch_angle, desc.spread);

    let (right, forward) = stable_perpendicular_basis(parent_dir);
    let rotated_perp = right * around_angle.cos() + forward * around_angle.sin();
    let base_dir = parent_dir * away_angle.cos() + rotated_perp * away_angle.sin();
    base_dir.normalize_or_zero()
}

fn spread_adjusted_branch_angle(branch_angle: f32, spread: f32) -> f32 {
    // Spread should widen narrow branch angles smoothly, not multiply the authored angle past
    // horizontal. That keeps a 90° branch angle exactly perpendicular for the whole spread range.
    const TREE_SPREAD_FULL_HORIZONTAL: f32 = 2.0;
    let horizontal_angle = PI * 0.5;
    if branch_angle >= horizontal_angle {
        return branch_angle;
    }

    let spread_factor = (spread / TREE_SPREAD_FULL_HORIZONTAL).clamp(0.0, 1.0);
    branch_angle + (horizontal_angle - branch_angle) * spread_factor
}

fn add_azimuth_variation(around_angle: f32, randomness: f32, rng: &mut impl RngExt) -> f32 {
    let randomness = randomness.clamp(0.0, 1.0);
    if randomness <= 0.0 {
        return around_angle;
    }

    around_angle + rng.random_range(-PI..=PI) * randomness
}

fn add_direction_variation(dir: Vec3, variation: f32, rng: &mut impl RngExt) -> Vec3 {
    let rand_x = rng.random_range(-variation..=variation);
    let rand_y = rng.random_range(-variation..=variation);
    let rand_z = rng.random_range(-variation..=variation);
    (dir + Vec3::new(rand_x, rand_y, rand_z)).normalize_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_desc() -> BranchingDesc {
        BranchingDesc {
            seed: 7,
            iterations: 3,
            branch_start_fraction: 0.5,
            branch_end_fraction: 1.0,
            initial_length: 4.0,
            length_dropoff: 0.5,
            spread: 0.0,
            randomness: 0.0,
            vertical_tendency: 0.0,
            branch_angle_min: 45.0_f32.to_radians(),
            branch_angle_max: 45.0_f32.to_radians(),
            branch_probability: 1.0,
            branch_count_min: 2,
            branch_count_max: 2,
            segment_length_variation: 0.0,
            continue_main_axis: false,
        }
    }

    #[test]
    fn branch_start_fraction_zero_branches_from_root() {
        let desc = BranchingDesc {
            branch_start_fraction: 0.0,
            ..test_desc()
        };
        let skeleton = generate_branch_skeleton(&desc);

        let level_zero_segments = skeleton
            .segments
            .iter()
            .filter(|segment| segment.level == 0)
            .collect::<Vec<_>>();

        assert_eq!(level_zero_segments.len(), 2);
        assert!(level_zero_segments
            .iter()
            .all(|segment| segment.start == Vec3::ZERO));
        assert!(level_zero_segments
            .iter()
            .all(|segment| segment.role == BranchSegmentRole::Lateral));
    }

    #[test]
    fn later_branch_start_fraction_keeps_branchless_base_segment() {
        let desc = test_desc();
        let skeleton = generate_branch_skeleton(&desc);

        let level_zero_segments = skeleton
            .segments
            .iter()
            .filter(|segment| segment.level == 0)
            .collect::<Vec<_>>();
        assert_eq!(level_zero_segments.len(), 1);
        assert_eq!(level_zero_segments[0].start, Vec3::ZERO);
        assert_eq!(level_zero_segments[0].role, BranchSegmentRole::MainAxis);

        let first_branch_pos = level_zero_segments[0].end;
        let first_branch_segments = skeleton
            .segments
            .iter()
            .filter(|segment| segment.level == 1 && segment.start == first_branch_pos)
            .collect::<Vec<_>>();
        assert_eq!(first_branch_segments.len(), 2);
        assert!(first_branch_segments
            .iter()
            .all(|segment| segment.role == BranchSegmentRole::Lateral));
    }

    #[test]
    fn branch_end_fraction_limits_late_lateral_branches() {
        let desc = BranchingDesc {
            branch_start_fraction: 0.0,
            branch_end_fraction: 0.0,
            iterations: 4,
            ..test_desc()
        };
        let skeleton = generate_branch_skeleton(&desc);

        assert_eq!(
            skeleton
                .segments
                .iter()
                .filter(|segment| segment.level == 0)
                .count(),
            2
        );
        assert_eq!(
            skeleton
                .segments
                .iter()
                .filter(|segment| segment.level == 1)
                .count(),
            2
        );
    }

    #[test]
    fn branch_direction_is_continuous_across_near_vertical_parent_dirs() {
        let desc = BranchingDesc {
            branch_angle_min: 45.0_f32.to_radians(),
            branch_angle_max: 45.0_f32.to_radians(),
            randomness: 0.0,
            ..test_desc()
        };
        let below = Vec3::new((1.0_f32 - 0.899 * 0.899).sqrt(), 0.899, 0.0);
        let above = Vec3::new((1.0_f32 - 0.901 * 0.901).sqrt(), 0.901, 0.0);
        let mut below_rng = StdRng::seed_from_u64(11);
        let mut above_rng = StdRng::seed_from_u64(11);

        let below_dir = calculate_branch_direction(below, 0, 2, 0, &desc, &mut below_rng);
        let above_dir = calculate_branch_direction(above, 0, 2, 0, &desc, &mut above_rng);

        assert!(below_dir.dot(above_dir) > 0.99);
    }

    #[test]
    fn ninety_degree_branch_angle_stays_perpendicular_with_randomness_and_spread() {
        let desc = BranchingDesc {
            branch_angle_min: 90.0_f32.to_radians(),
            branch_angle_max: 90.0_f32.to_radians(),
            randomness: 0.33,
            spread: 2.0,
            ..test_desc()
        };
        let parent_dir = Vec3::Y;
        let mut rng = StdRng::seed_from_u64(11);

        let branch_dir = calculate_branch_direction(parent_dir, 0, 2, 0, &desc, &mut rng);

        assert!(branch_dir.dot(parent_dir).abs() < 1.0e-5);
    }

    #[test]
    fn terminal_nodes_include_final_branch_tips() {
        let desc = BranchingDesc {
            iterations: 1,
            branch_start_fraction: 0.0,
            branch_count_min: 2,
            branch_count_max: 2,
            ..test_desc()
        };
        let skeleton = generate_branch_skeleton(&desc);

        assert_eq!(
            skeleton
                .nodes
                .iter()
                .filter(|node| node.level == desc.iterations)
                .count(),
            2
        );
    }

    #[test]
    fn continuing_main_axis_keeps_parent_after_lateral_branching() {
        let desc = BranchingDesc {
            continue_main_axis: true,
            branch_count_min: 1,
            branch_count_max: 1,
            ..test_desc()
        };
        let skeleton = generate_branch_skeleton(&desc);
        let first_branch_pos = skeleton
            .segments
            .iter()
            .find(|segment| segment.level == 0)
            .map(|segment| segment.end)
            .unwrap();

        let child_segments = skeleton
            .segments
            .iter()
            .filter(|segment| segment.level == 1 && segment.start == first_branch_pos)
            .collect::<Vec<_>>();

        assert_eq!(child_segments.len(), 2);
        assert!(child_segments
            .iter()
            .any(|segment| segment.role == BranchSegmentRole::MainAxis));
        assert!(child_segments
            .iter()
            .any(|segment| segment.role == BranchSegmentRole::Lateral));
        assert!(child_segments.iter().any(|segment| {
            let dir = (segment.end - segment.start).normalize_or_zero();
            dir.dot(Vec3::Y) > 0.99
        }));
    }
}
