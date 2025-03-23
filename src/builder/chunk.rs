use glam::{IVec3, UVec3};
use std::simd::Simd;

pub struct Chunk {
    pub resolution: UVec3,
    pub position: IVec3,
    /// Voxel data stored in a contiguous array. Each `u8` is a voxel type.
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn new(resolution: UVec3, position: IVec3) -> Self {
        Self {
            resolution,
            position,
            data: Self::build_chunk_data(resolution, position),
        }
    }

    /// Construct a chunk of data using SIMD for filling as a simple demonstration.
    /// This fills the chunk entirely with `1` (e.g., "dirt" voxels) for now.
    ///
    /// In the future, you could:
    /// - Use local voxel IDs plus `position` to compute absolute world positions.
    /// - Apply a density function to get a "weight" at each voxel.
    /// - Map that weight to physical types (0 = air, 1 = dirt, 2 = water, etc.).
    ///
    /// If the world is sparse, you might consider using a sparse structure (like a hash map)
    /// instead of a fully dense array, but for a standard dense chunk, a contiguous array is fine.
    fn build_chunk_data(resolution: UVec3, _position: IVec3) -> Vec<u8> {
        let total_voxels = resolution.x * resolution.y * resolution.z;
        let mut data = vec![0_u8; total_voxels as usize];

        // We’ll fill in blocks of 16 voxels at a time (you can adjust this to 8, 32, or another power of 2).
        const LANES: usize = 16;
        let fill_val = Simd::from_array([1; LANES]);

        // We’ll stride over our data in steps of LANES and fill simultaneously.
        // The leftover remainder (if not multiple-of-16 length) will be handled with a simple loop at the end.
        let simd_chunks = (total_voxels / LANES as u32) as usize;
        for chunk_idx in 0..simd_chunks {
            let offset = chunk_idx * LANES;
            // Write 16 "1" values at once.
            fill_val.copy_to_slice(&mut data[offset..offset + LANES]);
        }

        // Fill the remainder
        let remainder = total_voxels as usize % LANES;
        if remainder != 0 {
            let start = simd_chunks * LANES;
            for i in 0..remainder {
                data[start + i] = 1;
            }
        }

        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_chunk_data_all_dirt() {
        let resolution = UVec3::new(256, 256, 256);
        let chunk = Chunk::new(resolution, IVec3::new(0, 0, 0));

        // We expect every voxel to be set to 1.
        for &voxel_type in &chunk.data {
            assert_eq!(voxel_type, 1);
        }
    }
}
