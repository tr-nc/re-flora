use crate::tracer::voxel_encoding::append_indexed_cube_data;
use crate::tracer::Vertex;
use anyhow::Result;
use glam::IVec3;
use std::{collections::HashSet, f32::consts::PI};

fn gen_grass_column(voxel_count: u32, is_lod_used: bool) -> Result<(Vec<Vertex>, Vec<u32>)> {
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);
    let max_length = voxel_count - 1;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..voxel_count {
        let vertex_offset = vertices.len() as u32;
        let base_pos = IVec3::new(0, i as i32, 0);

        append_indexed_cube_data(
            &mut vertices,
            &mut indices,
            base_pos,
            vertex_offset,
            ORIGIN,
            max_length,
            is_lod_used,
        )?;
    }

    Ok((vertices, indices))
}

pub fn gen_tall_grass(is_lod_used: bool) -> Result<(Vec<Vertex>, Vec<u32>)> {
    gen_grass_column(8, is_lod_used)
}

pub fn gen_short_grass(is_lod_used: bool) -> Result<(Vec<Vertex>, Vec<u32>)> {
    gen_grass_column(4, is_lod_used)
}

pub fn gen_carrot(is_lod_used: bool) -> Result<(Vec<Vertex>, Vec<u32>)> {
    const BURIED_TIP_Y: i32 = -4;
    const ORIGIN: IVec3 = IVec3::new(0, BURIED_TIP_Y, 0);
    const LEAF_BASE_Y: i32 = 2;
    const LEAF_HEIGHT: i32 = 4;
    const MAX_LENGTH: u32 = 9;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Ten voxels tall from buried tip y=-4 to leaf tip y=5. Most orange root voxels sit below
    // the soil; y=0..1 leaves a small carrot shoulder visible above ground.
    const ROOT_LAYERS: &[(i32, i32)] = &[(-4, 0), (-3, 0), (-2, 1), (-1, 1), (0, 1), (1, 0)];
    for &(y, radius) in ROOT_LAYERS {
        for x in -radius..=radius {
            for z in -radius..=radius {
                if x.abs() + z.abs() > radius + 1 {
                    continue;
                }
                let vertex_offset = vertices.len() as u32;
                append_indexed_cube_data(
                    &mut vertices,
                    &mut indices,
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
            let vertex_offset = vertices.len() as u32;
            append_indexed_cube_data(
                &mut vertices,
                &mut indices,
                IVec3::new(base_x + lean_x, LEAF_BASE_Y + y, base_z + lean_z),
                vertex_offset,
                ORIGIN,
                MAX_LENGTH,
                is_lod_used,
            )?;
        }
    }

    Ok((vertices, indices))
}

type TomatoVoxelSet = HashSet<(i32, i32, i32)>;

fn append_unique_tomato_voxel(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    occupied: &mut TomatoVoxelSet,
    pos: IVec3,
    origin: IVec3,
    max_length: u32,
    is_lod_used: bool,
) -> Result<()> {
    if !occupied.insert((pos.x, pos.y, pos.z)) {
        return Ok(());
    }

    let vertex_offset = vertices.len() as u32;
    append_indexed_cube_data(
        vertices,
        indices,
        pos,
        vertex_offset,
        origin,
        max_length,
        is_lod_used,
    )
}

fn push_tomato_line(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    occupied: &mut TomatoVoxelSet,
    start: IVec3,
    end: IVec3,
    origin: IVec3,
    max_length: u32,
    is_lod_used: bool,
) -> Result<()> {
    let delta = end - start;
    let steps = delta.x.abs().max(delta.y.abs()).max(delta.z.abs()).max(1);

    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let pos = IVec3::new(
            (start.x as f32 + delta.x as f32 * t).round() as i32,
            (start.y as f32 + delta.y as f32 * t).round() as i32,
            (start.z as f32 + delta.z as f32 * t).round() as i32,
        );
        append_unique_tomato_voxel(
            vertices,
            indices,
            occupied,
            pos,
            origin,
            max_length,
            is_lod_used,
        )?;
    }

    Ok(())
}

pub fn gen_tomato(is_lod_used: bool) -> Result<(Vec<Vertex>, Vec<u32>)> {
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);
    const MAX_LENGTH: u32 = 14;
    const MAIN_STEM_TOP_Y: i32 = 9;
    const BRANCHES: &[(IVec3, IVec3)] = &[
        (IVec3::new(0, 3, 0), IVec3::new(-5, 5, -1)),
        (IVec3::new(0, 4, 0), IVec3::new(5, 6, 1)),
        (IVec3::new(0, 5, 0), IVec3::new(-7, 7, 2)),
        (IVec3::new(0, 6, 0), IVec3::new(7, 8, -2)),
        (IVec3::new(0, 7, 0), IVec3::new(-4, 10, 0)),
        (IVec3::new(0, 8, 0), IVec3::new(4, 11, 2)),
        (IVec3::new(0, 9, 0), IVec3::new(0, 12, 1)),
    ];
    const SECONDARY_BRANCHES: &[(IVec3, IVec3)] = &[
        (IVec3::new(-4, 4, -1), IVec3::new(-7, 5, -2)),
        (IVec3::new(4, 5, 1), IVec3::new(8, 6, 1)),
        (IVec3::new(-5, 7, 2), IVec3::new(-9, 8, 3)),
        (IVec3::new(5, 7, -2), IVec3::new(9, 8, -3)),
        (IVec3::new(-3, 10, 0), IVec3::new(-6, 11, 1)),
        (IVec3::new(3, 10, 2), IVec3::new(6, 12, 2)),
    ];

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut occupied = TomatoVoxelSet::new();

    // First pass: only the vine skeleton. Keep the root and every branch one voxel thick so the
    // silhouette is easy to tune before adding leaves or fruit in later passes.
    for y in 0..=MAIN_STEM_TOP_Y {
        append_unique_tomato_voxel(
            &mut vertices,
            &mut indices,
            &mut occupied,
            IVec3::new(0, y, 0),
            ORIGIN,
            MAX_LENGTH,
            is_lod_used,
        )?;
    }

    for &(start, end) in BRANCHES.iter().chain(SECONDARY_BRANCHES.iter()) {
        push_tomato_line(
            &mut vertices,
            &mut indices,
            &mut occupied,
            start,
            end,
            ORIGIN,
            MAX_LENGTH,
            is_lod_used,
        )?;
    }

    Ok((vertices, indices))
}

pub fn gen_lavender(is_lod_used: bool) -> Result<(Vec<Vertex>, Vec<u32>)> {
    const STEM_VOXEL_COUNT: u32 = 6;
    const LEAF_BALL_RADIUS: f32 = 1.5;
    const LEAF_BALL_BOUNDARY: i32 = LEAF_BALL_RADIUS as i32;
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);

    let max_vertical = (STEM_VOXEL_COUNT + LEAF_BALL_BOUNDARY as u32) as f32;
    let max_horizontal = LEAF_BALL_BOUNDARY as f32;
    let max_length = ((max_vertical * max_vertical + 2.0 * max_horizontal * max_horizontal).sqrt())
        .ceil()
        .max(1.0) as u32;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // draw the stem
    let total_stem_voxel_count = STEM_VOXEL_COUNT - LEAF_BALL_BOUNDARY as u32;
    for i in 0..total_stem_voxel_count {
        let vertex_offset = vertices.len() as u32;
        let base_pos = IVec3::new(0, i as i32, 0);

        append_indexed_cube_data(
            &mut vertices,
            &mut indices,
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

                let vertex_offset = vertices.len() as u32;
                let base_pos = IVec3::new(i, j, k) + IVec3::new(0, STEM_VOXEL_COUNT as i32, 0);

                append_indexed_cube_data(
                    &mut vertices,
                    &mut indices,
                    base_pos,
                    vertex_offset,
                    ORIGIN,
                    max_length,
                    is_lod_used,
                )?;
            }
        }
    }

    Ok((vertices, indices))
}

pub fn gen_ember_bloom(is_lod_used: bool) -> Result<(Vec<Vertex>, Vec<u32>)> {
    const HEIGHT: i32 = 12;
    // Width Configuration: How wide the plant swells
    const MAX_RADIUS: f32 = 2.0;
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);

    let max_vertical = (HEIGHT - 1) as f32;
    let max_horizontal = (MAX_RADIUS + 2.0).ceil(); // includes search padding
    let max_length = ((max_vertical * max_vertical + 2.0 * max_horizontal * max_horizontal).sqrt())
        .ceil()
        .max(1.0) as u32;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

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
                    let vertex_offset = vertices.len() as u32;

                    // No stem sway, just straight up for symmetry
                    let pos = IVec3::new(x, y, z);

                    append_indexed_cube_data(
                        &mut vertices,
                        &mut indices,
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

    Ok((vertices, indices))
}
