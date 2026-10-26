use super::growth;
#[cfg(test)]
use super::growth::TREE_DEFAULT_SIZE;
use crate::branch_skeleton::{BranchSegment, BranchSegmentRole, BranchingDesc};
use crate::geom::RoundCone;
use crate::util::stable_perpendicular_basis;
use glam::Vec3;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

const TREE_SAPLING_SIZE_RATIO: f32 = 0.12;
const TREE_SAPLING_THICKNESS_RATIO: f32 = 0.70;
const TREE_SAPLING_LEAF_DENSITY_RATIO: f32 = 0.25;
const TREE_SAPLING_LEAVES_SIZE_LEVEL: u32 = 1;

fn mature_tree_growth_age() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default = "TreeDesc::legacy_wire_defaults")]
pub struct TreeDesc {
    pub species: super::TreeSpecies,
    /// Legacy Conifer A/B architecture, retained by garden snapshots. Current species always
    /// use their own axes. Missing species is a decode-only retired-tree migration marker.
    pub axis_model: bool,
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

impl Default for TreeDesc {
    fn default() -> Self {
        super::TreeSpecies::AppleTree.default_desc()
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
        let mut aged = self.clone().into_current();
        aged.growth_age = age;
        aged.size = lerp(aged.size * TREE_SAPLING_SIZE_RATIO, aged.size);
        aged.trunk_thickness = lerp(
            aged.trunk_thickness * TREE_SAPLING_THICKNESS_RATIO,
            aged.trunk_thickness,
        );
        aged.leaf_density = lerp(
            aged.leaf_density * TREE_SAPLING_LEAF_DENSITY_RATIO,
            aged.leaf_density,
        );
        aged.leaves_size_level = lerp(
            TREE_SAPLING_LEAVES_SIZE_LEVEL.min(aged.leaves_size_level) as f32,
            aged.leaves_size_level as f32,
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
    axis_segments: Vec<super::AxisSegment>,
    trunk_branches: Vec<usize>,
    leaf_branches: Vec<usize>,
    trunks: Vec<RoundCone>,
    physical_trunks: Vec<RoundCone>,
    leaf_positions: Vec<Vec3>,
    leaf_placements: Vec<LeafPlacement>,
}

#[derive(Debug)]
pub struct Tree {
    exfoliating_bark: bool,
    built_objects: BuiltObjects,
}

impl Tree {
    pub fn new(desc: TreeDesc) -> Self {
        let desc = desc.into_current();
        let built_objects = Self::build(&desc);
        Tree {
            built_objects,
            exfoliating_bark: desc.species.exfoliating_bark(),
        }
    }

    pub(crate) fn bark_style(&self) -> u32 {
        u32::from(self.exfoliating_bark)
    }

    pub fn trunks(&self) -> &[RoundCone] {
        &self.built_objects.trunks
    }

    /// Root-to-crown height of the generated rest geometry, including half-voxel leaf extents.
    /// This is not the leader height, an age-independent authored size, or a wind/GPU bound.
    pub fn height_voxels(&self) -> f32 {
        self.trunks()
            .iter()
            .map(|cone| cone.aabb().max().y)
            .chain(
                self.relative_leaf_placements()
                    .iter()
                    .map(|leaf| leaf.position.y + 0.5),
            )
            .fold(0., f32::max)
    }

    /// The exact world-voxel wood used by stamping, surface extraction and ownership.
    /// Keep translation arithmetic identical; a local-space sign test is not equivalent
    /// at f32 voxel boundaries. `origin` is in world units, cones are in voxel units.
    pub fn world_trunks(&self, origin: Vec3) -> Vec<RoundCone> {
        Self::world_cones(self.trunks(), origin)
    }

    /// Shared physical-tree policy: visible root-axis wood in the authored rest pose.
    /// A side axis's continuation is still a side axis, regardless of species/level.
    /// No wind/kick pose or caller-selected species policy can enter this interface.
    pub(crate) fn world_physical_trunks(&self, origin: Vec3) -> Vec<RoundCone> {
        Self::world_cones(&self.built_objects.physical_trunks, origin)
    }

    fn world_cones(cones: &[RoundCone], origin: Vec3) -> Vec<RoundCone> {
        cones
            .iter()
            .cloned()
            .map(|mut cone| {
                cone.transform(origin * 256.);
                cone
            })
            .collect()
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

    /// Explicit axis identity, local arc position, branch order and radii for the axis model.
    /// Legacy skeletons return an empty slice; their existing recursive semantics are unchanged.
    pub fn axis_segments(&self) -> &[super::AxisSegment] {
        &self.built_objects.axis_segments
    }

    /// Branch ownership for every rendered trunk cone, in `trunks()` order.
    pub fn trunk_branch_indices(&self) -> &[usize] {
        &self.built_objects.trunk_branches
    }

    /// Attachment ownership in `relative_leaf_placements()` order.
    pub fn leaf_branch_indices(&self) -> &[usize] {
        &self.built_objects.leaf_branches
    }

    fn build(desc: &TreeDesc) -> BuiltObjects {
        let growth::GrowthSkeleton {
            branches: segments,
            axes: axis_segments,
            wood,
            foliage: leaf_anchors,
        } = growth::generate(desc);
        let mut leaf_rng = StdRng::seed_from_u64(desc.branching.seed ^ 0xA511_E9B3_D6E8_FD9D);
        let mut subdivision_rng =
            StdRng::seed_from_u64(desc.branching.seed ^ 0x63D8_3595_B529_7A4D);
        let leaf_positions = leaf_anchors
            .iter()
            .map(|(_, position, _)| *position)
            .collect();
        let (leaf_placements, leaf_branches) =
            generate_leaf_sprays(desc, &leaf_anchors, &mut leaf_rng);

        let mut trunks = Vec::new();
        let mut physical_trunks = Vec::new();
        let mut trunk_branches = Vec::new();
        // Roles describe local continuation. Root-axis membership also requires
        // every ancestor to be a continuation: never promote a lateral subtree.
        let mut root_axes = Vec::with_capacity(segments.len());
        for segment in &segments {
            root_axes.push(
                segment.role == BranchSegmentRole::MainAxis
                    && segment.parent.is_none_or(|parent| root_axes[parent]),
            );
        }
        for section in wood {
            let branch_index = section.branch;
            let segment = &segments[branch_index];
            let first_cone = trunks.len();
            let cone = RoundCone::new(
                section.radius_start.max(0.),
                segment.start,
                section.radius_end.max(0.),
                segment.end,
            );
            // subdivision now respects the toggle
            let subdivided_cones =
                subdivide_trunk_segment(&cone, desc, segment.level, &mut subdivision_rng);
            // Reuse exact authored/subdivided cones, not a straight approximation.
            if root_axes[branch_index] {
                physical_trunks.extend(subdivided_cones.iter().cloned());
            }
            // Authored radii are geometry, not a lighting-normal workaround.
            trunks.extend(subdivided_cones);
            trunk_branches.extend(std::iter::repeat_n(branch_index, trunks.len() - first_cone));
        }

        BuiltObjects {
            branches: segments,
            axis_segments,
            trunk_branches,
            leaf_branches,
            trunks,
            physical_trunks,
            leaf_positions,
            leaf_placements,
        }
    }
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
    let leaves_per_spray = if desc.leaf_density > 0. {
        // Positive foliage must survive quantization on tiny young sprays.
        (spray_volume * desc.leaf_density).round().max(1.) as usize
    } else {
        0
    };
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

            if desc.species.palmate_foliage() {
                // Five broad lobes and recessed sinuses in the flattened voxel spray.
                // Keep all samples attached to the same twig for coherent wind response.
                let angle = sample.y.atan2(sample.x);
                let envelope = 0.72 + 0.28 * (5. * angle).cos();
                if sample.x.hypot(sample.y) > envelope {
                    continue;
                }
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
        if leaves_per_spray > 0 && emitted == 0 {
            // Keep the visibility contract even when every bounded random attempt
            // misses the ellipsoid. Its center is always an admissible sample.
            placements.push(LeafPlacement {
                position: spray_center,
                anchor,
            });
            bindings.push(branch_index);
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
    use crate::tree_gen::growth::{thickness_at_level, visible_branch_levels};

    #[test]
    fn physical_trunks_use_root_axis_ancestry_for_every_registered_species_and_age() {
        let mut saw_lateral_continuation = false;
        for species in super::super::TreeSpecies::ALL {
            for axis_model in [false, true] {
                let mut authored = TreeDesc::default();
                authored.branching.iterations = 3;
                let mut desc = species.mature_desc(&authored);
                desc.axis_model = axis_model;
                for age in [0., 0.25, 1.] {
                    let tree = Tree::new(desc.at_age(age));
                    let root = |mut branch: usize| loop {
                        if tree.branches()[branch].role != BranchSegmentRole::MainAxis {
                            break false;
                        }
                        match tree.branches()[branch].parent {
                            Some(parent) => branch = parent,
                            None => break true,
                        }
                    };
                    let expected: Vec<_> = tree
                        .trunks()
                        .iter()
                        .zip(tree.trunk_branch_indices())
                        .filter(|(_, branch)| root(**branch))
                        .map(|(cone, _)| cone)
                        .collect();
                    let bits = |cone: &RoundCone| {
                        [
                            cone.center_a()
                                .extend(cone.radius_a())
                                .to_array()
                                .map(f32::to_bits),
                            cone.center_b()
                                .extend(cone.radius_b())
                                .to_array()
                                .map(f32::to_bits),
                        ]
                    };
                    assert_eq!(
                        tree.built_objects
                            .physical_trunks
                            .iter()
                            .map(bits)
                            .collect::<Vec<_>>(),
                        expected.iter().map(|cone| bits(cone)).collect::<Vec<_>>()
                    );
                    if age == 1. {
                        assert!(!expected.is_empty());
                        assert!(expected.len() < tree.trunks().len());
                        saw_lateral_continuation |=
                            tree.branches().iter().enumerate().any(|(i, branch)| {
                                branch.role == BranchSegmentRole::MainAxis && !root(i)
                            });
                    }
                    let shifted = Vec3::new(32., 0.25, -7.);
                    let expected_world: Vec<_> = expected
                        .into_iter()
                        .cloned()
                        .map(|mut cone| {
                            cone.transform(shifted * 256.);
                            bits(&cone)
                        })
                        .collect();
                    assert_eq!(
                        tree.world_physical_trunks(shifted)
                            .iter()
                            .map(bits)
                            .collect::<Vec<_>>(),
                        expected_world
                    );
                }
            }
        }
        assert!(
            saw_lateral_continuation,
            "exercise local continuation on a side axis too"
        );
    }

    #[test]
    fn axis_age_preserves_identity_and_reveals_connected_branches() {
        let desc = super::super::TreeSpecies::Conifer.mature_desc(&TreeDesc::default());
        let mature = Tree::new(desc.clone());
        let mut previous = 0;
        for age in [0., 0.25, 0.5, 0.75, 1.] {
            let tree = Tree::new(desc.at_age(age));
            assert_eq!(tree.branches().len(), mature.branches().len());
            assert!(tree.trunks().len() >= previous);
            previous = tree.trunks().len();
            let visible = tree.trunk_branch_indices();
            for &i in visible {
                if let Some(parent) = tree.branches()[i].parent {
                    assert!(visible.contains(&parent));
                }
                assert_eq!(
                    tree.axis_segments()[i].axis_id,
                    mature.axis_segments()[i].axis_id
                );
                assert_eq!(tree.branches()[i].parent, mature.branches()[i].parent);
            }
            assert_eq!(
                tree.relative_leaf_placements().len(),
                tree.leaf_branch_indices().len()
            );
            for (leaf, &i) in tree
                .relative_leaf_placements()
                .iter()
                .zip(tree.leaf_branch_indices())
            {
                assert!(visible.contains(&i));
                assert_eq!(leaf.anchor, tree.branches()[i].end);
            }
            crate::tree_gen::pose::TreePose::new(tree.branches(), Vec3::ZERO).unwrap();
        }
        assert_eq!(previous, mature.branches().len());
    }

    #[test]
    fn species_seed_and_architecture_round_trip_and_old_trees_convert_to_current_apples() {
        let desc = super::super::TreeSpecies::Conifer.mature_desc(&TreeDesc::default());
        let json = serde_json::to_string(&desc).unwrap();
        let restored: TreeDesc = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, desc);
        assert_eq!(Tree::new(restored).branches(), Tree::new(desc).branches());
        for species in [None, Some("garden_tree")] {
            let mut old = serde_json::to_value(TreeDesc::legacy_wire_defaults()).unwrap();
            old["size"] = 17.25.into();
            old["branching"]["seed"] = 907.into();
            if let Some(species) = species {
                old["species"] = species.into();
            } else {
                old.as_object_mut().unwrap().remove("species");
                old.as_object_mut().unwrap().remove("axis_model");
            }
            let retired: TreeDesc = serde_json::from_value(old).unwrap();
            assert!(retired.is_retired());
            let expected = super::super::TreeSpecies::AppleTree.mature_desc(&retired);
            let current = retired.clone().into_current();
            assert_eq!(current, expected);
            assert_eq!(current.size, 17.25);
            assert_eq!(current.branching.seed, 907);
            assert!(!serde_json::to_string(&current)
                .unwrap()
                .contains("garden_tree"));
            assert_eq!(
                Tree::new(retired.clone()).branches(),
                Tree::new(current.clone()).branches()
            );
            for age in [0., 0.45, 1.] {
                assert_eq!(retired.at_age(age), current.at_age(age));
            }
        }
        let missing: TreeDesc = serde_json::from_str("{}").unwrap();
        assert!(missing.is_retired());
        assert_eq!(missing.axis_model, false);
        assert_eq!(missing.trunk_thickness, 0.4);
        assert!(missing.enable_subdivision);
        assert_eq!(
            TreeDesc::default().species,
            super::super::TreeSpecies::AppleTree
        );
    }

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
                species: super::super::TreeSpecies::Conifer,
                axis_model: false,
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
                        thickness_at_level(&aged, base, segment.level)
                    );
                    assert_eq!(
                        cones.last().unwrap().radius_b(),
                        thickness_at_level(&aged, base, segment.level + 1)
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
                ..TreeDesc::legacy_wire_defaults().branching
            },
            leaf_offset: 0,
            enable_subdivision: false,
            species: super::super::TreeSpecies::Conifer,
            axis_model: false,
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
                ..TreeDesc::legacy_wire_defaults().branching
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
                ..TreeDesc::legacy_wire_defaults().branching
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
                ..TreeDesc::legacy_wire_defaults().branching
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
