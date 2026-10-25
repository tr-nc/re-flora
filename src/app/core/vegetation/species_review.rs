//! Opt-in native garden fixture: real selection, preview, placement, age, snapshots and GPU pose.
//! Never writes config or garden saves. Captures use the ordinary --screenshot path.
use super::*;
use crate::tree_gen::TreeSpecies;
use glam::Vec3Swizzles;

pub(in crate::app::core) struct TreeSpeciesReview {
    species: Vec<TreeSpecies>,
    seed: u64,
    walking_view: bool,
    frame: u32,
    target: Vec3,
}

impl TreeSpeciesReview {
    pub(in crate::app::core) fn from_env() -> Result<Option<Self>> {
        let Ok(mode) = std::env::var("RE_FLORA_TREE_SPECIES_REVIEW") else {
            return Ok(None);
        };
        let species = match mode.as_str() {
            "apple" | "apple-walk" => vec![TreeSpecies::AppleTree],
            "pine" => vec![TreeSpecies::Pine],
            "willow" => vec![TreeSpecies::WeepingWillow],
            "mixed" => TreeSpecies::PLANTABLE.to_vec(),
            _ => anyhow::bail!(
                "RE_FLORA_TREE_SPECIES_REVIEW must be apple, apple-walk, pine, willow or mixed"
            ),
        };
        let seed = std::env::var("RE_FLORA_TREE_SPECIES_SEED")
            .ok()
            .map(|s| s.parse::<u64>())
            .transpose()
            .context("invalid tree review seed")?
            .unwrap_or(122);
        Ok(Some(Self {
            species,
            seed,
            walking_view: mode == "apple-walk",
            frame: 0,
            target: Vec3::ZERO,
        }))
    }
}

impl App {
    pub(in crate::app::core) fn prepare_tree_species_review(&mut self) -> Result<()> {
        let Some(mut review) = self.tree_species_review.take() else {
            return Ok(());
        };
        if !self.terrain_persistence.allows_world_updates() {
            self.tree_species_review = Some(review);
            return Ok(());
        }
        if review.frame == 0 {
            self.clear_procedural_trees()?;
            self.remove_tree(self.trees.tuned_tree_id())?;
            self.debug_settings.adjustables.tree_age.value = 1.;
            self.debug_settings.adjustables.fruit_cycle.value = 0.84;
            self.debug_settings.adjustables.raster_tree_wind.value = true;
            self.debug_settings.adjustables.auto_daynight_cycle.value = false;
            self.set_manual_time_of_day(0.43);
            self.select_item_panel_slot(crate::app::core::ui_style::GROW_SLOT_INDEX);
            let mut center = Vec3::ZERO;
            for (index, species) in review.species.iter().copied().enumerate() {
                self.select_plant_variety(crate::app::core::plant_catalog::PlantVariety::Tree(
                    species,
                ));
                self.sync_tree_placement_preview_from_gui()?;
                self.tree_placement_preview_desc.branching.seed =
                    review.seed.wrapping_add(index as u64);
                self.rebuild_tree_placement_preview()?;
                let desc = self.tree_placement_preview_desc.clone();
                let horizontal = if review.species.len() == 1 {
                    Vec2::new(0.85, 1.45)
                } else {
                    // The ordinary garden is only two world units wide. Keep whole crowns,
                    // not just roots, inside it; an out-of-atlas World placement is not a
                    // valid fixture for the terrain publication or snapshot contract.
                    let extent =
                        (self.world_chunk_dim * super::super::VOXEL_DIM_PER_CHUNK).as_vec3() / 256.;
                    Vec2::new(extent.x * (0.25 + index as f32 * 0.25), extent.z * 0.62)
                };
                let position = Vec3::new(
                    horizontal.x,
                    self.query_terrain_height_cpu(horizontal),
                    horizontal.y,
                );
                self.add_tree(
                    desc.clone(),
                    TreePlacement::World(position),
                    TreeAddOptions::default().with_new_id(),
                )?;
                let record = self
                    .trees
                    .records
                    .values()
                    .find(|r| r.mature_desc == desc)
                    .context("review tree not published")?;
                anyhow::ensure!(
                    record.rest_tree.branches() == Tree::new(desc.clone()).branches(),
                    "preview/placement differ"
                );
                anyhow::ensure!(
                    !record.leaf_render_positions.is_empty(),
                    "review tree has no leaves"
                );
                anyhow::ensure!(
                    !record.fruit_specs.is_empty() == species.bears_apples(),
                    "incorrect species fruit"
                );
                log::info!("[TREE_SPECIES_REVIEW] planted={species:?} seed={} axes={} leaves={} fruit={} height_voxels={:.3} position={position:?} real_preview=true saved=false",
                    desc.branching.seed, record.rest_tree.branches().len(), record.leaf_render_positions.len(), record.fruit_specs.len(), record.rest_tree.height_voxels());
                center += position;
            }
            // Clear Grow after exercising selection/preview; don't let the next-placement
            // ghost obscure the planted crown in captures.
            self.select_item_panel_slot(crate::app::core::ui_style::GROW_SLOT_INDEX);
            review.target = center / review.species.len() as f32
                + Vec3::Y * if review.walking_view { 0.08 } else { 0.22 };
        }
        // Retain the fixture view after any one-shot startup camera placement.
        self.camera_control.apply_snapshot_mode(true);
        self.camera_control.set_orbit_focus(review.target);
        let eye = if review.walking_view {
            // Frozen review at the ordinary standing eye height, not an elevated orbit.
            // This opt-in pose does not change the player or any saved camera preference.
            let xz = review.target.xz() + Vec2::new(0.12, 0.28);
            Vec3::new(xz.x, self.query_terrain_height_cpu(xz) + 0.08, xz.y)
        } else {
            let distance = if review.species.len() == 1 { 1.0 } else { 1.8 };
            review.target + Vec3::new(0.25, 0.16, distance)
        };
        anyhow::ensure!(
            self.tracer.set_camera_pose_looking_at(eye, review.target),
            "tree review camera"
        );
        self.reset_camera_movement_input();
        if review.frame == 30 {
            // Exercise serialization, not just a clone, and the real garden observer restore.
            self.debug_settings.adjustables.tree_age.value = 0.45;
            self.update_all_tree_ages_from_gui()?;
            let snapshot = self.capture_tree_snapshot();
            let decoded: TreeSnapshot = serde_json::from_slice(&serde_json::to_vec(&snapshot)?)?;
            let prepared = self.prepare_tree_snapshot(&decoded)?;
            self.restore_tree_snapshot(prepared)?;
            self.verify_restored_tree_rendering(&decoded)?;
            self.debug_settings.adjustables.tree_age.value = 1.;
            self.update_all_tree_ages_from_gui()?;
        }
        if review.frame == 60 {
            self.validate_tree_poses()?;
            self.validate_tree_surface_pose()?;
            self.tracer.validate_gpu_tree_surface()?;
            self.tracer.validate_gpu_tree_lighting(true)?;
            log::info!("[TREE_SPECIES_REVIEW] passed species={:?} seed={} age_rebuild=true serialized_restore=true gpu_pose_surface_lighting=true saved=false", review.species, review.seed);
        }
        review.frame += 1;
        self.tree_species_review = Some(review);
        Ok(())
    }
}
