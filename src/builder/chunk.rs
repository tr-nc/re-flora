use glam::{IVec3, UVec3};

pub struct Chunk {
    pub resolution: UVec3,
    pub position: IVec3,
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn new(resolution: UVec3, position: IVec3) -> Self {
        Self {
            resolution,
            position,
            data: Self::build_chunk_data(resolution),
        }
    }

    // TODO: alter this function, so that it utilizes std::simd for better performance, to construct a chunk of data
    // the data is a 3D array of u8 values, where each value represents a voxel, make changes to the type of `data` for better performance if possible.
    // (like using a hash table, or whatever you think is best)
    // this function should be expandable and editable, i.e. for each voxel element, get the absolute position in the world space, 
    // by using the local ID of the voxel and the position of the chunk in the world space. And a density function is applied to each of the voxels to 
    // obtain the weight of the voxel, then a function can be applied to the weight to get the type of the voxel, like air, water, dirt, etc.
    // each type is denoted by a unique u8 value.
    // for a simple test, just fill the chunk with dirt. add a test for this function in place.
    fn build_chunk_data(resolution: UVec3) -> Vec<u8> {
        let mut data = vec![0; (resolution.x * resolution.y * resolution.z) as usize];
        for x in 0..resolution.x {
            for y in 0..resolution.y {
                for z in 0..resolution.z {
                    let index = (x * resolution.y * resolution.z + y * resolution.z + z) as usize;
                    data[index] = 1;
                }
            }
        }
        data
    }
}
