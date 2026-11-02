//! An intimate walkable foreground and a separate, rocky borrowed-landscape backdrop.
//! Both are ordinary editable atlas columns, not a distant mesh or enlarged world.
use super::{ellipse, rear_wall_z, unit, ARRIVAL};
use glam::Vec2;

pub(super) struct Column {
    pub height: u32,
    pub soil_depth: u32,
}

fn foreground(p: Vec2, seed: u32) -> f32 {
    let lobe = |c, r| (-ellipse(p, c, r).powi(2)).exp();
    let shift = (unit(seed) - 0.5) * 0.04;
    116. + 67. * lobe(Vec2::new(0.48 + shift, 0.29), Vec2::new(0.32, 0.35))
        + 22. * lobe(Vec2::new(0.76, 0.52), Vec2::new(0.18, 0.24))
        + 12. * lobe(Vec2::new(0.27, 0.61), Vec2::new(0.20, 0.22))
        - 10. * lobe(Vec2::new(0.35, 0.52), Vec2::new(0.10, 0.25))
        - 5. * lobe(ARRIVAL, Vec2::splat(0.18))
}

fn smoothstep(a: f32, b: f32, value: f32) -> f32 {
    let u = ((value - a) / (b - a)).clamp(0., 1.);
    u * u * (3. - 2. * u)
}

fn relief(p: Vec2, seed: u32) -> f32 {
    // A quiet strip behind the wall keeps the enclosure legible and its feet on soil.
    let separation = smoothstep(0.025, 0.075, rear_wall_z(p.x) - p.y);
    if separation == 0. {
        return 0.;
    }
    let shift = (unit(seed ^ 0x72696467) - 0.5) * 0.025;
    let ridge = |center: Vec2, radius: Vec2, height: f32, skew: f32| {
        let d = (p - center) / radius;
        let across = d.x + skew * d.y;
        let distance = Vec2::new(across, d.y).length();
        // Broad folds and smaller ribs break up the silhouette and rock faces.
        // Their amplitude dies at the summit and foot, keeping bounds / support stable.
        let folds = (d.x * 8. + d.y * 3.).sin() * (d.y * 7. - d.x * 2.).sin()
            + 0.35 * (d.x * 17. - d.y * 5.).sin();
        let envelope = (1. - distance).max(0.);
        height * envelope.powf(1.25) * (1. + 0.24 * folds * distance.min(1.))
    };
    let main = ridge(
        Vec2::new(0.43 + shift, 0.065),
        Vec2::new(0.24, 0.085),
        267.,
        0.30,
    );
    let knuckle = ridge(
        Vec2::new(0.35 + shift, 0.078),
        Vec2::new(0.14, 0.075),
        225.,
        -0.45,
    );
    let shoulder = ridge(
        Vec2::new(0.71 - shift, 0.080),
        Vec2::new(0.22, 0.085),
        211.,
        -0.35,
    );
    let left_spur = ridge(Vec2::new(0.20, 0.095), Vec2::new(0.17, 0.095), 147., -0.25);
    let right_spur = ridge(Vec2::new(0.88, 0.115), Vec2::new(0.14, 0.080), 100., 0.40);
    // Returning to low ground at the finite canvas edge avoids a sliced-off peak.
    separation
        * smoothstep(0., 0.035, p.y)
        * main
            .max(knuckle)
            .max(shoulder)
            .max(left_spur)
            .max(right_spur)
}

#[cfg(test)]
pub fn height_voxels(p: Vec2, seed: u32) -> u32 {
    column(p, seed).height
}

pub(super) fn column(p: Vec2, seed: u32) -> Column {
    let rise = relief(p, seed);
    let height = (foreground(p, seed) + rise).round() as u32;
    let rock_line =
        130. + 32. * (p.x * 39. + p.y * 23.).sin() + 22. * (p.x * 21. - p.y * 43.).cos();
    let soil_depth = if rise > rock_line {
        // Expose steep rock faces, but retain soil on lower shoulders / grassy ledges.
        // This is an authored material rule, not another height owner or live erosion.
        let dx = (relief(p + Vec2::X / 1024., seed) - relief(p - Vec2::X / 1024., seed)) * 0.5;
        let dz = (relief(p + Vec2::Y / 1024., seed) - relief(p - Vec2::Y / 1024., seed)) * 0.5;
        if rise > 235. || Vec2::new(dx, dz).length() > 1.5 {
            0
        } else {
            12
        }
    } else {
        30
    };
    Column { height, soil_depth }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden_layout::{REAR_WALL, TEMPLE_CENTER};

    #[test]
    fn background_has_unequal_peaks_a_saddle_and_headroom_inside_the_existing_world() {
        for seed in [crate::garden_layout::DEFAULT_SEED, 0, 7, u32::MAX] {
            let main = height_voxels(Vec2::new(0.43, 0.065), seed);
            let shoulder = height_voxels(Vec2::new(0.71, 0.080), seed);
            let saddle = height_voxels(Vec2::new(0.57, 0.075), seed);
            assert!(main > 400 && main < 460, "main={main}");
            assert!(
                shoulder > 330 && shoulder + 25 < main,
                "main={main} shoulder={shoulder}"
            );
            assert!(saddle + 40 < main && saddle < shoulder);
            assert!(height_voxels(TEMPLE_CENTER, seed) + 200 < main);
            for z in 0..512 {
                for x in 0..512 {
                    let p = Vec2::new(x as f32, z as f32) / 512.;
                    assert!(
                        height_voxels(p, seed) < 460,
                        "terrain must leave vertical headroom"
                    );
                }
            }
        }
    }

    #[test]
    fn mountains_never_change_the_courtyard_or_the_wall_substrate() {
        for seed in [crate::garden_layout::DEFAULT_SEED, 7] {
            for z in 0..256 {
                for x in 0..256 {
                    let p = Vec2::new(x as f32, z as f32) / 256.;
                    if p.y >= rear_wall_z(p.x) - 0.025 {
                        let c = column(p, seed);
                        assert_eq!(c.height, foreground(p, seed).round() as u32);
                        assert_eq!(c.soil_depth, 30);
                    }
                }
            }
            for span in REAR_WALL.windows(2) {
                for i in 0..100 {
                    let p = span[0].lerp(span[1], i as f32 / 100.);
                    assert_eq!(column(p, seed).soil_depth, 30);
                    assert!(height_voxels(p, seed) < 190);
                }
            }
        }
    }

    #[test]
    fn exposed_mountain_faces_and_soil_shoulders_are_real_atlas_materials() {
        assert_eq!(column(Vec2::new(0.43, 0.065), 7).soil_depth, 0);
        assert_eq!(column(Vec2::new(0.43, 0.16), 7).soil_depth, 30);
        let world = glam::UVec3::new(1024, 512, 1024);
        for p in [glam::UVec2::new(440, 67), glam::UVec2::new(440, 164)] {
            let c = column(p.as_vec2() / 1024., 7);
            let offset = glam::UVec3::new(p.x, c.height, p.y);
            let bytes =
                crate::garden_layout::terrain_chunk(offset, glam::UVec3::new(1, 2, 1), world, 7);
            assert_eq!(bytes, vec![if c.soil_depth == 0 { 7 } else { 2 }, 0]);
        }
    }
}
