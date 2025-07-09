mod clip_loader;
pub use clip_loader::*;

use anyhow::Result;
use kira::{
    sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings},
    AudioManager, AudioManagerSettings, DefaultBackend, Tween,
};

#[derive(PartialEq, Clone, Copy)]
pub enum PlayMode {
    Once,
    Loop,
}

pub type SoundHandle = StaticSoundHandle;

pub struct SoundDataConfig {
    pub volume: f32,
    pub playback_rate: f64,
    pub panning: f32,
    pub mode: PlayMode,
    pub fade_in_tween: Option<Tween>,
}

impl Default for SoundDataConfig {
    fn default() -> Self {
        Self {
            volume: 1.0,
            playback_rate: 1.0,
            panning: 0.0,
            fade_in_tween: None,
            mode: PlayMode::Once,
        }
    }
}

impl SoundDataConfig {
    pub fn to_settings(&self) -> StaticSoundSettings {
        let mut settings = StaticSoundSettings::new()
            .volume(self.volume)
            .playback_rate(self.playback_rate)
            .panning(self.panning)
            .fade_in_tween(self.fade_in_tween);
        if self.mode == PlayMode::Loop {
            settings = settings.loop_region(..);
        }
        settings
    }
}
