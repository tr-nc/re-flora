//! Frozen legacy layout and RNG order: saved Conifers must not silently change shape.
use super::{AxisSkeleton, Compiler};
use crate::tree_gen::TreeDesc;
use glam::Vec3;
use rand::{rngs::StdRng, RngExt, SeedableRng};
use std::f32::consts::TAU;

/// An authored architectural profile, distinct from geometry sampling resolution.
struct ConiferProfile {
    height: f32,
    crown_radius: f32,
    stations: u32,
    branches_per_station: u32,
    branch_radius_ratio: f32,
    twig_radius_ratio: f32,
}

impl Default for ConiferProfile {
    fn default() -> Self {
        Self {
            height: 170.,
            crown_radius: 64.,
            stations: 9,
            branches_per_station: 4,
            branch_radius_ratio: 0.30,
            twig_radius_ratio: 0.32,
        }
    }
}

pub(super) fn generate(desc: &TreeDesc) -> AxisSkeleton {
    let profile = ConiferProfile::default();
    let scale = (desc.size / 30.).max(0.);
    let height = profile.height * scale;
    let base_radius = (desc.size * desc.trunk_thickness).max(0.);
    let mut rng = StdRng::seed_from_u64(desc.branching.seed ^ 0xC091_FE42_A315_7721);
    let mut compiler = Compiler {
        skeleton: AxisSkeleton {
            segments: Vec::new(),
            metadata: Vec::new(),
        },
        next_axis: 0,
    };
    // Every whorl is an explicit endpoint on the same continuous trunk axis.
    let steps = profile.stations + 3;
    let lean = Vec3::new(
        rng.random_range(-0.018..=0.018),
        0.,
        rng.random_range(-0.018..=0.018),
    );
    let points = (0..=steps)
        .map(|i| {
            let u = i as f32 / steps as f32;
            Vec3::Y * height * u + lean * height * u * u
        })
        .collect::<Vec<_>>();
    let radii = (0..=steps)
        .map(|i| {
            let u = i as f32 / steps as f32;
            base_radius * (1. - 0.97 * u).powf(0.85)
        })
        .collect::<Vec<_>>();
    let reveal = (0..=steps)
        .map(|i| i as f32 / steps as f32 * 0.55)
        .collect::<Vec<_>>();
    let trunk = compiler.axis(None, &points, &radii, &reveal);
    let phase = rng.random_range(0.0..TAU);

    for station in 2..=profile.stations + 1 {
        let u = station as f32 / steps as f32;
        let parent = trunk[station as usize - 1];
        let base = points[station as usize];
        // Separate crown envelope from trunk internode length: lower limbs are wider.
        let crown_width = profile.crown_radius * scale * ((1. - u) / (1. - 2. / steps as f32));
        for branch in 0..profile.branches_per_station {
            let angle = phase
                + station as f32 * 2.399963
                + branch as f32 * TAU / profile.branches_per_station as f32
                + rng.random_range(-0.13..=0.13);
            let direction = Vec3::new(angle.cos(), 0., angle.sin());
            let length = crown_width * rng.random_range(0.88..=1.06);
            let axis_points = (0..=3)
                .map(|i| {
                    let v = i as f32 / 3.;
                    base + direction * length * v + Vec3::Y * length * (0.10 * v + 0.16 * v * v)
                })
                .collect::<Vec<_>>();
            let branch_radius = radii[station as usize] * profile.branch_radius_ratio;
            let axis_radii = (0..=3)
                .map(|i| branch_radius * (1. - 0.9 * i as f32 / 3.))
                .collect::<Vec<_>>();
            let birth = 0.12 + u * 0.55;
            let axis_reveal = (0..=3)
                .map(|i| birth + i as f32 / 3. * 0.12)
                .collect::<Vec<_>>();
            let lateral = compiler.axis(Some(parent), &axis_points, &axis_radii, &axis_reveal);

            // Feathered side sprays fill a layered conical crown without branching explosion.
            for site in 1..=3 {
                let side = Vec3::new(-direction.z, 0., direction.x);
                for sign in [-1., 1.] {
                    let twig_dir =
                        (direction * 0.55 + side * sign * 0.8 + Vec3::Y * 0.15).normalize();
                    let twig_length =
                        length * (0.33 - 0.04 * site as f32) * rng.random_range(0.85..=1.1);
                    let root = axis_points[site];
                    let twig_points = [
                        root,
                        root + twig_dir * twig_length * 0.5,
                        root + twig_dir * twig_length,
                    ];
                    let radius = axis_radii[site] * profile.twig_radius_ratio;
                    let radii = [radius, radius * 0.55, radius * 0.12];
                    let reveal = [
                        axis_reveal[site].min(0.9),
                        (axis_reveal[site] + 0.06).min(0.96),
                        1.,
                    ];
                    compiler.axis(Some(lateral[site - 1]), &twig_points, &radii, &reveal);
                }
            }
        }
    }
    compiler.skeleton
}
