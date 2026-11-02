//! Small timber hall, stamped into ordinary editable terrain before publication.
//! Dimensions are native terrain voxels; the foundation samples the real substrate.
use crate::{
    app::{path_band::CellSolid, world_edits::VoxelEdit},
    builder::{ModelTriangleGpu, VOXEL_TYPE_ASPHALT, VOXEL_TYPE_LIMESTONE, VOXEL_TYPE_OAK_WOOD},
    garden_layout,
    geom::{build_bvh, Cuboid, RoundCone},
};
use anyhow::{ensure, Result};
use glam::{Quat, UVec2, UVec3, Vec2, Vec3};

fn placement(world: UVec3) -> (Vec3, f32) {
    let xz = garden_layout::TEMPLE_CENTER * Vec2::new(world.x as f32, world.z as f32);
    (
        Vec3::new(xz.x, 0., xz.y),
        world.x.min(world.z) as f32 / 1024.,
    )
}

pub(super) fn ground_region(world: UVec3) -> (UVec3, UVec3) {
    let (origin, scale) = placement(world);
    let min = origin + Vec3::new(-106., 0., -76.) * scale;
    let max = origin + Vec3::new(106., 0., 170.) * scale;
    let offset = min.floor().as_uvec3();
    let end = UVec3::new(max.x.ceil() as u32 + 1, world.y, max.z.ceil() as u32 + 1);
    (offset, end - offset)
}

fn box_at(min: Vec3, max: Vec3) -> Cuboid {
    Cuboid::from_min_max(min, max)
}

fn beam(a: Vec3, b: Vec3, radius: f32) -> Cuboid {
    Cuboid::new_oriented(
        (a + b) * 0.5,
        Vec3::new(radius, a.distance(b) * 0.5, radius),
        Quat::from_rotation_arc(Vec3::Y, (b - a).normalize()),
    )
}

fn cuboids(cuboids: Vec<Cuboid>, voxel_type: u32) -> Result<VoxelEdit> {
    let bounds: Vec<_> = cuboids.iter().map(Cuboid::aabb).collect();
    let indices: Vec<_> = (0..cuboids.len() as u32).collect();
    Ok(VoxelEdit::StampCuboids {
        bvh_nodes: build_bvh(&bounds, &indices).map_err(anyhow::Error::msg)?,
        cuboids,
        voxel_type,
        atlas_state_write: Default::default(),
    })
}

fn roof_height(p: Vec2) -> f32 {
    let slope = (p.y.abs() / 96.).max((p.x.abs() - 48.) / 72.).clamp(0., 1.);
    // A short ridge, four hips and restrained upturned corners, not a flat gable.
    70. + 42. * (1. - slope).powf(1.45)
        + 9. * (p.x / 120.).abs().powi(6) * (p.y / 96.).abs().powi(6)
}

pub(super) fn plan(
    world: UVec3,
    mut ground: impl FnMut(UVec2) -> Result<f32>,
) -> Result<Vec<VoxelEdit>> {
    let (mut origin, scale) = placement(world);
    let column = |x, z| {
        let p = origin + Vec3::new(x, 0., z) * scale;
        UVec2::new(p.x.floor() as u32, p.z.floor() as u32)
    };
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    // Exhaustive foundation coverage, not a center ray that could leave a corner floating.
    let min = column(-104., -74.);
    let max = column(104., 104.);
    for z in min.y..=max.y {
        for x in min.x..=max.x {
            let h = ground(UVec2::new(x, z))?;
            low = low.min(h);
            high = high.max(h);
        }
    }
    origin.y = high + 4. * scale;
    let bottom = (low - origin.y) / scale - 3.;
    let mut stone = vec![box_at(
        Vec3::new(-104., bottom, -74.),
        Vec3::new(104., 0., 104.),
    )];
    // Wide low treads follow the approach without flattening the surrounding hill.
    for i in 0..8 {
        let z = 104. + i as f32 * 8.;
        let p = origin + Vec3::new(0., 0., z + 8.) * scale;
        let h = ground(UVec2::new(p.x.floor() as u32, p.z.floor() as u32))?;
        let top = (-3. * (i + 1) as f32).max((h - origin.y) / scale + 1.);
        stone.push(box_at(
            Vec3::new(-30., bottom.min((h - origin.y) / scale - 3.), z),
            Vec3::new(30., top, z + 8.),
        ));
    }
    // Continuous backing keeps sub-voxel board joints from becoming holes to the sky.
    let mut wood = vec![box_at(Vec3::new(-80., 3., -56.), Vec3::new(80., 66., -54.))];
    // Floorboards, three enclosed sides, and a genuinely open central entrance.
    for x in (-80..80).step_by(8) {
        wood.push(box_at(
            Vec3::new(x as f32, 0., -56.),
            Vec3::new(x as f32 + 7.5, 3., 88.),
        ));
        wood.push(box_at(
            Vec3::new(x as f32, 3., -57.),
            Vec3::new(x as f32 + 7.5, 66., -53.),
        ));
    }
    for x in [-80., 76.] {
        wood.push(box_at(Vec3::new(x, 0., -56.), Vec3::new(x + 4., 66., 56.)));
    }
    for x in [-80., -27., 27., 80.] {
        for z in [-56., 56.] {
            stone.push(box_at(
                Vec3::new(x - 5., 0., z - 5.),
                Vec3::new(x + 5., 6., z + 5.),
            ));
            wood.push(beam(Vec3::new(x, 6., z), Vec3::new(x, 70., z), 3.));
            for (y, half) in [(59., 5.), (63., 8.), (67., 11.)] {
                wood.push(box_at(
                    Vec3::new(x - half, y, z - 5.),
                    Vec3::new(x + half, y + 3., z + 5.),
                ));
            }
        }
    }
    for x in [-80., 80.] {
        wood.push(beam(Vec3::new(x, 0., 88.), Vec3::new(x, 70., 88.), 3.));
    }
    for z in [-56., 56., 88.] {
        wood.push(beam(Vec3::new(-91., 65., z), Vec3::new(91., 65., z), 3.));
    }
    for x in [-80., -27., 27., 80.] {
        wood.push(beam(Vec3::new(x, 65., -66.), Vec3::new(x, 65., 96.), 3.));
    }
    // Latticed side bays flank the door; no Glass or invisible cutout is needed.
    for (left, right) in [(-77., -27.), (27., 77.)] {
        wood.push(box_at(Vec3::new(left, 3., 53.), Vec3::new(right, 23., 57.)));
        for y in [24., 41., 56.] {
            wood.push(box_at(
                Vec3::new(left, y, 53.),
                Vec3::new(right, y + 2., 57.),
            ));
        }
        for i in 0..8 {
            let x = left + (right - left) * i as f32 / 7.;
            wood.push(box_at(
                Vec3::new(x - 0.8, 24., 54.),
                Vec3::new(x + 0.8, 58., 56.),
            ));
        }
    }
    // A modest timber altar and back screen give the small hall an interior focus.
    wood.push(box_at(
        Vec3::new(-25., 22., -42.),
        Vec3::new(25., 25., -24.),
    ));
    for x in [-21., 21.] {
        wood.push(beam(Vec3::new(x, 3., -33.), Vec3::new(x, 22., -33.), 2.));
    }
    wood.push(box_at(
        Vec3::new(-16., 25., -46.),
        Vec3::new(16., 50., -43.),
    ));
    // Exposed rafters under the tiled hips.
    for x in (-108..=108).step_by(12) {
        for sign in [-1., 1.] {
            // Follow the curved underside. A single ridge-to-eave chord cuts
            // through a concave-up roof and exposes timber through the tiles.
            for z in (0..96).step_by(12) {
                let a = Vec2::new(x as f32, sign * z as f32);
                let b = Vec2::new(x as f32, sign * (z + 12) as f32);
                wood.push(beam(
                    Vec3::new(a.x, roof_height(a) - 6., a.y),
                    Vec3::new(b.x, roof_height(b) - 6., b.y),
                    1.8,
                ));
            }
        }
    }
    for cuboid in wood.iter_mut().chain(stone.iter_mut()) {
        cuboid.scale(Vec3::splat(scale));
        cuboid.transform(origin);
        ensure!(
            cuboid.min().cmpge(Vec3::ZERO).all() && cuboid.max().cmplt(world.as_vec3()).all(),
            "temple outside world"
        );
    }
    let mut edits = vec![
        cuboids(stone, VOXEL_TYPE_LIMESTONE)?,
        cuboids(wood, VOXEL_TYPE_OAK_WOOD)?,
    ];
    let mut roof = Vec::new();
    let mut rolls = Vec::new();
    for x in (-120..120).step_by(6) {
        for z in (-96..96).step_by(6) {
            let foot = [
                Vec2::new(x as f32, z as f32),
                Vec2::new(x as f32 + 6., z as f32),
                Vec2::new(x as f32 + 6., z as f32 + 6.),
                Vec2::new(x as f32, z as f32 + 6.),
            ];
            let heights = foot.map(roof_height);
            let mut solid = CellSolid::new(foot, heights, -2., 1., 1.);
            for p in &mut solid.positions {
                *p = (*p * scale + origin) / 256.;
            }
            roof.extend(solid.triangles().map(|[a, b, c]| ModelTriangleGpu {
                a: a.extend(0.).to_array(),
                b: b.extend(0.).to_array(),
                c: c.extend(0.).to_array(),
            }));
            let a = Vec3::new(x as f32, heights[0] + 1., z as f32) * scale + origin;
            let b = Vec3::new(x as f32, heights[3] + 1., z as f32 + 6.) * scale + origin;
            rolls.push(RoundCone::new(1.35 * scale, a, 1.35 * scale, b));
        }
    }
    // Short raised ridge keeps the roof legible from the arrival camera.
    rolls.push(RoundCone::new(
        3. * scale,
        origin + Vec3::new(-50., 115., 0.) * scale,
        3. * scale,
        origin + Vec3::new(50., 115., 0.) * scale,
    ));
    for batch in roof.chunks(1200) {
        edits.push(VoxelEdit::StampModel {
            triangles: batch.to_vec(),
            position: Vec3::ZERO,
            material: VOXEL_TYPE_ASPHALT,
        });
    }
    let bounds: Vec<_> = rolls.iter().map(RoundCone::aabb).collect();
    let indices: Vec<_> = (0..rolls.len() as u32).collect();
    edits.push(VoxelEdit::StampRoundCones {
        bvh_nodes: build_bvh(&bounds, &indices).map_err(anyhow::Error::msg)?,
        round_cones: rolls,
        voxel_type: VOXEL_TYPE_ASPHALT,
    });
    log::info!("[HILLSIDE_TEMPLE] timber_hall=true bays=3 open_door=true veranda=true curved_hip_roof=true stairs=8 foundation_y={:.2} natural_surface={low:.2}..{high:.2} editable=true", origin.y);
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hall_is_grounded_editable_and_has_an_open_entrance() {
        for world in [UVec3::new(512, 512, 512), UVec3::new(1024, 512, 1024)] {
            let ground = |p: UVec2| 130. + (p.x / 32 + p.y / 64) as f32;
            let plan = plan(world, |p| Ok(ground(p))).unwrap();
            let wood = plan
                .iter()
                .find_map(|edit| match edit {
                    VoxelEdit::StampCuboids {
                        cuboids,
                        voxel_type: VOXEL_TYPE_OAK_WOOD,
                        ..
                    } => Some(cuboids),
                    _ => None,
                })
                .unwrap();
            let (mut origin, scale) = placement(world);
            let max = (origin + Vec3::new(104., 0., 104.) * scale).floor();
            origin.y = ground(UVec2::new(max.x as u32, max.z as u32)) + 4. * scale;
            let inside = |point: Vec3| {
                wood.iter().any(|c| {
                    (c.rotation().conjugate() * (point - c.center()))
                        .abs()
                        .cmple(c.half_size())
                        .all()
                })
            };
            assert!(
                !inside(origin + Vec3::new(0., 25., 55.) * scale),
                "door must be open at player height"
            );
            assert!(
                inside(origin + Vec3::new(78., 25., 0.) * scale),
                "side wall must be solid"
            );
            for cuboid in wood {
                for x in [-1., 1.] {
                    for y in [-1., 1.] {
                        for z in [-1., 1.] {
                            let corner = cuboid.center()
                                + cuboid.rotation() * (cuboid.half_size() * Vec3::new(x, y, z));
                            let local = (corner - origin) / scale;
                            assert!(
                                local.y < roof_height(Vec2::new(local.x, local.z)) - 1.,
                                "timber must remain below the tile skin: {local:?}"
                            );
                        }
                    }
                }
            }
            assert!(plan.iter().any(|e| matches!(
                e,
                VoxelEdit::StampModel {
                    material: VOXEL_TYPE_ASPHALT,
                    ..
                }
            )));
        }
    }

    #[test]
    fn roof_has_a_short_ridge_sloped_hips_and_lifted_corners() {
        assert_eq!(roof_height(Vec2::ZERO), roof_height(Vec2::new(40., 0.)));
        assert!(roof_height(Vec2::ZERO) > roof_height(Vec2::new(0., 80.)) + 20.);
        assert!(roof_height(Vec2::new(120., 96.)) > roof_height(Vec2::new(0., 96.)) + 8.);
    }
}
