use anyhow::Result;
use kira::info::Info;
use kira::sound::Sound;
use kira::Frame as KiraFrame;

use crate::audio::spatial_sound_calculator::SpatialSoundCalculator;

// Custom Sound implementation for real-time processing
pub struct RealTimeSpatialSound {
    spatial_sound_calculator: SpatialSoundCalculator,
}

impl Sound for RealTimeSpatialSound {
    fn process(&mut self, out: &mut [kira::Frame], dt: f64, _info: &Info) {
        log::debug!("out.len(): {}", out.len());
        
        let samples = self.spatial_sound_calculator.get_samples(out.len());
        for i in 0..out.len() {
            out[i] = samples[i].frame;
        }
    }

    fn finished(&self) -> bool {
        false // Looping sound
    }
}

// Add methods to the struct
impl RealTimeSpatialSound {
    pub fn new(spatial_sound_calculator: SpatialSoundCalculator) -> Result<Self> {
        Ok(Self {
            spatial_sound_calculator,
        })
    }
}

// Wrapper to implement SoundData trait for RealTimeSpatialSound
pub struct RealTimeSpatialSoundData {
    spatial_sound: RealTimeSpatialSound,
}

impl RealTimeSpatialSoundData {
    pub fn new(spatial_sound_calculator: SpatialSoundCalculator) -> Result<Self> {
        let spatial_sound = RealTimeSpatialSound::new(spatial_sound_calculator)?;
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
