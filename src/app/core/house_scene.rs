use super::App;
use crate::app::world_edits::{VoxelEdit, WorldEditPlan};
use crate::builder::{
    voxel_type_from_atlas_byte, PlainBuilder, TerrainHillField, VOXEL_TYPE_DIRT, VOXEL_TYPE_EMPTY,
    VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND, VOXEL_TYPE_STUCCO,
};
use crate::geom::{build_bvh, Cuboid};
use anyhow::{Context, Result};
use glam::{UVec3, Vec2, Vec3};

// All scene dimensions are terrain voxels. One world unit is 256 terrain voxels.
const HOUSE_MIN_X: f32 = 112.0;
const HOUSE_MAX_X: f32 = 222.0;
const HOUSE_MIN_Z: f32 = 242.0;
const HOUSE_MAX_Z: f32 = 376.0;
const SURFACE_SAMPLE_OFFSET: UVec3 = UVec3::new(112, 32, 242);
const SURFACE_SAMPLE_DIM: UVec3 = UVec3::new(111, 224, 135);
const WALL_THICKNESS: f32 = 8.0;
const FACADE_HALF_WIDTH: f32 = 26.0;
const FACADE_HEIGHT: f32 = 50.0;
const FACADE_REVEAL_RADIUS: f32 = 25.0;
const INTERIOR_SIDE_INSET: f32 = 14.0;
const INTERIOR_HEIGHT: f32 = 42.0;
const ROUND_DOOR_RADIUS: f32 = 18.0;
const HILL_CENTER_Z: f32 = 294.0;
const HILL_RADIUS_X: f32 = 110.0;
const HILL_RADIUS_Z: f32 = 145.0;
const HILL_RISE: f32 = 85.0;
const HILL_MAXIMUM_INFLATION: f32 = 8.0;
const HILL_NOISE_AMPLITUDE: f32 = 5.0;
const HILL_NOISE_FREQUENCY_WORLD: f32 = 3.0;
const HILL_NOISE_SEED: u32 = 0x484f_4242;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SurfaceSampleReport {
    median_y: u32,
    min_y: u32,
    max_y: u32,
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
    for z in 0..SURFACE_SAMPLE_DIM.z {
        for x in 0..SURFACE_SAMPLE_DIM.x {
            let surface = (0..SURFACE_SAMPLE_DIM.y).rev().find_map(|y| {
                let voxel_type = voxel_type_from_atlas_byte(
                    bytes[volume_index(UVec3::new(x, y, z), SURFACE_SAMPLE_DIM)],
                );
                is_terrain_voxel(voxel_type).then_some(SURFACE_SAMPLE_OFFSET.y + y)
            });
            heights.push(surface.with_context(|| {
                format!(
                    "house footprint column has no terrain at x={} z={}",
                    SURFACE_SAMPLE_OFFSET.x + x,
                    SURFACE_SAMPLE_OFFSET.z + z
                )
            })?);
        }
    }
    heights.sort_unstable();
    Ok(SurfaceSampleReport {
        median_y: heights[heights.len() / 2],
        min_y: *heights.first().context("empty house surface sample")?,
        max_y: *heights.last().context("empty house surface sample")?,
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
    })
}

fn box_at(min: Vec3, max: Vec3) -> Cuboid {
    Cuboid::from_min_max(min, max)
}

fn round_opening(base_y: f32, radius: f32, tunnel_min_z: f32, tunnel_max_z: f32) -> Vec<Cuboid> {
    let center_x = (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5;
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

fn hobbit_hill(surface: SurfaceSampleReport) -> TerrainHillField {
    TerrainHillField {
        center_voxels: Vec2::new((HOUSE_MIN_X + HOUSE_MAX_X) * 0.5, HILL_CENTER_Z),
        base_height_voxels: surface.median_y as f32 + 1.0,
        radii_voxels: Vec2::new(HILL_RADIUS_X, HILL_RADIUS_Z),
        rise_voxels: HILL_RISE,
        maximum_inflation_voxels: HILL_MAXIMUM_INFLATION,
        noise_amplitude_voxels: HILL_NOISE_AMPLITUDE,
        noise_frequency_world: HILL_NOISE_FREQUENCY_WORLD,
        noise_seed: HILL_NOISE_SEED,
    }
}

fn house_plan(surface: SurfaceSampleReport) -> Result<WorldEditPlan> {
    let base_y = surface.median_y as f32 + 1.0;
    let center_x = (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5;

    // The authored pieces stop at the facade; the canonical terrain hill is
    // the exterior wall and roof around the carved room.
    let floor_bottom_y = (surface.min_y as f32 + 1.0).min(base_y - 1.0);
    let floor = box_at(
        Vec3::new(
            HOUSE_MIN_X + INTERIOR_SIDE_INSET,
            floor_bottom_y,
            HOUSE_MIN_Z + INTERIOR_SIDE_INSET,
        ),
        Vec3::new(HOUSE_MAX_X - INTERIOR_SIDE_INSET, base_y, HOUSE_MAX_Z),
    );
    let facade = box_at(
        Vec3::new(
            center_x - FACADE_HALF_WIDTH,
            base_y,
            HOUSE_MAX_Z - WALL_THICKNESS,
        ),
        Vec3::new(
            center_x + FACADE_HALF_WIDTH,
            base_y + FACADE_HEIGHT,
            HOUSE_MAX_Z,
        ),
    );
    let interior = box_at(
        Vec3::new(
            HOUSE_MIN_X + INTERIOR_SIDE_INSET,
            base_y,
            HOUSE_MIN_Z + INTERIOR_SIDE_INSET,
        ),
        Vec3::new(
            HOUSE_MAX_X - INTERIOR_SIDE_INSET,
            base_y + INTERIOR_HEIGHT,
            HOUSE_MAX_Z - WALL_THICKNESS + 1.0,
        ),
    );
    let door_tunnel_max_z = HILL_CENTER_Z + HILL_RADIUS_Z + HILL_MAXIMUM_INFLATION;
    let facade_reveal = round_opening(base_y, FACADE_REVEAL_RADIUS, HOUSE_MAX_Z, door_tunnel_max_z);
    let round_door = round_opening(
        base_y,
        ROUND_DOOR_RADIUS,
        HOUSE_MAX_Z - WALL_THICKNESS - 1.0,
        door_tunnel_max_z,
    );

    Ok(WorldEditPlan {
        voxel_edits: vec![
            stamp_cuboids(vec![floor], VOXEL_TYPE_STUCCO)?,
            stamp_cuboids(facade_reveal, VOXEL_TYPE_EMPTY)?,
            stamp_cuboids(vec![facade], VOXEL_TYPE_STUCCO)?,
            stamp_cuboids(vec![interior], VOXEL_TYPE_EMPTY)?,
            stamp_cuboids(round_door, VOXEL_TYPE_EMPTY)?,
        ],
        // Loading publishes every chunk after applying this plan.
        build_edits: Vec::new(),
    })
}

impl App {
    pub(super) fn apply_house_scene(&mut self) -> Result<()> {
        let surface = sample_house_surface(&mut self.plain_builder)?;
        let hill = hobbit_hill(surface);
        let hill_bound = self.plain_builder.blend_terrain_hill(hill)?;
        self.execute_edit_plan(house_plan(surface)?)?;
        self.plain_builder.mark_all_solid_workgroups_dirty();
        log::info!(
            "[HOUSE_SCENE] built terrain-integrated Hobbit hill with round entrance ground_y={} footprint_surface_y={}..{} hill_bound={:?} hill_rise={} maximum_inflation={}",
            surface.median_y,
            surface.min_y,
            surface.max_y,
            hill_bound,
            HILL_RISE,
            HILL_MAXIMUM_INFLATION,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_surface() -> SurfaceSampleReport {
        SurfaceSampleReport {
            median_y: 100,
            min_y: 96,
            max_y: 104,
        }
    }

    #[test]
    fn hobbit_hill_is_centered_behind_the_round_facade() {
        let hill = hobbit_hill(test_surface());
        assert_eq!(hill.center_voxels.x, (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5);
        assert!(hill.center_voxels.y < HOUSE_MAX_Z);
        assert_eq!(hill.base_height_voxels, 101.0);
        assert_eq!(hill.radii_voxels, Vec2::new(HILL_RADIUS_X, HILL_RADIUS_Z));
        assert_eq!(hill.rise_voxels, HILL_RISE);
        assert_eq!(hill.maximum_inflation_voxels, HILL_MAXIMUM_INFLATION);
    }

    #[test]
    fn house_uses_hill_as_roof_and_carves_a_room_with_round_entrance() {
        let plan = house_plan(test_surface()).unwrap();

        assert_eq!(plan.voxel_edits.len(), 5);
        let VoxelEdit::StampCuboids {
            cuboids: floors,
            voxel_type: floor_type,
            ..
        } = &plan.voxel_edits[0]
        else {
            panic!("expected floor cuboids");
        };
        assert_eq!(*floor_type, VOXEL_TYPE_STUCCO);
        assert_eq!(floors.len(), 1);
        assert_eq!(floors[0].max().y, 101.0);

        let VoxelEdit::StampCuboids {
            cuboids: reveal_slices,
            voxel_type: reveal_type,
            ..
        } = &plan.voxel_edits[1]
        else {
            panic!("expected round facade reveal slices");
        };
        assert_eq!(*reveal_type, VOXEL_TYPE_EMPTY);
        assert_eq!(reveal_slices.len(), (FACADE_REVEAL_RADIUS * 2.0) as usize);
        assert!(reveal_slices
            .iter()
            .all(|slice| slice.min().z >= HOUSE_MAX_Z));

        let VoxelEdit::StampCuboids {
            cuboids: facades,
            voxel_type: facade_type,
            ..
        } = &plan.voxel_edits[2]
        else {
            panic!("expected facade cuboid");
        };
        assert_eq!(*facade_type, VOXEL_TYPE_STUCCO);
        assert_eq!(facades.len(), 1);
        assert_eq!(facades[0].width(), FACADE_HALF_WIDTH * 2.0);
        assert_eq!(facades[0].height(), FACADE_HEIGHT);

        let VoxelEdit::StampCuboids {
            cuboids: interiors,
            voxel_type: interior_type,
            ..
        } = &plan.voxel_edits[3]
        else {
            panic!("expected interior carve");
        };
        assert_eq!(*interior_type, VOXEL_TYPE_EMPTY);
        assert_eq!(interiors.len(), 1);
        assert_eq!(interiors[0].height(), INTERIOR_HEIGHT);
        assert!(interiors[0].max().z > HOUSE_MAX_Z - WALL_THICKNESS);

        let VoxelEdit::StampCuboids {
            cuboids: door_slices,
            voxel_type: door_type,
            ..
        } = &plan.voxel_edits[4]
        else {
            panic!("expected round door opening slices");
        };
        assert_eq!(*door_type, VOXEL_TYPE_EMPTY);
        assert_eq!(door_slices.len(), (ROUND_DOOR_RADIUS * 2.0) as usize);
        assert!(door_slices[0].width() < door_slices[door_slices.len() / 2].width());
        assert!(door_slices.last().unwrap().width() < door_slices[door_slices.len() / 2].width());
        assert!(door_slices.iter().all(|slice| {
            (slice.center().x - (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5).abs() < 1.0e-4
        }));
        assert!(door_slices
            .iter()
            .all(|slice| slice.max().z > HOUSE_MAX_Z + HILL_MAXIMUM_INFLATION));

        assert!(plan.voxel_edits.iter().all(|edit| !matches!(
            edit,
            VoxelEdit::StampCuboids {
                voxel_type: VOXEL_TYPE_ROCK,
                ..
            }
        )));
    }
}
