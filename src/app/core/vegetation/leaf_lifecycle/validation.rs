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
    poses: Vec<crate::particles::AttachedLeafRelease>,
}

impl Validation {
    pub fn from_env() -> Option<Self> {
        std::env::var_os("RE_FLORA_REAL_LEAVES_VALIDATE").map(|_| Self {
            stage: 0,
            frames: 0,
            count: 0,
            saw_growth: false,
            sentinel: None,
            poses: Vec::new(),
        })
    }

    pub fn wind(&self) -> WindFieldFrame {
        if matches!(self.stage, 0 | 1 | 3) {
            WindFieldFrame::uniform(Vec2::X * 10.0)
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

    // Read the real published GPU pose without committing a detachment. These
    // bounded probes exercise the same pose helper used by attached model draws.
    fn probe(&self, app: &App) -> Vec<crate::particles::AttachedLeafRelease> {
        let mut requests = Vec::new();
        for (&id, record) in &app.trees.records {
            let initial;
            let canopy = if let Some(state) = &record.leaf_lifecycle {
                &state.canopy
            } else {
                initial = LeafCanopy::new(
                    id,
                    record.canopy_acoustic_descriptor.generation(),
                    &record.leaf_render_positions,
                    0.0,
                );
                &initial
            };
            requests.extend(
                canopy
                    .plan(
                        app.trees.leaf_lifecycle.time,
                        app.trees.leaf_lifecycle.settings,
                        &WindFieldFrame::uniform(Vec2::X * 100.0),
                    )
                    .into_iter()
                    .take(64)
                    .map(|e| {
                        let local = record.leaf_render_local_positions[e.id.socket as usize];
                        (e, local)
                    }),
            );
        }
        requests.truncate(64);
        let a = &app.debug_settings.adjustables;
        let rgb = |c: egui::Color32| Vec3::new(c.r() as f32, c.g() as f32, c.b() as f32) / 255.;
        let colors = crate::tracer::solid_flora_height_color_tables(
            rgb(a.leaves_bottom_color.value),
            rgb(a.leaves_tip_color.value),
        );
        let poses: Vec<_> = app
            .tracer
            .gather_leaf_handoffs(&requests, colors)
            .unwrap()
            .into_iter()
            .map(|(_, pose)| pose)
            .collect();
        assert!(
            !poses.is_empty(),
            "pose probes need published attached leaves"
        );
        poses
    }

    fn assert_released_pose(&self, app: &App) {
        for pose in &self.poses {
            assert!(
                app.particle_snapshots
                    .iter()
                    .any(|p| p.leaf_shape_seed == Some(pose.seed)
                        && p.position_ws.distance(pose.position) < 1e-5
                        && (p.size - pose.size).abs() < 1e-6
                        && (p.color - pose.color).length() < 1e-5
                        && p.leaf_geometry
                            .is_some_and(|q| q.dot(pose.geometry_rotation).abs() > 1.0 - 1e-5)),
                "release changed model shape/size/color/center/full orientation: {:?}; candidates={:?}",
                pose.id,
                app.particle_snapshots.iter().filter(|p| p.leaf_shape_seed == Some(pose.seed))
                    .map(|p| (p.position_ws.distance(pose.position), p.size - pose.size,
                        (p.color - pose.color).length(), p.leaf_geometry.map(|q| q.dot(pose.geometry_rotation))))
                    .collect::<Vec<_>>()
            );
        }
    }

    fn step(&mut self, app: &mut App) {
        self.frames += 1;
        let source_particles = app.particle_system.source_leaf_count();
        match self.stage {
            0 => {
                app.debug_settings.adjustables.real_leaf_lifecycle.value = false;
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
                if self.frames == 1 {
                    let a = &mut app.debug_settings.adjustables;
                    a.attached_leaf_rotation.value = false;
                    // Reproducible excitation under the shared injected test wind.
                    a.leaf_flutter_amplitude_low.value = 0.8;
                    a.leaf_flutter_amplitude_high.value = 0.8;
                    return;
                }
                let current = self.probe(app);
                if self.frames == 2 {
                    self.poses = current;
                    return;
                }
                let changed = current
                    .iter()
                    .filter(|pose| {
                        let original = self.poses.iter().find(|p| p.id == pose.id).unwrap();
                        assert_eq!(
                            pose.seed, original.seed,
                            "rotation toggle rerolled the model"
                        );
                        pose.geometry_rotation.dot(original.geometry_rotation).abs() < 1.0 - 1e-5
                    })
                    .count();
                if self.frames == 4 {
                    assert!(
                        changed > 0,
                        "B must rotate geometry, not just its lighting normal"
                    );
                    app.debug_settings.adjustables.attached_leaf_rotation.value = false;
                    return;
                }
                assert_eq!(
                    changed, 0,
                    "A must retain the exact initial orientation over time and after B"
                );
                if self.frames == 3 {
                    app.debug_settings.adjustables.attached_leaf_rotation.value = true;
                    return;
                }
                self.poses = current;
                log::info!("[LEAF_LIFECYCLE][VALIDATE] rotation_ab=passed initial_seed=stable geometry_rotation=true");
                self.count = app
                    .trees
                    .records
                    .values()
                    .map(|r| r.leaf_render_positions.len())
                    .sum();
                assert!(self.count > 0, "leaf validation needs the startup tree");
                let a = &mut app.debug_settings.adjustables;
                a.real_leaf_lifecycle.value = true;
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
                self.assert_released_pose(app);
                log::info!("[LEAF_LIFECYCLE][VALIDATE] first_gust=passed transferred={} attached=0 gpu_occupancy=0 ecology_supply=0", self.count);
                self.stage = 2;
                self.frames = 0;
            }
            2 => {
                assert_eq!(
                    source_particles, self.count,
                    "rotation must not reset falling leaves"
                );
                assert_eq!(
                    app.trees.leaf_lifecycle.detached, self.count,
                    "rotation must not reset the lifecycle"
                );
                if self.frames == 1 {
                    app.debug_settings.adjustables.attached_leaf_rotation.value = true;
                }
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
                    self.poses = self.probe(app);
                    self.stage = 3;
                } else {
                    assert!(self.frames < 100, "regeneration failed to complete");
                }
            }
            3 => {
                assert_eq!(app.trees.leaf_lifecycle.detached, self.count * 2);
                assert_eq!(source_particles, self.count * 2);
                self.assert_released_pose(app);
                assert!(app.particle_system.capacity() >= source_particles);
                self.gpu_growth_is(app, 0.0);
                log::info!("[LEAF_LIFECYCLE][VALIDATE] second_generation=passed source_particles={source_particles} capacity={}", app.particle_system.capacity());
                app.debug_settings.adjustables.real_leaf_lifecycle.value = false;
                self.stage = 4;
            }
            4 => {
                assert_eq!(source_particles, 0);
                assert!(
                    app.particle_system
                        .position(self.sentinel.unwrap())
                        .is_some(),
                    "A/B reset removed an unrelated particle"
                );
                assert!(app
                    .trees
                    .records
                    .values()
                    .all(|r| r.leaf_lifecycle.is_none()));
                self.gpu_growth_is(app, 1.0);
                app.debug_settings.adjustables.real_leaf_lifecycle.value = true;
                self.stage = 5;
            }
            5 => {
                assert_eq!(app.trees.leaf_lifecycle.detached, 0);
                assert!(app.trees.records.values().all(|r| r
                    .leaf_lifecycle
                    .as_ref()
                    .unwrap()
                    .growth
                    .iter()
                    .all(|&g| g == 1.0)));
                self.gpu_growth_is(app, 1.0);
                app.debug_settings.adjustables.real_leaf_lifecycle.value = false;
                app.particle_system.despawn(self.sentinel.take().unwrap());
                app.debug_settings.adjustables.attached_leaf_rotation.value = false;
                self.stage = 6;
                log::info!("[LEAF_LIFECYCLE][VALIDATE] PASS all_leaf_transfer=true generations=2 regrowth=true gpu_publication=true ecology=true reversible_ab=true unrelated_particles=preserved rotation_ab=true release_model_pose=true");
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
