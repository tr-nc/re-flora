//! Simulation-owned plant water storage; never scheduled by draw visibility.
use super::*;
use crate::flora::hydration::{advance_receiver, available_water, settle_uptake, Settings};

fn is_soil(value: u8) -> bool {
    matches!(
        u32::from(value & 15),
        crate::builder::VOXEL_TYPE_DIRT | crate::builder::VOXEL_TYPE_SAND
    )
}

impl SurfaceBuilder {
    pub fn advance_plant_hydration(
        &mut self,
        plain: &mut crate::builder::PlainBuilder,
        elapsed_seconds: f64,
        settings: Settings,
    ) -> Result<()> {
        self.hydration_elapsed += elapsed_seconds;
        if self.hydration_elapsed < 0.25 {
            return Ok(());
        }
        let dt = (self.hydration_elapsed / 0.25).floor() * 0.25;
        self.hydration_elapsed -= dt;
        let start = std::time::Instant::now();
        let mut roots_count = 0usize;
        let mut read_bytes = 0usize;
        let mut withdrawn = 0.0;
        let mut transpired = 0.0;
        for (_, resources) in &mut self.resources.instances.chunk_flora_instances {
            if resources.hydration_roots_dirty {
                resources.hydration_roots = resources
                    .hydration
                    .iter()
                    .enumerate()
                    .flat_map(|(s, plants)| {
                        plants
                            .iter()
                            .enumerate()
                            .map(move |(i, &(root, _))| (root, s, i))
                    })
                    .collect();
                resources.hydration_roots.sort_unstable();
                resources.hydration_roots_dirty = false;
            }
            let receivers = &resources.hydration_roots;
            if receivers.is_empty() {
                continue;
            }
            let local = |root: u32| UVec3::new(root & 255, (root >> 8) & 255, (root >> 16) & 255);
            let mut min = UVec3::splat(u32::MAX);
            let mut max = UVec3::ZERO;
            // Include orphaned fractional soil balances: removal of a plant must
            // not return its previously consumed water to the soil.
            for root in receivers
                .iter()
                .map(|p| p.0)
                .chain(resources.root_water_reserve.keys().copied())
            {
                let p = local(root);
                min = min.min(p);
                max = max.max(p);
            }
            let dim = max - min + UVec3::ONE;
            let offset = resources.chunk_world_offset + min;
            let mut soil = plain.read_chunk_atlas_region(offset, dim)?;
            read_bytes += soil.len();
            let at = |root| {
                let p = local(root) - min;
                (p.x + dim.x * (p.y + dim.y * p.z)) as usize
            };
            let mut next_hydration = resources.hydration.clone();
            let mut next_reserve = resources.root_water_reserve.clone();
            next_reserve.retain(|root, _| {
                let value = soil[at(*root)];
                is_soil(value)
            });
            let mut changed = false;
            let dry_retention = (-dt / settings.normalized().transpiration_seconds).exp();
            for occupants in receivers.chunk_by(|a, b| a.0 == b.0) {
                let root = occupants[0].0;
                let index = at(root);
                let value = soil[index];
                // Dirt/sand IDs are tested against the authoritative registry below.
                let supported = is_soil(value);
                let level = if supported { (value >> 4) & 3 } else { 0 };
                let mut water =
                    available_water(level, next_reserve.get(&root).copied().unwrap_or(0.0));
                let before = water;
                if water == 0.0 {
                    for &(_, s, i) in occupants {
                        let h = &mut next_hydration[s][i].1;
                        let next = (f64::from(*h) * dry_retention) as f32;
                        transpired += f64::from(*h - next);
                        *h = next;
                    }
                    roots_count += occupants.len();
                    continue;
                }
                let mut plants: Vec<_> = occupants
                    .iter()
                    .map(|&(_, s, i)| next_hydration[s][i].1)
                    .collect();
                transpired += advance_receiver(&mut plants, &mut water, dt, settings);
                withdrawn += before - water;
                roots_count += plants.len();
                for (&(_, species, index), h) in occupants.iter().zip(plants) {
                    next_hydration[species][index].1 = h;
                }
                if supported {
                    let (new_level, debt) = settle_uptake(
                        level,
                        next_reserve.get(&root).copied().unwrap_or(0.0),
                        before - water,
                    );
                    soil[index] = (value & !0x30) | (new_level << 4);
                    changed |= soil[index] != value;
                    if debt > 0.0 {
                        next_reserve.insert(root, debt);
                    } else {
                        next_reserve.remove(&root);
                    }
                } else {
                    next_reserve.remove(&root);
                }
            }
            // Single owner writes the sampled region only after the prior GPU
            // work completes; no plant performs a competing per-instance debit.
            if changed {
                plain.write_chunk_atlas_region(offset, dim, &soil)?;
            }
            resources.hydration = next_hydration;
            resources.root_water_reserve = next_reserve;
            for species in 0..species::species_count() {
                resources.upload_hydration(species)?;
            }
        }
        log::debug!("[PLANT_HYDRATION] dt={dt:.3} plants={roots_count} read_bytes={read_bytes} uptake={withdrawn:.6} transpired={transpired:.6} cpu_ms={:.3}", start.elapsed().as_secs_f64()*1000.0);
        Ok(())
    }
}
