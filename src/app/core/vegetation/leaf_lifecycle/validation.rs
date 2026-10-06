//! Opt-in hidden app contract replay, never a normal unit test or GUI control.
//! Injects a deterministic shared-field snapshot at the lifecycle seam; does not
//! claim to benchmark natural wind or visually approve the candidate.
use super::*;
use crate::particles::{MotionMode, ParticleHandle, ParticleRenderKind, ParticleSpawn};
use crate::wind_field::WindFieldFrame;

#[derive(Clone)]
pub(super) struct Validation {
    stage: u32,
    frames: u32,
    count: usize,
    saw_growth: bool,
    sentinel: Option<ParticleHandle>,
}

impl Validation {
    pub fn from_env() -> Option<Self> {
        std::env::var_os("RE_FLORA_REAL_LEAVES_VALIDATE").map(|_| Self {
            stage: 0,
            frames: 0,
            count: 0,
            saw_growth: false,
            sentinel: None,
        })
    }

    pub fn wind(&self) -> WindFieldFrame {
        if self.stage == 1 || self.stage == 3 {
            // Deliberately catastrophic load validates complete handoff, not
            // the amount of shedding expected from an ordinary manual gust.
            WindFieldFrame::uniform(Vec2::X * 100.0)
        } else {
            WindFieldFrame::default()
        }
    }

    fn gpu_growth_is(&self, app: &App, expected: f32) {
        for tree in app
            .surface_builder
            .resources
            .instances
            .leaves_instances
            .values()
        {
            if tree.resources.instances_len == 0 {
                continue;
            }
            let bytes = tree.resources.leaf_state.buffer().read_back().unwrap();
            let values: &[f32] = bytemuck::cast_slice(&bytes);
            assert!(
                values.iter().all(|&v| v == expected),
                "published occupancy differs from canonical leaf state"
            );
        }
    }

    fn step(&mut self, app: &mut App) {
        self.frames += 1;
        let source_particles = app
            .particle_snapshots
            .iter()
            .filter(|p| p.leaf_geometry.is_some())
            .count();
        match self.stage {
            0 => {
                if self.sentinel.is_none() {
                    self.sentinel = app.particle_system.spawn(ParticleSpawn {
                        position: Vec3::new(1.0, 1.0, 1.0),
                        lifetime: 100.0,
                        motion_mode: MotionMode::Free,
                        render_kind: ParticleRenderKind::WaterDroplet,
                        size: 0.001,
                        wind_factor: 0.0,
                        gravity_factor: 0.0,
                        ..ParticleSpawn::default()
                    });
                }
                if self.frames < 3 {
                    return;
                }
                self.count = app
                    .trees
                    .records
                    .values()
                    .map(|r| r.leaf_render_positions.len())
                    .sum();
                assert!(self.count > 0, "leaf validation needs the startup tree");
                let a = &mut app.debug_settings.adjustables;
                a.leaf_connection_strength.value = 1.0;
                a.leaf_regrowth_delay.value = 0.1;
                a.leaf_regrowth_duration.value = 1.0;
                self.stage = 1;
            }
            1 => {
                assert_eq!(
                    app.trees.leaf_lifecycle.detached, self.count,
                    "strong wind must transfer the entire canopy in its first eligible frame"
                );
                assert_eq!(
                    source_particles, self.count,
                    "each missing leaf must have one falling counterpart"
                );
                assert!(app.trees.records.values().all(|r| r
                    .leaf_lifecycle
                    .as_ref()
                    .unwrap()
                    .growth
                    .iter()
                    .all(|&g| g == 0.0)));
                assert!(app.trees.ecology_regions().iter().all(|r| r.count == 0));
                self.gpu_growth_is(app, 0.0);
                log::info!("[LEAF_LIFECYCLE][VALIDATE] first_gust=passed transferred={} attached=0 gpu_occupancy=0 ecology_supply=0", self.count);
                self.stage = 2;
                self.frames = 0;
            }
            2 => {
                let all = app
                    .trees
                    .records
                    .values()
                    .flat_map(|r| &r.leaf_lifecycle.as_ref().unwrap().growth)
                    .collect::<Vec<_>>();
                self.saw_growth |= all.iter().any(|&&g| g > 0.0 && g < 1.0);
                if all.iter().all(|&&g| g == 1.0) {
                    assert!(self.saw_growth, "regeneration must grow, not pop");
                    assert_eq!(
                        app.trees
                            .ecology_regions()
                            .iter()
                            .map(|r| r.count as usize)
                            .sum::<usize>(),
                        self.count
                    );
                    self.gpu_growth_is(app, 1.0);
                    log::info!("[LEAF_LIFECYCLE][VALIDATE] regrowth=passed attached={} source_particles={source_particles}", self.count);
                    self.stage = 3;
                } else {
                    assert!(self.frames < 100, "regeneration failed to complete");
                }
            }
            3 => {
                assert_eq!(app.trees.leaf_lifecycle.detached, self.count * 2);
                assert!(source_particles >= self.count);
                assert!(app.particle_system.capacity() >= source_particles);
                self.gpu_growth_is(app, 0.0);
                log::info!("[LEAF_LIFECYCLE][VALIDATE] second_generation=passed source_particles={source_particles} capacity={}", app.particle_system.capacity());
                self.stage = 4;
            }
            4 | 5 => {
                assert_eq!(app.trees.leaf_lifecycle.detached, self.count * 2);
                assert!(
                    source_particles >= self.count,
                    "appearance switch cleared falling leaves"
                );
                assert!(app
                    .trees
                    .records
                    .values()
                    .all(|r| r.leaf_lifecycle.is_some()));
                assert!(
                    app.particle_system
                        .position(self.sentinel.unwrap())
                        .is_some(),
                    "appearance switch removed an unrelated particle"
                );
                if self.stage == 4 {
                    self.stage = 5;
                } else {
                    app.particle_system.despawn(self.sentinel.take().unwrap());
                    self.stage = 6;
                    log::info!("[LEAF_LIFECYCLE][VALIDATE] PASS all_leaf_transfer=true generations=2 regrowth=true gpu_publication=true ecology=true voxel_particles=true unrelated_particles=preserved");
                }
            }
            _ => {}
        }
    }
}

pub(super) fn drive(app: &mut App) {
    if let Some(mut validation) = app.trees.leaf_lifecycle.validation.take() {
        validation.step(app);
        app.trees.leaf_lifecycle.validation = Some(validation);
    }
}
