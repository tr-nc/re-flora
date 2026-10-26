//! Saved planting controls are sampled into a new tree's mature description.
//! Growth and restore consume that owned description, never today's controls.
use super::*;
use crate::app::gui_config::GuiAdjustables;
use crate::tree_gen::TreeSpecies;

mod review;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::app::core) struct TreePlantingSettings {
    scale: f32,
    apple_density: f32,
    instant_mature: bool,
}

impl TreePlantingSettings {
    fn from_gui(gui: &GuiAdjustables) -> Self {
        let bounded = |value: f32, low, high, fallback| {
            if value.is_finite() {
                value.clamp(low, high)
            } else {
                fallback
            }
        };
        Self {
            instant_mature: gui.tree_planting_instant_mature.value,
            scale: bounded(gui.tree_planting_scale.value, 0.5, 2., 1.),
            apple_density: bounded(gui.tree_planting_apple_density.value, 0., 1., 0.25),
        }
    }

    pub(in crate::app::core) fn initial_age(self) -> f32 {
        growth::planting_age(self.instant_mature)
    }

    pub(in crate::app::core) fn apply(self, mut desc: TreeDesc) -> TreeDesc {
        // Use the species' canonical planting size, not the tuning tree's size:
        // loading a previously scaled tree into the editor must not compound scale.
        desc.size = desc.species.default_desc().size * self.scale;
        desc.fruit_spawn_probability = if desc.species.bears_apples() {
            self.apple_density
        } else {
            0.
        };
        desc
    }
}

impl App {
    pub(in crate::app::core) fn tree_planting_settings(&self) -> TreePlantingSettings {
        TreePlantingSettings::from_gui(&self.debug_settings.adjustables)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maturity_preference_refreshes_preview_age_without_changing_the_mature_description() {
        let mut settings = crate::app::gui_config::DebugSettings::load();
        let desc = TreeSpecies::AppleTree.default_desc();
        settings.adjustables.tree_planting_instant_mature.value = true;
        let adult = TreePlantingSettings::from_gui(&settings.adjustables);
        settings.adjustables.tree_planting_instant_mature.value = false;
        let sapling = TreePlantingSettings::from_gui(&settings.adjustables);
        assert_ne!(
            adult, sapling,
            "the UI change detector must refresh the preview"
        );
        assert_eq!(adult.initial_age(), 1.);
        assert_eq!(sapling.initial_age(), SAPLING_AGE);
        assert_eq!(adult.apply(desc.clone()), sapling.apply(desc));
    }

    #[test]
    fn planting_controls_keep_owned_trees_and_growth_targets_independent() {
        let mut settings = crate::app::gui_config::DebugSettings::load();
        settings.adjustables.tree_planting_scale.value = 0.75;
        settings.adjustables.tree_planting_apple_density.value = 0.2;
        let authored = TreeSpecies::AppleTree.default_desc();
        let planted = TreePlantingSettings::from_gui(&settings.adjustables).apply(authored.clone());
        let stored = serde_json::to_string(&planted).unwrap();
        settings.adjustables.tree_planting_scale.value = 1.5;
        settings.adjustables.tree_planting_apple_density.value = 0.8;
        let next = TreePlantingSettings::from_gui(&settings.adjustables).apply(authored.clone());
        assert_eq!(planted.size, authored.size * 0.75);
        assert_eq!(next.size, authored.size * 1.5);
        assert_eq!(planted.fruit_spawn_probability, 0.2);
        assert_eq!(next.fruit_spawn_probability, 0.8);
        let restored: TreeDesc = serde_json::from_str(&stored).unwrap();
        for age in [SAPLING_AGE, 0.5, 0.75, 1.] {
            assert_eq!(restored.at_age(age), planted.at_age(age));
            assert_eq!(planted.at_age(age).fruit_spawn_probability, 0.2);
        }
        assert_eq!(serde_json::to_string(&planted).unwrap(), stored);
    }

    #[test]
    fn planting_scale_is_species_relative_and_never_compounds_after_restore() {
        let settings = TreePlantingSettings {
            scale: 1.5,
            apple_density: 1.,
            instant_mature: true,
        };
        for species in [
            TreeSpecies::AppleTree,
            TreeSpecies::Pine,
            TreeSpecies::WeepingWillow,
        ] {
            let mut authored = species.default_desc();
            authored.branching.seed = 907;
            let first = settings.apply(authored.clone());
            let again = settings.apply(first.clone());
            assert_eq!(again, first);
            assert_eq!(first.size, authored.size * 1.5);
            assert_eq!(first.branching.seed, 907);
            assert_eq!(
                first.fruit_spawn_probability,
                if species.bears_apples() { 1. } else { 0. }
            );
            assert!(Tree::new(first).height_voxels() > Tree::new(authored).height_voxels());
        }
    }

    #[test]
    fn apple_density_thins_stable_candidates_without_changing_tree_geometry() {
        let mut desc = TreeSpecies::AppleTree.default_desc();
        desc.branching.seed = 907;
        let tree = Tree::new(desc.clone());
        let origin = Vec3::splat(256.);
        let leaves: Vec<_> = tree
            .relative_leaf_positions()
            .iter()
            .map(|p| (*p + origin).round().as_uvec3())
            .collect();
        let fruit_ids = |density| {
            let adjusted = TreePlantingSettings {
                scale: 1.,
                apple_density: density,
                instant_mature: true,
            }
            .apply(desc.clone());
            assert_eq!(Tree::new(adjusted.clone()).branches(), tree.branches());
            tree_fruit_specs(&adjusted, origin, &leaves, 1.)
                .into_iter()
                .map(|fruit| fruit.id)
                .collect::<HashSet<_>>()
        };
        let sparse = fruit_ids(0.25);
        let dense = fruit_ids(1.);
        assert!(fruit_ids(0.).is_empty());
        assert!(!sparse.is_empty() && sparse.len() < dense.len());
        assert!(sparse.is_subset(&dense));
    }
}
