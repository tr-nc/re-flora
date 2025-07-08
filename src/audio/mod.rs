use anyhow::Result;
use kira::{
    sound::static_sound::StaticSoundData, AudioManager, AudioManagerSettings, DefaultBackend,
};
use std::{thread, time::Duration};

#[derive(PartialEq)]
pub enum PlayMode {
    Once,
    Loop,
}

pub struct AudioEngine {
    manager: AudioManager,
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())?;
        Ok(Self { manager })
    }

    pub fn play(&mut self, path: &str, mode: PlayMode) -> Result<()> {
        let mut sound_data = match StaticSoundData::from_file(path) {
            Ok(sound_data) => sound_data,
            Err(e) => {
                return Err(anyhow::anyhow!("Error loading sound {}: {:?}", path, e));
            }
        };

        if mode == PlayMode::Loop {
            sound_data = sound_data.loop_region(..);
        }

        self.manager.play(sound_data)?;
        Ok(())
    }
}

#[test]
fn test() -> Result<()> {
    let mut audio_engine = AudioEngine::new()?;

    let sound_path = "assets/sfx/wind-ambient.wav";

    println!("\nPlaying sound on a loop. It will repeat until the program exits.");
    audio_engine.play(sound_path, PlayMode::Loop)?;

    thread::sleep(Duration::from_secs(30));

    Ok(())
}
