use super::App;
use crate::app::world_edits::{VoxelEdit, WorldEditPlan};
use crate::builder::{
    voxel_type_from_atlas_byte, PlainBuilder, TerrainHillField, VOXEL_TYPE_DIRT, VOXEL_TYPE_EMPTY,
    VOXEL_TYPE_OAK_WOOD, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND, VOXEL_TYPE_STUCCO,
};
use crate::geom::{build_bvh, Cuboid, Torus};
use anyhow::{Context, Result};
use glam::{UVec3, Vec2, Vec3};

// All scene dimensions are terrain voxels. One world unit is 256 terrain voxels.
const HOUSE_MIN_X: f32 = 112.0;
const HOUSE_MAX_X: f32 = 222.0;
const HOUSE_MIN_Z: f32 = 178.0;
const HOUSE_MAX_Z: f32 = 354.0;
const SURFACE_SAMPLE_OFFSET: UVec3 = UVec3::new(112, 32, 178);
const SURFACE_SAMPLE_DIM: UVec3 = UVec3::new(111, 224, 177);
const WALL_THICKNESS: f32 = 16.0;
const FACADE_MATERIAL_DEPTH: f32 = WALL_THICKNESS;
const INTERIOR_SIDE_INSET: f32 = 10.0;
const MAIN_INTERIOR_HEIGHT: f32 = 46.0;
const FRONT_INTERIOR_HEIGHT: f32 = 38.0;
const FRONT_INTERIOR_DEPTH: f32 = 34.0;
const ROUND_DOOR_RADIUS: f32 = 18.0;
const DOOR_FRAME_MAJOR_RADIUS: f32 = 21.0;
const DOOR_FRAME_TUBE_RADIUS: f32 = 3.0;
const WINDOW_CENTER_X_OFFSET: f32 = 43.0;
const WINDOW_CENTER_HEIGHT: f32 = 27.0;
const ROUND_WINDOW_RADIUS: f32 = 7.0;
const WINDOW_FRAME_MAJOR_RADIUS: f32 = 9.0;
const WINDOW_FRAME_TUBE_RADIUS: f32 = 2.0;
const HILL_CENTER_Z: f32 = 220.0;
const HILL_RADIUS_X: f32 = 165.0;
const HILL_RADIUS_Z: f32 = 215.0;
const HILL_RISE: f32 = 72.0;
const HILL_MAXIMUM_INFLATION: f32 = 8.0;
const HILL_NOISE_AMPLITUDE: f32 = 5.0;
const HILL_NOISE_FREQUENCY_WORLD: f32 = 3.0;
const HILL_NOISE_SEED: u32 = 0x484f_4242;

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

fn hobbit_hill(surface: SurfaceSampleReport) -> TerrainHillField {
    TerrainHillField {
        center_voxels: Vec2::new((HOUSE_MIN_X + HOUSE_MAX_X) * 0.5, HILL_CENTER_Z),
        front_plane_z_voxels: HOUSE_MAX_Z,
        front_material_depth_voxels: FACADE_MATERIAL_DEPTH,
        front_material_voxel_type: VOXEL_TYPE_STUCCO,
        base_height_voxels: house_base_y(surface),
        radii_voxels: Vec2::new(HILL_RADIUS_X, HILL_RADIUS_Z),
        rise_voxels: HILL_RISE,
        maximum_inflation_voxels: HILL_MAXIMUM_INFLATION,
        noise_amplitude_voxels: HILL_NOISE_AMPLITUDE,
        noise_frequency_world: HILL_NOISE_FREQUENCY_WORLD,
        noise_seed: HILL_NOISE_SEED,
    }
}

fn nominal_cut_face_height() -> f32 {
    nominal_hill_profile_height((HOUSE_MIN_X + HOUSE_MAX_X) * 0.5, HOUSE_MAX_Z)
}

fn nominal_hill_profile_height(x: f32, z: f32) -> f32 {
    let center_x = (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5;
    let normalized = Vec2::new(
        (x - center_x) / HILL_RADIUS_X,
        (z - HILL_CENTER_Z) / HILL_RADIUS_Z,
    );
    HILL_RISE * (1.0 - normalized.length_squared())
}

fn nominal_visible_cut_face_height(surface: SurfaceSampleReport) -> f32 {
    house_base_y(surface) + nominal_cut_face_height() - surface.facade_center_y as f32
}

fn door_cut_ratio(surface: SurfaceSampleReport) -> f32 {
    ROUND_DOOR_RADIUS * 2.0 / nominal_visible_cut_face_height(surface)
}

fn house_plan(surface: SurfaceSampleReport) -> Result<WorldEditPlan> {
    let base_y = house_base_y(surface);
    let center_x = (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5;

    // The authored pieces stop at the facade; the canonical terrain hill is
    // the exterior wall and roof around the carved room.
    let floor_bottom_y = base_y - 1.0;
    let floor = box_at(
        Vec3::new(
            HOUSE_MIN_X + INTERIOR_SIDE_INSET,
            floor_bottom_y,
            HOUSE_MIN_Z + INTERIOR_SIDE_INSET,
        ),
        Vec3::new(HOUSE_MAX_X - INTERIOR_SIDE_INSET, base_y, HOUSE_MAX_Z),
    );
    let front_interior_min_z = HOUSE_MAX_Z - FRONT_INTERIOR_DEPTH;
    let main_interior = box_at(
        Vec3::new(
            HOUSE_MIN_X + INTERIOR_SIDE_INSET,
            base_y,
            HOUSE_MIN_Z + INTERIOR_SIDE_INSET,
        ),
        Vec3::new(
            HOUSE_MAX_X - INTERIOR_SIDE_INSET,
            base_y + MAIN_INTERIOR_HEIGHT,
            front_interior_min_z + 1.0,
        ),
    );
    let front_interior = box_at(
        Vec3::new(
            HOUSE_MIN_X + INTERIOR_SIDE_INSET,
            base_y,
            front_interior_min_z,
        ),
        Vec3::new(
            HOUSE_MAX_X - INTERIOR_SIDE_INSET,
            base_y + FRONT_INTERIOR_HEIGHT,
            HOUSE_MAX_Z - WALL_THICKNESS + 1.0,
        ),
    );
    let frame_center_z = HOUSE_MAX_Z - 0.5;
    let frames = vec![
        Torus::new(
            Vec3::new(center_x, base_y + ROUND_DOOR_RADIUS, frame_center_z),
            DOOR_FRAME_MAJOR_RADIUS,
            DOOR_FRAME_TUBE_RADIUS,
        ),
        Torus::new(
            Vec3::new(
                center_x - WINDOW_CENTER_X_OFFSET,
                base_y + WINDOW_CENTER_HEIGHT,
                frame_center_z,
            ),
            WINDOW_FRAME_MAJOR_RADIUS,
            WINDOW_FRAME_TUBE_RADIUS,
        ),
        Torus::new(
            Vec3::new(
                center_x + WINDOW_CENTER_X_OFFSET,
                base_y + WINDOW_CENTER_HEIGHT,
                frame_center_z,
            ),
            WINDOW_FRAME_MAJOR_RADIUS,
            WINDOW_FRAME_TUBE_RADIUS,
        ),
    ];
    debug_assert_eq!(frames[0].inner_radius(), ROUND_DOOR_RADIUS);
    debug_assert!(frames[1..]
        .iter()
        .all(|frame| frame.inner_radius() == ROUND_WINDOW_RADIUS));
    let mut openings = round_opening(
        center_x,
        base_y,
        ROUND_DOOR_RADIUS,
        HOUSE_MAX_Z - WALL_THICKNESS - 1.0,
        HOUSE_MAX_Z + 1.0,
    );
    for window_center_x in [
        center_x - WINDOW_CENTER_X_OFFSET,
        center_x + WINDOW_CENTER_X_OFFSET,
    ] {
        openings.extend(round_opening(
            window_center_x,
            base_y + WINDOW_CENTER_HEIGHT - ROUND_WINDOW_RADIUS,
            ROUND_WINDOW_RADIUS,
            HOUSE_MAX_Z - WALL_THICKNESS - 1.0,
            HOUSE_MAX_Z + 1.0,
        ));
    }

    Ok(WorldEditPlan {
        voxel_edits: vec![
            stamp_cuboids(vec![floor], VOXEL_TYPE_STUCCO)?,
            stamp_cuboids(vec![main_interior, front_interior], VOXEL_TYPE_EMPTY)?,
            stamp_toruses(frames, VOXEL_TYPE_OAK_WOOD)?,
            stamp_cuboids(openings, VOXEL_TYPE_EMPTY)?,
        ],
        // Loading publishes every chunk after applying this plan.
        build_edits: Vec::new(),
    })
}

impl App {
    pub(super) fn apply_house_scene(&mut self) -> Result<()> {
        let surface = sample_house_surface(&mut self.plain_builder)?;
        anyhow::ensure!(
            (0.6..=0.7).contains(&door_cut_ratio(surface)),
            "Hobbit door must occupy 60-70% of the visible cut face; ratio={:.3}",
            door_cut_ratio(surface),
        );
        let hill = hobbit_hill(surface);
        let hill_bound = self.plain_builder.blend_terrain_hill(hill)?;
        self.execute_edit_plan(house_plan(surface)?)?;
        self.plain_builder.mark_all_solid_workgroups_dirty();
        log::info!(
            "[HOUSE_SCENE] built raised terrain-integrated Hobbit hill with shallow stucco cut face, oak door frame, and two round windows base_y={} footprint_surface_y={}..{} facade_center_y={} hill_bound={:?} nominal_profile_height={:.1} nominal_visible_cut_height={:.1} door_ratio={:.3} hill_rise={} maximum_inflation={}",
            house_base_y(surface),
            surface.min_y,
            surface.max_y,
            surface.facade_center_y,
            hill_bound,
            nominal_cut_face_height(),
            nominal_visible_cut_face_height(surface),
            door_cut_ratio(surface),
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
            facade_center_y: 96,
        }
    }

    #[test]
    fn hobbit_hill_stops_at_the_vertical_facade_plane() {
        let hill = hobbit_hill(test_surface());
        assert_eq!(hill.center_voxels.x, (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5);
        assert!(hill.center_voxels.y < HOUSE_MAX_Z);
        assert_eq!(hill.front_plane_z_voxels, HOUSE_MAX_Z);
        assert_eq!(hill.front_material_voxel_type, VOXEL_TYPE_STUCCO);
        assert_eq!(hill.front_material_depth_voxels, FACADE_MATERIAL_DEPTH);
        assert_eq!(hill.base_height_voxels, 105.0);
        assert_eq!(hill.radii_voxels, Vec2::new(HILL_RADIUS_X, HILL_RADIUS_Z));
        assert_eq!(hill.rise_voxels, HILL_RISE);
        assert_eq!(hill.maximum_inflation_voxels, HILL_MAXIMUM_INFLATION);
        assert!(hill.rise_voxels / hill.radii_voxels.min_element() < 0.45);
        assert!((0.6..=0.7).contains(&door_cut_ratio(test_surface())));

        let interior_corner_x = HOUSE_MIN_X + INTERIOR_SIDE_INSET;
        let main_front_profile =
            nominal_hill_profile_height(interior_corner_x, HOUSE_MAX_Z - FRONT_INTERIOR_DEPTH);
        let vestibule_front_profile =
            nominal_hill_profile_height(interior_corner_x, HOUSE_MAX_Z - WALL_THICKNESS + 1.0);
        assert!(main_front_profile - HILL_NOISE_AMPLITUDE >= MAIN_INTERIOR_HEIGHT);
        assert!(vestibule_front_profile - HILL_NOISE_AMPLITUDE >= FRONT_INTERIOR_HEIGHT);
    }

    #[test]
    fn house_uses_cut_hill_with_round_door_windows_and_oak_frames() {
        let plan = house_plan(test_surface()).unwrap();

        assert_eq!(plan.voxel_edits.len(), 4);
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
        assert_eq!(floors[0].min().y, 104.0);
        assert_eq!(floors[0].max().y, 105.0);

        let VoxelEdit::StampCuboids {
            cuboids: interiors,
            voxel_type: interior_type,
            ..
        } = &plan.voxel_edits[1]
        else {
            panic!("expected interior carve");
        };
        assert_eq!(*interior_type, VOXEL_TYPE_EMPTY);
        assert_eq!(interiors.len(), 2);
        assert_eq!(interiors[0].height(), MAIN_INTERIOR_HEIGHT);
        assert_eq!(interiors[1].height(), FRONT_INTERIOR_HEIGHT);
        assert!(interiors.iter().all(|interior| interior.width() >= 90.0));
        assert!(interiors[0].depth() >= 130.0);
        assert!(interiors[0].max().z > interiors[1].min().z);
        assert!(interiors[1].max().z > HOUSE_MAX_Z - WALL_THICKNESS);

        let VoxelEdit::StampToruses {
            toruses: frames,
            voxel_type: frame_type,
            ..
        } = &plan.voxel_edits[2]
        else {
            panic!("expected torus frames");
        };
        assert_eq!(*frame_type, VOXEL_TYPE_OAK_WOOD);
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].inner_radius(), ROUND_DOOR_RADIUS);
        assert_eq!(frames[1].inner_radius(), ROUND_WINDOW_RADIUS);
        assert_eq!(frames[2].inner_radius(), ROUND_WINDOW_RADIUS);
        assert_eq!(frames[1].center().y, frames[2].center().y);
        assert_eq!(frames[0].center().z, HOUSE_MAX_Z - 0.5);

        let VoxelEdit::StampCuboids {
            cuboids: opening_slices,
            voxel_type: opening_type,
            ..
        } = &plan.voxel_edits[3]
        else {
            panic!("expected round opening slices");
        };
        assert_eq!(*opening_type, VOXEL_TYPE_EMPTY);
        assert_eq!(
            opening_slices.len(),
            (ROUND_DOOR_RADIUS * 2.0 + ROUND_WINDOW_RADIUS * 4.0) as usize
        );
        assert!(opening_slices
            .iter()
            .all(|slice| slice.max().z == HOUSE_MAX_Z + 1.0));

        assert!(plan.voxel_edits.iter().all(|edit| !matches!(
            edit,
            VoxelEdit::StampCuboids {
                voxel_type: VOXEL_TYPE_ROCK,
                ..
            }
        )));
    }
}
