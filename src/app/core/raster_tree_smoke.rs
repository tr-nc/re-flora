//! Explicit hidden-app acceptance of the same saved A/B field used by the Debug checkbox.
use super::App;
use crate::lighting::{LightId, LocalLight, PointLight};
use anyhow::{ensure, Result};
use glam::Vec3;

pub(super) struct RasterTreeSmoke {
    frame: u32,
    initial_draws: u64,
    light: Option<LightId>,
}
impl RasterTreeSmoke {
    pub(super) fn new() -> Self {
        Self {
            frame: 0,
            initial_draws: 0,
            light: None,
        }
    }
    pub(super) fn run_next(app: &mut App) -> bool {
        let Some(mut smoke) = app.launch_owners.raster_tree_smoke.take() else {
            return false;
        };
        let done = smoke
            .step(app)
            .unwrap_or_else(|error| panic!("[TREE][RASTER_SMOKE] failed: {error:#}"));
        if !done {
            app.launch_owners.raster_tree_smoke = Some(smoke);
        }
        done
    }
    fn step(&mut self, app: &mut App) -> Result<bool> {
        self.frame += 1;
        match self.frame {
            1 | 30 => {
                app.debug_settings.adjustables.raster_tree_static.value = false;
                app.debug_settings.adjustables.tree_stiffness.value = 0.5;
                app.debug_settings
                    .adjustables
                    .raster_tree_hybrid_lighting
                    .value = false;
            }
            21 => {
                self.light = Some(
                    app.local_lights.add(LocalLight::Point(
                        PointLight::new(
                            app.debug_tree_pos + Vec3::new(0.15, 0.2, 0.1),
                            Vec3::new(1., 0.4, 0.15),
                            0.02,
                            0.01,
                            1.,
                        )
                        .expect("valid tree-lighting smoke fixture"),
                    )),
                );
                log::info!("[TREE][HYBRID_LIGHTING] added point-light fixture");
                app.debug_settings
                    .adjustables
                    .raster_tree_hybrid_lighting
                    .value = true;
            }
            25 => {
                app.tracer.validate_gpu_tree_lighting(true)?;
            }
            49 | 64 | 81 => {
                app.drive_tree_pose_smoke()?;
            }
            10 | 50 => {
                app.debug_settings.adjustables.raster_tree_wind.value = self.frame == 50;
                app.debug_settings
                    .adjustables
                    .raster_tree_hybrid_lighting
                    .value = self.frame == 50;
                self.initial_draws = app.tracer.raster_trees.color_draws;
                app.debug_settings.adjustables.raster_tree_static.value = true;
            }
            20 | 60 | 85 | 125 => {
                ensure!(app.tracer.raster_trees.enabled, "B is not enabled");
                app.validate_tree_poses()?;
                app.validate_tree_surface_pose()?;
                app.tracer.validate_gpu_tree_surface()?;
                app.tracer.validate_gpu_tree_lighting(
                    app.debug_settings
                        .adjustables
                        .raster_tree_hybrid_lighting
                        .value,
                )?;
                if self.frame >= 60 {
                    ensure!(
                        app.tracer.raster_trees.posed_surface.is_some(),
                        "wind surface not published"
                    );
                }
                ensure!(app.tracer.raster_trees.index_count > 0, "B has no geometry");
                ensure!(
                    app.tracer.raster_trees.color_draws > self.initial_draws,
                    "B did not record a color draw"
                );
                ensure!(
                    app.tracer.raster_trees.source.terrain_revision()
                        == Some(app.visible_terrain_revision),
                    "stale tree mesh"
                );
                log::info!(
                    "[TREE][RASTER_SMOKE] frame={} B_draws={} triangles={} revision={:?}",
                    self.frame,
                    app.tracer.raster_trees.color_draws,
                    app.tracer.raster_trees.index_count / 3,
                    app.tracer.raster_trees.source.terrain_revision()
                );
            }
            40 => {
                ensure!(
                    !app.tracer.raster_trees.enabled,
                    "A did not restore voxel rendering"
                );
            }
            61 => {
                app.debug_settings.adjustables.tree_stiffness.value = 0.;
            }
            80 => {
                app.debug_settings.adjustables.tree_stiffness.value = 1.;
            }
            126 => {
                if let Some(light) = self.light.take() {
                    app.local_lights
                        .remove(light)
                        .expect("live tree-lighting smoke fixture");
                }
                log::info!("[TREE][HYBRID_LIGHTING] removed point-light fixture");
                app.debug_settings.adjustables.tree_stiffness.value = 0.5;
                app.debug_settings
                    .adjustables
                    .raster_tree_hybrid_lighting
                    .value = false;
            }
            87 => {
                app.debug_settings.adjustables.raster_tree_wind.value = false;
            }
            88 => {
                ensure!(
                    app.tracer.raster_trees.posed_surface.is_none(),
                    "wind toggle did not restore rest surface"
                );
                app.tracer.validate_gpu_tree_surface()?;
            }
            89 => {
                app.debug_settings.adjustables.raster_tree_wind.value = true;
            }
            65 => {
                app.tracer.validate_gpu_tree_surface()?;
                app.validate_tree_surface_queries()?;
            }
            66 => {
                app.exercise_posed_tree_edit()?;
            }
            70 => {
                app.debug_settings.adjustables.tree_age.value = 0.5;
                app.update_all_tree_ages_from_gui()?;
            }
            90 => {
                app.clear_procedural_trees()?;
                app.remove_tree(app.trees.tuned_tree_id())?;
            }
            100 => {
                ensure!(
                    app.tracer.raster_trees.index_count == 0,
                    "deleted trees remain rasterized"
                );
            }
            110 => {
                app.debug_settings.adjustables.tree_age.value = 1.0;
                app.replace_single_tree(app.debug_settings.tree.desc.clone(), app.debug_tree_pos)?;
            }
            130 => {
                app.debug_settings.adjustables.raster_tree_wind.value = false;
            }
            135 => {
                ensure!(
                    app.tracer.raster_trees.posed_surface.is_none(),
                    "wind did not restore static surface"
                );
                app.tracer.validate_gpu_tree_surface()?;
                app.tracer.validate_gpu_tree_lighting(false)?;
            }
            140 => {
                log::info!("[TREE][RASTER_SMOKE] passed A_B_A_B=true hybrid_lighting_roundtrip=true stiffness_sweep=true age_rebuild=true remove=true replace=true color_draws={}",app.tracer.raster_trees.color_draws);
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }
}
