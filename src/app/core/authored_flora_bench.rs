use super::App;
use crate::app::world_edits::TerrainBrushEdit;
use crate::flora::species;
use glam::{Vec2, Vec3};
use std::time::Instant;

#[derive(Debug)]
pub(super) struct AuthoredFloraBench {
    samples: u32,
    next_sample: u32,
    results: Vec<f32>,
    response_check: Option<ResponseCheck>,
    response_perf: Option<ResponsePerf>,
}

impl AuthoredFloraBench {
    pub(super) fn new(samples: u32) -> Self {
        Self {
            samples: samples.max(1),
            next_sample: 0,
            results: Vec::new(),
            response_check: std::env::var_os("RE_FLORA_VEGETATION_RESPONSE_VALIDATE")
                .map(|_| ResponseCheck::default()),
            response_perf: std::env::var_os("RE_FLORA_VEGETATION_RESPONSE_BENCH")
                .map(|_| ResponsePerf::default()),
        }
    }

    pub(super) fn run_next(app: &mut App) -> bool {
        let Some(mut bench) = app.launch_owners.authored_flora_bench.take() else {
            return false;
        };

        let done = bench.run_sample(app);
        if !done {
            app.launch_owners.authored_flora_bench = Some(bench);
        }
        done
    }

    fn run_sample(&mut self, app: &mut App) -> bool {
        if let Some(perf) = &mut self.response_perf {
            return perf.advance(app).expect("vegetation performance fixture");
        }
        if let Some(check) = &mut self.response_check {
            return check
                .advance(app)
                .unwrap_or_else(|err| panic!("[VEGETATION_RESPONSE][SCENE] {err:#}"));
        }
        if self.next_sample >= self.samples {
            self.log_summary();
            return true;
        }

        self.next_sample += 1;
        let sample = self.next_sample;
        app.player_tools.flora_paint_selection_index = 1;
        let center_xz = authored_flora_bench_center(sample - 1);
        let center_y = app.query_terrain_height_cpu(center_xz);
        let center = Vec3::new(center_xz.x, center_y, center_xz.y);
        let edit = TerrainBrushEdit {
            start: center,
            end: center,
            radius: super::TERRAIN_EDIT_DEFAULT_RADIUS,
        };

        let start = Instant::now();
        match app.apply_surface_flora_regeneration(edit, sample, true) {
            Ok(()) => {
                let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
                self.results.push(elapsed_ms);
                log::info!(
                    "[PERF][AUTHORED_FLORA_BENCH] sample {}/{} paint_step {:.3}ms species {} center {:.3},{:.3},{:.3}",
                    sample,
                    self.samples,
                    elapsed_ms,
                    species::flora_paint_selection_label(app.current_flora_paint_selection()),
                    center.x,
                    center.y,
                    center.z,
                );
            }
            Err(err) => {
                log::error!("[PERF][AUTHORED_FLORA_BENCH] sample {sample} failed: {err}");
                self.log_summary();
                return true;
            }
        }

        false
    }

    pub(super) fn fixed_response_time(&self) -> Option<f32> {
        self.response_perf
            .as_ref()
            .map(|perf| perf.frame as f32 / 60.)
    }

    fn log_summary(&self) {
        if self.results.is_empty() {
            log::info!("[PERF][AUTHORED_FLORA_BENCH_SUMMARY] samples 0");
            return;
        }

        let sum = self.results.iter().sum::<f32>();
        let avg = sum / self.results.len() as f32;
        let max = self
            .results
            .iter()
            .copied()
            .fold(0.0_f32, |acc, value| acc.max(value));
        let mut sorted = self.results.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let p95_idx = ((sorted.len() as f32 * 0.95).ceil() as usize).saturating_sub(1);
        let p95 = sorted[p95_idx.min(sorted.len() - 1)];

        log::info!(
            "[PERF][AUTHORED_FLORA_BENCH_SUMMARY] samples {} avg {:.3}ms p95 {:.3}ms max {:.3}ms",
            self.results.len(),
            avg,
            p95,
            max,
        );
    }
}

/// Same production scene and timeline in separate process-start modes. Does not
/// borrow the lifecycle replay's changing LOD, visibility or old-visual toggles.
#[derive(Debug, Default)]
struct ResponsePerf {
    frame: u32,
}

impl ResponsePerf {
    fn advance(&mut self, app: &mut App) -> anyhow::Result<bool> {
        let frame = self.frame;
        self.frame += 1;
        if frame < 30 {
            let species = frame % 3 + 1;
            app.player_tools.flora_paint_selection_index = species as usize;
            let xz = authored_flora_bench_center(frame);
            let center = Vec3::new(xz.x, app.query_terrain_height_cpu(xz), xz.y);
            let edit = TerrainBrushEdit {
                start: center,
                end: center,
                radius: super::TERRAIN_EDIT_DEFAULT_RADIUS,
            };
            app.apply_surface_flora_regeneration(edit, frame + 1, true)?;
            app.player_tools.flora_paint_selection_index = 0;
            app.apply_surface_flora_regeneration(edit, frame + 100, true)?;
        }
        if frame == 40 {
            app.terrain_physics.set_fruit_cycle(0.8, &mut app.tracer)?;
            let trees = app.terrain_physics.take_attached_fruit_refresh_trees();
            app.refresh_attached_tree_fruits(&trees)?;
            let target = Vec3::new(0.85, 0.50, 0.85);
            app.camera_control.set_orbit_focus(target);
            app.tracer
                .set_camera_pose_looking_at(target + Vec3::new(0., 0.55, 1.15), target);
        }
        if frame == 600 || frame == 2600 {
            let counts = (0..5)
                .map(|species| {
                    app.surface_builder
                        .resources
                        .instances
                        .chunk_flora_instances
                        .iter()
                        .map(|(_, chunk)| chunk.species_len(species))
                        .sum::<u32>()
                })
                .collect::<Vec<_>>();
            let leaves = app
                .surface_builder
                .resources
                .instances
                .leaves_instances
                .values()
                .map(|tree| tree.resources.instances_len)
                .sum::<u32>();
            let apples = app
                .surface_builder
                .resources
                .instances
                .apple_instances
                .values()
                .map(|tree| tree.resources.instances_len)
                .sum::<u32>();
            log::info!("[VEGETATION_RESPONSE][BENCH] phase={} app_frame={} simulation_frame={frame} flora={counts:?} leaves={leaves} apples={apples} timestep=0.016666667 controls=1,1,1 pose_hz=5",
                if frame == 600 { "sample" } else { "complete" }, app.time_info.total_frame_count());
        }
        Ok(frame == 2600)
    }
}

/// Opt-in lifecycle replay; the normal authored-flora benchmark is unchanged.
#[derive(Debug, Default)]
struct ResponseCheck {
    frame: u32,
    original: Option<(bool, bool, f32)>,
    edits: Vec<TerrainBrushEdit>,
    original_appearance: Option<(u32, f32)>,
    original_motion: Option<([f32; 4], f32, f32)>,
}

impl ResponseCheck {
    fn advance(&mut self, app: &mut App) -> anyhow::Result<bool> {
        let frame = self.frame;
        self.frame += 1;
        if frame == 0 {
            let gui = &app.debug_settings.adjustables;
            self.original_motion = Some((
                [
                    gui.vegetation_response_speed.value,
                    gui.vegetation_response_damping.value,
                    gui.vegetation_response_gain.value,
                    gui.vegetation_response_pose_hz.value,
                ],
                gui.tree_age.value,
                gui.fruit_cycle.value,
            ));
            self.original = Some((
                app.debug_settings.adjustables.flora_inertial_response.value,
                app.render_flags.enable_flora,
                app.debug_settings.adjustables.lod_distance.value,
            ));
            self.original_appearance = Some((
                app.debug_settings.adjustables.kochia_branch_count.value,
                app.debug_settings
                    .adjustables
                    .kochia_top_diameter_voxels
                    .value,
            ));
        }
        match frame {
            0..=2 => {
                app.player_tools.flora_paint_selection_index = frame as usize + 1;
                let xz = authored_flora_bench_center(frame * 8);
                let center = Vec3::new(xz.x, app.query_terrain_height_cpu(xz), xz.y);
                let edit = TerrainBrushEdit {
                    start: center,
                    end: center,
                    radius: super::TERRAIN_EDIT_DEFAULT_RADIUS,
                };
                app.apply_surface_flora_regeneration(edit, frame + 1, true)?;
                let count = app
                    .surface_builder
                    .authored_flora_base_positions_for_species(frame + 2)
                    .len();
                anyhow::ensure!(
                    count > 0,
                    "representative flower species {} failed to spawn",
                    frame + 2
                );
                self.edits.push(edit);
                log::info!(
                    "[VEGETATION_RESPONSE][SCENE] action=spawn species={} count={count}",
                    frame + 2
                );
            }
            3 => {
                app.player_tools.flora_paint_selection_index = 0;
                for edit in &self.edits {
                    app.apply_surface_flora_regeneration(*edit, 40, true)?;
                }
                let counts = [0, 1].map(|species| {
                    app.surface_builder
                        .resources
                        .instances
                        .chunk_flora_instances
                        .iter()
                        .map(|(_, chunk)| chunk.species_len(species))
                        .sum::<u32>()
                });
                anyhow::ensure!(
                    counts.iter().all(|&count| count > 0),
                    "grass fixture is empty: {counts:?}"
                );
                let target = self.edits[0].start + Vec3::Y * 0.06;
                app.camera_control.set_orbit_focus(target);
                app.tracer
                    .set_camera_pose_looking_at(target + Vec3::new(0., 0.20, 0.32), target);
                log::info!("[VEGETATION_RESPONSE][SCENE] action=paint-grass tall={} short={} close_camera=true", counts[0], counts[1]);
            }
            10 => {
                let identities = |app: &App| {
                    app.surface_builder
                        .resources
                        .instances
                        .chunk_flora_instances
                        .iter()
                        .flat_map(|(_, chunk)| {
                            chunk
                                .authored_response_instances
                                .iter()
                                .map(|plant| plant.response_id)
                        })
                        .collect::<Vec<_>>()
                };
                let before = identities(app);
                app.surface_builder.build_surface(glam::UVec3::ZERO, true)?;
                anyhow::ensure!(
                    identities(app) == before,
                    "chunk rebuild replaced authored plant identities"
                );
                log::info!(
                    "[VEGETATION_RESPONSE][SCENE] action=rebuild-chunk retained_ids={}",
                    before.len()
                );
            }
            15 => {
                app.debug_settings
                    .adjustables
                    .vegetation_response_speed
                    .value = 1.2;
                app.debug_settings
                    .adjustables
                    .vegetation_response_damping
                    .value = 1.3;
                app.debug_settings
                    .adjustables
                    .vegetation_response_gain
                    .value = 0.8;
                app.debug_settings
                    .adjustables
                    .vegetation_response_pose_hz
                    .value = 8.;
                app.debug_settings.adjustables.kochia_branch_count.value = 3;
                app.debug_settings
                    .adjustables
                    .kochia_top_diameter_voxels
                    .value = 9.;
                log::info!("[VEGETATION_RESPONSE][SCENE] action=change-kochia-profile");
            }
            20 => {
                let before = app
                    .surface_builder
                    .authored_flora_base_positions_for_species(2)
                    .len();
                app.apply_surface_flora_removal(self.edits[0])?;
                let after = app
                    .surface_builder
                    .authored_flora_base_positions_for_species(2)
                    .len();
                anyhow::ensure!(after < before, "flower removal had no effect");
                log::info!(
                    "[VEGETATION_RESPONSE][SCENE] action=remove before={before} after={after}"
                );
            }
            30 => {
                app.player_tools.flora_paint_selection_index = 1;
                app.apply_surface_flora_regeneration(self.edits[0], 1, true)?;
                log::info!("[VEGETATION_RESPONSE][SCENE] action=replant-same-brush");
            }
            35 | 75 => {
                app.terrain_physics.set_fruit_cycle(0.8, &mut app.tracer)?;
                let trees = app.terrain_physics.take_attached_fruit_refresh_trees();
                app.refresh_attached_tree_fruits(&trees)?;
                if frame == 75 {
                    let target = Vec3::new(1., 0.65, 1.);
                    app.camera_control.set_orbit_focus(target);
                    app.tracer
                        .set_camera_pose_looking_at(target + Vec3::new(0., 0.45, 1.15), target);
                }
            }
            38 => {
                app.terrain_physics.set_fruit_cycle(1., &mut app.tracer)?;
                let trees = app.terrain_physics.take_attached_fruit_refresh_trees();
                app.refresh_attached_tree_fruits(&trees)?;
            }
            55 | 65 => {
                app.debug_settings.adjustables.tree_age.value = if frame == 55 {
                    0.8
                } else {
                    self.original_motion.unwrap().1
                };
                app.update_all_tree_ages_from_gui()?;
            }
            40 => app.debug_settings.adjustables.flora_inertial_response.value = false,
            50 => app.render_flags.enable_flora = false,
            60 => app.render_flags.enable_flora = true,
            70 => app.debug_settings.adjustables.flora_inertial_response.value = true,
            80 => app.debug_settings.adjustables.lod_distance.value = 0.,
            90 => app.debug_settings.adjustables.lod_distance.value = 100.,
            100 => {
                app.tracer.validate_vegetation_response_draw_coverage()?;
                let (motion, age, _) = self.original_motion.unwrap();
                let gui = &mut app.debug_settings.adjustables;
                gui.vegetation_response_speed.value = motion[0];
                gui.vegetation_response_damping.value = motion[1];
                gui.vegetation_response_gain.value = motion[2];
                gui.vegetation_response_pose_hz.value = motion[3];
                gui.tree_age.value = age;
                let (_, visible, lod) = self.original.unwrap();
                app.debug_settings.adjustables.flora_inertial_response.value = true;
                app.render_flags.enable_flora = visible;
                app.debug_settings.adjustables.lod_distance.value = lod;
                let (branches, diameter) = self.original_appearance.unwrap();
                app.debug_settings.adjustables.kochia_branch_count.value = branches;
                app.debug_settings
                    .adjustables
                    .kochia_top_diameter_voxels
                    .value = diameter;
                log::info!("[VEGETATION_RESPONSE][SCENE] lifecycle_replay=completed frames=100 legacy_toggle=exercised visibility_toggle=exercised lod_toggle=exercised");
                log::info!(
                    "[VEGETATION_RESPONSE][TIMING] phase=c-first app_frame={}",
                    app.time_info.total_frame_count()
                );
            }
            300 => {
                app.debug_settings.adjustables.flora_inertial_response.value = false;
                log::info!(
                    "[VEGETATION_RESPONSE][TIMING] phase=legacy app_frame={}",
                    app.time_info.total_frame_count()
                );
            }
            500 => {
                app.debug_settings.adjustables.flora_inertial_response.value = true;
                log::info!(
                    "[VEGETATION_RESPONSE][TIMING] phase=c-repeat app_frame={}",
                    app.time_info.total_frame_count()
                );
            }
            700 => {
                app.debug_settings.adjustables.flora_inertial_response.value =
                    self.original.unwrap().0;
                log::info!(
                    "[VEGETATION_RESPONSE][TIMING] phase=complete app_frame={}",
                    app.time_info.total_frame_count()
                );
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }
}

fn authored_flora_bench_center(sample_index: u32) -> Vec2 {
    // Spread samples across the default 256-voxel world so repeated releases exercise
    // authored placement over varied terrain, not only one brush neighborhood.
    let grid_x = sample_index % 5;
    let grid_z = (sample_index / 5) % 5;
    let cycle = sample_index / 25;
    let x_vox = 36.0 + grid_x as f32 * 46.0 + (cycle % 3) as f32 * 3.0;
    let z_vox = 36.0 + grid_z as f32 * 46.0 + ((cycle + 1) % 3) as f32 * 3.0;
    Vec2::new(x_vox / 256.0, z_vox / 256.0)
}
