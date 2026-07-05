use crate::branch_skeleton::{generate_branch_skeleton, BranchingDesc};
use crate::tracer::voxel_encoding::{append_indexed_cube_data, FloraMeshData};
use anyhow::Result;
use glam::{IVec3, Vec3};
use std::{collections::HashSet, f32::consts::PI};

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

pub fn gen_carrot(is_lod_used: bool) -> Result<FloraMeshData> {
    const BURIED_TIP_Y: i32 = -4;
    const ORIGIN: IVec3 = IVec3::new(0, BURIED_TIP_Y, 0);
    const LEAF_BASE_Y: i32 = 2;
    const LEAF_HEIGHT: i32 = 4;
    const MAX_LENGTH: u32 = 9;

    let mut mesh = FloraMeshData::new(MAX_LENGTH);

    // Ten voxels tall from buried tip y=-4 to leaf tip y=5. Most orange root voxels sit below
    // the soil; y=0..1 leaves a small carrot shoulder visible above ground.
    const ROOT_LAYERS: &[(i32, i32)] = &[(-4, 0), (-3, 0), (-2, 1), (-1, 1), (0, 1), (1, 0)];
    for &(y, radius) in ROOT_LAYERS {
        for x in -radius..=radius {
            for z in -radius..=radius {
                if x.abs() + z.abs() > radius + 1 {
                    continue;
                }
                let vertex_offset = mesh.vertices.len() as u32;
                append_indexed_cube_data(
                    &mut mesh.vertices,
                    &mut mesh.indices,
                    &mut mesh.voxel_infos,
                    IVec3::new(x, y, z),
                    vertex_offset,
                    ORIGIN,
                    MAX_LENGTH,
                    is_lod_used,
                )?;
            }
        }
    }

    // Leafy tufts start above the exposed orange shoulder and reach up to y=5.
    const LEAF_TUFTS: &[(i32, i32)] = &[(0, 0), (-1, 0), (1, 1), (0, -1)];
    for &(base_x, base_z) in LEAF_TUFTS {
        for y in 0..LEAF_HEIGHT {
            let lean_x = base_x * y / 3;
            let lean_z = base_z * y / 3;
            let vertex_offset = mesh.vertices.len() as u32;
            append_indexed_cube_data(
                &mut mesh.vertices,
                &mut mesh.indices,
                &mut mesh.voxel_infos,
                IVec3::new(base_x + lean_x, LEAF_BASE_Y + y, base_z + lean_z),
                vertex_offset,
                ORIGIN,
                MAX_LENGTH,
                is_lod_used,
            )?;
        }
    }

    Ok(mesh)
}

type TomatoVoxelSet = HashSet<(i32, i32, i32)>;

fn push_unique_tomato_voxel(voxels: &mut Vec<IVec3>, occupied: &mut TomatoVoxelSet, pos: IVec3) {
    if occupied.insert((pos.x, pos.y, pos.z)) {
        voxels.push(pos);
    }
}

fn push_tomato_line(
    voxels: &mut Vec<IVec3>,
    occupied: &mut TomatoVoxelSet,
    start: IVec3,
    end: IVec3,
) {
    let delta = end - start;
    let steps = delta.x.abs().max(delta.y.abs()).max(delta.z.abs()).max(1);

    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let pos = IVec3::new(
            (start.x as f32 + delta.x as f32 * t).round() as i32,
            (start.y as f32 + delta.y as f32 * t).round() as i32,
            (start.z as f32 + delta.z as f32 * t).round() as i32,
        );
        push_unique_tomato_voxel(voxels, occupied, pos);
    }
}

fn round_vec3_to_ivec3(pos: Vec3) -> IVec3 {
    IVec3::new(
        pos.x.round() as i32,
        pos.y.round() as i32,
        pos.z.round() as i32,
    )
}

pub const TOMATO_BRANCHING_SEED_HIGH_BITS: u64 = 0x746f_0000_0000;
pub const TOMATO_BRANCHING_SEED_LOW_BITS: u32 = 0x6d61_746f;

pub fn default_tomato_branching_desc() -> BranchingDesc {
    BranchingDesc {
        // Fixed species-level seed: the authored tomato has procedural shape, but every placed
        // instance still shares this exact mesh until we intentionally add per-instance variants.
        seed: TOMATO_BRANCHING_SEED_HIGH_BITS | TOMATO_BRANCHING_SEED_LOW_BITS as u64,
        iterations: 5,
        initial_length: 4.0,
        length_dropoff: 0.76,
        spread: 0.08,
        randomness: 0.18,
        vertical_tendency: 0.48,
        branch_angle_min: 30.0_f32.to_radians(),
        branch_angle_max: 58.0_f32.to_radians(),
        branch_probability: 0.92,
        branch_count_min: 2,
        branch_count_max: 3,
        segment_length_variation: 0.10,
    }
}

fn tomato_vine_voxels(branching_desc: &BranchingDesc) -> Vec<IVec3> {
    let skeleton = generate_branch_skeleton(branching_desc);
    let mut voxels = Vec::new();
    let mut occupied = TomatoVoxelSet::new();

    // First pass: only the vine skeleton. Keep the root and every branch one voxel thick so the
    // silhouette is easy to tune before adding leaves or fruit in later passes.
    for segment in skeleton.segments {
        push_tomato_line(
            &mut voxels,
            &mut occupied,
            round_vec3_to_ivec3(segment.start),
            round_vec3_to_ivec3(segment.end),
        );
    }

    voxels
}

fn tomato_max_length(voxels: &[IVec3], origin: IVec3) -> u32 {
    voxels
        .iter()
        .map(|pos| (*pos - origin).as_vec3().length().ceil() as u32)
        .max()
        .unwrap_or(1)
        .max(1)
}

pub fn gen_tomato_with_branching_desc(
    branching_desc: &BranchingDesc,
    is_lod_used: bool,
) -> Result<FloraMeshData> {
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);

    let voxels = tomato_vine_voxels(branching_desc);
    let max_length = tomato_max_length(&voxels, ORIGIN);
    let mut mesh = FloraMeshData::new(max_length);

    for pos in voxels {
        let vertex_offset = mesh.vertices.len() as u32;
        append_indexed_cube_data(
            &mut mesh.vertices,
            &mut mesh.indices,
            &mut mesh.voxel_infos,
            pos,
            vertex_offset,
            ORIGIN,
            max_length,
            is_lod_used,
        )?;
    }

    Ok(mesh)
}

pub fn gen_tomato(is_lod_used: bool) -> Result<FloraMeshData> {
    gen_tomato_with_branching_desc(&default_tomato_branching_desc(), is_lod_used)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tomato_vine_has_one_ground_voxel() {
        let desc = default_tomato_branching_desc();
        let voxels = tomato_vine_voxels(&desc);

        assert!(voxels.contains(&IVec3::ZERO));
        assert_eq!(voxels.iter().filter(|pos| pos.y == 0).count(), 1);
        assert!(voxels.iter().all(|pos| pos.y >= 0));
    }

    #[test]
    fn tomato_vine_stays_in_half_height_scale() {
        let desc = default_tomato_branching_desc();
        let voxels = tomato_vine_voxels(&desc);
        let max_y = voxels.iter().map(|pos| pos.y).max().unwrap_or(0);

        assert!((7..=13).contains(&max_y), "max_y was {max_y}");
    }
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

pub fn gen_ember_bloom(is_lod_used: bool) -> Result<FloraMeshData> {
    const HEIGHT: i32 = 12;
    // Width Configuration: How wide the plant swells
    const MAX_RADIUS: f32 = 2.0;
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);

    let max_vertical = (HEIGHT - 1) as f32;
    let max_horizontal = (MAX_RADIUS + 2.0).ceil(); // includes search padding
    let max_length = ((max_vertical * max_vertical + 2.0 * max_horizontal * max_horizontal).sqrt())
        .ceil()
        .max(1.0) as u32;

    let mut mesh = FloraMeshData::new(max_length);

    for y in 0..HEIGHT {
        // Normalized height (0.0 at bottom, 1.0 at top)
        let t = y as f32 / HEIGHT as f32;

        // Vertical Profile:
        // Uses a sine wave to create a soft bulb shape.
        // It starts small, swells wide in the middle, and tapers at the top.
        // We add 0.5 base radius so it doesn't disappear completely at the very bottom.
        let vertical_swell = (t * PI).sin();
        let base_radius = 0.5 + (vertical_swell * MAX_RADIUS);

        // Define search area for this layer
        let search_radius = (base_radius + 1.5).ceil() as i32;

        for x in -search_radius..=search_radius {
            for z in -search_radius..=search_radius {
                // Calculate distance from center (0,0)
                let dist_sq = (x * x + z * z) as f32;
                let dist = dist_sq.sqrt();

                // Calculate Angle for the "Wavy" texture
                let angle = (z as f32).atan2(x as f32);

                // Wavy/Leafy Logic:
                // We use cos(angle * 6.0) to create 6 gentle lobes (leaves) wrapping around.
                // 'lobe_depth' controls how deep the ridges are.
                let lobe_depth = 0.6;
                let wave_modifier = (angle * 6.0).cos() * lobe_depth;

                // The effective radius limit at this specific angle
                let radius_limit = base_radius + wave_modifier;

                // Solid Fill Logic:
                // We fill everything inside the calculated radius.
                // This guarantees the shape is symmetrical and has no holes.
                if dist <= radius_limit {
                    let vertex_offset = mesh.vertices.len() as u32;

                    // No stem sway, just straight up for symmetry
                    let pos = IVec3::new(x, y, z);

                    append_indexed_cube_data(
                        &mut mesh.vertices,
                        &mut mesh.indices,
                        &mut mesh.voxel_infos,
                        pos,
                        vertex_offset,
                        ORIGIN,
                        max_length,
                        is_lod_used,
                    )?;
                }
            }
        }
    }

    Ok(mesh)
}
