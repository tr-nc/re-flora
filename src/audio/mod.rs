mod audio_engine;
pub use audio_engine::*;

mod clip_cache;
pub use clip_cache::*;

mod sound_clip;

mod spatial_sound_manager;
pub use spatial_sound_manager::*;

mod audio_decoder;
pub use audio_decoder::*;

mod pal;
pub use pal::*;

mod spatial_sound;
pub use spatial_sound::*;

pub use kira::Tween;
pub use sound_clip::SoundClip;

use kira::sound::static_sound::StaticSoundSettings;

#[derive(PartialEq, Clone, Copy)]
pub enum PlayMode {
    Once,
    Loop,
}

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
    /// convert to kira settings (crate-internal use only)
    pub(crate) fn to_settings(&self) -> StaticSoundSettings {
        let mut settings = StaticSoundSettings::new()
            .volume(self.volume)
            .playback_rate(self.playback_rate)
            .panning(self.panning)
            .fade_in_tween(self.fade_in_tween.map(|t| Tween { ..t }));

        if self.mode == PlayMode::Loop {
            settings = settings.loop_region(..);
        }
        settings
    }
}
