use super::App;
use crate::app::world_edits::{VoxelEdit, WorldEditPlan};
use crate::builder::{
    voxel_type_from_atlas_byte, PlainBuilder, VOXEL_TYPE_DIRT, VOXEL_TYPE_EMPTY, VOXEL_TYPE_ROCK,
    VOXEL_TYPE_SAND, VOXEL_TYPE_STUCCO,
};
use crate::geom::{build_bvh, Cuboid};
use anyhow::{Context, Result};
use glam::{UVec3, Vec3};

// All scene dimensions are terrain voxels. One world unit is 256 terrain voxels.
const HOUSE_MIN_X: f32 = 112.0;
const HOUSE_MAX_X: f32 = 222.0;
const HOUSE_MIN_Z: f32 = 242.0;
const HOUSE_MAX_Z: f32 = 376.0;
const SURFACE_SAMPLE_OFFSET: UVec3 = UVec3::new(112, 32, 242);
const SURFACE_SAMPLE_DIM: UVec3 = UVec3::new(111, 224, 135);
// The authored vertical dimensions were first reduced to 70%; keep the second
// 70% reduction explicit so the resulting 49% scale is easy to audit.
const HOUSE_HEIGHT_SCALE: f32 = 0.7 * 0.7;
const WALL_HEIGHT: f32 = 96.0 * HOUSE_HEIGHT_SCALE;
const WALL_THICKNESS: f32 = 8.0;
const ROOF_OVERHANG: f32 = 6.0;
const ROOF_THICKNESS: f32 = 9.0 * HOUSE_HEIGHT_SCALE;

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

fn scaled_height(original_voxels: f32) -> f32 {
    original_voxels * HOUSE_HEIGHT_SCALE
}

fn house_plan(surface: SurfaceSampleReport) -> Result<WorldEditPlan> {
    let base_y = surface.median_y as f32 + 1.0;
    let wall_bottom_y = surface.min_y as f32 + 1.0;
    let wall_top_y = base_y + WALL_HEIGHT;
    let roof_top_y = wall_top_y + ROOF_THICKNESS;

    // Carving the interior from `base_y` leaves the shell's top layer as the indoor floor.
    let wall_and_floor_shell = box_at(
        Vec3::new(HOUSE_MIN_X, wall_bottom_y, HOUSE_MIN_Z),
        Vec3::new(HOUSE_MAX_X, wall_top_y, HOUSE_MAX_Z),
    );
    let hollow_interior = box_at(
        Vec3::new(
            HOUSE_MIN_X + WALL_THICKNESS,
            base_y,
            HOUSE_MIN_Z + WALL_THICKNESS,
        ),
        Vec3::new(
            HOUSE_MAX_X - WALL_THICKNESS,
            wall_top_y,
            HOUSE_MAX_Z - WALL_THICKNESS,
        ),
    );

    let openings = vec![
        // Front door and window face the overlook camera.
        box_at(
            Vec3::new(154.0, base_y, HOUSE_MAX_Z - WALL_THICKNESS - 1.0),
            Vec3::new(180.0, base_y + scaled_height(69.0), HOUSE_MAX_Z + 1.0),
        ),
        box_at(
            Vec3::new(
                190.0,
                base_y + scaled_height(36.0),
                HOUSE_MAX_Z - WALL_THICKNESS - 1.0,
            ),
            Vec3::new(213.0, base_y + scaled_height(68.0), HOUSE_MAX_Z + 1.0),
        ),
        // Broad side windows keep the small interior naturally lit.
        box_at(
            Vec3::new(
                HOUSE_MAX_X - WALL_THICKNESS - 1.0,
                base_y + scaled_height(34.0),
                278.0,
            ),
            Vec3::new(HOUSE_MAX_X + 1.0, base_y + scaled_height(70.0), 338.0),
        ),
        box_at(
            Vec3::new(HOUSE_MIN_X - 1.0, base_y + scaled_height(34.0), 278.0),
            Vec3::new(
                HOUSE_MIN_X + WALL_THICKNESS + 1.0,
                base_y + scaled_height(70.0),
                338.0,
            ),
        ),
        box_at(
            Vec3::new(147.0, base_y + scaled_height(36.0), HOUSE_MIN_Z - 1.0),
            Vec3::new(
                188.0,
                base_y + scaled_height(68.0),
                HOUSE_MIN_Z + WALL_THICKNESS + 1.0,
            ),
        ),
    ];

    let flat_roof = box_at(
        Vec3::new(
            HOUSE_MIN_X - ROOF_OVERHANG,
            wall_top_y,
            HOUSE_MIN_Z - ROOF_OVERHANG,
        ),
        Vec3::new(
            HOUSE_MAX_X + ROOF_OVERHANG,
            roof_top_y,
            HOUSE_MAX_Z + ROOF_OVERHANG,
        ),
    );
    let planter_top_y = base_y + scaled_height(33.0);
    let planter_bottom_y = planter_top_y - scaled_height(9.0);
    let planter_bottom_thickness = scaled_height(3.0);
    let window_planter = vec![
        // A shallow hollow window box: bottom, front rail, and two end caps.
        box_at(
            Vec3::new(HOUSE_MAX_X, planter_bottom_y, 274.0),
            Vec3::new(
                HOUSE_MAX_X + 10.0,
                planter_bottom_y + planter_bottom_thickness,
                342.0,
            ),
        ),
        box_at(
            Vec3::new(
                HOUSE_MAX_X + 7.0,
                planter_bottom_y + planter_bottom_thickness,
                274.0,
            ),
            Vec3::new(HOUSE_MAX_X + 10.0, planter_top_y, 342.0),
        ),
        box_at(
            Vec3::new(
                HOUSE_MAX_X,
                planter_bottom_y + planter_bottom_thickness,
                274.0,
            ),
            Vec3::new(HOUSE_MAX_X + 7.0, planter_top_y, 277.0),
        ),
        box_at(
            Vec3::new(
                HOUSE_MAX_X,
                planter_bottom_y + planter_bottom_thickness,
                339.0,
            ),
            Vec3::new(HOUSE_MAX_X + 7.0, planter_top_y, 342.0),
        ),
    ];
    let planter_soil = box_at(
        Vec3::new(
            HOUSE_MAX_X + 1.0,
            planter_bottom_y + planter_bottom_thickness,
            277.0,
        ),
        Vec3::new(HOUSE_MAX_X + 7.0, planter_top_y - scaled_height(1.0), 339.0),
    );

    Ok(WorldEditPlan {
        voxel_edits: vec![
            stamp_cuboids(vec![wall_and_floor_shell], VOXEL_TYPE_STUCCO)?,
            stamp_cuboids(
                std::iter::once(hollow_interior).chain(openings).collect(),
                VOXEL_TYPE_EMPTY,
            )?,
            stamp_cuboids(vec![flat_roof], VOXEL_TYPE_STUCCO)?,
            stamp_cuboids(window_planter, VOXEL_TYPE_STUCCO)?,
            stamp_cuboids(vec![planter_soil], VOXEL_TYPE_DIRT)?,
        ],
        // Loading publishes every chunk after applying this plan.
        build_edits: Vec::new(),
    })
}

impl App {
    pub(super) fn apply_house_scene(&mut self) -> Result<()> {
        let surface = sample_house_surface(&mut self.plain_builder)?;
        self.execute_edit_plan(house_plan(surface)?)?;
        self.plain_builder.mark_all_solid_workgroups_dirty();
        log::info!(
            "[HOUSE_SCENE] built flat-roof house on unchanged natural terrain ground_y={} footprint_surface_y={}..{} height_scale={:.2} wall_thickness={} roof_overhang={}",
            surface.median_y,
            surface.min_y,
            surface.max_y,
            HOUSE_HEIGHT_SCALE,
            WALL_THICKNESS,
            ROOF_OVERHANG,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn house_uses_plantable_stucco_roof_and_floor_without_chimney() {
        assert!((HOUSE_HEIGHT_SCALE - 0.7 * 0.7).abs() < f32::EPSILON);

        let ground_y = 100;
        let plan = house_plan(SurfaceSampleReport {
            median_y: ground_y,
            min_y: ground_y - 4,
            max_y: ground_y + 4,
        })
        .unwrap();

        let VoxelEdit::StampCuboids {
            cuboids: structure,
            voxel_type: wall_type,
            ..
        } = &plan.voxel_edits[0]
        else {
            panic!("expected wall cuboids");
        };
        assert_eq!(*wall_type, VOXEL_TYPE_STUCCO);
        assert_eq!(structure.len(), 1);

        let VoxelEdit::StampCuboids {
            cuboids: carved_space,
            voxel_type: carved_type,
            ..
        } = &plan.voxel_edits[1]
        else {
            panic!("expected carved interior and openings");
        };
        assert_eq!(*carved_type, VOXEL_TYPE_EMPTY);
        let floor_surface_y = ground_y as f32 + 1.0;
        assert!(structure[0].min().y < floor_surface_y);
        assert!((carved_space[0].min().y - floor_surface_y).abs() < 1.0e-4);
        assert!((carved_space[1].max().y - floor_surface_y - scaled_height(69.0)).abs() < 1.0e-4);
        assert!((carved_space[2].min().y - floor_surface_y - scaled_height(36.0)).abs() < 1.0e-4);

        let VoxelEdit::StampCuboids {
            cuboids: roofs,
            voxel_type,
            ..
        } = &plan.voxel_edits[2]
        else {
            panic!("expected roof cuboids");
        };
        assert_eq!(*voxel_type, VOXEL_TYPE_STUCCO);
        assert_eq!(roofs.len(), 1);
        assert_eq!(roofs[0].rotation(), glam::Quat::IDENTITY);
        assert!((roofs[0].height() - ROOF_THICKNESS).abs() < 1.0e-4);
        assert_eq!(roofs[0].width(), HOUSE_MAX_X - HOUSE_MIN_X + 12.0);
        assert_eq!(roofs[0].depth(), HOUSE_MAX_Z - HOUSE_MIN_Z + 12.0);
        let built_height = roofs[0].max().y - (ground_y as f32 + 1.0);
        let original_wall_and_roof_height = 96.0 + 9.0;
        assert!((built_height - original_wall_and_roof_height * HOUSE_HEIGHT_SCALE).abs() < 1.0e-4);

        assert!(plan.voxel_edits.iter().all(|edit| !matches!(
            edit,
            VoxelEdit::StampCuboids {
                voxel_type: VOXEL_TYPE_ROCK,
                ..
            }
        )));

        let VoxelEdit::StampCuboids {
            cuboids: planter,
            voxel_type: planter_type,
            ..
        } = &plan.voxel_edits[3]
        else {
            panic!("expected window planter cuboids");
        };
        assert_eq!(*planter_type, VOXEL_TYPE_STUCCO);
        assert_eq!(planter.len(), 4);
        assert!((planter[0].height() - scaled_height(3.0)).abs() < 1.0e-4);
        assert!((planter[1].height() - scaled_height(6.0)).abs() < 1.0e-4);

        let VoxelEdit::StampCuboids {
            cuboids: soil,
            voxel_type: soil_type,
            ..
        } = &plan.voxel_edits[4]
        else {
            panic!("expected planter soil");
        };
        assert_eq!(*soil_type, VOXEL_TYPE_DIRT);
        assert_eq!(soil.len(), 1);
        assert!(soil[0].min().x >= HOUSE_MAX_X);
        assert!(soil[0].max().x < HOUSE_MAX_X + 10.0);
        assert!((soil[0].height() - scaled_height(5.0)).abs() < 1.0e-4);
    }
}
