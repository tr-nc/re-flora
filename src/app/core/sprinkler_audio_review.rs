//! Opt-in native, muted-only fixture. No settings, camera or world saves are written here.
use super::App;
use anyhow::{ensure, Result};
use glam::Vec3;

pub(super) struct SprinklerAudioReview {
    frame: u32,
    count: u32,
    center: Vec3,
    samples: Vec<f64>,
}
impl SprinklerAudioReview {
    pub fn from_environment() -> Option<Self> {
        let count: u32 = std::env::var("RE_FLORA_SPRINKLER_AUDIO_REVIEW")
            .ok()?
            .parse()
            .expect("audio review population");
        assert!(count <= 32);
        Some(Self {
            frame: 0,
            count,
            center: Vec3::ZERO,
            samples: Vec::with_capacity(120),
        })
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
            log::info!(
                "[SPRINKLER_AUDIO_REVIEW] placed={} committed={} muted=true",
                review.count,
                self.sprinklers.len()
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
            log::info!("[SPRINKLER_AUDIO_REVIEW] complete population={} samples={} reconcile_us_p50={:.3} p95={:.3} slots={:?}", review.count, review.samples.len(), review.samples[60], review.samples[114], counts);
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
        if (61..=180).contains(&review.frame) {
            assert_eq!(
                counts.0,
                review.count.min(16) as usize,
                "active voices must match committed bounded population"
            );
            review
                .samples
                .push(self.spatial_frame.sprinkler_reconcile_us());
        }
        if [180, 260, 295, 350].contains(&review.frame) {
            log::info!("[SPRINKLER_AUDIO_REVIEW] frame={} committed={} running={} active={} pending={} retiring={} render_owned={} engine_voices={}", review.frame, self.sprinklers.len(), review.running(), counts.0, counts.1, counts.2, counts.3, self.spatial_sound_manager.engine_voice_count());
        }
    }
}
