use crate::tracer::voxel_encoding::{append_indexed_cube_data, FloraMeshData};
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
