//! Internal immutable PCM storage, decoding, and resampling.

mod batch_resampler;
mod decoder;
mod streaming_resampler;

use crate::error::Result;
pub use batch_resampler::BatchResampler;
pub(crate) use decoder::decode_file;
use std::sync::Arc;
use std::time::Duration;
pub use streaming_resampler::{ResamplerType, StreamingResampler};

/// One immutable interleaved PCM allocation shared by Resident Clips and their Voices.
#[derive(Debug)]
pub(crate) struct PetalSonicAudioData {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    total_frames: usize,
}

impl PetalSonicAudioData {
    pub(crate) fn new(
        samples: Vec<f32>,
        sample_rate: u32,
        channels: u16,
        _duration: Duration,
    ) -> Self {
        let total_frames = samples.len() / channels as usize;
        Self {
            samples,
            sample_rate,
            channels,
            total_frames,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn total_frames(&self) -> usize {
        self.total_frames
    }

    /// Reuse this resident allocation at the same rate, or build its one replacement allocation.
    pub fn resample(self: Arc<Self>, target_sample_rate: u32) -> Result<Arc<Self>> {
        if target_sample_rate == self.sample_rate {
            return Ok(self);
        }

        let resampler = BatchResampler::new(
            self.sample_rate,
            target_sample_rate,
            self.channels,
            Some(1024),
        )?;
        let resampled_samples = resampler.resample_interleaved(&self.samples)?;
        let new_duration = Duration::from_secs_f64(
            resampled_samples.len() as f64 / (target_sample_rate * self.channels as u32) as f64,
        );

        Ok(Arc::new(Self::new(
            resampled_samples,
            target_sample_rate,
            self.channels,
            new_duration,
        )))
    }
}
