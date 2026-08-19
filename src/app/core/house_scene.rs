use super::App;
use crate::app::world_edits::{VoxelEdit, WorldEditPlan};
use crate::builder::{
    pack_voxel_atlas_byte, voxel_type_from_atlas_byte, PlainBuilder, VOXEL_TYPE_CHERRY_WOOD,
    VOXEL_TYPE_DIRT, VOXEL_TYPE_EMPTY, VOXEL_TYPE_OAK_WOOD, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND,
};
use crate::geom::{build_bvh, Cuboid};
use anyhow::{Context, Result};
use glam::{UVec3, Vec3};

// All scene dimensions are terrain voxels. One world unit is 256 terrain voxels.
const GRADE_OFFSET: UVec3 = UVec3::new(4, 32, 134);
const GRADE_DIM: UVec3 = UVec3::new(326, 224, 350);
const FLAT_MIN_X: u32 = 100;
const FLAT_MAX_X: u32 = 234;
const FLAT_MIN_Z: u32 = 230;
const FLAT_MAX_Z: u32 = 388;
const GRADE_BLEND_WIDTH: f32 = 96.0;
const TOPSOIL_DEPTH: u32 = 25;

const HOUSE_MIN_X: f32 = 112.0;
const HOUSE_MAX_X: f32 = 222.0;
const HOUSE_MIN_Z: f32 = 242.0;
const HOUSE_MAX_Z: f32 = 376.0;
const WALL_HEIGHT: f32 = 96.0;
const WALL_THICKNESS: f32 = 9.0;
const ROOF_OVERHANG: f32 = 8.0;
const ROOF_THICKNESS: f32 = 9.0;
const CHIMNEY_HEIGHT: f32 = 38.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LandscapeGradeReport {
    target_surface_y: u32,
    min_original_surface_y: u32,
    max_original_surface_y: u32,
    changed_voxels: usize,
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

fn rectangle_distance(x: u32, z: u32) -> f32 {
    let dx = if x < FLAT_MIN_X {
        (FLAT_MIN_X - x) as f32
    } else if x > FLAT_MAX_X {
        (x - FLAT_MAX_X) as f32
    } else {
        0.0
    };
    let dz = if z < FLAT_MIN_Z {
        (FLAT_MIN_Z - z) as f32
    } else if z > FLAT_MAX_Z {
        (z - FLAT_MAX_Z) as f32
    } else {
        0.0
    };
    dx.hypot(dz)
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn graded_surface_y(original_y: u32, target_y: u32, distance: f32) -> u32 {
    let weight = smoothstep01(1.0 - distance / GRADE_BLEND_WIDTH);
    (original_y as f32 + (target_y as f32 - original_y as f32) * weight).round() as u32
}

fn find_surface_heights(bytes: &[u8]) -> Result<Vec<u32>> {
    let mut heights = Vec::with_capacity((GRADE_DIM.x * GRADE_DIM.z) as usize);
    for z in 0..GRADE_DIM.z {
        for x in 0..GRADE_DIM.x {
            let surface = (0..GRADE_DIM.y).rev().find_map(|y| {
                let voxel_type =
                    voxel_type_from_atlas_byte(bytes[volume_index(UVec3::new(x, y, z), GRADE_DIM)]);
                is_terrain_voxel(voxel_type).then_some(GRADE_OFFSET.y + y)
            });
            heights.push(surface.with_context(|| {
                format!(
                    "house landscape column has no terrain at x={} z={}",
                    GRADE_OFFSET.x + x,
                    GRADE_OFFSET.z + z
                )
            })?);
        }
    }
    Ok(heights)
}

fn surface_height(heights: &[u32], local_x: u32, local_z: u32) -> u32 {
    heights[(local_z * GRADE_DIM.x + local_x) as usize]
}

fn median_house_surface(heights: &[u32]) -> u32 {
    let mut footprint = Vec::new();
    for world_z in FLAT_MIN_Z..=FLAT_MAX_Z {
        for world_x in FLAT_MIN_X..=FLAT_MAX_X {
            footprint.push(surface_height(
                heights,
                world_x - GRADE_OFFSET.x,
                world_z - GRADE_OFFSET.z,
            ));
        }
    }
    footprint.sort_unstable();
    footprint[footprint.len() / 2]
}

fn grade_landscape(plain_builder: &mut PlainBuilder) -> Result<LandscapeGradeReport> {
    let mut bytes = plain_builder
        .read_chunk_atlas_region(GRADE_OFFSET, GRADE_DIM)
        .context("read house landscape terrain")?;
    let heights = find_surface_heights(&bytes)?;
    let target_surface_y = median_house_surface(&heights);
    let min_original_surface_y = *heights.iter().min().context("empty house landscape")?;
    let max_original_surface_y = *heights.iter().max().context("empty house landscape")?;
    let mut changed_voxels = 0usize;

    for local_z in 0..GRADE_DIM.z {
        for local_x in 0..GRADE_DIM.x {
            let world_x = GRADE_OFFSET.x + local_x;
            let world_z = GRADE_OFFSET.z + local_z;
            let distance = rectangle_distance(world_x, world_z);
            if distance >= GRADE_BLEND_WIDTH {
                continue;
            }

            let original_surface_y = surface_height(&heights, local_x, local_z);
            let new_surface_y = graded_surface_y(original_surface_y, target_surface_y, distance);
            for local_y in 0..GRADE_DIM.y {
                let world_y = GRADE_OFFSET.y + local_y;
                let index = volume_index(UVec3::new(local_x, local_y, local_z), GRADE_DIM);
                let old = bytes[index];
                let old_type = voxel_type_from_atlas_byte(old);
                let new = if world_y <= new_surface_y {
                    let depth = new_surface_y - world_y;
                    let fill_type = if depth < TOPSOIL_DEPTH {
                        VOXEL_TYPE_DIRT
                    } else {
                        VOXEL_TYPE_ROCK
                    };
                    if is_terrain_voxel(old_type) && old_type == fill_type as u8 {
                        old
                    } else {
                        pack_voxel_atlas_byte(fill_type as u8, 0)
                    }
                } else {
                    pack_voxel_atlas_byte(VOXEL_TYPE_EMPTY as u8, 0)
                };
                if new != old {
                    bytes[index] = new;
                    changed_voxels += 1;
                }
            }
        }
    }

    plain_builder
        .write_chunk_atlas_region(GRADE_OFFSET, GRADE_DIM, &bytes)
        .context("write graded house landscape")?;

    Ok(LandscapeGradeReport {
        target_surface_y,
        min_original_surface_y,
        max_original_surface_y,
        changed_voxels,
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

fn house_plan(ground_surface_y: u32) -> Result<WorldEditPlan> {
    let base_y = ground_surface_y as f32 + 1.0;
    let wall_top_y = base_y + WALL_HEIGHT;
    let roof_top_y = wall_top_y + ROOF_THICKNESS;

    let outer_shell = box_at(
        Vec3::new(HOUSE_MIN_X, base_y, HOUSE_MIN_Z),
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
            Vec3::new(180.0, base_y + 69.0, HOUSE_MAX_Z + 1.0),
        ),
        box_at(
            Vec3::new(190.0, base_y + 36.0, HOUSE_MAX_Z - WALL_THICKNESS - 1.0),
            Vec3::new(213.0, base_y + 68.0, HOUSE_MAX_Z + 1.0),
        ),
        // Broad side windows keep the small interior naturally lit.
        box_at(
            Vec3::new(HOUSE_MAX_X - WALL_THICKNESS - 1.0, base_y + 34.0, 278.0),
            Vec3::new(HOUSE_MAX_X + 1.0, base_y + 70.0, 338.0),
        ),
        box_at(
            Vec3::new(HOUSE_MIN_X - 1.0, base_y + 34.0, 278.0),
            Vec3::new(HOUSE_MIN_X + WALL_THICKNESS + 1.0, base_y + 70.0, 338.0),
        ),
        box_at(
            Vec3::new(147.0, base_y + 36.0, HOUSE_MIN_Z - 1.0),
            Vec3::new(188.0, base_y + 68.0, HOUSE_MIN_Z + WALL_THICKNESS + 1.0),
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
    let chimney = box_at(
        Vec3::new(184.0, roof_top_y, 274.0),
        Vec3::new(199.0, roof_top_y + CHIMNEY_HEIGHT, 291.0),
    );

    Ok(WorldEditPlan {
        voxel_edits: vec![
            stamp_cuboids(vec![outer_shell], VOXEL_TYPE_OAK_WOOD)?,
            stamp_cuboids(
                std::iter::once(hollow_interior).chain(openings).collect(),
                VOXEL_TYPE_EMPTY,
            )?,
            stamp_cuboids(vec![flat_roof], VOXEL_TYPE_CHERRY_WOOD)?,
            stamp_cuboids(vec![chimney], VOXEL_TYPE_ROCK)?,
        ],
        // Loading publishes every chunk after applying this plan.
        build_edits: Vec::new(),
    })
}

impl App {
    pub(super) fn apply_house_scene(&mut self) -> Result<()> {
        let grade = grade_landscape(&mut self.plain_builder)?;
        self.execute_edit_plan(house_plan(grade.target_surface_y)?)?;
        self.plain_builder.mark_all_solid_workgroups_dirty();
        log::info!(
            "[HOUSE_SCENE] built flat-roof house ground_y={} original_surface_y={}..{} changed_landscape_voxels={} flat_x={}..{} flat_z={}..{} blend_width={}",
            grade.target_surface_y,
            grade.min_original_surface_y,
            grade.max_original_surface_y,
            grade.changed_voxels,
            FLAT_MIN_X,
            FLAT_MAX_X,
            FLAT_MIN_Z,
            FLAT_MAX_Z,
            GRADE_BLEND_WIDTH,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landscape_grade_is_flat_in_core_and_unchanged_at_blend_edge() {
        assert_eq!(graded_surface_y(75, 100, 0.0), 100);
        assert_eq!(graded_surface_y(75, 100, GRADE_BLEND_WIDTH), 75);
        let halfway = graded_surface_y(75, 100, GRADE_BLEND_WIDTH * 0.5);
        assert_eq!(halfway, 88);
    }

    #[test]
    fn house_uses_one_axis_aligned_flat_roof_and_no_foundation() {
        let ground_y = 100;
        let plan = house_plan(ground_y).unwrap();

        let VoxelEdit::StampCuboids {
            cuboids: roofs,
            voxel_type,
            ..
        } = &plan.voxel_edits[2]
        else {
            panic!("expected roof cuboids");
        };
        assert_eq!(*voxel_type, VOXEL_TYPE_CHERRY_WOOD);
        assert_eq!(roofs.len(), 1);
        assert_eq!(roofs[0].rotation(), glam::Quat::IDENTITY);
        assert_eq!(roofs[0].height(), ROOF_THICKNESS);

        let VoxelEdit::StampCuboids {
            cuboids: rock,
            voxel_type,
            ..
        } = &plan.voxel_edits[3]
        else {
            panic!("expected chimney cuboids");
        };
        assert_eq!(*voxel_type, VOXEL_TYPE_ROCK);
        assert!(rock.iter().all(|cuboid| cuboid.min().y > ground_y as f32));
    }
}
