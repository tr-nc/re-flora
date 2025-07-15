use crate::audio::{AudioEngine, ClipCache, SoundDataConfig};
use anyhow::Result;

pub struct PlayerClipCaches {
    pub walk: ClipCache,
    pub jump: ClipCache,
    pub land: ClipCache,
    pub run: ClipCache,
    #[allow(dead_code)]
    pub sneak: ClipCache,
    #[allow(dead_code)]
    pub sprint: ClipCache,

    // foot-step intervals (seconds)
    pub walk_interval: f32,
    pub run_interval: f32,
}

impl PlayerClipCaches {
    fn new() -> Result<Self> {
        let jump = Self::load_clip_cache("jump", 10)?;
        let land = Self::load_clip_cache("land", 10)?;
        let walk = Self::load_clip_cache("walk", 25)?;
        let sneak = Self::load_clip_cache("sneak", 25)?;
        let run = Self::load_clip_cache("run", 25)?;
        let sprint = Self::load_clip_cache("sprint", 25)?;

        Ok(Self {
            walk,
            jump,
            land,
            sneak,
            run,
            sprint,
            walk_interval: 0.35,
            run_interval: 0.25,
        })
    }

    fn load_clip_cache(sample_name: &str, sample_count: usize) -> Result<ClipCache> {
        let prefix_path =
            "assets/sfx/Footsteps SFX - Undergrowth & Leaves/TomWinandySFX - FS_UndergrowthLeaves_";
        let clip_paths: Vec<String> = (0..sample_count)
            .map(|i| {
                format!(
                    "{}{}_{}.wav",
                    prefix_path,
                    sample_name,
                    format!("{:02}", i + 1)
                )
            })
            .collect();
        let clip_cache = ClipCache::from_files(
            &clip_paths,
            SoundDataConfig {
                // volume: -5.0,
                ..Default::default()
            },
        )?;
        Ok(clip_cache)
    }
}

pub struct PlayerAudioController {
    audio_engine: AudioEngine,
    clip_caches: PlayerClipCaches,
    // time elapsed since last step sound
    time_since_last_step: f32,
}

impl PlayerAudioController {
    pub fn new(audio_engine: AudioEngine) -> Result<Self> {
        let clip_caches = PlayerClipCaches::new()?;
        Ok(Self {
            audio_engine,
            clip_caches,
            time_since_last_step: 0.0,
        })
    }

    pub fn play_jump(&mut self, speed: f32) {
        let clip = self.clip_caches.jump.next();
        let volume = self.calculate_speed_based_volume(speed, 0.5, 2.0);
        self.audio_engine.play_with_volume(&clip, volume).unwrap();
    }

    pub fn play_land(&mut self, speed: f32) {
        let clip = self.clip_caches.land.next();
        let volume = self.calculate_speed_based_volume(speed, 0.7, 1.5);
        self.audio_engine.play_with_volume(&clip, volume).unwrap();
    }

    pub fn play_step(&mut self, is_running: bool, speed: f32) {
        let cache = if is_running {
            &mut self.clip_caches.run
        } else {
            &mut self.clip_caches.walk
        };
        let clip = cache.next();
        let volume = if is_running {
            self.calculate_speed_based_volume(speed, 0.6, 1.0)
        } else {
            self.calculate_speed_based_volume(speed, 0.6, 1.0)
        };
        self.audio_engine.play_with_volume(&clip, volume).unwrap();
    }

    pub fn reset_walk_timer(&mut self) {
        self.time_since_last_step = 0.0;
    }

    fn calculate_speed_based_volume(&self, speed: f32, min_volume: f32, max_volume: f32) -> f32 {
        let max_speed = 3.0;
        let speed_ratio = (speed / max_speed).clamp(0.0, 1.0);
        let volume = min_volume + (max_volume - min_volume) * speed_ratio;
        // in case anything goes wrong
        volume.clamp(0.0, 2.0)
    }

    /// Call this once per frame from the camera update.
    pub fn update_walk_sound(
        &mut self,
        is_on_ground: bool,
        is_moving: bool,
        is_running: bool,
        speed: f32,
        frame_delta_time: f32,
    ) {
        let interval = if is_running {
            self.clip_caches.run_interval
        } else {
            self.clip_caches.walk_interval
        };

        if !(is_on_ground && is_moving) {
            self.time_since_last_step = interval;
            return;
        }

        self.time_since_last_step += frame_delta_time;
        if self.time_since_last_step >= interval {
            let cache = if is_running {
                &mut self.clip_caches.run
            } else {
                &mut self.clip_caches.walk
            };
            let clip = cache.next();
            let volume = if is_running {
                self.calculate_speed_based_volume(speed, 0.8, 1.5)
            } else {
                self.calculate_speed_based_volume(speed, 0.6, 1.2)
            };
            self.audio_engine.play_with_volume(&clip, volume).unwrap();
            self.time_since_last_step = 0.0;
        }
    }
}
