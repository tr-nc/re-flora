use crate::error::{PetalSonicError, Result};
use crate::math::Vec3;
use crate::spatial::native_hrtf::{NativeHrtfRenderMetrics, NativeHrtfTable};
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex, num_complex::Complex32};
use std::f32::consts::PI;
use std::sync::Arc;
use std::time::Instant;

/// Native Ambisonics order used by re-flora for high-quality Native Ambisonics.
pub const DEFAULT_NATIVE_AMBISONICS_ORDER: u32 = 4;
const MAX_NATIVE_AMBISONICS_ORDER: u32 = 4;
const MAX_NATIVE_AMBISONICS_CHANNELS: usize = 25;
const AMBISONICS_VIRTUAL_SPEAKER_COUNT: usize = 24;
const AMBISONICS_VIRTUAL_SPEAKER_WEIGHT: f32 = 4.0 * PI / AMBISONICS_VIRTUAL_SPEAKER_COUNT as f32;
const HIGH_ORDER_NATIVE_AMBISONICS_VIRTUAL_SPEAKER_COUNT: usize = 256;

// This 24-point spherical design avoids the overly smooth/high-frequency-dull
// result that came from equally averaging every native HRTF table entry.
const AMBISONICS_VIRTUAL_SPEAKERS: [[f32; 3]; AMBISONICS_VIRTUAL_SPEAKER_COUNT] = [
    [0.866_246_8, 0.422_518_64, 0.266_635_4],
    [0.866_246_8, -0.422_518_64, -0.266_635_4],
    [0.866_246_8, 0.266_635_4, -0.422_518_64],
    [0.866_246_8, -0.266_635_4, 0.422_518_64],
    [-0.866_246_8, 0.422_518_64, -0.266_635_4],
    [-0.866_246_8, -0.422_518_64, 0.266_635_4],
    [-0.866_246_8, 0.266_635_4, 0.422_518_64],
    [-0.866_246_8, -0.266_635_4, -0.422_518_64],
    [0.266_635_4, 0.866_246_8, 0.422_518_64],
    [-0.266_635_4, 0.866_246_8, -0.422_518_64],
    [-0.422_518_64, 0.866_246_8, 0.266_635_4],
    [0.422_518_64, 0.866_246_8, -0.266_635_4],
    [-0.266_635_4, -0.866_246_8, 0.422_518_64],
    [0.266_635_4, -0.866_246_8, -0.422_518_64],
    [0.422_518_64, -0.866_246_8, 0.266_635_4],
    [-0.422_518_64, -0.866_246_8, -0.266_635_4],
    [0.422_518_64, 0.266_635_4, 0.866_246_8],
    [-0.422_518_64, -0.266_635_4, 0.866_246_8],
    [0.266_635_4, -0.422_518_64, 0.866_246_8],
    [-0.266_635_4, 0.422_518_64, 0.866_246_8],
    [0.422_518_64, -0.266_635_4, -0.866_246_8],
    [-0.422_518_64, 0.266_635_4, -0.866_246_8],
    [0.266_635_4, 0.422_518_64, -0.866_246_8],
    [-0.266_635_4, -0.422_518_64, -0.866_246_8],
];

/// Returns the number of ACN channels for an Ambisonics order.
pub fn native_ambisonics_channel_count(order: u32) -> Result<usize> {
    if order > MAX_NATIVE_AMBISONICS_ORDER {
        return Err(PetalSonicError::Configuration(format!(
            "native Ambisonics currently supports order 0..={MAX_NATIVE_AMBISONICS_ORDER}, got {order}"
        )));
    }

    Ok(((order + 1) * (order + 1)) as usize)
}

/// Compute real ACN/N3D spherical-harmonic coefficients up to order 4.
///
/// The listener-local direction convention is the same as native HRTF:
/// `x=right`, `y=up`, `z=front`. Coefficients are returned in ACN order:
/// 0, then 1:-1/0/1, then 2:-2/-1/0/1/2.
fn native_ambisonics_coefficients(
    order: u32,
    direction: Vec3,
) -> Result<[f32; MAX_NATIVE_AMBISONICS_CHANNELS]> {
    native_ambisonics_channel_count(order)?;

    let direction = normalize_or_front(direction);
    let x = direction.x;
    let y = direction.y;
    let z = direction.z;

    let mut coeffs = [0.0; MAX_NATIVE_AMBISONICS_CHANNELS];
    coeffs[0] = 0.282_094_8; // sqrt(1 / 4pi)

    if order >= 1 {
        coeffs[1] = 0.488_602_52 * y;
        coeffs[2] = 0.488_602_52 * z;
        coeffs[3] = 0.488_602_52 * x;
    }

    if order >= 2 {
        coeffs[4] = 1.092_548_5 * x * y;
        coeffs[5] = 1.092_548_5 * y * z;
        coeffs[6] = 0.315_391_57 * (3.0 * z * z - 1.0);
        coeffs[7] = 1.092_548_5 * x * z;
        coeffs[8] = 0.546_274_24 * (x * x - y * y);
    }

    if order >= 3 {
        coeffs[9] = 0.590_044 * y * (3.0 * x * x - y * y);
        coeffs[10] = 2.890_611 * x * y * z;
        coeffs[11] = 0.457_046 * y * (4.0 * z * z - x * x - y * y);
        coeffs[12] = 0.373_176 * z * (2.0 * z * z - 3.0 * x * x - 3.0 * y * y);
        coeffs[13] = 0.457_046 * x * (4.0 * z * z - x * x - y * y);
        coeffs[14] = 1.445_306 * z * (x * x - y * y);
        coeffs[15] = 0.590_044 * x * (x * x - 3.0 * y * y);
    }

    if order >= 4 {
        let x2 = x * x;
        let y2 = y * y;
        let z2 = z * z;
        coeffs[16] = 2.503_343 * x * y * (x2 - y2);
        coeffs[17] = 1.770_131 * y * z * (3.0 * x2 - y2);
        coeffs[18] = 0.946_175 * x * y * (7.0 * z2 - 1.0);
        coeffs[19] = 0.669_047 * y * z * (7.0 * z2 - 3.0);
        coeffs[20] = 0.105_786 * (35.0 * z2 * z2 - 30.0 * z2 + 3.0);
        coeffs[21] = 0.669_047 * x * z * (7.0 * z2 - 3.0);
        coeffs[22] = 0.473_087 * (x2 - y2) * (7.0 * z2 - 1.0);
        coeffs[23] = 1.770_131 * x * z * (x2 - 3.0 * y2);
        coeffs[24] = 0.625_836 * (x2 * (x2 - 3.0 * y2) - y2 * (3.0 * x2 - y2));
    }

    Ok(coeffs)
}

fn native_ambisonics_channel_order(channel: usize) -> u32 {
    let mut order = 0u32;
    while ((order + 1) * (order + 1)) as usize <= channel {
        order += 1;
    }
    order
}

fn native_ambisonics_max_re_weights(order: u32) -> Result<[f32; MAX_NATIVE_AMBISONICS_CHANNELS]> {
    let channel_count = native_ambisonics_channel_count(order)?;
    let max_re_cosine = (137.9_f32.to_radians() / (order as f32 + 1.51)).cos();
    let mut weights = [1.0; MAX_NATIVE_AMBISONICS_CHANNELS];

    for (channel, weight) in weights.iter_mut().enumerate().take(channel_count) {
        *weight = legendre_polynomial(native_ambisonics_channel_order(channel), max_re_cosine);
    }

    Ok(weights)
}

fn legendre_polynomial(order: u32, x: f32) -> f32 {
    match order {
        0 => 1.0,
        1 => x,
        2 => 0.5 * (3.0 * x * x - 1.0),
        3 => 0.5 * (5.0 * x * x * x - 3.0 * x),
        4 => (35.0 * x * x * x * x - 30.0 * x * x + 3.0) / 8.0,
        _ => {
            unreachable!("native Ambisonics only supports order 0..={MAX_NATIVE_AMBISONICS_ORDER}")
        }
    }
}

fn native_ambisonics_decoder_speaker_directions(order: u32) -> Result<Vec<Vec3>> {
    native_ambisonics_channel_count(order)?;
    if order <= 3 {
        Ok(AMBISONICS_VIRTUAL_SPEAKERS
            .iter()
            .map(|speaker| Vec3::new(speaker[0], speaker[1], speaker[2]))
            .collect())
    } else {
        Ok(fibonacci_sphere_directions(
            HIGH_ORDER_NATIVE_AMBISONICS_VIRTUAL_SPEAKER_COUNT,
        ))
    }
}

fn fibonacci_sphere_directions(count: usize) -> Vec<Vec3> {
    let golden_angle = PI * (3.0 - 5.0_f32.sqrt());
    (0..count)
        .map(|index| {
            let y = 1.0 - 2.0 * ((index as f32 + 0.5) / count as f32);
            let radius = (1.0 - y * y).max(0.0).sqrt();
            let theta = index as f32 * golden_angle;
            Vec3::new(theta.cos() * radius, y, theta.sin() * radius)
        })
        .collect()
}

/// Native Ambisonics encoder for mono point sources.
#[derive(Debug, Clone)]
pub struct NativeAmbisonicsEncoder {
    order: u32,
    channel_count: usize,
}

impl NativeAmbisonicsEncoder {
    pub fn new(order: u32) -> Result<Self> {
        let channel_count = native_ambisonics_channel_count(order)?;
        Ok(Self {
            order,
            channel_count,
        })
    }

    /// Encode `input` and accumulate it into a planar Ambisonics output buffer.
    pub fn encode_source_accumulate(
        &self,
        direction: Vec3,
        input: &[f32],
        output_planar: &mut [f32],
    ) -> Result<()> {
        let frames = input.len();
        let expected_len = frames * self.channel_count;
        if output_planar.len() < expected_len {
            return Err(PetalSonicError::Configuration(format!(
                "native Ambisonics output buffer too small: need {expected_len} samples, got {}",
                output_planar.len()
            )));
        }

        let coeffs = native_ambisonics_coefficients(self.order, direction)?;
        for (channel, coeff) in coeffs.iter().copied().take(self.channel_count).enumerate() {
            let channel_offset = channel * frames;
            for (frame_index, input_sample) in input.iter().copied().enumerate() {
                output_planar[channel_offset + frame_index] += input_sample * coeff;
            }
        }

        Ok(())
    }
}

/// Delay-line state for native Ambisonics binaural decoding.
#[derive(Debug, Clone)]
pub struct NativeAmbisonicsBinauralState {
    delay_lines: Vec<f32>,
    write_index: usize,
    fft_state: Option<NativeAmbisonicsFftState>,
}

impl NativeAmbisonicsBinauralState {
    fn new(channel_count: usize, taps: usize) -> Self {
        Self {
            delay_lines: vec![0.0; channel_count * taps],
            write_index: 0,
            fft_state: None,
        }
    }

    fn with_fft_plan(
        channel_count: usize,
        taps: usize,
        fft_plan: Option<&NativeAmbisonicsFftPlan>,
    ) -> Self {
        Self {
            delay_lines: vec![0.0; channel_count * taps],
            write_index: 0,
            fft_state: fft_plan.map(NativeAmbisonicsFftState::new),
        }
    }
}

struct NativeAmbisonicsMinimumPhasePlan {
    fft_size: usize,
    inverse_scale: f32,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    real_input: Vec<f32>,
    real_output: Vec<f32>,
    spectrum: Vec<Complex32>,
    forward_scratch: Vec<Complex32>,
    inverse_scratch: Vec<Complex32>,
}

impl NativeAmbisonicsMinimumPhasePlan {
    fn new(taps: usize) -> Result<Self> {
        let fft_size = taps
            .checked_mul(2)
            .and_then(|value| value.checked_next_power_of_two())
            .ok_or_else(|| {
                PetalSonicError::Configuration(
                    "native Ambisonics minimum-phase FFT size overflow".to_string(),
                )
            })?;
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(fft_size);
        let inverse = planner.plan_fft_inverse(fft_size);
        Ok(Self {
            fft_size,
            inverse_scale: 1.0 / fft_size as f32,
            real_input: forward.make_input_vec(),
            real_output: inverse.make_output_vec(),
            spectrum: forward.make_output_vec(),
            forward_scratch: forward.make_scratch_vec(),
            inverse_scratch: inverse.make_scratch_vec(),
            forward,
            inverse,
        })
    }

    fn minimum_phase(&mut self, signal: &[f32]) -> Result<Vec<f32>> {
        self.real_input.fill(0.0);
        self.real_input[..signal.len()].copy_from_slice(signal);
        self.forward
            .process_with_scratch(
                &mut self.real_input,
                &mut self.spectrum,
                &mut self.forward_scratch,
            )
            .map_err(|error| {
                native_ambisonics_fft_error("minimum-phase forward magnitude", error)
            })?;

        for bin in &mut self.spectrum {
            bin.re = bin.norm().max(1.0e-9).ln();
            bin.im = 0.0;
        }
        force_real_realfft_bins(&mut self.spectrum);

        self.inverse
            .process_with_scratch(
                &mut self.spectrum,
                &mut self.real_output,
                &mut self.inverse_scratch,
            )
            .map_err(|error| {
                native_ambisonics_fft_error("minimum-phase inverse cepstrum", error)
            })?;

        let nyquist = self.fft_size / 2;
        self.real_input.fill(0.0);
        self.real_input[0] = self.real_output[0] * self.inverse_scale;
        for index in 1..nyquist {
            self.real_input[index] = 2.0 * self.real_output[index] * self.inverse_scale;
        }
        if self.fft_size.is_multiple_of(2) {
            self.real_input[nyquist] = self.real_output[nyquist] * self.inverse_scale;
        }

        self.forward
            .process_with_scratch(
                &mut self.real_input,
                &mut self.spectrum,
                &mut self.forward_scratch,
            )
            .map_err(|error| {
                native_ambisonics_fft_error("minimum-phase forward cepstrum", error)
            })?;

        for bin in &mut self.spectrum {
            let magnitude = bin.re.exp();
            let phase = bin.im;
            *bin = Complex32::new(magnitude * phase.cos(), magnitude * phase.sin());
        }
        force_real_realfft_bins(&mut self.spectrum);

        self.inverse
            .process_with_scratch(
                &mut self.spectrum,
                &mut self.real_output,
                &mut self.inverse_scratch,
            )
            .map_err(|error| native_ambisonics_fft_error("minimum-phase inverse signal", error))?;

        Ok(self.real_output[..signal.len()]
            .iter()
            .map(|sample| sample * self.inverse_scale)
            .collect())
    }
}

#[derive(Clone)]
struct NativeAmbisonicsFftFilter {
    left_spectrum: Vec<Complex32>,
    right_spectrum: Vec<Complex32>,
}

#[derive(Clone)]
struct NativeAmbisonicsFftPlan {
    block_frames: usize,
    fft_size: usize,
    inverse_scale: f32,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    filters: Vec<NativeAmbisonicsFftFilter>,
}

impl std::fmt::Debug for NativeAmbisonicsFftPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeAmbisonicsFftPlan")
            .field("block_frames", &self.block_frames)
            .field("fft_size", &self.fft_size)
            .field("channel_count", &self.filters.len())
            .finish()
    }
}

impl NativeAmbisonicsFftPlan {
    fn new(
        channel_count: usize,
        taps: usize,
        left_filters: &[f32],
        right_filters: &[f32],
        block_frames: usize,
    ) -> Result<Self> {
        if block_frames == 0 {
            return Err(PetalSonicError::Configuration(
                "native Ambisonics FFT block size must be non-zero".to_string(),
            ));
        }

        let fft_size = fft_convolution_size(block_frames, taps)?;
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(fft_size);
        let inverse = planner.plan_fft_inverse(fft_size);
        let mut padded = forward.make_input_vec();
        let mut spectrum = forward.make_output_vec();
        let mut scratch = forward.make_scratch_vec();
        let mut filters = Vec::with_capacity(channel_count);

        for channel in 0..channel_count {
            let filter_offset = channel * taps;
            padded.fill(0.0);
            padded[..taps].copy_from_slice(&left_filters[filter_offset..filter_offset + taps]);
            forward
                .process_with_scratch(&mut padded, &mut spectrum, &mut scratch)
                .map_err(|error| {
                    native_ambisonics_fft_error("precomputing left decoder filter", error)
                })?;
            let left_spectrum = spectrum.clone();

            padded.fill(0.0);
            padded[..taps].copy_from_slice(&right_filters[filter_offset..filter_offset + taps]);
            forward
                .process_with_scratch(&mut padded, &mut spectrum, &mut scratch)
                .map_err(|error| {
                    native_ambisonics_fft_error("precomputing right decoder filter", error)
                })?;
            let right_spectrum = spectrum.clone();

            filters.push(NativeAmbisonicsFftFilter {
                left_spectrum,
                right_spectrum,
            });
        }

        Ok(Self {
            block_frames,
            fft_size,
            inverse_scale: 1.0 / fft_size as f32,
            forward,
            inverse,
            filters,
        })
    }
}

#[derive(Debug, Clone)]
struct NativeAmbisonicsFftState {
    block_frames: usize,
    fft_size: usize,
    forward_input: Vec<f32>,
    input_spectrum: Vec<Complex32>,
    left_spectrum: Vec<Complex32>,
    right_spectrum: Vec<Complex32>,
    left_time: Vec<f32>,
    right_time: Vec<f32>,
    left_overlap: Vec<f32>,
    right_overlap: Vec<f32>,
    forward_scratch: Vec<Complex32>,
    inverse_scratch: Vec<Complex32>,
}

impl NativeAmbisonicsFftState {
    fn new(plan: &NativeAmbisonicsFftPlan) -> Self {
        Self {
            block_frames: plan.block_frames,
            fft_size: plan.fft_size,
            forward_input: plan.forward.make_input_vec(),
            input_spectrum: plan.forward.make_output_vec(),
            left_spectrum: plan.inverse.make_input_vec(),
            right_spectrum: plan.inverse.make_input_vec(),
            left_time: plan.inverse.make_output_vec(),
            right_time: plan.inverse.make_output_vec(),
            left_overlap: vec![0.0; plan.fft_size - plan.block_frames],
            right_overlap: vec![0.0; plan.fft_size - plan.block_frames],
            forward_scratch: plan.forward.make_scratch_vec(),
            inverse_scratch: plan.inverse.make_scratch_vec(),
        }
    }

    fn matches_plan(&self, plan: &NativeAmbisonicsFftPlan) -> bool {
        self.block_frames == plan.block_frames && self.fft_size == plan.fft_size
    }
}

/// Native Ambisonics binaural decoder derived from the native HRTF table.
#[derive(Debug, Clone)]
pub struct NativeAmbisonicsBinauralDecoder {
    channel_count: usize,
    taps: usize,
    left_filters: Vec<f32>,
    right_filters: Vec<f32>,
    fft_plan: Option<Arc<NativeAmbisonicsFftPlan>>,
}

impl NativeAmbisonicsBinauralDecoder {
    #[cfg(test)]
    pub fn new(table: Arc<NativeHrtfTable>, order: u32) -> Result<Self> {
        Self::from_table(table, order, None)
    }

    pub fn with_frame_size(
        table: Arc<NativeHrtfTable>,
        order: u32,
        frame_size: usize,
    ) -> Result<Self> {
        Self::from_table(table, order, Some(frame_size))
    }

    fn from_table(
        table: Arc<NativeHrtfTable>,
        order: u32,
        frame_size: Option<usize>,
    ) -> Result<Self> {
        let channel_count = native_ambisonics_channel_count(order)?;
        let taps = table.taps();
        let mut left_filters = vec![0.0; channel_count * taps];
        let mut right_filters = vec![0.0; channel_count * taps];
        let direction_count = table.direction_count();
        if direction_count == 0 {
            return Err(PetalSonicError::Configuration(
                "native Ambisonics decoder requires at least one HRTF direction".to_string(),
            ));
        }

        // Project native HRTFs from an equal-area virtual-speaker grid with max-rE
        // order weighting. Orders 0..=3 keep the 24-point design; order 4 uses a denser
        // Fibonacci grid because the smaller design
        // is not intended for fourth-order binaural Ambisonics.
        let order_weights = native_ambisonics_max_re_weights(order)?;
        let speaker_directions = native_ambisonics_decoder_speaker_directions(order)?;
        let speaker_weight = if order <= 3 {
            AMBISONICS_VIRTUAL_SPEAKER_WEIGHT
        } else {
            4.0 * PI / speaker_directions.len() as f32
        };
        let mut minimum_phase_plan = if order >= 4 {
            Some(NativeAmbisonicsMinimumPhasePlan::new(taps)?)
        } else {
            None
        };
        for (speaker_index, speaker_direction) in speaker_directions.iter().copied().enumerate() {
            let hrtf_index = table.nearest_direction_index(speaker_direction);
            let entry = table.direction(hrtf_index).ok_or_else(|| {
                PetalSonicError::Configuration(format!(
                    "native HRTF direction index {hrtf_index} for Ambisonics virtual speaker {speaker_index} disappeared during decoder build"
                ))
            })?;
            let coeffs = native_ambisonics_coefficients(order, speaker_direction)?;
            let minimum_phase_left;
            let minimum_phase_right;
            let (left_hrir, right_hrir) = if let Some(plan) = &mut minimum_phase_plan {
                minimum_phase_left = plan.minimum_phase(&entry.left)?;
                minimum_phase_right = plan.minimum_phase(&entry.right)?;
                (
                    minimum_phase_left.as_slice(),
                    minimum_phase_right.as_slice(),
                )
            } else {
                (entry.left.as_slice(), entry.right.as_slice())
            };

            for channel in 0..channel_count {
                let scaled_coeff = coeffs[channel] * speaker_weight * order_weights[channel];
                let filter_offset = channel * taps;
                for tap in 0..taps {
                    left_filters[filter_offset + tap] += left_hrir[tap] * scaled_coeff;
                    right_filters[filter_offset + tap] += right_hrir[tap] * scaled_coeff;
                }
            }
        }

        let fft_plan = frame_size
            .map(|frame_size| {
                NativeAmbisonicsFftPlan::new(
                    channel_count,
                    taps,
                    &left_filters,
                    &right_filters,
                    frame_size,
                )
                .map(Arc::new)
            })
            .transpose()?;

        Ok(Self {
            channel_count,
            taps,
            left_filters,
            right_filters,
            fft_plan,
        })
    }

    #[cfg(test)]
    pub fn channel_count(&self) -> usize {
        self.channel_count
    }

    pub fn create_state(&self) -> NativeAmbisonicsBinauralState {
        if let Some(fft_plan) = self.fft_plan.as_deref() {
            NativeAmbisonicsBinauralState::with_fft_plan(
                self.channel_count,
                self.taps,
                Some(fft_plan),
            )
        } else {
            NativeAmbisonicsBinauralState::new(self.channel_count, self.taps)
        }
    }

    /// Decode a planar Ambisonics buffer into an interleaved stereo output buffer.
    ///
    /// Output is accumulated, not cleared.
    pub fn decode(
        &self,
        state: &mut NativeAmbisonicsBinauralState,
        input_planar: &[f32],
        output_interleaved: &mut [f32],
    ) -> Result<NativeHrtfRenderMetrics> {
        if self.taps == 0 {
            return Err(PetalSonicError::Configuration(
                "native Ambisonics decoder has zero taps".to_string(),
            ));
        }

        if !input_planar.len().is_multiple_of(self.channel_count) {
            return Err(PetalSonicError::Configuration(format!(
                "native Ambisonics input length {} is not divisible by channel count {}",
                input_planar.len(),
                self.channel_count
            )));
        }

        let frames = input_planar.len() / self.channel_count;
        if output_interleaved.len() < frames * 2 {
            return Err(PetalSonicError::Configuration(format!(
                "native Ambisonics decode output buffer too small: need {}, got {} samples",
                frames * 2,
                output_interleaved.len()
            )));
        }

        self.ensure_state_compatible(state);

        let convolution_start = Instant::now();
        if let Some(plan) = self.fft_plan.as_deref()
            && frames == plan.block_frames
            && let Some(fft_state) = state.fft_state.as_ref()
            && fft_state.matches_plan(plan)
        {
            self.decode_frequency_domain(state, plan, input_planar, output_interleaved)?;
            return Ok(NativeHrtfRenderMetrics {
                direction_lookup_time_us: 0,
                convolution_time_us: convolution_start.elapsed().as_micros() as u64,
            });
        }

        self.decode_time_domain(state, input_planar, output_interleaved, frames);

        Ok(NativeHrtfRenderMetrics {
            direction_lookup_time_us: 0,
            convolution_time_us: convolution_start.elapsed().as_micros() as u64,
        })
    }

    fn ensure_state_compatible(&self, state: &mut NativeAmbisonicsBinauralState) {
        if state.delay_lines.len() != self.channel_count * self.taps {
            *state = self.create_state();
            return;
        }

        if let Some(plan) = self.fft_plan.as_deref() {
            let needs_fft_state = state
                .fft_state
                .as_ref()
                .is_none_or(|fft_state| !fft_state.matches_plan(plan));
            if needs_fft_state {
                *state = self.create_state();
            }
        }
    }

    fn decode_frequency_domain(
        &self,
        state: &mut NativeAmbisonicsBinauralState,
        plan: &NativeAmbisonicsFftPlan,
        input_planar: &[f32],
        output_interleaved: &mut [f32],
    ) -> Result<()> {
        let fft_state = state.fft_state.as_mut().ok_or_else(|| {
            PetalSonicError::SpatialAudio(
                "native Ambisonics FFT state is not initialized".to_string(),
            )
        })?;

        fft_state.left_spectrum.fill(Complex32::new(0.0, 0.0));
        fft_state.right_spectrum.fill(Complex32::new(0.0, 0.0));

        for channel in 0..self.channel_count {
            let channel_offset = channel * plan.block_frames;
            let filter = &plan.filters[channel];

            fft_state.forward_input.fill(0.0);
            fft_state.forward_input[..plan.block_frames]
                .copy_from_slice(&input_planar[channel_offset..channel_offset + plan.block_frames]);
            plan.forward
                .process_with_scratch(
                    &mut fft_state.forward_input,
                    &mut fft_state.input_spectrum,
                    &mut fft_state.forward_scratch,
                )
                .map_err(|error| native_ambisonics_fft_error("processing input channel", error))?;

            for (((left, right), input_bin), (left_filter, right_filter)) in fft_state
                .left_spectrum
                .iter_mut()
                .zip(fft_state.right_spectrum.iter_mut())
                .zip(&fft_state.input_spectrum)
                .zip(filter.left_spectrum.iter().zip(&filter.right_spectrum))
            {
                *left += *input_bin * *left_filter;
                *right += *input_bin * *right_filter;
            }
        }
        force_real_realfft_bins(&mut fft_state.left_spectrum);
        force_real_realfft_bins(&mut fft_state.right_spectrum);

        plan.inverse
            .process_with_scratch(
                &mut fft_state.left_spectrum,
                &mut fft_state.left_time,
                &mut fft_state.inverse_scratch,
            )
            .map_err(|error| native_ambisonics_fft_error("inverse left ear", error))?;
        plan.inverse
            .process_with_scratch(
                &mut fft_state.right_spectrum,
                &mut fft_state.right_time,
                &mut fft_state.inverse_scratch,
            )
            .map_err(|error| native_ambisonics_fft_error("inverse right ear", error))?;

        let scale = plan.inverse_scale;
        for frame_index in 0..plan.block_frames {
            let previous_left = fft_state
                .left_overlap
                .get(frame_index)
                .copied()
                .unwrap_or(0.0);
            let previous_right = fft_state
                .right_overlap
                .get(frame_index)
                .copied()
                .unwrap_or(0.0);
            let out_index = frame_index * 2;
            output_interleaved[out_index] +=
                fft_state.left_time[frame_index] * scale + previous_left;
            output_interleaved[out_index + 1] +=
                fft_state.right_time[frame_index] * scale + previous_right;
        }

        refresh_overlap(
            &mut fft_state.left_overlap,
            &fft_state.left_time,
            plan.block_frames,
            scale,
        );
        refresh_overlap(
            &mut fft_state.right_overlap,
            &fft_state.right_time,
            plan.block_frames,
            scale,
        );
        append_delay_lines(
            &mut state.delay_lines,
            &mut state.write_index,
            self.channel_count,
            self.taps,
            input_planar,
            plan.block_frames,
        );

        Ok(())
    }

    fn decode_time_domain(
        &self,
        state: &mut NativeAmbisonicsBinauralState,
        input_planar: &[f32],
        output_interleaved: &mut [f32],
        frames: usize,
    ) {
        for frame_index in 0..frames {
            let mut left = 0.0f32;
            let mut right = 0.0f32;

            for channel in 0..self.channel_count {
                let channel_offset = channel * frames;
                let state_offset = channel * self.taps;
                let filter_offset = channel * self.taps;
                state.delay_lines[state_offset + state.write_index] =
                    input_planar[channel_offset + frame_index];

                let mut tap = 0usize;
                for delay_index in (0..=state.write_index).rev() {
                    let delayed = state.delay_lines[state_offset + delay_index];
                    left += delayed * self.left_filters[filter_offset + tap];
                    right += delayed * self.right_filters[filter_offset + tap];
                    tap += 1;
                }
                for delay_index in (state.write_index + 1..self.taps).rev() {
                    let delayed = state.delay_lines[state_offset + delay_index];
                    left += delayed * self.left_filters[filter_offset + tap];
                    right += delayed * self.right_filters[filter_offset + tap];
                    tap += 1;
                }
            }

            let out_index = frame_index * 2;
            output_interleaved[out_index] += left;
            output_interleaved[out_index + 1] += right;
            state.write_index = (state.write_index + 1) % self.taps;
        }
    }
}

fn fft_convolution_size(block_frames: usize, taps: usize) -> Result<usize> {
    let linear_convolution_len = block_frames
        .checked_add(taps)
        .and_then(|value| value.checked_sub(1))
        .ok_or_else(|| {
            PetalSonicError::Configuration("native Ambisonics FFT size overflow".to_string())
        })?;
    linear_convolution_len
        .checked_next_power_of_two()
        .ok_or_else(|| {
            PetalSonicError::Configuration("native Ambisonics FFT size overflow".to_string())
        })
}

fn native_ambisonics_fft_error(context: &str, error: realfft::FftError) -> PetalSonicError {
    PetalSonicError::SpatialAudio(format!("native Ambisonics FFT {context} failed: {error}"))
}

fn force_real_realfft_bins(spectrum: &mut [Complex32]) {
    if let Some(first) = spectrum.first_mut() {
        first.im = 0.0;
    }
    if spectrum.len() > 1
        && let Some(last) = spectrum.last_mut()
    {
        last.im = 0.0;
    }
}

fn refresh_overlap(overlap: &mut [f32], time_domain: &[f32], block_frames: usize, scale: f32) {
    let remaining_old = overlap.len().saturating_sub(block_frames);
    if remaining_old > 0 {
        overlap.copy_within(block_frames..block_frames + remaining_old, 0);
    }
    overlap[remaining_old..].fill(0.0);

    for (overlap_sample, tail_sample) in overlap.iter_mut().zip(&time_domain[block_frames..]) {
        *overlap_sample += *tail_sample * scale;
    }
}

fn append_delay_lines(
    delay_lines: &mut [f32],
    write_index: &mut usize,
    channel_count: usize,
    taps: usize,
    input_planar: &[f32],
    frames: usize,
) {
    for frame_index in 0..frames {
        for channel in 0..channel_count {
            let channel_offset = channel * frames;
            let state_offset = channel * taps;
            delay_lines[state_offset + *write_index] = input_planar[channel_offset + frame_index];
        }
        *write_index = (*write_index + 1) % taps;
    }
}

fn normalize_or_front(direction: Vec3) -> Vec3 {
    if direction.is_finite() && direction.length_squared() > f32::EPSILON {
        direction.normalize()
    } else {
        Vec3::Z
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial::native_hrtf::{NativeHrtfDirection, NativeHrtfTable};

    #[test]
    fn channel_count_matches_ambisonics_order() {
        assert_eq!(native_ambisonics_channel_count(0).unwrap(), 1);
        assert_eq!(native_ambisonics_channel_count(1).unwrap(), 4);
        assert_eq!(native_ambisonics_channel_count(2).unwrap(), 9);
        assert_eq!(native_ambisonics_channel_count(3).unwrap(), 16);
        assert_eq!(native_ambisonics_channel_count(4).unwrap(), 25);
        assert!(native_ambisonics_channel_count(5).is_err());
    }

    #[test]
    fn default_native_ambisonics_order_is_four() {
        assert_eq!(DEFAULT_NATIVE_AMBISONICS_ORDER, 4);
        assert_eq!(
            native_ambisonics_channel_count(DEFAULT_NATIVE_AMBISONICS_ORDER).unwrap(),
            25
        );
    }

    #[test]
    fn order_four_coefficients_use_expected_front_axis_terms() {
        let coeffs = native_ambisonics_coefficients(4, Vec3::Z).unwrap();

        assert!((coeffs[0] - 0.282_094_8).abs() < 1e-6);
        assert!((coeffs[2] - 0.488_602_52).abs() < 1e-6);
        assert!((coeffs[6] - 0.630_783_14).abs() < 1e-6);
        assert!((coeffs[12] - 0.746_352).abs() < 1e-6);
        assert!((coeffs[20] - 0.846_288).abs() < 1e-6);
        for (channel, coefficient) in coeffs.iter().enumerate() {
            if [0, 2, 6, 12, 20].contains(&channel) {
                continue;
            }
            assert!(coefficient.abs() < 1e-6, "channel {channel}");
        }
    }

    #[test]
    fn virtual_speakers_are_order_two_orthonormal() {
        let channel_count = native_ambisonics_channel_count(2).unwrap();
        let mut basis = Vec::new();
        for speaker in AMBISONICS_VIRTUAL_SPEAKERS {
            basis.push(
                native_ambisonics_coefficients(2, Vec3::new(speaker[0], speaker[1], speaker[2]))
                    .unwrap(),
            );
        }

        for a in 0..channel_count {
            for b in 0..channel_count {
                let integral = basis
                    .iter()
                    .map(|coeffs| coeffs[a] * coeffs[b] * AMBISONICS_VIRTUAL_SPEAKER_WEIGHT)
                    .sum::<f32>();
                let expected = if a == b { 1.0 } else { 0.0 };
                assert!(
                    (integral - expected).abs() < 1e-5,
                    "channels {a}/{b}: expected {expected}, got {integral}"
                );
            }
        }
    }

    #[test]
    fn minimum_phase_transform_preserves_magnitude_response() {
        let input = vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let mut plan = NativeAmbisonicsMinimumPhasePlan::new(input.len()).unwrap();
        let output = plan.minimum_phase(&input).unwrap();

        assert_eq!(output.len(), input.len());
        let input_magnitude = fft_magnitudes(&input, plan.fft_size);
        let output_magnitude = fft_magnitudes(&output, plan.fft_size);
        for (input_bin, output_bin) in input_magnitude.iter().zip(output_magnitude) {
            assert!((input_bin - output_bin).abs() < 1e-3);
        }
        assert!(output[0].abs() > 0.99);
    }

    fn fft_magnitudes(signal: &[f32], fft_size: usize) -> Vec<f32> {
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(fft_size);
        let mut input = forward.make_input_vec();
        let mut spectrum = forward.make_output_vec();
        let mut scratch = forward.make_scratch_vec();
        input[..signal.len()].copy_from_slice(signal);
        forward
            .process_with_scratch(&mut input, &mut spectrum, &mut scratch)
            .unwrap();
        spectrum.iter().map(|bin| bin.norm()).collect()
    }

    #[test]
    fn max_re_weights_match_ambisonic_channel_groups() {
        let weights = native_ambisonics_max_re_weights(2).unwrap();

        assert!((weights[0] - 1.0).abs() < 1e-6);
        for channel in 1..=3 {
            assert!((weights[channel] - weights[1]).abs() < 1e-6);
        }
        for channel in 4..=8 {
            assert!((weights[channel] - weights[4]).abs() < 1e-6);
        }
        assert!(weights[1] > weights[4]);
        assert!(weights[4] > 0.0);
    }

    #[test]
    fn native_encode_accumulates_planar_channels() {
        let encoder = NativeAmbisonicsEncoder::new(0).unwrap();
        let mut encoded = [1.0, 2.0, 3.0];
        encoder
            .encode_source_accumulate(Vec3::Z, &[4.0, 5.0, 6.0], &mut encoded)
            .unwrap();

        assert!((encoded[0] - (1.0 + 4.0 * 0.282_094_8)).abs() < 1e-6);
        assert!((encoded[1] - (2.0 + 5.0 * 0.282_094_8)).abs() < 1e-6);
        assert!((encoded[2] - (3.0 + 6.0 * 0.282_094_8)).abs() < 1e-6);
    }

    #[test]
    fn order_zero_decode_reconstructs_single_direction_hrir() {
        let table = Arc::new(
            NativeHrtfTable::new(
                48_000,
                vec![NativeHrtfDirection::new(Vec3::Z, vec![1.0], vec![2.0])],
            )
            .unwrap(),
        );
        let encoder = NativeAmbisonicsEncoder::new(0).unwrap();
        let decoder = NativeAmbisonicsBinauralDecoder::new(table, 0).unwrap();
        let mut state = decoder.create_state();
        let mut encoded = [0.0];
        let mut output = [0.0, 0.0];

        encoder
            .encode_source_accumulate(Vec3::Z, &[1.0], &mut encoded)
            .unwrap();
        decoder.decode(&mut state, &encoded, &mut output).unwrap();

        assert!((output[0] - 1.0).abs() < 1e-5);
        assert!((output[1] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn frequency_domain_decode_matches_time_domain_across_blocks() {
        let table = Arc::new(
            NativeHrtfTable::new(
                48_000,
                vec![
                    NativeHrtfDirection::new(
                        Vec3::Z,
                        vec![0.75, -0.2, 0.05],
                        vec![0.5, 0.125, -0.25],
                    ),
                    NativeHrtfDirection::new(Vec3::X, vec![0.1, 0.2, -0.1], vec![-0.3, 0.4, 0.2]),
                    NativeHrtfDirection::new(
                        Vec3::Y,
                        vec![-0.2, 0.05, 0.3],
                        vec![0.25, -0.15, 0.1],
                    ),
                ],
            )
            .unwrap(),
        );
        let time_decoder = NativeAmbisonicsBinauralDecoder::new(table.clone(), 2).unwrap();
        let fft_decoder = NativeAmbisonicsBinauralDecoder::with_frame_size(table, 2, 8).unwrap();
        let mut time_state = time_decoder.create_state();
        let mut fft_state = fft_decoder.create_state();
        let channel_count = time_decoder.channel_count();
        let frames = 8;

        for block_index in 0..3 {
            let mut input = vec![0.0f32; channel_count * frames];
            for channel in 0..channel_count {
                for frame in 0..frames {
                    let phase =
                        block_index as f32 * 0.31 + channel as f32 * 0.17 + frame as f32 * 0.13;
                    input[channel * frames + frame] = phase.sin() * 0.25;
                }
            }

            let mut time_output = vec![0.0f32; frames * 2];
            let mut fft_output = vec![0.0f32; frames * 2];
            time_decoder
                .decode(&mut time_state, &input, &mut time_output)
                .unwrap();
            fft_decoder
                .decode(&mut fft_state, &input, &mut fft_output)
                .unwrap();

            for (time_sample, fft_sample) in time_output.iter().zip(fft_output) {
                assert!((*time_sample - fft_sample).abs() < 1e-4);
            }
        }
    }
}
