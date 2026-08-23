use super::SpatialSoundManager;
use crate::gameplay::camera::{FootstepEvent, FootstepKind, Gait};
use anyhow::Result;
use rand::RngExt;

struct FootstepClipBank {
    walk_paths: Vec<String>,
    jump_paths: Vec<String>,
    land_paths: Vec<String>,
    run_paths: Vec<String>,
}

impl FootstepClipBank {
    fn new() -> Self {
        Self {
            jump_paths: Self::clip_paths("jump", 10),
            land_paths: Self::clip_paths("land", 10),
            walk_paths: Self::clip_paths("walk", 25),
            run_paths: Self::clip_paths("run", 25),
        }
    }

    fn clip_paths(sample_name: &str, sample_count: usize) -> Vec<String> {
        let prefix =
            "assets/sfx/Footsteps SFX - Undergrowth & Leaves/TomWinandySFX - FS_UndergrowthLeaves_";
        (1..=sample_count)
            .map(|index| format!("{prefix}{sample_name}_{index:02}.wav"))
            .collect()
    }

    fn random_path(paths: &[String]) -> &str {
        let index = (rand::rng().random::<u32>() as usize) % paths.len();
        &paths[index]
    }

    fn path_for(&self, event: &FootstepEvent) -> &str {
        // The installed bank contains only undergrowth/leaves recordings. Surface remains part of
        // the semantic event, but every surface intentionally falls back to this existing bank.
        match event.kind {
            FootstepKind::Jump => Self::random_path(&self.jump_paths),
            FootstepKind::Land => Self::random_path(&self.land_paths),
            FootstepKind::Stride(Gait::Walk) => Self::random_path(&self.walk_paths),
            FootstepKind::Stride(Gait::Run) => Self::random_path(&self.run_paths),
        }
    }
}

/// Explicit comparison and recovery route for local footsteps while formal spatial routing is
/// unavailable. This adapter is intentionally event-driven and never participates in gameplay
/// cadence or contact generation.
pub struct LocalPlayerFootstepAudio {
    spatial_sound_manager: SpatialSoundManager,
    clip_bank: FootstepClipBank,
    volume_gain_db: f32,
}

impl LocalPlayerFootstepAudio {
    pub fn new(spatial_sound_manager: SpatialSoundManager) -> Self {
        Self {
            spatial_sound_manager,
            clip_bank: FootstepClipBank::new(),
            volume_gain_db: -40.0,
        }
    }

    pub fn set_volume_gain_db(&mut self, volume_gain_db: f32) {
        self.volume_gain_db = volume_gain_db;
    }

    pub fn play_legacy_2d(&self, event: &FootstepEvent) -> Result<()> {
        let (minimum_db, maximum_db) = match event.kind {
            FootstepKind::Jump | FootstepKind::Land => (-6.0, 6.0),
            FootstepKind::Stride(_) => (-4.0, 0.0),
        };
        let speed_ratio = (event.speed_mps / 3.0).clamp(0.0, 1.0);
        let event_gain_db = minimum_db + (maximum_db - minimum_db) * speed_ratio;
        self.spatial_sound_manager.add_non_spatial_source(
            self.clip_bank.path_for(event),
            event_gain_db + self.volume_gain_db,
        )
    }
}
