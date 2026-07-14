use crate::tracer::voxel_encoding::{
    append_indexed_cube_data, append_indexed_cube_data_with_info, FloraMeshData, FloraVoxelInfo,
    FLORA_VOXEL_MATERIAL_ALLIUM_CORE, FLORA_VOXEL_MATERIAL_GRADIENT,
};
use anyhow::Result;
use glam::{IVec2, IVec3, Vec2};
use std::collections::BTreeMap;

const KOCHIA_INNER_BRANCH_COUNT: usize = 6;
const KOCHIA_MIDDLE_BRANCH_COUNT: usize = 10;
const KOCHIA_OUTER_BRANCH_COUNT: usize = 12;
const KOCHIA_BRANCH_COUNT: usize =
    KOCHIA_INNER_BRANCH_COUNT + KOCHIA_MIDDLE_BRANCH_COUNT + KOCHIA_OUTER_BRANCH_COUNT;

fn gen_grass_column(voxel_count: u32, is_lod_used: bool) -> Result<FloraMeshData> {
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);
    let max_length = voxel_count - 1;

    let mut mesh = FloraMeshData::new(max_length);

    for i in 0..voxel_count {
        let vertex_offset = mesh.vertices.len() as u32;
        let base_pos = IVec3::new(0, i as i32, 0);

        append_indexed_cube_data(
            &mut mesh.vertices,
            &mut mesh.indices,
            &mut mesh.voxel_infos,
            base_pos,
            vertex_offset,
            ORIGIN,
            max_length,
            is_lod_used,
        )?;
    }

    Ok(mesh)
}

pub fn gen_tall_grass(is_lod_used: bool) -> Result<FloraMeshData> {
    gen_grass_column(8, is_lod_used)
}

pub fn gen_short_grass(is_lod_used: bool) -> Result<FloraMeshData> {
    gen_grass_column(4, is_lod_used)
}

pub fn gen_kochia(is_lod_used: bool) -> Result<FloraMeshData> {
    const MAX_HEIGHT: i32 = 14;
    const GOLDEN_ANGLE: f32 = 2.399_963_1;

    #[derive(Clone, Copy)]
    struct KochiaVoxel {
        pos: IVec3,
        branch_progress: f32,
        shell_floor: f32,
        animation_group: u8,
    }

    fn smootherstep(value: f32) -> f32 {
        let t = value.clamp(0.0, 1.0);
        t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    }

    let mut voxels = BTreeMap::<(i32, i32, i32), KochiaVoxel>::new();
    let mut insert_voxel = |voxel: KochiaVoxel| {
        voxels
            .entry((voxel.pos.x, voxel.pos.y, voxel.pos.z))
            .or_insert(voxel);
    };

    for branch_idx in 0..KOCHIA_BRANCH_COUNT {
        let angle = branch_idx as f32 * GOLDEN_ANGLE;
        let direction = Vec2::new(angle.cos(), angle.sin());
        let perpendicular = Vec2::new(-direction.y, direction.x);
        let jitter = ((branch_idx * 7) % 11) as f32 / 10.0;

        let middle_end = KOCHIA_INNER_BRANCH_COUNT + KOCHIA_MIDDLE_BRANCH_COUNT;
        let (height, target_radius, root_radius) = if branch_idx < KOCHIA_INNER_BRANCH_COUNT {
            (
                MAX_HEIGHT - (branch_idx % 2) as i32,
                0.8 + jitter * 1.1,
                0.4 + jitter * 0.9,
            )
        } else if branch_idx < middle_end {
            (
                11 + (branch_idx % 3) as i32,
                3.2 + jitter * 1.6,
                1.1 + jitter * 1.0,
            )
        } else {
            (
                9 + (branch_idx % 3) as i32,
                5.2 + jitter * 0.9,
                1.6 + jitter * 1.2,
            )
        };
        let root = IVec2::new(
            (direction.x * root_radius).round() as i32,
            (direction.y * root_radius).round() as i32,
        );
        let target = Vec2::new(direction.x * target_radius, direction.y * target_radius);
        let bow = (jitter - 0.5) * 0.9;
        let animation_group = branch_idx as u8 + 1;
        // Color follows whole branches rather than world height: short, wide branches form the
        // red side shell from root to tip, while tall inner branches expose a yellow core until
        // their tips join the red crown. Intermediate branches bridge the two smoothly.
        let shell_floor = smootherstep((target_radius - 2.2) / 3.0);

        for y in 0..height {
            let progress = y as f32 / (height - 1) as f32;
            let outward_t = progress.powf(1.35);
            let lateral = (std::f32::consts::PI * progress).sin() * bow;
            let root_f = root.as_vec2();
            let xz = root_f.lerp(target, outward_t) + perpendicular * lateral;
            let pos = IVec3::new(xz.x.round() as i32, y, xz.y.round() as i32);
            insert_voxel(KochiaVoxel {
                pos,
                branch_progress: progress,
                shell_floor,
                animation_group,
            });

            if progress > 0.45 && progress < 0.92 && (y as usize + branch_idx * 3) % 4 == 0 {
                let side_step = IVec2::new(
                    if direction.x > 0.35 {
                        1
                    } else if direction.x < -0.35 {
                        -1
                    } else {
                        0
                    },
                    if direction.y > 0.35 {
                        1
                    } else if direction.y < -0.35 {
                        -1
                    } else {
                        0
                    },
                );
                insert_voxel(KochiaVoxel {
                    pos: pos + IVec3::new(side_step.x, 0, side_step.y),
                    branch_progress: progress,
                    shell_floor,
                    animation_group,
                });
            }
        }
    }

    let max_length = voxels
        .values()
        .map(|voxel| voxel.pos.as_vec3().length())
        .fold(1.0_f32, f32::max)
        .ceil() as u32;
    let mut mesh = FloraMeshData::new(max_length);

    for voxel in voxels.values() {
        let tip_shell = smootherstep((voxel.branch_progress - 0.48) / 0.52);
        let shell_depth = voxel.shell_floor.max(tip_shell);
        let vertex_offset = mesh.vertices.len() as u32;
        append_indexed_cube_data_with_info(
            &mut mesh.vertices,
            &mut mesh.indices,
            &mut mesh.voxel_infos,
            voxel.pos,
            vertex_offset,
            FloraVoxelInfo::with_animation_group(
                shell_depth,
                voxel.branch_progress,
                voxel.branch_progress,
                FLORA_VOXEL_MATERIAL_GRADIENT,
                voxel.animation_group,
            ),
            is_lod_used,
        )?;
    }

    Ok(mesh)
}

pub fn gen_lavender(is_lod_used: bool) -> Result<FloraMeshData> {
    const STEM_VOXEL_COUNT: u32 = 6;
    const LEAF_BALL_RADIUS: f32 = 1.5;
    const LEAF_BALL_BOUNDARY: i32 = LEAF_BALL_RADIUS as i32;
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);

    let max_vertical = (STEM_VOXEL_COUNT + LEAF_BALL_BOUNDARY as u32) as f32;
    let max_horizontal = LEAF_BALL_BOUNDARY as f32;
    let max_length = ((max_vertical * max_vertical + 2.0 * max_horizontal * max_horizontal).sqrt())
        .ceil()
        .max(1.0) as u32;

    let mut mesh = FloraMeshData::new(max_length);

    // draw the stem
    let total_stem_voxel_count = STEM_VOXEL_COUNT - LEAF_BALL_BOUNDARY as u32;
    for i in 0..total_stem_voxel_count {
        let vertex_offset = mesh.vertices.len() as u32;
        let base_pos = IVec3::new(0, i as i32, 0);

        append_indexed_cube_data(
            &mut mesh.vertices,
            &mut mesh.indices,
            &mut mesh.voxel_infos,
            base_pos,
            vertex_offset,
            ORIGIN,
            max_length,
            is_lod_used,
        )?;
    }

    // draw the leaf ball at the top of the stem
    for i in -LEAF_BALL_BOUNDARY..=LEAF_BALL_BOUNDARY {
        for j in -LEAF_BALL_BOUNDARY..=LEAF_BALL_BOUNDARY {
            for k in -LEAF_BALL_BOUNDARY..=LEAF_BALL_BOUNDARY {
                if i * i + j * j + k * k > LEAF_BALL_BOUNDARY * LEAF_BALL_BOUNDARY {
                    continue;
                }

                let vertex_offset = mesh.vertices.len() as u32;
                let base_pos = IVec3::new(i, j, k) + IVec3::new(0, STEM_VOXEL_COUNT as i32, 0);

                append_indexed_cube_data(
                    &mut mesh.vertices,
                    &mut mesh.indices,
                    &mut mesh.voxel_infos,
                    base_pos,
                    vertex_offset,
                    ORIGIN,
                    max_length,
                    is_lod_used,
                )?;
            }
        }
    }

    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn kochia_mesh_has_a_readable_branched_silhouette() {
        let mesh = gen_kochia(false).unwrap();
        let positions = mesh
            .voxel_infos
            .iter()
            .map(|entry| entry.pos)
            .collect::<Vec<_>>();
        let unique_positions = positions.iter().copied().collect::<HashSet<_>>();

        assert_eq!(unique_positions.len(), positions.len());
        assert!(
            (260..=380).contains(&positions.len()),
            "{} voxels",
            positions.len()
        );
        assert_eq!(positions.iter().map(|pos| pos.y).min(), Some(0));
        assert_eq!(positions.iter().map(|pos| pos.y).max(), Some(13));
        let x_span = positions.iter().map(|pos| pos.x).max().unwrap()
            - positions.iter().map(|pos| pos.x).min().unwrap()
            + 1;
        let z_span = positions.iter().map(|pos| pos.z).max().unwrap()
            - positions.iter().map(|pos| pos.z).min().unwrap()
            + 1;
        assert!((13..=15).contains(&x_span), "x span {x_span}");
        assert!((13..=15).contains(&z_span), "z span {z_span}");

        let roots = positions
            .iter()
            .filter(|pos| pos.y == 0)
            .map(|pos| IVec2::new(pos.x, pos.z))
            .collect::<HashSet<_>>();
        assert!(roots.len() >= 8, "only {} distinct roots", roots.len());

        let outer_max_height = positions
            .iter()
            .filter(|pos| pos.x.abs().max(pos.z.abs()) >= 4)
            .map(|pos| pos.y)
            .max()
            .unwrap();
        assert!(outer_max_height < 13);
    }

    #[test]
    fn kochia_voxels_retain_branch_animation_groups() {
        let mesh = gen_kochia(false).unwrap();
        let groups = mesh
            .voxel_infos
            .iter()
            .map(|entry| entry.info.animation_group())
            .collect::<HashSet<_>>();

        assert!(
            groups.len() >= 20,
            "only {} surviving branch groups",
            groups.len()
        );
        assert!(!groups.contains(&0));
    }

    #[test]
    fn kochia_color_shell_follows_branches() {
        let mesh = gen_kochia(false).unwrap();
        let color_gradient = |entry: &crate::tracer::voxel_encoding::FloraVoxelInfoEntry| {
            (entry.info.packed & 0xff) as f32 / 255.0
        };

        let outer_gradients = mesh
            .voxel_infos
            .iter()
            .filter(|entry| {
                entry.info.animation_group() as usize
                    > KOCHIA_INNER_BRANCH_COUNT + KOCHIA_MIDDLE_BRANCH_COUNT
            })
            .map(color_gradient)
            .collect::<Vec<_>>();
        assert!(!outer_gradients.is_empty());
        let outer_branch_min = outer_gradients.into_iter().fold(1.0_f32, f32::min);
        assert!(outer_branch_min > 0.95, "outer minimum {outer_branch_min}");

        let inner_gradients = mesh
            .voxel_infos
            .iter()
            .filter(|entry| entry.info.animation_group() as usize <= KOCHIA_INNER_BRANCH_COUNT)
            .map(color_gradient)
            .collect::<Vec<_>>();
        assert!(inner_gradients.iter().any(|gradient| *gradient < 0.05));
        assert!(inner_gradients.iter().any(|gradient| *gradient > 0.95));

        let lower_gradients = mesh
            .voxel_infos
            .iter()
            .filter(|entry| entry.pos.y <= 2)
            .map(color_gradient)
            .collect::<Vec<_>>();
        assert!(lower_gradients.iter().any(|gradient| *gradient < 0.05));
        assert!(lower_gradients.iter().any(|gradient| *gradient > 0.95));
    }
}

pub fn gen_ember_bloom(is_lod_used: bool) -> Result<FloraMeshData> {
    // Keep the legacy generator/key name so existing saves still resolve this species.
    // Visually this is now a tall ornamental allium: a slender stalk topped by a
    // blocky globe made from a dense core and a few individual florets.
    const STEM_HEIGHT: i32 = 12;
    const FLOWER_CENTER_Y: i32 = 14;
    const FLOWER_RADIUS: i32 = 2;
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);

    let max_vertical = (FLOWER_CENTER_Y + FLOWER_RADIUS) as f32;
    let max_horizontal = FLOWER_RADIUS as f32;
    let max_length = ((max_vertical * max_vertical + 2.0 * max_horizontal * max_horizontal).sqrt())
        .ceil()
        .max(1.0) as u32;
    let mut mesh = FloraMeshData::new(max_length);

    let mut append_voxel = |pos: IVec3, material_id: u8| -> Result<()> {
        let vertex_offset = mesh.vertices.len() as u32;
        let gradient = (pos - ORIGIN).as_vec3().length() / max_length as f32;
        append_indexed_cube_data_with_info(
            &mut mesh.vertices,
            &mut mesh.indices,
            &mut mesh.voxel_infos,
            pos,
            vertex_offset,
            FloraVoxelInfo::new(gradient, gradient, gradient, material_id),
            is_lod_used,
        )
    };

    for y in 0..STEM_HEIGHT {
        append_voxel(IVec3::new(0, y, 0), FLORA_VOXEL_MATERIAL_GRADIENT)?;
    }

    for y in -FLOWER_RADIUS..=FLOWER_RADIUS {
        for x in -FLOWER_RADIUS..=FLOWER_RADIUS {
            for z in -FLOWER_RADIUS..=FLOWER_RADIUS {
                let distance_sq = x * x + y * y + z * z;
                let pos = IVec3::new(x, FLOWER_CENTER_Y + y, z);
                if distance_sq <= 4 {
                    append_voxel(pos, FLORA_VOXEL_MATERIAL_ALLIUM_CORE)?;
                }
            }
        }
    }

    Ok(mesh)
}
