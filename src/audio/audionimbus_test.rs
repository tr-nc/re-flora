use crate::audio::audio_decoder::get_audio_data;
use anyhow::Result;
use glam::Vec3;
use rand::Rng;
use std::collections::HashMap;
use uuid::Uuid;

pub struct SpatialSoundSource {
    position: Vec3,
    volume: f32,
    samples: Vec<f32>,
    sample_rate: u32,
    number_of_frames: usize,
}

impl SpatialSoundSource {
    pub fn new(path: &str, volume: f32, position: Vec3) -> Result<Self> {
        let (samples, sample_rate, number_of_frames) = get_audio_data(path)
            .map_err(|e| anyhow::anyhow!("Failed to load audio file: {}", e))?;

        Ok(Self {
            position,
            volume,
            samples,
            sample_rate,
            number_of_frames,
        })
    }
}

pub struct SpatialSoundManager {
    sources: HashMap<Uuid, SpatialSoundSource>,
}

impl SpatialSoundManager {
    pub fn new() -> Self {
        todo!();
    }

    pub fn add_source(&mut self, source: SpatialSoundSource) -> Uuid {
        let mut id = Uuid::new_v4();
        while self.sources.contains_key(&id) {
            log::warn!("Source with this UUID already exists, generating new UUID");
            id = Uuid::new_v4();
        }
        self.sources.insert(id, source);
        id
    }

    pub fn get_source(&self, id: Uuid) -> Option<&SpatialSoundSource> {
        self.sources.get(&id)
    }

    pub fn remove_source(&mut self, id: Uuid) {
        self.sources.remove(&id);
    }
}

#[test]
fn testing() {}
