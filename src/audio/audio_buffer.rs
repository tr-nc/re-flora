use anyhow::Result;

pub struct AudioBuffer {
    data: Vec<f32>,
    num_channels: usize,
    frame_size: usize,
}

impl AudioBuffer {
    pub fn new(frame_size: usize, num_channels: usize) -> Result<Self> {
        let data = vec![0.0; frame_size * num_channels];
        Ok(Self {
            data,
            num_channels,
            frame_size,
        })
    }

    pub fn data(&self) -> &[f32] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [f32] {
        &mut self.data
    }

    pub fn create_audionimbus_buffer(&mut self) -> Result<audionimbus::AudioBuffer<&mut [f32]>> {
        audionimbus::AudioBuffer::try_with_data_and_settings(
            &mut self.data,
            &audionimbus::AudioBufferSettings {
                num_channels: Some(self.num_channels),
                ..Default::default()
            },
        )
    }

    pub fn num_channels(&self) -> usize {
        self.num_channels
    }

    pub fn frame_size(&self) -> usize {
        self.frame_size
    }
}