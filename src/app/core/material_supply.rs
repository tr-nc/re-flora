//! Debug material supply is independent of geometry bounds, support and the real backpack.
use super::voxel_backpack::BackpackVoxel;
use super::*;

impl App {
    pub(super) fn infinite_materials_enabled(&self) -> bool {
        self.debug_settings.adjustables.infinite_materials.value
    }

    pub(super) fn selected_edit_material(&self) -> BackpackVoxel {
        self.rooftop_scene
            .as_ref()
            .map_or(self.player_tools.edit_material, |s| s.material)
    }

    pub(super) fn select_edit_material(&mut self, material: BackpackVoxel) {
        if let Some(scene) = self.rooftop_scene.as_mut() {
            scene.material = material;
        } else {
            self.player_tools.edit_material = material;
        }
    }

    /// Opt-in real-app validation in a fresh normal garden, not a unit/GPU benchmark.
    pub(super) fn validate_material_supply(&mut self) -> Result<()> {
        anyhow::ensure!(
            self.rooftop_scene.is_none(),
            "material supply check expects the normal garden"
        );
        let inventory = self.voxel_backpack.snapshot();
        anyhow::ensure!(
            inventory.iter().all(|e| e.count == 0),
            "material supply check expects an empty backpack"
        );
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        let center = self
            .contree_builder
            .query_terrain_ray_cpu(Vec3::new(0.5, 2.1, 0.5), Vec3::NEG_Y)
            .context("material supply check needs a terrain surface")?
            .position;
        let edit = crate::app::world_edits::TerrainBrushEdit {
            start: center,
            end: center,
            radius: 0.03,
        };
        let original = self.debug_settings.adjustables.infinite_materials.value;
        let check = (|| -> Result<()> {
            self.debug_settings.adjustables.infinite_materials.value = false;
            anyhow::ensure!(
                self.apply_soil_deposition(edit, 16, 1)?.added == 0,
                "finite mode created soil from an empty backpack"
            );
            self.debug_settings.adjustables.infinite_materials.value = true;
            let infinite = self.apply_soil_deposition(edit, 16, 1)?;
            anyhow::ensure!(
                infinite.added > 0,
                "infinite mode could not deposit with an empty backpack"
            );
            anyhow::ensure!(
                self.voxel_backpack.snapshot() == inventory,
                "infinite mode changed real inventory"
            );
            anyhow::ensure!(
                self.voxel_backpack.placement_supply(
                    self.infinite_materials_enabled(),
                    self.selected_edit_material()
                ) == Some((self.selected_edit_material(), u32::MAX)),
                "Edit did not share infinite supply"
            );
            self.debug_settings.adjustables.infinite_materials.value = false;
            anyhow::ensure!(
                self.apply_soil_deposition(edit, 16, 1)?.added == 0,
                "disabling infinite supply left a phantom stock"
            );
            // A fixture-owned seven-voxel grant verifies finite debit, then removes its remainder.
            self.voxel_backpack.deposit(BackpackVoxel::Dirt, 7);
            let finite = self.apply_soil_deposition(edit, 16, 1)?;
            anyhow::ensure!(
                finite.added > 0 && finite.added <= 7,
                "finite deposition exceeded stock or failed"
            );
            let remaining = self.voxel_backpack.count(BackpackVoxel::Dirt);
            anyhow::ensure!(
                remaining == 7 - finite.added,
                "finite mode did not debit the actual write"
            );
            self.voxel_backpack.withdraw(BackpackVoxel::Dirt, remaining);
            anyhow::ensure!(
                self.voxel_backpack.snapshot() == inventory,
                "supply check did not restore inventory"
            );
            log::info!("[MATERIAL_SUPPLY][CHECK] empty_finite=0 infinite_added={} finite_added={} toggle_back=true real_inventory_preserved=true shared_edit_supply=true normal_geometry_bounds=true", infinite.added, finite.added);
            Ok(())
        })();
        self.debug_settings.adjustables.infinite_materials.value = original;
        check
    }
}
