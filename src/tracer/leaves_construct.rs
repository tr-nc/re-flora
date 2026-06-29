use crate::tracer::{voxel_encoding::append_indexed_cube_data, Vertex};
use anyhow::Result;
use glam::{IVec3, Vec3};
use noise::{NoiseFn, Perlin};

pub const DEFAULT_LEAF_INNER_DENSITY: f32 = 0.5;
pub const DEFAULT_LEAF_OUTER_DENSITY: f32 = 0.25;
pub const DEFAULT_LEAF_INNER_RADIUS: f32 = 8.0;
pub const DEFAULT_LEAF_OUTER_RADIUS: f32 = 16.0;

#[derive(Debug, Clone)]
pub struct LeafVoxelShape {
    pub offsets: Vec<IVec3>,
    pub max_length: u32,
}

fn push_voxel(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    pos: IVec3,
    origin: IVec3,
    max_length: u32,
    is_lod_used: bool,
) -> Result<()> {
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

/// Generates voxel offsets for hollow sphere-shaped leaves.
///
/// # Parameters
/// - `inner_density`: Density at the inner shell edge (0.0 to 1.0)
/// - `outer_density`: Density at the outer shell edge (0.0 to 1.0)
/// - `inner_radius`: Inner radius of the hollow sphere (max 128 due to encoding constraints)
/// - `outer_radius`: Outer radius of the hollow sphere (max 128 due to encoding constraints)
///
/// # Returns
/// The leaf-shell voxel offsets and the gradient length used by the foliage shaders.
pub fn generate_voxel_leaf_shape(
    inner_density: f32,
    outer_density: f32,
    inner_radius: f32,
    outer_radius: f32,
) -> Result<LeafVoxelShape> {
    if outer_radius > 128.0 {
        return Err(anyhow::anyhow!(
            "Outer radius must be <= 128 due to encoding constraints"
        ));
    }

    if inner_radius > outer_radius {
        return Err(anyhow::anyhow!("Inner radius must be <= outer radius"));
    }

    let max_length = outer_radius.ceil().max(1.0) as u32;

    if inner_density.max(outer_density) <= 0.0 {
        return Ok(LeafVoxelShape {
            offsets: Vec::new(),
            max_length,
        });
    }

    let mut offsets = Vec::new();
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

                // calculate density within the shell region only
                let falloff_density = if outer_radius > inner_radius {
                    // shell region: gradient from 0.0 at inner_radius to 1.0 at outer_radius
                    let shell_ratio =
                        (distance_from_center - inner_radius) / (outer_radius - inner_radius);
                    // mix density: inner_density at inner edge, outer_density at outer edge
                    inner_density * (1.0 - shell_ratio) + outer_density * shell_ratio
                } else {
                    // when inner_radius == outer_radius, single shell layer
                    let color_gradient = (distance_from_center / outer_radius).min(1.0);
                    inner_density * (1.0 - color_gradient) + outer_density * color_gradient
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
                    offsets.push(pos);
                }
            }
        }
    }

    Ok(LeafVoxelShape {
        offsets,
        max_length,
    })
}

/// Generates indexed voxel data for hollow sphere-shaped leaves.
///
/// This is kept for callers that need the historical cluster mesh. Tree leaves now render the
/// same offsets as per-voxel instances instead of baking every offset into this mesh.
#[allow(dead_code)]
pub fn generate_indexed_voxel_leaves(
    inner_density: f32,
    outer_density: f32,
    inner_radius: f32,
    outer_radius: f32,
    is_lod_used: bool,
) -> Result<(Vec<Vertex>, Vec<u32>)> {
    let shape =
        generate_voxel_leaf_shape(inner_density, outer_density, inner_radius, outer_radius)?;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let origin = IVec3::ZERO;

    for pos in shape.offsets {
        push_voxel(
            &mut vertices,
            &mut indices,
            pos,
            origin,
            shape.max_length,
            is_lod_used,
        )?;
    }

    Ok((vertices, indices))
}

/// Generates a one-voxel mesh for per-leaf-voxel tree rendering.
pub fn generate_indexed_single_voxel_leaf(
    max_length: u32,
    is_lod_used: bool,
) -> Result<(Vec<Vertex>, Vec<u32>)> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    push_voxel(
        &mut vertices,
        &mut indices,
        IVec3::ZERO,
        IVec3::ZERO,
        max_length.max(1),
        is_lod_used,
    )?;

    Ok((vertices, indices))
}

/// Generates a compact 4-voxel-diameter apple mesh centered on the instance anchor.
///
/// The apple is intentionally render-only: tree placement creates instances for
/// this mesh instead of stamping fruit into the terrain voxel field.
pub fn generate_indexed_voxel_apple(is_lod_used: bool) -> Result<(Vec<Vertex>, Vec<u32>)> {
    const ORIGIN: IVec3 = IVec3::ZERO;
    const MAX_LENGTH: u32 = 2;
    const BODY_RADIUS_XZ: f32 = 2.0;
    const BODY_RADIUS_Y: f32 = 2.0;
    const MIN_VOXEL: i32 = -2;
    const MAX_VOXEL: i32 = 1;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for x in MIN_VOXEL..=MAX_VOXEL {
        for y in MIN_VOXEL..=MAX_VOXEL {
            for z in MIN_VOXEL..=MAX_VOXEL {
                let p = Vec3::new(
                    (x as f32 + 0.5) / BODY_RADIUS_XZ,
                    (y as f32 + 0.5) / BODY_RADIUS_Y,
                    (z as f32 + 0.5) / BODY_RADIUS_XZ,
                );
                let shape = p.length_squared();

                if shape <= 1.0 {
                    push_voxel(
                        &mut vertices,
                        &mut indices,
                        IVec3::new(x, y, z),
                        ORIGIN,
                        MAX_LENGTH,
                        is_lod_used,
                    )?;
                }
            }
        }
    }

    Ok((vertices, indices))
}
