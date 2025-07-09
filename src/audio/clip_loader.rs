use crate::audio::SoundDataConfig;
use anyhow::{anyhow, Result};
use kira::sound::static_sound::StaticSoundData;
use rand::rng;
use rand::seq::SliceRandom;

pub struct ClipLoader {
    /// The master list of all loaded sound clips. This vector is never modified after creation.
    clips: Vec<StaticSoundData>,
    /// A queue of indices pointing to the `clips` vector. This queue is shuffled and clips are
    /// drawn from it.
    play_queue: Vec<usize>,
    /// The most recently played clip index, used to avoid consecutive duplicates across shuffles.
    last_played: Option<usize>,
}

impl ClipLoader {
    pub fn new<S: AsRef<str>>(clip_paths: &[S], config: SoundDataConfig) -> Result<Self> {
        if clip_paths.is_empty() {
            return Err(anyhow!(
                "Cannot create a ClipLoader with an empty list of clip paths."
            ));
        }

        let settings = config.to_settings();
        let mut clips = Vec::new();
        for path in clip_paths {
            // Use .as_ref() to get a &str from the generic type S
            let sound_data =
                StaticSoundData::from_file(path.as_ref())?.with_settings(settings.clone());
            clips.push(sound_data);
        }

        let mut play_queue: Vec<usize> = (0..clips.len()).collect();
        play_queue.shuffle(&mut rng());

        Ok(Self {
            clips,
            play_queue,
            last_played: None,
        })
    }

    pub fn get_next_clip(&mut self) -> StaticSoundData {
        if self.play_queue.is_empty() {
            if self.clips.is_empty() {
                panic!("ClipLoader has no clips to play, even after attempting to reshuffle.");
            }
            let mut new_queue: Vec<usize> = (0..self.clips.len()).collect();
            new_queue.shuffle(&mut rng());

            // avoid repeating the same clip across queue boundaries
            if let (Some(last), true) = (self.last_played, new_queue.len() > 1) {
                if new_queue[0] == last {
                    new_queue.swap(0, 1);
                }
            }

            self.play_queue = new_queue;
        }

        let clip_index = self.play_queue.pop().unwrap();
        self.last_played = Some(clip_index);
        self.clips[clip_index].clone()
    }
}
