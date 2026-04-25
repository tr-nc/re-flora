use super::App;
use crate::app::world_edits::{
    BuildEdit, ClearVoxelRegionEdit, CubePlacementEdit, FencePostPlacementEdit, TerrainRemovalEdit,
    TreeAddOptions, TreePlacement, TreePlacementEdit, VoxelEdit, WorldEditPlan,
};
use crate::app::world_ops;
use crate::builder::{ChunkModifyReadback, VOXEL_TYPE_CHERRY_WOOD, VOXEL_TYPE_OAK_WOOD};
use crate::geom::{build_bvh, Cuboid, RoundCone, Sphere, UAabb3};
use crate::procedual_placer::{generate_positions, PlacerDesc};
use crate::tree_gen::{Tree, TreeDesc};
use crate::util::cluster_positions;
use anyhow::Result;
use glam::{UVec3, Vec2, Vec3};
use rand::Rng;
use std::collections::HashSet;
use std::time::Instant;

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
    pub tree_height_variance: f32,
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
            tree_height_variance: 0.0,
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
                egui::Slider::new(&mut self.tree_height_variance, 0.0..=1.0)
                    .text("Height Variance"),
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
        .min(max_inclusive);
        let max = UVec3::new(
            max_f.x.max(0.0).ceil() as u32,
            max_f.y.max(0.0).ceil() as u32,
            max_f.z.max(0.0).ceil() as u32,
        )
        .min(max_inclusive);
        if min.cmpgt(max).any() {
            return None;
        }

        let clipped_aabb = crate::geom::Aabb3::new(min.as_vec3(), max.as_vec3());
        let bvh_nodes = build_bvh(&[clipped_aabb], &[0_u32]).ok()?;
        let rebuild_bound = UAabb3::new(
            bvh_nodes[0].aabb.min_uvec3().min(max_inclusive),
            bvh_nodes[0].aabb.max_uvec3().min(max_inclusive),
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
}

struct CompiledTreePlacement {
    trunk_voxel_edit: VoxelEdit,
    rebuild_bound: UAabb3,
    tree_pos: Vec3,
    this_bound: UAabb3,
    quantized_leaf_positions: Vec<UVec3>,
    world_leaf_positions: Vec<Vec3>,
}

struct TreePlacementService;

impl TreePlacementService {
    fn compile(tree_desc: TreeDesc, tree_pos: Vec3, prev_bound: UAabb3) -> CompiledTreePlacement {
        let tree = Tree::new(tree_desc);
        let mut round_cones = Vec::with_capacity(tree.trunks().len());
        for tree_trunk in tree.trunks() {
            let mut round_cone = tree_trunk.clone();
            round_cone.transform(tree_pos * 256.0);
            round_cones.push(round_cone);
        }

        let leaves_data_sequential = (0..round_cones.len()).map(|i| i as u32).collect::<Vec<_>>();
        let aabbs = round_cones.iter().map(RoundCone::aabb).collect::<Vec<_>>();

        let bvh_nodes = build_bvh(&aabbs, &leaves_data_sequential).unwrap();
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
            set.into_iter().collect::<Vec<_>>()
        };

        CompiledTreePlacement {
            trunk_voxel_edit: VoxelEdit::StampRoundCones {
                bvh_nodes,
                round_cones,
                voxel_type: VOXEL_TYPE_CHERRY_WOOD,
            },
            rebuild_bound: this_bound.union_with(&prev_bound),
            tree_pos,
            this_bound,
            quantized_leaf_positions,
            world_leaf_positions,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct TreeRecord {
    position: Vec3,
    bound: UAabb3,
}

impl App {
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
            tree_desc.seed = rng.random_range(1..10000);

            self.apply_tree_variations(&mut tree_desc, &mut rng);
            self.apply_tree_placement(TreePlacementEdit {
                tree_desc,
                placement: TreePlacement::World(*tree_pos),
                options: TreeAddOptions::default().with_new_id(),
            })?;
        }

        Ok(())
    }

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

    pub(super) fn remove_tree(&mut self, tree_id: u32) -> Result<()> {
        self.tracer
            .remove_tree_leaves(&mut self.surface_builder.resources, tree_id)?;
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
            tree_desc.spread *=
                1.0 + rng.random_range(-config.spread_variance..=config.spread_variance);
        }

        if config.randomness_variance > 0.0 {
            tree_desc.randomness = (tree_desc.randomness
                + rng.random_range(-config.randomness_variance..=config.randomness_variance))
            .clamp(0.0, 1.0);
        }

        if config.vertical_tendency_variance > 0.0 {
            tree_desc.vertical_tendency = (tree_desc.vertical_tendency
                + rng.random_range(
                    -config.vertical_tendency_variance..=config.vertical_tendency_variance,
                ))
            .clamp(-1.0, 1.0);
        }

        if config.branch_probability_variance > 0.0 {
            tree_desc.branch_probability = (tree_desc.branch_probability
                + rng.random_range(
                    -config.branch_probability_variance..=config.branch_probability_variance,
                ))
            .clamp(0.0, 1.0);
        }

        if config.tree_height_variance > 0.0 {
            tree_desc.tree_height *=
                1.0 + rng.random_range(-config.tree_height_variance..=config.tree_height_variance);
        }

        if config.length_dropoff_variance > 0.0 {
            tree_desc.length_dropoff = (tree_desc.length_dropoff
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
            tree_desc.iterations =
                ((tree_desc.iterations as f32 + variation).round() as u32).clamp(1, 12);
        }

        if config.leaves_size_level_variance > 0.0 {
            let variation = rng.random_range(
                -config.leaves_size_level_variance..=config.leaves_size_level_variance,
            );
            tree_desc.leaves_size_level =
                ((tree_desc.leaves_size_level as f32 + variation).round() as u32).clamp(0, 8);
        }
    }

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
    ) -> Result<ChunkModifyReadback> {
        let total_start = Instant::now();
        if let Some(compiled) = TerrainSurfaceRemovalService::compile(edit) {
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
                        target_voxel_type,
                        max_write_count,
                    )?,
                _ => unreachable!("terrain surface removal compiled into unexpected edit type"),
            };
            let modify_elapsed = modify_start.elapsed();
            let mesh_start = Instant::now();
            world_ops::mesh_generate_preserve_flora_for_sphere_edit(
                &mut self.surface_builder,
                &mut self.contree_builder,
                &mut self.scene_accel_builder,
                super::VOXEL_DIM_PER_CHUNK,
                compiled.rebuild_bound,
                world_ops::FloraSphereEdit {
                    center: edit.center,
                    radius: edit.radius,
                    tick: self.flora_tick,
                },
            )?;
            let mesh_elapsed = mesh_start.elapsed();
            let total_elapsed = total_start.elapsed();
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("terrain_edit_removal_total", total_elapsed);
            crate::util::BENCH.lock().unwrap().summary();
            log::info!(
                "[TERRAIN_EDIT] removal total={:.3}ms modify={:.3}ms rebuild={:.3}ms center={:?} radius={:.3}",
                total_elapsed.as_secs_f64() * 1000.0,
                modify_elapsed.as_secs_f64() * 1000.0,
                mesh_elapsed.as_secs_f64() * 1000.0,
                edit.center,
                edit.radius,
            );
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
                    )?,
                _ => unreachable!("terrain surface placement compiled into unexpected edit type"),
            };
            let modify_elapsed = modify_start.elapsed();
            let mesh_start = Instant::now();
            world_ops::mesh_generate_preserve_flora_for_sphere_edit(
                &mut self.surface_builder,
                &mut self.contree_builder,
                &mut self.scene_accel_builder,
                super::VOXEL_DIM_PER_CHUNK,
                compiled.rebuild_bound,
                world_ops::FloraSphereEdit {
                    center: edit.center,
                    radius: edit.radius,
                    tick: self.flora_tick,
                },
            )?;
            let mesh_elapsed = mesh_start.elapsed();
            let total_elapsed = total_start.elapsed();
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("terrain_edit_placement_total", total_elapsed);
            crate::util::BENCH.lock().unwrap().summary();
            log::info!(
                "[TERRAIN_EDIT] placement total={:.3}ms modify={:.3}ms rebuild={:.3}ms center={:?} radius={:.3}",
                total_elapsed.as_secs_f64() * 1000.0,
                modify_elapsed.as_secs_f64() * 1000.0,
                mesh_elapsed.as_secs_f64() * 1000.0,
                edit.center,
                edit.radius,
            );
            return Ok(stats);
        }
        Ok(ChunkModifyReadback::default())
    }

    pub(super) fn apply_surface_flora_regeneration(
        &mut self,
        edit: TerrainRemovalEdit,
    ) -> Result<()> {
        if let Some(compiled) = TerrainSurfaceRemovalService::compile(edit) {
            world_ops::mesh_regenerate_flora_for_sphere_edit(
                &mut self.surface_builder,
                super::VOXEL_DIM_PER_CHUNK,
                compiled.rebuild_bound,
                world_ops::FloraSphereEdit {
                    center: edit.center,
                    radius: edit.radius,
                    tick: self.flora_tick.wrapping_sub(super::FLORA_FULL_GROWTH_TICKS),
                },
            )?;
        } else {
            log::info!(
                "Flora regen compile skipped: center={:?}, radius={}",
                edit.center,
                edit.radius
            );
        }
        Ok(())
    }

    pub(super) fn apply_flora_trim(&mut self, edit: TerrainRemovalEdit) -> Result<()> {
        if let Some(compiled) = TerrainSurfaceRemovalService::compile(edit) {
            let target_age = super::FLORA_FULL_GROWTH_TICKS / 2;
            world_ops::mesh_trim_flora_for_sphere_edit(
                &mut self.surface_builder,
                super::VOXEL_DIM_PER_CHUNK,
                compiled.rebuild_bound,
                world_ops::FloraSphereEdit {
                    center: edit.center,
                    radius: edit.radius,
                    tick: self.flora_tick,
                },
                target_age,
            )?;
        } else {
            log::info!(
                "Flora trim compile skipped: center={:?}, radius={}",
                edit.center,
                edit.radius
            );
        }
        Ok(())
    }

    pub(super) fn apply_tree_placement(&mut self, edit: TreePlacementEdit) -> Result<()> {
        let TreePlacementEdit {
            tree_desc,
            placement,
            options,
        } = edit;

        if options.clean_before_add {
            self.clean_up_prev_tree()?;
        }

        let tree_pos = match placement {
            TreePlacement::Terrain(horizontal) => {
                let terrain_height = self.query_terrain_height_cpu(horizontal);
                Vec3::new(horizontal.x, terrain_height, horizontal.y)
            }
            TreePlacement::World(world) => world,
        };

        let tree_id = if options.assign_new_id {
            let current_id = self.next_tree_id;
            self.next_tree_id += 1;
            current_id
        } else {
            self.single_tree_id
        };

        let compiled = TreePlacementService::compile(tree_desc, tree_pos, self.prev_bound);

        self.execute_edit_plan(WorldEditPlan::with_voxel(compiled.trunk_voxel_edit))?;
        self.tracer.add_tree_leaves(
            &mut self.surface_builder.resources,
            tree_id,
            &compiled.quantized_leaf_positions,
        )?;
        self.execute_edit_plan(WorldEditPlan::with_build(BuildEdit::RebuildMesh(
            compiled.rebuild_bound,
        )))?;

        self.prev_bound = compiled.rebuild_bound;

        let leaf_clusters =
            cluster_positions(&compiled.world_leaf_positions, super::LEAF_CLUSTER_DISTANCE);

        self.tree_audio_manager.add_tree_sources_from_clusters(
            tree_id,
            compiled.tree_pos,
            &leaf_clusters,
            false,
            true,
        )?;

        self.tree_records.insert(
            tree_id,
            TreeRecord {
                position: compiled.tree_pos,
                bound: compiled.this_bound,
            },
        );

        self.upsert_tree_leaf_emitter(
            tree_id,
            compiled.tree_pos,
            &compiled.this_bound,
            &leaf_clusters,
        );

        Ok(())
    }
}
