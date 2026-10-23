use anyhow::{Context, Result};
use petalsonic::ResidentClip;
use std::collections::HashMap;
use std::path::Path;

/// Immutable, preloaded runtime SFX. Playback never scans directories or decodes files.
/// Source recordings and candidates are not loaded unless declared in runtime_assets.
pub struct AudioClipCache {
    clips: HashMap<String, ResidentClip>,
}

impl AudioClipCache {
    /// Preload the runtime manifest. Declared assets remain required: missing or invalid
    /// files return a path-qualified error. Unlisted files have no effect on startup.
    pub fn new() -> Result<Self> {
        let cache = Self::load(
            Path::new(&crate::util::get_project_root()),
            super::runtime_assets::paths(),
        )?;
        log::info!(
            "[AUDIO][ASSETS] loaded={} selection=runtime_manifest directory_scan=false",
            cache.len()
        );
        Ok(cache)
    }

    fn load(project_root: &Path, paths: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut clips = HashMap::new();
        for path in paths {
            let key = path.replace('\\', "/");
            if clips.contains_key(&key) {
                continue;
            }
            let full_path = project_root.join(&key);
            let clip = ResidentClip::from_path(&full_path)
                .with_context(|| format!("Failed to load declared runtime SFX: {key}"))?;
            clips.insert(key, clip);
        }
        Ok(Self { clips })
    }

    /// Look up a project-relative path, e.g. assets/sfx/click.wav.
    pub fn get(&self, path: &str) -> Option<ResidentClip> {
        self.clips.get(&path.replace('\\', "/")).cloned()
    }

    pub fn len(&self) -> usize {
        self.clips.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Tiny PCM16 mono WAV; no audio device or project assets are needed by these tests.
    fn write_clip(root: &Path, path: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&44u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&48_000u32.to_le_bytes());
        wav.extend_from_slice(&96_000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&8u32.to_le_bytes());
        for sample in [0i16, 100, -100, 0] {
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        fs::write(path, wav).unwrap();
    }

    #[test]
    fn loads_runtime_manifest_without_decoding_unlisted_candidates_or_originals() {
        let root = tempfile::tempdir().unwrap();
        let paths = super::super::runtime_assets::paths();
        for path in &paths {
            write_clip(root.path(), path);
        }
        // A stray invalid WAV used to abort startup during the recursive scan.
        let stray = "assets/sfx/candidates/broken-original.wav";
        fs::create_dir_all(root.path().join("assets/sfx/candidates")).unwrap();
        fs::write(root.path().join(stray), b"not a WAV").unwrap();
        let cache = AudioClipCache::load(root.path(), paths.clone()).unwrap();
        assert_eq!(cache.len(), paths.len());
        for path in &paths {
            assert!(cache.get(path).is_some(), "missing {path}");
        }
        assert!(cache.get(stray).is_none());
        let selected = &paths[0];
        assert!(cache.get(&selected.replace('/', "\\")).is_some());
        assert!(cache.get(selected).is_some()); // repeated playback retains the clip
    }

    #[test]
    fn missing_and_corrupt_declared_clips_report_the_relative_path() {
        let root = tempfile::tempdir().unwrap();
        let path = "assets/sfx/required.wav";
        for corrupt in [false, true] {
            if corrupt {
                fs::create_dir_all(root.path().join("assets/sfx")).unwrap();
                fs::write(root.path().join(path), b"invalid WAV").unwrap();
            }
            let error = AudioClipCache::load(root.path(), [path.to_owned()])
                .err()
                .expect("declared clip must fail to load");
            assert!(error.to_string().contains(path));
        }
    }

    #[test]
    fn empty_selection_needs_no_asset_directory_and_duplicates_are_deduplicated() {
        let root = tempfile::tempdir().unwrap();
        assert!(AudioClipCache::load(root.path(), []).unwrap().is_empty());
        let path = "assets/sfx/selected.wav";
        write_clip(root.path(), path);
        let cache =
            AudioClipCache::load(root.path(), [path.to_owned(), path.replace('/', "\\")]).unwrap();
        assert_eq!(cache.len(), 1);
    }
}
