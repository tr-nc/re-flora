//! One generation interface for every saved tree architecture.
//! Species rules and legacy adapters stop here: geometry sees only connected branches,
//! age-selected wood/radii and foliage attachment sites, never a species-name decision.
use super::{axis, TreeDesc};
use crate::branch_skeleton::{generate_branch_skeleton_with_rng, BranchSegment};
use glam::Vec3;
use rand::{rngs::StdRng, SeedableRng};

pub(super) mod rules;

pub(super) const TREE_DEFAULT_SIZE: f32 = 30.;
const TREE_SAPLING_VISIBLE_BRANCH_LEVELS: u32 = 1;

pub(super) struct WoodSection {
    pub branch: usize,
    pub radius_start: f32,
    pub radius_end: f32,
}

pub(super) struct GrowthSkeleton {
    /// Full deterministic topology, including wood not yet visible at the requested age.
    pub branches: Vec<BranchSegment>,
    /// Empty only for the frozen recursive adapter, preserving the existing consumer contract.
    pub axes: Vec<axis::AxisSegment>,
    pub wood: Vec<WoodSection>,
    /// (branch identity, attachment position, attachment direction), in stable branch order.
    pub foliage: Vec<(usize, Vec3, Vec3)>,
}

pub(super) fn generate(desc: &TreeDesc) -> GrowthSkeleton {
    let rule = desc.species.growth_rule(desc.axis_model);
    let (branches, axes) = if let Some(skeleton) = axis::generate(desc, rule.architecture) {
        (skeleton.segments, skeleton.metadata)
    } else {
        // Legacy recursive arithmetic/RNG order remain unchanged for existing descriptions.
        let mut branching = desc.branching.normalized();
        branching.initial_length *= (desc.size / TREE_DEFAULT_SIZE).max(0.);
        let mut rng = StdRng::seed_from_u64(branching.seed);
        (
            generate_branch_skeleton_with_rng(&branching, &mut rng).segments,
            Vec::new(),
        )
    };
    let visible_levels = visible_branch_levels(desc.growth_age, desc.branching.iterations);
    let leaf_level = visible_levels.saturating_sub(desc.leaf_offset).max(1);
    let visible: Vec<_> = branches
        .iter()
        .enumerate()
        .map(|(i, branch)| {
            if axes.is_empty() {
                branch.level < visible_levels
            } else {
                axes[i].reveal_age <= desc.growth_age.clamp(0., 1.)
            }
        })
        .collect();
    let last_visible_trunk = axes
        .iter()
        .enumerate()
        .filter(|(i, axis)| axis.order == 0 && visible[*i])
        .map(|(i, _)| i)
        .next_back();
    let foliage = branches
        .iter()
        .enumerate()
        .filter(|(i, _)| visible[*i])
        .filter(|(i, branch)| match rule.foliage {
            rules::FoliageRule::RecursiveDepth => branch.level.saturating_add(1) == leaf_level,
            rules::FoliageRule::TwigTips | rules::FoliageRule::TwigAxes => {
                let axis = &axes[*i];
                let leafy_twig = axis.order == 2
                    && (matches!(rule.foliage, rules::FoliageRule::TwigAxes) || axis.u_end == 1.);
                leafy_twig || Some(*i) == last_visible_trunk
            }
        })
        .map(|(i, branch)| {
            (
                i,
                branch.end,
                (branch.end - branch.start).normalize_or_zero(),
            )
        })
        .collect();
    let base_thickness = desc.trunk_thickness * desc.size;
    let wood = branches
        .iter()
        .enumerate()
        .filter(|(i, _)| visible[*i])
        .map(|(i, branch)| {
            let (radius_start, radius_end) = if axes.is_empty() {
                (
                    thickness_at_level(desc, base_thickness, branch.level),
                    thickness_at_level(desc, base_thickness, branch.level + 1),
                )
            } else {
                (axes[i].radius_start, axes[i].radius_end)
            };
            WoodSection {
                branch: i,
                radius_start,
                radius_end,
            }
        })
        .collect();
    GrowthSkeleton {
        branches,
        axes,
        wood,
        foliage,
    }
}

pub(super) fn thickness_at_level(desc: &TreeDesc, base_thickness: f32, level: u32) -> f32 {
    let mut thickness = base_thickness;
    for current_level in 0..level {
        thickness = if desc.thickness_reduction > 0.0 {
            thickness * desc.thickness_reduction
        } else {
            thickness * 0.1_f32.powf((current_level + 1) as f32)
        };
    }
    thickness
}

pub(super) fn visible_branch_levels(age: f32, mature_levels: u32) -> u32 {
    let mature_levels = mature_levels.max(1);
    let young_levels = TREE_SAPLING_VISIBLE_BRANCH_LEVELS.min(mature_levels);
    let age = age.clamp(0.0, 1.0);
    let smooth_age = age * age * (3.0 - 2.0 * age);
    let level_count = mature_levels - young_levels + 1;
    (young_levels + (smooth_age * level_count as f32).floor() as u32).min(mature_levels)
}
