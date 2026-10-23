//! Two equal-duration resident layers, one playback cursor and one acoustic Voice.
//! Offsets stay fixed through looping, seeking, rate changes and virtual/silent advancement.
use crate::audio_data::PetalSonicAudioData;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

/// Latest blend position: 0 = primary only, 1 = secondary only.
/// Each source owns its own control. Publication never restarts either layer.
#[derive(Clone, Debug)]
pub struct LoopBlendControl(Arc<AtomicU32>);

impl Default for LoopBlendControl {
    fn default() -> Self {
        Self(Arc::new(AtomicU32::new(0.0f32.to_bits())))
    }
}

impl LoopBlendControl {
    pub fn set(&self, position: f32) {
        let position = if position.is_finite() {
            position.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.0.store(position.to_bits(), Ordering::Relaxed);
    }

    pub fn position(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LoopBlend {
    pub data: Arc<PetalSonicAudioData>,
    pub phase_offset: f32,
    pub control: LoopBlendControl,
}

impl LoopBlend {
    pub(crate) fn resampled_to(mut self, sample_rate: u32) -> crate::error::Result<Self> {
        self.data = self.data.resample(sample_rate)?;
        Ok(self)
    }
}

#[derive(Debug)]
pub(crate) struct LoopBlendRenderer {
    blend: LoopBlend,
    offset_frames: usize,
    position: f32,
    target: f32,
}

impl LoopBlendRenderer {
    pub(crate) fn new(blend: LoopBlend) -> Self {
        let position = blend.control.position();
        let offset_frames = (blend.phase_offset as f64 * blend.data.total_frames() as f64) as usize;
        Self {
            blend,
            offset_frames,
            position,
            target: position,
        }
    }

    pub(crate) fn begin_block(&mut self) {
        self.target = self.blend.control.position();
    }

    pub(crate) fn mix(&mut self, primary: f32, frame: usize, sample_rate: u32) -> f32 {
        // Bound control discontinuities to a 20 ms full-range ramp; no callback locks/allocations.
        let step = 1.0 / (0.020 * sample_rate as f32);
        self.position += (self.target - self.position).clamp(-step, step);
        let secondary_frame = (frame + self.offset_frames) % self.blend.data.total_frames();
        let secondary = mono_frame(&self.blend.data, secondary_frame);
        let (a, b) = equal_power_gains(self.position);
        primary * a + secondary * b
    }
}

pub(crate) fn mono_frame(data: &PetalSonicAudioData, frame: usize) -> f32 {
    let channels = data.channels().max(1) as usize;
    let start = frame * channels;
    data.samples()[start..start + channels].iter().sum::<f32>() / channels as f32
}

fn equal_power_gains(position: f32) -> (f32, f32) {
    ((1.0 - position).sqrt(), position.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_control_is_bounded_and_equal_power_has_exact_endpoints() {
        let control = LoopBlendControl::default();
        for (value, expected) in [(f32::NAN, 0.0), (-1.0, 0.0), (2.0, 1.0), (0.4, 0.4)] {
            control.set(value);
            assert_eq!(control.position(), expected);
        }
        assert_eq!(equal_power_gains(0.0), (1.0, 0.0));
        assert_eq!(equal_power_gains(1.0), (0.0, 1.0));
        for i in 0..=100 {
            let (a, b) = equal_power_gains(i as f32 / 100.0);
            assert!((a * a + b * b - 1.0).abs() < 2e-7);
        }
    }
}
