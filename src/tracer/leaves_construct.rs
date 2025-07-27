use crate::tracer::{voxel_encoding::append_indexed_cube_data, Vertex};
use anyhow::Result;
use glam::IVec3;
use noise::{NoiseFn, Perlin};

/// Generates indexed voxel data for hollow sphere-shaped leaves.
///
/// # Parameters
/// - `inner_density`: Density at the inner shell edge (0.0 to 1.0)
/// - `outer_density`: Density at the outer shell edge (0.0 to 1.0)
/// - `inner_radius`: Inner radius of the hollow sphere (max 128 due to encoding constraints)
/// - `outer_radius`: Outer radius of the hollow sphere (max 128 due to encoding constraints)
///
/// # Returns
/// A tuple of (vertices, indices) for rendering the voxel leaves.
pub fn generate_indexed_voxel_leaves(
    inner_density: f32,
    outer_density: f32,
    inner_radius: f32,
    outer_radius: f32,
) -> Result<(Vec<Vertex>, Vec<u32>)> {
    if outer_radius > 128.0 {
        return Err(anyhow::anyhow!(
            "Outer radius must be <= 128 due to encoding constraints"
        ));
    }

    if inner_radius > outer_radius {
        return Err(anyhow::anyhow!("Inner radius must be <= outer radius"));
    }

    if inner_density.max(outer_density) <= 0.0 {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let noise = Perlin::new(42); // Fixed seed for consistent results
    let outer_radius_i = outer_radius as i32;

    // iterate through a bounding box around the sphere
    for x in -outer_radius_i..=outer_radius_i {
        for y in -outer_radius_i..=outer_radius_i {
            for z in -outer_radius_i..=outer_radius_i {
                let pos = IVec3::new(x, y, z);
                let distance_from_center = pos.as_vec3().length();

                // skip if outside outer_radius or inside inner_radius (hollow center)
                if distance_from_center > outer_radius || distance_from_center < inner_radius {
                    continue;
                }

                // calculate gradient and density within the shell region only
                let (color_gradient, wind_gradient, falloff_density) = if outer_radius
                    > inner_radius
                {
                    // shell region: gradient from 0.0 at inner_radius to 1.0 at outer_radius
                    let shell_ratio =
                        (distance_from_center - inner_radius) / (outer_radius - inner_radius);
                    let color_gradient = shell_ratio.min(1.0);
                    // mix density: inner_density at inner edge, outer_density at outer edge
                    let density = inner_density * (1.0 - shell_ratio) + outer_density * shell_ratio;
                    let wind_gradient = distance_from_center / outer_radius;
                    (color_gradient, wind_gradient, density)
                } else {
                    // when inner_radius == outer_radius, single shell layer
                    let color_gradient = (distance_from_center / outer_radius).min(1.0);
                    let density =
                        inner_density * (1.0 - color_gradient) + outer_density * color_gradient;
                    let wind_gradient = distance_from_center / outer_radius;
                    (color_gradient, wind_gradient, density)
                };

                // use noise to determine if we should place a voxel here
                let noise_freq = 1.1;
                let noise_value = noise.get([
                    x as f64 * noise_freq,
                    y as f64 * noise_freq,
                    z as f64 * noise_freq,
                ]);
                let noise_threshold = (1.0 - falloff_density) as f64; // Higher density = lower threshold

                if noise_value > noise_threshold {
                    let vertex_offset = vertices.len() as u32;

                    append_indexed_cube_data(
                        &mut vertices,
                        &mut indices,
                        pos,
                        vertex_offset,
                        color_gradient,
                        wind_gradient,
                    )?;
                }
            }
        }
    }

    Ok((vertices, indices))
}
