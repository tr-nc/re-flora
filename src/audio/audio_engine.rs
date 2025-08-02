use crate::audio::SoundClip;
use anyhow::Result;
use kira::{
    sound::static_sound::StaticSoundHandle, AudioManager, AudioManagerSettings, Decibels,
    DefaultBackend,
};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AudioEngine {
    manager: Arc<Mutex<AudioManager<DefaultBackend>>>,
}

pub struct SoundHandle {
    inner: StaticSoundHandle,
}

impl SoundHandle {
    /// Stop playback; if `tween` is None, stops immediately.
    #[allow(dead_code)]
    pub fn stop(&mut self, tween: Option<crate::audio::Tween>) {
        self.inner.stop(tween.unwrap_or_default());
    }
}

impl AudioEngine {
    /// Create with default kira backend/settings
    pub fn new() -> Result<Self> {
        let mgr = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())?;
        Ok(Self {
            manager: Arc::new(Mutex::new(mgr)),
        })
    }

    /// Play a clip with its baked-in settings
    #[allow(dead_code)]
    pub fn play(&self, clip: &SoundClip) -> Result<SoundHandle> {
        let mut mgr = self.manager.lock().unwrap();
        let handle = mgr.play(clip.as_kira().clone())?;
        Ok(SoundHandle { inner: handle })
    }

    /// Converts a linear amplitude (0.0‒1.0) to decibels.
    ///
    /// * `amplitude` – a non-negative linear gain value.
    /// * returns     – a `Decibels` instance. Values at or below 0.0 map to `SILENCE`.
    pub fn amplitude_to_decibels(amplitude: f32) -> Decibels {
        if amplitude <= 0.0 {
            return Decibels::SILENCE;
        }
        return Decibels(20.0 * amplitude.log10());
    }

    /// Play a clip with a custom volume
    #[allow(dead_code)]
    pub fn play_with_volume(&self, clip: &SoundClip, volume: f32) -> Result<SoundHandle> {
        let mut mgr = self.manager.lock().unwrap();
        let data = clip
            .as_kira()
            .clone()
            .volume(Self::amplitude_to_decibels(volume));
        let handle = mgr.play(data)?;
        Ok(SoundHandle { inner: handle })
    }

    pub fn get_manager(&self) -> &Arc<Mutex<AudioManager<DefaultBackend>>> {
        &self.manager
    }
}
