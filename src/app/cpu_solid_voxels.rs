use glam::{UVec3, Vec3};
use std::{collections::HashMap, sync::Arc};

pub(crate) const EMPTY_VOXEL_TYPE: u8 = 0;

#[derive(Debug)]
pub(crate) struct CpuSolidVoxelChunk {
    chunk_id: UVec3,
    dim: UVec3,
    solid_bits: Vec<u64>,
    solid_count: usize,
    revision: u64,
}

#[derive(Default)]
pub(crate) struct CpuSolidVoxelStore {
    chunks: HashMap<UVec3, Arc<CpuSolidVoxelChunk>>,
}

impl CpuSolidVoxelStore {
    pub(crate) fn upsert_from_voxel_types(
        &mut self,
        chunk_id: UVec3,
        dim: UVec3,
        voxel_types: &[u8],
    ) -> anyhow::Result<(Arc<CpuSolidVoxelChunk>, bool)> {
        let (solid_bits, solid_count) = solid_bits_from_voxel_types(dim, voxel_types)?;
        if let Some(existing) = self.chunks.get(&chunk_id) {
            if existing.dim == dim
                && existing.solid_count == solid_count
                && existing.solid_bits == solid_bits
            {
                return Ok((Arc::clone(existing), false));
            }
        }

        let revision = self
            .chunks
            .get(&chunk_id)
            .map_or(1, |chunk| chunk.revision.saturating_add(1));
        let chunk = Arc::new(CpuSolidVoxelChunk {
            chunk_id,
            dim,
            solid_bits,
            solid_count,
            revision,
        });
        self.chunks.insert(chunk_id, Arc::clone(&chunk));
        Ok((chunk, true))
    }

    pub(crate) fn get(&self, chunk_id: UVec3) -> Option<Arc<CpuSolidVoxelChunk>> {
        self.chunks.get(&chunk_id).cloned()
    }

    pub(crate) fn revision(&self, chunk_id: UVec3) -> Option<u64> {
        self.chunks.get(&chunk_id).map(|chunk| chunk.revision)
    }
}

impl CpuSolidVoxelChunk {
    #[cfg(test)]
    fn from_voxel_types(
        chunk_id: UVec3,
        dim: UVec3,
        revision: u64,
        voxel_types: &[u8],
    ) -> anyhow::Result<Self> {
        let (solid_bits, solid_count) = solid_bits_from_voxel_types(dim, voxel_types)?;

        Ok(Self {
            chunk_id,
            dim,
            solid_bits,
            solid_count,
            revision,
        })
    }

    pub(crate) fn chunk_id(&self) -> UVec3 {
        self.chunk_id
    }

    pub(crate) fn dim(&self) -> UVec3 {
        self.dim
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn solid_count(&self) -> usize {
        self.solid_count
    }

    pub(crate) fn is_solid_local_voxel(&self, local_voxel: UVec3) -> bool {
        if local_voxel.cmpge(self.dim).any() {
            return false;
        }
        let idx = flat_index(self.dim, local_voxel);
        self.solid_bits
            .get(idx / u64::BITS as usize)
            .is_some_and(|bits| (bits & (1u64 << (idx % u64::BITS as usize))) != 0)
    }

    pub(crate) fn sample_solid_ws(&self, point_ws: Vec3) -> bool {
        if !point_ws.is_finite() {
            return false;
        }

        let local = (point_ws - self.chunk_id.as_vec3()) * self.dim.as_vec3();
        let max = self.dim.as_ivec3() - glam::IVec3::ONE;
        let local_voxel = local.floor().as_ivec3().clamp(glam::IVec3::ZERO, max);
        self.is_solid_local_voxel(local_voxel.as_uvec3())
    }
}

fn solid_bits_from_voxel_types(
    dim: UVec3,
    voxel_types: &[u8],
) -> anyhow::Result<(Vec<u64>, usize)> {
    let voxel_count = voxel_count(dim)?;
    if voxel_types.len() < voxel_count {
        anyhow::bail!(
            "CPU solid voxel source is too small: got {}, need {} for dim {:?}",
            voxel_types.len(),
            voxel_count,
            dim,
        );
    }

    let mut solid_bits = vec![0; voxel_count.div_ceil(u64::BITS as usize)];
    let mut solid_count = 0;
    for (idx, &voxel_type) in voxel_types.iter().take(voxel_count).enumerate() {
        if voxel_type != EMPTY_VOXEL_TYPE {
            solid_bits[idx / u64::BITS as usize] |= 1u64 << (idx % u64::BITS as usize);
            solid_count += 1;
        }
    }

    Ok((solid_bits, solid_count))
}

fn voxel_count(dim: UVec3) -> anyhow::Result<usize> {
    let count = dim.x as u64 * dim.y as u64 * dim.z as u64;
    usize::try_from(count).map_err(|_| anyhow::anyhow!("voxel chunk {:?} is too large", dim))
}

fn flat_index(dim: UVec3, local_voxel: UVec3) -> usize {
    ((local_voxel.z as usize * dim.y as usize + local_voxel.y as usize) * dim.x as usize)
        + local_voxel.x as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_solid_voxels_as_bits() {
        let mut store = CpuSolidVoxelStore::default();
        let mut voxel_types = vec![0; 64];
        voxel_types[1] = 7;
        let (chunk, changed) = store
            .upsert_from_voxel_types(UVec3::new(1, 2, 3), UVec3::new(4, 4, 4), &voxel_types)
            .unwrap();

        assert!(changed);
        assert_eq!(chunk.revision(), 1);
        assert_eq!(chunk.solid_count(), 1);
        assert!(chunk.is_solid_local_voxel(UVec3::new(1, 0, 0)));
        assert!(!chunk.is_solid_local_voxel(UVec3::new(0, 0, 0)));
    }

    #[test]
    fn unchanged_upsert_preserves_existing_chunk_revision() {
        let mut store = CpuSolidVoxelStore::default();
        let mut voxel_types = vec![0; 8];
        voxel_types[1] = 7;

        let (first, changed) = store
            .upsert_from_voxel_types(UVec3::new(1, 0, 1), UVec3::new(2, 2, 2), &voxel_types)
            .unwrap();
        assert!(changed);

        let (same, changed) = store
            .upsert_from_voxel_types(UVec3::new(1, 0, 1), UVec3::new(2, 2, 2), &voxel_types)
            .unwrap();
        assert!(!changed);
        assert_eq!(same.revision(), first.revision());
        assert!(Arc::ptr_eq(&same, &first));

        voxel_types[2] = 9;
        let (updated, changed) = store
            .upsert_from_voxel_types(UVec3::new(1, 0, 1), UVec3::new(2, 2, 2), &voxel_types)
            .unwrap();
        assert!(changed);
        assert_eq!(updated.revision(), first.revision() + 1);
    }

    #[test]
    fn world_sampling_clamps_to_chunk_voxels() {
        let chunk = CpuSolidVoxelChunk::from_voxel_types(
            UVec3::new(1, 0, 1),
            UVec3::new(2, 2, 2),
            1,
            &[0, 0, 0, 0, 0, 0, 0, 9],
        )
        .unwrap();

        assert!(chunk.sample_solid_ws(Vec3::new(2.0, 1.0, 2.0)));
        assert!(!chunk.sample_solid_ws(Vec3::new(1.1, 0.1, 1.1)));
    }
}
