use crate::error::{PetalSonicError, Result};
use crate::math::Vec3;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex, num_complex::Complex32};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

const DEFAULT_DIRECTION: Vec3 = Vec3::Z;
const PETALHRTF_MAGIC: &[u8; 8] = b"PETHRTF\0";
const PETALHRTF_VERSION: u32 = 1;
const PETALHRTF_HEADER_BYTES: usize = 8 + 4 + 4 + 4 + 4;
const F32_BYTES: usize = std::mem::size_of::<f32>();

/// One directional HRIR pair in a native PetalSonic HRTF table.
#[derive(Debug, Clone)]
pub struct NativeHrtfDirection {
    /// Unit direction in listener-local coordinates.
    pub direction: Vec3,
    /// Left-ear impulse response taps.
    pub left: Vec<f32>,
    /// Right-ear impulse response taps.
    pub right: Vec<f32>,
}

impl NativeHrtfDirection {
    pub fn new(direction: Vec3, left: Vec<f32>, right: Vec<f32>) -> Self {
        Self {
            direction,
            left,
            right,
        }
    }
}

/// Runtime-ready HRTF table used by the native binaural renderer.
#[derive(Debug, Clone)]
pub struct NativeHrtfTable {
    sample_rate: u32,
    taps: usize,
    directions: Vec<NativeHrtfDirection>,
}

impl NativeHrtfTable {
    pub fn new(sample_rate: u32, directions: Vec<NativeHrtfDirection>) -> Result<Self> {
        if sample_rate == 0 {
            return Err(PetalSonicError::Configuration(
                "native HRTF sample rate must be non-zero".to_string(),
            ));
        }

        let Some(first) = directions.first() else {
            return Err(PetalSonicError::Configuration(
                "native HRTF table must contain at least one direction".to_string(),
            ));
        };

        let taps = first.left.len();
        if taps == 0 {
            return Err(PetalSonicError::Configuration(
                "native HRTF impulse responses must contain at least one tap".to_string(),
            ));
        }

        let mut normalized_directions = Vec::with_capacity(directions.len());
        for (index, mut entry) in directions.into_iter().enumerate() {
            if entry.left.len() != taps || entry.right.len() != taps {
                return Err(PetalSonicError::Configuration(format!(
                    "native HRTF direction {index} has mismatched tap count"
                )));
            }

            if !entry.left.iter().all(|sample| sample.is_finite())
                || !entry.right.iter().all(|sample| sample.is_finite())
            {
                return Err(PetalSonicError::Configuration(format!(
                    "native HRTF direction {index} contains non-finite taps"
                )));
            }

            entry.direction = normalize_direction(entry.direction);
            normalized_directions.push(entry);
        }

        Ok(Self {
            sample_rate,
            taps,
            directions: normalized_directions,
        })
    }

    /// Load a native PetalSonic HRTF table from a `.petalhrtf` file.
    pub fn from_petalhrtf_file(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_petalhrtf_bytes(&bytes)
    }

    /// Decode a native PetalSonic HRTF table from bytes.
    ///
    /// Binary format, little-endian:
    ///
    /// - magic: `PETHRTF\0` (8 bytes)
    /// - version: `u32` (`1`)
    /// - sample rate: `u32`
    /// - direction count: `u32`
    /// - taps per ear: `u32`
    /// - repeated direction records:
    ///   - listener-local unit direction: `f32 x, y, z`
    ///   - left HRIR taps: `taps * f32`
    ///   - right HRIR taps: `taps * f32`
    pub fn from_petalhrtf_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = PetalHrtfReader::new(bytes);
        let magic = reader.read_bytes(PETALHRTF_MAGIC.len())?;
        if magic != PETALHRTF_MAGIC {
            return Err(PetalSonicError::Configuration(
                "invalid native HRTF magic; expected PETHRTF".to_string(),
            ));
        }

        let version = reader.read_u32()?;
        if version != PETALHRTF_VERSION {
            return Err(PetalSonicError::Configuration(format!(
                "unsupported native HRTF version {version}; expected {PETALHRTF_VERSION}"
            )));
        }

        let sample_rate = reader.read_u32()?;
        let direction_count = reader.read_u32()? as usize;
        let taps = reader.read_u32()? as usize;
        validate_petalhrtf_size(bytes.len(), direction_count, taps)?;

        let mut directions = Vec::with_capacity(direction_count);
        for _ in 0..direction_count {
            let direction = Vec3::new(reader.read_f32()?, reader.read_f32()?, reader.read_f32()?);

            let mut left = Vec::with_capacity(taps);
            for _ in 0..taps {
                left.push(reader.read_f32()?);
            }

            let mut right = Vec::with_capacity(taps);
            for _ in 0..taps {
                right.push(reader.read_f32()?);
            }

            directions.push(NativeHrtfDirection::new(direction, left, right));
        }
        reader.finish()?;

        Self::new(sample_rate, directions)
    }

    /// Encode this table as `.petalhrtf` bytes.
    #[cfg(test)]
    pub fn to_petalhrtf_bytes(&self) -> Vec<u8> {
        let direction_record_bytes = (3 + self.taps * 2) * F32_BYTES;
        let total_bytes = PETALHRTF_HEADER_BYTES + self.directions.len() * direction_record_bytes;
        let mut bytes = Vec::with_capacity(total_bytes);

        bytes.extend_from_slice(PETALHRTF_MAGIC);
        bytes.extend_from_slice(&PETALHRTF_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(self.directions.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(self.taps as u32).to_le_bytes());

        for entry in &self.directions {
            bytes.extend_from_slice(&entry.direction.x.to_le_bytes());
            bytes.extend_from_slice(&entry.direction.y.to_le_bytes());
            bytes.extend_from_slice(&entry.direction.z.to_le_bytes());
            for sample in &entry.left {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            for sample in &entry.right {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }

        bytes
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn taps(&self) -> usize {
        self.taps
    }

    pub fn direction_count(&self) -> usize {
        self.directions.len()
    }

    pub fn direction(&self, index: usize) -> Option<&NativeHrtfDirection> {
        self.directions.get(index)
    }

    pub fn nearest_direction_index(&self, direction: Vec3) -> usize {
        let direction = normalize_direction(direction);
        let mut best_index = 0usize;
        let mut best_dot = f32::NEG_INFINITY;

        for (index, entry) in self.directions.iter().enumerate() {
            let dot = direction.dot(entry.direction);
            if dot > best_dot {
                best_dot = dot;
                best_index = index;
            }
        }

        best_index
    }
}

/// Detailed timing metrics for native HRTF rendering.
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeHrtfRenderMetrics {
    /// Time spent selecting the HRTF direction.
    pub direction_lookup_time_us: u64,
    /// Time spent running HRIR convolution.
    pub convolution_time_us: u64,
}

#[derive(Clone)]
struct NativeHrtfFftDirection {
    left_spectrum: Vec<Complex32>,
    right_spectrum: Vec<Complex32>,
}

#[derive(Clone)]
struct NativeHrtfFftPlan {
    block_frames: usize,
    fft_size: usize,
    complex_len: usize,
    inverse_scale: f32,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    directions: Vec<NativeHrtfFftDirection>,
}

impl std::fmt::Debug for NativeHrtfFftPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeHrtfFftPlan")
            .field("block_frames", &self.block_frames)
            .field("fft_size", &self.fft_size)
            .field("complex_len", &self.complex_len)
            .field("direction_count", &self.directions.len())
            .finish()
    }
}

impl NativeHrtfFftPlan {
    fn new(table: &NativeHrtfTable, block_frames: usize) -> Result<Self> {
        if block_frames == 0 {
            return Err(PetalSonicError::Configuration(
                "native HRTF FFT block size must be non-zero".to_string(),
            ));
        }

        let fft_size = fft_convolution_size(block_frames, table.taps)?;
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(fft_size);
        let inverse = planner.plan_fft_inverse(fft_size);
        let complex_len = forward.complex_len();
        let mut padded = forward.make_input_vec();
        let mut spectrum = forward.make_output_vec();
        let mut scratch = forward.make_scratch_vec();
        let mut directions = Vec::with_capacity(table.directions.len());

        for entry in &table.directions {
            padded.fill(0.0);
            padded[..table.taps].copy_from_slice(&entry.left);
            forward
                .process_with_scratch(&mut padded, &mut spectrum, &mut scratch)
                .map_err(|error| native_hrtf_fft_error("precomputing left HRIR", error))?;
            let left_spectrum = spectrum.clone();

            padded.fill(0.0);
            padded[..table.taps].copy_from_slice(&entry.right);
            forward
                .process_with_scratch(&mut padded, &mut spectrum, &mut scratch)
                .map_err(|error| native_hrtf_fft_error("precomputing right HRIR", error))?;
            let right_spectrum = spectrum.clone();

            directions.push(NativeHrtfFftDirection {
                left_spectrum,
                right_spectrum,
            });
        }

        Ok(Self {
            block_frames,
            fft_size,
            complex_len,
            inverse_scale: 1.0 / fft_size as f32,
            forward,
            inverse,
            directions,
        })
    }
}

#[derive(Debug, Clone)]
struct NativeHrtfFftSourceState {
    block_frames: usize,
    fft_size: usize,
    forward_input: Vec<f32>,
    input_spectrum: Vec<Complex32>,
    left_spectrum: Vec<Complex32>,
    right_spectrum: Vec<Complex32>,
    left_time: Vec<f32>,
    right_time: Vec<f32>,
    forward_scratch: Vec<Complex32>,
    inverse_scratch: Vec<Complex32>,
}

impl NativeHrtfFftSourceState {
    fn new(plan: &NativeHrtfFftPlan) -> Self {
        Self {
            block_frames: plan.block_frames,
            fft_size: plan.fft_size,
            forward_input: plan.forward.make_input_vec(),
            input_spectrum: plan.forward.make_output_vec(),
            left_spectrum: plan.inverse.make_input_vec(),
            right_spectrum: plan.inverse.make_input_vec(),
            left_time: plan.inverse.make_output_vec(),
            right_time: plan.inverse.make_output_vec(),
            forward_scratch: plan.forward.make_scratch_vec(),
            inverse_scratch: plan.inverse.make_scratch_vec(),
        }
    }

    fn matches_plan(&self, plan: &NativeHrtfFftPlan) -> bool {
        self.block_frames == plan.block_frames && self.fft_size == plan.fft_size
    }
}

/// Per-source convolution state for native HRTF rendering.
#[derive(Debug, Clone)]
pub struct NativeHrtfSourceState {
    delay_line: Vec<f32>,
    write_index: usize,
    cached_direction: Vec3,
    cached_direction_index: Option<usize>,
    rendered_direction_index: Option<usize>,
    fft_state: Option<NativeHrtfFftSourceState>,
}

impl NativeHrtfSourceState {
    fn new(taps: usize, fft_plan: Option<&NativeHrtfFftPlan>) -> Self {
        Self {
            delay_line: vec![0.0; taps],
            write_index: 0,
            cached_direction: DEFAULT_DIRECTION,
            cached_direction_index: None,
            rendered_direction_index: None,
            fft_state: fft_plan.map(NativeHrtfFftSourceState::new),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.delay_line.fill(0.0);
        self.write_index = 0;
        self.cached_direction = DEFAULT_DIRECTION;
        self.cached_direction_index = None;
        self.rendered_direction_index = None;
        if let Some(state) = &mut self.fft_state {
            state.forward_input.fill(0.0);
            state.input_spectrum.fill(Complex32::new(0.0, 0.0));
            state.left_spectrum.fill(Complex32::new(0.0, 0.0));
            state.right_spectrum.fill(Complex32::new(0.0, 0.0));
            state.left_time.fill(0.0);
            state.right_time.fill(0.0);
            state.forward_scratch.fill(Complex32::new(0.0, 0.0));
            state.inverse_scratch.fill(Complex32::new(0.0, 0.0));
        }
    }
}

/// Native HRTF renderer.
///
/// The gameplay path uses a fixed-size frequency-domain overlap-save convolution plan,
/// with the original time-domain FIR kept as a fallback/reference for unusual block sizes.
/// Direction selection remains nearest-neighbor so movement and regression tests stay
/// deterministic. Filter changes crossfade complete convolutions over one render block,
/// with both filters reading the same input history (including pre-change samples).
#[derive(Debug, Clone)]
pub struct NativeHrtfRenderer {
    table: Arc<NativeHrtfTable>,
    fft_plan: Option<Arc<NativeHrtfFftPlan>>,
}

impl NativeHrtfRenderer {
    #[cfg(test)]
    pub fn new(table: Arc<NativeHrtfTable>) -> Self {
        Self {
            table,
            fft_plan: None,
        }
    }

    pub fn with_frame_size(table: Arc<NativeHrtfTable>, frame_size: usize) -> Result<Self> {
        let fft_plan = NativeHrtfFftPlan::new(&table, frame_size)?;
        Ok(Self {
            table,
            fft_plan: Some(Arc::new(fft_plan)),
        })
    }

    pub fn create_source_state(&self) -> NativeHrtfSourceState {
        NativeHrtfSourceState::new(self.table.taps, self.fft_plan.as_deref())
    }

    /// Render a mono source block into an interleaved stereo output buffer.
    ///
    /// Output is accumulated, not cleared, so callers can mix multiple sources.
    #[cfg(test)]
    pub fn render_source(
        &self,
        state: &mut NativeHrtfSourceState,
        direction: Vec3,
        input: &[f32],
        output_interleaved: &mut [f32],
    ) -> Result<()> {
        self.render_source_with_metrics(state, direction, input, output_interleaved)?;
        Ok(())
    }

    /// Render a mono source block and return timing metrics for profiling.
    pub fn render_source_with_metrics(
        &self,
        state: &mut NativeHrtfSourceState,
        direction: Vec3,
        input: &[f32],
        output_interleaved: &mut [f32],
    ) -> Result<NativeHrtfRenderMetrics> {
        let frames = input.len();
        if frames == 0 {
            return Ok(NativeHrtfRenderMetrics::default());
        }
        if output_interleaved.len() < frames * 2 {
            return Err(PetalSonicError::Configuration(format!(
                "native HRTF output buffer too small: need {}, got {} samples",
                frames * 2,
                output_interleaved.len()
            )));
        }

        self.ensure_source_state_compatible(state);
        let (direction_index, direction_lookup_time_us) =
            self.lookup_direction_index(state, direction);
        let convolution_start = Instant::now();

        if let Some(plan) = self.fft_plan.as_deref()
            && input.len() == plan.block_frames
            && let Some(fft_state) = state.fft_state.as_ref()
            && fft_state.matches_plan(plan)
        {
            self.render_source_frequency_domain(
                state,
                plan,
                direction_index,
                input,
                output_interleaved,
            )?;
            state.rendered_direction_index = Some(direction_index);
            return Ok(NativeHrtfRenderMetrics {
                direction_lookup_time_us,
                convolution_time_us: convolution_start.elapsed().as_micros() as u64,
            });
        }

        self.render_source_time_domain(state, direction_index, input, output_interleaved);
        state.rendered_direction_index = Some(direction_index);

        Ok(NativeHrtfRenderMetrics {
            direction_lookup_time_us,
            convolution_time_us: convolution_start.elapsed().as_micros() as u64,
        })
    }

    fn ensure_source_state_compatible(&self, state: &mut NativeHrtfSourceState) {
        if state.delay_line.len() != self.table.taps {
            *state = self.create_source_state();
            return;
        }

        if let Some(plan) = self.fft_plan.as_deref() {
            let needs_fft_state = state
                .fft_state
                .as_ref()
                .is_none_or(|fft_state| !fft_state.matches_plan(plan));
            if needs_fft_state {
                *state = self.create_source_state();
            }
        }
    }

    fn lookup_direction_index(
        &self,
        state: &mut NativeHrtfSourceState,
        direction: Vec3,
    ) -> (usize, u64) {
        let normalized_direction = normalize_direction(direction);
        let lookup_start = Instant::now();
        let direction_index = if let Some(cached_index) = state.cached_direction_index {
            if normalized_direction.dot(state.cached_direction) > 0.999_5 {
                cached_index
            } else {
                let index = self.table.nearest_direction_index(normalized_direction);
                state.cached_direction = normalized_direction;
                state.cached_direction_index = Some(index);
                index
            }
        } else {
            let index = self.table.nearest_direction_index(normalized_direction);
            state.cached_direction = normalized_direction;
            state.cached_direction_index = Some(index);
            index
        };

        (direction_index, lookup_start.elapsed().as_micros() as u64)
    }

    fn render_source_frequency_domain(
        &self,
        state: &mut NativeHrtfSourceState,
        plan: &NativeHrtfFftPlan,
        direction_index: usize,
        input: &[f32],
        output_interleaved: &mut [f32],
    ) -> Result<()> {
        let fft_state = state.fft_state.as_mut().ok_or_else(|| {
            PetalSonicError::SpatialAudio("native HRTF FFT state is not initialized".to_string())
        })?;
        let history = self.table.taps - 1;
        fft_state.forward_input.fill(0.0);
        // The ring includes one extra (oldest) sample; skip it to supply taps-1
        // historical samples in chronological order. This also supports FIR/FFT
        // block-size changes without stale overlap from a different filter.
        for i in 0..history {
            fft_state.forward_input[i] =
                state.delay_line[(state.write_index + 1 + i) % self.table.taps];
        }
        fft_state.forward_input[history..history + input.len()].copy_from_slice(input);
        plan.forward
            .process_with_scratch(
                &mut fft_state.forward_input,
                &mut fft_state.input_spectrum,
                &mut fft_state.forward_scratch,
            )
            .map_err(|error| native_hrtf_fft_error("processing input block", error))?;

        let previous = state.rendered_direction_index.unwrap_or(direction_index);
        let changing = previous != direction_index;
        // Two inverse pairs only while changing direction. No render-time allocation.
        for old in [true, false] {
            if old && !changing {
                continue;
            }
            let hrir = &plan.directions[if old { previous } else { direction_index }];
            for (((left, right), input_bin), (left_hrir, right_hrir)) in fft_state
                .left_spectrum
                .iter_mut()
                .zip(fft_state.right_spectrum.iter_mut())
                .zip(&fft_state.input_spectrum)
                .zip(hrir.left_spectrum.iter().zip(&hrir.right_spectrum))
            {
                *left = *input_bin * *left_hrir;
                *right = *input_bin * *right_hrir;
            }
            force_real_realfft_bins(&mut fft_state.left_spectrum);
            force_real_realfft_bins(&mut fft_state.right_spectrum);
            plan.inverse
                .process_with_scratch(
                    &mut fft_state.left_spectrum,
                    &mut fft_state.left_time,
                    &mut fft_state.inverse_scratch,
                )
                .map_err(|error| native_hrtf_fft_error("inverse left ear", error))?;
            plan.inverse
                .process_with_scratch(
                    &mut fft_state.right_spectrum,
                    &mut fft_state.right_time,
                    &mut fft_state.inverse_scratch,
                )
                .map_err(|error| native_hrtf_fft_error("inverse right ear", error))?;
            for frame in 0..input.len() {
                let blend = if changing {
                    (frame + 1) as f32 / input.len() as f32
                } else {
                    1.0
                };
                let weight = if old { 1.0 - blend } else { blend };
                output_interleaved[2 * frame] +=
                    fft_state.left_time[history + frame] * plan.inverse_scale * weight;
                output_interleaved[2 * frame + 1] +=
                    fft_state.right_time[history + frame] * plan.inverse_scale * weight;
            }
        }
        append_delay_line(&mut state.delay_line, &mut state.write_index, input);
        Ok(())
    }
    fn render_source_time_domain(
        &self,
        state: &mut NativeHrtfSourceState,
        direction_index: usize,
        input: &[f32],
        output_interleaved: &mut [f32],
    ) {
        let hrir = &self.table.directions[direction_index];
        let previous =
            &self.table.directions[state.rendered_direction_index.unwrap_or(direction_index)];
        let taps = self.table.taps;

        for (frame_index, input_sample) in input.iter().copied().enumerate() {
            let blend = (frame_index + 1) as f32 / input.len() as f32;
            state.delay_line[state.write_index] = input_sample;

            let mut left = 0.0f32;
            let mut right = 0.0f32;
            let mut tap = 0usize;
            for delay_index in (0..=state.write_index).rev() {
                let delayed = state.delay_line[delay_index];
                left +=
                    delayed * (previous.left[tap] + blend * (hrir.left[tap] - previous.left[tap]));
                right += delayed
                    * (previous.right[tap] + blend * (hrir.right[tap] - previous.right[tap]));
                tap += 1;
            }
            for delay_index in (state.write_index + 1..taps).rev() {
                let delayed = state.delay_line[delay_index];
                left +=
                    delayed * (previous.left[tap] + blend * (hrir.left[tap] - previous.left[tap]));
                right += delayed
                    * (previous.right[tap] + blend * (hrir.right[tap] - previous.right[tap]));
                tap += 1;
            }

            let out_index = frame_index * 2;
            output_interleaved[out_index] += left;
            output_interleaved[out_index + 1] += right;

            state.write_index = (state.write_index + 1) % taps;
        }
    }
}

fn fft_convolution_size(block_frames: usize, taps: usize) -> Result<usize> {
    let linear_convolution_len = block_frames
        .checked_add(taps)
        .and_then(|value| value.checked_sub(1))
        .ok_or_else(|| {
            PetalSonicError::Configuration("native HRTF FFT size overflow".to_string())
        })?;
    linear_convolution_len
        .checked_next_power_of_two()
        .ok_or_else(|| PetalSonicError::Configuration("native HRTF FFT size overflow".to_string()))
}

fn native_hrtf_fft_error(context: &str, error: realfft::FftError) -> PetalSonicError {
    PetalSonicError::SpatialAudio(format!("native HRTF FFT {context} failed: {error}"))
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

fn append_delay_line(delay_line: &mut [f32], write_index: &mut usize, input: &[f32]) {
    for sample in input {
        delay_line[*write_index] = *sample;
        *write_index = (*write_index + 1) % delay_line.len();
    }
}

struct PetalHrtfReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PetalHrtfReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self.offset.checked_add(count).ok_or_else(|| {
            PetalSonicError::Configuration("native HRTF read offset overflow".to_string())
        })?;

        let Some(slice) = self.bytes.get(self.offset..end) else {
            return Err(PetalSonicError::Configuration(
                "native HRTF file ended unexpectedly".to_string(),
            ));
        };

        self.offset = end;
        Ok(slice)
    }

    fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes(
            bytes.try_into().expect("u32 slice size"),
        ))
    }

    fn read_f32(&mut self) -> Result<f32> {
        let bytes = self.read_bytes(4)?;
        Ok(f32::from_le_bytes(
            bytes.try_into().expect("f32 slice size"),
        ))
    }

    fn finish(&self) -> Result<()> {
        if self.offset != self.bytes.len() {
            return Err(PetalSonicError::Configuration(format!(
                "native HRTF file has {} trailing bytes",
                self.bytes.len() - self.offset
            )));
        }

        Ok(())
    }
}

fn validate_petalhrtf_size(bytes_len: usize, direction_count: usize, taps: usize) -> Result<()> {
    let taps_per_direction = taps.checked_mul(2).ok_or_else(|| {
        PetalSonicError::Configuration("native HRTF tap count overflow".to_string())
    })?;
    let floats_per_direction = 3usize.checked_add(taps_per_direction).ok_or_else(|| {
        PetalSonicError::Configuration("native HRTF direction size overflow".to_string())
    })?;
    let bytes_per_direction = floats_per_direction.checked_mul(F32_BYTES).ok_or_else(|| {
        PetalSonicError::Configuration("native HRTF direction byte size overflow".to_string())
    })?;
    let directions_bytes = direction_count
        .checked_mul(bytes_per_direction)
        .ok_or_else(|| {
            PetalSonicError::Configuration("native HRTF table byte size overflow".to_string())
        })?;
    let expected_bytes = PETALHRTF_HEADER_BYTES
        .checked_add(directions_bytes)
        .ok_or_else(|| {
            PetalSonicError::Configuration("native HRTF total byte size overflow".to_string())
        })?;

    if bytes_len != expected_bytes {
        return Err(PetalSonicError::Configuration(format!(
            "native HRTF file size mismatch: expected {expected_bytes} bytes, got {bytes_len}"
        )));
    }

    Ok(())
}

fn normalize_direction(direction: Vec3) -> Vec3 {
    if direction.is_finite() && direction.length_squared() > f32::EPSILON {
        direction.normalize()
    } else {
        DEFAULT_DIRECTION
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rotation_only_filter_boundary_is_continuous() {
        for fft in [false, true] {
            let table = Arc::new(
                NativeHrtfTable::new(
                    48_000,
                    vec![
                        NativeHrtfDirection::new(Vec3::X, vec![1.0; 16], vec![0.2; 16]),
                        NativeHrtfDirection::new(Vec3::NEG_X, vec![0.2; 16], vec![1.0; 16]),
                    ],
                )
                .unwrap(),
            );
            let renderer = if fft {
                NativeHrtfRenderer::with_frame_size(table, 64).unwrap()
            } else {
                NativeHrtfRenderer::new(table)
            };
            let mut state = renderer.create_source_state();
            let input = [0.015625; 64];
            let mut previous = [0.0; 128];
            renderer
                .render_source(&mut state, Vec3::X, &input, &mut previous)
                .unwrap();
            for direction in [Vec3::NEG_X, Vec3::X, Vec3::NEG_X] {
                let mut next = [0.0; 128];
                renderer
                    .render_source(&mut state, direction, &input, &mut next)
                    .unwrap();
                for ear in 0..2 {
                    let jump = (next[ear] - previous[126 + ear]).abs();
                    assert!(jump < 0.005, "fft={fft}, boundary jump={jump}");
                    for frame in 1..64 {
                        assert!(
                            (next[2 * frame + ear] - next[2 * (frame - 1) + ear]).abs() < 0.005
                        );
                    }
                }
                previous = next;
            }
        }
    }

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_renderer() -> (NativeHrtfRenderer, NativeHrtfSourceState) {
        let table = NativeHrtfTable::new(
            48_000,
            vec![NativeHrtfDirection::new(
                Vec3::NEG_Z,
                vec![1.0, 0.25],
                vec![0.5, -0.25],
            )],
        )
        .unwrap();
        let renderer = NativeHrtfRenderer::new(Arc::new(table));
        let state = renderer.create_source_state();
        (renderer, state)
    }

    fn unique_test_suffix() -> usize {
        static NEXT_SUFFIX: AtomicUsize = AtomicUsize::new(0);
        NEXT_SUFFIX.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    #[ignore = "release real-table rotation sweep and timing probe"]
    fn real_table_rotation_sweep_matches_fir() {
        let table = Arc::new(
            NativeHrtfTable::from_petalhrtf_bytes(include_bytes!(
                "../../asset/hrtf/hrtf_b_nh172.petalhrtf"
            ))
            .unwrap(),
        );
        let fft = NativeHrtfRenderer::with_frame_size(table.clone(), 512).unwrap();
        let fir = NativeHrtfRenderer::new(table);
        let mut fft_state = fft.create_source_state();
        let mut fir_state = fir.create_source_state();
        let mut timings = Vec::new();
        let mut max_error = 0.0f32;
        for block in 0..128 {
            let yaw = block as f32 * 0.071;
            let direction = Vec3::new(yaw.sin(), 0.1, yaw.cos());
            let input: Vec<_> = (0..512)
                .map(|i| ((block * 512 + i) as f32 * 0.61).sin() * 0.2)
                .collect();
            let mut actual = [0.0; 1024];
            let mut expected = [0.0; 1024];
            let start = Instant::now();
            fft.render_source(&mut fft_state, direction, &input, &mut actual)
                .unwrap();
            timings.push(start.elapsed().as_micros());
            fir.render_source(&mut fir_state, direction, &input, &mut expected)
                .unwrap();
            for (a, e) in actual.iter().zip(expected) {
                assert!(a.is_finite());
                max_error = max_error.max((a - e).abs());
            }
        }
        timings.sort_unstable();
        eprintln!(
            "real HRIR rotation: max FFT/FIR error={max_error}, p95={}us / 512 frames",
            timings[121]
        );
        assert!(max_error < 0.0001);
    }

    #[test]
    fn changing_filters_matches_complete_history_convolutions() {
        // Longer-than-block tails, rapid direction reversals, and alternating
        // FFT/FIR sizes must all use identical sample history for both filters.
        let table = Arc::new(
            NativeHrtfTable::new(
                48_000,
                vec![
                    NativeHrtfDirection::new(Vec3::X, vec![0.03; 23], vec![-0.01; 23]),
                    NativeHrtfDirection::new(Vec3::NEG_X, vec![-0.02; 23], vec![0.04; 23]),
                ],
            )
            .unwrap(),
        );
        let renderer = NativeHrtfRenderer::with_frame_size(table.clone(), 8).unwrap();
        let reference = NativeHrtfRenderer::new(table);
        let mut actual = renderer.create_source_state();
        let mut fixed_a = reference.create_source_state();
        let mut fixed_b = reference.create_source_state();
        let mut previous = 0;
        for block in 0..30 {
            let frames = if block % 3 == 0 { 3 } else { 8 };
            let input: Vec<_> = (0..frames)
                .map(|i| ((block * 7 + i) as f32 * 0.3).sin())
                .collect();
            let mut a = vec![0.0; frames * 2];
            let mut b = a.clone();
            reference
                .render_source(&mut fixed_a, Vec3::X, &input, &mut a)
                .unwrap();
            reference
                .render_source(&mut fixed_b, Vec3::NEG_X, &input, &mut b)
                .unwrap();
            let current = block % 2;
            let direction = if current == 0 { Vec3::X } else { Vec3::NEG_X };
            let mut output = vec![0.25; frames * 2];
            renderer
                .render_source(&mut actual, direction, &input, &mut output)
                .unwrap();
            for i in 0..output.len() {
                let old = if previous == 0 { a[i] } else { b[i] };
                let new = if current == 0 { a[i] } else { b[i] };
                let blend = (i / 2 + 1) as f32 / frames as f32;
                let expected = 0.25 + old + blend * (new - old);
                assert!(
                    (output[i] - expected).abs() < 0.00001,
                    "block={block}, sample={i}"
                );
            }
            // An empty render must not consume a direction change or history.
            renderer
                .render_source(&mut actual, -direction, &[], &mut [])
                .unwrap();
            previous = current;
        }
        actual.reset();
        let mut fresh = renderer.create_source_state();
        let mut reset_output = [0.0; 16];
        let mut fresh_output = [0.0; 16];
        renderer
            .render_source(&mut actual, Vec3::NEG_X, &[0.5; 8], &mut reset_output)
            .unwrap();
        renderer
            .render_source(&mut fresh, Vec3::NEG_X, &[0.5; 8], &mut fresh_output)
            .unwrap();
        assert_eq!(reset_output, fresh_output);
    }

    #[test]
    fn table_validates_shape_and_sample_rate() {
        assert!(NativeHrtfTable::new(0, Vec::new()).is_err());
        assert!(NativeHrtfTable::new(48_000, Vec::new()).is_err());
        assert!(
            NativeHrtfTable::new(
                48_000,
                vec![NativeHrtfDirection::new(
                    Vec3::NEG_Z,
                    vec![1.0],
                    vec![1.0, 0.0],
                )],
            )
            .is_err()
        );
    }

    #[test]
    fn petalhrtf_bytes_round_trip_table_data() {
        let table = NativeHrtfTable::new(
            44_100,
            vec![
                NativeHrtfDirection::new(Vec3::X * 2.0, vec![1.0, 0.5], vec![0.25, 0.0]),
                NativeHrtfDirection::new(Vec3::NEG_Z, vec![0.0, -0.5], vec![1.0, 0.125]),
            ],
        )
        .unwrap();

        let decoded = NativeHrtfTable::from_petalhrtf_bytes(&table.to_petalhrtf_bytes()).unwrap();

        assert_eq!(decoded.sample_rate(), 44_100);
        assert_eq!(decoded.direction_count(), 2);
        assert_eq!(decoded.taps(), 2);
        let first = decoded.direction(0).unwrap();
        assert!((first.direction - Vec3::X).length() < 1e-6);
        assert_eq!(first.left, [1.0, 0.5]);
        assert_eq!(first.right, [0.25, 0.0]);
    }

    #[test]
    fn petalhrtf_file_loader_reads_round_trip_bytes() {
        let table = NativeHrtfTable::new(
            48_000,
            vec![NativeHrtfDirection::new(Vec3::NEG_Z, vec![1.0], vec![0.5])],
        )
        .unwrap();
        let path = std::env::temp_dir().join(format!(
            "petalsonic-test-{}-{}.petalhrtf",
            std::process::id(),
            unique_test_suffix()
        ));

        std::fs::write(&path, table.to_petalhrtf_bytes()).unwrap();
        let decoded = NativeHrtfTable::from_petalhrtf_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(decoded.sample_rate(), 48_000);
        assert_eq!(decoded.direction_count(), 1);
        assert_eq!(decoded.taps(), 1);
    }

    #[test]
    fn petalhrtf_rejects_bad_magic_version_and_size() {
        let table = NativeHrtfTable::new(
            48_000,
            vec![NativeHrtfDirection::new(Vec3::NEG_Z, vec![1.0], vec![1.0])],
        )
        .unwrap();
        let mut bytes = table.to_petalhrtf_bytes();

        let mut bad_magic = bytes.clone();
        bad_magic[0] = b'X';
        assert!(NativeHrtfTable::from_petalhrtf_bytes(&bad_magic).is_err());

        let mut bad_version = bytes.clone();
        bad_version[8..12].copy_from_slice(&999u32.to_le_bytes());
        assert!(NativeHrtfTable::from_petalhrtf_bytes(&bad_version).is_err());

        bytes.push(0);
        assert!(NativeHrtfTable::from_petalhrtf_bytes(&bytes).is_err());
    }

    #[test]
    fn nearest_direction_uses_largest_dot_product() {
        let table = NativeHrtfTable::new(
            48_000,
            vec![
                NativeHrtfDirection::new(Vec3::X, vec![1.0], vec![0.0]),
                NativeHrtfDirection::new(Vec3::NEG_X, vec![0.0], vec![1.0]),
            ],
        )
        .unwrap();

        assert_eq!(table.nearest_direction_index(Vec3::new(10.0, 0.0, 0.1)), 0);
        assert_eq!(table.nearest_direction_index(Vec3::new(-2.0, 0.0, 0.0)), 1);
    }

    #[test]
    fn render_source_convolves_and_accumulates_stereo_output() {
        let (renderer, mut state) = test_renderer();
        let input = [1.0, 0.0, 0.0];
        let mut output = [10.0, 20.0, 0.0, 0.0, 0.0, 0.0];

        renderer
            .render_source(&mut state, Vec3::NEG_Z, &input, &mut output)
            .unwrap();

        assert!((output[0] - 11.0).abs() < 1e-6);
        assert!((output[1] - 20.5).abs() < 1e-6);
        assert!((output[2] - 0.25).abs() < 1e-6);
        assert!((output[3] + 0.25).abs() < 1e-6);
        assert_eq!(output[4], 0.0);
        assert_eq!(output[5], 0.0);
    }

    #[test]
    fn render_source_preserves_delay_line_across_blocks() {
        let (renderer, mut state) = test_renderer();
        let mut first = [0.0; 2];
        renderer
            .render_source(&mut state, Vec3::NEG_Z, &[1.0], &mut first)
            .unwrap();

        let mut second = [0.0; 2];
        renderer
            .render_source(&mut state, Vec3::NEG_Z, &[0.0], &mut second)
            .unwrap();

        assert!((second[0] - 0.25).abs() < 1e-6);
        assert!((second[1] + 0.25).abs() < 1e-6);
    }

    #[test]
    fn frequency_domain_renderer_matches_time_domain_across_blocks() {
        let table = NativeHrtfTable::new(
            48_000,
            vec![NativeHrtfDirection::new(
                Vec3::NEG_Z,
                vec![0.75, -0.2, 0.05],
                vec![0.5, 0.125, -0.25],
            )],
        )
        .unwrap();
        let time_renderer = NativeHrtfRenderer::new(Arc::new(table.clone()));
        let fft_renderer = NativeHrtfRenderer::with_frame_size(Arc::new(table), 8).unwrap();
        let mut time_state = time_renderer.create_source_state();
        let mut fft_state = fft_renderer.create_source_state();

        for input in [
            [0.1, -0.25, 0.5, 1.0, -0.75, 0.0, 0.33, -0.12],
            [0.0, 0.25, -0.5, 0.75, 0.125, -0.33, 0.2, 0.0],
        ] {
            let mut time_output = [0.0f32; 16];
            let mut fft_output = [0.0f32; 16];
            time_renderer
                .render_source(&mut time_state, Vec3::NEG_Z, &input, &mut time_output)
                .unwrap();
            fft_renderer
                .render_source(&mut fft_state, Vec3::NEG_Z, &input, &mut fft_output)
                .unwrap();

            for (time_sample, fft_sample) in time_output.iter().zip(fft_output) {
                assert!((*time_sample - fft_sample).abs() < 1e-5);
            }
        }
    }
}
