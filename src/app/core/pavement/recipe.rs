//! Material/layout choice frozen at gesture start. The interaction and atlas
//! publication owners are shared; paving-only tuning stays with paving.
use super::*;
use crate::app::path_band::PathSettings;

#[derive(Clone, Copy, Debug)]
pub(super) enum ConstructionRecipe {
    Pavement(PavementRecipe),
    LowWall(LowWallRecipe),
}
impl From<PavementRecipe> for ConstructionRecipe {
    fn from(recipe: PavementRecipe) -> Self {
        Self::Pavement(recipe)
    }
}
impl From<LowWallRecipe> for ConstructionRecipe {
    fn from(recipe: LowWallRecipe) -> Self {
        Self::LowWall(recipe)
    }
}
impl ConstructionRecipe {
    pub(super) fn path_settings(self) -> PathSettings {
        match self {
            Self::Pavement(recipe) => recipe.path_settings(),
            Self::LowWall(recipe) => recipe.path_settings(),
        }
    }
    pub(super) fn stroke(self, start: Vec3, heading: Vec3) -> Option<PathStroke> {
        match self {
            Self::Pavement(recipe) => recipe.stroke(start, heading),
            Self::LowWall(recipe) => PathStroke::new(recipe.path_settings(), start, heading),
        }
    }
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Pavement(recipe) => match recipe.kind {
                PavementKind::Slabs => "Slabs",
                PavementKind::Brick => "Brick",
                PavementKind::Dirt => "Dirt",
            },
            Self::LowWall(recipe) => match recipe.kind {
                LowWallKind::Earth => "EarthWall",
                LowWallKind::Stone => "StoneWall",
                LowWallKind::Brick => "BrickWall",
            },
        }
    }
    pub(super) fn is_dirt(self) -> bool {
        matches!(
            self,
            Self::Pavement(PavementRecipe {
                kind: PavementKind::Dirt,
                ..
            })
        )
    }
    pub(super) fn retraction_scale(self) -> Option<f32> {
        match self {
            Self::Pavement(recipe) if recipe.retract_stroke && !self.is_dirt() => {
                Some(recipe.retract_scale)
            }
            _ => None,
        }
    }
    pub(super) fn accepts(self, coverage: &PavementCoverage, cell: PathCell) -> bool {
        match self {
            Self::Pavement(_) => coverage.accepts(cell),
            // Wall columns intentionally share their physical end plane. Only
            // interior overlap is illegal; paving's joint clearance would reject
            // every second column. Coverage still records each whole column once.
            Self::LowWall(_) => !coverage.published_pieces().iter().any(|prior| {
                crate::app::path_band::overlaps_quads(prior.footprint, cell.footprint, -1e-6)
            }),
        }
    }
    /// Preview geometry only. Wall publication/coverage uses a whole column,
    /// while its preview shows the individual layers at their actual elevations.
    pub(super) fn pieces(self, row: PathSample) -> Vec<PathCell> {
        match self {
            Self::Pavement(recipe) => recipe.pieces(row),
            Self::LowWall(recipe) => recipe
                .pieces(row)
                .into_iter()
                .map(|s| s.preview())
                .collect(),
        }
    }
}
