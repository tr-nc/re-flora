//! Broad, irregular veteran architecture compiled into ordinary connected axes.
//! Long scaffolds carry secondary branches and numerous leaf-bearing spur shoots;
//! foliage is never a detached crown shell. Old species forms remain unchanged.
use super::{AxisSkeleton, Compiler, TreeDesc};
use glam::Vec3;
use rand::{rngs::StdRng, RngExt, SeedableRng};
use std::f32::consts::{PI, TAU};

fn radial(a: f32) -> Vec3 {
    Vec3::new(a.cos(), 0., a.sin())
}
fn taper(radius: f32, count: usize, tip: f32) -> Vec<f32> {
    (0..=count)
        .map(|i| radius * (1. - (1. - tip) * i as f32 / count as f32))
        .collect()
}
fn ages(birth: f32, duration: f32, count: usize) -> Vec<f32> {
    (0..=count)
        .map(|i| (birth + duration * i as f32 / count as f32).min(1.))
        .collect()
}

pub(super) fn generate(desc: &TreeDesc) -> AxisSkeleton {
    let mut out = Compiler::default();
    let mut rng = StdRng::seed_from_u64(desc.branching.seed ^ 0x9A21_50F1_6320_771D);
    let scale = (desc.size / 30.).max(0.);
    let height = 142. * scale;
    let crown = 110. * scale;
    let phase = rng.random_range(0.0..TAU);
    let lean = radial(phase) * height * 0.08;
    let across = radial(phase + PI * 0.5);
    let points: Vec<_> = (0..=10)
        .map(|i| {
            let u = i as f32 / 10.;
            Vec3::Y * height * u
                + lean * u.powf(1.25)
                + across * height * 0.035 * (u * 3. * PI).sin() * u.sqrt()
                + radial(phase + u * 7.) * height * 0.012 * (u * PI).sin()
        })
        .collect();
    let base = desc.size * desc.trunk_thickness;
    let radii: Vec<_> = (0..=10)
        .map(|i| {
            let u = i as f32 / 10.;
            let flare = 1. + 0.65 * (-u * 14.).exp();
            base * (1. - 0.86 * u).powf(0.72) * flare * (1. + 0.07 * (phase + i as f32 * 1.8).sin())
        })
        .collect();
    let trunk = out.axis(None, &points, &radii, &ages(0., 0.42, 10));
    for (station, count) in [(3, 3), (4, 2), (5, 3), (6, 2), (7, 2), (8, 1), (9, 1)] {
        let u = station as f32 / 10.;
        for limb in 0..count {
            let angle = phase
                + station as f32 * 2.3999631
                + limb as f32 * TAU / count as f32
                + rng.random_range(-0.35..0.35);
            let dir = radial(angle);
            let side = radial(angle + PI * 0.5);
            let length = crown * (1. - u * 0.33) * rng.random_range(0.83..1.13);
            let lift = rng.random_range(0.25..0.52);
            let curl = rng.random_range(-0.22..0.22);
            let root = points[station];
            let fork: Vec<_> = (0..=6)
                .map(|i| {
                    let v = i as f32 / 6.;
                    root + dir * length * v
                        + side * length * (curl * (v * PI).sin() + 0.045 * (v * 3. * PI).sin())
                        + Vec3::Y * length * (lift * v + 0.24 * v * v)
                })
                .collect();
            let fork_radii = taper(radii[station] * rng.random_range(0.62..0.78), 6, 0.07);
            let birth = 0.12 + u * 0.28;
            let fork_ages = ages(birth, 0.20, 6);
            let scaffold = out.axis(Some(trunk[station - 1]), &fork, &fork_radii, &fork_ages);
            for site in 2..=6 {
                for sign in [-1., 1.] {
                    let root = fork[site];
                    let direction = (dir * rng.random_range(0.2..0.5)
                        + side * sign * 0.8
                        + Vec3::Y * rng.random_range(0.45..0.95))
                    .normalize();
                    let len = length * rng.random_range(0.28..0.48);
                    let branch: Vec<_> = (0..=3)
                        .map(|i| {
                            let v = i as f32 / 3.;
                            root + direction * len * v + dir * len * 0.16 * v * v
                                - Vec3::Y * len * 0.15 * v * v
                        })
                        .collect();
                    let branch_radii = taper(fork_radii[site] * 0.38, 3, 0.10);
                    let branch_ages = ages(fork_ages[site], 0.18, 3);
                    let secondary = out.axis(
                        Some(scaffold[site - 1]),
                        &branch,
                        &branch_radii,
                        &branch_ages,
                    );
                    for (j, attach) in [2, 3].into_iter().enumerate() {
                        let root = branch[attach];
                        let fan_dir = (dir * (0.4 + 0.3 * j as f32)
                            + side * sign * if j == 0 { -0.6 } else { 0.6 }
                            + Vec3::Y * rng.random_range(0.35..0.85))
                        .normalize();
                        let len = len * rng.random_range(0.32..0.58);
                        let spur = [
                            root,
                            root + fan_dir * len * 0.52,
                            root + fan_dir * len + Vec3::Y * len * 0.10,
                        ];
                        out.axis(
                            Some(secondary[attach - 1]),
                            &spur,
                            &taper(branch_radii[attach] * 0.35, 2, 0.08),
                            &ages(branch_ages[attach], 0.13, 2),
                        );
                    }
                }
            }
        }
    }
    out.skeleton
}
