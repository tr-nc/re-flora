//! Explicit real-app contract for the sole procedural mesh tree system.
use super::App;
use crate::lighting::{LightId, LocalLight, PointLight};
use anyhow::{ensure, Result};
use glam::Vec3;

pub(super) struct RasterTreeSmoke {
    frame: u32,
    fingerprint: u64,
    terrain_revision: u32,
    light: Option<LightId>,
}
impl RasterTreeSmoke {
    pub(super) fn new() -> Self {
        Self {
            frame: 0,
            fingerprint: 0,
            terrain_revision: 0,
            light: None,
        }
    }
    pub(super) fn run_next(app: &mut App) -> bool {
        let Some(mut smoke) = app.launch_owners.raster_tree_smoke.take() else {
            return false;
        };
        let done = smoke
            .step(app)
            .unwrap_or_else(|e| panic!("[TREE][MESH_SMOKE] failed: {e:#}"));
        if !done {
            app.launch_owners.raster_tree_smoke = Some(smoke);
        }
        done
    }
    fn step(&mut self, app: &mut App) -> Result<bool> {
        self.frame += 1;
        match self.frame {
            1 => {
                app.debug_settings.adjustables.tree_wind.value = false;
            }
            20 => {
                self.fingerprint = app.tracer.raster_trees.rest_mesh.rest_fingerprint();
                self.terrain_revision = app.visible_terrain_revision;
                ensure!(
                    app.tracer.raster_trees.index_count > 0,
                    "missing continuous wood"
                );
                app.tracer.validate_gpu_tree_surface()?;
                app.tracer.validate_gpu_tree_lighting()?;
                ensure!(
                    app.tracer.raster_trees.posed_surface.is_some(),
                    "static tree lost query/collision surface"
                );
                self.light = Some(
                    app.local_lights.add(LocalLight::Point(
                        PointLight::new(
                            app.debug_tree_pos + Vec3::new(0.15, 0.2, 0.1),
                            Vec3::new(1., 0.4, 0.15),
                            0.02,
                            0.01,
                            1.,
                        )
                        .unwrap(),
                    )),
                );
            }
            25 => {
                app.tracer.validate_gpu_tree_lighting()?;
            }
            30 => {
                app.debug_settings.adjustables.tree_wind.value = true;
            }
            40 | 60 | 85 => {
                app.drive_tree_pose_smoke()?;
            }
            41 | 61 | 86 => {
                app.validate_tree_poses()?;
                app.validate_tree_surface_pose()?;
                app.tracer.validate_gpu_tree_surface()?;
                app.validate_tree_surface_queries()?;
                app.tracer.validate_gpu_tree_lighting()?;
                ensure!(
                    app.tracer.raster_trees.rest_mesh.rest_fingerprint() == self.fingerprint,
                    "wind changed rest geometry"
                );
            }
            50 => {
                app.debug_settings.adjustables.tree_stiffness.value = 0.;
            }
            70 => {
                app.debug_settings.adjustables.tree_stiffness.value = 1.;
            }
            90 => {
                app.debug_settings.adjustables.tree_age.value = 0.5;
                app.update_all_tree_ages_from_gui()?;
            }
            100 => {
                ensure!(
                    app.visible_terrain_revision == self.terrain_revision,
                    "mesh age changed terrain"
                );
                ensure!(
                    app.tracer.raster_trees.rest_mesh.rest_fingerprint() != self.fingerprint,
                    "age did not update wood"
                );
                app.tracer.validate_gpu_tree_surface()?;
                app.clear_procedural_trees()?;
                app.remove_tree(app.trees.tuned_tree_id())?;
            }
            110 => {
                ensure!(
                    app.tracer.raster_trees.index_count == 0,
                    "deleted tree remains visible"
                );
                ensure!(
                    app.tracer.raster_trees.scene.nodes.is_empty(),
                    "deleted tree remains in scene queries"
                );
                ensure!(
                    app.visible_terrain_revision == self.terrain_revision,
                    "tree removal changed terrain"
                );
                app.debug_settings.adjustables.tree_age.value = 1.;
                app.replace_single_tree(app.debug_settings.tree.desc.clone(), app.debug_tree_pos)?;
            }
            125 => {
                ensure!(
                    app.tracer.raster_trees.rest_mesh.rest_fingerprint() == self.fingerprint,
                    "replacement rerolled geometry"
                );
                app.tracer.validate_gpu_tree_surface()?;
                if let Some(light) = self.light.take() {
                    app.local_lights.remove(light).unwrap();
                }
                app.debug_settings.adjustables.tree_wind.value = false;
                app.debug_settings.adjustables.tree_stiffness.value = 0.5;
            }
            135 => {
                ensure!(
                    !app.tracer.raster_trees.wind_enabled,
                    "wind disable not published"
                );
                ensure!(
                    app.tracer.raster_trees.posed_surface.is_some(),
                    "wind disable removed collision"
                );
                app.tracer.validate_gpu_tree_surface()?;
                app.tracer.validate_gpu_tree_lighting()?;
            }
            160 => {
                log::info!("[TREE][MESH_SMOKE] PASS welded_mesh=true gpu_pose=true gpu_surface=true queries=true wind_roundtrip=true age=true remove_replace=true terrain_unchanged=true color_draws={}",app.tracer.raster_trees.color_draws);
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }
}
