use kira::sound::static_sound::StaticSoundData;
use std::sync::Arc;

#[derive(Clone)]
pub struct SoundClip {
    pub data: Arc<StaticSoundData>,
}

impl SoundClip {
    pub fn as_kira(&self) -> &StaticSoundData {
        &self.data
    }
}
