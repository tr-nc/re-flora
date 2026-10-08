use crate::error::{PetalSonicError, Result};

pub struct BatchResampler {
    source_sample_rate: u32,
    target_sample_rate: u32,
    channels: u16,
    chunk_size: usize,
}

impl BatchResampler {
    /// Creates a new batch resampler for offline audio processing.
    ///
    /// # Arguments
    /// * `source_sample_rate` - The sample rate of the input audio
    /// * `target_sample_rate` - The desired sample rate of the output audio
    /// * `channels` - Number of channels in the audio data
    /// * `chunk_size` - Optional size of processing chunks (defaults to 1024)
    ///
    /// # Returns
    /// A new `BatchResampler` instance
    pub fn new(
        source_sample_rate: u32,
        target_sample_rate: u32,
        channels: u16,
        chunk_size: Option<usize>,
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

        Ok(Self {
            source_sample_rate,
            target_sample_rate,
            channels,
            chunk_size: chunk_size.unwrap_or(1024),
        })
    }

    /// Resamples a single channel of audio data.
    ///
    /// # Data Format
    /// - **Input**: NON-INTERLEAVED (planar) - Single channel data: `[L0, L1, L2, ...]`
    /// - **Output**: NON-INTERLEAVED (planar) - Single channel data: `[L0, L1, L2, ...]`
    ///
    /// # Arguments
    /// * `channel_samples` - A slice of f32 samples from a single audio channel (NOT interleaved)
    ///
    /// # Returns
    /// A vector of resampled f32 samples for the single channel (NOT interleaved)
    pub fn resample_channel(&self, channel_samples: &[f32]) -> Result<Vec<f32>> {
        if self.source_sample_rate == self.target_sample_rate {
            return Ok(channel_samples.to_vec());
        }

        use rubato::{FftFixedIn, Resampler};

        let mut resampler = FftFixedIn::new(
            self.source_sample_rate as usize,
            self.target_sample_rate as usize,
            self.chunk_size,
            2, // sub_chunks
            1, // single channel
        )
        .map_err(|e| PetalSonicError::AudioLoading(format!("Failed to create resampler: {}", e)))?;

        let mut output_buffer = Vec::new();
        let mut input_index = 0;

        while input_index < channel_samples.len() {
            let remaining_samples = channel_samples.len() - input_index;
            let samples_to_process = remaining_samples.min(self.chunk_size);

            if samples_to_process == 0 {
                break;
            }

            // Pad the chunk to chunk_size if needed
            let mut input_chunk = vec![0.0f32; self.chunk_size];
            let end_index = (input_index + samples_to_process).min(channel_samples.len());
            input_chunk[..samples_to_process]
                .copy_from_slice(&channel_samples[input_index..end_index]);

            let waves_in = vec![input_chunk];
            let waves_out = resampler
                .process(&waves_in, None)
                .map_err(|e| PetalSonicError::AudioLoading(format!("Resampling error: {}", e)))?;

            if let Some(first_channel) = waves_out.first() {
                output_buffer.extend_from_slice(first_channel);
            }

            input_index += samples_to_process;
        }

        Ok(output_buffer)
    }

    /// Resamples multi-channel interleaved audio data.
    ///
    /// # Data Format
    /// - **Input**: INTERLEAVED - Samples from all channels mixed: `[L0, R0, L1, R1, L2, R2, ...]`
    /// - **Output**: INTERLEAVED - Resampled samples from all channels mixed: `[L0, R0, L1, R1, ...]`
    ///
    /// For stereo (2-channel) audio:
    /// - Input:  `[Left0, Right0, Left1, Right1, ...]`
    /// - Output: `[Left0, Right0, Left1, Right1, ...]` (at new sample rate)
    ///
    /// # Arguments
    /// * `interleaved_samples` - A slice of f32 samples with all channels interleaved
    ///
    /// # Returns
    /// A vector of resampled f32 samples with all channels interleaved
    ///
    /// # Implementation Note
    /// This function internally de-interleaves the data, resamples each channel separately,
    /// then re-interleaves the results.
    pub fn resample_interleaved(&self, interleaved_samples: &[f32]) -> Result<Vec<f32>> {
        if self.source_sample_rate == self.target_sample_rate {
            return Ok(interleaved_samples.to_vec());
        }

        // Split into channels
        let mut channel_samples = Vec::new();
        for ch in 0..self.channels as usize {
            let channel_data: Vec<f32> = interleaved_samples
                .chunks(self.channels as usize)
                .map(|frame| frame.get(ch).copied().unwrap_or(0.0))
                .collect();
            channel_samples.push(channel_data);
        }

        // Resample each channel
        let mut resampled_channels = Vec::new();
        for channel_data in &channel_samples {
            let resampled = self.resample_channel(channel_data)?;
            resampled_channels.push(resampled);
        }

        // Interleave the resampled channels
        let mut interleaved_samples = Vec::new();
        let new_frames = resampled_channels[0].len();

        for frame_idx in 0..new_frames {
            for resampled_channel in resampled_channels.iter().take(self.channels as usize) {
                if frame_idx < resampled_channel.len() {
                    interleaved_samples.push(resampled_channel[frame_idx]);
                }
            }
        }

        Ok(interleaved_samples)
    }
}
