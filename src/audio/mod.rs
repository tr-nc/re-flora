use anyhow::Result;
use kira::{
    sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings},
    AudioManager, AudioManagerSettings, DefaultBackend, StartTime, Tween,
};
use std::{thread, time::Duration};

#[derive(PartialEq, Clone, Copy)]
pub enum PlayMode {
    Once,
    Loop,
}

/// A handle to a sound that is currently playing.
/// This can be used to control the sound after it has been played.
pub type SoundHandle = StaticSoundHandle;

/// Configuration for playing a sound.
pub struct PlayConfig {
    /// The volume to play the sound at. 1.0 is the original volume.
    pub volume: f32,
    /// The playback rate (speed/pitch) of the sound. 1.0 is the original speed.
    pub playback_rate: f64,
    /// The stereo panning of the sound. 0.0 is center, -1.0 is left, 1.0 is right.
    pub panning: f32,
    /// Whether the sound should loop or play only once.
    pub mode: PlayMode,
    /// The tween to use for the fade-in of the sound.
    pub fade_in_tween: Option<Tween>,
}

/// Default implementation for PlayConfig.
/// Plays the sound once at normal volume, pitch, and panning.
impl Default for PlayConfig {
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

pub struct AudioEngine {
    manager: AudioManager,
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())?;
        Ok(Self { manager })
    }

    /// Plays a sound with the specified configuration.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to the sound to play.
    /// * `config` - The configuration for volume, pitch, panning, and looping.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `SoundHandle` if successful, which can be used
    /// for later control of the sound instance.
    pub fn play(&mut self, path: &str, config: PlayConfig) -> Result<SoundHandle> {
        let mut settings = StaticSoundSettings::new()
            .volume(config.volume)
            .playback_rate(config.playback_rate)
            .panning(config.panning)
            .fade_in_tween(config.fade_in_tween);

        if config.mode == PlayMode::Loop {
            settings = settings.loop_region(..);
        }

        let sound_data = StaticSoundData::from_file(path)?.with_settings(settings);

        let handle = self.manager.play(sound_data)?;
        Ok(handle)
    }
}

#[test]
fn test() -> Result<()> {
    let mut audio_engine = AudioEngine::new()?;

    let leaf_rustling_sound = "assets/sfx/leaf-rustling.wav";
    let wind_ambient_sound = "assets/sfx/wind-ambient.wav";

    // --- Play the leaf rustling sound, looped and panned to the left ---
    println!("Playing looping leaf rustling sound...");
    let _leaf_handle = audio_engine.play(
        leaf_rustling_sound,
        PlayConfig {
            mode: PlayMode::Loop,
            volume: -20.0,
            fade_in_tween: Some(Tween {
                start_time: StartTime::Immediate,
                duration: Duration::from_secs(2),
                ..Default::default()
            }),
            ..Default::default()
        },
    )?;

    println!("Playing wind sound (higher pitch)...");
    let _wind_handle = audio_engine.play(
        wind_ambient_sound,
        PlayConfig {
            mode: PlayMode::Loop,
            volume: -10.0,
            fade_in_tween: Some(Tween {
                start_time: StartTime::Immediate,
                duration: Duration::from_secs(2),
                ..Default::default()
            }),
            ..Default::default()
        },
    )?;

    thread::sleep(Duration::from_secs(30));

    Ok(())
}
