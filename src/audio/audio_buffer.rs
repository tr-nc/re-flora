use anyhow::Result;
use audionimbus::{AudioBuffer as AudionimbusAudioBuffer, Context};

pub struct AudioBuffer {
    data: Vec<f32>,
    buffer: AudionimbusAudioBuffer<Vec<f32>>,
    context: Context,
}

impl AudioBuffer {
    pub fn new(context: Context, frame_size: usize, num_channels: usize) -> Result<Self> {
        let mut data = vec![0.0; frame_size * num_channels];

        let num_samples = data.len() / num_channels;

        let mut channel_ptrs = Vec::with_capacity(num_channels);
        let base_ptr = data.as_mut_ptr();
        for ch in 0..num_channels {
            let ptr = unsafe { base_ptr.add(ch * num_samples) };
            channel_ptrs.push(ptr);
        }

        let buffer = unsafe {
            AudionimbusAudioBuffer::<Vec<f32>>::try_new(channel_ptrs, num_samples)
                .map_err(|e| anyhow::anyhow!("Failed to create AudioBuffer: {:?}", e))?
        };

        Ok(Self {
            data,
            buffer,
            context,
        })
    }

    pub fn as_raw(&self) -> &AudionimbusAudioBuffer<Vec<f32>> {
        &self.buffer
    }

    pub fn get_data(&self) -> &[f32] {
        &self.data
    }

    pub fn set_data(&mut self, data: &[f32]) -> Result<()> {
        if data.len() > self.data.len() {
            return Err(anyhow::anyhow!(
                "Input data size ({}) exceeds buffer capacity ({})",
                data.len(),
                self.data.len()
            ));
        }

        // Copy data into our internal buffer
        self.data[..data.len()].copy_from_slice(data);

        // Fill remaining space with zeros if input is smaller
        if data.len() < self.data.len() {
            self.data[data.len()..].fill(0.0);
        }

        Ok(())
    }

    pub fn to_interleaved(&self) -> Vec<f32> {
        let mut output = vec![0.0; self.buffer.num_samples() * self.buffer.num_channels()];
        self.buffer.interleave(&self.context, &mut output);
        output
    }
}

// SAFETY: AudioBuffer is safe to send between threads because:
// - The raw pointers point to our owned Vec<f32> data
// - The Vec<f32> itself is Send + Sync
// - We never access the raw pointers from different threads simultaneously
unsafe impl Send for AudioBuffer {}
unsafe impl Sync for AudioBuffer {}
