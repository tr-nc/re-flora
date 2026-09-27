//! Adapt committed vegetation into the shared ecology scheduler; consumers own their animals.
use super::App;
use crate::ecology::{Animal, Ecology, Habitat, RegionKey, ATTEMPTS, RANGE};
use crate::particles::ButterflySpawnSource;
use anyhow::Result;

pub(super) struct EcologyRuntime {
    scheduler: Ecology,
    next_refresh: f64,
    births: [[u64; 3]; 2],
    last_supply: [u64; 3],
}
impl EcologyRuntime {
    pub(super) fn new() -> Self {
        Self {
            scheduler: Ecology::new(9173),
            next_refresh: 0.,
            births: [[0; 3]; 2],
            last_supply: [0; 3],
        }
    }
    pub(super) fn clear(&mut self) {
        self.scheduler.clear();
        self.next_refresh = 0.;
        log::info!("[ECOLOGY][CLEAR] groups=0 clocks=disarmed");
    }
}
impl App {
    fn sample_ecology_habitat(&mut self, key: RegionKey, slot: u32) -> Result<Option<Habitat>> {
        match key {
            RegionKey::Surface(chunk, species) => self
                .surface_builder
                .sample_ecology_root(chunk, species, slot),
            RegionKey::Canopy(tree) => Ok(self.trees.sample_ecology_leaf(tree, slot)),
        }
    }
    pub(super) fn update_ambient_ecology(&mut self, now: f64) -> Result<()> {
        let world_ready = self.terrain_persistence.allows_world_updates();
        let listener = self.tracer.camera_position();
        // Metadata publication never expands grass or leaf arrays. A new candidate is fetched
        // only when the shared clock offers an opportunity; active audio validates <=3 hosts.
        if now >= self.ecology.next_refresh || !world_ready {
            let mut regions = Vec::new();
            if world_ready {
                if self.render_flags.enable_flora {
                    regions.extend(self.surface_builder.ecology_regions());
                }
                if self.debug_settings.tree.render_leaves {
                    regions.extend(self.trees.ecology_regions());
                }
            }
            let mut supply = [0; 3];
            for region in &regions {
                supply[region.kind] += u64::from(region.count);
            }
            self.ecology.scheduler.publish(regions, listener);
            self.ecology.last_supply = supply;
            let active: Vec<_> = self.summer_cicadas.active_habitats().collect();
            let mut valid = Vec::new();
            for site in active {
                let visible = world_ready
                    && match site.region {
                        RegionKey::Surface(..) => self.render_flags.enable_flora,
                        RegionKey::Canopy(_) => self.debug_settings.tree.render_leaves,
                    };
                if visible && site.position.distance(listener) <= RANGE {
                    match self.sample_ecology_habitat(site.region, site.slot) {
                        Ok(Some(current)) if current == site => valid.push(site),
                        Ok(_) => {}
                        Err(error) => log::warn!("[ECOLOGY] host validation failed: {error:#}"),
                    }
                }
            }
            self.summer_cicadas.update(now, Some(&valid))?;
            self.ecology.next_refresh = now + 0.5;
        } else {
            self.summer_cicadas.update(now, None)?;
        }

        let butterfly_enabled = world_ready
            && self.render_flags.enable_particles
            && self.launch_owners.allows_ambient_particle_emitters();
        if butterfly_enabled {
            self.butterfly_emitter_desc = Self::butterfly_desc_from_gui_adjustables(
                &self.debug_settings.adjustables,
                self.debug_settings.butterfly_flight.variant,
                self.debug_settings.butterfly_flight.tuning,
            );
            self.ensure_butterfly_emitter();
            for emitter in &mut self.butterfly_emitters {
                emitter.apply_desc(&self.butterfly_emitter_desc);
            }
        }
        for animal in [Animal::Butterfly, Animal::Cicada] {
            let (capacity, scale) = match animal {
                Animal::Butterfly => (
                    butterfly_enabled
                        && self
                            .butterfly_emitters
                            .first_mut()
                            .is_some_and(|e| e.has_capacity(&self.particle_system)),
                    f64::from(self.butterfly_emitter_desc.spawn_rate_per_source) / 0.00002,
                ),
                Animal::Cicada => (world_ready && self.summer_cicadas.has_capacity(), 1.),
            };
            if !self.ecology.scheduler.due(animal, now, capacity, scale) {
                continue;
            }
            for _ in 0..ATTEMPTS {
                let Some((region, slot)) = self.ecology.scheduler.candidate(animal, now) else {
                    continue;
                };
                let candidate = match region.key {
                    RegionKey::Canopy(tree) => self.trees.sample_ecology_leaf_candidate(tree, slot),
                    _ => self.sample_ecology_habitat(region.key, slot)?,
                };
                let Some(site) = candidate else {
                    continue;
                };
                if site.position.distance(listener) > RANGE {
                    continue;
                }
                let accepted = match animal {
                    Animal::Butterfly => {
                        let source = if site.kind == 2 {
                            ButterflySpawnSource::tree_leaf(site.position)
                        } else {
                            ButterflySpawnSource::ground_flora(site.position)
                        };
                        self.butterfly_emitters[0]
                            .spawn_at(&mut self.particle_system, source)
                            .is_some()
                    }
                    Animal::Cicada => self.summer_cicadas.start(site, now)?,
                };
                if accepted {
                    self.ecology.births[animal as usize][site.kind] += 1;
                    self.ecology.scheduler.accepted(animal, site.region, now);
                    break;
                }
            }
        }
        self.advance_cicada_smoke(now)
    }
}
mod smoke;
pub(super) use smoke::CicadaSmoke;
