//! Adapt committed vegetation into the shared ecology scheduler; consumers own their animals.
use super::App;
use crate::ecology::{Animal, Ecology, Habitat, RegionKey, ATTEMPTS, RANGE};
use crate::particles::ButterflySpawnSource;
use anyhow::Result;
use std::time::Instant;

pub(super) struct EcologyRuntime {
    scheduler: Ecology,
    next_refresh: f64,
    next_report: f64,
    samples_us: Vec<f64>,
    roots_read: u64,
    candidates: u64,
    births: [[u64; 3]; 2],
    last_supply: [u64; 3],
}
impl EcologyRuntime {
    pub(super) fn new() -> Self {
        Self {
            scheduler: Ecology::new(9173),
            next_refresh: 0.,
            next_report: 0.,
            samples_us: Vec::with_capacity(4096),
            roots_read: 0,
            candidates: 0,
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
            RegionKey::Surface(chunk, species) => {
                self.ecology.roots_read += 1;
                self.surface_builder
                    .sample_ecology_root(chunk, species, slot)
            }
            RegionKey::Canopy(tree) => Ok(self.trees.sample_ecology_leaf(tree, slot)),
        }
    }
    pub(super) fn update_ambient_ecology(&mut self, now: f64) -> Result<()> {
        let started = Instant::now();
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
            if supply != self.ecology.last_supply {
                log::info!(
                    "[ECOLOGY][SUPPLY] grass={} plants={} leaves={} nearby_groups={}",
                    supply[0],
                    supply[1],
                    supply[2],
                    self.ecology.scheduler.region_count()
                );
                self.ecology.last_supply = supply;
            }
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
            self.butterfly_emitter_desc =
                Self::butterfly_desc_from_gui_adjustables(&self.debug_settings.adjustables);
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
                self.ecology.candidates += 1;
                let Some(site) = self.sample_ecology_habitat(region.key, slot)? else {
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
                    log::info!("[ECOLOGY][BIRTH] time={now:.3} animal={animal:?} kind={} region={:?} slot={} position={:?}", site.kind, site.region, site.slot, site.position);
                    break;
                }
            }
        }
        if self.ecology.samples_us.len() < 4096 {
            self.ecology
                .samples_us
                .push(started.elapsed().as_secs_f64() * 1e6);
        }
        if now >= self.ecology.next_report {
            let samples = &mut self.ecology.samples_us;
            samples.sort_by(f64::total_cmp);
            let n = samples.len();
            log::info!("[ECOLOGY][PERF] samples={n} cpu_us_p50={:.3} cpu_us_p95={:.3} cpu_us_p99={:.3} cpu_us_max={:.3} groups={} candidate_attempts={} root_bytes={} births={:?}", samples[n/2], samples[n*95/100], samples[n*99/100], samples[n-1], self.ecology.scheduler.region_count(), self.ecology.candidates, self.ecology.roots_read*8, self.ecology.births);
            samples.clear();
            self.ecology.roots_read = 0;
            self.ecology.candidates = 0;
            self.ecology.next_report = now + 5.;
        }
        self.advance_cicada_smoke(now)
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
                let path = "target/summer-evidence/ecology-smoke-scene.rflterrain";
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
                        Habitat {
                            region: RegionKey::Canopy(tree),
                            ..
                        } => Some(tree),
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
                    matches!(key.region, RegionKey::Canopy(tree) if Some(tree) == smoke.removed_tree)),
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
