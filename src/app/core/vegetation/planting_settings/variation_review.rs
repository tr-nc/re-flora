//! Opt-in hidden app acceptance. This explicit fixture replaces the scene's trees;
//! it does not save GUI settings or a garden unless the caller separately asks to.
use super::*;

impl App {
    pub(in crate::app::core) fn review_tree_variation(&mut self) -> Result<()> {
        if std::env::var_os("RE_FLORA_TREE_VARIATION_REVIEW").is_none() {
            return Ok(());
        }
        let floats = [
            "tree_size_variation",
            "tree_hue_variation",
            "tree_hue_mean",
            "tree_planting_scale",
        ];
        let bools = ["tree_randomize_new", "tree_randomize_shape"];
        let old_floats: Vec<_> = floats
            .into_iter()
            .map(|id| {
                (
                    id,
                    GuiAdjustables::get_float_param_mut(&mut self.debug_settings.adjustables, id)
                        .unwrap()
                        .value,
                )
            })
            .collect();
        let old_bools: Vec<_> = bools
            .into_iter()
            .map(|id| {
                (
                    id,
                    GuiAdjustables::get_bool_param_mut(&mut self.debug_settings.adjustables, id)
                        .unwrap()
                        .value,
                )
            })
            .collect();
        let old_species = self.player_tools.planting.tree_species;
        let old_key = self.tree_placement_individual;
        let result = (|| -> Result<()> {
            for id in self.trees.records.keys().copied().collect::<Vec<_>>() {
                self.remove_tree(id)?;
            }
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            self.player_tools.planting.tree_species = TreeSpecies::AppleTree;
            let gui = &mut self.debug_settings.adjustables;
            gui.tree_randomize_new.value = true;
            gui.tree_randomize_shape.value = true;
            gui.tree_size_variation.value = 15.;
            gui.tree_hue_variation.value = 8.;
            gui.tree_planting_scale.value = 0.85;
            let mut planted = Vec::new();
            for (index, (x, z, hue)) in [(0.65, 0.7, -70.), (1.3, 0.7, -30.), (1., 1.4, 15.)]
                .into_iter()
                .enumerate()
            {
                self.tree_placement_individual = 907 + index as u64;
                self.debug_settings.adjustables.tree_hue_mean.value = hue;
                self.sync_tree_placement_preview_from_gui()?;
                let desc = self.tree_placement_preview_desc.clone();
                self.sync_tree_placement_preview_from_gui()?;
                anyhow::ensure!(
                    self.tree_placement_preview_desc == desc,
                    "preview rerolled while idle"
                );
                let ground = self.query_terrain_height_cpu(Vec2::new(x, z));
                let id = self.plant_tree_with_maturity(
                    desc.clone(),
                    Vec3::new(x, ground, z),
                    index != 0,
                )?;
                anyhow::ensure!(
                    self.trees.records[&id].mature_desc == desc,
                    "placement differs from preview"
                );
                log::info!(
                    "[TREE_VARIATION_REVIEW] tree={id} seed={} size={} appearance={:?}",
                    desc.branching.seed,
                    desc.size,
                    desc.appearance
                );
                planted.push((id, desc));
                self.advance_tree_placement_preview()?;
            }
            let before = self.capture_tree_snapshot();
            self.debug_settings.adjustables.tree_randomize_new.value = false;
            self.sync_tree_placement_preview_from_gui()?;
            let fixed = self.tree_placement_preview_desc.clone();
            self.advance_tree_placement_preview()?;
            anyhow::ensure!(
                self.tree_placement_preview_desc == fixed,
                "disabled variation still rerolls"
            );
            anyhow::ensure!(
                before == self.capture_tree_snapshot(),
                "toggle edited existing trees"
            );
            self.debug_settings.adjustables.tree_hue_mean.value = 55.;
            self.debug_settings.adjustables.tree_planting_scale.value = 0.5;
            self.sync_tree_placement_preview_from_gui()?;
            self.advance_tree_growth(900.)?;
            for (id, desc) in &planted {
                anyhow::ensure!(
                    self.trees.records[id].mature_desc == *desc,
                    "growth resampled current controls"
                );
                anyhow::ensure!(
                    self.surface_builder.resources.instances.leaves_instances[id].appearance
                        == desc.appearance,
                    "leaf publication lost per-tree color"
                );
            }
            anyhow::ensure!(
                self.trees.records[&planted[0].0].age == 1.,
                "sapling did not mature"
            );
            let snapshot = self.capture_tree_snapshot();
            let decoded: super::super::TreeSnapshot =
                serde_json::from_str(&serde_json::to_string(&snapshot)?)?;
            let prepared = self.prepare_tree_snapshot(&decoded)?;
            self.restore_tree_snapshot(prepared)?;
            anyhow::ensure!(
                self.capture_tree_snapshot() == snapshot,
                "snapshot changed resolved individuals"
            );
            self.verify_restored_tree_rendering(&decoded)?;
            log::info!("[TREE_VARIATION_REVIEW] verified=true preview_stable=true fixed_toggle=true existing_unchanged=true growth=true snapshot=true color_publication=true trees={}", planted.len());
            Ok(())
        })();
        for (id, value) in old_floats {
            GuiAdjustables::get_float_param_mut(&mut self.debug_settings.adjustables, id)
                .unwrap()
                .value = value;
        }
        for (id, value) in old_bools {
            GuiAdjustables::get_bool_param_mut(&mut self.debug_settings.adjustables, id)
                .unwrap()
                .value = value;
        }
        self.player_tools.planting.tree_species = old_species;
        self.tree_placement_individual = old_key;
        self.sync_tree_placement_preview_from_gui()?;
        result
    }
}
