//! Opt-in native exercise of preview -> paid/free placement -> growth -> save/load.
use super::*;

impl App {
    pub(in crate::app::core) fn review_tree_planting_settings(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_TREE_PLANTING_REVIEW") else {
            return Ok(());
        };
        anyhow::ensure!(
            matches!(mode.as_str(), "seed" | "load"),
            "unknown planting review mode"
        );
        self.player_tools.planting.tree_species = TreeSpecies::AppleTree;
        // This older fixture tests exact scale/density, not random spreads.
        self.debug_settings.adjustables.tree_randomize_new.value = true;
        self.debug_settings.adjustables.tree_randomize_shape.value = true;
        self.debug_settings.adjustables.tree_size_variation.value = 0.;
        if mode == "seed" {
            // Only this explicit fixture clears the initial scene, before its baseline.
            let ids: Vec<_> = self.trees.records.keys().copied().collect();
            for id in ids {
                self.remove_tree(id)?;
            }
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            self.material_supply_diagnostic_override = Some(true);
            self.debug_settings
                .adjustables
                .tree_planting_instant_mature
                .value = true;
            self.debug_settings.adjustables.tree_planting_scale.value = 1.;
            self.debug_settings
                .adjustables
                .tree_planting_apple_density
                .value = 1.;
            self.tree_placement_individual = 907;
            self.sync_tree_placement_preview_from_gui()?;
            let first_desc = self.tree_placement_preview_desc.clone();
            let ground = self.query_terrain_height_cpu(Vec2::new(0.72, 1.22));
            let first = self.plant_owned_tree(first_desc.clone(), Vec3::new(0.72, ground, 1.22))?;
            anyhow::ensure!(
                self.trees.records[&first].age == 1.
                    && self.trees.records[&first].growth_seconds.is_none(),
                "instant planting did not create an adult without a growth clock"
            );
            let old = self.capture_tree_snapshot();
            let generation = self.trees.records[&first]
                .canopy_acoustic_descriptor
                .generation();
            self.debug_settings.adjustables.tree_planting_scale.value = 1.5;
            self.debug_settings
                .adjustables
                .tree_planting_apple_density
                .value = 0.1;
            self.sync_tree_placement_preview_from_gui()?;
            anyhow::ensure!(
                self.tree_placement_preview_desc.branching.seed == 907,
                "slider edit rerolled preview seed"
            );
            anyhow::ensure!(
                self.capture_tree_snapshot() == old
                    && self.trees.records[&first]
                        .canopy_acoustic_descriptor
                        .generation()
                        == generation,
                "planting controls changed or republished an existing tree"
            );
            self.tree_placement_individual = 908;
            self.sync_tree_placement_preview_from_gui()?;
            let second_desc = self.tree_placement_preview_desc.clone();
            self.material_supply_diagnostic_override = Some(false);
            self.debug_settings
                .adjustables
                .tree_planting_instant_mature
                .value = false;
            self.garden_economy.buy("sapling/apple_tree")?;
            let stock_before = self.garden_economy.count("sapling/apple_tree");
            let ground = self.query_terrain_height_cpu(Vec2::new(1.25, 1.22));
            let second =
                self.plant_owned_tree(second_desc.clone(), Vec3::new(1.25, ground, 1.22))?;
            anyhow::ensure!(
                self.trees.records[&second].age == SAPLING_AGE
                    && self.trees.records[&second].growth_seconds == Some(0.)
                    && self.trees.records[&second].mature_desc == second_desc
                    && self.garden_economy.count("sapling/apple_tree") + 1 == stock_before,
                "paid placement did not capture preview settings or consume one sapling"
            );
            let before_change = self.capture_tree_snapshot();
            self.debug_settings
                .adjustables
                .tree_planting_instant_mature
                .value = true;
            self.debug_settings.adjustables.tree_planting_scale.value = 0.5;
            self.debug_settings
                .adjustables
                .tree_planting_apple_density
                .value = 0.;
            self.sync_tree_placement_preview_from_gui()?;
            anyhow::ensure!(
                self.capture_tree_snapshot() == before_change,
                "controls changed a growing tree"
            );
            self.advance_tree_growth(900.)?;
            anyhow::ensure!(
                self.trees.records[&second].age == 1.
                    && self.trees.records[&second].mature_desc == second_desc
                    && self.trees.records[&first].mature_desc == first_desc
                    && self.trees.records[&first]
                        .canopy_acoustic_descriptor
                        .generation()
                        == generation,
                "later growth consumed current planting settings or republished old tree"
            );
        }
        let old = self.capture_tree_snapshot();
        let first = self
            .trees
            .records
            .values()
            .find(|r| r.mature_desc.branching.seed == 907)
            .context("first planted tree missing")?;
        let second = self
            .trees
            .records
            .values()
            .find(|r| r.mature_desc.branching.seed == 908)
            .context("second planted tree missing")?;
        let base = TreeSpecies::AppleTree.default_desc().size;
        anyhow::ensure!(
            self.trees.records.len() == 2
                && first.age == 1.
                && second.age == 1.
                && (first.mature_desc.size - base).abs() < 1e-5
                && (second.mature_desc.size - base * 1.5).abs() < 1e-5
                && first.mature_desc.fruit_spawn_probability == 1.
                && second.mature_desc.fruit_spawn_probability == 0.1
                && first.fruit_specs.len() > second.fruit_specs.len(),
            "per-tree size/density did not survive planting or load"
        );
        let first_count = first.fruit_specs.len();
        let second_count = second.fruit_specs.len();
        self.debug_settings.adjustables.tree_planting_scale.value = 0.5;
        self.debug_settings
            .adjustables
            .tree_planting_apple_density
            .value = 0.;
        self.sync_tree_placement_preview_from_gui()?;
        anyhow::ensure!(
            self.capture_tree_snapshot() == old
                && (self.tree_placement_preview_desc.size - base * 0.5).abs() < 1e-5
                && self.tree_placement_preview_desc.fruit_spawn_probability == 0.,
            "loaded trees followed current planting settings"
        );
        self.verify_restored_tree_rendering(&old)?;
        log::info!("[TREE_PLANTING_REVIEW] mode={mode} first_scale=1 first_density=1 first_fruits={first_count} second_scale=1.5 second_density=0.1 second_fruits={second_count} next_scale=0.5 next_density=0 old_trees_unchanged=true instant_adult=true unchecked_sapling=true growth_independent=true verified=true");
        Ok(())
    }
}
