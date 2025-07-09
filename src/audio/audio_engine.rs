use crate::audio::{SoundClip, SoundDataConfig};
use anyhow::Result;
use kira::{
    sound::static_sound::StaticSoundHandle, AudioManager, AudioManagerSettings, DefaultBackend,
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
    /// stop playback; if `tween` is None, stops immediately.
    #[allow(unused)]
    pub fn stop(&mut self, tween: Option<crate::audio::Tween>) {
        self.inner.stop(tween.unwrap_or_default());
    }
}

impl AudioEngine {
    /// create with default kira backend/settings
    pub fn new() -> Result<Self> {
        let mgr = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())?;
        Ok(Self {
            manager: Arc::new(Mutex::new(mgr)),
        })
    }

    /// play a clip with its baked-in settings
    pub fn play(&self, clip: &SoundClip) -> Result<SoundHandle> {
        let mut mgr = self.manager.lock().unwrap();
        let handle = mgr.play(clip.as_kira().clone())?;
        Ok(SoundHandle { inner: handle })
    }

    /// play a clip but override settings at call site
    #[allow(unused)]
    pub fn play_with_config(
        &self,
        clip: &SoundClip,
        config: SoundDataConfig,
    ) -> Result<SoundHandle> {
        let mut mgr = self.manager.lock().unwrap();
        let data = clip.as_kira().clone().with_settings(config.to_settings());
        let handle = mgr.play(data)?;
        Ok(SoundHandle { inner: handle })
    }
}
