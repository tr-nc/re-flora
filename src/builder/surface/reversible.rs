//! Exact removal receipts for an editable gesture, not a second flora snapshot.
//! Restore only removed roots; surviving plants retain their live growth and age.
use super::*;
use std::{collections::HashSet, mem::size_of};

pub struct FloraRemovalReceipt {
    chunk: UVec3,
    species: Vec<Vec<Instance>>,
    authored: Vec<AuthoredFloraInstance>,
}
fn identity(i: &Instance) -> u32 {
    i.packed_local_pos & 0xffffff
}
fn removed_instances(before: &[Instance], after: &[Instance]) -> Vec<Instance> {
    let remaining: HashSet<_> = after.iter().map(identity).collect();
    before
        .iter()
        .filter(|i| !remaining.contains(&identity(i)))
        .copied()
        .collect()
}
fn merge_instances(live: &mut Vec<Instance>, removed: &[Instance]) {
    let mut present: HashSet<_> = live.iter().map(identity).collect();
    live.extend(
        removed
            .iter()
            .filter(|i| present.insert(identity(i)))
            .copied(),
    );
}
impl SurfaceBuilder {
    fn read_flora_species(&self, chunk: UVec3) -> Result<Vec<Vec<Instance>>> {
        let index = self.get_chunk_resource_index(chunk)?;
        let resources = &self.resources.instances.chunk_flora_instances[index].1;
        (0..species::FLORA_SPECIES.len())
            .map(|species| {
                let count = resources.species_len(species) as usize;
                if count == 0 {
                    return Ok(Vec::new());
                }
                let offset = FloraInstanceResources::species_offset(species) as usize
                    * size_of::<Instance>();
                let bytes = resources
                    .resource
                    .instances_buf
                    .read_back_range(offset as u64, (count * size_of::<Instance>()) as u64)?;
                Ok(bytemuck::try_cast_slice::<u8, Instance>(&bytes)
                    .map_err(|e| anyhow::anyhow!("flora removal readback: {e}"))?
                    .to_vec())
            })
            .collect()
    }
    /// Authored plants retain their immediate burial policy. Ordinary grass has
    /// a separate support/three-layer burial stage in reconcile_grass_terrain.
    pub fn remove_authored_flora_buried_by_voxels(&mut self, deposited: &[UVec3]) -> Result<()> {
        let mut roots_by_chunk: HashMap<UVec3, HashSet<UVec3>> = HashMap::new();
        for &voxel in deposited {
            if voxel.y > 0 {
                let root = voxel - UVec3::Y;
                roots_by_chunk
                    .entry(self.chunk_id_for_world_voxel(root))
                    .or_default()
                    .insert(root);
            }
        }
        let mut dirty_growth = HashSet::new();
        for (chunk, buried) in roots_by_chunk {
            let mut live = self.read_flora_species(chunk)?;
            let index = self.get_chunk_resource_index(chunk)?;
            for (species, instances) in live
                .iter_mut()
                .enumerate()
                .skip(species::MODEL_FLOWER_FIRST_SPECIES as usize)
            {
                let before = instances.len();
                instances.retain(|instance| {
                    !buried.contains(
                        &(unpack_flora_instance_local_position(*instance)
                            + chunk * self.voxel_dim_per_chunk),
                    )
                });
                if before != instances.len() {
                    self.resources.instances.chunk_flora_instances[index]
                        .1
                        .write_species_instances(species, instances)?;
                }
            }
            let removed: Vec<_> = self
                .authored_flora
                .instances_for_chunk(chunk)
                .iter()
                .filter(|root| buried.contains(&root.base_world_vox))
                .copied()
                .collect();
            self.authored_flora
                .instances_for_chunk_mut(chunk)
                .retain(|root| !buried.contains(&root.base_world_vox));
            self.resources.instances.chunk_flora_instances[index]
                .1
                .authored_response_instances
                .retain(|root| !buried.contains(&root.base_world_vox));
            for root in removed {
                let radius = species::species()[root.species_index as usize]
                    .grass_growth_influence_radius_voxels;
                dirty_growth.extend(
                    self.grass_growth_potential_affected_chunks(root.base_world_vox, radius),
                );
            }
        }
        for chunk in dirty_growth {
            self.rebuild_grass_growth_potential_for_chunk(chunk)?;
        }
        Ok(())
    }

    pub fn remove_flora_reversibly(
        &mut self,
        chunk: UVec3,
        start: Vec3,
        end: Vec3,
        radius: f32,
        tick: u32,
        now_ms: u32,
    ) -> Result<FloraRemovalReceipt> {
        let before = self.read_flora_species(chunk)?;
        let authored = self.authored_flora.instances_for_chunk(chunk).to_vec();
        self.edit_flora_instances_for_brush(chunk, start, end, radius, tick, now_ms)?;
        let after = self.read_flora_species(chunk)?;
        let species: Vec<_> = before
            .iter()
            .zip(&after)
            .map(|(a, b)| removed_instances(a, b))
            .collect();
        let authored = authored
            .into_iter()
            .filter_map(|mut root| {
                let local = root.base_world_vox - chunk * self.voxel_dim_per_chunk;
                let position = pack_flora_instance(local, 0, 0).packed_local_pos;
                let instance = species[root.species_index as usize]
                    .iter()
                    .find(|i| identity(i) == position)?;
                root.growth_progress = instance.packed_local_pos >> 24;
                root.spawn_start_ms = instance.spawn_start_ms;
                Some(root)
            })
            .collect();
        Ok(FloraRemovalReceipt {
            chunk,
            species,
            authored,
        })
    }
    pub fn restore_removed_flora(&mut self, receipt: &FloraRemovalReceipt) -> Result<()> {
        for root in &receipt.authored {
            anyhow::ensure!(
                !self
                    .authored_flora
                    .instances_for_chunk(receipt.chunk)
                    .iter()
                    .any(|existing| (existing.base_world_vox == root.base_world_vox
                        && existing.response_id != root.response_id)
                        || (existing.response_id == root.response_id
                            && existing.base_world_vox != root.base_world_vox)),
                "flora withdrawal conflicts with another authored root; it was not overwritten"
            );
        }
        // A retry must not republish an older GPU copy of an authored root
        // that is already live (including growth or species changes).
        let authored: Vec<_> = receipt
            .authored
            .iter()
            .filter(|root| {
                !self
                    .authored_flora
                    .instances_for_chunk(receipt.chunk)
                    .iter()
                    .any(|live| live.response_id == root.response_id)
            })
            .copied()
            .collect();
        let positions: HashSet<_> = authored
            .iter()
            .map(|root| {
                (
                    root.species_index,
                    pack_flora_instance(
                        root.base_world_vox - receipt.chunk * self.voxel_dim_per_chunk,
                        0,
                        0,
                    )
                    .packed_local_pos,
                )
            })
            .collect();
        let mut live = self.read_flora_species(receipt.chunk)?;
        let index = self.get_chunk_resource_index(receipt.chunk)?;
        for (species, removed) in receipt.species.iter().enumerate() {
            if removed.is_empty() {
                continue;
            }
            let restore: Vec<_> = removed
                .iter()
                .filter(|instance| {
                    species::FLORA_SPECIES[species].placement_mode
                        != species::FloraPlacementMode::Authored
                        || positions.contains(&(species as u32, identity(instance)))
                })
                .copied()
                .collect();
            if restore.is_empty() {
                continue;
            }
            merge_instances(&mut live[species], &restore);
            self.resources.instances.chunk_flora_instances[index]
                .1
                .write_species_instances(species, &live[species])?;
        }
        let mut dirty = HashSet::new();
        for root in &authored {
            let roots = self.authored_flora.instances_for_chunk_mut(receipt.chunk);
            if roots.iter().any(|r| {
                r.response_id == root.response_id || r.base_world_vox == root.base_world_vox
            }) {
                continue;
            }
            roots.push(*root);
            self.authored_flora.next_response_id =
                self.authored_flora.next_response_id.max(root.response_id);
            self.resources.instances.chunk_flora_instances[index]
                .1
                .authored_response_instances
                .push(*root);
            let radius = species::species()[root.species_index as usize]
                .grass_growth_influence_radius_voxels;
            dirty.extend(self.grass_growth_potential_affected_chunks(root.base_world_vox, radius));
        }
        dirty.insert(receipt.chunk);
        for chunk in dirty {
            self.rebuild_grass_growth_potential_for_chunk(chunk)?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flora_removal_receipt_restores_only_deleted_roots_without_rewinding_survivors() {
        let a = pack_flora_instance(UVec3::new(1, 2, 3), 12, 40);
        let b = pack_flora_instance(UVec3::new(2, 3, 4), 42, 50);
        let grown = pack_flora_instance(UVec3::new(2, 3, 4), 200, 50);
        let receipt = removed_instances(&[a, b], &[grown]);
        assert_eq!(receipt.len(), 1);
        let mut live = vec![grown];
        merge_instances(&mut live, &receipt);
        merge_instances(&mut live, &receipt);
        assert_eq!(live.len(), 2);
        assert_eq!(live[0].packed_local_pos, grown.packed_local_pos);
        assert_eq!(live[1].packed_local_pos, a.packed_local_pos);
        assert_eq!(live[1].spawn_start_ms, 40);
    }
}
