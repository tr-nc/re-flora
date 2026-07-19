use crate::tracer::voxel_encoding::{
    append_indexed_cube_data, append_indexed_cube_data_with_info, FloraMeshData, FloraVoxelInfo,
};
use anyhow::Result;
use glam::{IVec3, Vec3};
use noise::{NoiseFn, Perlin};

pub const DEFAULT_LEAF_INNER_DENSITY: f32 = 0.5;
pub const DEFAULT_LEAF_OUTER_DENSITY: f32 = 0.25;
pub const DEFAULT_LEAF_INNER_RADIUS: f32 = 8.0;
pub const DEFAULT_LEAF_OUTER_RADIUS: f32 = 16.0;
pub const TREE_FRUIT_MAX_RADIUS_VOXELS: u32 = 2;

#[derive(Debug, Clone)]
pub struct LeafVoxelShape {
    pub offsets: Vec<IVec3>,
    pub max_length: u32,
}

fn push_voxel(
    mesh: &mut FloraMeshData,
    pos: IVec3,
    origin: IVec3,
    max_length: u32,
    is_lod_used: bool,
) -> Result<()> {
    let vertex_offset = mesh.vertices.len() as u32;
    append_indexed_cube_data(
        &mut mesh.vertices,
        &mut mesh.indices,
        &mut mesh.voxel_infos,
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
) -> Result<FloraMeshData> {
    let shape =
        generate_voxel_leaf_shape(inner_density, outer_density, inner_radius, outer_radius)?;
    let origin = IVec3::ZERO;
    let mut mesh = FloraMeshData::new(shape.max_length);

    for pos in shape.offsets {
        push_voxel(&mut mesh, pos, origin, shape.max_length, is_lod_used)?;
    }

    Ok(mesh)
}

/// Generates a one-voxel mesh for per-leaf-voxel tree rendering.
pub fn generate_indexed_single_voxel_leaf(
    max_length: u32,
    is_lod_used: bool,
) -> Result<FloraMeshData> {
    let max_length = max_length.max(1);
    let mut mesh = FloraMeshData::new(max_length);
    push_voxel(&mut mesh, IVec3::ZERO, IVec3::ZERO, max_length, is_lod_used)?;

    Ok(mesh)
}

/// Generates the mature apple mesh centered on the instance anchor.
///
/// The apple is intentionally render-only: tree placement creates instances for
/// this mesh instead of stamping fruit into the terrain voxel field.
pub fn generate_indexed_voxel_apple(is_lod_used: bool) -> Result<FloraMeshData> {
    const MAX_LENGTH: u32 = TREE_FRUIT_MAX_RADIUS_VOXELS;

    let mut mesh = FloraMeshData::new(MAX_LENGTH);

    for pos in voxel_apple_offsets() {
        let vertex_offset = mesh.vertices.len() as u32;
        let color_gradient = ((pos.y + MAX_LENGTH as i32) as f32
            / (MAX_LENGTH as f32 * 2.0).max(1.0))
        .clamp(0.0, 1.0);
        append_indexed_cube_data_with_info(
            &mut mesh.vertices,
            &mut mesh.indices,
            &mut mesh.voxel_infos,
            pos,
            vertex_offset,
            FloraVoxelInfo::new(color_gradient, 1.0, color_gradient, 0),
            is_lod_used,
        )?;
    }

    Ok(mesh)
}

/// The shared raster description used by attached apple rendering, collision-probe
/// rendering, and the dynamic convex collider.
pub fn voxel_apple_offsets() -> Vec<IVec3> {
    voxel_apple_offsets_for_radius(TREE_FRUIT_MAX_RADIUS_VOXELS)
}

pub fn voxel_apple_offsets_for_radius(radius_voxels: u32) -> Vec<IVec3> {
    let radius_voxels = radius_voxels.clamp(1, TREE_FRUIT_MAX_RADIUS_VOXELS);
    let radius = radius_voxels as i32;
    let diameter = radius.saturating_mul(2) as usize;
    let mut offsets = Vec::with_capacity(diameter.saturating_pow(3));
    for x in -radius..radius {
        for y in -radius..radius {
            for z in -radius..radius {
                let center = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (center / radius_voxels as f32).length_squared() <= 1.0 {
                    offsets.push(IVec3::new(x, y, z));
                }
            }
        }
    }
    offsets
}

/// A two-times debug derivation of the regular apple. Each source voxel expands into eight
/// standard-size voxels, so the probe is easier to see without breaking the scene's voxel scale.
pub fn collision_probe_apple_offsets() -> Vec<IVec3> {
    const SCALE: i32 = 2;
    let source_offsets = voxel_apple_offsets();
    let mut offsets = Vec::with_capacity(source_offsets.len() * SCALE.pow(3) as usize);
    for source in source_offsets {
        let min = source * SCALE;
        for x in 0..SCALE {
            for y in 0..SCALE {
                for z in 0..SCALE {
                    offsets.push(min + IVec3::new(x, y, z));
                }
            }
        }
    }
    offsets
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn voxel_apple_description_is_unique_and_centered() {
        let offsets = voxel_apple_offsets();
        let unique = offsets.iter().copied().collect::<HashSet<_>>();

        assert_eq!(unique.len(), offsets.len());
        assert!(!offsets.is_empty());
        for offset in offsets {
            assert!(unique.contains(&(-offset - IVec3::ONE)));
        }
    }

    #[test]
    fn voxel_apple_radius_stages_are_nested_unit_voxel_shapes() {
        let radius_one = voxel_apple_offsets_for_radius(1)
            .into_iter()
            .collect::<HashSet<_>>();
        let radius_two = voxel_apple_offsets_for_radius(2)
            .into_iter()
            .collect::<HashSet<_>>();

        assert_eq!(radius_one.len(), 8);
        assert!(radius_one.is_subset(&radius_two));
        assert!(radius_one.len() < radius_two.len());
        assert_eq!(
            voxel_apple_offsets_for_radius(TREE_FRUIT_MAX_RADIUS_VOXELS + 1),
            voxel_apple_offsets()
        );
    }

    #[test]
    fn collision_probe_apple_is_two_times_bigger_with_unit_voxels() {
        let offsets = collision_probe_apple_offsets();
        let unique = offsets.iter().copied().collect::<HashSet<_>>();

        assert_eq!(offsets.len(), voxel_apple_offsets().len() * 8);
        assert_eq!(unique.len(), offsets.len());
        let radius = TREE_FRUIT_MAX_RADIUS_VOXELS as i32;
        assert_eq!(
            offsets.iter().map(|offset| offset.min_element()).min(),
            Some(-radius * 2)
        );
        assert_eq!(
            offsets.iter().map(|offset| offset.max_element()).max(),
            Some(radius * 2 - 1)
        );
    }
}
