//! Authoritative file-backed SFX declarations, shared by playback and preloading.
//! Importing a WAV into assets/sfx does not opt it into the running game.

pub(crate) const TERRAIN_EDIT_LOOP_PATH: &str =
    "assets/sfx/ROCKMisc_Designed Rock Movement Loop A_SARM_RkBrck_Stereo-Loop.wav";
pub(crate) const ITEM_PANEL_SCROLL_SFX_PATH: &str =
    "assets/sfx/MECHSwtch_Game Boy Advance SP, B Button, On 05_SARM_BTNS.wav";
pub(crate) const CICADA_CLIPS: [&str; 2] = [
    "assets/sfx/summer_cicadas/dog_day_01.wav",
    "assets/sfx/summer_cicadas/linne_01.wav",
];

pub(crate) struct FootstepClipBank {
    pub(crate) walk_paths: Vec<String>,
    pub(crate) jump_paths: Vec<String>,
    pub(crate) land_paths: Vec<String>,
    pub(crate) run_paths: Vec<String>,
}

impl FootstepClipBank {
    pub(crate) fn new() -> Self {
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
}

pub(super) fn paths() -> Vec<String> {
    let bank = FootstepClipBank::new();
    bank.jump_paths
        .into_iter()
        .chain(bank.land_paths)
        .chain(bank.walk_paths)
        .chain(bank.run_paths)
        .chain(CICADA_CLIPS.into_iter().map(str::to_owned))
        .chain([TERRAIN_EDIT_LOOP_PATH, ITEM_PANEL_SCROLL_SFX_PATH].map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::{Component, Path};

    #[test]
    fn runtime_manifest_contains_only_unique_project_relative_wavs() {
        let paths = paths();
        assert_eq!(paths.len(), 74);
        assert_eq!(paths.iter().collect::<HashSet<_>>().len(), paths.len());
        for path in paths {
            assert!(path.starts_with("assets/sfx/"));
            assert!(path.ends_with(".wav"));
            assert!(Path::new(&path)
                .components()
                .all(|component| matches!(component, Component::Normal(_))));
            assert!(!path.contains("original"));
            assert!(!path.contains("sprinkler/"));
            assert!(!path.contains("apple-dirt/"));
            assert!(!path.contains("leaves-breeze/"));
            assert!(!path.contains("leaves-strong-wind/"));
            assert!(!path.contains("tree_sound_48k"));
        }
    }
}
