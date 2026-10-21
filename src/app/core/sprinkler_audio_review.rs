//! Opt-in native, muted-only fixture. No settings, camera or world saves are written here.
use super::App;
use anyhow::{ensure, Result};
use glam::Vec3;

pub(super) struct SprinklerAudioReview {
    frame: u32,
    count: u32,
    center: Vec3,
    samples: Vec<f64>,
    reload: Option<String>,
    reload_waiting: bool,
    reload_resumed: bool,
    wet_pause: bool,
    pause_sample: Option<(u32, u32, f32)>,
    soil_center: Vec3,
}
impl SprinklerAudioReview {
    pub fn from_environment() -> Option<Self> {
        let count: u32 = std::env::var("RE_FLORA_SPRINKLER_AUDIO_REVIEW")
            .ok()?
            .parse()
            .expect("audio review population");
        assert!(count <= 32);
        Some(Self {
            wet_pause: std::env::var_os("RE_FLORA_SPRINKLER_WET_PAUSE").is_some(),
            pause_sample: None,
            soil_center: Vec3::ZERO,
            reload: std::env::var("RE_FLORA_SPRINKLER_AUDIO_RELOAD").ok(),
            reload_waiting: false,
            reload_resumed: false,
            frame: 0,
            count,
            center: Vec3::ZERO,
            samples: Vec::with_capacity(120),
        })
    }
    pub fn complete(&self) -> bool {
        self.frame > 360
    }
    pub fn apply_render_inputs(&self, inputs: &mut crate::tracer::RenderFrameInputs) {
        // Disposable frame facts only: never mutate saved GUI values.
        if (181..=200).contains(&self.frame) {
            inputs.environment.sprinkler_mist_density = 0.0;
            inputs.environment.sprinkler_mist_particles = false;
            inputs.environment.sprinkler_rotation_speed = 0.0;
        }
        if self.frame > 180 {
            inputs.environment.sprinkler_pulse_frequency = 20.0;
        }
        if (211..=225).contains(&self.frame) {
            inputs.environment.sprinkler_pulse_duty = 0.0;
        }
        if (226..=240).contains(&self.frame) {
            inputs.environment.sprinkler_pulse_duty = 100.0;
        }
    }
    pub fn running(&self) -> bool {
        !(241..=270).contains(&self.frame)
    }
}
impl App {
    pub(super) fn advance_sprinkler_audio_review(&mut self) -> Result<()> {
        let Some(mut review) = self.sprinkler_audio_review.take() else {
            return Ok(());
        };
        ensure!(
            self.mute_audio_output,
            "audio review requires --mute (never unmute)"
        );
        if review.frame == 0 {
            ensure!(
                self.sprinklers.is_empty(),
                "review requires no preexisting sprinklers"
            );
            review.center = self.tracer.camera_pose().position;
            for i in 0..review.count {
                self.apply_sprinkler_placement(
                    review.center
                        + Vec3::new(
                            (i % 6) as f32 * 0.12 - 0.3,
                            -0.1,
                            0.5 + (i / 6) as f32 * 0.12,
                        ),
                )?;
            }
            if review.wet_pause {
                let xz = glam::Vec2::new(0.72, 1.30);
                review.soil_center = Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y);
            }
            log::info!(
                "[SPRINKLER_AUDIO_REVIEW] placed={} committed={} muted=true",
                review.count,
                self.sprinklers.len()
            );
        }
        if review.frame == 120
            && review.reload.is_some()
            && !review.reload_resumed
            && !review.reload_waiting
        {
            let counts = self.spatial_frame.sprinkler_diagnostics();
            ensure!(
                counts.0 == review.count.min(16) as usize && counts.0 > 0,
                "reload must start with genuine active voices: {counts:?}"
            );
            let retained_sources = self
                .sprinklers
                .audio_sources()
                .into_iter()
                .map(|s| (s.id, s.position, s.animation_phase))
                .collect::<Vec<_>>();
            let clock_before = (
                self.world_clock.flora_tick(),
                self.world_clock.flora_tick_fraction(),
            );
            *self.terrain_persistence.snapshot_path_mut() = review.reload.clone().unwrap();
            self.perform_runtime_terrain_load();
            ensure!(
                clock_before
                    == (
                        self.world_clock.flora_tick(),
                        self.world_clock.flora_tick_fraction()
                    ),
                "synchronous load must not advance world time"
            );
            ensure!(
                self.water.phase() == super::water::WaterPhase::Quiesced
                    && !self.terrain_persistence.allows_sprinkler_audio(),
                "replacement must retain acknowledged water freeze and audio gate"
            );
            ensure!(
                retained_sources
                    == self
                        .sprinklers
                        .audio_sources()
                        .into_iter()
                        .map(|s| (s.id, s.position, s.animation_phase))
                        .collect::<Vec<_>>(),
                "session source identities, positions and phases must survive load exactly"
            );
            let counts = self.spatial_frame.sprinkler_diagnostics();
            ensure!(
                counts.0 + counts.1 == 0,
                "accepted load must retire active/pending voices immediately: {counts:?}"
            );
            ensure!(
                self.terrain_persistence.awaits_dependents(),
                "load must publish through normal dependency gate"
            );
            ensure!(
                self.sprinklers.len() == review.count as usize,
                "session sprinklers must survive terrain replacement"
            );
            review.reload_waiting = true;
            log::info!("[SPRINKLER_AUDIO_RELOAD] published active=0 session_retained=exact awaiting_dependents=true water=Quiesced world_clock=frozen_during_load");
        }
        if review.reload_waiting {
            if self.terrain_persistence.awaits_dependents() {
                ensure!(
                    self.spatial_frame.sprinkler_diagnostics().0 == 0,
                    "no re-admission before dependencies ready"
                );
                self.sprinkler_audio_review = Some(review);
                return Ok(());
            }
            ensure!(
                self.terrain_persistence.allows_world_updates()
                    && self.water.terrain_status().is_ready()
                    && self.water.phase() == super::water::WaterPhase::Running,
                "runtime load must resume real water/world readiness"
            );
            review.reload_waiting = false;
            review.reload_resumed = true;
            log::info!("[SPRINKLER_AUDIO_RELOAD] water=Running terrain=Ready world_updates=true reanchor=next_spatial_frame");
        }
        if review.frame == 150 && review.reload.is_some() {
            ensure!(
                self.spatial_frame.sprinkler_diagnostics().0 == review.count.min(16) as usize,
                "replacement must reanchor retained voices"
            );
            *self.terrain_persistence.snapshot_path_mut() =
                "target/garden-delivery/final_acceptance/intentionally-missing.rflterrain".into();
            self.perform_runtime_terrain_load();
            ensure!(
                self.spatial_frame.sprinkler_diagnostics().0 == 0,
                "recoverable failure also retires old voices"
            );
            ensure!(
                self.terrain_persistence.allows_world_updates()
                    && self.terrain_persistence.can_start_operation(),
                "pre-mutation failure must remain recoverable"
            );
            log::info!(
                "[SPRINKLER_AUDIO_RELOAD] recoverable_failure=true retired=true world_updates=true"
            );
        }
        if review.frame == 170 && review.reload.is_some() {
            ensure!(
                self.spatial_frame.sprinkler_diagnostics().0 == review.count.min(16) as usize,
                "recoverable failure must reanchor retained voices"
            );
            log::info!(
                "[SPRINKLER_AUDIO_RELOAD] complete successful_and_recoverable_load=true cap=16"
            );
        }
        if [201, 206, 271, 281].contains(&review.frame) {
            ensure!(self.mute_audio_output);
            match review.frame {
                201 => {
                    ensure!(self.debug_settings.audio_mix.terrain.enabled);
                    self.debug_settings.audio_mix.terrain.enabled = false;
                }
                206 => self.debug_settings.audio_mix.terrain.enabled = true,
                271 => {
                    ensure!(self.debug_settings.audio_mix.master.enabled);
                    self.debug_settings.audio_mix.master.enabled = false;
                }
                281 => self.debug_settings.audio_mix.master.enabled = true,
                _ => unreachable!(),
            }
            self.apply_effective_master_volume_gain("native mix review");
        }
        if review.wet_pause && review.frame == 230 {
            self.add_watering_brush_moisture(crate::app::world_edits::TerrainBrushEdit {
                start: review.soil_center,
                end: review.soil_center,
                radius: 0.12,
            })?;
        }
        if review.wet_pause && [250, 269, 295].contains(&review.frame) {
            let soil = self
                .plain_builder
                .sample_soil_moisture_sphere(review.soil_center, 0.08)?;
            let tick = self.world_clock.flora_tick();
            let fraction = self.world_clock.flora_tick_fraction();
            ensure!(soil.sum > 0, "pause review requires genuinely wet soil");
            if review.frame == 250 {
                review.pause_sample = Some((soil.sum, tick, fraction));
            }
            if review.frame == 269 {
                ensure!(review.pause_sample == Some((soil.sum, tick, fraction)), "global pause must preserve settled wet soil and clock: old={:?} now=({}, {tick}, {fraction})", review.pause_sample, soil.sum);
            }
            if review.frame == 295 {
                ensure!(
                    tick > review.pause_sample.unwrap().1,
                    "world clock must resume"
                );
            }
            log::info!(
                "[SPRINKLER_WET_PAUSE] frame={} soil={soil:?} tick={tick} fraction={fraction}",
                review.frame
            );
        }
        if review.frame == 300 {
            let removed =
                self.remove_sprinklers_in_brush(crate::app::world_edits::TerrainBrushEdit {
                    start: review.center,
                    end: review.center,
                    radius: 3.0,
                })?;
            ensure!(removed == review.count as usize);
        }
        if review.frame == 360 {
            let counts = self.spatial_frame.sprinkler_diagnostics();
            ensure!(
                counts.0 + counts.1 + counts.2 == 0,
                "removal did not reclaim all slots: {counts:?}"
            );
            review.samples.sort_by(f64::total_cmp);
            log::info!("[SPRINKLER_AUDIO_REVIEW] complete population={} samples={} reconcile_us_p50={:.3} p95={:.3} slots={:?}", review.count, review.samples.len(), review.samples[review.samples.len() / 2], review.samples[review.samples.len() * 95 / 100], counts);
        }
        review.frame += 1;
        self.sprinkler_audio_review = Some(review);
        Ok(())
    }
    pub(super) fn observe_sprinkler_audio_review(&mut self) {
        let Some(review) = &mut self.sprinkler_audio_review else {
            return;
        };
        let counts = self.spatial_frame.sprinkler_diagnostics();
        assert!(counts.0 + counts.1 + counts.2 <= 16);
        if (61..=180).contains(&review.frame)
            && !review.reload_waiting
            && (review.reload.is_none()
                || !((120..=130).contains(&review.frame) || (150..=160).contains(&review.frame)))
        {
            assert_eq!(
                counts.0,
                review.count.min(16) as usize,
                "active voices must match committed bounded population"
            );
            review
                .samples
                .push(self.spatial_frame.sprinkler_reconcile_us());
        }
        if [195, 210, 238, 295].contains(&review.frame) {
            assert_eq!(
                counts.0,
                review.count.min(16) as usize,
                "optical independence / continuous / resume"
            );
        }
        if [205, 222, 260, 278, 350].contains(&review.frame) {
            assert_eq!(
                counts.0 + counts.1 + counts.2,
                0,
                "zero duty / pause / removal must reclaim voices"
            );
        }
        if [180, 195, 205, 210, 222, 238, 260, 278, 295, 350].contains(&review.frame) {
            log::info!("[SPRINKLER_AUDIO_REVIEW] frame={} committed={} running={} active={} pending={} retiring={} render_owned={} engine_voices={}", review.frame, self.sprinklers.len(), review.running(), counts.0, counts.1, counts.2, counts.3, self.spatial_sound_manager.engine_voice_count());
        }
    }
}
