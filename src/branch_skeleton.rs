use glam::Vec3;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use std::f32::consts::PI;

#[derive(Debug, Clone)]
pub struct BranchingDesc {
    pub seed: u64,
    pub iterations: u32,
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BranchSegment {
    pub start: Vec3,
    pub end: Vec3,
    pub level: u32,
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
    let mut skeleton = BranchSkeleton::default();
    recurse(
        Vec3::ZERO,
        Vec3::Y,
        0,
        desc,
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
        return;
    }

    skeleton.nodes.push(BranchNode { pos, level });

    let length_variation_factor = {
        let random_factor = rng.random_range(-1.0..=1.0);
        1.0 + random_factor * desc.segment_length_variation
    };

    let segment_length = length * length_variation_factor;
    let level_factor = (level as f32) / (desc.iterations as f32);
    let vertical_influence = desc.vertical_tendency * level_factor;
    let adjusted_dir = (dir + Vec3::new(0.0, vertical_influence, 0.0)).normalize_or_zero();
    let end_pos = pos + adjusted_dir * segment_length;

    skeleton.segments.push(BranchSegment {
        start: pos,
        end: end_pos,
        level,
    });

    let should_branch = level < desc.iterations - 1
        && (level == 0 || rng.random::<f32>() < desc.branch_probability);

    if should_branch {
        let branch_count = if desc.branch_count_min == desc.branch_count_max {
            desc.branch_count_min
        } else {
            rng.random_range(desc.branch_count_min..=desc.branch_count_max)
        };

        for i in 0..branch_count {
            let new_dir =
                calculate_branch_direction(adjusted_dir, i, branch_count, level, desc, rng);

            if new_dir != Vec3::ZERO {
                recurse(
                    end_pos,
                    new_dir,
                    level + 1,
                    desc,
                    length * desc.length_dropoff,
                    skeleton,
                    rng,
                );
            }
        }
    } else {
        let new_dir = add_direction_variation(adjusted_dir, desc.randomness * 0.2, rng);
        recurse(
            end_pos,
            new_dir,
            level + 1,
            desc,
            length * desc.length_dropoff,
            skeleton,
            rng,
        );
    }
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
    let away_angle =
        rng.random_range(desc.branch_angle_min..=desc.branch_angle_max) * (1.0 + desc.spread);

    let up = if parent_dir.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let right = parent_dir.cross(up).normalize_or_zero();
    let forward = parent_dir.cross(right).normalize_or_zero();

    let branch_dir = {
        let rotated_perp = right * around_angle.cos() + forward * around_angle.sin();
        let base_dir = parent_dir * away_angle.cos() + rotated_perp * away_angle.sin();
        base_dir.normalize_or_zero()
    };

    add_direction_variation(branch_dir, desc.randomness, rng)
}

fn add_direction_variation(dir: Vec3, variation: f32, rng: &mut impl RngExt) -> Vec3 {
    let rand_x = rng.random_range(-variation..=variation);
    let rand_y = rng.random_range(-variation..=variation);
    let rand_z = rng.random_range(-variation..=variation);
    (dir + Vec3::new(rand_x, rand_y, rand_z)).normalize_or_zero()
}
