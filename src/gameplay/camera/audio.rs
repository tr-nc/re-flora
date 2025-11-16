use crate::audio::SpatialSoundManager;
use anyhow::Result;
use glam::Vec3;
use rand::Rng;

pub struct PlayerClipCaches {
    // Store file paths for spatial audio
    pub walk_paths: Vec<String>,
    pub jump_paths: Vec<String>,
    pub land_paths: Vec<String>,
    pub run_paths: Vec<String>,
    #[allow(dead_code)]
    pub sneak_paths: Vec<String>,
    #[allow(dead_code)]
    pub sprint_paths: Vec<String>,

    // foot-step intervals (seconds)
    pub walk_interval: f32,
    pub run_interval: f32,
}

impl PlayerClipCaches {
    fn new() -> Result<Self> {
        let jump_paths = Self::load_clip_paths("jump", 10);
        let land_paths = Self::load_clip_paths("land", 10);
        let walk_paths = Self::load_clip_paths("walk", 25);
        let sneak_paths = Self::load_clip_paths("sneak", 25);
        let run_paths = Self::load_clip_paths("run", 25);
        let sprint_paths = Self::load_clip_paths("sprint", 25);

        Ok(Self {
            walk_paths,
            jump_paths,
            land_paths,
            sneak_paths,
            run_paths,
            sprint_paths,
            walk_interval: 0.35,
            run_interval: 0.25,
        })
    }

    fn load_clip_paths(sample_name: &str, sample_count: usize) -> Vec<String> {
        let prefix_path =
            "assets/sfx/Footsteps SFX - Undergrowth & Leaves/TomWinandySFX - FS_UndergrowthLeaves_";
        (0..sample_count)
            .map(|i| format!("{}{}_{:02}.wav", prefix_path, sample_name, i + 1))
            .collect()
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
    spatial_sound_manager: SpatialSoundManager,
    clip_caches: PlayerClipCaches,
    // time elapsed since last step sound
    time_since_last_step: f32,
    volume_gain: f32,
}

impl PlayerAudioController {
    pub fn new(spatial_sound_manager: SpatialSoundManager) -> Result<Self> {
        let clip_caches = PlayerClipCaches::new()?;
        Ok(Self {
            spatial_sound_manager,
            clip_caches,
            time_since_last_step: 0.0,
            volume_gain: 0.0,
        })
    }

    fn play_footstep(&self, clip_path: &str, volume: f32) -> Result<()> {
        log::debug!(
            "Playing footstep: {} (volume: {})",
            clip_path,
            volume + self.volume_gain
        );
        self.spatial_sound_manager
            .add_non_spatial_source(clip_path, volume + self.volume_gain)?;
        Ok(())
    }

    pub fn set_footstep_volume_gain(&mut self, volume_gain: f32) {
        self.volume_gain = volume_gain;
    }

    pub fn play_jumping(&mut self, speed: f32, _position: Vec3) {
        let volume = self.calculate_speed_based_volume(speed, -6.0, 6.0);
        let path = PlayerClipCaches::get_random_path(&self.clip_caches.jump_paths);
        if let Err(e) = self.play_footstep(path, volume) {
            log::error!("Failed to play non-spatial jump sound: {}", e);
        }
    }

    pub fn play_landing(&mut self, speed: f32, _position: Vec3) {
        let volume = self.calculate_speed_based_volume(speed, -6.0, 6.0);
        let path = PlayerClipCaches::get_random_path(&self.clip_caches.land_paths);
        if let Err(e) = self.play_footstep(path, volume) {
            log::error!("Failed to play non-spatial landing sound: {}", e);
        }
    }

    pub fn play_step(&mut self, is_running: bool, speed: f32, _position: Vec3) {
        let volume = self.calculate_speed_based_volume(speed, -4.0, 0.0);
        let paths = if is_running {
            &self.clip_caches.run_paths
        } else {
            &self.clip_caches.walk_paths
        };
        let path = PlayerClipCaches::get_random_path(paths);
        if let Err(e) = self.play_footstep(path, volume) {
            log::error!("Failed to play non-spatial step sound: {}", e);
        }
    }

    pub fn reset_walk_timer(&mut self) {
        self.time_since_last_step = 0.0;
    }

    fn calculate_speed_based_volume(&self, speed: f32, min_volume: f32, max_volume: f32) -> f32 {
        let max_speed = 3.0;
        let speed_ratio = (speed / max_speed).clamp(0.0, 1.0);
        min_volume + (max_volume - min_volume) * speed_ratio
    }

    /// Call this once per frame from the camera update.
    pub fn update_walk_sound(
        &mut self,
        is_on_ground: bool,
        is_moving: bool,
        is_running: bool,
        speed: f32,
        frame_delta_time: f32,
        _position: Vec3,
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
            let volume = self.calculate_speed_based_volume(speed, -4.0, 0.0);
            let paths = if is_running {
                &self.clip_caches.run_paths
            } else {
                &self.clip_caches.walk_paths
            };
            let path = PlayerClipCaches::get_random_path(paths);
            if let Err(e) = self.play_footstep(path, volume) {
                log::error!("Failed to play non-spatial walk sound: {}", e);
            }
            self.time_since_last_step = 0.0;
        }
    }
}
