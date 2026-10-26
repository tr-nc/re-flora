//! Layout/curve adapters for the shared axis compiler. No tree-species identity switches.
//! Parameters describe garden-scale architecture, not botanical growth simulation.
use super::{AxisSkeleton, Compiler, TreeDesc};
use crate::tree_gen::growth::rules::{LayeredCrown, ShootRule, SpreadingCrown};
use glam::Vec3;
use rand::{rngs::StdRng, RngExt, SeedableRng};
use std::f32::consts::{PI, TAU};

const GOLDEN_ANGLE: f32 = 2.399_963_1;

fn radial(angle: f32) -> Vec3 {
    Vec3::new(angle.cos(), 0., angle.sin())
}

fn taper(radius: f32, steps: usize, tip_ratio: f32) -> Vec<f32> {
    (0..=steps)
        .map(|i| radius * (1. - (1. - tip_ratio) * i as f32 / steps as f32))
        .collect()
}

fn growing_leader_radii(desc: &TreeDesc, mut radii: Vec<f32>) -> Vec<f32> {
    // Garden saplings author a slender cylindrical leader before developing the
    // mature taper. Keeping this in the architectural recipe preserves the
    // authored-radius contract of the axis compiler and legacy tree profiles.
    if desc.growth_age < 1. && desc.trunk_thickness > 0. {
        let mature_blend = ((desc.growth_age - 0.5) * 2.).clamp(0., 1.);
        for radius in &mut radii {
            *radius = 0.85 + (*radius - 0.85) * mature_blend;
        }
    }
    radii
}

fn reveal(birth: f32, duration: f32, steps: usize) -> Vec<f32> {
    (0..=steps)
        .map(|i| (birth + duration * i as f32 / steps as f32).min(1.))
        .collect()
}

pub(super) fn spreading(desc: &TreeDesc, profile: &SpreadingCrown) -> AxisSkeleton {
    let mut out = Compiler::default();
    let mut rng = StdRng::seed_from_u64(desc.branching.seed ^ 0xA991_E723_5711_029D);
    let scale = (desc.size / 30.).max(0.);
    let height = profile.height * scale;
    let crown = profile.crown_radius * scale;
    let phase = rng.random_range(0.0..TAU);
    let lean = radial(phase) * height * rng.random_range(0.025..0.065);
    let trunk_points: Vec<_> = (0..=8)
        .map(|i| {
            let u = i as f32 / 8.;
            Vec3::Y * height * u + lean * (u * u + 0.18 * (u * PI).sin())
        })
        .collect();
    let trunk_radii = growing_leader_radii(desc, taper(desc.size * desc.trunk_thickness, 8, 0.06));
    let trunk = out.axis(None, &trunk_points, &trunk_radii, &reveal(0., 0.48, 8));

    // Unequal scaffold levels, not a stack of identical umbrellas. The trunk remains
    // connected through the crown; spaces between leaf masses reveal the curved wood.
    for &(station, count) in profile.stations {
        let u = station as f32 / 8.;
        let envelope = (1. - ((u - 0.53) / 0.72).powi(2)).max(0.).sqrt();
        for limb in 0..count {
            let angle = phase
                + station as f32 * GOLDEN_ANGLE
                + limb as f32 * TAU / count as f32
                + rng.random_range(-0.28..0.28);
            let dir = radial(angle);
            let side = radial(angle + PI * 0.5);
            let length = crown * envelope * rng.random_range(0.78..1.15);
            let curl = rng.random_range(-0.14..0.14);
            let lift = rng.random_range(profile.scaffold_lift.clone());
            let root = trunk_points[station];
            let points: Vec<_> = (0..=4)
                .map(|i| {
                    let v = i as f32 / 4.;
                    root + dir * length * v
                        + side * length * curl * (v * PI).sin()
                        + Vec3::Y * length * (lift * v - 0.30 * v * v)
                })
                .collect();
            let radii = taper(trunk_radii[station] * 0.52, 4, 0.10);
            let birth = 0.10 + u * 0.42;
            let ages = reveal(birth, 0.16, 4);
            let scaffold = out.axis(Some(trunk[station - 1]), &points, &radii, &ages);
            for site in 2..=4 {
                for sign in [-1., 1.] {
                    let root = points[site];
                    let twig_radius = radii[site] * 0.30;
                    let start_age = ages[site];
                    match profile.shoots {
                        ShootRule::Pendant => {
                            // Each shoot has a distinct bend and free end. Leaf sprays follow the
                            // descending axis instead of painting a solid sphere around its tip.
                            let drop =
                                (root.y - 16. * scale).max(0.) * rng.random_range(0.58..0.92);
                            let sweep = length * rng.random_range(0.12..0.27);
                            let across = (dir * 0.65 + side * sign * 0.76).normalize();
                            let points: Vec<_> = (0..=6)
                                .map(|i| {
                                    let v = i as f32 / 6.;
                                    root + across * sweep * (1. - (-3. * v).exp())
                                        + side * sign * sweep * 0.12 * (v * TAU).sin()
                                        - Vec3::Y * drop * v.powf(1.35)
                                })
                                .collect();
                            out.axis(
                                Some(scaffold[site - 1]),
                                &points,
                                &taper(twig_radius, 6, 0.08),
                                &reveal(start_age, 0.28, 6),
                            );
                        }
                        ShootRule::Rising => {
                            let direction =
                                (dir * 0.55 + side * sign * 0.72 + Vec3::Y * 0.65).normalize();
                            let length = length * rng.random_range(0.27..0.45);
                            let points: Vec<_> = (0..=3)
                                .map(|i| {
                                    let v = i as f32 / 3.;
                                    root + direction * length * v - Vec3::Y * length * 0.22 * v * v
                                })
                                .collect();
                            out.axis(
                                Some(scaffold[site - 1]),
                                &points,
                                &taper(twig_radius, 3, 0.12),
                                &reveal(start_age, 0.25, 3),
                            );
                        }
                    }
                }
            }
        }
    }
    out.skeleton
}

pub(super) fn layered(desc: &TreeDesc, profile: &LayeredCrown) -> AxisSkeleton {
    let mut out = Compiler::default();
    let mut rng = StdRng::seed_from_u64(desc.branching.seed ^ 0x916E_5C07_5721_473D);
    let scale = (desc.size / 30.).max(0.);
    let height = profile.height * scale;
    let phase = rng.random_range(0.0..TAU);
    let lean = radial(phase) * height * rng.random_range(0.025..0.055);
    // Seeded internode spacing and branch counts break the fixed-whorl repetition.
    let heights: Vec<_> = (0..=12)
        .map(|i| {
            if i == 0 || i == 12 {
                i as f32 / 12.
            } else {
                (i as f32 + rng.random_range(-0.28..0.28)) / 12.
            }
        })
        .collect();
    let points: Vec<_> = heights
        .iter()
        .map(|&u| {
            Vec3::Y * height * u
                + lean * u * u
                + radial(phase + PI * 0.5) * height * 0.015 * (u * TAU).sin()
        })
        .collect();
    let radii: Vec<_> = heights
        .iter()
        .map(|&u| desc.size * desc.trunk_thickness * (1. - 0.97 * u).powf(0.85))
        .collect();
    let radii = growing_leader_radii(desc, radii);
    let ages: Vec<_> = heights.iter().map(|&u| u * 0.50).collect();
    let trunk = out.axis(None, &points, &radii, &ages);
    for station in 3..=11 {
        let u = heights[station];
        let count = rng.random_range(profile.branches_per_station.clone());
        for limb in 0..count {
            let angle = phase
                + station as f32 * GOLDEN_ANGLE
                + limb as f32 * TAU / count as f32
                + rng.random_range(-0.35..0.35);
            let dir = radial(angle);
            let side = radial(angle + PI * 0.5);
            let length =
                profile.crown_radius * scale * (1. - u).powf(0.45) * rng.random_range(0.60..1.15);
            let sag = rng.random_range(profile.scaffold_sag.clone());
            let curl = rng.random_range(-0.12..0.12);
            let limb_points: Vec<_> = (0..=4)
                .map(|i| {
                    let v = i as f32 / 4.;
                    points[station]
                        + dir * length * v
                        + side * length * curl * (v * PI).sin()
                        + Vec3::Y * length * (-sag * v + 0.48 * v * v * v)
                })
                .collect();
            let limb_radii = taper(radii[station] * 0.38, 4, 0.10);
            let ages = reveal(0.12 + u * 0.48, 0.16, 4);
            let lateral = out.axis(Some(trunk[station - 1]), &limb_points, &limb_radii, &ages);
            for site in 2..=4 {
                for sign in [-1., 1.] {
                    let direction = (dir * 0.55 + side * sign * 0.78 + Vec3::Y * 0.32).normalize();
                    let len = length * (0.43 - site as f32 * 0.05) * rng.random_range(0.8..1.3);
                    let root = limb_points[site];
                    let twig_points = [
                        root,
                        root + direction * len * 0.5,
                        root + direction * len + Vec3::Y * len * 0.18,
                    ];
                    out.axis(
                        Some(lateral[site - 1]),
                        &twig_points,
                        &taper(limb_radii[site] * 0.32, 2, 0.10),
                        &reveal(ages[site], 0.20, 2),
                    );
                }
            }
        }
    }
    out.skeleton
}
