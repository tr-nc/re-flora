//! Explicit opt-in real-app fixtures. All edits use normal vegetation and Save/Load paths.
use super::*;
use crate::app::world_edits::{TerrainBrushEdit, TreeAddOptions, TreePlacement};
use glam::{Vec2, Vec3};

pub(in crate::app::core) struct CicadaSmoke {
    phase: u8,
    next_action: f64,
    removed_host: Option<Habitat>,
    empty_births: [[u64; 3]; 2],
    scene: Option<String>,
    lifecycle: bool,
}
impl CicadaSmoke {
    pub(in crate::app::core) fn from_environment() -> Option<Self> {
        let scene = std::env::var("RE_FLORA_ECOLOGY_SCENE").ok();
        let lifecycle = std::env::var("RE_FLORA_ECOLOGY_SMOKE").as_deref() == Ok("1")
            || std::env::var("RE_FLORA_CICADA_SMOKE").as_deref() == Ok("1");
        (scene.is_some() || lifecycle).then_some(Self {
            phase: 0,
            next_action: 0.,
            removed_host: None,
            empty_births: [[0; 3]; 2],
            scene,
            lifecycle,
        })
    }
}
impl App {
    fn clear_ecology_fixture_vegetation(&mut self) -> Result<()> {
        self.apply_surface_flora_removal(TerrainBrushEdit {
            start: Vec3::ONE,
            end: Vec3::ONE,
            radius: 4.,
        })?;
        let trees: Vec<_> = self
            .trees
            .ecology_regions()
            .into_iter()
            .filter_map(|r| match r.key {
                RegionKey::Canopy(id) => Some(id),
                _ => None,
            })
            .collect();
        for id in trees {
            self.remove_tree(id)?;
        }
        self.ecology.next_refresh = 0.;
        Ok(())
    }
    fn seed_ecology_fixture(&mut self, scene: &str) -> Result<()> {
        anyhow::ensure!(
            ["grass", "plants", "canopy", "dense", "empty"].contains(&scene),
            "unknown ecology scene: {scene}"
        );
        self.clear_ecology_fixture_vegetation()?;
        let previous = self.player_tools.flora_paint_selection_index;
        let result = (|| -> Result<()> {
            if matches!(scene, "grass" | "dense" | "plants") {
                let count = if scene == "plants" { 8 } else { 2 };
                for i in 0..count {
                    self.player_tools.flora_paint_selection_index =
                        if scene == "plants" { 1 + i % 2 } else { 0 };
                    let xz = if scene == "dense" {
                        Vec2::new(0.6 + i as f32 * 0.7, 0.65)
                    } else {
                        Vec2::new(0.35 + (i % 4) as f32 * 0.2, 0.45 + (i / 4) as f32 * 0.25)
                    };
                    let center = Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y);
                    self.apply_surface_flora_regeneration(
                        TerrainBrushEdit {
                            start: center,
                            end: center,
                            radius: if scene == "dense" { 0.65 } else { 0.1 },
                        },
                        i as u32 + 1,
                        true,
                    )?;
                }
            }
            if scene == "canopy" {
                let xz = Vec2::new(0.65, 0.65);
                self.add_tree(
                    crate::tree_gen::TreeDesc::default(),
                    TreePlacement::World(Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y)),
                    TreeAddOptions {
                        assign_new_id: true,
                    },
                )?;
            }
            Ok(())
        })();
        self.player_tools.flora_paint_selection_index = previous;
        result?;
        // Fixtures must not depend on a user-owned camera snapshot. The normal
        // overview camera can be outside the bounded ecology listener range.
        let focus_xz = Vec2::new(0.65, 0.65);
        let focus = Vec3::new(
            focus_xz.x,
            self.query_terrain_height_cpu(focus_xz) + 0.12,
            focus_xz.y,
        );
        self.tracer
            .set_camera_pose_looking_at(focus + Vec3::new(0.0, 0.25, 0.6), focus);
        self.ecology.clear();
        log::info!("[ECOLOGY][SCENE] seeded={scene} normal_vegetation_edits=true");
        Ok(())
    }
    pub(super) fn advance_cicada_smoke(&mut self, now: f64) -> Result<()> {
        let Some(mut smoke) = self.cicada_smoke.take() else {
            return Ok(());
        };
        if now < smoke.next_action || !self.terrain_persistence.can_start_operation() {
            self.cicada_smoke = Some(smoke);
            return Ok(());
        }
        match smoke.phase {
            0 => {
                if let Some(scene) = &smoke.scene {
                    self.seed_ecology_fixture(scene)?;
                }
                let path = format!(
                    "target/summer-evidence/ecology-{}.rflterrain",
                    smoke.scene.as_deref().unwrap_or("mixed")
                );
                std::fs::create_dir_all("target/summer-evidence")?;
                self.perform_startup_terrain_save(std::path::Path::new(&path))?;
                *self.terrain_persistence.snapshot_path_mut() = path;
                log::info!(
                    "[ECOLOGY][SMOKE] saved live garden lifecycle={}",
                    smoke.lifecycle
                );
                if !smoke.lifecycle {
                    return Ok(());
                }
                smoke.next_action = now + 8.;
            }
            1 | 2 => {
                let before = self.summer_cicadas.active_habitats().count();
                if before == 0 {
                    self.cicada_smoke = Some(smoke);
                    return Ok(());
                }
                let mut old_handles = Vec::new();
                for emitter in &mut self.butterfly_emitters {
                    emitter.collect_butterfly_states(
                        &self.particle_system,
                        &mut old_handles,
                        &mut Vec::new(),
                        &mut Vec::new(),
                        &mut Vec::new(),
                    );
                }
                self.perform_runtime_terrain_load();
                anyhow::ensure!(
                    old_handles
                        .iter()
                        .all(|h| !self.particle_system.is_alive_handle(*h)),
                    "old-world butterflies survived replacement"
                );
                log::info!(
                    "[ECOLOGY][SMOKE] old_butterflies={} retained=0",
                    old_handles.len()
                );
                anyhow::ensure!(
                    self.terrain_persistence.awaits_dependents(),
                    "ecology world replacement did not publish"
                );
                anyhow::ensure!(
                    self.summer_cicadas.active_habitats().count() == 0,
                    "cicadas survived world replacement"
                );
                anyhow::ensure!(
                    self.ecology.scheduler.region_count() == 0,
                    "old habitat index survived world replacement"
                );
                log::info!("[ECOLOGY][SMOKE] replacement={} active_before={before} active_after=0 groups=0", smoke.phase);
                smoke.next_action = now + 10.;
            }
            3 => {
                let Some(host) = self.summer_cicadas.active_habitats().next() else {
                    self.cicada_smoke = Some(smoke);
                    return Ok(());
                };
                match host.region {
                    RegionKey::Canopy(tree) => self.remove_tree(tree)?,
                    RegionKey::Surface(..) => {
                        self.apply_surface_flora_removal(TerrainBrushEdit {
                            start: host.position,
                            end: host.position,
                            radius: 0.08,
                        })?
                    }
                }
                smoke.removed_host = Some(host);
                smoke.next_action = now + 0.6;
                log::info!("[ECOLOGY][SMOKE] removed sounding host={host:?}");
            }
            4 => {
                anyhow::ensure!(
                    !self
                        .summer_cicadas
                        .active_habitats()
                        .any(|h| Some(h) == smoke.removed_host),
                    "removed host retained audio"
                );
                let host = smoke.removed_host.unwrap();
                anyhow::ensure!(
                    self.sample_ecology_habitat(host.region, host.slot)? != Some(host),
                    "host removal did not change real vegetation"
                );
                log::info!("[ECOLOGY][SMOKE] sounding_host_removal=passed");
                self.clear_ecology_fixture_vegetation()?;
                smoke.next_action = now + 1.;
            }
            5 => {
                anyhow::ensure!(
                    self.ecology.last_supply == [0; 3]
                        && self.ecology.scheduler.region_count() == 0
                        && self.summer_cicadas.active_habitats().count() == 0,
                    "empty garden retained supply or audio"
                );
                smoke.empty_births = self.ecology.births;
                smoke.next_action = now + 20.;
            }
            _ => {
                anyhow::ensure!(
                    smoke.empty_births == self.ecology.births,
                    "empty garden generated animals"
                );
                log::info!("[ECOLOGY][SMOKE] passed repeated_world_replacement=true sounding_host_removal=true empty_garden_no_births_seconds=20");
                return Ok(());
            }
        }
        smoke.phase += 1;
        self.cicada_smoke = Some(smoke);
        Ok(())
    }
}
