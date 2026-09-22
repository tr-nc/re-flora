use crate::branch_skeleton::{generate_branch_skeleton_with_rng, BranchSegment, BranchingDesc};
use crate::geom::RoundCone;
use crate::util::stable_perpendicular_basis;
use glam::Vec3;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

const TREE_DEFAULT_SIZE: f32 = 30.0;
const TREE_SAPLING_SIZE_RATIO: f32 = 0.12;
const TREE_SAPLING_THICKNESS_RATIO: f32 = 0.70;
const TREE_SAPLING_LEAF_DENSITY_RATIO: f32 = 0.25;
const TREE_SAPLING_LEAVES_SIZE_LEVEL: u32 = 1;
const TREE_SAPLING_VISIBLE_BRANCH_LEVELS: u32 = 1;

fn mature_tree_growth_age() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct TreeDesc {
    pub branching: BranchingDesc,
    #[serde(skip, default = "mature_tree_growth_age")]
    pub growth_age: f32,
    pub size: f32,
    pub trunk_thickness: f32,
    pub thickness_reduction: f32,
    pub leaves_size_level: u32,
    pub leaf_offset: u32,
    pub leaf_density: f32,
    pub leaf_spray_width_ratio: f32,
    pub leaf_spray_thickness_ratio: f32,
    pub leaf_spray_tip_offset_ratio: f32,
    pub fruit_spawn_probability: f32,
    pub fruit_side_offset_voxels: f32,
    pub fruit_side_offset_variance_voxels: f32,
    pub fruit_down_offset_voxels: f32,
    pub fruit_down_offset_variance_voxels: f32,
    pub fruit_swing_length_voxels: f32,
    pub fruit_swing_max_angle_degrees: f32,
    pub fruit_swing_speed: f32,
    pub fruit_swing_speed_variation: f32,
    pub fruit_swing_min_response: f32,
    pub enable_subdivision: bool,
    pub subdivision_count_min: u32,
    pub subdivision_count_max: u32,
    pub subdivision_randomness: f32,
    pub subdivision_randomness_progression: f32,
}

pub fn default_tree_branching_desc() -> BranchingDesc {
    BranchingDesc {
        seed: 122,
        iterations: 7,
        branch_start_fraction: 1.0 / 6.0,
        branch_end_fraction: 1.0,
        initial_length: 48.0,
        length_dropoff: 0.78,
        spread: 0.0,
        randomness: 0.33,
        vertical_tendency: 0.47,
        branch_angle_min: 24.0 * PI / 180.0,
        branch_angle_max: 48.0 * PI / 180.0,
        branch_probability: 0.82,
        branch_count_min: 2,
        branch_count_max: 3,
        segment_length_variation: 0.12,
        continue_main_axis: false,
    }
}

impl Default for TreeDesc {
    fn default() -> Self {
        TreeDesc {
            branching: default_tree_branching_desc(),
            growth_age: mature_tree_growth_age(),
            size: TREE_DEFAULT_SIZE,
            trunk_thickness: 0.40,
            thickness_reduction: 0.61,
            leaves_size_level: 5,
            leaf_offset: 1,
            leaf_density: 0.055,
            leaf_spray_width_ratio: 0.65,
            leaf_spray_thickness_ratio: 0.35,
            leaf_spray_tip_offset_ratio: 0.25,
            fruit_spawn_probability: 0.30,
            fruit_side_offset_voxels: 0.5,
            fruit_side_offset_variance_voxels: 1.5,
            fruit_down_offset_voxels: 4.0,
            fruit_down_offset_variance_voxels: 2.0,
            fruit_swing_length_voxels: 2.0,
            fruit_swing_max_angle_degrees: 50.0,
            fruit_swing_speed: 2.1,
            fruit_swing_speed_variation: 0.3,
            fruit_swing_min_response: 0.18,
            enable_subdivision: true,
            subdivision_count_min: 6,
            subdivision_count_max: 9,
            subdivision_randomness: 2.6,
            subdivision_randomness_progression: 3.0,
        }
    }
}

impl TreeDesc {
    /// Returns the authored mature tree shape interpolated toward a small sapling preset.
    ///
    /// The authored description is the exact age-1 endpoint. The mature deterministic skeleton
    /// remains the source topology at every age, but younger trees reveal fewer branch levels so
    /// scrubbing upward adds branches instead of starting with the mature silhouette.
    pub fn at_age(&self, age: f32) -> Self {
        let age = age.clamp(0.0, 1.0);
        let smooth_age = age * age * (3.0 - 2.0 * age);
        let lerp = |young: f32, mature: f32| young + (mature - young) * smooth_age;
        let mut aged = self.clone();
        aged.growth_age = age;
        aged.size = lerp(self.size * TREE_SAPLING_SIZE_RATIO, self.size);
        aged.trunk_thickness = lerp(
            self.trunk_thickness * TREE_SAPLING_THICKNESS_RATIO,
            self.trunk_thickness,
        );
        aged.leaf_density = lerp(
            self.leaf_density * TREE_SAPLING_LEAF_DENSITY_RATIO,
            self.leaf_density,
        );
        aged.leaves_size_level = lerp(
            TREE_SAPLING_LEAVES_SIZE_LEVEL.min(self.leaves_size_level) as f32,
            self.leaves_size_level as f32,
        )
        .round() as u32;
        aged
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LeafPlacement {
    /// Leaf voxel position relative to the tree origin.
    pub position: Vec3,
    /// Twig attachment point used to keep wind motion coherent within a spray.
    pub anchor: Vec3,
}

#[derive(Debug)]
struct BuiltObjects {
    branches: Vec<BranchSegment>,
    trunk_branches: Vec<usize>,
    leaf_branches: Vec<usize>,
    trunks: Vec<RoundCone>,
    leaf_positions: Vec<Vec3>,
    leaf_placements: Vec<LeafPlacement>,
}

#[derive(Debug)]
pub struct Tree {
    built_objects: BuiltObjects,
}

impl Tree {
    pub fn new(desc: TreeDesc) -> Self {
        let built_objects = Self::build(&desc);
        Tree { built_objects }
    }

    pub fn trunks(&self) -> &[RoundCone] {
        &self.built_objects.trunks
    }

    /// Obtain the leaf positions relative to the tree position.
    pub fn relative_leaf_positions(&self) -> &[Vec3] {
        &self.built_objects.leaf_positions
    }

    /// Obtain independently placeable leaf voxels relative to the tree position.
    pub fn relative_leaf_placements(&self) -> &[LeafPlacement] {
        &self.built_objects.leaf_placements
    }

    /// Full deterministic topology, including branches not yet revealed by age.
    /// Indices are stable across age/subdivision/thin-wood changes for one authored tree.
    pub fn branches(&self) -> &[BranchSegment] {
        &self.built_objects.branches
    }

    /// Branch ownership for every rendered trunk cone, in `trunks()` order.
    pub fn trunk_branch_indices(&self) -> &[usize] {
        &self.built_objects.trunk_branches
    }

    /// Attachment ownership in `relative_leaf_placements()` order.
    pub fn leaf_branch_indices(&self) -> &[usize] {
        &self.built_objects.leaf_branches
    }

    fn thickness_at_level(desc: &TreeDesc, base_thickness: f32, level: u32) -> f32 {
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

    fn build(desc: &TreeDesc) -> BuiltObjects {
        let mut branching_desc = desc.branching.normalized();
        let length_scale = (desc.size / TREE_DEFAULT_SIZE).max(0.0);
        branching_desc.initial_length *= length_scale;
        let mut skeleton_rng = StdRng::seed_from_u64(branching_desc.seed);
        let mut leaf_rng = StdRng::seed_from_u64(branching_desc.seed ^ 0xA511_E9B3_D6E8_FD9D);
        let mut subdivision_rng =
            StdRng::seed_from_u64(branching_desc.seed ^ 0x63D8_3595_B529_7A4D);
        let base_thickness = desc.trunk_thickness * desc.size;
        let skeleton = generate_branch_skeleton_with_rng(&branching_desc, &mut skeleton_rng);
        let visible_branch_levels =
            visible_branch_levels(desc.growth_age, branching_desc.iterations);

        let leaf_level = visible_branch_levels
            .saturating_sub(desc.leaf_offset)
            .max(1);
        let leaf_anchors = skeleton
            .segments
            .iter()
            .enumerate()
            .filter(|(_, segment)| segment.level < visible_branch_levels)
            .filter(|(_, segment)| segment.level.saturating_add(1) == leaf_level)
            .map(|(branch_index, segment)| {
                let direction = (segment.end - segment.start).normalize_or_zero();
                (branch_index, segment.end, direction)
            })
            .collect::<Vec<_>>();
        let leaf_positions = leaf_anchors
            .iter()
            .map(|(_, position, _)| *position)
            .collect();
        let (leaf_placements, leaf_branches) =
            generate_leaf_sprays(desc, &leaf_anchors, &mut leaf_rng);

        let mut trunks = Vec::new();
        let mut trunk_branches = Vec::new();
        for (branch_index, segment) in skeleton
            .segments
            .iter()
            .enumerate()
            .filter(|(_, segment)| segment.level < visible_branch_levels)
        {
            let first_cone = trunks.len();
            let thickness_start = Self::thickness_at_level(desc, base_thickness, segment.level);
            let thickness_end = Self::thickness_at_level(desc, base_thickness, segment.level + 1);
            let cone = RoundCone::new(
                thickness_start.max(0.),
                segment.start,
                thickness_end.max(0.),
                segment.end,
            );
            // subdivision now respects the toggle
            let subdivided_cones =
                subdivide_trunk_segment(&cone, desc, segment.level, &mut subdivision_rng);
            // Authored radii are geometry, not a lighting-normal workaround.
            trunks.extend(subdivided_cones);
            trunk_branches.extend(std::iter::repeat_n(branch_index, trunks.len() - first_cone));
        }

        BuiltObjects {
            branches: skeleton.segments,
            trunk_branches,
            leaf_branches,
            trunks,
            leaf_positions,
            leaf_placements,
        }
    }
}

fn visible_branch_levels(age: f32, mature_levels: u32) -> u32 {
    let mature_levels = mature_levels.max(1);
    let young_levels = TREE_SAPLING_VISIBLE_BRANCH_LEVELS.min(mature_levels);
    let age = age.clamp(0.0, 1.0);
    let smooth_age = age * age * (3.0 - 2.0 * age);
    let level_count = mature_levels - young_levels + 1;
    (young_levels + (smooth_age * level_count as f32).floor() as u32).min(mature_levels)
}

fn generate_leaf_sprays(
    desc: &TreeDesc,
    anchors: &[(usize, Vec3, Vec3)],
    rng: &mut StdRng,
) -> (Vec<LeafPlacement>, Vec<usize>) {
    let diameter = 2.0_f32.powi(desc.leaves_size_level.min(8) as i32);
    let along_radius = (diameter * 0.5).max(1.0);
    let width_radius = (along_radius * desc.leaf_spray_width_ratio.max(0.05)).max(1.0);
    let thickness_radius = (along_radius * desc.leaf_spray_thickness_ratio.max(0.05)).max(1.0);
    let spray_volume = 4.0 / 3.0 * PI * along_radius * width_radius * thickness_radius;
    let leaves_per_spray = (spray_volume * desc.leaf_density.max(0.0)).round() as usize;
    let mut placements = Vec::with_capacity(anchors.len().saturating_mul(leaves_per_spray));

    let mut bindings = Vec::with_capacity(placements.capacity());
    for &(branch_index, anchor, branch_direction) in anchors {
        let axis = if branch_direction.length_squared() > 0.0 {
            branch_direction
        } else {
            Vec3::Y
        };
        let (side, vertical) = stable_perpendicular_basis(axis);
        let spray_center = anchor + axis * along_radius * desc.leaf_spray_tip_offset_ratio;
        let mut emitted = 0;
        let mut attempts = 0;
        let max_attempts = leaves_per_spray.saturating_mul(8).max(8);

        while emitted < leaves_per_spray && attempts < max_attempts {
            attempts += 1;
            let sample = Vec3::new(
                rng.random_range(-1.0..=1.0),
                rng.random_range(-1.0..=1.0),
                rng.random_range(-1.0..=1.0),
            );
            if sample.length_squared() > 1.0 {
                continue;
            }

            // A branch-oriented ellipsoid creates a readable spray instead of another
            // world-aligned ball. Individual positions remain independent render instances.
            let position = spray_center
                + axis * (sample.x * along_radius)
                + side * (sample.y * width_radius)
                + vertical * (sample.z * thickness_radius);
            placements.push(LeafPlacement { position, anchor });
            bindings.push(branch_index);
            emitted += 1;
        }
    }

    (placements, bindings)
}

/// Subdivides a single RoundCone into multiple, smaller, slightly perturbed cones.
/// Respects the `enable_subdivision` toggle.
fn subdivide_trunk_segment(
    cone: &RoundCone,
    desc: &TreeDesc,
    level: u32,
    rng: &mut StdRng,
) -> Vec<RoundCone> {
    // early-out if subdivision is disabled
    if !desc.enable_subdivision {
        return vec![cone.clone()];
    }

    let axis = cone.center_b() - cone.center_a();

    // do not subdivide if the segment is too short or if subdivision is effectively disabled.
    if desc.subdivision_count_max <= 1 {
        return vec![cone.clone()];
    }

    // 0.0 at root, 1.0 at the deepest level
    let t = (level as f32) / (desc.branching.iterations as f32).max(1.0);

    // Shape the curve with an exponent:
    //  - 1.0 => roughly linear
    //  - >1.0 => more weight toward the tip
    //  - <1.0 => more weight toward the base
    let curve_exp = desc.subdivision_randomness_progression.max(0.01);
    let mut weight = t.powf(curve_exp);

    // Ensure the base still has some randomness:
    // min_root_factor: 0.0 = allow root to be ~0, 0.2 = root is at least 20% of max.
    let min_root_factor = 0.2;
    let min_weight = min_root_factor;
    // Remap [0..1] into [min_weight..1]
    weight = min_weight + (1.0 - min_weight) * weight;

    // Final randomness scale for this level
    let iteration_randomness = desc.subdivision_randomness * weight;

    let num_segments = if desc.subdivision_count_min >= desc.subdivision_count_max {
        desc.subdivision_count_min
    } else {
        rng.random_range(desc.subdivision_count_min..=desc.subdivision_count_max)
    };

    if num_segments <= 1 {
        return vec![cone.clone()];
    }

    let mut subdivided_trunks = Vec::with_capacity(num_segments as usize);
    let mut current_pos = cone.center_a();
    let segment_vec = axis / num_segments as f32;

    let (perp1, perp2) = stable_perpendicular_basis(axis);

    let root_radius = desc.trunk_thickness * desc.size;

    for i in 1..=num_segments {
        let start_t = (i - 1) as f32 / num_segments as f32;
        let end_t = i as f32 / num_segments as f32;
        let segment_start_radius = cone.radius_a() * (1.0 - start_t) + cone.radius_b() * start_t;
        let segment_end_radius = cone.radius_a() * (1.0 - end_t) + cone.radius_b() * end_t;

        let mut next_pos;

        if i == num_segments {
            next_pos = cone.center_b();
        } else {
            next_pos = current_pos + segment_vec;
            if iteration_randomness > 0.0 {
                let random_angle = rng.random_range(0.0..2.0 * PI);
                let random_dir_perp = perp1 * random_angle.cos() + perp2 * random_angle.sin();

                // 0 at root, → 1 as radius gets small
                let radius_ratio =
                    (segment_start_radius / root_radius.max(f32::MIN_POSITIVE)).clamp(0.0, 1.0);
                let tip_bias = 1.0 - radius_ratio; // 0 at base, 1 at tip-ish

                let displacement_magnitude = segment_start_radius
                    * iteration_randomness
                    * tip_bias
                    * rng.random_range(0.5..=1.0);

                next_pos += random_dir_perp * displacement_magnitude;
            }
        }

        subdivided_trunks.push(RoundCone::new(
            segment_start_radius.max(0.),
            current_pos,
            segment_end_radius.max(0.),
            next_pos,
        ));

        current_pos = next_pos;
    }

    subdivided_trunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_bindings_survive_subdivision_and_age() {
        for seed in [7, 122] {
            let mut desc = TreeDesc::default();
            desc.branching.seed = seed;
            let mature = Tree::new(desc.clone());
            for age in [0.0, 0.25, 0.5, 1.0] {
                for subdivision in [false, true] {
                    desc.growth_age = age;
                    desc.enable_subdivision = subdivision;
                    let tree = Tree::new(desc.clone());
                    assert_eq!(tree.branches(), mature.branches());
                    assert_eq!(tree.trunks().len(), tree.trunk_branch_indices().len());
                    assert_eq!(
                        tree.relative_leaf_placements().len(),
                        tree.leaf_branch_indices().len()
                    );
                    let visible_levels = visible_branch_levels(age, desc.branching.iterations);
                    for &branch in tree.trunk_branch_indices() {
                        assert!(tree.branches()[branch].level < visible_levels);
                    }
                    for (leaf, &branch) in tree
                        .relative_leaf_placements()
                        .iter()
                        .zip(tree.leaf_branch_indices())
                    {
                        assert_eq!(leaf.anchor, tree.branches()[branch].end);
                        assert!(tree.branches()[branch].level < visible_levels);
                    }
                }
            }
        }
    }

    #[test]
    fn branch_identity_survives_age_scaling() {
        let desc = TreeDesc::default();
        let mature = Tree::new(desc.clone());
        for age in [0.0, 0.25, 0.5, 1.0] {
            let tree = Tree::new(desc.at_age(age));
            assert_eq!(tree.branches().len(), mature.branches().len());
            for (young, adult) in tree.branches().iter().zip(mature.branches()) {
                assert_eq!(young.parent, adult.parent);
                assert_eq!(young.level, adult.level);
                assert_eq!(young.role, adult.role);
            }
        }
    }

    #[test]
    fn authored_radii_survive_subdivision_and_age_without_a_minimum() {
        for subdivision in [false, true] {
            let desc = TreeDesc {
                size: 20.,
                enable_subdivision: subdivision,
                ..Default::default()
            };
            for age in [0., 0.5, 1.] {
                let aged = desc.at_age(age);
                let tree = Tree::new(aged.clone());
                assert!(tree.trunks().iter().any(|cone| cone.radius_b() < 1.));
                for (branch, segment) in tree.branches().iter().enumerate() {
                    let cones: Vec<_> = tree
                        .trunks()
                        .iter()
                        .zip(tree.trunk_branch_indices())
                        .filter_map(|(cone, &id)| (id == branch).then_some(cone))
                        .collect();
                    if segment.level >= visible_branch_levels(age, aged.branching.iterations) {
                        assert!(cones.is_empty());
                        continue;
                    }
                    assert!(
                        !cones.is_empty(),
                        "visible thin branches must not be culled"
                    );
                    let base = aged.trunk_thickness * aged.size;
                    assert_eq!(
                        cones[0].radius_a(),
                        Tree::thickness_at_level(&aged, base, segment.level)
                    );
                    assert_eq!(
                        cones.last().unwrap().radius_b(),
                        Tree::thickness_at_level(&aged, base, segment.level + 1)
                    );
                    assert_eq!(cones[0].center_a(), segment.start);
                    assert_eq!(cones.last().unwrap().center_b(), segment.end);
                    for pair in cones.windows(2) {
                        assert_eq!(pair[0].radius_b(), pair[1].radius_a());
                    }
                    for cone in cones {
                        assert!(cone.center_a().is_finite() && cone.center_b().is_finite());
                        assert!(cone.radius_a() >= 0. && cone.radius_b() >= 0.);
                        assert!(cone.signed_distance(cone.center_a()).is_finite());
                    }
                }
            }
            let raw = Tree::new(desc.clone());
            assert!(raw.trunks().iter().any(|c| c.radius_b() < 0.5));
            assert_eq!(
                format!("{:?}", raw.trunks()),
                format!("{:?}", Tree::new(desc).trunks())
            );
        }
    }

    #[test]
    fn retired_guard_settings_are_ignored_and_not_resaved() {
        let desc = TreeDesc::default();
        for preserve in [false, true] {
            for cull in [false, true] {
                let mut old = serde_json::to_value(&desc).unwrap();
                old["preserve_thin_branches"] = preserve.into();
                old["cull_thin_branches"] = cull.into();
                let loaded: TreeDesc = serde_json::from_value(old).unwrap();
                assert_eq!(loaded, desc);
                let saved = serde_json::to_value(loaded).unwrap();
                assert!(saved.get("preserve_thin_branches").is_none());
                assert!(saved.get("cull_thin_branches").is_none());
            }
        }
    }

    #[test]
    fn nonpositive_radius_does_not_make_nan_or_inflate_wood() {
        for thickness in [0., -1.] {
            let zero = Tree::new(TreeDesc {
                trunk_thickness: thickness,
                ..TreeDesc::default()
            });
            for cone in zero.trunks() {
                assert_eq!(cone.radius_a(), 0.);
                assert_eq!(cone.radius_b(), 0.);
                assert!(cone.center_a().is_finite() && cone.center_b().is_finite());
                assert!(cone.signed_distance(Vec3::ZERO).is_finite());
            }
        }
    }

    #[test]
    fn mature_tree_age_preserves_the_authored_description_exactly() {
        let desc = TreeDesc::default();

        assert_eq!(desc.at_age(1.0), desc);
        assert_eq!(desc.at_age(f32::INFINITY), desc);
    }

    #[test]
    fn tree_age_uses_a_deterministic_smaller_sapling_endpoint() {
        let desc = TreeDesc::default();
        let sapling = desc.at_age(0.0);
        let halfway = desc.at_age(0.5);

        assert_eq!(sapling.branching, desc.branching);
        assert_eq!(sapling.growth_age, 0.0);
        assert_eq!(halfway.growth_age, 0.5);
        assert!(sapling.size > 0.0);
        assert!(sapling.size < halfway.size && halfway.size < desc.size);
        assert!(sapling.trunk_thickness < halfway.trunk_thickness);
        assert!(halfway.trunk_thickness < desc.trunk_thickness);
        assert!(sapling.leaf_density < halfway.leaf_density);
        assert!(halfway.leaf_density < desc.leaf_density);
        assert!(sapling.leaves_size_level <= halfway.leaves_size_level);
        assert!(halfway.leaves_size_level <= desc.leaves_size_level);
        assert_eq!(desc.at_age(-1.0), sapling);
    }

    #[test]
    fn tree_age_reveals_more_of_the_mature_branch_skeleton() {
        let desc = TreeDesc {
            enable_subdivision: false,
            ..TreeDesc::default()
        };
        let sapling = Tree::new(desc.at_age(0.0));
        let halfway = Tree::new(desc.at_age(0.5));
        let mature = Tree::new(desc.at_age(1.0));

        assert_eq!(sapling.trunks().len(), 1);
        assert!(sapling.trunks().len() < halfway.trunks().len());
        assert!(halfway.trunks().len() < mature.trunks().len());
    }

    #[test]
    fn visible_branch_levels_progress_monotonically_to_the_authored_depth() {
        let mature_levels = 7;
        let levels =
            [0.0, 0.25, 0.5, 0.75, 1.0].map(|age| visible_branch_levels(age, mature_levels));

        assert_eq!(levels[0], 1);
        assert_eq!(levels[4], mature_levels);
        assert!(levels.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn leaf_offset_zero_places_leaves_on_terminal_branch_tips() {
        let desc = TreeDesc {
            branching: BranchingDesc {
                iterations: 1,
                branch_start_fraction: 0.0,
                branch_count_min: 2,
                branch_count_max: 2,
                randomness: 0.0,
                segment_length_variation: 0.0,
                ..default_tree_branching_desc()
            },
            leaf_offset: 0,
            enable_subdivision: false,
            ..TreeDesc::default()
        };

        let tree = Tree::new(desc);

        assert_eq!(tree.relative_leaf_positions().len(), 2);
    }

    #[test]
    fn leaf_sprays_are_deterministic_independent_placements() {
        let desc = TreeDesc {
            branching: BranchingDesc {
                seed: 7,
                iterations: 2,
                branch_count_min: 2,
                branch_count_max: 2,
                ..default_tree_branching_desc()
            },
            leaves_size_level: 3,
            leaf_offset: 0,
            ..TreeDesc::default()
        };

        let first = Tree::new(desc.clone());
        let second = Tree::new(desc);

        assert!(!first.relative_leaf_positions().is_empty());
        assert!(
            first.relative_leaf_placements().len() > first.relative_leaf_positions().len(),
            "each foliage anchor should expand into independently placed leaf voxels"
        );
        assert_eq!(
            first
                .relative_leaf_placements()
                .iter()
                .map(|leaf| (leaf.position, leaf.anchor))
                .collect::<Vec<_>>(),
            second
                .relative_leaf_placements()
                .iter()
                .map(|leaf| (leaf.position, leaf.anchor))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn leaf_density_does_not_change_trunk_subdivision() {
        let sparse_desc = TreeDesc {
            branching: BranchingDesc {
                seed: 7,
                iterations: 3,
                ..default_tree_branching_desc()
            },
            leaf_density: 0.005,
            ..TreeDesc::default()
        };
        let mut dense_desc = sparse_desc.clone();
        dense_desc.leaf_density = 0.2;

        let sparse_tree = Tree::new(sparse_desc);
        let dense_tree = Tree::new(dense_desc);

        assert_eq!(sparse_tree.trunks().len(), dense_tree.trunks().len());
        for (sparse, dense) in sparse_tree.trunks().iter().zip(dense_tree.trunks()) {
            assert_eq!(sparse.center_a(), dense.center_a());
            assert_eq!(sparse.center_b(), dense.center_b());
            assert_eq!(sparse.radius_a(), dense.radius_a());
            assert_eq!(sparse.radius_b(), dense.radius_b());
        }
    }

    #[test]
    fn tree_size_scales_branch_length() {
        let desc = TreeDesc {
            branching: BranchingDesc {
                seed: 1,
                iterations: 1,
                randomness: 0.0,
                segment_length_variation: 0.0,
                ..default_tree_branching_desc()
            },
            enable_subdivision: false,
            size: TREE_DEFAULT_SIZE,
            ..TreeDesc::default()
        };
        let mut half_size_desc = desc.clone();
        half_size_desc.size = TREE_DEFAULT_SIZE * 0.5;

        let default_tree = Tree::new(desc);
        let half_size_tree = Tree::new(half_size_desc);
        let default_segment = &default_tree.trunks()[0];
        let half_size_segment = &half_size_tree.trunks()[0];

        let default_length = default_segment
            .center_b()
            .distance(default_segment.center_a());
        let half_size_length = half_size_segment
            .center_b()
            .distance(half_size_segment.center_a());

        assert!((half_size_length - default_length * 0.5).abs() < 0.001);
    }
}
