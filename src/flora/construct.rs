use crate::tracer::voxel_encoding::{
    append_indexed_cube_data, append_indexed_cube_data_with_info, FloraMeshData, FloraVoxelInfo,
    FLORA_VOXEL_MATERIAL_ALLIUM_CORE, FLORA_VOXEL_MATERIAL_GRADIENT,
};
use anyhow::Result;
use glam::IVec3;

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
