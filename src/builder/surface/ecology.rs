//! Observe committed instance storage. The caller runs after loading/edit/growth jobs complete;
//! these paths already wait for their own fences. This observer submits no jobs or waits.
use super::*;
use crate::ecology::{Habitat, Region, RegionKey};
impl SurfaceBuilder {
    pub(crate) fn ecology_regions(&self) -> Vec<Region> {
        self.resources
            .instances
            .chunk_flora_instances
            .iter()
            .enumerate()
            .flat_map(|(chunk, (_, r))| {
                (0..self.flora_species_count).map(move |kind| Region {
                    key: RegionKey::Surface(chunk, kind as u32),
                    kind: usize::from(!species::is_grass_species_index(kind as u32)),
                    count: r.species_len(kind),
                    center: (r.chunk_world_offset.as_vec3()
                        + self.voxel_dim_per_chunk.as_vec3() * 0.5)
                        / 256.,
                    radius: self.voxel_dim_per_chunk.as_vec3().length() * 0.5 / 256.,
                })
            })
            .collect()
    }
    pub(crate) fn sample_ecology_root(
        &self,
        chunk: usize,
        kind: u32,
        slot: u32,
    ) -> Result<Option<Habitat>> {
        let Some((_, r)) = self.resources.instances.chunk_flora_instances.get(chunk) else {
            return Ok(None);
        };
        if kind as usize >= self.flora_species_count || slot >= r.species_len(kind as usize) {
            return Ok(None);
        }
        let bytes = r.resource.instances_buf.read_back_range(
            u64::from(FloraInstanceResources::species_offset(kind as usize) + slot) * 8,
            8,
        )?;
        let instance: resources::Instance = bytemuck::pod_read_unaligned(&bytes);
        Ok(root_habitat(
            instance,
            r.chunk_world_offset,
            self.voxel_dim_per_chunk,
            chunk,
            kind,
            slot,
        ))
    }
}
fn root_habitat(
    instance: resources::Instance,
    offset: UVec3,
    dim: UVec3,
    chunk: usize,
    kind: u32,
    slot: u32,
) -> Option<Habitat> {
    let local = unpack_flora_instance_local_position(instance);
    if instance.packed_local_pos >> 24 == 0 || !local.cmplt(dim).all() {
        return None;
    }
    Some(Habitat {
        region: RegionKey::Surface(chunk, kind),
        slot,
        token: u64::from(instance.packed_local_pos & 0xffffff)
            | (u64::from(instance.spawn_start_ms) << 32),
        position: ((offset + local).as_vec3() + Vec3::splat(0.5)) / 256.,
        kind: usize::from(!species::is_grass_species_index(kind)),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_live_valid_roots_supply_habitats_and_growth_does_not_change_identity() {
        let mut i = resources::Instance {
            packed_local_pos: 12 | (34 << 8) | (56 << 16),
            spawn_start_ms: 42,
        };
        assert!(root_habitat(i, UVec3::ZERO, UVec3::splat(256), 0, 0, 0).is_none());
        i.packed_local_pos |= 1 << 24;
        let a = root_habitat(i, UVec3::new(256, 0, 0), UVec3::splat(256), 0, 0, 0).unwrap();
        i.packed_local_pos |= 255 << 24;
        assert_eq!(
            Some(a),
            root_habitat(i, UVec3::new(256, 0, 0), UVec3::splat(256), 0, 0, 0)
        );
        i.spawn_start_ms += 1;
        assert_ne!(
            a.token,
            root_habitat(i, UVec3::ZERO, UVec3::splat(256), 0, 0, 0)
                .unwrap()
                .token
        );
        assert!(root_habitat(i, UVec3::ZERO, UVec3::splat(8), 0, 0, 0).is_none());
    }
}
