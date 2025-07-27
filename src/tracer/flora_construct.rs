use crate::tracer::voxel_encoding::append_indexed_cube_data;
use crate::tracer::Vertex;
use anyhow::Result;
use glam::IVec3;

pub fn gen_grass() -> Result<(Vec<Vertex>, Vec<u32>)> {
    const VOXEL_COUNT: u32 = 8;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..VOXEL_COUNT {
        let vertex_offset = vertices.len() as u32;
        let base_pos = IVec3::new(0, i as i32, 0);

        // Calculate color gradient: 0.0 for bottom (i=0), 1.0 for tip (i=voxel_count-1)
        let gradient = if VOXEL_COUNT > 1 {
            i as f32 / (VOXEL_COUNT - 1) as f32
        } else {
            0.0
        };

        append_indexed_cube_data(
            &mut vertices,
            &mut indices,
            base_pos,
            vertex_offset,
            gradient,
            gradient,
        )?;
    }

    Ok((vertices, indices))
}

pub fn gen_lavender() -> Result<(Vec<Vertex>, Vec<u32>)> {
    const STEM_VOXEL_COUNT: u32 = 8;
    const LEAF_BALL_RADIUS: f32 = 2.0;
    const LEAF_BALL_BOUNDARY: i32 = LEAF_BALL_RADIUS as i32;
    const TOTAL_HEIGHT: u32 = STEM_VOXEL_COUNT + (LEAF_BALL_BOUNDARY * 2 + 1) as u32;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // draw the stem
    let total_stem_voxel_count = STEM_VOXEL_COUNT - LEAF_BALL_BOUNDARY as u32;
    for i in 0..total_stem_voxel_count {
        let vertex_offset = vertices.len() as u32;
        let base_pos = IVec3::new(0, i as i32, 0);

        // it never reaches 1, because 1 means the leaf ball, and we only need the shadow underneath it
        let mut color_gradient = i as f32 / total_stem_voxel_count as f32;
        color_gradient = color_gradient.powf(5.0);

        let wind_gradient = base_pos.y as f32 / TOTAL_HEIGHT as f32;

        append_indexed_cube_data(
            &mut vertices,
            &mut indices,
            base_pos,
            vertex_offset,
            color_gradient,
            wind_gradient,
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

                const COLOR_GRADIENT: f32 = 1.0;
                let wind_gradient = base_pos.y as f32 / TOTAL_HEIGHT as f32;
                append_indexed_cube_data(
                    &mut vertices,
                    &mut indices,
                    base_pos,
                    vertex_offset,
                    COLOR_GRADIENT,
                    wind_gradient,
                )?;
            }
        }
    }

    Ok((vertices, indices))
}
