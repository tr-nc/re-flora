//! Adapt committed vegetation into sparse insect habitats without owning vegetation state.
use super::App;
use crate::audio::{CicadaHabitat, CicadaHabitatKey};
use crate::builder::{FloraInstanceResources, Instance};
use crate::flora::species;
use anyhow::Result;
use glam::{UVec3, Vec3};

const GRASS_SAMPLES_PER_SPECIES_PER_CHUNK: u32 = 8;

impl App {
    pub(super) fn update_summer_cicadas(&mut self, now: f64) -> Result<()> {
        if self.summer_cicadas.refresh_due(now) {
            let mut habitats = Vec::new();
            // Loading frames return before this hook. Flora edits/growth wait for their GPU jobs
            // before returning; regular rendering only reads these mapped instance buffers.
            // Sample 8 entries/species/chunk (128 entries in the default world), never a full
            // grass readback, new GPU dispatch, device wait, or inferred random terrain point.
            if self.render_flags.enable_flora && self.terrain_persistence.allows_world_updates() {
                for (_, chunk) in &self
                    .surface_builder
                    .get_resources()
                    .instances
                    .chunk_flora_instances
                {
                    for kind in [
                        species::TALL_GRASS_SPECIES_INDEX,
                        species::SHORT_GRASS_SPECIES_INDEX,
                    ] {
                        let len = chunk.species_len(kind as usize);
                        let count = len.min(GRASS_SAMPLES_PER_SPECIES_PER_CHUNK);
                        for sample in 0..count {
                            let index = sample * len / count;
                            let offset = u64::from(
                                FloraInstanceResources::species_offset(kind as usize) + index,
                            ) * std::mem::size_of::<Instance>() as u64;
                            let bytes = chunk
                                .resource
                                .instances_buf
                                .read_back_range(offset, std::mem::size_of::<Instance>() as u64)?;
                            let instance: Instance = bytemuck::pod_read_unaligned(&bytes);
                            if let Some(position) = grass_habitat_position(
                                instance.packed_local_pos,
                                chunk.chunk_world_offset,
                            ) {
                                habitats.push(CicadaHabitat {
                                    key: CicadaHabitatKey::Grass(position.to_array()),
                                    // Place within the grass, one voxel above its real root.
                                    position: (position.as_vec3() + Vec3::Y) / 256.0,
                                });
                            }
                        }
                    }
                }
                if self.debug_settings.tree.render_leaves {
                    habitats.extend(self.trees.cicada_canopy_habitats());
                }
            }
            self.summer_cicadas
                .refresh(habitats, self.tracer.camera_position(), now);
        }
        self.summer_cicadas.update(now)?;
        self.advance_cicada_smoke(now)
    }
}

fn grass_habitat_position(packed: u32, chunk_offset: UVec3) -> Option<UVec3> {
    // Zero growth has no visible blade. Growth must never enter the decoded coordinate.
    (packed >> 24 > 0)
        .then(|| chunk_offset + UVec3::new(packed & 255, (packed >> 8) & 255, (packed >> 16) & 255))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grass_habitats_use_live_roots_and_ignore_growth_bits() {
        let root = 12 | (34 << 8) | (56 << 16);
        assert_eq!(grass_habitat_position(root, UVec3::ZERO), None);
        for growth in [1, 17, 255] {
            assert_eq!(
                grass_habitat_position(root | (growth << 24), UVec3::new(256, 0, 256)),
                Some(UVec3::new(268, 34, 312))
            );
        }
    }
}

// Opt-in real-app acceptance. Uses the ordinary Save/Load and tree-removal paths, with actual
// playing sources. No test world, fake engine, or accelerated audio clock.
pub(super) struct CicadaSmoke {
    phase: u8,
    next_action: f64,
    removed_tree: Option<u32>,
}

impl CicadaSmoke {
    pub(super) fn from_environment() -> Option<Self> {
        (std::env::var("RE_FLORA_CICADA_SMOKE").as_deref() == Ok("1")).then_some(Self {
            phase: 0,
            next_action: 0.0,
            removed_tree: None,
        })
    }
}

impl App {
    fn advance_cicada_smoke(&mut self, now: f64) -> Result<()> {
        let Some(mut smoke) = self.cicada_smoke.take() else {
            return Ok(());
        };
        if now < smoke.next_action || !self.terrain_persistence.can_start_operation() {
            self.cicada_smoke = Some(smoke);
            return Ok(());
        }
        match smoke.phase {
            0 => {
                let path = "target/summer-evidence/cicada-scene.rflterrain";
                std::fs::create_dir_all("target/summer-evidence")?;
                self.perform_startup_terrain_save(std::path::Path::new(path))?;
                *self.terrain_persistence.snapshot_path_mut() = path.to_owned();
                log::info!("[AUDIO][CICADAS][SMOKE] saved live garden");
                smoke.next_action = now + 8.0;
            }
            1 | 2 => {
                let before = self.summer_cicadas.active_habitats().count();
                if before == 0 {
                    self.cicada_smoke = Some(smoke);
                    return Ok(());
                }
                self.perform_runtime_terrain_load();
                anyhow::ensure!(
                    self.terrain_persistence.awaits_dependents(),
                    "cicada smoke world replacement did not publish"
                );
                anyhow::ensure!(
                    self.summer_cicadas.active_habitats().count() == 0,
                    "cicadas survived world replacement"
                );
                log::info!(
                    "[AUDIO][CICADAS][SMOKE] replacement={} active_before={before} active_after=0",
                    smoke.phase
                );
                smoke.next_action = now + 10.0;
            }
            3 => {
                let tree = self
                    .summer_cicadas
                    .active_habitats()
                    .find_map(|key| match key {
                        CicadaHabitatKey::Canopy(tree, _, _) => Some(tree),
                        _ => None,
                    });
                let Some(tree) = tree else {
                    self.cicada_smoke = Some(smoke);
                    return Ok(());
                };
                self.remove_tree(tree)?;
                smoke.removed_tree = Some(tree);
                smoke.next_action = now + 1.0;
                log::info!("[AUDIO][CICADAS][SMOKE] removed sounding canonical tree={tree}");
            }
            _ => {
                anyhow::ensure!(!self.summer_cicadas.active_habitats().any(|key|
                    matches!(key, CicadaHabitatKey::Canopy(tree, _, _) if Some(tree) == smoke.removed_tree)),
                    "removed tree retained a cicada source");
                log::info!("[AUDIO][CICADAS][SMOKE] passed repeated_world_replacement=true sounding_tree_removal=true");
                return Ok(());
            }
        }
        smoke.phase += 1;
        self.cicada_smoke = Some(smoke);
        Ok(())
    }
}
