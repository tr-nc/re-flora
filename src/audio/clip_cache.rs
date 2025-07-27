use crate::audio::{SoundClip, SoundDataConfig};
use anyhow::{anyhow, Result};
use kira::sound::static_sound::StaticSoundData;
use rand::seq::SliceRandom;
use std::sync::Arc;

/// A cache that can hold one or multiple `SoundClip`s.  
/// When multiple clips are stored, each call to `next()` returns a randomly
/// shuffled item with no immediate repeats across shuffle boundaries.
pub struct ClipCache {
    clips: Vec<SoundClip>,
    play_queue: Vec<usize>,
    last_played: Option<usize>,
}

impl ClipCache {
    /// Build a cache from file paths and a common `SoundDataConfig`.
    pub fn from_files<S: AsRef<str>>(clip_paths: &[S], config: SoundDataConfig) -> Result<Self> {
        if clip_paths.is_empty() {
            return Err(anyhow!(
                "Cannot create a ClipCache with an empty list of clip paths."
            ));
        }

        let settings = config.to_settings();
        let mut rng = rand::rng();

        let clips: Vec<SoundClip> = clip_paths
            .iter()
            .map(|path| {
                Ok(SoundClip {
                    data: Arc::new(
                        StaticSoundData::from_file(path.as_ref())?.with_settings(settings.clone()),
                    ),
                })
            })
            .collect::<Result<_>>()?;

        // prepare an initial play queue (may contain only a single index).
        let mut play_queue: Vec<usize> = (0..clips.len()).collect();
        play_queue.shuffle(&mut rng);

        Ok(Self {
            clips,
            play_queue,
            last_played: None,
        })
    }

    /// Get the next clip.  
    /// - If only one clip is cached, always returns that clip.  
    /// - If multiple clips are cached, uses a shuffled queue with no immediate repeats.
    pub fn next(&mut self) -> SoundClip {
        // fast path for a single cached clip.
        if self.clips.len() == 1 {
            return self.clips[0].clone();
        }

        // refill & reshuffle queue when empty.
        if self.play_queue.is_empty() {
            let mut new_queue: Vec<usize> = (0..self.clips.len()).collect();
            let mut rng = rand::rng();
            new_queue.shuffle(&mut rng);

            // avoid repeating the same clip across queue boundaries
            if let (Some(last), true) = (self.last_played, new_queue.len() > 1) {
                if new_queue[0] == last {
                    new_queue.swap(0, 1);
                }
            }

            self.play_queue = new_queue;
        }

        let clip_index = self
            .play_queue
            .pop()
            .expect("ClipCache play_queue unexpectedly empty");
        self.last_played = Some(clip_index);
        self.clips[clip_index].clone()
    }

    /// Number of cached clips.
    #[allow(unused)]
    pub fn len(&self) -> usize {
        self.clips.len()
    }

    /// Whether the cache is empty.
    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }
}
