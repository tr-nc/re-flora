use anyhow::Result;
use petalsonic::audio_data::PetalSonicAudioData;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// Cache for pre-loaded audio clips to avoid redundant file I/O.
///
/// This cache loads all audio files from the assets/sfx directory at initialization
/// and provides O(1) lookup by full path. This is crucial for performance when the
/// same audio clip (e.g., tree_sound_48k.wav) needs to be instantiated thousands of times.
pub struct AudioClipCache {
    clips: HashMap<String, Arc<PetalSonicAudioData>>,
}

impl AudioClipCache {
    /// Creates a new AudioClipCache and pre-loads all audio files from assets/sfx.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The assets/sfx directory cannot be read
    /// - Any audio file fails to load
    pub fn new() -> Result<Self> {
        let mut clips = HashMap::new();

        // Construct the path to assets/sfx
        let project_root = crate::util::get_project_root();
        let sfx_dir = format!("{}assets/sfx", project_root);
        let sfx_path = Path::new(&sfx_dir);

        // Check if directory exists
        if !sfx_path.exists() {
            return Err(anyhow::anyhow!(
                "Audio directory does not exist: {}",
                sfx_dir
            ));
        }

        // Recursively load all .wav files
        Self::load_wav_files_recursive(&mut clips, sfx_path, &project_root)?;

        println!("AudioClipCache initialized with {} clips", clips.len());

        Ok(Self { clips })
    }

    /// Recursively loads all .wav files from a directory
    fn load_wav_files_recursive(
        clips: &mut HashMap<String, Arc<PetalSonicAudioData>>,
        dir: &Path,
        project_root: &str,
    ) -> Result<()> {
        let entries = fs::read_dir(dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Recursively process subdirectories
                Self::load_wav_files_recursive(clips, &path, project_root)?;
            } else if path.is_file() && path.extension().is_some_and(|ext| ext == "wav") {
                // Process .wav files
                let full_path_str = path.to_str().ok_or_else(|| {
                    anyhow::anyhow!("Failed to convert path to string: {:?}", path)
                })?;

                // Normalize path separators
                let normalized_full_path = full_path_str.replace('\\', "/");
                let normalized_root = project_root.replace('\\', "/");

                // Strip the project root to get the relative path
                let relative_path =
                    if let Some(rel) = normalized_full_path.strip_prefix(&normalized_root) {
                        rel.to_string()
                    } else {
                        return Err(anyhow::anyhow!(
                            "Path {} is not under project root {}",
                            normalized_full_path,
                            normalized_root
                        ));
                    };

                // Load the audio data
                let audio_data = PetalSonicAudioData::from_path(&normalized_full_path)?;

                println!("Cached audio clip: {}", relative_path);
                clips.insert(relative_path, audio_data);
            }
        }

        Ok(())
    }

    /// Gets a cached audio clip by its full path.
    ///
    /// # Arguments
    /// * `path` - The full path to the audio file (e.g., "assets/sfx/tree_sound_48k.wav")
    ///
    /// # Returns
    /// Some(Arc<PetalSonicAudioData>) if the clip is cached, None otherwise
    pub fn get(&self, path: &str) -> Option<Arc<PetalSonicAudioData>> {
        // Normalize the input path to match cached paths
        let normalized_path = path.replace('\\', "/");
        self.clips.get(&normalized_path).cloned()
    }

    /// Returns the number of cached audio clips
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.clips.len()
    }

    /// Returns true if the cache is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }
}
