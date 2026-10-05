//! Opt-in hidden native evidence. Drives the same saved GUI fields in one
//! running app, never writes configuration. Not an alternate rendering path.
use super::App;
use anyhow::{ensure, Result};
use std::path::PathBuf;

pub(super) struct OrderedDitherReview {
    frame: u32,
    phase: usize,
    grid: bool,
    sun_view: bool,
    output: PathBuf,
}

impl OrderedDitherReview {
    pub(super) fn from_environment() -> Option<Self> {
        let mode = std::env::var("RE_FLORA_ORDERED_DITHER_REVIEW").ok()?;
        assert!(
            mode == "compare" || mode == "grid" || mode == "sun",
            "dither review: use compare, grid or sun"
        );
        let output = PathBuf::from(
            std::env::var_os("RE_FLORA_ORDERED_DITHER_OUT")
                .expect("dither review needs output directory"),
        );
        Some(Self {
            frame: 0,
            phase: 0,
            grid: mode == "grid",
            sun_view: mode == "sun",
            output,
        })
    }

    fn advance(&mut self, app: &mut App) -> Result<bool> {
        if app.loading_state.is_some() || app.splash_transition.is_some() {
            return Ok(false);
        }
        let total = if self.grid { 16 } else { CASES.len() };
        if self.phase == total {
            log::info!("[ORDERED_DITHER_REVIEW] complete phases={total} saved=false");
            return Ok(true);
        }
        let (name, flags, pattern, strength) = if self.grid {
            ("grid", 31, (self.phase % 2) as u32, 1.0)
        } else {
            CASES[self.phase]
        };
        if self.sun_view {
            let gui = &app.debug_settings.adjustables;
            let (altitude, azimuth) = App::calculate_sun_position(
                gui.time_of_day.value,
                gui.latitude.value,
                gui.season.value,
            );
            let direction = crate::util::get_sun_dir(altitude.asin().to_degrees(), azimuth * 360.0);
            let position = crate::app::camera_snapshots::player_default_camera_pose().position;
            let target = position + direction + glam::Vec3::new(0.12, -0.12, 0.0);
            ensure!(
                app.tracer.set_camera_pose_looking_at(position, target),
                "sun-view review camera"
            );
        }
        let gui = &mut app.debug_settings.adjustables;
        gui.ordered_dither_global.value = flags & 1 != 0;
        gui.ordered_dither_god_rays.value = flags & 2 != 0;
        gui.ordered_dither_lens_flare.value = flags & 4 != 0;
        gui.ordered_dither_sky_background.value = flags & 8 != 0;
        gui.ordered_dither_terrain_ambient.value = flags & 16 != 0;
        gui.ordered_dither_pattern.value = pattern;
        gui.ordered_dither_strength.value = strength;
        gui.ordered_dither_levels.value = 8;
        gui.scene_pixel_ratio.value = if self.grid {
            (self.phase / 4) as u32
        } else {
            3
        };
        gui.scene_supersampling_enabled.value = true;
        gui.scene_supersampling_quality.value = if self.grid {
            ((self.phase / 2) % 2) as u32
        } else {
            0
        };
        gui.scene_pixel_resolve_mode.value = if self.grid {
            (self.phase % 2) as u32
        } else {
            1
        };
        let filename = if self.grid {
            format!("grid-{}.png", self.phase)
        } else {
            format!("{name}.png")
        };
        let image = self.output.join(filename);
        if self.frame == 0 {
            std::fs::create_dir_all(&self.output)?;
            log::info!("[ORDERED_DITHER_REVIEW] phase={} case={name} app_frame={} flags={flags} pattern={pattern} levels=8 strength={strength} ratio={} density={} resolve={} saved=false",
                self.phase, app.time_info.total_frame_count(), gui.scene_pixel_ratio.value,
                gui.scene_supersampling_quality.value, gui.scene_pixel_resolve_mode.value);
            if self.grid && self.phase == 8 {
                let accepted = app
                    .window_state
                    .window()
                    .request_inner_size(winit::dpi::PhysicalSize::new(1023, 767));
                if let Some(size) = accepted {
                    app.queue_frame_extent(re_flora_vkn::Extent2D::new(size.width, size.height));
                }
            }
        }
        if self.frame == 48 {
            ensure!(
                !image.exists(),
                "review refuses stale image: {}",
                image.display()
            );
            *app.launch_owners.screenshot_mut() =
                super::screenshot::ScreenshotRuntime::new(Some(crate::ScreenshotOptions {
                    path: image.to_string_lossy().into_owned(),
                    delay: 0.0,
                    sequence: None,
                }));
        }
        self.frame += 1;
        ensure!(
            self.frame < 2400,
            "dither review screenshot readiness stalled"
        );
        if self.frame >= 64 && image.exists() {
            self.phase += 1;
            self.frame = 0;
        }
        Ok(false)
    }
}

// A return-to-original case demonstrates an in-process A/B, not separate builds.
const CASES: &[(&str, u32, u32, f32)] = &[
    ("original", 0, 0, 1.0),
    ("global-bayer", 1, 0, 1.0),
    ("global-halftone", 1, 1, 1.0),
    ("god-rays", 2, 0, 1.0),
    ("lens-flare", 4, 0, 1.0),
    ("terrain-ambient", 16, 0, 1.0),
    ("sky-background", 8, 0, 1.0),
    ("local-combined", 30, 0, 1.0),
    ("global-overrides-local", 31, 0, 1.0),
    ("zero-strength", 31, 0, 0.0),
    ("original-return", 0, 0, 1.0),
];

impl App {
    pub(super) fn advance_ordered_dither_review(&mut self) -> Result<bool> {
        let Some(mut review) = self.ordered_dither_review.take() else {
            return Ok(false);
        };
        let result = review.advance(self);
        self.ordered_dither_review = Some(review);
        result
    }
}
