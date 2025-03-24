use glam::{IVec3, UVec3};
use noise::{NoiseFn, Perlin, Simplex};
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
            data: generate_chunk_cpu(resolution, position),
        }
    }
}

/// Linear index for a 3D coordinate (x,y,z) in a chunk with the given resolution.
#[inline]
fn index_3d(x: u32, y: u32, z: u32, resolution: UVec3) -> usize {
    (x * resolution.y * resolution.z + y * resolution.z + z) as usize
}

fn determine_voxel_type(weight: f32) -> u8 {
    if weight < 0.0 {
        BLOCK_AIR
    } else {
        BLOCK_DIRT
    }
}

pub fn generate_chunk_cpu(resolution: UVec3, chunk_pos: IVec3) -> Vec<u8> {
    let total_voxels = (resolution.x * resolution.y * resolution.z) as usize;
    let mut data = vec![BLOCK_AIR; total_voxels];

    let perlin = Simplex::new(999);
    for x in 0..resolution.x {
        let wx = chunk_pos.x as f64 + x as f64;
        for y in 0..resolution.y {
            let wy = chunk_pos.y as f64 + y as f64;
            for z in 0..resolution.z {
                let wz = chunk_pos.z as f64 + z as f64;
                let val = perlin.get([wx, wy, wz]);
            }
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn generate_chunk_test() {
        let dims = [16, 128, 256];

        // cpu test
        for &d in &dims {
            // TODO: generate chunk with the given dimensions, and print time respectivelly.
            let start = Instant::now();
            let chunk = generate_chunk_cpu(UVec3::new(d, d, d), IVec3::new(0, 0, 0));
            let duration = start.elapsed();
            println!("Chunk dim: {:4}, brute force: {:?}", d, duration);
        }
    }
}
