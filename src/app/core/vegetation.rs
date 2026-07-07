use super::App;
use crate::app::world_edits::{
    BuildEdit, ClearVoxelRegionEdit, CubePlacementEdit, FencePostPlacementEdit, TerrainBrushEdit,
    TerrainRemovalEdit, TreeAddOptions, TreePlacement, TreePlacementEdit, VoxelEdit, WorldEditPlan,
};
use crate::app::world_ops;
use crate::builder::{ChunkModifyReadback, VOXEL_TYPE_CHERRY_WOOD, VOXEL_TYPE_OAK_WOOD};
use crate::flora::species;
use crate::geom::{build_bvh, Cuboid, RoundCone, Sphere, UAabb3};
use crate::procedual_placer::{generate_positions, PlacerDesc};
use crate::tree_gen::{Tree, TreeDesc};
use crate::util::cluster_positions;
use anyhow::Result;
use glam::{UVec3, Vec2, Vec3};
use rand::{Rng, RngExt};
use std::collections::HashSet;
use std::time::Instant;

const AUTHORED_FLORA_GROWTH_MATURE: u32 = 0xff;
const AUTHORED_FLORA_NATURAL_CANDIDATES_PER_PLANT: u32 = 48;

#[derive(Clone, Copy, Debug)]
struct SpecialFloraDistributionParams {
    plants_per_release: u32,
    cluster_radius_voxels: f32,
    min_spacing_voxels: f32,
    cluster_bias: f32,
    outlier_chance: f32,
}

#[derive(Debug, Clone)]
pub(super) struct TreeVariationConfig {
    pub size_variance: f32,
    pub trunk_thickness_variance: f32,
    pub trunk_thickness_min_variance: f32,
    pub spread_variance: f32,
    pub randomness_variance: f32,
    pub vertical_tendency_variance: f32,
    pub branch_probability_variance: f32,
    pub leaves_size_level_variance: f32,
    pub iterations_variance: f32,
    pub initial_length_variance: f32,
    pub length_dropoff_variance: f32,
    pub thickness_reduction_variance: f32,
}

impl Default for TreeVariationConfig {
    fn default() -> Self {
        TreeVariationConfig {
            size_variance: 0.0,
            trunk_thickness_variance: 0.0,
            trunk_thickness_min_variance: 0.0,
            spread_variance: 0.0,
            randomness_variance: 0.0,
            vertical_tendency_variance: 0.0,
            branch_probability_variance: 0.0,
            leaves_size_level_variance: 0.0,
            iterations_variance: 0.0,
            initial_length_variance: 0.0,
            length_dropoff_variance: 0.0,
            thickness_reduction_variance: 0.0,
        }
    }
}

impl TreeVariationConfig {
    #[allow(dead_code)]
    pub fn edit_by_gui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        ui.heading("Variation Settings");

        changed |= ui
            .add(egui::Slider::new(&mut self.size_variance, 0.0..=1.0).text("Size Variance"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.trunk_thickness_variance, 0.0..=1.0)
                    .text("Thickness Variance"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.trunk_thickness_min_variance, 0.0..=1.0)
                    .text("Min Thickness Variance"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.iterations_variance, 0.0..=5.0)
                    .text("Iterations Variance"),
            )
            .changed();

        ui.separator();
        ui.heading("Shape Variation");

        changed |= ui
            .add(
                egui::Slider::new(&mut self.initial_length_variance, 0.0..=1.0)
                    .text("Initial Length Variance"),
            )
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.spread_variance, 0.0..=1.0).text("Spread Variance"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.vertical_tendency_variance, 0.0..=1.0)
                    .text("Vertical Tendency Variance"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.length_dropoff_variance, 0.0..=1.0)
                    .text("Length Dropoff Variance"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.thickness_reduction_variance, 0.0..=1.0)
                    .text("Thickness Reduction Variance"),
            )
            .changed();

        ui.separator();
        ui.heading("Branching Variation");

        changed |= ui
            .add(
                egui::Slider::new(&mut self.branch_probability_variance, 0.0..=1.0)
                    .text("Branch Probability Variance"),
            )
            .changed();

        ui.separator();
        ui.heading("Detail Variation");

        changed |= ui
            .add(
                egui::Slider::new(&mut self.randomness_variance, 0.0..=1.0)
                    .text("Randomness Variance"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.leaves_size_level_variance, 0.0..=5.0)
                    .text("Leaves Size Variance"),
            )
            .changed();

        changed
    }
}

struct CompiledFencePlacement {
    voxel_edit: VoxelEdit,
    rebuild_bound: UAabb3,
}

struct CompiledTerrainSurfaceRemoval {
    voxel_edit: VoxelEdit,
    rebuild_bound: UAabb3,
}

struct CompiledTerrainBrushSurfaceEdit {
    rebuild_bound: UAabb3,
}

#[allow(dead_code)]
struct FencePostPlacementService;

#[allow(dead_code)]
impl FencePostPlacementService {
    fn compile(
        edit: FencePostPlacementEdit,
        terrain_height: f32,
        voxel_type: u32,
    ) -> CompiledFencePlacement {
        let downward_offset = edit.height * 0.4;
        let mut base = Vec3::new(edit.horizontal.x, terrain_height, edit.horizontal.y) * 256.0;
        base.y -= downward_offset;

        let cuboid = Cuboid::new(
            base + Vec3::Y * (edit.height * 0.5),
            Vec3::new(edit.half_width, edit.height * 0.5, edit.half_depth),
        );
        let cuboids = vec![cuboid];
        let leaves_data_sequential = vec![0_u32];
        let aabbs = vec![cuboids[0].aabb()];
        let bvh_nodes = build_bvh(&aabbs, &leaves_data_sequential).unwrap();
        let rebuild_bound =
            UAabb3::new(bvh_nodes[0].aabb.min_uvec3(), bvh_nodes[0].aabb.max_uvec3());

        CompiledFencePlacement {
            voxel_edit: VoxelEdit::StampCuboids {
                bvh_nodes,
                cuboids,
                voxel_type,
            },
            rebuild_bound,
        }
    }
}

#[allow(dead_code)]
struct CubePlacementService;

impl CubePlacementService {
    #[allow(dead_code)]
    fn compile(edit: CubePlacementEdit) -> CompiledFencePlacement {
        let half_extent = edit.size * 0.5;
        let cuboid = Cuboid::new(
            edit.center * 256.0,
            Vec3::new(half_extent, half_extent, half_extent),
        );
        let cuboids = vec![cuboid];
        let leaves_data_sequential = vec![0_u32];
        let aabbs = vec![cuboids[0].aabb()];
        let bvh_nodes = build_bvh(&aabbs, &leaves_data_sequential).unwrap();
        let rebuild_bound =
            UAabb3::new(bvh_nodes[0].aabb.min_uvec3(), bvh_nodes[0].aabb.max_uvec3());

        CompiledFencePlacement {
            voxel_edit: VoxelEdit::StampCuboids {
                bvh_nodes,
                cuboids,
                voxel_type: edit.voxel_type,
            },
            rebuild_bound,
        }
    }
}

struct TerrainSurfaceRemovalService;

impl TerrainSurfaceRemovalService {
    fn compile(edit: TerrainRemovalEdit) -> Option<CompiledTerrainSurfaceRemoval> {
        Self::compile_with_voxel_type(edit, crate::builder::VOXEL_TYPE_EMPTY)
    }

    fn compile_with_voxel_type(
        edit: TerrainRemovalEdit,
        voxel_type: u32,
    ) -> Option<CompiledTerrainSurfaceRemoval> {
        if edit.radius <= 0.0 {
            return None;
        }

        let center_voxel = edit.center * 256.0;
        let radius_voxel = edit.radius * 256.0;
        let sphere = Sphere::new(center_voxel, radius_voxel);
        let world_dim = super::VOXEL_DIM_PER_CHUNK * super::CHUNK_DIM;
        let max_inclusive = world_dim - UVec3::ONE;
        let sphere_aabb = sphere.aabb();
        let min_f = sphere_aabb.min();
        let max_f = sphere_aabb.max();
        if max_f.x < 0.0
            || max_f.y < 0.0
            || max_f.z < 0.0
            || min_f.x > max_inclusive.x as f32
            || min_f.y > max_inclusive.y as f32
            || min_f.z > max_inclusive.z as f32
        {
            return None;
        }

        let min = UVec3::new(
            min_f.x.max(0.0).floor() as u32,
            min_f.y.max(0.0).floor() as u32,
            min_f.z.max(0.0).floor() as u32,
        )
        .min(world_dim);
        let max_exclusive = UVec3::new(
            max_f.x.max(0.0).ceil() as u32,
            max_f.y.max(0.0).ceil() as u32,
            max_f.z.max(0.0).ceil() as u32,
        )
        .min(world_dim);
        if min.cmpge(max_exclusive).any() {
            return None;
        }

        // The voxel modify dispatch treats the BVH AABB max as exclusive. Keep it clipped to
        // `world_dim` rather than `world_dim - 1`; otherwise the positive atlas edge (voxel 255
        // in the default 256³ chunk) is never dispatched and cannot be dug away.
        let clipped_aabb = crate::geom::Aabb3::new(min.as_vec3(), max_exclusive.as_vec3());
        let bvh_nodes = build_bvh(&[clipped_aabb], &[0_u32]).ok()?;
        let rebuild_max = UVec3::new(
            max_exclusive.x.saturating_sub(1),
            max_exclusive.y.saturating_sub(1),
            max_exclusive.z.saturating_sub(1),
        )
        .min(max_inclusive);
        let rebuild_bound = UAabb3::new(
            bvh_nodes[0].aabb.min_uvec3().min(max_inclusive),
            rebuild_max,
        );

        Some(CompiledTerrainSurfaceRemoval {
            voxel_edit: VoxelEdit::StampSurfaceSpheres {
                bvh_nodes,
                spheres: vec![sphere],
                voxel_type,
            },
            rebuild_bound,
        })
    }

    fn compile_surface_brush(edit: TerrainBrushEdit) -> Option<CompiledTerrainBrushSurfaceEdit> {
        if edit.radius <= 0.0 {
            return None;
        }

        let start_voxel = edit.start * 256.0;
        let end_voxel = edit.end * 256.0;
        let radius_voxel = edit.radius * 256.0;
        let min_f = start_voxel.min(end_voxel) - Vec3::splat(radius_voxel);
        let max_f = start_voxel.max(end_voxel) + Vec3::splat(radius_voxel);
        let world_dim = super::VOXEL_DIM_PER_CHUNK * super::CHUNK_DIM;
        let max_inclusive = world_dim - UVec3::ONE;
        if max_f.x < 0.0
            || max_f.y < 0.0
            || max_f.z < 0.0
            || min_f.x > max_inclusive.x as f32
            || min_f.y > max_inclusive.y as f32
            || min_f.z > max_inclusive.z as f32
        {
            return None;
        }

        let min = UVec3::new(
            min_f.x.max(0.0).floor() as u32,
            min_f.y.max(0.0).floor() as u32,
            min_f.z.max(0.0).floor() as u32,
        )
        .min(world_dim);
        let max_exclusive = UVec3::new(
            max_f.x.max(0.0).ceil() as u32,
            max_f.y.max(0.0).ceil() as u32,
            max_f.z.max(0.0).ceil() as u32,
        )
        .min(world_dim);
        if min.cmpge(max_exclusive).any() {
            return None;
        }

        let clipped_aabb = crate::geom::Aabb3::new(min.as_vec3(), max_exclusive.as_vec3());
        let bvh_nodes = build_bvh(&[clipped_aabb], &[0_u32]).ok()?;
        let rebuild_max = UVec3::new(
            max_exclusive.x.saturating_sub(1),
            max_exclusive.y.saturating_sub(1),
            max_exclusive.z.saturating_sub(1),
        )
        .min(max_inclusive);
        let rebuild_bound = UAabb3::new(
            bvh_nodes[0].aabb.min_uvec3().min(max_inclusive),
            rebuild_max,
        );

        Some(CompiledTerrainBrushSurfaceEdit { rebuild_bound })
    }
}

struct CompiledTreePlacement {
    trunk_voxel_edit: VoxelEdit,
    rebuild_bound: UAabb3,
    tree_pos: Vec3,
    this_bound: UAabb3,
    quantized_leaf_positions: Vec<UVec3>,
    quantized_apple_positions: Vec<UVec3>,
    world_leaf_positions: Vec<Vec3>,
}

struct TreePlacementService;

const APPLE_SPAWN_PROBABILITY: f32 = 0.30;
const APPLE_MIN_HANG_VOXELS: f32 = 4.0;
const APPLE_HANG_VARIANCE_VOXELS: f32 = 2.0;
const APPLE_MIN_SIDE_OFFSET_VOXELS: f32 = 0.5;
const APPLE_SIDE_OFFSET_VARIANCE_VOXELS: f32 = 1.5;

fn analyze_round_cones(round_cones: &[RoundCone]) -> (f32, f32, f32, f32, usize, usize) {
    let mut radius_min = f32::INFINITY;
    let mut radius_max = 0.0_f32;
    let mut length_min = f32::INFINITY;
    let mut length_max = 0.0_f32;
    let mut short_count = 0;
    let mut steep_count = 0;

    for cone in round_cones {
        let radius_a = cone.radius_a();
        let radius_b = cone.radius_b();
        let length = (cone.center_b() - cone.center_a()).length();
        radius_min = radius_min.min(radius_a).min(radius_b);
        radius_max = radius_max.max(radius_a).max(radius_b);
        length_min = length_min.min(length);
        length_max = length_max.max(length);
        if length <= 1e-4 {
            short_count += 1;
        }
        if (radius_a - radius_b).abs() > length {
            steep_count += 1;
        }
    }

    if round_cones.is_empty() {
        radius_min = 0.0;
        length_min = 0.0;
    }

    (
        radius_min,
        radius_max,
        length_min,
        length_max,
        short_count,
        steep_count,
    )
}

fn mix_u32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

fn unit_hash(seed: u64, leaf_index: usize, salt: u32, leaf_pos: UVec3) -> f32 {
    let mut value =
        seed as u32 ^ (seed >> 32) as u32 ^ (leaf_index as u32).wrapping_mul(0x9e37_79b9);
    value ^= leaf_pos.x.wrapping_mul(0x85eb_ca6b);
    value ^= leaf_pos.y.wrapping_mul(0xc2b2_ae35);
    value ^= leaf_pos.z.wrapping_mul(0x27d4_eb2f);
    value ^= salt;
    (mix_u32(value) as f32) / (u32::MAX as f32 + 1.0)
}

fn quantized_apple_positions(
    seed: u64,
    tree_pos_voxels: Vec3,
    leaf_positions: &[UVec3],
) -> Vec<UVec3> {
    let mut apple_positions = HashSet::new();
    for (leaf_index, leaf_pos) in leaf_positions.iter().copied().enumerate() {
        if unit_hash(seed, leaf_index, 0xA511_E9B3, leaf_pos) >= APPLE_SPAWN_PROBABILITY {
            continue;
        }

        let random_angle =
            unit_hash(seed, leaf_index, 0x63D8_3595, leaf_pos) * std::f32::consts::TAU;
        let random_dir = Vec2::new(random_angle.cos(), random_angle.sin());
        let leaf_pos_voxels = leaf_pos.as_vec3();
        let relative_leaf_pos = leaf_pos_voxels - tree_pos_voxels;
        let outward_dir = Vec2::new(relative_leaf_pos.x, relative_leaf_pos.z).normalize_or_zero();
        let hang_dir = (outward_dir * 0.65 + random_dir * 0.35).normalize_or_zero();
        let hang_dir = if hang_dir.length_squared() > 0.0 {
            hang_dir
        } else {
            random_dir
        };
        let side_offset = APPLE_MIN_SIDE_OFFSET_VOXELS
            + unit_hash(seed, leaf_index, 0xB529_7A4D, leaf_pos)
                * APPLE_SIDE_OFFSET_VARIANCE_VOXELS;
        let hang_offset = APPLE_MIN_HANG_VOXELS
            + unit_hash(seed, leaf_index, 0x68E3_1DA4, leaf_pos) * APPLE_HANG_VARIANCE_VOXELS;
        let apple_pos = leaf_pos_voxels
            + Vec3::new(
                hang_dir.x * side_offset,
                -hang_offset,
                hang_dir.y * side_offset,
            );
        if apple_pos.min_element() < 0.0 {
            continue;
        }
        apple_positions.insert(apple_pos.round().as_uvec3());
    }

    let mut apple_positions = apple_positions.into_iter().collect::<Vec<_>>();
    apple_positions.sort_by_key(|pos| (pos.x, pos.y, pos.z));
    apple_positions
}

impl TreePlacementService {
    fn compile(
        tree_desc: TreeDesc,
        tree_pos: Vec3,
        extra_rebuild_bound: UAabb3,
    ) -> CompiledTreePlacement {
        let tree = Tree::new(tree_desc.clone());
        let mut round_cones = Vec::with_capacity(tree.trunks().len());
        for tree_trunk in tree.trunks() {
            let mut round_cone = tree_trunk.clone();
            round_cone.transform(tree_pos * 256.0);
            round_cones.push(round_cone);
        }

        let (radius_min, radius_max, length_min, length_max, short_count, steep_count) =
            analyze_round_cones(&round_cones);

        let leaves_data_sequential = (0..round_cones.len()).map(|i| i as u32).collect::<Vec<_>>();
        let aabbs = round_cones.iter().map(RoundCone::aabb).collect::<Vec<_>>();

        let bvh_nodes = build_bvh(&aabbs, &leaves_data_sequential).unwrap();
        log::info!(
            "[TREE_DEBUG] compile trunks={} leaves={} size={:.3} trunk_thickness={:.3} min_trunk_thickness={:.3} thickness_reduction={:.3} iterations={} radius_min={:.3} radius_max={:.3} length_min={:.3} length_max={:.3} short_segments={} radius_delta_gt_length={} bound_min={:?} bound_max={:?}",
            round_cones.len(),
            tree.relative_leaf_positions().len(),
            tree_desc.size,
            tree_desc.trunk_thickness,
            tree_desc.trunk_thickness_min,
            tree_desc.thickness_reduction,
            tree_desc.branching.iterations,
            radius_min,
            radius_max,
            length_min,
            length_max,
            short_count,
            steep_count,
            bvh_nodes[0].aabb.min(),
            bvh_nodes[0].aabb.max(),
        );

        let this_bound = UAabb3::new(bvh_nodes[0].aabb.min_uvec3(), bvh_nodes[0].aabb.max_uvec3());

        let relative_leaf_positions = tree.relative_leaf_positions();
        let world_leaf_positions = relative_leaf_positions
            .iter()
            .map(|leaf_pos| *leaf_pos / 256.0 + tree_pos)
            .collect::<Vec<_>>();
        let offseted_leaf_positions = relative_leaf_positions
            .iter()
            .map(|leaf_pos| *leaf_pos + tree_pos * 256.0)
            .collect::<Vec<_>>();
        let quantized_leaf_positions = {
            let set = offseted_leaf_positions
                .iter()
                .map(|pos| pos.as_uvec3())
                .collect::<HashSet<_>>();
            let mut positions = set.into_iter().collect::<Vec<_>>();
            positions.sort_by_key(|pos| (pos.x, pos.y, pos.z));
            positions
        };
        let quantized_apple_positions = quantized_apple_positions(
            tree_desc.branching.seed,
            tree_pos * 256.0,
            &quantized_leaf_positions,
        );

        CompiledTreePlacement {
            trunk_voxel_edit: VoxelEdit::StampRoundCones {
                bvh_nodes,
                round_cones,
                voxel_type: VOXEL_TYPE_CHERRY_WOOD,
            },
            rebuild_bound: this_bound.union_with(&extra_rebuild_bound),
            tree_pos,
            this_bound,
            quantized_leaf_positions,
            quantized_apple_positions,
            world_leaf_positions,
        }
    }
}

fn authored_flora_hash(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

fn authored_flora_candidate_seed(
    species_index: u32,
    paint_dab_serial: u32,
    plant_index: u32,
    attempt: u32,
) -> u32 {
    let mut seed = paint_dab_serial ^ species_index.wrapping_mul(0x9e37_79b9);
    seed ^= plant_index.wrapping_mul(0x27d4_eb2d);
    seed ^= attempt.wrapping_mul(0x1656_67b1);
    authored_flora_hash(seed)
}

fn authored_flora_base_center_from_stem(stem_world_vox: UVec3) -> Vec3 {
    Vec3::new(
        stem_world_vox.x as f32 + 0.5,
        stem_world_vox.y as f32 - 0.5,
        stem_world_vox.z as f32 + 0.5,
    )
}

fn authored_flora_xz_distance_sq(a: Vec3, b: Vec3) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx * dx + dz * dz
}

fn authored_flora_nearest_xz_distance_sq(
    candidate_base_center_vox: Vec3,
    existing_base_centers_vox: &[Vec3],
    placed_base_centers_vox: &[Vec3],
) -> f32 {
    existing_base_centers_vox
        .iter()
        .chain(placed_base_centers_vox.iter())
        .map(|center| authored_flora_xz_distance_sq(candidate_base_center_vox, *center))
        .fold(f32::INFINITY, f32::min)
}

fn authored_flora_release_is_outlier(
    paint_dab_serial: u32,
    species_index: u32,
    chance: f32,
) -> bool {
    let chance = chance.clamp(0.0, 1.0);
    if chance <= 0.0 {
        return false;
    }
    let seed = authored_flora_hash(
        paint_dab_serial ^ species_index.wrapping_mul(0x85eb_ca6b) ^ 0xc2b2_ae35,
    );
    authored_flora_unit_from_seed(seed) < chance
}

fn authored_flora_existing_cluster_anchor(
    edit: TerrainBrushEdit,
    existing_base_centers_vox: &[Vec3],
    cluster_radius_vox: f32,
    seed: u32,
) -> Option<Vec3> {
    let start_vox = edit.start * 256.0;
    let end_vox = edit.end * 256.0;
    let brush_radius_vox = edit.radius * 256.0;
    let influence_radius_sq = (brush_radius_vox + cluster_radius_vox).powi(2);
    let mut candidates = existing_base_centers_vox
        .iter()
        .copied()
        .filter(|center| distance_sq_to_segment(*center, start_vox, end_vox) <= influence_radius_sq)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| {
        let key_a = authored_flora_hash(
            seed ^ (a.x.floor() as u32).wrapping_mul(0x9e37_79b9)
                ^ (a.z.floor() as u32).wrapping_mul(0x7f4a_7c15),
        );
        let key_b = authored_flora_hash(
            seed ^ (b.x.floor() as u32).wrapping_mul(0x9e37_79b9)
                ^ (b.z.floor() as u32).wrapping_mul(0x7f4a_7c15),
        );
        key_a.cmp(&key_b)
    });
    candidates.into_iter().next()
}

fn authored_flora_natural_candidate_score(
    candidate_base_center_vox: Vec3,
    cluster_anchor_vox: Vec3,
    existing_base_centers_vox: &[Vec3],
    placed_base_centers_vox: &[Vec3],
    params: SpecialFloraDistributionParams,
    seed: u32,
) -> Option<f32> {
    let nearest_distance_sq = authored_flora_nearest_xz_distance_sq(
        candidate_base_center_vox,
        existing_base_centers_vox,
        placed_base_centers_vox,
    );
    let min_spacing_sq = params.min_spacing_voxels * params.min_spacing_voxels;
    if nearest_distance_sq.is_finite() && nearest_distance_sq < min_spacing_sq {
        return None;
    }

    let cluster_radius = params.cluster_radius_voxels.max(1.0);
    let cluster_distance =
        authored_flora_xz_distance_sq(candidate_base_center_vox, cluster_anchor_vox).sqrt();
    let cluster_score = (1.0 - cluster_distance / cluster_radius).clamp(0.0, 1.0);
    let random_score = authored_flora_unit_from_seed(seed ^ 0x27d4_eb2d);
    let spacing_score = if nearest_distance_sq.is_finite() {
        (nearest_distance_sq.sqrt() / cluster_radius).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let cluster_bias = params.cluster_bias.clamp(0.0, 1.0);
    Some(
        cluster_score * cluster_bias
            + random_score * (1.0 - cluster_bias)
            + spacing_score * 0.18
            + authored_flora_unit_from_seed(seed ^ 0x9e37_79b9) * 0.03,
    )
}

fn authored_flora_unit_from_seed(seed: u32) -> f32 {
    (seed as f32) / (u32::MAX as f32 + 1.0)
}

fn distance_sq_to_segment(point: Vec3, start: Vec3, end: Vec3) -> f32 {
    let segment = end - start;
    let segment_len_sq = segment.length_squared().max(1.0e-4);
    let t = ((point - start).dot(segment) / segment_len_sq).clamp(0.0, 1.0);
    let closest = start + segment * t;
    point.distance_squared(closest)
}

#[derive(Clone, Debug)]
pub(super) struct TreeRecord {
    position: Vec3,
    bound: UAabb3,
}

impl App {
    #[allow(dead_code)]
    pub(super) fn generate_procedural_trees(&mut self) -> Result<()> {
        self.clear_procedural_trees()?;
        self.remove_tree(self.single_tree_id)?;

        let prev_bound = self.prev_bound;
        self.execute_edit_plan(WorldEditPlan::with_voxel(VoxelEdit::ClearVoxelRegion(
            ClearVoxelRegionEdit {
                offset: prev_bound.min(),
                dim: prev_bound.max() - prev_bound.min(),
            },
        )))?;

        let world_size = super::CHUNK_DIM * super::VOXEL_DIM_PER_CHUNK;
        let map_padding = 50.0;
        let map_dimensions = Vec2::new(
            world_size.x as f32 - map_padding * 2.0,
            world_size.z as f32 - map_padding * 2.0,
        );
        let grid_size = 120.0;
        let mut placer_desc = PlacerDesc::new(42);
        placer_desc.threshold = 0.55;

        let tree_positions_2d = generate_positions(
            map_dimensions,
            Vec2::new(map_padding, map_padding),
            grid_size,
            &placer_desc,
        );

        log::info!("Generated {} procedural trees", tree_positions_2d.len());

        let tree_positions_3d = self.query_terrain_heights_for_positions(&tree_positions_2d)?;

        let mut rng = rand::rng();

        for tree_pos in tree_positions_3d.iter() {
            let mut tree_desc = self.debug_tree_desc.clone();
            tree_desc.branching.seed = rng.random_range(1..10000);

            self.apply_tree_variations(&mut tree_desc, &mut rng);
            self.apply_tree_placement(TreePlacementEdit {
                tree_desc,
                placement: TreePlacement::World(*tree_pos),
                options: TreeAddOptions::default().with_new_id(),
            })?;
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub(super) fn clear_procedural_trees(&mut self) -> Result<()> {
        let tree_ids_to_remove: Vec<u32> = self
            .tree_records
            .keys()
            .copied()
            .filter(|&id| id >= 1)
            .collect();

        for tree_id in tree_ids_to_remove {
            self.remove_tree(tree_id)?;
        }

        log::info!("Cleared all procedural trees and their sound sources");
        Ok(())
    }

    #[allow(dead_code)]
    pub(super) fn remove_tree(&mut self, tree_id: u32) -> Result<()> {
        self.tracer
            .remove_tree_leaves(&mut self.surface_builder.resources, tree_id)?;
        self.request_vsm_history_reset();
        self.tree_audio_manager.remove_tree(tree_id);
        self.remove_leaf_emitter(tree_id);
        match self.tree_records.remove(&tree_id) {
            Some(record) => {
                log::debug!(
                    "Removed tree {} at position {:?}, bound {:?}",
                    tree_id,
                    record.position,
                    record.bound
                );
            }
            None => {
                log::debug!("Tree {} was not registered during removal", tree_id);
            }
        }
        Ok(())
    }

    fn tree_rebuild_chunk_ids(&self, old_bound: UAabb3, new_bound: UAabb3) -> Vec<UVec3> {
        let mut chunk_ids =
            world_ops::affected_chunk_indices_for_bound(old_bound, super::VOXEL_DIM_PER_CHUNK);
        chunk_ids.extend(world_ops::affected_chunk_indices_for_bound(
            new_bound,
            super::VOXEL_DIM_PER_CHUNK,
        ));

        let mut seen = HashSet::new();
        chunk_ids.retain(|chunk_id| seen.insert(*chunk_id));
        chunk_ids
    }

    #[allow(dead_code)]
    pub(super) fn replace_single_tree_deferred(
        &mut self,
        tree_desc: TreeDesc,
        tree_pos: Vec3,
    ) -> Result<()> {
        self.replace_single_tree_with_rebuild_mode(tree_desc, tree_pos, true)
    }

    #[allow(dead_code)]
    #[allow(dead_code)]
    pub(super) fn replace_single_tree(
        &mut self,
        tree_desc: TreeDesc,
        tree_pos: Vec3,
    ) -> Result<()> {
        self.replace_single_tree_with_rebuild_mode(tree_desc, tree_pos, false)
    }

    fn replace_single_tree_with_rebuild_mode(
        &mut self,
        tree_desc: TreeDesc,
        tree_pos: Vec3,
        defer_rebuild: bool,
    ) -> Result<()> {
        let total_start = Instant::now();
        let mut old_remove_ms = 0.0;
        let mut old_clear_ms = 0.0;

        let old_bound = if let Some(record) = self.tree_records.remove(&self.single_tree_id) {
            let old_remove_start = Instant::now();
            self.tracer
                .remove_tree_leaves(&mut self.surface_builder.resources, self.single_tree_id)?;
            self.tree_audio_manager.remove_tree(self.single_tree_id);
            self.remove_leaf_emitter(self.single_tree_id);
            old_remove_ms = old_remove_start.elapsed().as_secs_f32() * 1000.0;
            record.bound
        } else {
            UAabb3::default()
        };

        let compile_start = Instant::now();
        let compiled = TreePlacementService::compile(tree_desc, tree_pos, UAabb3::default());
        let compile_elapsed = compile_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("tree_gui_compile", compile_elapsed);

        let trunk_count = match &compiled.trunk_voxel_edit {
            VoxelEdit::StampRoundCones { round_cones, .. } => round_cones.len(),
            _ => 0,
        };

        if old_bound.has_size() {
            let old_clear_start = Instant::now();
            self.execute_edit_plan(WorldEditPlan::with_voxel(VoxelEdit::ClearVoxelRegion(
                ClearVoxelRegionEdit {
                    offset: old_bound.min(),
                    dim: old_bound.max() - old_bound.min(),
                },
            )))?;
            old_clear_ms = old_clear_start.elapsed().as_secs_f32() * 1000.0;
        }

        let trunk_start = Instant::now();
        self.execute_edit_plan(WorldEditPlan::with_voxel(compiled.trunk_voxel_edit))?;
        let trunk_elapsed = trunk_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("tree_gui_trunk_voxel", trunk_elapsed);

        let rebuild_chunk_ids = self.tree_rebuild_chunk_ids(old_bound, compiled.this_bound);
        let rebuild_start = Instant::now();
        if defer_rebuild {
            self.enqueue_deferred_chunk_rebuilds_without_flora(&rebuild_chunk_ids);
        } else {
            world_ops::mesh_generate_chunks_without_flora(
                &mut self.surface_builder,
                &mut self.contree_builder,
                &mut self.scene_accel_builder,
                super::VOXEL_DIM_PER_CHUNK,
                rebuild_chunk_ids.clone(),
            )?;
        }
        let rebuild_elapsed = rebuild_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("tree_gui_rebuild", rebuild_elapsed);

        self.finish_tree_placement(
            self.single_tree_id,
            compiled.tree_pos,
            compiled.this_bound,
            compiled.rebuild_bound,
            rebuild_chunk_ids.len(),
            &compiled.quantized_leaf_positions,
            &compiled.quantized_apple_positions,
            &compiled.world_leaf_positions,
            total_start,
            compile_elapsed,
            trunk_elapsed,
            rebuild_elapsed,
            trunk_count,
            true,
        )?;
        self.prev_bound = self.prev_bound.union_with(&old_bound);

        log::info!(
            "[PERF][TREE_GUI] replace_total {:.2}ms old_remove {:.2}ms old_clear {:.2}ms",
            total_start.elapsed().as_secs_f32() * 1000.0,
            old_remove_ms,
            old_clear_ms,
        );

        Ok(())
    }

    #[allow(dead_code)]
    pub(super) fn edit_tree_with_variance(
        tree_desc: &mut TreeDesc,
        tree_variation_config: &mut TreeVariationConfig,
        ui: &mut egui::Ui,
    ) -> (bool, bool) {
        let mut regenerate_pressed = false;

        if ui.button("🌲 Regenerate Procedural Trees").clicked() {
            regenerate_pressed = true;
        }

        ui.separator();

        let tree_changed = tree_desc.edit_by_gui(ui);

        ui.separator();

        tree_variation_config.edit_by_gui(ui);

        (tree_changed, regenerate_pressed)
    }

    #[allow(dead_code)]
    pub(super) fn apply_tree_variations(&self, tree_desc: &mut TreeDesc, rng: &mut impl Rng) {
        let config = &self.tree_variation_config;

        if config.size_variance > 0.0 {
            tree_desc.size *= 1.0 + rng.random_range(-config.size_variance..=config.size_variance);
        }

        if config.trunk_thickness_variance > 0.0 {
            tree_desc.trunk_thickness *= 1.0
                + rng.random_range(
                    -config.trunk_thickness_variance..=config.trunk_thickness_variance,
                );
        }

        if config.trunk_thickness_min_variance > 0.0 {
            tree_desc.trunk_thickness_min *= 1.0
                + rng.random_range(
                    -config.trunk_thickness_min_variance..=config.trunk_thickness_min_variance,
                );
        }

        if config.spread_variance > 0.0 {
            tree_desc.branching.spread *=
                1.0 + rng.random_range(-config.spread_variance..=config.spread_variance);
        }

        if config.randomness_variance > 0.0 {
            tree_desc.branching.randomness = (tree_desc.branching.randomness
                + rng.random_range(-config.randomness_variance..=config.randomness_variance))
            .clamp(0.0, 1.0);
        }

        if config.vertical_tendency_variance > 0.0 {
            tree_desc.branching.vertical_tendency = (tree_desc.branching.vertical_tendency
                + rng.random_range(
                    -config.vertical_tendency_variance..=config.vertical_tendency_variance,
                ))
            .clamp(-1.0, 1.0);
        }

        if config.branch_probability_variance > 0.0 {
            tree_desc.branching.branch_probability = (tree_desc.branching.branch_probability
                + rng.random_range(
                    -config.branch_probability_variance..=config.branch_probability_variance,
                ))
            .clamp(0.0, 1.0);
        }

        if config.initial_length_variance > 0.0 {
            tree_desc.branching.initial_length *= 1.0
                + rng
                    .random_range(-config.initial_length_variance..=config.initial_length_variance);
        }

        if config.length_dropoff_variance > 0.0 {
            tree_desc.branching.length_dropoff = (tree_desc.branching.length_dropoff
                + rng.random_range(
                    -config.length_dropoff_variance..=config.length_dropoff_variance,
                ))
            .clamp(0.1, 1.0);
        }

        if config.thickness_reduction_variance > 0.0 {
            tree_desc.thickness_reduction = (tree_desc.thickness_reduction
                + rng.random_range(
                    -config.thickness_reduction_variance..=config.thickness_reduction_variance,
                ))
            .clamp(0.0, 1.0);
        }

        if config.iterations_variance > 0.0 {
            let variation =
                rng.random_range(-config.iterations_variance..=config.iterations_variance);
            tree_desc.branching.iterations =
                ((tree_desc.branching.iterations as f32 + variation).round() as u32).clamp(1, 12);
        }

        if config.leaves_size_level_variance > 0.0 {
            let variation = rng.random_range(
                -config.leaves_size_level_variance..=config.leaves_size_level_variance,
            );
            tree_desc.leaves_size_level =
                ((tree_desc.leaves_size_level as f32 + variation).round() as u32).clamp(0, 8);
        }
    }

    #[allow(dead_code)]
    pub(super) fn query_terrain_heights_for_positions(
        &mut self,
        positions_2d: &[Vec2],
    ) -> Result<Vec<Vec3>> {
        if positions_2d.is_empty() {
            return Ok(vec![]);
        }

        let query_positions: Vec<Vec2> = positions_2d
            .iter()
            .map(|pos| Vec2::new(pos.x, pos.y))
            .collect();

        let terrain_heights = query_positions
            .iter()
            .map(|&pos| self.query_terrain_height_cpu(pos))
            .collect::<Vec<_>>();

        let positions_3d = positions_2d
            .iter()
            .zip(terrain_heights.iter())
            .map(|(pos_2d, &height)| Vec3::new(pos_2d.x, height, pos_2d.y))
            .collect();

        Ok(positions_3d)
    }

    #[allow(dead_code)]
    pub(super) fn clean_up_prev_tree(&mut self) -> Result<()> {
        self.tree_audio_manager.remove_all();

        let prev_bound = self.prev_bound;
        self.execute_edit_plan(WorldEditPlan {
            voxel_edits: vec![VoxelEdit::ClearVoxelRegion(ClearVoxelRegionEdit {
                offset: prev_bound.min(),
                dim: prev_bound.max() - prev_bound.min(),
            })],
            build_edits: vec![BuildEdit::RebuildMesh(prev_bound)],
        })?;

        Ok(())
    }

    #[allow(dead_code)]
    pub(super) fn add_tree(
        &mut self,
        tree_desc: TreeDesc,
        placement: TreePlacement,
        options: TreeAddOptions,
    ) -> Result<()> {
        self.apply_tree_placement(TreePlacementEdit {
            tree_desc,
            placement,
            options,
        })
    }

    #[allow(dead_code)]
    pub(super) fn plant_map_region_fence_posts(&mut self) -> Result<()> {
        const BASE_FENCE_HEIGHT: f32 = 60.0;
        const FENCE_HEIGHT_SCALE: f32 = 0.45;
        const FENCE_HEIGHT: f32 = BASE_FENCE_HEIGHT * FENCE_HEIGHT_SCALE;
        const BASE_FENCE_SIZE: f32 = 10.0;
        const FENCE_SIZE_SCALE: f32 = 0.3;
        const POST_HALF_WIDTH: f32 = BASE_FENCE_SIZE * FENCE_SIZE_SCALE * 0.5;
        const POST_HALF_DEPTH: f32 = POST_HALF_WIDTH;
        const BORDER_PADDING: f32 = 0.2;
        const EDGE_INTERIOR_COLUMNS: u32 = 30;

        let map_size = super::CHUNK_DIM.as_vec3();
        let min_x = BORDER_PADDING;
        let max_x = map_size.x - BORDER_PADDING;
        let min_z = BORDER_PADDING;
        let max_z = map_size.z - BORDER_PADDING;

        let edge_segments = EDGE_INTERIOR_COLUMNS + 1;
        let mut post_positions = Vec::with_capacity((4 * EDGE_INTERIOR_COLUMNS + 4) as usize);

        for i in 0..=edge_segments {
            let t = i as f32 / edge_segments as f32;
            let x = min_x + (max_x - min_x) * t;
            post_positions.push(Vec2::new(x, min_z));
        }

        for i in 1..=edge_segments {
            let t = i as f32 / edge_segments as f32;
            let z = min_z + (max_z - min_z) * t;
            post_positions.push(Vec2::new(max_x, z));
        }

        for i in 1..=edge_segments {
            let t = i as f32 / edge_segments as f32;
            let x = max_x - (max_x - min_x) * t;
            post_positions.push(Vec2::new(x, max_z));
        }

        for i in 1..edge_segments {
            let t = i as f32 / edge_segments as f32;
            let z = max_z - (max_z - min_z) * t;
            post_positions.push(Vec2::new(min_x, z));
        }

        for horizontal in post_positions {
            self.apply_fence_post_placement(FencePostPlacementEdit {
                horizontal,
                height: FENCE_HEIGHT,
                half_width: POST_HALF_WIDTH,
                half_depth: POST_HALF_DEPTH,
            })?;
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub(super) fn apply_fence_post_placement(
        &mut self,
        edit: FencePostPlacementEdit,
    ) -> Result<()> {
        self.apply_fence_like_placement(edit, VOXEL_TYPE_OAK_WOOD)
    }

    #[allow(dead_code)]
    fn apply_fence_like_placement(
        &mut self,
        edit: FencePostPlacementEdit,
        voxel_type: u32,
    ) -> Result<()> {
        let terrain_height = self.query_terrain_height_cpu(edit.horizontal);
        let compiled = FencePostPlacementService::compile(edit, terrain_height, voxel_type);

        self.execute_edit_plan(WorldEditPlan::with_voxel_and_build(
            compiled.voxel_edit,
            BuildEdit::RebuildMesh(compiled.rebuild_bound),
        ))
    }

    #[allow(dead_code)]
    pub(super) fn apply_cube_placement(&mut self, edit: CubePlacementEdit) -> Result<()> {
        let compiled = CubePlacementService::compile(edit);
        self.execute_edit_plan(WorldEditPlan::with_voxel_and_build(
            compiled.voxel_edit,
            BuildEdit::RebuildMesh(compiled.rebuild_bound),
        ))
    }

    pub(super) fn apply_surface_terrain_removal(
        &mut self,
        edit: TerrainRemovalEdit,
        target_voxel_type: Option<u32>,
        max_write_count: Option<u32>,
        max_removed_counts: Option<[u32; crate::builder::EDIT_STATS_VOXEL_TYPE_COUNT]>,
    ) -> Result<ChunkModifyReadback> {
        let total_start = Instant::now();
        if let Some(compiled) = TerrainSurfaceRemovalService::compile(edit) {
            let rebuild_bound = compiled.rebuild_bound;
            let stats = match compiled.voxel_edit {
                VoxelEdit::StampSurfaceSpheres {
                    bvh_nodes,
                    spheres,
                    voxel_type,
                } => self
                    .plain_builder
                    .chunk_modify_surface_spheres_with_voxel_type(
                        &bvh_nodes,
                        &spheres,
                        voxel_type,
                        target_voxel_type,
                        max_write_count,
                        max_removed_counts,
                    )?,
                _ => unreachable!("terrain surface removal compiled into unexpected edit type"),
            };
            if stats.stats.removed_counts.iter().any(|&count| count > 0)
                || stats.stats.added_counts.iter().any(|&count| count > 0)
            {
                self.request_vsm_history_reset();
            }
            let rebuild_chunk_ids = world_ops::affected_chunk_indices_for_bound(
                rebuild_bound,
                super::VOXEL_DIM_PER_CHUNK,
            );
            self.enqueue_deferred_flora_preserving_chunk_rebuilds(
                &rebuild_chunk_ids,
                world_ops::FloraBrushEdit {
                    start: edit.center,
                    end: edit.center,
                    radius: edit.radius,
                    tick: self.flora_tick,
                },
            );
            let total_elapsed = total_start.elapsed();
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("terrain_edit_removal_total", total_elapsed);
            crate::util::BENCH.lock().unwrap().summary();
            return Ok(stats);
        }
        Ok(ChunkModifyReadback::default())
    }

    pub(super) fn apply_surface_terrain_placement(
        &mut self,
        edit: TerrainRemovalEdit,
        voxel_type: u32,
        max_write_count: u32,
    ) -> Result<ChunkModifyReadback> {
        let total_start = Instant::now();
        if let Some(compiled) =
            TerrainSurfaceRemovalService::compile_with_voxel_type(edit, voxel_type)
        {
            let rebuild_bound = compiled.rebuild_bound;
            let modify_start = Instant::now();
            let stats = match compiled.voxel_edit {
                VoxelEdit::StampSurfaceSpheres {
                    bvh_nodes,
                    spheres,
                    voxel_type,
                } => self
                    .plain_builder
                    .chunk_modify_surface_spheres_with_voxel_type(
                        &bvh_nodes,
                        &spheres,
                        voxel_type,
                        None,
                        Some(max_write_count),
                        None,
                    )?,
                _ => unreachable!("terrain surface placement compiled into unexpected edit type"),
            };
            if stats.stats.removed_counts.iter().any(|&count| count > 0)
                || stats.stats.added_counts.iter().any(|&count| count > 0)
            {
                self.request_vsm_history_reset();
            }
            let _modify_elapsed = modify_start.elapsed();
            let mesh_start = Instant::now();
            let rebuild_chunk_ids = world_ops::affected_chunk_indices_for_bound(
                rebuild_bound,
                super::VOXEL_DIM_PER_CHUNK,
            );
            self.enqueue_deferred_flora_preserving_chunk_rebuilds(
                &rebuild_chunk_ids,
                world_ops::FloraBrushEdit {
                    start: edit.center,
                    end: edit.center,
                    radius: edit.radius,
                    tick: self.flora_tick,
                },
            );
            let _mesh_elapsed = mesh_start.elapsed();
            let total_elapsed = total_start.elapsed();
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("terrain_edit_placement_total", total_elapsed);
            crate::util::BENCH.lock().unwrap().summary();

            return Ok(stats);
        }
        Ok(ChunkModifyReadback::default())
    }

    pub(super) fn apply_surface_terrain_smooth(
        &mut self,
        center: Vec3,
        radius: f32,
        strength: f32,
        max_delta: f32,
        deadband: f32,
    ) -> Result<()> {
        let flora_brush_edit = TerrainBrushEdit {
            start: center,
            end: center,
            radius,
        };
        let flora_rebuild_bound =
            TerrainSurfaceRemovalService::compile_surface_brush(flora_brush_edit)
                .map(|compiled| compiled.rebuild_bound);
        let flora_edit = world_ops::FloraBrushEdit {
            start: center,
            end: center,
            radius,
            tick: self.flora_tick,
        };

        let terrain_rebuild_bound = self
            .plain_builder
            .smooth_terrain_dirt(center, radius, strength, max_delta, deadband)?;

        if let Some(flora_rebuild_bound) = flora_rebuild_bound {
            world_ops::mesh_remove_flora_for_brush_edit(
                &mut self.surface_builder,
                super::VOXEL_DIM_PER_CHUNK,
                flora_rebuild_bound,
                flora_edit,
            )?;
        }

        let Some(rebuild_bound) = terrain_rebuild_bound else {
            return Ok(());
        };

        self.request_vsm_history_reset();
        let rebuild_chunk_ids =
            world_ops::affected_chunk_indices_for_bound(rebuild_bound, super::VOXEL_DIM_PER_CHUNK);
        self.enqueue_deferred_chunk_rebuilds_without_flora(&rebuild_chunk_ids);
        Ok(())
    }

    pub(super) fn apply_surface_flora_regeneration(
        &mut self,
        edit: TerrainBrushEdit,
        paint_dab_serial: u32,
        is_release_step: bool,
    ) -> Result<()> {
        if let Some(compiled) = TerrainSurfaceRemovalService::compile_surface_brush(edit) {
            let paint_selection = self.current_flora_paint_selection();
            let paint_brush = species::flora_paint_brush_settings(paint_selection);
            if let species::FloraPaintSelection::Species(species_index) = paint_selection {
                if species::is_authored_plant_species_index(species_index) {
                    if is_release_step {
                        self.paint_authored_surface_flora(
                            edit,
                            compiled.rebuild_bound,
                            species_index,
                            paint_dab_serial,
                            paint_brush,
                        )?;
                    }
                    return Ok(());
                }
            }

            world_ops::mesh_regenerate_flora_for_brush_edit(
                &mut self.surface_builder,
                super::VOXEL_DIM_PER_CHUNK,
                compiled.rebuild_bound,
                world_ops::FloraBrushEdit {
                    start: edit.start,
                    end: edit.end,
                    radius: edit.radius,
                    tick: self.flora_tick.wrapping_sub(super::FLORA_FULL_GROWTH_TICKS),
                },
                paint_selection,
                paint_dab_serial,
                paint_brush,
            )?;
        } else {
            log::info!(
                "Flora regen compile skipped: start={:?}, end={:?}, radius={}",
                edit.start,
                edit.end,
                edit.radius
            );
        }
        Ok(())
    }

    fn paint_authored_surface_flora(
        &mut self,
        edit: TerrainBrushEdit,
        rebuild_bound: UAabb3,
        species_index: u32,
        paint_dab_serial: u32,
        paint_brush: species::FloraPaintBrushSettings,
    ) -> Result<()> {
        if paint_brush.soft_spacing_voxels == 0 {
            return Ok(());
        }
        let distribution = SpecialFloraDistributionParams {
            plants_per_release: self
                .gui_adjustables
                .special_flora_plants_per_release
                .value
                .clamp(1, 8),
            cluster_radius_voxels: self
                .gui_adjustables
                .special_flora_cluster_radius_voxels
                .value
                .max(1.0),
            min_spacing_voxels: self
                .gui_adjustables
                .special_flora_min_spacing_voxels
                .value
                .max(0.0),
            cluster_bias: self.gui_adjustables.special_flora_cluster_bias.value,
            outlier_chance: self.gui_adjustables.special_flora_outlier_chance.value,
        };
        let radius_vox = edit.radius * 256.0;
        let radius_sq = radius_vox * radius_vox;
        let start_vox = edit.start * 256.0;
        let end_vox = edit.end * 256.0;
        let world_dim = super::VOXEL_DIM_PER_CHUNK * super::CHUNK_DIM;
        let min = rebuild_bound
            .min()
            .min(world_dim.saturating_sub(UVec3::ONE));
        let max = rebuild_bound.max().min(world_dim);
        if min.x >= max.x || min.z >= max.z {
            return Ok(());
        }

        let existing_base_centers_vox = self
            .surface_builder
            .authored_flora_stem_positions_for_species(species_index)
            .into_iter()
            .map(authored_flora_base_center_from_stem)
            .collect::<Vec<_>>();
        let mut placed_base_centers_vox = Vec::new();
        let mut dirty_chunks = Vec::new();
        let x_span = max.x - min.x;
        let z_span = max.z - min.z;
        let release_seed = authored_flora_hash(
            paint_dab_serial ^ species_index.wrapping_mul(0x517c_c1b7) ^ 0x68bc_21ebu32,
        );
        let is_outlier_release = authored_flora_release_is_outlier(
            paint_dab_serial,
            species_index,
            distribution.outlier_chance,
        );
        let existing_anchor = (!is_outlier_release).then(|| {
            authored_flora_existing_cluster_anchor(
                edit,
                &existing_base_centers_vox,
                distribution.cluster_radius_voxels,
                release_seed,
            )
        });
        let mut release_anchor_vox = existing_anchor.flatten();

        for plant_index in 0..distribution.plants_per_release {
            let mut best_candidate: Option<(f32, UVec3, Vec3, u32)> = None;

            for attempt in 0..AUTHORED_FLORA_NATURAL_CANDIDATES_PER_PLANT {
                let seed = authored_flora_candidate_seed(
                    species_index,
                    paint_dab_serial,
                    plant_index,
                    attempt,
                );
                let x_seed = authored_flora_hash(seed ^ 0xa511_e9b3);
                let z_seed = authored_flora_hash(seed ^ 0x63d8_3595);
                let x_vox = min.x
                    + ((authored_flora_unit_from_seed(x_seed) * x_span as f32).floor() as u32)
                        .min(x_span.saturating_sub(1));
                let z_vox = min.z
                    + ((authored_flora_unit_from_seed(z_seed) * z_span as f32).floor() as u32)
                        .min(z_span.saturating_sub(1));

                let pos_xz = Vec2::new((x_vox as f32 + 0.5) / 256.0, (z_vox as f32 + 0.5) / 256.0);
                let terrain_height = self.query_terrain_height_cpu(pos_xz);
                let base_y = (terrain_height * 256.0).floor().max(0.0) as u32;
                if base_y + 1 >= world_dim.y {
                    continue;
                }

                let base_center_vox =
                    Vec3::new(x_vox as f32 + 0.5, base_y as f32 + 0.5, z_vox as f32 + 0.5);
                if distance_sq_to_segment(base_center_vox, start_vox, end_vox) > radius_sq {
                    continue;
                }

                let cluster_anchor_vox = *release_anchor_vox.get_or_insert(base_center_vox);
                let Some(score) = authored_flora_natural_candidate_score(
                    base_center_vox,
                    cluster_anchor_vox,
                    &existing_base_centers_vox,
                    &placed_base_centers_vox,
                    distribution,
                    seed,
                ) else {
                    continue;
                };
                if best_candidate
                    .as_ref()
                    .is_none_or(|(best_score, _, _, _)| score > *best_score)
                {
                    best_candidate = Some((
                        score,
                        UVec3::new(x_vox, base_y + 1, z_vox),
                        base_center_vox,
                        seed,
                    ));
                }
            }

            let Some((_, stem_world_vox, base_center_vox, seed)) = best_candidate else {
                continue;
            };
            if self
                .surface_builder
                .try_add_authored_flora_instance_deferred(
                    species_index,
                    stem_world_vox,
                    AUTHORED_FLORA_GROWTH_MATURE,
                    seed,
                    &mut dirty_chunks,
                )
            {
                placed_base_centers_vox.push(base_center_vox);
            }
        }

        self.surface_builder
            .sync_authored_flora_dirty_chunks(&dirty_chunks)?;
        Ok(())
    }

    pub(super) fn apply_surface_flora_removal(&mut self, edit: TerrainBrushEdit) -> Result<()> {
        if let Some(compiled) = TerrainSurfaceRemovalService::compile_surface_brush(edit) {
            world_ops::mesh_remove_flora_for_brush_edit(
                &mut self.surface_builder,
                super::VOXEL_DIM_PER_CHUNK,
                compiled.rebuild_bound,
                world_ops::FloraBrushEdit {
                    start: edit.start,
                    end: edit.end,
                    radius: edit.radius,
                    tick: self.flora_tick,
                },
            )?;
        } else {
            log::info!(
                "Flora removal compile skipped: start={:?}, end={:?}, radius={}",
                edit.start,
                edit.end,
                edit.radius
            );
        }
        Ok(())
    }

    pub(super) fn apply_flora_trim(&mut self, edit: TerrainRemovalEdit) -> Result<()> {
        let brush_edit = TerrainBrushEdit {
            start: edit.center,
            end: edit.center,
            radius: edit.radius,
        };
        if let Some(compiled) = TerrainSurfaceRemovalService::compile_surface_brush(brush_edit) {
            let target_age = super::FLORA_TRIM_MAX_GROWTH_PROGRESS;
            let growing_chunks = world_ops::mesh_trim_flora_for_brush_edit(
                &mut self.surface_builder,
                super::VOXEL_DIM_PER_CHUNK,
                compiled.rebuild_bound,
                world_ops::FloraBrushEdit {
                    start: brush_edit.start,
                    end: brush_edit.end,
                    radius: brush_edit.radius,
                    tick: self.flora_tick,
                },
                target_age,
            )?;
            for chunk_id in growing_chunks {
                self.track_growing_flora_chunk(chunk_id);
            }
        } else {
            log::info!(
                "Flora trim compile skipped: start={:?}, end={:?}, radius={}",
                brush_edit.start,
                brush_edit.end,
                brush_edit.radius
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_tree_placement(
        &mut self,
        tree_id: u32,
        tree_pos: Vec3,
        this_bound: UAabb3,
        rebuild_bound: UAabb3,
        rebuild_chunk_count: usize,
        quantized_leaf_positions: &[UVec3],
        quantized_apple_positions: &[UVec3],
        world_leaf_positions: &[Vec3],
        total_start: Instant,
        compile_elapsed: std::time::Duration,
        trunk_elapsed: std::time::Duration,
        rebuild_elapsed: std::time::Duration,
        trunk_count: usize,
        benchmark_gui_tree: bool,
    ) -> Result<()> {
        let add_leaves_start = Instant::now();
        self.tracer.add_tree_leaves(
            &mut self.surface_builder.resources,
            tree_id,
            quantized_leaf_positions,
        )?;
        self.tracer.add_tree_apples(
            &mut self.surface_builder.resources,
            tree_id,
            quantized_apple_positions,
        )?;
        self.request_vsm_history_reset();
        let add_leaves_elapsed = add_leaves_start.elapsed();
        if benchmark_gui_tree {
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("tree_gui_add_leaves", add_leaves_elapsed);
        }

        self.prev_bound = self.prev_bound.union_with(&this_bound);

        let cluster_start = Instant::now();
        let leaf_clusters = cluster_positions(world_leaf_positions, super::LEAF_CLUSTER_DISTANCE);
        let cluster_elapsed = cluster_start.elapsed();
        if benchmark_gui_tree {
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("tree_gui_cluster_leaves", cluster_elapsed);
        }

        let audio_start = Instant::now();
        self.tree_audio_manager.add_tree_sources_from_clusters(
            tree_id,
            tree_pos,
            &leaf_clusters,
            false,
            true,
        )?;
        let audio_elapsed = audio_start.elapsed();
        if benchmark_gui_tree {
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("tree_gui_audio_sources", audio_elapsed);
        }

        self.tree_records.insert(
            tree_id,
            TreeRecord {
                position: tree_pos,
                bound: this_bound,
            },
        );

        let emitter_start = Instant::now();
        self.upsert_tree_leaf_emitter(tree_id, tree_pos, &this_bound, &leaf_clusters);
        let emitter_elapsed = emitter_start.elapsed();
        if benchmark_gui_tree {
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("tree_gui_leaf_emitter", emitter_elapsed);

            log::info!(
                "[PERF][TREE_GUI] add_total {:.2}ms compile {:.2}ms trunk_voxel {:.2}ms add_leaves {:.2}ms rebuild {:.2}ms cluster {:.2}ms audio {:.2}ms emitter {:.2}ms trunks {} leaves {} apples {} clusters {} rebuild_chunks {} bound {:?}",
                total_start.elapsed().as_secs_f32() * 1000.0,
                compile_elapsed.as_secs_f32() * 1000.0,
                trunk_elapsed.as_secs_f32() * 1000.0,
                add_leaves_elapsed.as_secs_f32() * 1000.0,
                rebuild_elapsed.as_secs_f32() * 1000.0,
                cluster_elapsed.as_secs_f32() * 1000.0,
                audio_elapsed.as_secs_f32() * 1000.0,
                emitter_elapsed.as_secs_f32() * 1000.0,
                trunk_count,
                quantized_leaf_positions.len(),
                quantized_apple_positions.len(),
                leaf_clusters.len(),
                rebuild_chunk_count,
                rebuild_bound,
            );
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub(super) fn apply_tree_placement(&mut self, edit: TreePlacementEdit) -> Result<()> {
        let total_start = Instant::now();
        let TreePlacementEdit {
            tree_desc,
            placement,
            options,
        } = edit;
        let benchmark_gui_tree = !options.assign_new_id;

        if options.clean_before_add {
            self.clean_up_prev_tree()?;
        }

        let TreePlacement::World(tree_pos) = placement;

        let tree_id = if options.assign_new_id {
            let current_id = self.next_tree_id;
            self.next_tree_id += 1;
            current_id
        } else {
            self.single_tree_id
        };

        let compile_start = Instant::now();
        let compiled = TreePlacementService::compile(tree_desc, tree_pos, UAabb3::default());
        let compile_elapsed = compile_start.elapsed();
        if benchmark_gui_tree {
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("tree_gui_compile", compile_elapsed);
        }

        let trunk_count = match &compiled.trunk_voxel_edit {
            VoxelEdit::StampRoundCones { round_cones, .. } => round_cones.len(),
            _ => 0,
        };

        let trunk_start = Instant::now();
        self.execute_edit_plan(WorldEditPlan::with_voxel(compiled.trunk_voxel_edit))?;
        let trunk_elapsed = trunk_start.elapsed();
        if benchmark_gui_tree {
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("tree_gui_trunk_voxel", trunk_elapsed);
        }

        let rebuild_start = Instant::now();
        self.execute_edit_plan(WorldEditPlan::with_build(
            BuildEdit::RebuildMeshWithoutFlora(compiled.rebuild_bound),
        ))?;
        let rebuild_elapsed = rebuild_start.elapsed();
        if benchmark_gui_tree {
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("tree_gui_rebuild", rebuild_elapsed);
        }

        self.finish_tree_placement(
            tree_id,
            compiled.tree_pos,
            compiled.this_bound,
            compiled.rebuild_bound,
            world_ops::affected_chunk_indices_for_bound(
                compiled.rebuild_bound,
                super::VOXEL_DIM_PER_CHUNK,
            )
            .len(),
            &compiled.quantized_leaf_positions,
            &compiled.quantized_apple_positions,
            &compiled.world_leaf_positions,
            total_start,
            compile_elapsed,
            trunk_elapsed,
            rebuild_elapsed,
            trunk_count,
            benchmark_gui_tree,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_surface_removal_bvh_reaches_positive_atlas_edge() {
        let world_dim = super::super::VOXEL_DIM_PER_CHUNK * super::super::CHUNK_DIM;
        let positive_edge_x = world_dim.x as f32 / super::super::VOXEL_DIM_PER_CHUNK.x as f32;
        let compiled = TerrainSurfaceRemovalService::compile(TerrainRemovalEdit {
            center: Vec3::new(positive_edge_x, 0.5, 0.5),
            radius: super::super::TERRAIN_EDIT_DEFAULT_RADIUS,
        })
        .expect("edge-overlapping edit should compile");

        let VoxelEdit::StampSurfaceSpheres { bvh_nodes, .. } = &compiled.voxel_edit else {
            panic!("expected surface sphere edit");
        };

        assert_eq!(bvh_nodes[0].aabb.max().x, world_dim.x as f32);
        assert_eq!(compiled.rebuild_bound.max().x, world_dim.x - 1);
    }

    #[test]
    fn terrain_brush_capsule_bound_covers_start_end_and_radius() {
        let radius = super::super::TERRAIN_EDIT_DEFAULT_RADIUS;
        let compiled = TerrainSurfaceRemovalService::compile_surface_brush(TerrainBrushEdit {
            start: Vec3::new(0.25, 0.5, 0.25),
            end: Vec3::new(0.75, 0.5, 0.75),
            radius,
        })
        .expect("capsule edit should compile");

        let radius_vox = (radius * 256.0).ceil() as u32;
        assert!(compiled.rebuild_bound.min().x <= 64_u32.saturating_sub(radius_vox));
        assert!(compiled.rebuild_bound.min().z <= 64_u32.saturating_sub(radius_vox));
        assert!(
            compiled.rebuild_bound.max().x >= 192_u32.saturating_add(radius_vox).saturating_sub(1)
        );
        assert!(
            compiled.rebuild_bound.max().z >= 192_u32.saturating_add(radius_vox).saturating_sub(1)
        );
    }
}
