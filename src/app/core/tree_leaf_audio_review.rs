//! Explicit hidden/muted native fixture; never mutates saved GUI or world settings.
use crate::{audio::TreeAudioManager, wind_field::WindFieldFrame};
use anyhow::{ensure, Result};

pub(super) struct Review {
    start: Option<f32>,
    stage: usize,
    passed: bool,
}

const STAGES: [(f32, bool, f32); 6] = [
    (1.0, false, 0.0),
    (1.0, true, 0.0),
    (2.0, true, 3.6),
    (2.0, true, 8.0),
    (1.0, false, 8.0),
    (2.0, true, 8.0),
];

impl Review {
    pub fn from_environment() -> Option<Self> {
        std::env::var_os("RE_FLORA_TREE_LEAF_AUDIO_REVIEW").map(|_| Self {
            start: None,
            stage: 0,
            passed: false,
        })
    }

    pub fn inputs(&mut self, time: f32, muted: bool) -> Result<(bool, WindFieldFrame)> {
        ensure!(muted, "tree leaf audio review requires --mute");
        self.start.get_or_insert(time);
        let (_, recorded, strength) = STAGES[self.stage];
        Ok((recorded, WindFieldFrame::uniform(glam::Vec2::X * strength)))
    }

    pub fn observe(&mut self, audio: &TreeAudioManager, time: f32) -> Result<()> {
        if self.passed {
            return Ok(());
        }
        let elapsed = time - self.start.unwrap();
        let (end, recorded, _) = STAGES[self.stage];
        if elapsed < end {
            return Ok(());
        }
        let (total, layered, min, max) = audio.recorded_mix_diagnostics();
        ensure!(total > 0, "review needs genuine populated tree canopies");
        ensure!(
            layered == if recorded { total } else { 0 },
            "A/B source mismatch: {total} total, {layered} layered"
        );
        match self.stage {
            1 => ensure!(max < 0.001, "calm wind must remain breeze-only"),
            2 => ensure!(
                min > 0.0 && max < 1.0,
                "weak wind must mix both layers: {min}..{max}"
            ),
            3 | 5 => ensure!(min > 0.99, "strong wind must reach strong recording: {min}"),
            _ => (),
        }
        log::info!("[TREE_LEAF_AUDIO_REVIEW] stage={} recorded={recorded} canopies={total} layered={layered} blend={min:.6}..{max:.6} saved=false", self.stage);
        if self.stage == 5 {
            self.passed = true;
            log::info!("[TREE_LEAF_AUDIO_REVIEW] passed stages=6 one_voice_per_generation=true saved=false");
        } else {
            // Only observations advance the fixture, so a rendering hitch cannot skip a stage.
            self.stage += 1;
            self.start = Some(time);
        }
        Ok(())
    }
}
