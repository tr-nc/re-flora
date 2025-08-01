use anyhow::Result;
use audionimbus::{AudioBuffer as AudionimbusAudioBuffer, AudioBufferSettings, Context};

pub struct AudioBuffer<'a> {
    _data: Vec<f32>,
    buffer: AudionimbusAudioBuffer<&'a mut [f32]>,
    context: Context,
}

impl<'a> AudioBuffer<'a> {
    pub fn new(context: Context, frame_size: usize, num_channels: usize) -> Result<Self> {
        let mut data = vec![0.0; frame_size * num_channels];

        // We need to create the buffer with a reference to our data
        // This requires unsafe code to tie the lifetime to self
        let data_ptr = data.as_mut_ptr();
        let data_len = data.len();
        let data_slice = unsafe { std::slice::from_raw_parts_mut(data_ptr, data_len) };
        let data_slice: &'a mut [f32] = unsafe { std::mem::transmute(data_slice) };

        let buffer = AudionimbusAudioBuffer::try_with_data_and_settings(
            data_slice,
            &AudioBufferSettings {
                num_channels: Some(num_channels),
                ..Default::default()
            },
        )
        .map_err(|e| anyhow::anyhow!("Failed to create AudioBuffer: {:?}", e))?;

        Ok(Self {
            _data: data,
            buffer,
            context,
        })
    }

    pub fn as_raw(&self) -> &AudionimbusAudioBuffer<&'a mut [f32]> {
        &self.buffer
    }

    pub fn set_data(&mut self, data: &[f32]) -> Result<()> {
        if data.len() > self._data.len() {
            return Err(anyhow::anyhow!(
                "Input data size ({}) exceeds buffer capacity ({})",
                data.len(),
                self._data.len()
            ));
        }

        // Copy data into our internal buffer
        self._data[..data.len()].copy_from_slice(data);

        // Fill remaining space with zeros if input is smaller
        if data.len() < self._data.len() {
            self._data[data.len()..].fill(0.0);
        }

        Ok(())
    }

    pub fn to_interleaved(&self) -> Vec<f32> {
        let mut output = vec![0.0; self.buffer.num_samples() * self.buffer.num_channels()];
        self.buffer.interleave(&self.context, &mut output);
        output
    }
}
