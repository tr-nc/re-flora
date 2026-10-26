//! Typed architectural recipes, not saved tree identity or a second set of live GUI values.
//! Numeric variation lives here; genuinely different curve/layout algorithms live in axis/.
use std::ops::{Range, RangeInclusive};

#[derive(Clone, Copy)]
pub(in crate::tree_gen) enum Architecture {
    Recursive,
    LegacyConifer,
    Spreading(&'static SpreadingCrown),
    Layered(&'static LayeredCrown),
}

#[derive(Clone, Copy)]
pub(in crate::tree_gen) enum FoliageRule {
    RecursiveDepth,
    TwigTips,
    TwigAxes,
}

#[derive(Clone, Copy)]
pub(in crate::tree_gen) struct GrowthRule {
    pub architecture: Architecture,
    pub foliage: FoliageRule,
}

#[derive(Clone, Copy)]
pub(in crate::tree_gen) enum ShootRule {
    Rising,
    Pendant,
}

pub(in crate::tree_gen) struct SpreadingCrown {
    pub stations: &'static [(usize, u32)],
    pub height: f32,
    pub crown_radius: f32,
    pub scaffold_lift: Range<f32>,
    pub shoots: ShootRule,
}

pub(in crate::tree_gen) struct LayeredCrown {
    pub height: f32,
    pub crown_radius: f32,
    pub branches_per_station: RangeInclusive<u32>,
    pub scaffold_sag: Range<f32>,
}

pub(in crate::tree_gen) const RECURSIVE: GrowthRule = GrowthRule {
    architecture: Architecture::Recursive,
    foliage: FoliageRule::RecursiveDepth,
};
pub(in crate::tree_gen) const LEGACY_CONIFER: GrowthRule = GrowthRule {
    architecture: Architecture::LegacyConifer,
    foliage: FoliageRule::TwigAxes,
};
pub(in crate::tree_gen) const APPLE: GrowthRule = GrowthRule {
    architecture: Architecture::Spreading(&SpreadingCrown {
        stations: &[(2, 2), (3, 3), (5, 2), (6, 2), (7, 2)],
        height: 100.,
        crown_radius: 82.,
        scaffold_lift: 1.15..1.65,
        shoots: ShootRule::Rising,
    }),
    foliage: FoliageRule::TwigTips,
};
pub(in crate::tree_gen) const PINE: GrowthRule = GrowthRule {
    architecture: Architecture::Layered(&LayeredCrown {
        height: 205.,
        crown_radius: 90.,
        branches_per_station: 2..=4,
        scaffold_sag: 0.06..0.27,
    }),
    foliage: FoliageRule::TwigAxes,
};
// Unpollarded London plane: clear lower trunk, ascending major limbs and a
// broad, open rounded crown. Not a willow's curtain or a clipped street-tree ball.
pub(in crate::tree_gen) const PLANE: GrowthRule = GrowthRule {
    architecture: Architecture::Spreading(&SpreadingCrown {
        stations: &[(3, 2), (4, 2), (5, 2), (6, 2), (7, 1)],
        height: 190.,
        crown_radius: 112.,
        scaffold_lift: 0.65..1.0,
        shoots: ShootRule::Rising,
    }),
    foliage: FoliageRule::TwigAxes,
};
pub(in crate::tree_gen) const WILLOW: GrowthRule = GrowthRule {
    architecture: Architecture::Spreading(&SpreadingCrown {
        stations: &[(2, 2), (3, 3), (5, 2), (6, 2), (7, 2)],
        height: 182.,
        crown_radius: 98.,
        scaffold_lift: 0.50..0.75,
        shoots: ShootRule::Pendant,
    }),
    foliage: FoliageRule::TwigAxes,
};
