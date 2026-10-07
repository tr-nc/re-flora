use crate::error::{PetalSonicError, Result};
use rubato::{
    FastFixedIn, PolynomialDegree, Resampler, SincFixedIn, SincInterpolationParameters,
    SincInterpolationType, WindowFunction,
};

/// Type of resampler algorithm to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResamplerType {
    /// Fast polynomial resampler - lower quality but faster
    Fast,
    /// Sinc interpolation resampler - higher quality but slower
    #[default]
    Sinc,
}

enum ResamplerImpl {
    Fast(FastFixedIn<f32>),
    Sinc(SincFixedIn<f32>),
}

impl ResamplerImpl {
    fn process_into_buffer(
        &mut self,
        input: &[Vec<f32>],
        output: &mut [Vec<f32>],
    ) -> std::result::Result<(usize, usize), rubato::ResampleError> {
        match self {
            Self::Fast(r) => r.process_into_buffer(input, output, None),
            Self::Sinc(r) => r.process_into_buffer(input, output, None),
        }
    }

    fn output_buffer_allocate(&self) -> Vec<Vec<f32>> {
        match self {
            Self::Fast(r) => r.output_buffer_allocate(true),
            Self::Sinc(r) => r.output_buffer_allocate(true),
        }
    }
}

/// A real-time streaming resampler that converts audio from one sample rate to another
/// in real-time with minimal latency. Uses a fixed-input-size approach where the world
/// generates a fixed number of frames and the resampler produces variable output based
/// on the sample rate ratio.
pub struct StreamingResampler {
    resampler: ResamplerImpl,
    source_sample_rate: u32,
    target_sample_rate: u32,
    channels: u16,
    input_chunk_size: usize,
    input_waves: Vec<Vec<f32>>,
    output_waves: Vec<Vec<f32>>,
}

impl StreamingResampler {
    /// Creates a new streaming resampler with fixed input size
    ///
    /// # Arguments
    /// * `source_sample_rate` - The sample rate of the audio being produced by the world
    /// * `target_sample_rate` - The sample rate required by the audio device
    /// * `channels` - Number of audio channels
    /// * `input_frames` - The fixed number of frames to generate at world sample rate per chunk
    /// * `resampler_type` - Type of resampler algorithm to use (defaults to Sinc if None)
    ///
    /// # Returns
    /// A new `StreamingResampler` instance configured for real-time processing
    pub fn new(
        source_sample_rate: u32,
        target_sample_rate: u32,
        channels: u16,
        input_frames: usize,
        resampler_type: Option<ResamplerType>,
    ) -> Result<Self> {
        if source_sample_rate == 0 || target_sample_rate == 0 {
            return Err(PetalSonicError::AudioFormat(
                "Sample rates must be greater than 0".to_string(),
            ));
        }

        if channels == 0 {
            return Err(PetalSonicError::AudioFormat(
                "Channel count must be greater than 0".to_string(),
            ));
        }

        if input_frames == 0 {
            return Err(PetalSonicError::AudioFormat(
                "Input frames must be greater than 0".to_string(),
            ));
        }

        // target/source (output/input)
        let resample_ratio = target_sample_rate as f64 / source_sample_rate as f64;
        let resampler_type = resampler_type.unwrap_or_default();

        let resampler = match resampler_type {
            ResamplerType::Fast => {
                let fast = FastFixedIn::new(
                    resample_ratio,
                    1.0, // the ratio's always fixed
                    PolynomialDegree::Septic,
                    input_frames,
                    channels as usize,
                )
                .map_err(|e| {
                    PetalSonicError::AudioLoading(format!("Failed to create fast resampler: {}", e))
                })?;
                ResamplerImpl::Fast(fast)
            }
            ResamplerType::Sinc => {
                let params = SincInterpolationParameters {
                    sinc_len: 256,
                    f_cutoff: 0.95,
                    interpolation: SincInterpolationType::Linear,
                    oversampling_factor: 256,
                    window: WindowFunction::BlackmanHarris2,
                };

                let sinc = SincFixedIn::new(
                    resample_ratio,
                    1.0, // we're not changing it dynamically
                    params,
                    input_frames,
                    channels as usize,
                )
                .map_err(|e| {
                    PetalSonicError::AudioLoading(format!("Failed to create sinc resampler: {}", e))
                })?;
                ResamplerImpl::Sinc(sinc)
            }
        };

        let input_waves = vec![vec![0.0; input_frames]; channels as usize];
        let output_waves = resampler.output_buffer_allocate();

        Ok(Self {
            resampler,
            source_sample_rate,
            target_sample_rate,
            channels,
            input_chunk_size: input_frames,
            input_waves,
            output_waves,
        })
    }

    /// Processes interleaved audio samples and resamples them to the target rate
    ///
    /// # Arguments
    /// * `input_samples` - Interleaved f32 samples at the source sample rate (must be exactly input_chunk_size frames)
    /// * `output_samples` - Interleaved f32 buffer to fill with resampled audio (will be resized as needed)
    ///
    /// # Returns
    /// A tuple of (output_frames_written, input_frames_consumed)
    ///
    /// # Important
    /// - Input must contain exactly `input_chunk_size` frames (input_chunk_size * channels samples)
    /// - Output size will vary based on the resampling ratio
    pub fn process_interleaved(
        &mut self,
        input_samples: &[f32],
        output_samples: &mut [f32],
    ) -> Result<(usize, usize)> {
        let channels = self.channels as usize;
        let input_frames = input_samples.len() / channels;

        if input_frames != self.input_chunk_size {
            return Err(PetalSonicError::AudioFormat(format!(
                "Input size mismatch: expected {} frames, got {} frames",
                self.input_chunk_size, input_frames
            )));
        }

        // Bypass resampling if sample rates are identical
        if self.source_sample_rate == self.target_sample_rate {
            let samples_to_copy = input_samples.len().min(output_samples.len());
            output_samples[..samples_to_copy].copy_from_slice(&input_samples[..samples_to_copy]);
            return Ok((input_frames, input_frames));
        }

        // De-interleave into storage allocated with this output session.
        for frame_idx in 0..input_frames {
            for ch in 0..channels {
                self.input_waves[ch][frame_idx] = input_samples[frame_idx * channels + ch];
            }
        }

        // Rubato's allocating `process` convenience API is intentionally avoided on
        // the render thread. Both planar buffers are reused for every quantum.
        let (input_frames_consumed, output_frames) = self
            .resampler
            .process_into_buffer(&self.input_waves, &mut self.output_waves)
            .map_err(|e| {
                PetalSonicError::AudioLoading(format!("Streaming resampling error: {}", e))
            })?;
        let output_samples_needed = output_frames * channels;

        // Check if output buffer is large enough
        if output_samples.len() < output_samples_needed {
            return Err(PetalSonicError::AudioFormat(format!(
                "Output buffer too small: need {} samples, got {}",
                output_samples_needed,
                output_samples.len()
            )));
        }

        // Re-interleave output
        for frame_idx in 0..output_frames {
            for ch in 0..channels {
                output_samples[frame_idx * channels + ch] = self.output_waves[ch][frame_idx];
            }
        }

        Ok((output_frames, input_frames_consumed))
    }
}
