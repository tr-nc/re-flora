//! Persistent flora, independent of GPU allocations and of the process clock.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FloraSnapshot {
    chunks: Vec<ChunkSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct ChunkSnapshot {
    coordinate: [u32; 3],
    species: Vec<SpeciesSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct SpeciesSnapshot {
    key: String,
    // Packed local position/growth, then spawn animation age (MAX means inactive).
    instances: Vec<[u32; 2]>,
    // Response identity and seed, in the same order as authored instances.
    authored: Vec<[u64; 2]>,
}

impl FloraSnapshot {
    pub fn validate(&self, chunk_dim: UVec3, voxel_dim: UVec3) -> Result<()> {
        anyhow::ensure!(
            self.chunks.len() == chunk_dim.element_product() as usize,
            "garden must contain every flora chunk"
        );
        let mut chunks = HashSet::new();
        let mut identities = HashSet::new();
        for chunk in &self.chunks {
            let coordinate = UVec3::from_array(chunk.coordinate);
            anyhow::ensure!(
                coordinate.cmplt(chunk_dim).all() && chunks.insert(chunk.coordinate),
                "invalid or duplicate flora chunk"
            );
            anyhow::ensure!(
                chunk.species.len() == species::MAX_FLORA_SPECIES,
                "flora species count differs"
            );
            for (index, saved) in chunk.species.iter().enumerate() {
                let desc = &species::FLORA_SPECIES[index];
                anyhow::ensure!(saved.key == desc.key, "flora species schema differs");
                anyhow::ensure!(
                    saved.instances.len() <= MAX_FLORA_INSTANCES_PER_SPECIES as usize,
                    "too many flora instances"
                );
                let authored = desc.placement_mode == species::FloraPlacementMode::Authored;
                let mut roots = HashSet::new();
                anyhow::ensure!(
                    saved.authored.len() == if authored { saved.instances.len() } else { 0 },
                    "authored flora metadata differs from instance count"
                );
                for &[packed, _] in &saved.instances {
                    let local = UVec3::new(packed & 255, (packed >> 8) & 255, (packed >> 16) & 255);
                    anyhow::ensure!(local.cmplt(voxel_dim).all(), "flora root outside chunk");
                    anyhow::ensure!(
                        !authored || roots.insert(packed & 0x00ff_ffff),
                        "duplicate authored plant root"
                    );
                }
                for &[id, seed] in &saved.authored {
                    anyhow::ensure!(
                        id < u64::MAX && seed <= u32::MAX as u64 && identities.insert(id),
                        "invalid or duplicate authored flora identity"
                    );
                }
            }
        }
        Ok(())
    }

    pub fn counts(&self) -> (usize, usize) {
        self.chunks
            .iter()
            .flat_map(|chunk| &chunk.species)
            .fold((0, 0), |(total, authored), s| {
                (total + s.instances.len(), authored + s.authored.len())
            })
    }

    pub fn same_layout_and_growth(&self, other: &Self) -> bool {
        self.chunks.len() == other.chunks.len()
            && self.chunks.iter().zip(&other.chunks).all(|(a, b)| {
                a.coordinate == b.coordinate
                    && a.species.len() == b.species.len()
                    && a.species.iter().zip(&b.species).all(|(a, b)| {
                        a.key == b.key
                            && a.authored == b.authored
                            && a.instances.len() == b.instances.len()
                            && a.instances
                                .iter()
                                .zip(&b.instances)
                                .all(|(a, b)| a[0] == b[0])
                    })
            })
    }
}

impl SurfaceBuilder {
    /// Caller has quiesced GPU writes. Read the live buffer, not stale CPU growth values.
    pub fn capture_flora_snapshot(&self, now_ms: u32) -> Result<FloraSnapshot> {
        let mut chunks = Vec::new();
        for (_, resources) in &self.resources.instances.chunk_flora_instances {
            let mut saved_species = Vec::new();
            for (index, desc) in species::FLORA_SPECIES.iter().enumerate() {
                let count = resources.species_len(index) as usize;
                let mut saved = SpeciesSnapshot {
                    key: desc.key.to_owned(),
                    instances: Vec::new(),
                    authored: Vec::new(),
                };
                if count > 0 {
                    let offset = FloraInstanceResources::species_offset(index) as usize
                        * size_of::<Instance>();
                    let bytes = resources
                        .resource
                        .instances_buf
                        .read_back_range(offset as u64, (count * size_of::<Instance>()) as u64)?;
                    let instances: &[Instance] = bytemuck::try_cast_slice(&bytes)
                        .map_err(|error| anyhow::anyhow!("flora snapshot readback: {error}"))?;
                    let authored_by_position = self
                        .authored_flora
                        .instances_for_chunk(resources.chunk_id)
                        .iter()
                        .filter(|instance| instance.species_index == index as u32)
                        .map(|instance| (instance.base_world_vox, instance))
                        .collect::<HashMap<_, _>>();
                    for instance in instances {
                        let age = if instance.spawn_start_ms == u32::MAX {
                            u32::MAX
                        } else {
                            now_ms.wrapping_sub(instance.spawn_start_ms)
                        };
                        saved.instances.push([instance.packed_local_pos, age]);
                        if desc.placement_mode == species::FloraPlacementMode::Authored {
                            let position = unpack_flora_instance_local_position(*instance)
                                + resources.chunk_world_offset;
                            let authored =
                                authored_by_position.get(&position).ok_or_else(|| {
                                    anyhow::anyhow!("authored flora GPU/CPU roots differ")
                                })?;
                            saved
                                .authored
                                .push([authored.response_id, authored.seed as u64]);
                        }
                    }
                }
                saved_species.push(saved);
            }
            chunks.push(ChunkSnapshot {
                coordinate: resources.chunk_id.to_array(),
                species: saved_species,
            });
        }
        chunks.sort_by_key(|chunk| chunk.coordinate);
        Ok(FloraSnapshot { chunks })
    }

    /// Replaces, never appends. Resources and competition fields are rebuilt, not serialized.
    pub fn restore_flora_snapshot(&mut self, snapshot: &FloraSnapshot, now_ms: u32) -> Result<()> {
        self.authored_flora = AuthoredFloraStore::default();
        for chunk in &snapshot.chunks {
            let chunk_id = UVec3::from_array(chunk.coordinate);
            let chunk_index = self.get_chunk_resource_index(chunk_id)?;
            let resources = &mut self.resources.instances.chunk_flora_instances[chunk_index].1;
            resources.authored_response_instances.clear();
            for (index, saved) in chunk.species.iter().enumerate() {
                let instances = saved
                    .instances
                    .iter()
                    .map(|&[packed_local_pos, age]| Instance {
                        packed_local_pos,
                        spawn_start_ms: if age == u32::MAX {
                            u32::MAX
                        } else {
                            now_ms.wrapping_sub(age)
                        },
                    })
                    .collect::<Vec<_>>();
                resources.write_species_instances(index, &instances)?;
                for (instance, &[response_id, seed]) in instances.iter().zip(&saved.authored) {
                    let authored = AuthoredFloraInstance {
                        response_id,
                        seed: seed as u32,
                        species_index: index as u32,
                        base_world_vox: unpack_flora_instance_local_position(*instance)
                            + resources.chunk_world_offset,
                        growth_progress: instance.packed_local_pos >> 24,
                        spawn_start_ms: instance.spawn_start_ms,
                    };
                    self.authored_flora.next_response_id =
                        self.authored_flora.next_response_id.max(response_id);
                    self.authored_flora
                        .instances_for_chunk_mut(chunk_id)
                        .push(authored);
                    resources.authored_response_instances.push(authored);
                }
            }
        }
        for chunk in &snapshot.chunks {
            self.rebuild_grass_growth_potential_for_chunk(UVec3::from_array(chunk.coordinate))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> FloraSnapshot {
        FloraSnapshot {
            chunks: vec![ChunkSnapshot {
                coordinate: [0, 0, 0],
                species: species::FLORA_SPECIES
                    .iter()
                    .map(|desc| SpeciesSnapshot {
                        key: desc.key.to_owned(),
                        instances: Vec::new(),
                        authored: Vec::new(),
                    })
                    .collect(),
            }],
        }
    }

    #[test]
    fn grass_and_authored_snapshot_preserves_growth_identity_and_age() {
        let mut snapshot = fixture();
        snapshot.chunks[0].species[0].instances.push([
            pack_flora_instance(UVec3::new(3, 2, 1), 123, 0).packed_local_pos,
            450,
        ]);
        snapshot.chunks[0].species[2].instances.push([
            pack_flora_instance(UVec3::new(7, 2, 1), 254, 0).packed_local_pos,
            u32::MAX,
        ]);
        snapshot.chunks[0].species[2]
            .authored
            .push([u64::MAX - 1, u32::MAX as u64]);
        snapshot.validate(UVec3::ONE, UVec3::splat(8)).unwrap();
        let encoded = serde_json::to_vec(&snapshot).unwrap();
        let decoded: FloraSnapshot = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(snapshot, decoded);
        assert_eq!(snapshot.counts(), (2, 1));
    }

    #[test]
    fn malformed_flora_never_reaches_gpu_restoration() {
        let mut snapshot = fixture();
        snapshot.validate(UVec3::ONE, UVec3::splat(8)).unwrap();
        snapshot.chunks[0].species[2].instances.push([0, 0]);
        assert!(snapshot.validate(UVec3::ONE, UVec3::splat(8)).is_err());
        snapshot.chunks[0].species[2].authored.push([1, 2]);
        snapshot.validate(UVec3::ONE, UVec3::splat(8)).unwrap();
        snapshot.chunks[0].species[3].instances.push([0, 0]);
        snapshot.chunks[0].species[3].authored.push([1, 2]);
        assert!(snapshot.validate(UVec3::ONE, UVec3::splat(8)).is_err());
        snapshot.chunks[0].species[3].authored[0][0] = 2;
        snapshot.chunks[0].species[0].instances.push([255, 0]);
        assert!(snapshot.validate(UVec3::ONE, UVec3::splat(8)).is_err());
    }
}
