//! Clip allocated Contree nodes into a dense block instead of retracing every voxel.
use super::*;

pub(super) fn try_export(
    chunk_dim: UVec3,
    voxel_dim: UVec3,
    scene_chunks: &[Option<UVec3>],
    caches: &HashMap<UVec3, Arc<ContreeCpuChunkCache>>,
    min: UVec3,
    dim: UVec3,
    mask: u32,
) -> Option<Vec<u8>> {
    contree_levels_for_voxel_dim(voxel_dim).ok()?;
    // Beyond this range the legacy float-center sampler may lose half-voxel
    // precision. Leave those inputs, and malformed trees, on the original path.
    if (chunk_dim.as_u64vec3() * voxel_dim.as_u64vec3()).max_element() > (1 << 22) {
        return None;
    }
    let max = min + dim;
    let first = min / voxel_dim;
    let last = (max - UVec3::ONE) / voxel_dim;
    let mut writer = BlockWriter {
        min,
        max,
        dim,
        mask,
        bytes: vec![0; dim.x as usize * dim.y as usize * dim.z as usize],
    };
    for z in first.z..=last.z {
        for y in first.y..=last.y {
            for x in first.x..=last.x {
                let chunk = UVec3::new(x, y, z);
                if !contree_scene_chunk_present(chunk_dim, scene_chunks, chunk) {
                    continue;
                }
                let cache = caches.get(&chunk)?;
                if cache.chunk_idx != chunk {
                    return None;
                }
                if !cache.nodes.is_empty() {
                    writer.visit(cache, 0, chunk * voxel_dim, voxel_dim.x)?;
                }
            }
        }
    }
    Some(writer.bytes)
}

struct BlockWriter {
    min: UVec3,
    max: UVec3,
    dim: UVec3,
    mask: u32,
    bytes: Vec<u8>,
}

impl BlockWriter {
    fn visit(
        &mut self,
        cache: &ContreeCpuChunkCache,
        address: usize,
        origin: UVec3,
        side: u32,
    ) -> Option<()> {
        let node = *cache.nodes.get(address)?;
        let child_side = side.checked_div(4).filter(|&side| side > 0)?;
        if (node.packed_0 & 1 != 0) != (child_side == 1) {
            return None;
        }
        let mut children = u64::from(node.child_mask_lo) | (u64::from(node.child_mask_hi) << 32);
        let mut child_address = (node.packed_0 >> 1) as usize;
        while children != 0 {
            let index = children.trailing_zeros();
            children &= children - 1;
            let child = origin + UVec3::new(index % 4, index / 16, (index / 4) % 4) * child_side;
            if child.cmplt(self.max).all()
                && (child + UVec3::splat(child_side)).cmpgt(self.min).all()
            {
                if child_side == 1 {
                    let p = child - self.min;
                    let i = p.x as usize
                        + self.dim.x as usize * (p.y as usize + self.dim.y as usize * p.z as usize);
                    self.bytes[i] = (*cache.leaves.get(child_address)? & self.mask) as u8;
                } else {
                    self.visit(cache, child_address, child, child_side)?;
                }
            }
            child_address += 1;
        }
        Some(())
    }
}
