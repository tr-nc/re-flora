//! Explicit native acceptance fixture, never part of normal gameplay/unit tests.
use super::*;
use crate::flora::hydration::Settings;

impl SurfaceBuilder {
    pub fn verify_hydration_runtime(
        &mut self,
        plain: &mut crate::builder::PlainBuilder,
    ) -> Result<()> {
        let mut chosen = Vec::new();
        for authored in [false, true] {
            let wanted = (0..species::species_count())
                .find(|&s| {
                    (s >= species::MODEL_FLOWER_FIRST_SPECIES as usize) == authored
                        && self
                            .resources
                            .instances
                            .chunk_flora_instances
                            .iter()
                            .any(|(_, r)| !r.hydration[s].is_empty())
                })
                .ok_or_else(|| anyhow::anyhow!("hydration review missing authored={authored}"))?;
            for (chunk, (_, resource)) in self
                .resources
                .instances
                .chunk_flora_instances
                .iter()
                .enumerate()
            {
                if let Some(&(root, _)) = resource.hydration[wanted].first() {
                    chosen.push((chunk, wanted, root));
                    break;
                }
            }
        }
        anyhow::ensure!(
            chosen.len() == 2,
            "hydration review requires grass and a flower"
        );
        for &(chunk, s, root) in &chosen {
            let r = &mut self.resources.instances.chunk_flora_instances[chunk].1;
            r.hydration[s][0].1 = 0.2;
            r.upload_hydration(s)?;
            let p = r.chunk_world_offset
                + UVec3::new(root & 255, (root >> 8) & 255, (root >> 16) & 255);
            plain.write_chunk_atlas_region(
                p,
                UVec3::ONE,
                &[crate::builder::VOXEL_TYPE_DIRT as u8 | 0x30],
            )?;
        }
        let settings = Settings {
            uptake_seconds: 1.0,
            transpiration_seconds: 10.0,
        };
        self.advance_plant_hydration(plain, 0.25, settings)?;
        for &(chunk, s, _) in &chosen {
            let h = self.resources.instances.chunk_flora_instances[chunk]
                .1
                .hydration[s][0]
                .1;
            anyhow::ensure!(h > 0.2 && h < 0.5, "wet root must recover gradually: {h}");
        }
        let saved = self.capture_flora_snapshot(0)?;
        let serialized = serde_json::to_vec(&saved)?;
        let decoded: snapshot::FloraSnapshot = serde_json::from_slice(&serialized)?;
        decoded.validate(self.chunk_bound.max(), self.voxel_dim_per_chunk)?;
        // Mutate real state, then restore using the same public path as garden Load.
        self.advance_plant_hydration(plain, 0.25, settings)?;
        self.restore_flora_snapshot(&decoded, 0)?;
        anyhow::ensure!(
            self.capture_flora_snapshot(0)? == saved,
            "hydration restore must be exact"
        );
        for &(chunk, s, root) in &chosen {
            let r = &mut self.resources.instances.chunk_flora_instances[chunk].1;
            let before = r.hydration[s].clone();
            let bytes = r.resource.instances_buf.read_back_range(
                u64::from(FloraInstanceResources::species_offset(s))
                    * std::mem::size_of::<Instance>() as u64,
                u64::from(r.species_len(s)) * std::mem::size_of::<Instance>() as u64,
            )?;
            let mut instances = bytemuck::try_cast_slice::<u8, Instance>(&bytes)
                .map_err(|e| anyhow::anyhow!("hydration review: {e}"))?
                .to_vec();
            instances.reverse();
            r.write_species_instances(s, &instances)?;
            for &(position, h) in &before {
                anyhow::ensure!(
                    r.hydration[s].iter().find(|p| p.0 == position).unwrap().1 == h,
                    "draw reorder reset hydration"
                );
            }
            // Remove and replant a real root: a new lifetime must not inherit old stress.
            instances.retain(|i| i.packed_local_pos & 0xffffff != root);
            r.write_species_instances(s, &instances)?;
            instances.push(pack_flora_instance(
                UVec3::new(root & 255, (root >> 8) & 255, (root >> 16) & 255),
                255,
                123,
            ));
            r.write_species_instances(s, &instances)?;
            anyhow::ensure!(
                r.hydration[s].last().unwrap().1 == crate::flora::hydration::DEFAULT_HYDRATION,
                "replanted root inherited former plant water"
            );
        }
        self.restore_flora_snapshot(&decoded, 0)?;
        // Exercise the real reversible tool path, not just an isolated vector remap.
        let (chunk, _, root) = chosen[0];
        let resource = &self.resources.instances.chunk_flora_instances[chunk].1;
        let chunk_id = resource.chunk_id;
        let world = resource.chunk_world_offset
            + UVec3::new(root & 255, (root >> 8) & 255, (root >> 16) & 255);
        let before: Vec<std::collections::BTreeMap<_, _>> = resource
            .hydration
            .iter()
            .map(|plants| plants.iter().copied().collect())
            .collect();
        let center = (world.as_vec3() + Vec3::new(0.5, 1.0, 0.5)) / 256.0;
        let receipt = self.remove_flora_reversibly(chunk_id, center, center, 0.02, 0, 0)?;
        anyhow::ensure!(
            self.resources.instances.chunk_flora_instances[chunk]
                .1
                .hydration
                .iter()
                .map(Vec::len)
                .sum::<usize>()
                < before
                    .iter()
                    .map(std::collections::BTreeMap::len)
                    .sum::<usize>(),
            "review removal must remove roots"
        );
        self.restore_removed_flora(&receipt)?;
        let after: Vec<std::collections::BTreeMap<_, _>> =
            self.resources.instances.chunk_flora_instances[chunk]
                .1
                .hydration
                .iter()
                .map(|plants| plants.iter().copied().collect())
                .collect();
        anyhow::ensure!(
            before == after,
            "reversible removal must preserve water without rewinding survivors"
        );
        self.restore_flora_snapshot(&decoded, 0)?;
        // End with dry roots and no root-zone reserve, to isolate stored tissue water.
        let mut old = Vec::new();
        for &(chunk, s, root) in &chosen {
            let r = &mut self.resources.instances.chunk_flora_instances[chunk].1;
            old.push(r.hydration[s][0].1);
            r.root_water_reserve.remove(&root);
            let p = r.chunk_world_offset
                + UVec3::new(root & 255, (root >> 8) & 255, (root >> 16) & 255);
            plain.write_chunk_atlas_region(
                p,
                UVec3::ONE,
                &[crate::builder::VOXEL_TYPE_DIRT as u8],
            )?;
        }
        self.advance_plant_hydration(plain, 0.25, settings)?;
        for (&(chunk, s, _), before) in chosen.iter().zip(old) {
            let h = self.resources.instances.chunk_flora_instances[chunk]
                .1
                .hydration[s][0]
                .1;
            anyhow::ensure!(
                h > 0.0 && h < before && h > before * 0.95,
                "dry roots must spend storage gradually"
            );
        }
        log::info!("[PLANT_HYDRATION_REVIEW] passed=true wet_recovery=gradual dry_loss=gradual grass_and_flowers=true save_restore=exact draw_reorder=stable new_lifetime=reset removal_undo=exact instance_stride=8");
        Ok(())
    }
}
