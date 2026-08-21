use super::App;
use crate::app::world_edits::{VoxelEdit, WorldEditPlan};
use crate::builder::{
    voxel_type_from_atlas_byte, PlainBuilder, VOXEL_TYPE_DIRT, VOXEL_TYPE_EMPTY, VOXEL_TYPE_ROCK,
    VOXEL_TYPE_SAND, VOXEL_TYPE_STUCCO,
};
use crate::geom::{build_bvh, Cuboid};
use anyhow::{Context, Result};
use glam::{Quat, UVec3, Vec3};

// All scene dimensions are terrain voxels. One world unit is 256 terrain voxels.
const HOUSE_MIN_X: f32 = 112.0;
const HOUSE_MAX_X: f32 = 222.0;
const HOUSE_MIN_Z: f32 = 242.0;
const HOUSE_MAX_Z: f32 = 376.0;
const SURFACE_SAMPLE_OFFSET: UVec3 = UVec3::new(112, 32, 242);
const SURFACE_SAMPLE_DIM: UVec3 = UVec3::new(111, 224, 135);
const WALL_THICKNESS: f32 = 8.0;
const ROOF_OVERHANG: f32 = 6.0;
const ROOF_THICKNESS: f32 = 5.0;
const A_FRAME_RISE: f32 = 72.0;
const GABLE_LAYER_HEIGHT: f32 = 2.0;
const ROUND_DOOR_RADIUS: f32 = 18.0;

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

fn roof_panel(start: Vec3, end: Vec3, min_z: f32, max_z: f32) -> Cuboid {
    let run = end - start;
    let length = Vec3::new(run.x, run.y, 0.0).length();
    let rotation = Quat::from_rotation_z(run.y.atan2(run.x));
    Cuboid::new_oriented(
        Vec3::new(
            (start.x + end.x) * 0.5,
            (start.y + end.y) * 0.5,
            (min_z + max_z) * 0.5,
        ),
        Vec3::new(length * 0.5, ROOF_THICKNESS * 0.5, (max_z - min_z) * 0.5),
        rotation,
    )
}

fn gable_end_layers(eave_y: f32, ridge_y: f32, min_z: f32, max_z: f32) -> Vec<Cuboid> {
    let center_x = (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5;
    let half_width = (HOUSE_MAX_X - HOUSE_MIN_X) * 0.5;
    let mut layers = Vec::new();
    let mut layer_bottom = eave_y;
    while layer_bottom < ridge_y {
        let layer_top = (layer_bottom + GABLE_LAYER_HEIGHT).min(ridge_y);
        let layer_center_y = (layer_bottom + layer_top) * 0.5;
        let height_fraction = ((layer_center_y - eave_y) / (ridge_y - eave_y)).clamp(0.0, 1.0);
        let layer_half_width = half_width * (1.0 - height_fraction);
        if layer_half_width > f32::EPSILON {
            layers.push(box_at(
                Vec3::new(center_x - layer_half_width, layer_bottom, min_z),
                Vec3::new(center_x + layer_half_width, layer_top, max_z),
            ));
        }
        layer_bottom = layer_top;
    }
    layers
}

fn round_door_opening(base_y: f32) -> Vec<Cuboid> {
    let center_x = (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5;
    let center_y = base_y + ROUND_DOOR_RADIUS;
    let mut slices = Vec::new();
    let mut slice_bottom = base_y;
    let top = base_y + ROUND_DOOR_RADIUS * 2.0;
    while slice_bottom < top {
        let slice_top = (slice_bottom + 1.0).min(top);
        let sample_y = (slice_bottom + slice_top) * 0.5;
        let half_width = (ROUND_DOOR_RADIUS.powi(2) - (sample_y - center_y).powi(2))
            .max(0.0)
            .sqrt();
        slices.push(box_at(
            Vec3::new(
                center_x - half_width,
                slice_bottom,
                HOUSE_MAX_Z - WALL_THICKNESS - 1.0,
            ),
            Vec3::new(center_x + half_width, slice_top, HOUSE_MAX_Z + 1.0),
        ));
        slice_bottom = slice_top;
    }
    slices
}

fn house_plan(surface: SurfaceSampleReport) -> Result<WorldEditPlan> {
    let base_y = surface.median_y as f32 + 1.0;
    let eave_y = surface.min_y as f32 + 1.0;
    let ridge_y = base_y + A_FRAME_RISE;
    let center_x = (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5;

    // Extend the floor into the uneven natural terrain while leaving its top at indoor grade.
    let floor_bottom_y = eave_y.min(base_y - 1.0);
    let floor = box_at(
        Vec3::new(HOUSE_MIN_X, floor_bottom_y, HOUSE_MIN_Z),
        Vec3::new(HOUSE_MAX_X, base_y, HOUSE_MAX_Z),
    );
    let mut gables = gable_end_layers(eave_y, ridge_y, HOUSE_MIN_Z, HOUSE_MIN_Z + WALL_THICKNESS);
    gables.extend(gable_end_layers(
        eave_y,
        ridge_y,
        HOUSE_MAX_Z - WALL_THICKNESS,
        HOUSE_MAX_Z,
    ));
    let roof_min_z = HOUSE_MIN_Z - ROOF_OVERHANG;
    let roof_max_z = HOUSE_MAX_Z + ROOF_OVERHANG;
    let roofs = vec![
        roof_panel(
            Vec3::new(HOUSE_MIN_X - ROOF_OVERHANG, eave_y, 0.0),
            Vec3::new(center_x, ridge_y, 0.0),
            roof_min_z,
            roof_max_z,
        ),
        roof_panel(
            Vec3::new(center_x, ridge_y, 0.0),
            Vec3::new(HOUSE_MAX_X + ROOF_OVERHANG, eave_y, 0.0),
            roof_min_z,
            roof_max_z,
        ),
    ];

    Ok(WorldEditPlan {
        voxel_edits: vec![
            stamp_cuboids(vec![floor], VOXEL_TYPE_STUCCO)?,
            stamp_cuboids(gables, VOXEL_TYPE_STUCCO)?,
            stamp_cuboids(roofs, VOXEL_TYPE_STUCCO)?,
            stamp_cuboids(round_door_opening(base_y), VOXEL_TYPE_EMPTY)?,
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
            "[HOUSE_SCENE] built A-frame house with ground-level eaves and round door on unchanged natural terrain ground_y={} footprint_surface_y={}..{} ridge_rise={} roof_thickness={} roof_overhang={}",
            surface.median_y,
            surface.min_y,
            surface.max_y,
            A_FRAME_RISE,
            ROOF_THICKNESS,
            ROOF_OVERHANG,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn house_is_a_stucco_a_frame_with_ground_level_eaves_and_round_door() {
        let ground_y = 100;
        let min_y = ground_y - 4;
        let plan = house_plan(SurfaceSampleReport {
            median_y: ground_y,
            min_y,
            max_y: ground_y + 4,
        })
        .unwrap();

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
        assert_eq!(floors[0].min().y, min_y as f32 + 1.0);
        assert_eq!(floors[0].max().y, ground_y as f32 + 1.0);

        let VoxelEdit::StampCuboids {
            cuboids: gables,
            voxel_type: gable_type,
            ..
        } = &plan.voxel_edits[1]
        else {
            panic!("expected stepped gable ends");
        };
        assert_eq!(*gable_type, VOXEL_TYPE_STUCCO);
        assert!(gables.len() > 2);
        assert!(gables[0].width() > gables[1].width());

        let VoxelEdit::StampCuboids {
            cuboids: roofs,
            voxel_type,
            ..
        } = &plan.voxel_edits[2]
        else {
            panic!("expected roof cuboids");
        };
        assert_eq!(*voxel_type, VOXEL_TYPE_STUCCO);
        assert_eq!(roofs.len(), 2);
        let left_slope = roofs[0].rotation() * Vec3::X;
        let right_slope = roofs[1].rotation() * Vec3::X;
        assert!(left_slope.x > 0.0 && left_slope.y > 0.0);
        assert!(right_slope.x > 0.0 && right_slope.y < 0.0);
        assert!((roofs[0].height() - ROOF_THICKNESS).abs() < 1.0e-4);
        assert_eq!(roofs[0].depth(), HOUSE_MAX_Z - HOUSE_MIN_Z + 12.0);
        assert!(roofs.iter().all(|roof| roof.min().y <= min_y as f32 + 1.0));
        assert!(roofs
            .iter()
            .all(|roof| roof.max().y >= ground_y as f32 + 1.0 + A_FRAME_RISE));

        assert!(plan.voxel_edits.iter().all(|edit| !matches!(
            edit,
            VoxelEdit::StampCuboids {
                voxel_type: VOXEL_TYPE_ROCK,
                ..
            }
        )));

        let VoxelEdit::StampCuboids {
            cuboids: door_slices,
            voxel_type: door_type,
            ..
        } = &plan.voxel_edits[3]
        else {
            panic!("expected round door opening slices");
        };
        assert_eq!(*door_type, VOXEL_TYPE_EMPTY);
        assert_eq!(door_slices.len(), (ROUND_DOOR_RADIUS * 2.0) as usize);
        assert!(door_slices[0].width() < door_slices[door_slices.len() / 2].width());
        assert!(door_slices.last().unwrap().width() < door_slices[door_slices.len() / 2].width());
        assert!(door_slices
            .iter()
            .all(|slice| (slice.center().x - (HOUSE_MIN_X + HOUSE_MAX_X) * 0.5).abs() < 1.0e-4));
    }
}
