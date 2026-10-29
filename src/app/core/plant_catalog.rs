//! Grow's catalog and session selection, independent of toolbar slots and render backends.
use crate::flora::species::{self, FloraPaintSelection};
pub(crate) use crate::tree_gen::TreeSpecies;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum PlantType {
    #[default]
    Flora,
    Tree,
}

impl PlantType {
    pub(crate) const ALL: [Self; 2] = [Self::Flora, Self::Tree];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Flora => "Flora",
            Self::Tree => "Tree",
        }
    }

    pub(crate) fn varieties(self) -> Vec<PlantVariety> {
        match self {
            Self::Flora => species::PLAYER_FLORA_PAINT_SELECTIONS
                .iter()
                .copied()
                .map(PlantVariety::Flora)
                .collect(),
            Self::Tree => TreeSpecies::PLANTABLE
                .iter()
                .copied()
                .map(PlantVariety::Tree)
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlantVariety {
    Flora(FloraPaintSelection),
    Tree(TreeSpecies),
}

impl PlantVariety {
    pub(crate) fn plant_type(self) -> PlantType {
        match self {
            Self::Flora(_) => PlantType::Flora,
            Self::Tree(_) => PlantType::Tree,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Flora(selection) => species::flora_paint_selection_label(selection),
            Self::Tree(species) => species.label(),
        }
    }
}

/// Each category remembers its variety when switching Type or leaving Grow.
#[derive(Clone, Debug)]
pub(super) struct PlantingSelection {
    pub(super) plant_type: PlantType,
    // Retain index access for existing flora benchmark/scene authoring callers.
    pub(super) flora_index: usize,
    pub(super) tree_species: TreeSpecies,
}

impl Default for PlantingSelection {
    fn default() -> Self {
        Self {
            plant_type: PlantType::Flora,
            flora_index: 0,
            tree_species: TreeSpecies::AppleTree,
        }
    }
}

impl PlantingSelection {
    pub(super) fn flora(&self) -> FloraPaintSelection {
        let catalog = species::PLAYER_FLORA_PAINT_SELECTIONS;
        catalog[self.flora_index % catalog.len()]
    }

    pub(super) fn variety(&self) -> PlantVariety {
        match self.plant_type {
            PlantType::Flora => PlantVariety::Flora(self.flora()),
            PlantType::Tree => PlantVariety::Tree(self.tree_species),
        }
    }

    pub(super) fn select_type(&mut self, plant_type: PlantType) -> bool {
        if self.plant_type == plant_type {
            return false;
        }
        self.plant_type = plant_type;
        true
    }

    pub(super) fn select_variety(&mut self, variety: PlantVariety) -> bool {
        if !variety.plant_type().varieties().contains(&variety) || self.variety() == variety {
            return false;
        }
        match variety {
            PlantVariety::Flora(flora) => {
                self.flora_index = species::PLAYER_FLORA_PAINT_SELECTIONS
                    .iter()
                    .position(|&entry| entry == flora)
                    .unwrap();
            }
            PlantVariety::Tree(species) => self.tree_species = species,
        }
        self.plant_type = variety.plant_type();
        true
    }

    pub(super) fn next_variety(&self) -> PlantVariety {
        let catalog = self.plant_type.varieties();
        let current = catalog
            .iter()
            .position(|&entry| entry == self.variety())
            .unwrap_or(0);
        catalog[(current + 1) % catalog.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_preserves_flora_and_offers_four_distinct_garden_tree_species() {
        use crate::tree_gen::TreeDesc;
        assert_eq!(
            PlantType::Flora.varieties(),
            species::PLAYER_FLORA_PAINT_SELECTIONS
                .iter()
                .copied()
                .map(PlantVariety::Flora)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            PlantType::Tree.varieties(),
            vec![
                PlantVariety::Tree(TreeSpecies::AppleTree),
                PlantVariety::Tree(TreeSpecies::Pine),
                PlantVariety::Tree(TreeSpecies::LondonPlane),
                PlantVariety::Tree(TreeSpecies::Ginkgo)
            ]
        );
        let authored = TreeDesc::default();
        assert_eq!(TreeSpecies::AppleTree.mature_desc(&authored), authored);
    }

    #[test]
    fn categories_remember_varieties_and_cycle_locally() {
        let mut selection = PlantingSelection::default();
        let flower = PlantVariety::Flora(species::PLAYER_FLORA_PAINT_SELECTIONS[1]);
        assert!(selection.select_variety(flower));
        assert!(selection.select_type(PlantType::Tree));
        assert_eq!(
            selection.variety(),
            PlantVariety::Tree(TreeSpecies::AppleTree)
        );
        assert_eq!(
            selection.next_variety(),
            PlantVariety::Tree(TreeSpecies::Pine)
        );
        assert!(selection.select_variety(selection.next_variety()));
        assert_eq!(
            selection.next_variety(),
            PlantVariety::Tree(TreeSpecies::LondonPlane)
        );
        assert!(selection.select_type(PlantType::Flora));
        assert!(selection.select_type(PlantType::Tree));
        assert_eq!(selection.variety(), PlantVariety::Tree(TreeSpecies::Pine));
        assert!(selection.select_type(PlantType::Flora));
        assert_eq!(selection.variety(), flower);
        assert!(
            !selection.select_variety(PlantVariety::Flora(FloraPaintSelection::Species(u32::MAX)))
        );
        assert_eq!(selection.variety(), flower);
    }
}
