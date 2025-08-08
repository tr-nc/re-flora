use anyhow::Result;
use kira::info::Info;
use kira::sound::Sound;

use crate::audio::spatial_sound_manager::SpatialSoundManager;

pub struct RealTimeSpatialSound {
    spatial_sound_manager: SpatialSoundManager,
}

impl Sound for RealTimeSpatialSound {
    fn process(&mut self, out: &mut [kira::Frame], dt: f64, _info: &Info) {
        let device_sampling_rate = 1.0 / dt;
        self.spatial_sound_manager
            .fill_samples(out, device_sampling_rate);
    }

    fn finished(&self) -> bool {
        false // Looping sound
    }
}

impl RealTimeSpatialSound {
    pub fn new(spatial_sound_manager: SpatialSoundManager) -> Result<Self> {
        Ok(Self {
            spatial_sound_manager,
        })
    }
}

pub struct RealTimeSpatialSoundData {
    spatial_sound: RealTimeSpatialSound,
}

impl RealTimeSpatialSoundData {
    pub fn new(spatial_sound_manager: SpatialSoundManager) -> Result<Self> {
        let spatial_sound = RealTimeSpatialSound::new(spatial_sound_manager)?;
        Ok(Self { spatial_sound })
    }
}

impl kira::sound::SoundData for RealTimeSpatialSoundData {
    type Error = anyhow::Error;
    type Handle = (); // Use unit type since we don't need a specific handle type

    fn into_sound(self) -> Result<(Box<dyn kira::sound::Sound>, Self::Handle), Self::Error> {
        Ok((Box::new(self.spatial_sound), ()))
    }
}
