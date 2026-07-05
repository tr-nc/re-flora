use crate::tracer::voxel_encoding::append_indexed_cube_data;
use crate::tracer::Vertex;
use anyhow::Result;
use glam::{IVec3, Vec3};
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
    radius: i32,
    origin: IVec3,
    max_length: u32,
    is_lod_used: bool,
) -> Result<()> {
    let delta = end - start;
    let steps = delta.x.abs().max(delta.y.abs()).max(delta.z.abs()).max(1);
    let radius_sq = radius * radius;

    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let center = IVec3::new(
            (start.x as f32 + delta.x as f32 * t).round() as i32,
            (start.y as f32 + delta.y as f32 * t).round() as i32,
            (start.z as f32 + delta.z as f32 * t).round() as i32,
        );

        for dx in -radius..=radius {
            for dy in -radius..=radius {
                for dz in -radius..=radius {
                    if dx * dx + dy * dy + dz * dz > radius_sq {
                        continue;
                    }
                    append_unique_tomato_voxel(
                        vertices,
                        indices,
                        occupied,
                        center + IVec3::new(dx, dy, dz),
                        origin,
                        max_length,
                        is_lod_used,
                    )?;
                }
            }
        }
    }

    Ok(())
}

fn push_tomato_leaf_patch(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    occupied: &mut TomatoVoxelSet,
    base: IVec3,
    major_step: IVec3,
    length: i32,
    width: i32,
    origin: IVec3,
    max_length: u32,
    is_lod_used: bool,
) -> Result<()> {
    let side_step = if major_step.x != 0 || major_step.z != 0 {
        IVec3::new(-major_step.z.signum(), 0, major_step.x.signum())
    } else {
        IVec3::X
    };

    push_tomato_line(
        vertices,
        indices,
        occupied,
        base,
        base + major_step * length,
        0,
        origin,
        max_length,
        is_lod_used,
    )?;

    for i in 0..=length {
        let t = if length == 0 {
            0.0
        } else {
            i as f32 / length as f32
        };
        let mid = base + major_step * i + IVec3::new(0, -(i / 5), 0);
        let mut half_width = ((t * PI).sin() * width as f32).round() as i32;
        if i == 0 || i == length {
            half_width = half_width.min(1);
        }

        for side in -half_width..=half_width {
            // Tomato leaves are visibly lobed. Dropping some edge cubes cuts a serrated outline
            // into the otherwise simple voxel diamond without adding per-instance randomness.
            if half_width > 1 && side.abs() == half_width && (i + side.abs()) % 2 == 0 {
                continue;
            }

            let pos = mid + side_step * side;
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
    }

    Ok(())
}

fn push_tomato_fruit(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    occupied: &mut TomatoVoxelSet,
    center: IVec3,
    origin: IVec3,
    max_length: u32,
    is_lod_used: bool,
) -> Result<()> {
    const FRUIT_RADIUS_XZ: f32 = 2.15;
    const FRUIT_RADIUS_Y: f32 = 2.05;
    const FRUIT_SEARCH_RADIUS: i32 = 2;

    for dx in -FRUIT_SEARCH_RADIUS..=FRUIT_SEARCH_RADIUS {
        for dy in -FRUIT_SEARCH_RADIUS..=FRUIT_SEARCH_RADIUS {
            for dz in -FRUIT_SEARCH_RADIUS..=FRUIT_SEARCH_RADIUS {
                let p = Vec3::new(
                    dx as f32 / FRUIT_RADIUS_XZ,
                    dy as f32 / FRUIT_RADIUS_Y,
                    dz as f32 / FRUIT_RADIUS_XZ,
                );
                if p.length_squared() > 1.0 {
                    continue;
                }

                append_unique_tomato_voxel(
                    vertices,
                    indices,
                    occupied,
                    center + IVec3::new(dx, dy, dz),
                    origin,
                    max_length,
                    is_lod_used,
                )?;
            }
        }
    }

    let sepal_center = center + IVec3::new(0, 3, 0);
    const SEPAL_TIPS: &[IVec3] = &[
        IVec3::new(2, 0, 0),
        IVec3::new(-2, 0, 0),
        IVec3::new(0, 0, 2),
        IVec3::new(0, 0, -2),
        IVec3::new(1, 1, 1),
        IVec3::new(-1, 1, -1),
    ];
    append_unique_tomato_voxel(
        vertices,
        indices,
        occupied,
        sepal_center,
        origin,
        max_length,
        is_lod_used,
    )?;
    for &tip in SEPAL_TIPS {
        push_tomato_line(
            vertices,
            indices,
            occupied,
            sepal_center,
            sepal_center + tip,
            0,
            origin,
            max_length,
            is_lod_used,
        )?;
    }

    Ok(())
}

pub fn gen_tomato(is_lod_used: bool) -> Result<(Vec<Vertex>, Vec<u32>)> {
    const ORIGIN: IVec3 = IVec3::new(0, 0, 0);
    const MAX_LENGTH: u32 = 28;
    const PRIMARY_BRANCHES: &[(IVec3, IVec3)] = &[
        (IVec3::new(0, 6, 0), IVec3::new(-6, 11, -1)),
        (IVec3::new(0, 8, 0), IVec3::new(7, 12, 1)),
        (IVec3::new(0, 11, 0), IVec3::new(-8, 16, 2)),
        (IVec3::new(0, 13, 0), IVec3::new(7, 17, -2)),
        (IVec3::new(0, 16, 0), IVec3::new(-4, 22, 0)),
        (IVec3::new(0, 17, 0), IVec3::new(4, 23, 2)),
    ];
    const FRUITS: &[(IVec3, IVec3)] = &[
        (IVec3::new(-5, 7, -1), IVec3::new(-4, 10, -1)),
        (IVec3::new(-2, 10, 2), IVec3::new(-1, 12, 1)),
        (IVec3::new(4, 8, 1), IVec3::new(4, 11, 1)),
        (IVec3::new(7, 11, 2), IVec3::new(7, 13, 1)),
        (IVec3::new(-7, 14, 2), IVec3::new(-7, 16, 2)),
        (IVec3::new(3, 14, -3), IVec3::new(4, 16, -2)),
        (IVec3::new(6, 16, -1), IVec3::new(6, 18, -1)),
        (IVec3::new(-3, 18, 1), IVec3::new(-3, 20, 0)),
        (IVec3::new(1, 19, 3), IVec3::new(2, 21, 2)),
    ];
    const LEAF_PATCHES: &[(IVec3, IVec3, i32, i32)] = &[
        (IVec3::new(-3, 11, -1), IVec3::new(-1, 1, 0), 5, 2),
        (IVec3::new(-6, 11, -1), IVec3::new(-1, 0, -1), 4, 2),
        (IVec3::new(3, 13, 1), IVec3::new(1, 1, 0), 5, 2),
        (IVec3::new(7, 12, 1), IVec3::new(1, 0, 1), 4, 2),
        (IVec3::new(-5, 16, 2), IVec3::new(-1, 1, 0), 5, 2),
        (IVec3::new(-8, 16, 2), IVec3::new(-1, 0, 1), 4, 2),
        (IVec3::new(4, 17, -2), IVec3::new(1, 1, 0), 5, 2),
        (IVec3::new(7, 17, -2), IVec3::new(1, 0, -1), 4, 2),
        (IVec3::new(-2, 20, 0), IVec3::new(-1, 1, 0), 4, 2),
        (IVec3::new(2, 21, 2), IVec3::new(1, 1, 0), 4, 2),
        (IVec3::new(0, 20, 0), IVec3::new(0, 1, 1), 4, 2),
    ];

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut occupied = TomatoVoxelSet::new();

    // A potted tomato has a thick green lower stem that narrows into branching vines.
    for y in 0..=17 {
        append_unique_tomato_voxel(
            &mut vertices,
            &mut indices,
            &mut occupied,
            IVec3::new(0, y, 0),
            ORIGIN,
            MAX_LENGTH,
            is_lod_used,
        )?;
        if y <= 6 {
            append_unique_tomato_voxel(
                &mut vertices,
                &mut indices,
                &mut occupied,
                IVec3::new(1, y, 0),
                ORIGIN,
                MAX_LENGTH,
                is_lod_used,
            )?;
        }
    }

    for &(start, end) in PRIMARY_BRANCHES {
        push_tomato_line(
            &mut vertices,
            &mut indices,
            &mut occupied,
            start,
            end,
            0,
            ORIGIN,
            MAX_LENGTH,
            is_lod_used,
        )?;
    }

    for &(fruit_center, hanger) in FRUITS {
        push_tomato_line(
            &mut vertices,
            &mut indices,
            &mut occupied,
            hanger,
            fruit_center + IVec3::new(0, 3, 0),
            0,
            ORIGIN,
            MAX_LENGTH,
            is_lod_used,
        )?;
        push_tomato_fruit(
            &mut vertices,
            &mut indices,
            &mut occupied,
            fruit_center,
            ORIGIN,
            MAX_LENGTH,
            is_lod_used,
        )?;
    }

    for &(base, major_step, length, width) in LEAF_PATCHES {
        push_tomato_leaf_patch(
            &mut vertices,
            &mut indices,
            &mut occupied,
            base,
            major_step,
            length,
            width,
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
