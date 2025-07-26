use crate::tracer::voxel_encoding::append_indexed_cube_data;
use crate::tracer::Vertex;
use anyhow::Result;
use glam::IVec3;

pub fn generate_indexed_voxel_grass_blade(voxel_count: u32) -> Result<(Vec<Vertex>, Vec<u32>)> {
    if voxel_count == 0 {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..voxel_count {
        let vertex_offset = vertices.len() as u32;
        let base_pos = IVec3::new(0, i as i32, 0);

        // Calculate color gradient: 0.0 for bottom (i=0), 1.0 for tip (i=voxel_count-1)
        let color_gradient = if voxel_count > 1 {
            i as f32 / (voxel_count - 1) as f32
        } else {
            0.0
        };

        append_indexed_cube_data(
            &mut vertices,
            &mut indices,
            base_pos,
            color_gradient,
            vertex_offset,
        )?;
    }

    Ok((vertices, indices))
}
