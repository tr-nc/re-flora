use super::planting::AuthoredFloraPlacementBatch;
use super::App;
use crate::app::world_edits::{VoxelEdit, WorldEditTransaction};
use crate::builder::{
    voxel_type_from_atlas_byte, PlainBuilder, VOXEL_TYPE_DIRT, VOXEL_TYPE_EMISSIVE,
    VOXEL_TYPE_EMPTY, VOXEL_TYPE_IVY, VOXEL_TYPE_LIMESTONE, VOXEL_TYPE_OAK_WOOD, VOXEL_TYPE_PETAL,
    VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND, VOXEL_TYPE_STUCCO,
};
use crate::flora::species::{EMBER_BLOOM_SPECIES_INDEX, LAVENDER_SPECIES_INDEX};
use crate::geom::{build_bvh, Cuboid, Sphere, Torus};
use anyhow::{Context, Result};
use glam::{Quat, UVec2, UVec3, Vec3};

// Terrain voxels: 256 voxels per world unit. All decorative solids go through the
// same loading transaction, visible publication, DDGI and collision paths as walls.
const HOUSE_MIN_X: f32 = 100.0;
const HOUSE_MAX_X: f32 = 252.0;
const HOUSE_MIN_Z: f32 = 214.0;
const HOUSE_MAX_Z: f32 = 354.0;
const CENTER_X: f32 = (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5;
const WALL_HEIGHT: f32 = 60.0;
const WALL_THICKNESS: f32 = 10.0;
const ROOF_RISE: f32 = 30.0;
const DOOR_RADIUS: f32 = 20.0;
const WINDOW_OFFSET: f32 = 48.0;
const WINDOW_HEIGHT: f32 = 32.0;
const WINDOW_RADIUS: f32 = 10.0;
const WINDOW_PANE_THICKNESS: f32 = 2.0;
const SURFACE_SAMPLE_OFFSET: UVec3 = UVec3::new(HOUSE_MIN_X as u32, 32, HOUSE_MIN_Z as u32);
const SURFACE_SAMPLE_DIM: UVec3 = UVec3::new(
    (HOUSE_MAX_X - HOUSE_MIN_X) as u32 + 1,
    224,
    (HOUSE_MAX_Z - HOUSE_MIN_Z) as u32 + 1,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SurfaceSampleReport {
    median_y: u32,
    min_y: u32,
    max_y: u32,
    facade_center_y: u32,
}

fn volume_index(local: UVec3, dim: UVec3) -> usize {
    ((local.z * dim.y + local.y) * dim.x + local.x) as usize
}

fn is_terrain_voxel(voxel_type: u8) -> bool {
    matches!(
        voxel_type as u32,
        VOXEL_TYPE_DIRT | VOXEL_TYPE_SAND | VOXEL_TYPE_ROCK
    )
}

fn sample_house_surface(plain_builder: &mut PlainBuilder) -> Result<SurfaceSampleReport> {
    let bytes = plain_builder
        .read_chunk_atlas_region(SURFACE_SAMPLE_OFFSET, SURFACE_SAMPLE_DIM)
        .context("read natural terrain below house")?;
    let mut heights = Vec::with_capacity((SURFACE_SAMPLE_DIM.x * SURFACE_SAMPLE_DIM.z) as usize);
    let mut facade_center_y = None;
    for z in 0..SURFACE_SAMPLE_DIM.z {
        for x in 0..SURFACE_SAMPLE_DIM.x {
            let surface = (0..SURFACE_SAMPLE_DIM.y).rev().find_map(|y| {
                let voxel_type = voxel_type_from_atlas_byte(
                    bytes[volume_index(UVec3::new(x, y, z), SURFACE_SAMPLE_DIM)],
                );
                is_terrain_voxel(voxel_type).then_some(SURFACE_SAMPLE_OFFSET.y + y)
            });
            let surface = surface.with_context(|| {
                format!(
                    "house footprint column has no terrain at x={} z={}",
                    SURFACE_SAMPLE_OFFSET.x + x,
                    SURFACE_SAMPLE_OFFSET.z + z
                )
            })?;
            if x == SURFACE_SAMPLE_DIM.x / 2 && z == SURFACE_SAMPLE_DIM.z - 1 {
                facade_center_y = Some(surface);
            }
            heights.push(surface);
        }
    }
    heights.sort_unstable();
    Ok(SurfaceSampleReport {
        median_y: heights[heights.len() / 2],
        min_y: *heights.first().context("empty house surface sample")?,
        max_y: *heights.last().context("empty house surface sample")?,
        facade_center_y: facade_center_y.context("missing facade center surface sample")?,
    })
}

fn stamp_cuboids(cuboids: Vec<Cuboid>, voxel_type: u32) -> Result<VoxelEdit> {
    let aabbs = cuboids.iter().map(Cuboid::aabb).collect::<Vec<_>>();
    let leaves = (0..cuboids.len() as u32).collect::<Vec<_>>();
    let bvh_nodes = build_bvh(&aabbs, &leaves).map_err(anyhow::Error::msg)?;
    Ok(VoxelEdit::StampCuboids {
        bvh_nodes,
        cuboids,
        voxel_type,
        atlas_state_write: Default::default(),
    })
}

fn stamp_toruses(toruses: Vec<Torus>, voxel_type: u32) -> Result<VoxelEdit> {
    let aabbs = toruses.iter().map(Torus::aabb).collect::<Vec<_>>();
    let leaves = (0..toruses.len() as u32).collect::<Vec<_>>();
    let bvh_nodes = build_bvh(&aabbs, &leaves).map_err(anyhow::Error::msg)?;
    Ok(VoxelEdit::StampToruses {
        bvh_nodes,
        toruses,
        voxel_type,
    })
}

fn box_at(min: Vec3, max: Vec3) -> Cuboid {
    Cuboid::from_min_max(min, max)
}

fn round_opening(
    center_x: f32,
    base_y: f32,
    radius: f32,
    tunnel_min_z: f32,
    tunnel_max_z: f32,
) -> Vec<Cuboid> {
    let center_y = base_y + radius;
    let mut slices = Vec::new();
    let mut slice_bottom = base_y;
    let top = base_y + radius * 2.0;
    while slice_bottom < top {
        let slice_top = (slice_bottom + 1.0).min(top);
        let sample_y = (slice_bottom + slice_top) * 0.5;
        let half_width = (radius.powi(2) - (sample_y - center_y).powi(2))
            .max(0.0)
            .sqrt();
        slices.push(box_at(
            Vec3::new(center_x - half_width, slice_bottom, tunnel_min_z),
            Vec3::new(center_x + half_width, slice_top, tunnel_max_z),
        ));
        slice_bottom = slice_top;
    }
    slices
}

fn house_base_y(surface: SurfaceSampleReport) -> f32 {
    surface.max_y as f32 + 1.0
}

fn roof_y(x: f32, base: f32) -> f32 {
    base + WALL_HEIGHT + ROOF_RISE * (1.0 - (x - CENTER_X).abs() / 76.0)
}

fn stamp_spheres(spheres: Vec<Sphere>, voxel_type: u32) -> Result<VoxelEdit> {
    let aabbs = spheres.iter().map(Sphere::aabb).collect::<Vec<_>>();
    let leaves = (0..spheres.len() as u32).collect::<Vec<_>>();
    Ok(VoxelEdit::StampSpheres {
        bvh_nodes: build_bvh(&aabbs, &leaves).map_err(anyhow::Error::msg)?,
        spheres,
        voxel_type,
    })
}

fn beam_between(a: Vec3, b: Vec3, radius: f32) -> Cuboid {
    Cuboid::new_oriented(
        (a + b) * 0.5,
        Vec3::new(radius, a.distance(b) * 0.5, radius),
        Quat::from_rotation_arc(Vec3::Y, (b - a).normalize()),
    )
}

fn house_plan(surface: SurfaceSampleReport) -> Result<WorldEditTransaction> {
    let base = house_base_y(surface);
    let mut edits = Vec::new();
    // Continuous backing and foundation make the recessed joints closed and solid.
    edits.push(stamp_cuboids(
        vec![box_at(
            Vec3::new(HOUSE_MIN_X, surface.min_y as f32 - 2.0, HOUSE_MIN_Z),
            Vec3::new(HOUSE_MAX_X, base + WALL_HEIGHT, HOUSE_MAX_Z),
        )],
        VOXEL_TYPE_STUCCO,
    )?);
    let mut stones = Vec::new();
    // Staggered, slightly irregular blocks; limestone is its own material, not a
    // recoloring of the world's Rock or the existing warm Stucco.
    for row in 0..10 {
        let y = base + row as f32 * 9.0;
        let offset = if row % 2 == 0 { 0.0 } else { -10.0 };
        for col in 0..9 {
            let x0 = (HOUSE_MIN_X + offset + col as f32 * 20.0).max(HOUSE_MIN_X);
            let x1 = (HOUSE_MIN_X + offset + (col + 1) as f32 * 20.0 - 1.0).min(HOUSE_MAX_X);
            if x1 <= x0 {
                continue;
            }
            let top = (y + 8.0).min(roof_y(x0, base).min(roof_y(x1, base)));
            if top <= y {
                continue;
            }
            for z in [HOUSE_MIN_Z - 1.0, HOUSE_MAX_Z - 2.0] {
                stones.push(box_at(Vec3::new(x0, y, z), Vec3::new(x1, top, z + 4.0)));
            }
        }
        if row >= 7 {
            continue;
        }
        for col in 0..8 {
            let z0 = (HOUSE_MIN_Z + offset + col as f32 * 20.0).max(HOUSE_MIN_Z);
            let z1 = (HOUSE_MIN_Z + offset + (col + 1) as f32 * 20.0 - 1.0).min(HOUSE_MAX_Z);
            if z1 <= z0 {
                continue;
            }
            for x in [HOUSE_MIN_X - 1.0, HOUSE_MAX_X - 2.0] {
                stones.push(box_at(
                    Vec3::new(x, y, z0),
                    Vec3::new(x + 4.0, (y + 8.0).min(base + WALL_HEIGHT), z1),
                ));
            }
        }
    }
    // Solid stone gables, then the same recessed face stones finish their surfaces.
    let mut gables = Vec::new();
    for y in 0..ROOF_RISE as u32 {
        let half = 76.0 * (1.0 - y as f32 / ROOF_RISE);
        for z in [HOUSE_MIN_Z, HOUSE_MAX_Z - WALL_THICKNESS] {
            gables.push(box_at(
                Vec3::new(CENTER_X - half, base + WALL_HEIGHT + y as f32, z),
                Vec3::new(
                    CENTER_X + half,
                    base + WALL_HEIGHT + y as f32 + 1.0,
                    z + WALL_THICKNESS,
                ),
            ));
        }
    }
    edits.push(stamp_cuboids(gables, VOXEL_TYPE_STUCCO)?);
    edits.push(stamp_cuboids(stones, VOXEL_TYPE_LIMESTONE)?);
    // Carve a proper room. The opening has a flat threshold and a round arch;
    // its 40-voxel clear width is comfortably above the player's capsule width.
    let mut carve = vec![box_at(
        Vec3::new(
            HOUSE_MIN_X + WALL_THICKNESS,
            base,
            HOUSE_MIN_Z + WALL_THICKNESS,
        ),
        Vec3::new(
            HOUSE_MAX_X - WALL_THICKNESS,
            base + WALL_HEIGHT,
            HOUSE_MAX_Z - WALL_THICKNESS,
        ),
    )];
    carve.extend(round_opening(
        CENTER_X,
        base + 8.0,
        DOOR_RADIUS,
        HOUSE_MAX_Z - WALL_THICKNESS - 1.0,
        HOUSE_MAX_Z + 5.0,
    ));
    carve.push(box_at(
        Vec3::new(
            CENTER_X - DOOR_RADIUS,
            base,
            HOUSE_MAX_Z - WALL_THICKNESS - 1.0,
        ),
        Vec3::new(CENTER_X + DOOR_RADIUS, base + 28.0, HOUSE_MAX_Z + 5.0),
    ));
    for x in [CENTER_X - WINDOW_OFFSET, CENTER_X + WINDOW_OFFSET] {
        carve.extend(round_opening(
            x,
            base + WINDOW_HEIGHT - WINDOW_RADIUS,
            WINDOW_RADIUS,
            HOUSE_MAX_Z - WALL_THICKNESS - 1.0,
            HOUSE_MAX_Z + 5.0,
        ));
    }
    edits.push(stamp_cuboids(carve, VOXEL_TYPE_EMPTY)?);
    let mut wood = Vec::new();
    // Seamed oak boards and a broad entrance path cover the raised foundation.
    for i in 0..11 {
        let x = HOUSE_MIN_X + WALL_THICKNESS + i as f32 * 12.0;
        wood.push(box_at(
            Vec3::new(x, base - 3.0, HOUSE_MIN_Z + WALL_THICKNESS),
            Vec3::new(
                (x + 11.0).min(HOUSE_MAX_X - WALL_THICKNESS),
                base,
                HOUSE_MAX_Z,
            ),
        ));
    }
    for step in 0..5 {
        let z = HOUSE_MAX_Z + step as f32 * 8.0;
        wood.push(box_at(
            Vec3::new(CENTER_X - 24.0, surface.min_y as f32 - 4.0, z),
            Vec3::new(CENTER_X + 24.0, base - step as f32 * 2.0, z + 8.0),
        ));
    }
    // Roof boards are real sloped terrain solids with a six-voxel living soil cap.
    let mut soil = Vec::new();
    for side in [-1.0, 1.0] {
        let a = Vec3::new(
            CENTER_X + side * 86.0,
            roof_y(CENTER_X + side * 86.0, base),
            284.0,
        );
        let b = Vec3::new(CENTER_X, roof_y(CENTER_X, base), 284.0);
        let rotation = Quat::from_rotation_z((b.y - a.y).atan2((b.x - a.x).abs()) * -side);
        let center = (a + b) * 0.5;
        wood.push(Cuboid::new_oriented(
            center,
            Vec3::new(a.distance(b) * 0.5, 3.0, 82.0),
            rotation,
        ));
        soil.push(Cuboid::new_oriented(
            center + Vec3::Y * 5.0,
            Vec3::new(a.distance(b) * 0.5, 3.0, 80.0),
            rotation,
        ));
        for z in [202.0, 364.0] {
            wood.push(beam_between(
                Vec3::new(a.x, a.y + 5.0, z),
                Vec3::new(b.x, b.y + 5.0, z),
                3.0,
            ));
        }
    }
    // Door jambs, warm window sills, window boxes, a little table and two benches.
    for x in [CENTER_X - 22.0, CENTER_X + 22.0] {
        wood.push(box_at(
            Vec3::new(x - 2.0, base, 351.0),
            Vec3::new(x + 2.0, base + 28.0, 358.0),
        ));
    }
    for x in [CENTER_X - WINDOW_OFFSET, CENTER_X + WINDOW_OFFSET] {
        wood.push(box_at(
            Vec3::new(x - 15.0, base + 17.0, 350.0),
            Vec3::new(x + 15.0, base + 20.0, 367.0),
        ));
        wood.push(box_at(
            Vec3::new(x - 15.0, base + 13.0, 361.0),
            Vec3::new(x + 15.0, base + 19.0, 366.0),
        ));
        soil.push(box_at(
            Vec3::new(x - 12.0, base + 17.0, 357.0),
            Vec3::new(x + 12.0, base + 20.0, 362.0),
        ));
    }
    wood.push(box_at(
        Vec3::new(134.0, base + 15.0, 244.0),
        Vec3::new(186.0, base + 19.0, 272.0),
    ));
    for x in [137.0, 180.0] {
        for z in [247.0, 266.0] {
            wood.push(box_at(
                Vec3::new(x, base, z),
                Vec3::new(x + 4.0, base + 15.0, z + 4.0),
            ));
        }
    }
    for z in [232.0, 279.0] {
        wood.push(box_at(
            Vec3::new(127.0, base + 9.0, z),
            Vec3::new(194.0, base + 12.0, z + 10.0),
        ));
        for x in [134.0, 183.0] {
            wood.push(box_at(
                Vec3::new(x, base, z + 2.0),
                Vec3::new(x + 5.0, base + 9.0, z + 8.0),
            ));
        }
    }
    // Lantern shelves and tiny shades. The exposed emissive cores are scanned by
    // the existing voxel-emitter provider, so deleting a lamp also removes its light.
    let mut lamps = Vec::new();
    for (x, z) in [(118.0, 291.0), (233.0, 250.0)] {
        wood.push(box_at(
            Vec3::new(x - 5.0, base + 29.0, z - 5.0),
            Vec3::new(x + 5.0, base + 31.0, z + 5.0),
        ));
        wood.push(box_at(
            Vec3::new(x - 5.0, base + 37.0, z - 5.0),
            Vec3::new(x + 5.0, base + 39.0, z + 5.0),
        ));
        lamps.push(box_at(
            Vec3::new(x - 1.0, base + 32.0, z - 1.0),
            Vec3::new(x + 1.0, base + 35.0, z + 1.0),
        ));
    }
    edits.push(stamp_cuboids(wood, VOXEL_TYPE_OAK_WOOD)?);
    edits.push(stamp_cuboids(soil, VOXEL_TYPE_DIRT)?);
    let frames = [CENTER_X - WINDOW_OFFSET, CENTER_X + WINDOW_OFFSET]
        .into_iter()
        .map(|x| {
            Torus::new(
                Vec3::new(x, base + WINDOW_HEIGHT, 355.0),
                WINDOW_RADIUS + 2.0,
                2.0,
            )
        })
        .collect();
    edits.push(stamp_toruses(frames, VOXEL_TYPE_OAK_WOOD)?);
    // Only the upper half of the door ring: keep the threshold completely clear.
    let mut arch = Vec::new();
    for i in 0..24 {
        let t = i as f32 / 24.0 * std::f32::consts::PI;
        let u = (i + 1) as f32 / 24.0 * std::f32::consts::PI;
        arch.push(beam_between(
            Vec3::new(
                CENTER_X + 22.0 * t.cos(),
                base + 28.0 + 22.0 * t.sin(),
                355.0,
            ),
            Vec3::new(
                CENTER_X + 22.0 * u.cos(),
                base + 28.0 + 22.0 * u.sin(),
                355.0,
            ),
            2.0,
        ));
    }
    edits.push(stamp_cuboids(arch, VOXEL_TYPE_OAK_WOOD)?);
    let panes = [CENTER_X - WINDOW_OFFSET, CENTER_X + WINDOW_OFFSET]
        .into_iter()
        .flat_map(|x| {
            round_opening(
                x,
                base + WINDOW_HEIGHT - WINDOW_RADIUS,
                WINDOW_RADIUS,
                HOUSE_MAX_Z - WINDOW_PANE_THICKNESS,
                HOUSE_MAX_Z,
            )
        })
        .collect();
    // Retain the isolated Glass encoding and its canonical empty state bits.
    edits.push(stamp_cuboids(panes, VOXEL_TYPE_SAND)?);
    edits.push(stamp_cuboids(lamps, VOXEL_TYPE_EMISSIVE)?);
    add_ivy(&mut edits, base)?;
    Ok(WorldEditTransaction::during_loading(edits))
}

fn add_ivy(edits: &mut Vec<VoxelEdit>, base: f32) -> Result<()> {
    let mut stems = Vec::new();
    let mut leaves = Vec::new();
    let mut petals = Vec::new();
    let mut hearts = Vec::new();
    // Two climbing fans leave the doorway and both panes unobstructed.
    for (root_x, direction) in [(104.0, 1.0), (248.0, -1.0)] {
        for branch in 0..3 {
            let mut previous = Vec3::new(root_x, base + 2.0, 358.0);
            for i in 1..15 {
                let h = i as f32 * 4.2;
                let x = root_x
                    + direction * (h * (0.1 + branch as f32 * 0.19) + (i as f32 * 0.8).sin() * 2.0);
                let center = Vec3::new(x, base + h, 358.0 + branch as f32);
                stems.push(beam_between(previous, center, 0.65));
                previous = center;
                for side in [-1.0, 1.0] {
                    let leaf_center = center + Vec3::new(side * 2.8, 0.6, 1.0);
                    // Overlapping tapered diamonds give each leaf a tip and a central vein.
                    let angle = side * -0.55;
                    leaves.push(Cuboid::new_oriented(
                        leaf_center,
                        Vec3::new(1.5, 3.1, 0.7),
                        Quat::from_rotation_z(angle),
                    ));
                    leaves.push(Cuboid::new_oriented(
                        leaf_center + Vec3::Y * 2.0,
                        Vec3::new(0.8, 1.8, 0.6),
                        Quat::from_rotation_z(angle),
                    ));
                }
                if (i + branch) % 5 == 0 {
                    let flower = center + Vec3::Z * 3.0;
                    for petal in 0..5 {
                        let angle = petal as f32 * std::f32::consts::TAU / 5.0;
                        petals.push(Sphere::new(
                            flower + Vec3::new(angle.cos(), angle.sin(), 0.0) * 2.2,
                            1.6,
                        ));
                    }
                    hearts.push(Sphere::new(flower + Vec3::Z, 1.0));
                }
            }
        }
    }
    edits.push(stamp_cuboids(stems, VOXEL_TYPE_OAK_WOOD)?);
    edits.push(stamp_cuboids(leaves, VOXEL_TYPE_IVY)?);
    edits.push(stamp_spheres(petals, VOXEL_TYPE_PETAL)?);
    edits.push(stamp_spheres(hearts, VOXEL_TYPE_STUCCO)?);
    Ok(())
}

impl App {
    pub(super) fn apply_house_scene(&mut self) -> Result<()> {
        let surface = sample_house_surface(&mut self.plain_builder)?;
        self.execute_world_edit(house_plan(surface)?)?;
        self.cottage_base_y = Some(house_base_y(surface));
        self.plain_builder.mark_all_solid_workgroups_dirty();
        log::info!("[HOUSE_SCENE] built cozy limestone cottage base_y={} natural_surface={}..{} room=132x60x120 door_clear_width=40 glass_panes=2 pane_thickness=2 soil_roof=true ivy_fans=2 lanterns=2 persistence=disabled", house_base_y(surface), surface.min_y, surface.max_y);
        Ok(())
    }

    pub(super) fn finish_house_garden(&mut self) -> Result<()> {
        let mut batch = AuthoredFloraPlacementBatch::new();
        let mut planted = 0;
        let mut seed = 0;
        for z in (219..354).step_by(16) {
            for x in (99..255).step_by(14) {
                seed += 1;
                if seed % 3 == 0 {
                    continue;
                }
                let Ok(anchor) = self.resolve_plantable_surface_column(UVec2::new(x, z)) else {
                    continue;
                };
                let species = if seed % 4 == 0 {
                    EMBER_BLOOM_SPECIES_INDEX
                } else {
                    LAVENDER_SPECIES_INDEX
                };
                planted += usize::from(
                    self.try_place_authored_flora(&mut batch, species, anchor, 255, 0, seed),
                );
            }
        }
        for x in [116, 128, 140, 212, 224, 236] {
            if let Ok(anchor) = self.resolve_plantable_surface_column(UVec2::new(x, 360)) {
                planted += usize::from(self.try_place_authored_flora(
                    &mut batch,
                    EMBER_BLOOM_SPECIES_INDEX,
                    anchor,
                    255,
                    0,
                    x,
                ));
            }
        }
        self.finish_authored_flora_placement(batch)?;
        anyhow::ensure!(planted >= 50, "cottage roof planting incomplete: {planted}");
        log::info!(
            "[HOUSE_SCENE][GARDEN] authored_flowers={planted} substrate_validated=true growth=255"
        );
        self.verify_house_passage()?;
        Ok(())
    }

    fn verify_house_passage(&mut self) -> Result<()> {
        // Query the published collision terrain through the door at player height.
        let floor = self
            .query_terrain_ray_cpu(
                Vec3::new(
                    CENTER_X / 256.0,
                    self.cottage_base_y.context("missing cottage floor")? / 256.0 + 0.12,
                    320.0 / 256.0,
                ),
                Vec3::NEG_Y,
            )
            .context("cottage has no interior floor")?;
        let eye = floor.position.y + 0.08;
        let hit = self
            .query_terrain_ray_cpu(Vec3::new(CENTER_X / 256.0, eye, 390.0 / 256.0), Vec3::NEG_Z)
            .context("cottage has no back wall")?;
        anyhow::ensure!(
            hit.position.z < 340.0 / 256.0,
            "cottage entrance is blocked: {:?}",
            hit.position
        );
        for x in [CENTER_X - WINDOW_OFFSET, CENTER_X + WINDOW_OFFSET] {
            let glass = self
                .query_terrain_ray_cpu(
                    Vec3::new(
                        x / 256.0,
                        (self.cottage_base_y.unwrap() + WINDOW_HEIGHT) / 256.0,
                        380.0 / 256.0,
                    ),
                    Vec3::NEG_Z,
                )
                .context("missing window collision")?;
            anyhow::ensure!(
                glass.voxel_type == VOXEL_TYPE_SAND && glass.position.z >= 352.0 / 256.0,
                "cottage window collision differs from Glass pane: {glass:?}"
            );
        }
        log::info!("[HOUSE_SCENE][WINDOWS] glass_collision_panes=2 optical_mode=GlassExperiment");
        let walk = self.terrain_physics.move_player_capsule(
            crate::gameplay::camera::PlayerWalkMovementRequest {
                camera_position: Vec3::new(CENTER_X / 256.0, eye + 0.001, 360.0 / 256.0),
                camera_height: 0.08,
                desired_translation: Vec3::new(0.0, 0.0, -56.0 / 256.0),
            },
            0.25,
        )?;
        anyhow::ensure!(
            walk.translation.z < -50.0 / 256.0,
            "player capsule cannot enter cottage: {walk:?}"
        );
        log::info!(
            "[HOUSE_SCENE][WALK] production_capsule_translation={:?} grounded={}",
            walk.translation,
            walk.grounded
        );
        log::info!("[HOUSE_SCENE][PASSAGE] player_eye_y={eye:.5} doorway_ray_hit_z={:.5} floor_material={} published_cpu_collision=true", hit.position.z, floor.voxel_type);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cottage_keeps_two_thin_glass_panes_and_a_clear_threshold() {
        let surface = SurfaceSampleReport {
            median_y: 100,
            min_y: 96,
            max_y: 104,
            facade_center_y: 96,
        };
        let plan = house_plan(surface).unwrap();
        let panes = plan
            .voxel_edits()
            .iter()
            .find_map(|edit| match edit {
                VoxelEdit::StampCuboids {
                    cuboids,
                    voxel_type: VOXEL_TYPE_SAND,
                    atlas_state_write,
                    ..
                } => {
                    assert_eq!(*atlas_state_write, Default::default());
                    Some(cuboids)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(panes.len(), WINDOW_RADIUS as usize * 4);
        assert!(panes
            .iter()
            .all(|pane| pane.min().z == HOUSE_MAX_Z - 2.0 && pane.max().z == HOUSE_MAX_Z));
        assert!(panes
            .iter()
            .all(|pane| (pane.center().x - CENTER_X).abs() > DOOR_RADIUS));
        let carved = plan
            .voxel_edits()
            .iter()
            .find_map(|edit| match edit {
                VoxelEdit::StampCuboids {
                    cuboids,
                    voxel_type: VOXEL_TYPE_EMPTY,
                    ..
                } => Some(cuboids),
                _ => None,
            })
            .unwrap();
        let base = house_base_y(surface);
        assert!(carved.iter().any(|cut| cut.min().y == base
            && cut.min().x == CENTER_X - DOOR_RADIUS
            && cut.max().x == CENTER_X + DOOR_RADIUS
            && cut.min().z < HOUSE_MAX_Z - WALL_THICKNESS
            && cut.max().z > HOUSE_MAX_Z));
    }
}
