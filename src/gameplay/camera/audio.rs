use crate::audio::{AudioEngine, ClipCache, SoundDataConfig, SpatialSoundManager};
use anyhow::Result;
use glam::Vec3;
use rand::Rng;

pub struct PlayerClipCaches {
    pub walk: ClipCache,
    pub jump: ClipCache,
    pub land: ClipCache,
    pub run: ClipCache,
    #[allow(dead_code)]
    pub sneak: ClipCache,
    #[allow(dead_code)]
    pub sprint: ClipCache,

    // Store file paths for spatial audio
    pub walk_paths: Vec<String>,
    pub jump_paths: Vec<String>,
    pub land_paths: Vec<String>,
    pub run_paths: Vec<String>,
    pub sneak_paths: Vec<String>,
    pub sprint_paths: Vec<String>,

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
            walk: walk.0,
            jump: jump.0,
            land: land.0,
            sneak: sneak.0,
            run: run.0,
            sprint: sprint.0,
            walk_paths: walk.1,
            jump_paths: jump.1,
            land_paths: land.1,
            sneak_paths: sneak.1,
            run_paths: run.1,
            sprint_paths: sprint.1,
            walk_interval: 0.35,
            run_interval: 0.25,
        })
    }

    fn load_clip_cache(sample_name: &str, sample_count: usize) -> Result<(ClipCache, Vec<String>)> {
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
        Ok((clip_cache, clip_paths))
    }

    fn get_random_path(paths: &[String]) -> &str {
        if paths.is_empty() {
            panic!("Cannot choose from empty path list");
        }
        let mut rng = rand::rng();
        let index = (rng.random::<u32>() as usize) % paths.len();
        &paths[index]
    }
}

pub struct PlayerAudioController {
    audio_engine: AudioEngine,
    spatial_sound_manager: Option<SpatialSoundManager>,
    clip_caches: PlayerClipCaches,
    // time elapsed since last step sound
    time_since_last_step: f32,

    volume_multiplier: f32,
}

impl PlayerAudioController {
    pub fn new(
        audio_engine: AudioEngine,
        spatial_sound_manager: Option<SpatialSoundManager>,
    ) -> Result<Self> {
        let clip_caches = PlayerClipCaches::new()?;
        Ok(Self {
            audio_engine,
            spatial_sound_manager,
            clip_caches,
            time_since_last_step: 0.0,
            volume_multiplier: 1.0,
        })
    }

    fn play_spatial_footstep(&self, clip_path: &str, volume: f32, position: Vec3) -> Result<()> {
        if let Some(ref spatial_manager) = self.spatial_sound_manager {
            spatial_manager.add_single_play_source(
                clip_path,
                volume * self.volume_multiplier,
                position,
            )?;
        } else {
            // Fallback to non-spatial audio if spatial manager is not available
            // This should not happen in normal operation but provides safety
            log::warn!("Spatial sound manager not available, using fallback non-spatial audio");
        }
        Ok(())
    }

    pub fn set_spatial_sound_manager(&mut self, spatial_sound_manager: SpatialSoundManager) {
        self.spatial_sound_manager = Some(spatial_sound_manager);
    }

    pub fn play_jumping(&mut self, speed: f32, position: Vec3) {
        let clip = self.clip_caches.jump.next();
        let volume = self.calculate_speed_based_volume(speed, 0.5, 2.0);
        let path = PlayerClipCaches::get_random_path(&self.clip_caches.jump_paths);
        if let Err(e) = self.play_spatial_footstep(path, volume, position) {
            log::error!("Failed to play spatial jump sound: {}", e);
            // Fallback to regular audio
            self.audio_engine
                .play_with_volume(&clip, volume * self.volume_multiplier)
                .unwrap();
        }
    }

    pub fn play_landing(&mut self, speed: f32, position: Vec3) {
        let clip = self.clip_caches.land.next();
        let volume = self.calculate_speed_based_volume(speed, 0.7, 1.5);
        let path = PlayerClipCaches::get_random_path(&self.clip_caches.land_paths);
        if let Err(e) = self.play_spatial_footstep(path, volume, position) {
            log::error!("Failed to play spatial landing sound: {}", e);
            // Fallback to regular audio
            self.audio_engine
                .play_with_volume(&clip, volume * self.volume_multiplier)
                .unwrap();
        }
    }

    pub fn play_step(&mut self, is_running: bool, speed: f32, position: Vec3) {
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
        let paths = if is_running {
            &self.clip_caches.run_paths
        } else {
            &self.clip_caches.walk_paths
        };
        let path = PlayerClipCaches::get_random_path(paths);
        if let Err(e) = self.play_spatial_footstep(path, volume, position) {
            log::error!("Failed to play spatial step sound: {}", e);
            // Fallback to regular audio
            self.audio_engine
                .play_with_volume(&clip, volume * self.volume_multiplier)
                .unwrap();
        }
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
        position: Vec3,
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
            let paths = if is_running {
                &self.clip_caches.run_paths
            } else {
                &self.clip_caches.walk_paths
            };
            let path = PlayerClipCaches::get_random_path(paths);
            if let Err(e) = self.play_spatial_footstep(path, volume, position) {
                log::error!("Failed to play spatial walk sound: {}", e);
                // Fallback to regular audio
                self.audio_engine
                    .play_with_volume(&clip, volume * self.volume_multiplier)
                    .unwrap();
            }
            self.time_since_last_step = 0.0;
        }
    }
}
