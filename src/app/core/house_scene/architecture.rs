//! Fine-scale, editable half-timber cottage recipe. Dimensions are terrain voxels,
//! not Minecraft blocks (256 voxels/world unit). Material identity stays canonical.
use crate::{
    app::world_edits::{VoxelEdit, WorldEditTransaction},
    builder::{
        VOXEL_TYPE_DIRT, VOXEL_TYPE_EMISSIVE, VOXEL_TYPE_EMPTY, VOXEL_TYPE_GLASS, VOXEL_TYPE_IVY,
        VOXEL_TYPE_LIMESTONE, VOXEL_TYPE_OAK_WOOD, VOXEL_TYPE_ROCK, VOXEL_TYPE_TERRACOTTA,
    },
    geom::{build_bvh, Cuboid, RoundCone},
};
use anyhow::Result;
use glam::{Quat, Vec3};

// Keep the cottage in the garden interior, not beside the finite terrain edge.
pub(super) const ORIGIN: Vec3 = Vec3::new(320., 0., 240.);
pub(super) const MIN_X: f32 = 88.;
pub(super) const MAX_X: f32 = 272.;
pub(super) const MIN_Z: f32 = 214.;
pub(super) const FRONT_Z: f32 = 354.;
pub(super) const DOOR_X: f32 = 136.;
pub(super) const FLOOR_HEIGHT: f32 = 64.;
const WALL_TOP: f32 = FLOOR_HEIGHT * 2.;
const RIDGE_Z: f32 = 280.;
const ROOF_HALF_DEPTH: f32 = 94.;
const PANE_THICKNESS: f32 = 2.;

#[derive(Clone, Copy, Debug)]
pub(super) struct Planter {
    pub min_x: u32,
    pub max_x: u32,
    pub min_z: u32,
    pub max_z: u32,
    /// Soil top relative to the finished ground floor.
    pub top: f32,
}

// The same bounds author the actual containers and the ordinary plant contacts.
pub(super) const PLANTERS: [Planter; 6] = [
    Planter {
        min_x: 204,
        max_x: 250,
        min_z: 362,
        max_z: 372,
        top: 16.,
    },
    Planter {
        min_x: 116,
        max_x: 154,
        min_z: 361,
        max_z: 371,
        top: 76.,
    },
    Planter {
        min_x: 239,
        max_x: 265,
        min_z: 366,
        max_z: 376,
        top: 77.,
    },
    Planter {
        min_x: 96,
        max_x: 124,
        min_z: 348,
        max_z: 359,
        top: 151.,
    },
    Planter {
        min_x: 94,
        max_x: 106,
        min_z: 362,
        max_z: 374,
        top: 7.,
    },
    Planter {
        min_x: 263,
        max_x: 277,
        min_z: 389,
        max_z: 401,
        top: 7.,
    },
];

#[derive(Clone, Copy)]
pub(super) struct Window {
    pub x: f32,
    pub bottom: f32,
    pub width: f32,
    pub height: f32,
    pub z: f32,
}

pub(super) const WINDOWS: [Window; 4] = [
    Window {
        x: 226.,
        bottom: 23.,
        width: 48.,
        height: 29.,
        z: FRONT_Z,
    },
    Window {
        x: 135.,
        bottom: 88.,
        width: 30.,
        height: 30.,
        z: FRONT_Z,
    },
    Window {
        x: 219.,
        bottom: 78.,
        width: 36.,
        height: 40.,
        z: FRONT_Z,
    },
    Window {
        x: 222.,
        bottom: 151.,
        width: 28.,
        height: 22.,
        z: 343.,
    },
];

fn box_at(min: Vec3, max: Vec3) -> Cuboid {
    Cuboid::from_min_max(min, max)
}

fn cuboids(cuboids: Vec<Cuboid>, voxel_type: u32) -> Result<VoxelEdit> {
    let bounds = cuboids.iter().map(Cuboid::aabb).collect::<Vec<_>>();
    let indices = (0..cuboids.len() as u32).collect::<Vec<_>>();
    Ok(VoxelEdit::StampCuboids {
        bvh_nodes: build_bvh(&bounds, &indices).map_err(anyhow::Error::msg)?,
        cuboids,
        voxel_type,
        atlas_state_write: Default::default(),
    })
}

fn round_cones(round_cones: Vec<RoundCone>, voxel_type: u32) -> Result<VoxelEdit> {
    let bounds = round_cones.iter().map(RoundCone::aabb).collect::<Vec<_>>();
    let indices = (0..round_cones.len() as u32).collect::<Vec<_>>();
    Ok(VoxelEdit::StampRoundCones {
        bvh_nodes: build_bvh(&bounds, &indices).map_err(anyhow::Error::msg)?,
        round_cones,
        voxel_type,
    })
}

fn beam(a: Vec3, b: Vec3, radius: f32) -> Cuboid {
    Cuboid::new_oriented(
        (a + b) * 0.5,
        Vec3::new(radius, a.distance(b) * 0.5, radius),
        Quat::from_rotation_arc(Vec3::Y, (b - a).normalize()),
    )
}

fn roof_height(z: f32) -> f32 {
    WALL_TOP + 62. * (1. - ((z - RIDGE_Z).abs() / ROOF_HALF_DEPTH).powf(1.1))
}

/// Build in local height coordinates, then translate all primitives exactly once.
/// Ordered stamps keep openings, dedicated Glass, soil and light cores authoritative.
pub(super) fn plan(base: f32, ground_min: f32, approach_y: f32) -> Result<WorldEditTransaction> {
    let mut edits = Vec::new();
    let mut plaster = vec![box_at(
        Vec3::new(MIN_X, ground_min - base - 3., MIN_Z),
        Vec3::new(MAX_X, WALL_TOP, FRONT_Z),
    )];
    // Closed side gables follow the roof, including the shallow curved eaves.
    for i in 0..35 {
        let z = MIN_Z + i as f32 * 4.;
        for x in [MIN_X, MAX_X - 8.] {
            plaster.push(box_at(
                Vec3::new(x, WALL_TOP, z),
                Vec3::new(x + 8., roof_height(z).min(roof_height(z + 4.)), z + 4.),
            ));
        }
    }
    for z in [MIN_Z, FRONT_Z - 8.] {
        plaster.push(box_at(
            Vec3::new(MIN_X, WALL_TOP, z),
            Vec3::new(MAX_X, roof_height(z).min(roof_height(z + 8.)), z + 8.),
        ));
    }
    edits.push(cuboids(plaster, VOXEL_TYPE_LIMESTONE)?);
    let mut cuts = vec![box_at(
        Vec3::new(MIN_X + 8., 0., MIN_Z + 8.),
        Vec3::new(MAX_X - 8., WALL_TOP + 1., FRONT_Z - 8.),
    )];
    // Generous, flat-threshold entrance; open door leaves are added inside the jambs.
    cuts.push(box_at(
        Vec3::new(DOOR_X - 22., 0., 345.),
        Vec3::new(DOOR_X + 22., 51., 362.),
    ));
    edits.push(cuboids(cuts, VOXEL_TYPE_EMPTY)?);

    let mut wood = Vec::new();
    let mut rounded_wood = Vec::new();
    let mut stone = Vec::new();
    let mut tiles = Vec::new();
    // Raised stone plinth, post feet and restrained staggered corner quoins.
    for x in [MIN_X + 3., 180., MAX_X - 3.] {
        for z in [MIN_Z + 3., FRONT_Z] {
            stone.push(box_at(
                Vec3::new(x - 7., ground_min - base - 4., z - 7.),
                Vec3::new(x + 7., 4., z + 7.),
            ));
            wood.push(box_at(
                Vec3::new(x - 3., 4., z - 3.),
                Vec3::new(x + 3., WALL_TOP + 3., z + 3.),
            ));
            for y in [FLOOR_HEIGHT, WALL_TOP] {
                wood.push(box_at(
                    Vec3::new(x - 5., y - 5., z - 5.),
                    Vec3::new(x + 5., y + 3., z + 5.),
                ));
            }
        }
    }
    for row in 0..8 {
        for x in [MIN_X, MAX_X - 6.] {
            stone.push(box_at(
                Vec3::new(x, row as f32 * 7., 352.),
                Vec3::new(x + 6., row as f32 * 7. + 6., 356.),
            ));
        }
    }
    // Floors have fine board joints, not a coarse grid. Leave a real stairwell.
    for y in [0., FLOOR_HEIGHT] {
        wood.push(box_at(
            Vec3::new(96., y - 4., 222.),
            Vec3::new(264., y - 1., 354.),
        ));
        for i in 0..21 {
            let x = 96. + i as f32 * 8.;
            wood.push(box_at(
                Vec3::new(x, y - 1., 222.),
                Vec3::new(x + 7.4, y, 354.),
            ));
        }
    }
    // Front and side floor bands, roof cornices and diagonally braced half timber.
    for y in [3., FLOOR_HEIGHT, WALL_TOP] {
        for z in [MIN_Z, FRONT_Z + 1.] {
            let spans = if y == 3. && z > FRONT_Z {
                vec![(MIN_X - 5., DOOR_X - 26.), (DOOR_X + 26., MAX_X + 5.)]
            } else {
                vec![(MIN_X - 5., MAX_X + 5.)]
            };
            for (left, right) in spans {
                wood.push(box_at(
                    Vec3::new(left, y - 3., z - 3.),
                    Vec3::new(right, y + 2., z + 3.),
                ));
            }
        }
        for x in [MIN_X, MAX_X] {
            wood.push(box_at(
                Vec3::new(x - 3., y - 3., MIN_Z),
                Vec3::new(x + 3., y + 2., FRONT_Z),
            ));
        }
    }
    for x in [94., 266.] {
        wood.push(beam(
            Vec3::new(x, 113., 357.),
            Vec3::new(x + if x < 180. { 15. } else { -15. }, 127., 357.),
            2.,
        ));
    }
    for y in [4., 68.] {
        wood.push(beam(
            Vec3::new(275., y, 222.),
            Vec3::new(275., y + 52., 245.),
            2.,
        ));
        wood.push(beam(
            Vec3::new(275., y + 52., 293.),
            Vec3::new(275., y, 343.),
            2.,
        ));
    }
    // Continuous roof substrate and individually overlapping terracotta tiles.
    // A swept, gently flared profile replaces the reference's big stair-step roof.
    for side in [-1., 1.] {
        for row in 0..11 {
            let z0 = RIDGE_Z + side * row as f32 * ROOF_HALF_DEPTH / 11.;
            let z1 = RIDGE_Z + side * (row + 1) as f32 * ROOF_HALF_DEPTH / 11.;
            let a = Vec3::new(180., roof_height(z0), z0);
            let b = Vec3::new(180., roof_height(z1), z1);
            let rotation = Quat::from_rotation_arc(Vec3::Z, (b - a).normalize());
            let normal = (rotation * Vec3::Y) * side;
            wood.push(Cuboid::new_oriented(
                (a + b) * 0.5,
                Vec3::new(104., 2., a.distance(b) * 0.5 + 0.5),
                rotation,
            ));
            for col in 0..21 {
                let x0 = 76. + col as f32 * 10.;
                let x1 = (x0 + 9.4_f32).min(284.);
                let center =
                    Vec3::new((x0 + x1) * 0.5, (a.y + b.y) * 0.5, (z0 + z1) * 0.5) + normal * 3.;
                tiles.push(Cuboid::new_oriented(
                    center,
                    Vec3::new((x1 - x0) * 0.5, 1.6, a.distance(b) * 0.5 + 1.),
                    rotation,
                ));
            }
            for x in [78., 128., 180., 232., 282.] {
                rounded_wood.push(RoundCone::new(
                    2.,
                    Vec3::new(x, a.y - 2., z0),
                    2.,
                    Vec3::new(x, b.y - 2., z1),
                ));
            }
        }
    }
    // Main balcony: projecting deck, corbels, turned balusters and rounded handrail.
    wood.push(box_at(
        Vec3::new(174., 60., 353.),
        Vec3::new(278., 64., 384.),
    ));
    for x in [180., 223., 269.] {
        wood.push(beam(Vec3::new(x, 49., 355.), Vec3::new(x, 61., 379.), 2.));
    }
    add_railing(
        &mut wood,
        &mut rounded_wood,
        Vec3::new(178., 65., 382.),
        Vec3::new(274., 65., 382.),
        22.,
        12.,
    );
    for x in [178., 274.] {
        add_railing(
            &mut wood,
            &mut rounded_wood,
            Vec3::new(x, 65., 356.),
            Vec3::new(x, 65., 382.),
            22.,
            13.,
        );
    }
    // Window shutters have thin inset panels and actual louvres.
    for x in [111., 159.] {
        wood.push(box_at(
            Vec3::new(x - 5., 86., 356.),
            Vec3::new(x + 5., 119., 359.),
        ));
        for y in (90..117).step_by(5) {
            rounded_wood.push(RoundCone::new(
                1.,
                Vec3::new(x - 4., y as f32, 360.),
                1.,
                Vec3::new(x + 4., y as f32, 360.),
            ));
        }
    }
    // Door leaves stand open into the room, keeping the entire 44-voxel opening free.
    for x in [111., 159.] {
        wood.push(box_at(
            Vec3::new(x - 1.5, 0., 328.),
            Vec3::new(x + 1.5, 50., 354.),
        ));
    }
    for x in [111., 161.] {
        wood.push(box_at(
            Vec3::new(x - 2., 0., 352.),
            Vec3::new(x + 2., 53., 359.),
        ));
    }
    wood.push(box_at(
        Vec3::new(109., 51., 352.),
        Vec3::new(165., 55., 360.),
    ));
    // Interior stairs and landing make the second floor genuinely reachable.
    edits.push(cuboids(wood, VOXEL_TYPE_OAK_WOOD)?);
    edits.push(cuboids(
        vec![box_at(
            Vec3::new(114., 57., 228.),
            Vec3::new(248., 68., 264.),
        )],
        VOXEL_TYPE_EMPTY,
    )?);
    let mut stairs = Vec::new();
    for i in 0..18 {
        stairs.push(box_at(
            Vec3::new(114. + i as f32 * 7.5, 0., 232.),
            Vec3::new(
                114. + (i + 1) as f32 * 7.5,
                (i + 1) as f32 * FLOOR_HEIGHT / 18.,
                260.,
            ),
        ));
    }
    // A small dining table and benches stay to the right of the entrance route.
    stairs.push(box_at(
        Vec3::new(201., 18., 284.),
        Vec3::new(251., 21., 312.),
    ));
    for x in [204., 245.] {
        for z in [287., 306.] {
            stairs.push(box_at(Vec3::new(x, 0., z), Vec3::new(x + 3., 18., z + 3.)));
        }
    }
    for z in [274., 318.] {
        stairs.push(box_at(
            Vec3::new(202., 10., z),
            Vec3::new(250., 13., z + 7.),
        ));
        for x in [205., 244.] {
            stairs.push(box_at(
                Vec3::new(x, 0., z + 1.),
                Vec3::new(x + 3., 10., z + 6.),
            ));
        }
    }
    edits.push(cuboids(stairs, VOXEL_TYPE_OAK_WOOD)?);
    edits.push(round_cones(rounded_wood, VOXEL_TYPE_OAK_WOOD)?);
    edits.push(cuboids(stone, VOXEL_TYPE_LIMESTONE)?);
    edits.push(cuboids(tiles, VOXEL_TYPE_TERRACOTTA)?);
    edits.push(round_cones(
        vec![RoundCone::new(
            3.,
            Vec3::new(76., 193., RIDGE_Z),
            3.,
            Vec3::new(284., 193., RIDGE_Z),
        )],
        VOXEL_TYPE_TERRACOTTA,
    )?);

    add_dormer(&mut edits)?;
    add_windows(&mut edits)?;
    add_planters(&mut edits, ground_min - base)?;
    add_lanterns(&mut edits)?;
    add_ivy(&mut edits)?;
    // Brick chimney, with an open dark flue instead of a solid block on the ridge.
    let mut chimney = Vec::new();
    for row in 0..6 {
        chimney.push(box_at(
            Vec3::new(122., 180. + row as f32 * 6., 248.),
            Vec3::new(138., 185. + row as f32 * 6., 264.),
        ));
    }
    edits.push(cuboids(chimney, VOXEL_TYPE_TERRACOTTA)?);
    edits.push(cuboids(
        vec![box_at(
            Vec3::new(119., 214., 245.),
            Vec3::new(141., 218., 267.),
        )],
        VOXEL_TYPE_LIMESTONE,
    )?);
    edits.push(cuboids(
        vec![box_at(
            Vec3::new(126., 207., 252.),
            Vec3::new(134., 220., 260.),
        )],
        VOXEL_TYPE_EMPTY,
    )?);
    // Steps adapt to the actual facade ground; no floating threshold on sloped terrain.
    let count = ((base - approach_y) / 3.).ceil().clamp(1., 20.) as u32;
    let steps = (0..count)
        .map(|i| {
            let z = FRONT_Z + i as f32 * 6.;
            box_at(
                Vec3::new(DOOR_X - 27., ground_min - base - 3., z),
                Vec3::new(DOOR_X + 27., -(i as f32) * 3., z + 6.),
            )
        })
        .collect();
    edits.push(cuboids(steps, VOXEL_TYPE_TERRACOTTA)?);
    for edit in &mut edits {
        match edit {
            VoxelEdit::StampCuboids {
                cuboids: shapes,
                bvh_nodes,
                ..
            } => {
                for shape in shapes.iter_mut() {
                    shape.transform(ORIGIN + Vec3::Y * base);
                }
                *bvh_nodes = build_bvh(
                    &shapes.iter().map(Cuboid::aabb).collect::<Vec<_>>(),
                    &(0..shapes.len() as u32).collect::<Vec<_>>(),
                )
                .map_err(anyhow::Error::msg)?;
            }
            VoxelEdit::StampRoundCones {
                round_cones: shapes,
                bvh_nodes,
                ..
            } => {
                for shape in shapes.iter_mut() {
                    shape.transform(ORIGIN + Vec3::Y * base);
                }
                *bvh_nodes = build_bvh(
                    &shapes.iter().map(RoundCone::aabb).collect::<Vec<_>>(),
                    &(0..shapes.len() as u32).collect::<Vec<_>>(),
                )
                .map_err(anyhow::Error::msg)?;
            }
            _ => unreachable!("cottage recipe only authors cuboids and round cones"),
        }
    }
    Ok(WorldEditTransaction::during_loading(edits))
}

fn add_railing(
    wood: &mut Vec<Cuboid>,
    rounds: &mut Vec<RoundCone>,
    a: Vec3,
    b: Vec3,
    height: f32,
    spacing: f32,
) {
    wood.push(beam(a, b, 1.4));
    rounds.push(RoundCone::new(
        1.8,
        a + Vec3::Y * height,
        1.8,
        b + Vec3::Y * height,
    ));
    let count = (a.distance(b) / spacing).round() as u32;
    for i in 0..=count {
        let p = a.lerp(b, i as f32 / count as f32);
        rounds.push(RoundCone::new(1.6, p, 0.9, p + Vec3::Y * (height * 0.5)));
        rounds.push(RoundCone::new(
            0.9,
            p + Vec3::Y * (height * 0.5),
            1.6,
            p + Vec3::Y * height,
        ));
    }
}

fn add_dormer(edits: &mut Vec<VoxelEdit>) -> Result<()> {
    edits.push(cuboids(
        vec![box_at(
            Vec3::new(198., 143., 310.),
            Vec3::new(246., 184., 351.),
        )],
        VOXEL_TYPE_EMPTY,
    )?);
    let mut walls = vec![
        box_at(Vec3::new(198., 143., 310.), Vec3::new(203., 174., 343.)),
        box_at(Vec3::new(241., 143., 310.), Vec3::new(246., 174., 343.)),
        box_at(Vec3::new(198., 143., 338.), Vec3::new(246., 174., 343.)),
    ];
    for i in 0..14 {
        let half = 24. * (1. - i as f32 / 14.);
        walls.push(box_at(
            Vec3::new(222. - half, 174. + i as f32, 338.),
            Vec3::new(222. + half, 175. + i as f32, 343.),
        ));
    }
    edits.push(cuboids(walls, VOXEL_TYPE_LIMESTONE)?);
    let mut wood = vec![box_at(
        Vec3::new(196., 140., 340.),
        Vec3::new(248., 144., 360.),
    )];
    let mut rails = Vec::new();
    add_railing(
        &mut wood,
        &mut rails,
        Vec3::new(199., 144., 358.),
        Vec3::new(245., 144., 358.),
        12.,
        11.5,
    );
    let mut tiles = Vec::new();
    for side in [-1., 1.] {
        let a = Vec3::new(222., 189., 324.);
        let b = Vec3::new(222. + side * 29., 172., 324.);
        let rotation = Quat::from_rotation_z(-side * (17_f32 / 29.).atan());
        let center = (a + b) * 0.5;
        wood.push(Cuboid::new_oriented(
            center,
            Vec3::new(a.distance(b) * 0.5, 2., 25.),
            rotation,
        ));
        tiles.push(Cuboid::new_oriented(
            center + Vec3::Y * 2.5,
            Vec3::new(a.distance(b) * 0.5, 1.5, 24.),
            rotation,
        ));
        wood.push(beam(
            Vec3::new(a.x, a.y, 350.),
            Vec3::new(b.x, b.y, 350.),
            2.,
        ));
    }
    edits.push(cuboids(wood, VOXEL_TYPE_OAK_WOOD)?);
    edits.push(round_cones(rails, VOXEL_TYPE_OAK_WOOD)?);
    edits.push(cuboids(tiles, VOXEL_TYPE_TERRACOTTA)?);
    Ok(())
}

fn add_windows(edits: &mut Vec<VoxelEdit>) -> Result<()> {
    let mut cuts = Vec::new();
    let mut glass = Vec::new();
    let mut frames = Vec::new();
    for w in WINDOWS {
        let left = w.x - w.width * 0.5;
        let right = w.x + w.width * 0.5;
        let top = w.bottom + w.height;
        cuts.push(box_at(
            Vec3::new(left, w.bottom, w.z - 10.),
            Vec3::new(right, top, w.z + 2.),
        ));
        glass.push(box_at(
            Vec3::new(left, w.bottom, w.z - PANE_THICKNESS),
            Vec3::new(right, top, w.z),
        ));
        for x in [left - 1., right + 1.] {
            frames.push(box_at(
                Vec3::new(x - 1., w.bottom - 2., w.z),
                Vec3::new(x + 1., top + 2., w.z + 4.),
            ));
        }
        for y in [w.bottom - 1., top + 1.] {
            frames.push(box_at(
                Vec3::new(left - 3., y - 1., w.z),
                Vec3::new(right + 3., y + 1., w.z + 4.),
            ));
        }
        frames.push(box_at(
            Vec3::new(w.x - 0.7, w.bottom, w.z),
            Vec3::new(w.x + 0.7, top, w.z + 2.),
        ));
        frames.push(box_at(
            Vec3::new(left, w.bottom + w.height * 0.52, w.z),
            Vec3::new(right, w.bottom + w.height * 0.52 + 1.3, w.z + 2.),
        ));
        frames.push(box_at(
            Vec3::new(left - 4., w.bottom - 4., w.z - 1.),
            Vec3::new(right + 4., w.bottom - 2., w.z + 7.),
        ));
    }
    for floor in [0., FLOOR_HEIGHT] {
        cuts.push(box_at(
            Vec3::new(262., floor + 23., 254.),
            Vec3::new(275., floor + 49., 286.),
        ));
        glass.push(box_at(
            Vec3::new(MAX_X - PANE_THICKNESS, floor + 23., 254.),
            Vec3::new(MAX_X, floor + 49., 286.),
        ));
        for z in [253., 270., 287.] {
            frames.push(box_at(
                Vec3::new(MAX_X, floor + 21., z - 1.),
                Vec3::new(MAX_X + 3., floor + 51., z + 1.),
            ));
        }
        for y in [22., 36., 50.] {
            frames.push(box_at(
                Vec3::new(MAX_X, floor + y - 1., 252.),
                Vec3::new(MAX_X + 4., floor + y + 1., 288.),
            ));
        }
    }
    edits.push(cuboids(cuts, VOXEL_TYPE_EMPTY)?);
    edits.push(cuboids(glass, VOXEL_TYPE_GLASS)?);
    edits.push(cuboids(frames, VOXEL_TYPE_OAK_WOOD)?);
    Ok(())
}

fn add_planters(edits: &mut Vec<VoxelEdit>, ground_min: f32) -> Result<()> {
    let mut boxes = Vec::new();
    let mut soil = Vec::new();
    for (i, p) in PLANTERS.into_iter().enumerate() {
        let x0 = p.min_x as f32;
        let x1 = p.max_x as f32;
        let z0 = p.min_z as f32;
        let z1 = p.max_z as f32;
        // Ground pots have their own feet down to natural terrain, not floating boxes.
        let bottom = if i >= 4 { ground_min - 2. } else { p.top - 7. };
        boxes.push(box_at(
            Vec3::new(x0 - 2., bottom, z0 - 2.),
            Vec3::new(x1 + 2., p.top, z1 + 2.),
        ));
        for x in [x0 - 1., x1 + 1.] {
            boxes.push(box_at(
                Vec3::new(x - 1., p.top - 8., z0 - 3.),
                Vec3::new(x + 1., p.top + 1., z1 + 3.),
            ));
        }
        soil.push(box_at(
            Vec3::new(x0, p.top - 4., z0),
            Vec3::new(x1, p.top, z1),
        ));
    }
    edits.push(cuboids(boxes, VOXEL_TYPE_OAK_WOOD)?);
    edits.push(cuboids(soil, VOXEL_TYPE_DIRT)?);
    Ok(())
}

fn add_lanterns(edits: &mut Vec<VoxelEdit>) -> Result<()> {
    let mut metal = Vec::new();
    let mut cores = Vec::new();
    for (index, (x, y, z)) in [(175., 45., 366.), (195., 111., 366.), (226., 46., 286.)]
        .into_iter()
        .enumerate()
    {
        if index < 2 {
            metal.push(box_at(
                Vec3::new(x - 1., y + 7., z - 12.),
                Vec3::new(x + 1., y + 10., z + 1.),
            ));
        } else {
            // The dining pendant is physically suspended from the upper floor.
            metal.push(box_at(
                Vec3::new(x - 0.8, y + 4., z - 0.8),
                Vec3::new(x + 0.8, FLOOR_HEIGHT - 3., z + 0.8),
            ));
        }
        metal.push(box_at(
            Vec3::new(x - 0.8, y + 4., z - 0.8),
            Vec3::new(x + 0.8, y + 9., z + 0.8),
        ));
        for h in [-4., 3.] {
            metal.push(box_at(
                Vec3::new(x - 3.5, y + h, z - 3.5),
                Vec3::new(x + 3.5, y + h + 1., z + 3.5),
            ));
        }
        for dx in [-2.5, 2.5] {
            for dz in [-2.5, 2.5] {
                metal.push(box_at(
                    Vec3::new(x + dx - 0.5, y - 3., z + dz - 0.5),
                    Vec3::new(x + dx + 0.5, y + 3., z + dz + 0.5),
                ));
            }
        }
        cores.push(box_at(
            Vec3::new(x - 1.5, y - 3., z - 1.5),
            Vec3::new(x + 1.5, y + 3., z + 1.5),
        ));
    }
    edits.push(cuboids(metal, VOXEL_TYPE_ROCK)?);
    edits.push(cuboids(cores, VOXEL_TYPE_EMISSIVE)?);
    Ok(())
}

fn add_ivy(edits: &mut Vec<VoxelEdit>) -> Result<()> {
    let mut stems = Vec::new();
    let mut leaves = Vec::new();
    // Foliage only: all blooms come from the existing living-flower catalog.
    for (root_x, root_y, root_z, length) in [
        (93., 8., 359., 117.),
        (103., 150., 362., -57.),
        (267., 79., 379., -35.),
    ] {
        for branch in 0..2 {
            let mut previous = Vec3::new(root_x, root_y, root_z);
            for i in 1..=24 {
                let t = i as f32 / 24.;
                let center = Vec3::new(
                    root_x + (t * 12. + branch as f32 * 2.).sin() * 3. + branch as f32 * 3.,
                    root_y + t * length,
                    root_z + t * 2.,
                );
                stems.push(RoundCone::new(0.65, previous, 0.65, center));
                for side in [-1., 1.] {
                    leaves.push(Cuboid::new_oriented(
                        center + Vec3::new(side * 2.5, 0., 0.8),
                        Vec3::new(1.4, 2.8, 0.7),
                        Quat::from_rotation_z(side * 0.65),
                    ));
                }
                previous = center;
            }
        }
    }
    edits.push(round_cones(stems, VOXEL_TYPE_OAK_WOOD)?);
    edits.push(cuboids(leaves, VOXEL_TYPE_IVY)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Analytic oracle uses the actual ordered primitives, including oriented roof
    // pieces and round balusters, rather than their conservative bounding boxes.
    fn material(plan: &WorldEditTransaction, p: Vec3) -> u32 {
        let p = p + ORIGIN;
        let mut result = VOXEL_TYPE_EMPTY;
        for edit in plan.voxel_edits() {
            let (inside, fill) = match edit {
                VoxelEdit::StampCuboids {
                    cuboids,
                    voxel_type,
                    ..
                } => (
                    cuboids.iter().any(|s| {
                        (s.rotation().conjugate() * (p - s.center()))
                            .abs()
                            .cmple(s.half_size())
                            .all()
                    }),
                    *voxel_type,
                ),
                VoxelEdit::StampRoundCones {
                    round_cones,
                    voxel_type,
                    ..
                } => (
                    round_cones.iter().any(|s| s.signed_distance(p) <= 0.),
                    *voxel_type,
                ),
                _ => panic!("unexpected primitive"),
            };
            if inside {
                result = fill;
            }
        }
        result
    }

    #[test]
    fn entrance_room_and_upper_floor_are_real_connected_spaces() {
        for base in [105., 132.] {
            let p = plan(base, base - 16., base - 7.).unwrap();
            for z in (270..391).step_by(3) {
                for dx in [-6., 0., 6.] {
                    for y in [4., 20., 36.] {
                        assert_eq!(
                            material(&p, Vec3::new(DOOR_X + dx, base + y, z as f32)),
                            VOXEL_TYPE_EMPTY,
                            "blocked entrance at {dx}, {y}, {z}"
                        );
                    }
                }
            }
            assert_eq!(
                material(&p, Vec3::new(136., base - 2., 320.)),
                VOXEL_TYPE_OAK_WOOD
            );
            assert_eq!(
                material(&p, Vec3::new(219., base + 62., 320.)),
                VOXEL_TYPE_OAK_WOOD
            );
            assert_eq!(
                material(&p, Vec3::new(219., base + 84., 320.)),
                VOXEL_TYPE_EMPTY
            );
            for i in 0..18 {
                let x = 117. + i as f32 * 7.5;
                let top = (i + 1) as f32 * FLOOR_HEIGHT / 18.;
                assert_eq!(
                    material(&p, Vec3::new(x, base + top - 0.5, 246.)),
                    VOXEL_TYPE_OAK_WOOD
                );
                assert_eq!(
                    material(&p, Vec3::new(x, base + top + 36., 246.)),
                    VOXEL_TYPE_EMPTY
                );
            }
        }
    }

    #[test]
    fn six_windows_are_thin_dedicated_glass_in_openings_not_opaque_backing() {
        let base = 116.;
        let p = plan(base, 96., 108.).unwrap();
        for w in WINDOWS {
            let point = Vec3::new(
                w.x - w.width * 0.23,
                base + w.bottom + w.height * 0.7,
                w.z - 1.,
            );
            assert_eq!(material(&p, point), VOXEL_TYPE_GLASS);
            assert_eq!(material(&p, point - Vec3::Z * 4.), VOXEL_TYPE_EMPTY);
        }
        for y in [42., 106.] {
            assert_eq!(
                material(&p, Vec3::new(271., base + y, 261.)),
                VOXEL_TYPE_GLASS
            );
            assert_eq!(
                material(&p, Vec3::new(267., base + y, 261.)),
                VOXEL_TYPE_EMPTY
            );
        }
        for mode in [
            crate::voxel_material::VoxelMaterialMode::Standard,
            crate::voxel_material::VoxelMaterialMode::GlassExperiment,
        ] {
            let glass = crate::voxel_material::material_for(VOXEL_TYPE_GLASS, mode);
            assert_eq!(
                glass.surface_class,
                crate::voxel_material::VoxelSurfaceClass::Dielectric
            );
            assert!(glass.collision_solid && !glass.blocks_ddgi_visibility);
        }
    }

    #[test]
    fn containers_have_exposed_soil_and_balcony_has_open_baluster_gaps() {
        let base = 116.;
        let p = plan(base, 96., 108.).unwrap();
        for planter in PLANTERS {
            for x in (planter.min_x + 2..planter.max_x - 1).step_by(4) {
                let point = Vec3::new(
                    x as f32 + 0.5,
                    base + planter.top - 0.5,
                    (planter.min_z + planter.max_z) as f32 * 0.5,
                );
                assert_eq!(material(&p, point), VOXEL_TYPE_DIRT);
                assert_eq!(material(&p, point + Vec3::Y * 2.), VOXEL_TYPE_EMPTY);
            }
        }
        assert_eq!(
            material(&p, Vec3::new(184., base + 76., 382.)),
            VOXEL_TYPE_EMPTY
        );
        assert_eq!(
            material(&p, Vec3::new(190., base + 76., 382.)),
            VOXEL_TYPE_OAK_WOOD
        );
        assert_eq!(
            material(&p, Vec3::new(184., base + 87., 382.)),
            VOXEL_TYPE_OAK_WOOD
        );
        assert_eq!(
            material(&p, Vec3::new(180., base + 193., RIDGE_Z)),
            VOXEL_TYPE_TERRACOTTA
        );
    }
}
