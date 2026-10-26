//! Opt-in fixed-camera Release fixture for the sole square-band grass renderer.
//! Uses normal seeded grass and the shared benchmark clock; never saves tuning.
use super::App;
use anyhow::{ensure, Result};
use glam::{Vec2, Vec3};

const POST_PAINT_WARMUP_FRAMES: u32 = 111;

#[derive(Debug)]
pub(super) struct GrassStemReview {
    pub(super) frame: u32,
    case: String,
    distance: &'static str,
    species_mode: u32,
    curved: bool,
    grid: u32,
    pose_reuse: bool,
    pixelization: bool,
    pixel_lifecycle: bool,
    resolution: u32,
    sample_frames: u32,
    pub(super) interactive: bool,
    moisture_review: Option<String>,
    moisture_snapshot: Option<tempfile::TempDir>,
    snapshot_soil_sum: Option<u32>,
}

impl GrassStemReview {
    pub(super) fn from_environment() -> Option<Self> {
        if let Ok(case) = std::env::var("RE_FLORA_GRASS_STEM_REVIEW") {
            let mut review = Self::parse(&case).expect("grass stem review case");
            review.grid = environment_uint("RE_FLORA_GRASS_STEM_GRID", 3);
            review.moisture_review = std::env::var("RE_FLORA_GRASS_MOISTURE_REVIEW").ok();
            if let Some(mode) = &review.moisture_review {
                assert!(
                    ["dry", "manual", "sprinkler", "lifecycle", "support"].contains(&mode.as_str()),
                    "moisture review must be dry, manual, sprinkler, lifecycle or support"
                );
            }
            if review.moisture_review.as_deref() == Some("lifecycle") {
                review.moisture_snapshot =
                    Some(tempfile::tempdir().expect("disposable wet snapshot"));
            }
            let reuse = environment_uint("RE_FLORA_GRASS_BAND_POSE_REUSE", 1);
            assert!(reuse <= 1, "pose reuse must be 0 or 1");
            review.pose_reuse = reuse != 0;
            let pixels = environment_uint("RE_FLORA_GRASS_BAND_PIXELIZATION", 0);
            assert!(pixels <= 1, "pixelization must be 0 or 1");
            review.pixelization = pixels != 0;
            review.pixel_lifecycle = std::env::var_os("RE_FLORA_STEM_PIXEL_LIFECYCLE").is_some();
            review.resolution = environment_uint("RE_FLORA_STEM_PIXEL_RESOLUTION", 45);
            assert!(
                (8..=512).contains(&review.resolution),
                "stem samples must be 8..512"
            );
            // Legacy review requests below the saved model-grid minimum are
            // accepted, but report/apply the effective value, not the request.
            review.resolution =
                crate::flora::models::StemExperiment::normalize_model_resolution(review.resolution);
            review.sample_frames = environment_uint("RE_FLORA_STEM_SAMPLE_FRAMES", 300);
            assert!(
                (3..=15).contains(&review.grid) && review.grid % 2 == 1,
                "grass grid must be odd, 3..15"
            );
            assert!(
                (60..=600).contains(&review.sample_frames),
                "sample frames must be 60..600"
            );
            Some(review)
        } else if std::env::var_os("RE_FLORA_GRASS_STEM_TRYOUT").is_some() {
            let mut review = Self::parse("mid-both-b").unwrap();
            review.interactive = true;
            Some(review)
        } else {
            None
        }
    }

    fn parse(case: &str) -> Result<Self> {
        let parts: Vec<_> = case.split('-').collect();
        ensure!(
            parts.len() == 3,
            "expected <near|mid|far|top|low|inside|wide>-<both|tall|short|curved>-b"
        );
        let distance = match parts[0] {
            "near" => "near",
            "mid" => "mid",
            "far" => "far",
            "top" => "top",
            "low" => "low",
            "inside" => "inside",
            "wide" => "wide",
            _ => anyhow::bail!("unknown grass review distance"),
        };
        let species_mode = match parts[1] {
            "both" | "curved" => 0,
            "tall" => 1,
            "short" => 2,
            _ => anyhow::bail!("unknown grass review species mode"),
        };
        ensure!(parts[2] == "b", "legacy voxel grass mode has been removed; use the same scene with suffix -b for square-band meshes");
        Ok(Self {
            frame: 0,
            case: case.into(),
            distance,
            species_mode,
            curved: parts[1] == "curved",
            grid: 3,
            pose_reuse: true,
            pixelization: false,
            pixel_lifecycle: false,
            resolution: 45,
            sample_frames: 300,
            interactive: false,
            moisture_review: None,
            moisture_snapshot: None,
            snapshot_soil_sum: None,
        })
    }

    pub(super) fn fixed_response_time(&self) -> Option<f32> {
        // Freeze wind/spawn pose after warmup for moisture A/B images. Old
        // fixtures retain their advancing fixed clock and all existing modes.
        let frame = if self.moisture_review.is_some() {
            self.frame
                .min(self.grid * self.grid + POST_PAINT_WARMUP_FRAMES)
        } else {
            self.frame
        };
        (!self.interactive).then_some(frame as f32 / 60.)
    }

    fn pixel_settings(&self, frame: u32) -> (bool, u32) {
        if self.pixel_lifecycle {
            (
                (frame / 40) % 2 == 1,
                [32, 45, 192, 512][(frame / 60 % 4) as usize],
            )
        } else {
            (self.pixelization, self.resolution)
        }
    }

    pub(super) fn advance(&mut self, app: &mut App) -> Result<bool> {
        let frame = self.frame;
        self.frame += 1;
        // Manual review seeds the same grass patch and camera, then hands all
        // controls and movement back to the player. No perf overlay or saving.
        if self.interactive && frame >= 10 {
            return Ok(false);
        }
        ensure!(
            self.interactive || (app.perf_logging && app.gpu_profiler.is_some()),
            "grass stem review requires --perf GPU timestamps"
        );
        let xz = Vec2::new(0.85, 0.85);
        let warmup_frames = self.grid * self.grid + POST_PAINT_WARMUP_FRAMES;
        let total_frames = warmup_frames + self.sample_frames;
        if frame < self.grid * self.grid {
            // The authored-flora launch starts empty. Populate a fixed dense
            // grass patch via the real paint operation, using mature growth ticks.
            let half = (self.grid / 2) as f32;
            let patch = xz
                + Vec2::new(
                    (frame % self.grid) as f32 - half,
                    (frame / self.grid) as f32 - half,
                ) * 0.10;
            let center = Vec3::new(patch.x, app.query_terrain_height_cpu(patch), patch.y);
            app.player_tools.planting.flora_index = 0;
            app.apply_surface_flora_regeneration_at(
                crate::app::world_edits::TerrainBrushEdit {
                    start: center,
                    end: center,
                    radius: super::TERRAIN_EDIT_DEFAULT_RADIUS,
                },
                frame + 1,
                true,
                self.fixed_response_time().map_or_else(
                    || app.time_info.time_since_start_duration().as_millis() as u32,
                    |time| (time * 1000.0) as u32,
                ),
            )?;
        }
        let target = Vec3::new(xz.x, app.query_terrain_height_cpu(xz) + 0.015, xz.y);
        let offset = match self.distance {
            "near" => Vec3::new(0.025, 0.025, 0.07),
            "mid" => Vec3::new(0.08, 0.12, 0.30),
            // Nonzero Z keeps the downward look direction distinct from up.
            "top" => Vec3::new(0.0, 0.12, 0.0001),
            "low" => Vec3::new(0.0, 0.04, 0.0001),
            "inside" => Vec3::new(0.0, 0.008, 0.0001),
            "wide" => Vec3::new(0.5, 1.0, 1.6),
            _ => Vec3::new(0.25, 0.45, 1.0),
        };
        app.camera_control.set_orbit_focus(target);
        ensure!(
            app.tracer
                .set_camera_pose_looking_at(target + offset, target),
            "grass stem review camera"
        );
        if let Some(mode) = &self.moisture_review {
            app.debug_settings.adjustables.soil_drying_speed.value =
                if mode == "lifecycle" && frame >= 180 {
                    3.0
                } else {
                    0.0
                };
            if frame == 0 && (mode == "sprinkler" || mode == "lifecycle") {
                let base_xz = xz - Vec2::new(0.14, 0.0);
                let base = Vec3::new(base_xz.x, app.query_terrain_height_cpu(base_xz), base_xz.y);
                app.apply_sprinkler_placement(base)?;
                let gui = &mut app.debug_settings.adjustables;
                gui.sprinkler_wetting_speed.value = 3.0;
                app.sprinkler_diagnostic_timing.duty_percent = 100.0;
            }
            if (mode == "manual" || mode == "lifecycle" || mode == "support")
                && (10..=25).contains(&frame)
            {
                let center = Vec3::new(xz.x, app.query_terrain_height_cpu(xz), xz.y);
                app.add_watering_brush_moisture(crate::app::world_edits::TerrainBrushEdit {
                    start: center,
                    end: center,
                    radius: 0.12,
                })?;
            }
            if mode == "support" && frame == 150 {
                let center = Vec3::new(xz.x, app.query_terrain_height_cpu(xz), xz.y);
                let before = app
                    .plain_builder
                    .sample_soil_moisture_sphere(center, 0.05)?;
                ensure!(before.sum > 0, "support removal requires real wet soil");
                let removed = app.apply_surface_terrain_removal(
                    crate::app::world_edits::TerrainBrushEdit {
                        start: center,
                        end: center,
                        radius: 0.04,
                    },
                    Some(crate::builder::VOXEL_TYPE_DIRT),
                    None,
                    None,
                )?;
                ensure!(removed.stats.count_removed(crate::builder::VOXEL_TYPE_DIRT) > 0);
                let after = app
                    .plain_builder
                    .sample_soil_moisture_sphere(center, 0.05)?;
                ensure!(
                    after.count < before.count,
                    "Dig must actually remove grass support"
                );
                // Brush overlaps the world boundary. Production editing, watering and
                // planting must clip it rather than wrapping atlas addresses.
                let edge_xz = Vec2::new(0.002, 0.85);
                let edge = Vec3::new(edge_xz.x, app.query_terrain_height_cpu(edge_xz), edge_xz.y);
                let edit = crate::app::world_edits::TerrainBrushEdit {
                    start: edge,
                    end: edge,
                    radius: 0.04,
                };
                app.apply_surface_flora_regeneration_at(edit, 150, true, 1000)?;
                app.add_watering_brush_moisture(edit)?;
                let boundary = app.apply_surface_terrain_removal(
                    edit,
                    Some(crate::builder::VOXEL_TYPE_DIRT),
                    None,
                    None,
                )?;
                ensure!(
                    boundary
                        .stats
                        .count_removed(crate::builder::VOXEL_TYPE_DIRT)
                        > 0
                );
                ensure!(
                    app.resolve_plantable_surface_column(glam::UVec2::splat(u32::MAX))
                        .is_err(),
                    "invalid columns must not wrap"
                );
                log::info!("[GRASS_SUPPORT_REVIEW] removed={} soil_before={before:?} soil_after={after:?} boundary_removed={} invalid_column=rejected real_edits=true", removed.stats.count_removed(crate::builder::VOXEL_TYPE_DIRT), boundary.stats.count_removed(crate::builder::VOXEL_TYPE_DIRT));
            }
            if mode == "lifecycle" {
                let center = Vec3::new(xz.x, app.query_terrain_height_cpu(xz), xz.y);
                let edit = crate::app::world_edits::TerrainBrushEdit {
                    start: center,
                    end: center,
                    radius: 0.12,
                };
                if frame == 30 || frame == 60 {
                    let paused = frame == 30;
                    app.sprinkler_diagnostic_timing.duty_percent = if paused { 0.0 } else { 100.0 };
                    log::info!("[GRASS_MOISTURE_LIFECYCLE] valve_paused={paused} frame={frame} authoritative_duty={}", app.sprinkler_diagnostic_timing.duty_percent);
                }
                if frame == 60 {
                    let before = app
                        .plain_builder
                        .sample_soil_moisture_sphere(center, 0.08)?;
                    ensure!(before.sum > 0, "replant needs actual wet soil");
                    app.apply_surface_flora_removal(edit)?;
                    app.apply_surface_flora_regeneration_at(edit, 600, true, 1000)?;
                    log::info!("[GRASS_MOISTURE_LIFECYCLE] replant wet_soil={before:?}");
                }
                if frame == 90 {
                    let path = self
                        .moisture_snapshot
                        .as_ref()
                        .unwrap()
                        .path()
                        .join("wet.rfworld");
                    *app.terrain_persistence.snapshot_path_mut() =
                        path.to_string_lossy().into_owned();
                    self.snapshot_soil_sum = Some(
                        app.plain_builder
                            .sample_soil_moisture_sphere(center, 0.08)?
                            .sum,
                    );
                    app.perform_runtime_terrain_save();
                    ensure!(path.is_file(), "disposable wet snapshot was not saved");
                }
                if frame == 120 {
                    let removed = app.remove_sprinklers_in_brush(
                        crate::app::world_edits::TerrainBrushEdit {
                            radius: 0.4,
                            ..edit
                        },
                    )?;
                    ensure!(removed == 1, "lifecycle sprinkler removal");
                }
                if frame == 150 {
                    app.perform_runtime_terrain_load();
                    let restored = app
                        .plain_builder
                        .sample_soil_moisture_sphere(center, 0.08)?;
                    ensure!(
                        Some(restored.sum) == self.snapshot_soil_sum,
                        "wet snapshot moisture changed on restore"
                    );
                    log::info!(
                        "[GRASS_MOISTURE_LIFECYCLE] restored soil={restored:?} original_sum={:?}",
                        self.snapshot_soil_sum
                    );
                }
            }
            if frame == warmup_frames
                || frame == total_frames
                || (mode == "lifecycle" && [30, 59, 60, 90, 150, 180, 300, 600].contains(&frame))
            {
                let center = Vec3::new(xz.x, app.query_terrain_height_cpu(xz), xz.y);
                let stats = app
                    .plain_builder
                    .sample_soil_moisture_sphere(center, 0.08)?;
                let neighbor_xz = xz + Vec2::new(0.35, 0.0);
                let neighbor = Vec3::new(
                    neighbor_xz.x,
                    app.query_terrain_height_cpu(neighbor_xz),
                    neighbor_xz.y,
                );
                let dry = app
                    .plain_builder
                    .sample_soil_moisture_sphere(neighbor, 0.04)?;
                log::info!("[GRASS_MOISTURE_REVIEW] mode={mode} response=true frame={frame} soil={stats:?} neighbor={dry:?} pose_reuse={} pixels={} growth_override=1 drying_speed={} saved=false", self.pose_reuse, self.pixelization, app.debug_settings.adjustables.soil_drying_speed.value);
            }
        }
        let gui = &mut app.debug_settings.adjustables;
        gui.grass_render_mode.value = self.species_mode;
        if !self.interactive {
            gui.grass_band_pose_reuse.value = self.pose_reuse;
            gui.flora_growth_override_enabled.value = true;
            gui.flora_growth_override.value = if self.pixel_lifecycle {
                (frame % 160) as f32 / 159.0
            } else {
                1.0
            };
            (
                gui.grass_band_pixelization.value,
                gui.flower_stem_model_resolution.value,
            ) = self.pixel_settings(frame);
            if self.pixel_lifecycle {
                // Resize after tiles have been rendered, not only during loading.
                let size = match frame {
                    52 => Some((1152, 648)),
                    132 => Some((960, 600)),
                    212 => Some((1280, 720)),
                    _ => None,
                };
                if let Some((width, height)) = size {
                    let accepted = app
                        .window_state
                        .window()
                        .request_inner_size(winit::dpi::PhysicalSize::new(width, height));
                    log::info!("[STEM_PIXEL_LIFECYCLE] frame={frame} resize={width}x{height} accepted={accepted:?} saved=false");
                }
                if frame.is_multiple_of(40) {
                    log::info!(
                        "[STEM_PIXEL_LIFECYCLE] frame={frame} enabled={} resolution={} growth={} saved=false",
                        gui.grass_band_pixelization.value,
                        gui.flower_stem_model_resolution.value,
                        gui.flora_growth_override.value
                    );
                }
            }
            gui.flora_spawn_duration_seconds.value = 0.28;
            gui.flora_inertial_response.value = true;
            // Shared sampling quality is fixed unless the lifecycle fixture is active.
            gui.grass_natural_bend_max_voxels.value = if self.curved { 4.0 } else { 2.0 };
        }
        app.render_flags.enable_flora = true;
        if frame == warmup_frames
            || (self.moisture_review.as_deref() == Some("lifecycle")
                && [30, 59, 60, 90, 150, 180, 300, 600].contains(&frame))
        {
            if let Some(directory) = std::env::var_os("RE_FLORA_GRASS_STEM_CAPTURE") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory)?;
                *app.launch_owners.screenshot_mut() =
                    super::screenshot::ScreenshotRuntime::new(Some(crate::ScreenshotOptions {
                        path: directory
                            .join(if self.moisture_review.as_deref() == Some("lifecycle") {
                                format!("{}-{frame}.png", self.case)
                            } else {
                                format!("{}.png", self.case)
                            })
                            .to_string_lossy()
                            .into_owned(),
                        delay: 0.0,
                        sequence: None,
                    }));
            }
        }
        if frame == 0 || frame == warmup_frames || frame == total_frames {
            let counts = [0, 1].map(|species| {
                app.surface_builder
                    .resources
                    .instances
                    .chunk_flora_instances
                    .iter()
                    .map(|(_, chunk)| chunk.species_len(species))
                    .sum::<u32>()
            });
            ensure!(
                frame == 0 || counts.iter().all(|count| *count > 0),
                "grass review has empty grass species: {counts:?}"
            );
            log::info!("[GRASS_STEM_REVIEW] case={} phase={} app_frame={} simulation_frame={} grass={counts:?} camera={:?} target={:?} resolution={} saved=false grid={} geometry=square_bands",
                self.case, if frame == 0 { "start" } else if frame == warmup_frames { "sample" } else { "complete" },
                app.time_info.total_frame_count(), frame, (target + offset).to_array(), target.to_array(), gui.flower_stem_model_resolution.value, self.grid);
        }
        Ok(frame == total_frames)
    }
}

fn environment_uint(name: &str, default: u32) -> u32 {
    std::env::var(name).map_or(default, |value| {
        value.parse().expect("unsigned review setting")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_planting_clock_does_not_depend_on_real_runtime() {
        let mut review = GrassStemReview::parse("wide-both-b").unwrap();
        review.frame = 225;
        assert_eq!(review.fixed_response_time(), Some(3.75));
        review.frame += POST_PAINT_WARMUP_FRAMES;
        assert!(review.fixed_response_time().unwrap() - 3.75 > 0.28);
        review.interactive = true;
        assert_eq!(review.fixed_response_time(), None);
    }

    #[test]
    fn moisture_review_freezes_pose_but_not_the_original_fixture() {
        let mut review = GrassStemReview::parse("near-both-b").unwrap();
        review.moisture_review = Some("manual".into());
        review.frame = 120;
        let pose = review.fixed_response_time();
        review.frame = 600;
        assert_eq!(review.fixed_response_time(), pose);
        review.moisture_review = None;
        assert_eq!(review.fixed_response_time(), Some(10.0));
    }

    #[test]
    fn pixel_lifecycle_changes_the_effective_settings() {
        let mut review = GrassStemReview::parse("near-both-b").unwrap();
        assert_eq!(review.pixel_settings(200), (false, 45));
        review.pixel_lifecycle = true;
        for (frame, expected) in [
            (40, (true, 32)),
            (80, (false, 45)),
            (120, (true, 192)),
            (200, (true, 512)),
        ] {
            assert_eq!(review.pixel_settings(frame), expected);
        }
    }

    #[test]
    fn review_accepts_mesh_cases_and_rejects_retired_voxel_cases() {
        for scene in [
            "near-both",
            "mid-both",
            "far-both",
            "mid-tall",
            "mid-short",
            "near-curved",
            "top-both",
            "low-both",
            "inside-both",
            "low-curved",
        ] {
            assert!(GrassStemReview::parse(&format!("{scene}-a")).is_err());
            assert!(GrassStemReview::parse(&format!("{scene}-b")).is_ok());
        }
        assert!(GrassStemReview::parse("near-both-c").is_err());
        assert!(GrassStemReview::parse("mid-all-a").is_err());
        assert!(GrassStemReview::parse("near-both-a-extra").is_err());
    }
}
