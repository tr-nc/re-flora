//! Developer-only comparison of the retired air experiment. Never saves controls.
use super::App;
use anyhow::{ensure, Context, Result};
use std::path::PathBuf;

pub(super) struct AirReview {
    mode: String,
    directory: Option<PathBuf>,
    contracts: Option<Vec<u8>>,
    field_hash: Option<u64>,
    original: Option<(bool, bool, f32, f32)>,
    frozen_wind: Option<crate::wind_field::WindFieldFrame>,
}

impl AirReview {
    pub(super) fn from_env() -> Result<Option<Self>> {
        let Ok(mode) = std::env::var("RE_FLORA_SPRINKLER_AIR_REVIEW") else {
            return Ok(None);
        };
        ensure!(matches!(mode.as_str(), "original" | "air" | "switch"),
            "RE_FLORA_SPRINKLER_AIR_REVIEW expects original|air|switch with RE_FLORA_SPRINKLER_REVIEW");
        let directory = if mode == "switch" {
            let path =
                PathBuf::from(std::env::var("RE_FLORA_SPRINKLER_AIR_CAPTURE").context(
                    "switch review needs a NEW RE_FLORA_SPRINKLER_AIR_CAPTURE directory",
                )?);
            ensure!(
                !path.exists(),
                "air capture directory must not already exist"
            );
            std::fs::create_dir_all(&path)?;
            Some(path)
        } else {
            None
        };
        Ok(Some(Self {
            mode,
            directory,
            contracts: None,
            field_hash: None,
            original: None,
            frozen_wind: None,
        }))
    }

    pub(super) fn world_running(&self, frame: u32) -> bool {
        self.mode != "switch" || !(180..420).contains(&frame)
    }

    pub(super) fn apply_render_inputs(
        &mut self,
        frame: u32,
        inputs: &mut crate::tracer::RenderFrameInputs,
    ) {
        inputs.environment.sprinkler_mist_air = match self.mode.as_str() {
            "air" => true,
            "switch" => (220..260).contains(&frame) || frame >= 300,
            _ => false,
        };
        if !self.world_running(frame) {
            // WorldClock pause alone does not freeze the independently animated
            // wind field. Hold the REAL uploaded wind snapshot for a matched A/B.
            inputs.wind.field = *self.frozen_wind.get_or_insert(inputs.wind.field);
        } else {
            self.frozen_wind = None;
        }
    }

    pub(super) fn prepare(&mut self, app: &mut App, frame: u32) -> Result<()> {
        if frame == 0 {
            ensure!(app.mute_audio_output, "air review must be muted");
            ensure!(
                std::env::var_os("RE_FLORA_SPRINKLER_OPTICAL_DIAGNOSTIC").is_none(),
                "do not mix original consumer ablation with the air-renderer comparison"
            );
            log::info!(
                "[SPRINKLER_AIR_REVIEW] mode={} saved=false water=unchanged droplets=unchanged",
                self.mode
            );
        }
        if self.mode != "switch" {
            return Ok(());
        }
        if frame == 180 {
            let gui = &app.debug_settings.adjustables;
            self.original = Some((
                app.render_flags.enable_god_rays,
                app.render_flags.enable_particles,
                gui.sprinkler_mist_density.value,
                app.sprinkler_diagnostic_timing.duty_percent,
            ));
        }
        if frame == 190 {
            self.contracts = Some(contracts(app)?);
        }
        if [250, 290, 330].contains(&frame) {
            ensure!(
                self.contracts.as_ref() == Some(&contracts(app)?),
                "air A/B changed paused contact or droplet geometry"
            );
            log::info!(
                "[SPRINKLER_AIR_REVIEW] frame={frame} paused_contacts_and_droplets=identical"
            );
        }
        if [245, 250, 410].contains(&frame) {
            let (info, density) = app.tracer.sprinkler_air_review_field()?;
            ensure!(
                bytemuck::cast_slice::<_, f32>(std::slice::from_ref(&info))
                    .iter()
                    .all(|v| v.is_finite()),
                "nonfinite air header"
            );
            let live = density.iter().filter(|&&d| d > 0).count();
            let sum: u64 = density.iter().map(|&d| u64::from(d)).sum();
            let hash = density.iter().fold(0xcbf29ce484222325u64, |h, &d| {
                (h ^ u64::from(d)).wrapping_mul(0x100000001b3)
            });
            if frame == 410 {
                ensure!(
                    live == 0 && info.lower_valid[3] == 0.0,
                    "duty-zero retained stale density"
                );
            } else {
                ensure!(
                    live > 0 && info.lower_valid[3] == 1.0,
                    "air field has no live mist"
                );
                if frame == 245 {
                    self.field_hash = Some(hash);
                } else {
                    ensure!(self.field_hash == Some(hash), "paused air field changed");
                }
            }
            log::info!("[SPRINKLER_AIR_FIELD] frame={frame} live={live} sum={sum} hash={hash:x} cell={} side={}", info.cell_side[0], info.cell_side[3]);
        }
        if let Some((god, particles, density, duty)) = self.original {
            app.render_flags.enable_god_rays = god && !(300..340).contains(&frame);
            app.render_flags.enable_particles = particles && !(340..380).contains(&frame);
            let gui = &mut app.debug_settings.adjustables;
            gui.sprinkler_mist_density.value = if (380..400).contains(&frame) {
                0.0
            } else {
                density
            };
            app.sprinkler_diagnostic_timing.duty_percent = if (400..420).contains(&frame) {
                0.0
            } else {
                duty
            };
        }
        let capture = match frame {
            200 => Some("original"),
            240 => Some("air"),
            280 => Some("original-return"),
            320 => Some("air-no-global-rays"),
            360 => Some("air-no-particles"),
            390 => Some("zero-mist"),
            415 => Some("zero-duty"),
            440 => Some("resumed"),
            _ => None,
        };
        if let Some(label) = capture {
            let path = self
                .directory
                .as_ref()
                .unwrap()
                .join(format!("{label}.png"));
            *app.launch_owners.screenshot_mut() =
                super::screenshot::ScreenshotRuntime::new(Some(crate::ScreenshotOptions {
                    path: path.to_string_lossy().into_owned(),
                    delay: 0.0,
                    sequence: None,
                }));
            log::info!(
                "[SPRINKLER_AIR_REVIEW] frame={frame} capture={label} world_running={}",
                self.world_running(frame)
            );
        }
        if frame == 450 {
            log::info!("[SPRINKLER_AIR_REVIEW] complete toggle_pause_zero_resume=true saved=false");
        }
        Ok(())
    }
}

fn contracts(app: &App) -> Result<Vec<u8>> {
    let contacts = app.tracer.sprinkler_water_review_contacts()?;
    let mut bytes = bytemuck::cast_slice(&contacts).to_vec();
    for lobe in app.tracer.sprinkler_review_droplets()? {
        // Environment lighting may converge while paused; optical births,
        // trajectories, first-contact removal and amplitudes may not change.
        bytes.extend_from_slice(bytemuck::bytes_of(&lobe.center_amplitude));
        bytes.extend_from_slice(bytemuck::bytes_of(&lobe.radii_near_geometry));
        bytes.extend_from_slice(bytemuck::bytes_of(&lobe.tangent_rotary));
    }
    Ok(bytes)
}
