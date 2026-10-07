const FDN_LINE_COUNT: usize = 8;
const MAX_PRE_DELAY_SECONDS: f32 = 0.25;
const PARAMETER_SMOOTHING_SECONDS: f32 = 0.05;
const QUIET_THRESHOLD: f32 = 1.0e-7;
const QUIET_BLOCKS_BEFORE_RESET: u8 = 8;
const DELAY_SECONDS: [f32; FDN_LINE_COUNT] = [
    0.0297, 0.0331, 0.0371, 0.0411, 0.0437, 0.0479, 0.0533, 0.0593,
];
const INPUT_SIGNS: [f32; FDN_LINE_COUNT] = [1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0];
const LEFT_SIGNS: [f32; FDN_LINE_COUNT] = [1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
const RIGHT_SIGNS: [f32; FDN_LINE_COUNT] = [1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LateReverbParameters {
    pub pre_delay_seconds: f32,
    pub rt60_seconds: [f32; 3],
    pub wet_gain: f32,
}

impl LateReverbParameters {
    pub(crate) const SILENT: Self = Self {
        pre_delay_seconds: 0.0,
        rt60_seconds: [0.5; 3],
        wet_gain: 0.0,
    };

    fn sanitized(self) -> Self {
        Self {
            pre_delay_seconds: finite_or(self.pre_delay_seconds, 0.0)
                .clamp(0.0, MAX_PRE_DELAY_SECONDS),
            rt60_seconds: self
                .rt60_seconds
                .map(|rt60| finite_or(rt60, 0.5).clamp(0.05, 20.0)),
            wet_gain: finite_or(self.wet_gain, 0.0).clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ThreeBandDampingState {
    low: f32,
    low_mid: f32,
}

/// Shared listener-centric, three-band feedback delay network.
///
/// All storage is allocated during construction. Stable processing performs a fixed amount of
/// work per active sample and never allocates, blocks, or queries scene geometry.
pub(crate) struct ThreeBandFdn {
    sample_rate: f32,
    delay_lines: [Vec<f32>; FDN_LINE_COUNT],
    delay_indices: [usize; FDN_LINE_COUNT],
    damping_states: [ThreeBandDampingState; FDN_LINE_COUNT],
    pre_delay_line: Vec<f32>,
    pre_delay_index: usize,
    low_coefficient: f32,
    low_mid_coefficient: f32,
    current_pre_delay_samples: f32,
    target_pre_delay_samples: f32,
    current_feedback_gains: [[f32; 3]; FDN_LINE_COUNT],
    target_feedback_gains: [[f32; 3]; FDN_LINE_COUNT],
    configured_parameters: LateReverbParameters,
    configured_wet_gain: f32,
    current_wet_gain: f32,
    target_wet_gain: f32,
    smoothing_remaining: usize,
    enabled: bool,
    has_energy: bool,
    quiet_blocks: u8,
    cumulative_input_energy: f64,
    cumulative_output_energy: f64,
}

impl ThreeBandFdn {
    pub(crate) fn new(sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(1) as f32;
        let delay_lines = std::array::from_fn(|index| {
            let samples = (DELAY_SECONDS[index] * sample_rate).round().max(1.0) as usize;
            vec![0.0; samples]
        });
        let max_pre_delay_samples = (MAX_PRE_DELAY_SECONDS * sample_rate).ceil() as usize + 2;
        let initial = LateReverbParameters::SILENT;
        let initial_feedback = feedback_gains(sample_rate, initial.rt60_seconds);
        Self {
            sample_rate,
            delay_lines,
            delay_indices: [0; FDN_LINE_COUNT],
            damping_states: [ThreeBandDampingState::default(); FDN_LINE_COUNT],
            pre_delay_line: vec![0.0; max_pre_delay_samples],
            pre_delay_index: 0,
            low_coefficient: one_pole_coefficient(400.0, sample_rate),
            low_mid_coefficient: one_pole_coefficient(4_000.0, sample_rate),
            current_pre_delay_samples: 0.0,
            target_pre_delay_samples: 0.0,
            current_feedback_gains: initial_feedback,
            target_feedback_gains: initial_feedback,
            configured_parameters: initial,
            configured_wet_gain: 0.0,
            current_wet_gain: 0.0,
            target_wet_gain: 0.0,
            smoothing_remaining: 0,
            enabled: true,
            has_energy: false,
            quiet_blocks: 0,
            cumulative_input_energy: 0.0,
            cumulative_output_energy: 0.0,
        }
    }

    pub(crate) fn set_parameters(&mut self, parameters: LateReverbParameters) {
        let parameters = parameters.sanitized();
        self.configured_parameters = parameters;
        self.target_pre_delay_samples = parameters.pre_delay_seconds * self.sample_rate;
        self.target_feedback_gains = feedback_gains(self.sample_rate, parameters.rt60_seconds);
        self.configured_wet_gain = parameters.wet_gain;
        self.target_wet_gain = if self.enabled {
            self.configured_wet_gain
        } else {
            0.0
        };
        self.begin_smoothing();
    }

    pub(crate) fn needs_processing(&self) -> bool {
        self.has_energy || self.smoothing_remaining > 0
    }

    pub(crate) fn telemetry(&self) -> (LateReverbParameters, f64, f64) {
        (
            self.configured_parameters,
            self.cumulative_input_energy,
            self.cumulative_output_energy,
        )
    }

    pub(crate) fn process_block(
        &mut self,
        input: &[f32],
        output_interleaved_stereo: &mut [f32],
        enabled: bool,
    ) {
        debug_assert!(output_interleaved_stereo.len() >= input.len() * 2);
        if enabled != self.enabled {
            self.enabled = enabled;
            self.target_wet_gain = if enabled {
                self.configured_wet_gain
            } else {
                0.0
            };
            self.begin_smoothing();
        }

        let has_input = enabled && input.iter().any(|sample| sample.abs() > QUIET_THRESHOLD);
        if has_input {
            self.has_energy = true;
        }
        if !self.has_energy
            && self.smoothing_remaining == 0
            && self.current_wet_gain <= QUIET_THRESHOLD
            && self.target_wet_gain <= QUIET_THRESHOLD
        {
            return;
        }

        let mut block_peak = 0.0f32;
        for (frame, input_sample) in input.iter().copied().enumerate() {
            self.advance_smoothed_parameters();
            let injection = if enabled { input_sample } else { 0.0 };
            self.cumulative_input_energy += f64::from(injection) * f64::from(injection);
            let delayed_input = self.process_pre_delay(injection);

            let outputs: [f32; FDN_LINE_COUNT] =
                std::array::from_fn(|line| self.delay_lines[line][self.delay_indices[line]]);
            let sum = outputs.iter().sum::<f32>();
            let mut left = 0.0;
            let mut right = 0.0;

            for line in 0..FDN_LINE_COUNT {
                left += outputs[line] * LEFT_SIGNS[line];
                right += outputs[line] * RIGHT_SIGNS[line];

                let householder = outputs[line] - 0.25 * sum;
                let bands = split_three_bands(
                    householder,
                    &mut self.damping_states[line],
                    self.low_coefficient,
                    self.low_mid_coefficient,
                );
                let feedback = bands[0] * self.current_feedback_gains[line][0]
                    + bands[1] * self.current_feedback_gains[line][1]
                    + bands[2] * self.current_feedback_gains[line][2];
                self.delay_lines[line][self.delay_indices[line]] =
                    feedback + delayed_input * INPUT_SIGNS[line] * 0.25;
                self.delay_indices[line] =
                    (self.delay_indices[line] + 1) % self.delay_lines[line].len();
            }

            let normalization = 1.0 / (FDN_LINE_COUNT as f32).sqrt();
            let wet = self.current_wet_gain * normalization;
            let wet_left = left * wet;
            let wet_right = right * wet;
            output_interleaved_stereo[frame * 2] += wet_left;
            output_interleaved_stereo[frame * 2 + 1] += wet_right;
            self.cumulative_output_energy += f64::from(wet_left) * f64::from(wet_left)
                + f64::from(wet_right) * f64::from(wet_right);
            block_peak = block_peak.max(left.abs()).max(right.abs());
        }

        if has_input || block_peak > QUIET_THRESHOLD {
            self.quiet_blocks = 0;
            self.has_energy = true;
        } else {
            self.quiet_blocks = self.quiet_blocks.saturating_add(1);
            if self.quiet_blocks >= QUIET_BLOCKS_BEFORE_RESET {
                self.clear_state();
            }
        }
    }

    fn begin_smoothing(&mut self) {
        self.smoothing_remaining = (PARAMETER_SMOOTHING_SECONDS * self.sample_rate)
            .round()
            .max(1.0) as usize;
    }

    fn advance_smoothed_parameters(&mut self) {
        if self.smoothing_remaining == 0 {
            return;
        }
        let remaining = self.smoothing_remaining as f32;
        self.current_pre_delay_samples +=
            (self.target_pre_delay_samples - self.current_pre_delay_samples) / remaining;
        self.current_wet_gain += (self.target_wet_gain - self.current_wet_gain) / remaining;
        for line in 0..FDN_LINE_COUNT {
            for band in 0..3 {
                self.current_feedback_gains[line][band] += (self.target_feedback_gains[line][band]
                    - self.current_feedback_gains[line][band])
                    / remaining;
            }
        }
        self.smoothing_remaining -= 1;
    }

    fn process_pre_delay(&mut self, input: f32) -> f32 {
        self.pre_delay_line[self.pre_delay_index] = input;
        let length = self.pre_delay_line.len();
        let delay = self
            .current_pre_delay_samples
            .clamp(0.0, (length - 2) as f32);
        let integer_delay = delay.floor() as usize;
        let fraction = delay - integer_delay as f32;
        let first = (self.pre_delay_index + length - integer_delay) % length;
        let second = (first + length - 1) % length;
        let output =
            self.pre_delay_line[first] * (1.0 - fraction) + self.pre_delay_line[second] * fraction;
        self.pre_delay_index = (self.pre_delay_index + 1) % length;
        output
    }

    fn clear_state(&mut self) {
        for line in &mut self.delay_lines {
            line.fill(0.0);
        }
        self.pre_delay_line.fill(0.0);
        self.damping_states.fill(ThreeBandDampingState::default());
        self.has_energy = false;
        self.quiet_blocks = 0;
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn one_pole_coefficient(cutoff_hz: f32, sample_rate: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * cutoff_hz / sample_rate).exp()
}

fn feedback_gains(sample_rate: f32, rt60_seconds: [f32; 3]) -> [[f32; 3]; FDN_LINE_COUNT] {
    std::array::from_fn(|line| {
        let delay_seconds = ((DELAY_SECONDS[line] * sample_rate).round().max(1.0)) / sample_rate;
        std::array::from_fn(|band| 10.0f32.powf(-3.0 * delay_seconds / rt60_seconds[band]))
    })
}

fn split_three_bands(
    input: f32,
    state: &mut ThreeBandDampingState,
    low_coefficient: f32,
    low_mid_coefficient: f32,
) -> [f32; 3] {
    state.low += low_coefficient * (input - state.low);
    state.low_mid += low_mid_coefficient * (input - state.low_mid);
    [state.low, state.low_mid - state.low, input - state.low_mid]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    #[test]
    fn three_band_feedback_uses_independent_rt60_values() {
        let gains = feedback_gains(48_000.0, [0.2, 1.0, 3.0]);
        for line in gains {
            assert!(line[0] < line[1]);
            assert!(line[1] < line[2]);
            assert!(line.iter().all(|gain| (0.0..1.0).contains(gain)));
        }
    }

    #[test]
    fn silent_network_stays_bit_silent_and_finite() {
        let mut reverb = ThreeBandFdn::new(48_000);
        reverb.set_parameters(LateReverbParameters {
            wet_gain: 0.5,
            ..LateReverbParameters::SILENT
        });
        let input = [0.0; 256];
        let mut output = [0.0; 512];
        for _ in 0..16 {
            reverb.process_block(&input, &mut output, true);
        }
        assert!(
            output
                .iter()
                .all(|sample| *sample == 0.0 && sample.is_finite())
        );
    }

    #[test]
    fn impulse_produces_a_decaying_stereo_tail_without_instability() {
        let mut reverb = ThreeBandFdn::new(8_000);
        reverb.set_parameters(LateReverbParameters {
            pre_delay_seconds: 0.01,
            rt60_seconds: [0.3, 0.6, 1.2],
            wet_gain: 0.5,
        });
        let mut input = [0.0; 128];
        let mut output = [0.0; 256];
        input[0] = 1.0;
        let mut early_energy = 0.0;
        let mut late_energy = 0.0;
        for block in 0..100 {
            output.fill(0.0);
            reverb.process_block(&input, &mut output, true);
            input.fill(0.0);
            let energy = output.iter().map(|sample| sample * sample).sum::<f32>();
            if block < 25 {
                early_energy += energy;
            } else {
                late_energy += energy;
            }
            assert!(output.iter().all(|sample| sample.is_finite()));
        }
        assert!(early_energy > 0.0);
        assert!(late_energy > 0.0);
        assert!(late_energy < early_energy);
        let (parameters, cumulative_input_energy, cumulative_output_energy) = reverb.telemetry();
        assert_eq!(parameters.pre_delay_seconds, 0.01);
        assert_eq!(parameters.rt60_seconds, [0.3, 0.6, 1.2]);
        assert_eq!(parameters.wet_gain, 0.5);
        assert_eq!(cumulative_input_energy, 1.0);
        assert!(cumulative_output_energy > 0.0);
    }

    #[test]
    fn runtime_disable_smoothly_mutes_the_wet_output() {
        let mut reverb = ThreeBandFdn::new(8_000);
        reverb.set_parameters(LateReverbParameters {
            pre_delay_seconds: 0.0,
            rt60_seconds: [1.0; 3],
            wet_gain: 0.8,
        });
        let mut input = [1.0; 128];
        let mut output = [0.0; 256];
        for _ in 0..20 {
            reverb.process_block(&input, &mut output, true);
            input.fill(0.0);
            output.fill(0.0);
        }
        for _ in 0..8 {
            reverb.process_block(&input, &mut output, false);
            output.fill(0.0);
        }
        assert_eq!(reverb.current_wet_gain, 0.0);
        assert!(reverb.has_energy);
    }

    #[test]
    #[ignore = "release-mode performance probe"]
    fn active_three_band_fdn_release_budget() {
        const SAMPLE_RATE: u32 = 48_000;
        const FRAMES: usize = 1_024;
        const BLOCKS: usize = 2_000;
        let mut reverb = ThreeBandFdn::new(SAMPLE_RATE);
        reverb.set_parameters(LateReverbParameters {
            pre_delay_seconds: 0.02,
            rt60_seconds: [0.8, 1.4, 0.9],
            wet_gain: 0.35,
        });
        let input = [0.1; FRAMES];
        let mut output = [0.0; FRAMES * 2];
        for _ in 0..64 {
            reverb.process_block(black_box(&input), black_box(&mut output), true);
        }

        let started = Instant::now();
        for _ in 0..BLOCKS {
            output.fill(0.0);
            reverb.process_block(black_box(&input), black_box(&mut output), true);
        }
        let elapsed = started.elapsed();
        let audio_seconds = FRAMES as f64 * BLOCKS as f64 / SAMPLE_RATE as f64;
        let realtime_cpu_percent = elapsed.as_secs_f64() / audio_seconds * 100.0;
        println!(
            "active three-band FDN: blocks={BLOCKS} frames={FRAMES} elapsed_ms={:.3} us_per_block={:.3} realtime_cpu_percent={realtime_cpu_percent:.3}",
            elapsed.as_secs_f64() * 1_000.0,
            elapsed.as_secs_f64() * 1_000_000.0 / BLOCKS as f64,
        );
        assert!(output.iter().all(|sample| sample.is_finite()));
    }
}
