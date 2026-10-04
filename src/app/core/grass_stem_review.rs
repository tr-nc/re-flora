//! Opt-in fixed-camera Release A/B fixture. Uses the normal seeded grass scene,
//! the real GUI setting and shared fixed-time benchmark clock; never saves tuning.
use super::App;
use anyhow::{ensure, Result};
use glam::{Vec2, Vec3};

const WARMUP_FRAMES: u32 = 120;
const TOTAL_FRAMES: u32 = 420;

#[derive(Debug)]
pub(super) struct GrassStemReview {
    pub(super) frame: u32,
    case: String,
    experimental: bool,
    distance: &'static str,
    species_mode: u32,
    curved: bool,
    pub(super) interactive: bool,
}

impl GrassStemReview {
    pub(super) fn from_environment() -> Option<Self> {
        if let Ok(case) = std::env::var("RE_FLORA_GRASS_STEM_REVIEW") {
            Some(Self::parse(&case).expect("grass stem review case"))
        } else if std::env::var_os("RE_FLORA_GRASS_STEM_TRYOUT").is_some() {
            let mut review = Self::parse("mid-both-a").unwrap();
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
            "expected <near|mid|far|top|low|inside>-<both|tall|short|curved>-<a|b>"
        );
        let distance = match parts[0] {
            "near" => "near",
            "mid" => "mid",
            "far" => "far",
            "top" => "top",
            "low" => "low",
            "inside" => "inside",
            _ => anyhow::bail!("unknown grass review distance"),
        };
        let species_mode = match parts[1] {
            "both" | "curved" => 0,
            "tall" => 1,
            "short" => 2,
            _ => anyhow::bail!("unknown grass review species mode"),
        };
        let experimental = match parts[2] {
            "a" => false,
            "b" => true,
            _ => anyhow::bail!("grass review mode must be a or b"),
        };
        Ok(Self {
            frame: 0,
            case: case.into(),
            experimental,
            distance,
            species_mode,
            curved: parts[1] == "curved",
            interactive: false,
        })
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
        if frame < 9 {
            // The authored-flora launch starts empty. Populate a fixed dense
            // grass patch via the real paint operation, using mature growth ticks.
            let patch = xz + Vec2::new((frame % 3) as f32 - 1.0, (frame / 3) as f32 - 1.0) * 0.10;
            let center = Vec3::new(patch.x, app.query_terrain_height_cpu(patch), patch.y);
            app.player_tools.flora_paint_selection_index = 0;
            app.apply_surface_flora_regeneration(
                crate::app::world_edits::TerrainBrushEdit {
                    start: center,
                    end: center,
                    radius: super::TERRAIN_EDIT_DEFAULT_RADIUS,
                },
                frame + 1,
                true,
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
            _ => Vec3::new(0.25, 0.45, 1.0),
        };
        app.camera_control.set_orbit_focus(target);
        ensure!(
            app.tracer
                .set_camera_pose_looking_at(target + offset, target),
            "grass stem review camera"
        );
        let gui = &mut app.debug_settings.adjustables;
        gui.grass_stem_rendering.value = self.experimental;
        gui.grass_render_mode.value = self.species_mode;
        if !self.interactive {
            gui.flora_growth_override_enabled.value = true;
            gui.flora_growth_override.value = 1.0;
            gui.flora_inertial_response.value = true;
            // Shared sampling quality is held fixed across both modes and all cases.
            gui.flower_stem_model_resolution.value = 45;
            gui.grass_natural_bend_min_voxels.value = if self.curved { 4.0 } else { 0.0 };
            gui.grass_natural_bend_max_voxels.value = if self.curved { 4.0 } else { 2.0 };
        }
        app.render_flags.enable_flora = true;
        if frame == WARMUP_FRAMES {
            if let Some(directory) = std::env::var_os("RE_FLORA_GRASS_STEM_CAPTURE") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory)?;
                *app.launch_owners.screenshot_mut() =
                    super::screenshot::ScreenshotRuntime::new(Some(crate::ScreenshotOptions {
                        path: directory
                            .join(format!("{}.png", self.case))
                            .to_string_lossy()
                            .into_owned(),
                        delay: 0.0,
                        sequence: None,
                    }));
            }
        }
        if frame == 0 || frame == WARMUP_FRAMES || frame == TOTAL_FRAMES {
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
            log::info!("[GRASS_STEM_REVIEW] case={} phase={} app_frame={} simulation_frame={} grass={counts:?} camera={:?} target={:?} resolution=45 saved=false",
                self.case, if frame == 0 { "start" } else if frame == WARMUP_FRAMES { "sample" } else { "complete" },
                app.time_info.total_frame_count(), frame, (target + offset).to_array(), target.to_array());
        }
        Ok(frame == TOTAL_FRAMES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paired_cases_only_change_the_rendering_mode() {
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
            let a = GrassStemReview::parse(&format!("{scene}-a")).unwrap();
            let b = GrassStemReview::parse(&format!("{scene}-b")).unwrap();
            assert!(!a.experimental);
            assert!(b.experimental);
            assert_eq!(a.distance, b.distance);
            assert_eq!(a.species_mode, b.species_mode);
            assert_eq!(a.curved, b.curved);
        }
        assert!(GrassStemReview::parse("near-both-c").is_err());
        assert!(GrassStemReview::parse("mid-all-a").is_err());
        assert!(GrassStemReview::parse("near-both-a-extra").is_err());
    }
}
