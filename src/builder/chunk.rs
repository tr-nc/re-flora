use glam::{IVec3, UVec3};
use std::simd::{cmp::SimdPartialOrd, Simd};

pub const BLOCK_AIR: u8 = 0;
pub const BLOCK_DIRT: u8 = 1;

pub static BLOCK_TYPES: &[(u8, &str)] = &[(BLOCK_AIR, "Air"), (BLOCK_DIRT, "Dirt")];

pub struct Chunk {
    pub resolution: UVec3,
    pub position: IVec3,
    /// Voxel data (dense). Each `u8` is a block type.
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn new(resolution: UVec3, position: IVec3) -> Self {
        Self {
            resolution,
            position,
            data: build_sphere_data_simd_optimized(resolution, position),
        }
    }
}

/// Common function that returns sphere properties:
/// - radius (f32)
/// - radius_sq (f32)
/// - center_x, center_y, center_z (each f32)
fn sphere_params(resolution: UVec3, chunk_pos: IVec3) -> (f32, f32, f32, f32, f32) {
    // Sphere radius = half the smallest dimension
    let radius = (resolution.x.min(resolution.y).min(resolution.z)) as f32 * 0.5;
    let radius_sq = radius * radius;

    // Sphere center = (chunk_pos + resolution/2)
    let center_x = chunk_pos.x as f32 + resolution.x as f32 * 0.5;
    let center_y = chunk_pos.y as f32 + resolution.y as f32 * 0.5;
    let center_z = chunk_pos.z as f32 + resolution.z as f32 * 0.5;

    (radius, radius_sq, center_x, center_y, center_z)
}

/// Linear index for a 3D coordinate (x,y,z) in a chunk with the given resolution.
#[inline]
fn index_3d(x: u32, y: u32, z: u32, resolution: UVec3) -> usize {
    (x * resolution.y * resolution.z + y * resolution.z + z) as usize
}

/// Builds a chunk with a spherical region of *dirt*, using **plain for-loops** (no SIMD).
/// - The sphere is centered at `(chunk_position + resolution/2)`.
/// - The radius is half of the smallest dimension of the chunk resolution.
/// - Voxels inside the sphere => `BLOCK_DIRT`, outside => `BLOCK_AIR`.
pub fn build_sphere_data_brute_force(resolution: UVec3, chunk_pos: IVec3) -> Vec<u8> {
    let total_voxels = (resolution.x * resolution.y * resolution.z) as usize;
    let mut data = vec![BLOCK_AIR; total_voxels];

    let (_radius, radius_sq, center_x, center_y, center_z) = sphere_params(resolution, chunk_pos);

    for x in 0..resolution.x {
        let wx = chunk_pos.x as f32 + x as f32;
        for y in 0..resolution.y {
            let wy = chunk_pos.y as f32 + y as f32;
            for z in 0..resolution.z {
                let wz = chunk_pos.z as f32 + z as f32;

                let dx = wx - center_x;
                let dy = wy - center_y;
                let dz = wz - center_z;

                let dist_sq = dx * dx + dy * dy + dz * dz;
                let data_idx = index_3d(x, y, z, resolution);

                if dist_sq <= radius_sq {
                    data[data_idx] = BLOCK_DIRT;
                } else {
                    data[data_idx] = BLOCK_AIR;
                }
            }
        }
    }

    data
}

pub fn build_sphere_data_simd_optimized(resolution: UVec3, chunk_pos: IVec3) -> Vec<u8> {
    let total_voxels = (resolution.x * resolution.y * resolution.z) as usize;
    let mut data = vec![BLOCK_AIR; total_voxels];

    let (_radius, radius_sq, center_x, center_y, center_z) = sphere_params(resolution, chunk_pos);
    let radius_sq_v = Simd::splat(radius_sq);

    // Precompute dx^2 and dy^2
    let mut dx2 = vec![0.0_f32; resolution.x as usize];
    let mut dy2 = vec![0.0_f32; resolution.y as usize];

    for x in 0..resolution.x {
        let dx = (chunk_pos.x as f32 + x as f32) - center_x;
        dx2[x as usize] = dx * dx;
    }
    for y in 0..resolution.y {
        let dy = (chunk_pos.y as f32 + y as f32) - center_y;
        dy2[y as usize] = dy * dy;
    }

    const LANES: usize = 32;

    // For each x,y, do a SIMD pass over z
    for x in 0..resolution.x {
        let dx2_val = dx2[x as usize];

        // Optional bounding: if dx2[x] > radius_sq => everything is outside
        if dx2_val > radius_sq {
            continue;
        }

        for y in 0..resolution.y {
            let xy_sq = dx2_val + dy2[y as usize];
            // Another bounding check
            if xy_sq > radius_sq {
                continue;
            }

            let data_idx_base = index_3d(x, y, 0, resolution);

            let mut z = 0;
            while z < resolution.z {
                let chunk_count = (resolution.z - z).min(LANES as u32) as usize;

                // Build z-array for partial or full chunk
                let mut z_array = [0.0_f32; LANES];
                for lane_i in 0..chunk_count {
                    let wz = (chunk_pos.z as f32 + (z + lane_i as u32) as f32) - center_z;
                    z_array[lane_i] = wz;
                }

                let z_v = Simd::from_array(z_array);
                let dist_sq_v = {
                    // dist^2 = xy_sq + z^2
                    let z_sq = z_v * z_v;
                    z_sq + Simd::splat(xy_sq)
                };

                let inside_mask = dist_sq_v.simd_le(radius_sq_v);

                // If all inside => write BLOCK_DIRT
                // If none => leave as BLOCK_AIR
                // else => lane by lane
                let all_inside = inside_mask.all();
                let all_outside = !inside_mask.any();

                if all_inside {
                    for lane_i in 0..chunk_count {
                        data[data_idx_base + (z + lane_i as u32) as usize] = BLOCK_DIRT;
                    }
                } else if !all_outside {
                    let bitmask = inside_mask.to_bitmask();
                    for lane_i in 0..chunk_count {
                        let lane_bit = 1 << lane_i;
                        if (bitmask & lane_bit) != 0 {
                            data[data_idx_base + (z + lane_i as u32) as usize] = BLOCK_DIRT;
                        }
                    }
                }

                z += chunk_count as u32;
            }
        }
    }

    data
}

// debug tests
// ---- builder::chunk::tests::compare_brute_force_vs_simd stdout ----
// Chunk dim:   16, brute force: 113.5µs, simd: 131.7µs
// Chunk dim:  128, brute force: 29.6299ms, simd: 33.471ms
// Chunk dim:  256, brute force: 258.6347ms, simd: 244.7651ms

// release tests
// ---- builder::chunk::tests::compare_brute_force_vs_simd stdout ----
// Chunk dim:   16, brute force: 11.7µs, simd: 6.5µs
// Chunk dim:  128, brute force: 2.7613ms, simd: 1.2618ms
// Chunk dim:  256, brute force: 16.5897ms, simd: 9.8462ms

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_sphere_generation_simd() {
        // A small chunk for quick tests
        let resolution = UVec3::new(64, 64, 64);
        let chunk_data = build_sphere_data_simd_optimized(resolution, IVec3::new(0, 0, 0));

        // Corner (0,0,0) is definitely outside => BLOCK_AIR
        let corner_idx = index_3d(0, 0, 0, resolution);
        assert_eq!(
            chunk_data[corner_idx], BLOCK_AIR,
            "Corners should be outside the sphere => Air"
        );

        // Check near the center => likely inside => BLOCK_DIRT
        let center_idx = index_3d(32, 32, 32, resolution);
        assert_eq!(
            chunk_data[center_idx], BLOCK_DIRT,
            "Center should be inside the sphere => Dirt"
        );

        for i in 0..64 {
            for j in 0..64 {
                print!("{:?} ", chunk_data[index_3d(i, j, 5, resolution)]);
            }
            println!();
        }
    }

    #[test]
    fn test_sphere_generation_brute_force() {
        // A small chunk for quick tests
        let resolution = UVec3::new(64, 64, 64);
        let chunk_data = build_sphere_data_brute_force(resolution, IVec3::new(0, 0, 0));

        // Corner (0,0,0) is definitely outside => BLOCK_AIR
        let corner_idx = index_3d(0, 0, 0, resolution);
        assert_eq!(
            chunk_data[corner_idx], BLOCK_AIR,
            "Corners should be outside the sphere => Air"
        );

        // Check near the center => likely inside => BLOCK_DIRT
        let center_idx = index_3d(32, 32, 32, resolution);
        assert_eq!(
            chunk_data[center_idx], BLOCK_DIRT,
            "Center should be inside the sphere => Dirt"
        );

        for i in 0..64 {
            for j in 0..64 {
                print!("{:?} ", chunk_data[index_3d(i, j, 5, resolution)]);
            }
            println!();
        }
    }

    #[test]
    fn compare_brute_force_vs_simd() {
        let dims = [16, 128, 256];

        for &d in &dims {
            // Build sphere data using brute force
            let resolution = UVec3::new(d, d, d);
            let start = Instant::now();
            let _bf_data = build_sphere_data_brute_force(resolution, IVec3::new(0, 0, 0));
            let brute_force_duration = start.elapsed();

            // Build sphere data using SIMD
            let start = Instant::now();
            let _simd_data = build_sphere_data_simd_optimized(resolution, IVec3::new(0, 0, 0));
            let simd_duration = start.elapsed();

            println!(
                "Chunk dim: {:4}, brute force: {:?}, simd: {:?}",
                d, brute_force_duration, simd_duration
            );
        }
    }
}
