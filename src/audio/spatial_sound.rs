use anyhow::Result;
use audionimbus::*;
use glam::Vec3;
use image::Frame;
use kira::info::Info;
use kira::sound::static_sound::StaticSoundData;
use kira::sound::{PlaybackState, Region, Sound, SoundData};
use kira::Frame as KiraFrame;
use kira::Tween;
use std::sync::{Arc, Mutex};

use crate::audio::audio_buffer::AudioBuffer as WrappedAudioBuffer;

/// Generic function to process audio in chunks using any effect that can be applied to AudioBuffer
/// F is a closure that takes (input_buffer, iteration_index) and applies an effect to the wrapped_output_buffer
/// Handles cases where the last chunk is smaller than frame_window_size by padding with zeros
fn process_audio_chunks<F>(
    input: &[Sample],
    frame_window_size: usize,
    number_of_frames: usize,
    output_number_of_channels: usize,
    wrapped_output_buffer: &WrappedAudioBuffer,
    mut effect_fn: F,
) -> Vec<f32>
where
    F: FnMut(&AudioBuffer<&[f32]>, usize),
{
    let mut interleaved_output = vec![0.0; number_of_frames * output_number_of_channels];
    let num_of_iterations = number_of_frames.div_ceil(frame_window_size);

    // Pre-allocate reusable buffer for padding (only used when needed)
    let mut padded_chunk = vec![0.0; frame_window_size];

    for i in 0..num_of_iterations {
        let start_idx = i * frame_window_size;
        let end_idx = (start_idx + frame_window_size).min(number_of_frames);
        let actual_chunk_size = end_idx - start_idx;

        // Use input slice directly or pad into reusable buffer
        let input_slice = if actual_chunk_size < frame_window_size {
            // Clear and copy into reusable padded buffer
            padded_chunk.fill(0.0);
            padded_chunk[..actual_chunk_size].copy_from_slice(&input[start_idx..end_idx]);
            padded_chunk.as_slice()
        } else {
            &input[start_idx..end_idx]
        };

        let input_buffer = AudioBuffer::try_with_data(input_slice).unwrap();

        effect_fn(&input_buffer, i);

        let interleaved_frame_output = wrapped_output_buffer.to_interleaved();

        // Copy only the actual output (not the padded part for the last chunk)
        let output_start = i * frame_window_size * output_number_of_channels;
        let output_size = actual_chunk_size * output_number_of_channels;
        let output_end = output_start + output_size;

        interleaved_output[output_start..output_end]
            .copy_from_slice(&interleaved_frame_output[..output_size]);
    }

    interleaved_output
}

// Custom Sound implementation for real-time processing
struct RealTimeSpatialSound {
    context: Context,
    audio_settings: AudioSettings,
    hrtf: Hrtf,
    source: Source, // The audio source in the simulator
    frame_window_size: usize,
    number_of_frames: usize,
    sample_rate: u32,

    direct_effect: DirectEffect,
    binaural_effect: BinauralEffect,

    simulator: Arc<Mutex<Simulator<Direct>>>, // Shared for updates

    // Dynamic state (updated from game loop)
    player_position: Arc<Mutex<Vec3>>,
    target_position: Arc<Mutex<Vec3>>,

    // Buffer for input audio (e.g., from a streaming source)
    input_buf: Vec<Sample>, // Fill this from your audio source (e.g., loop or generate)

    // Current simulation outputs (updated periodically)
    simulation_outputs: Arc<Mutex<SimulationOutputs>>,

    // Position in the audio stream
    cursor_pos: usize,
}

impl Sound for RealTimeSpatialSound {
    fn process(&mut self, out: &mut [kira::Frame], dt: f64, info: &Info) {
        log::debug!("len of out: {}", out.len());
        log::debug!("dt: {:?}", dt);

        let chunk_size = self.frame_window_size;
        let num_frames = chunk_size.min(self.input_buf.len() - self.cursor_pos);

        if num_frames == 0 {
            return;
        }

        // Get current positions (locked briefly)
        let player_pos = *self.player_position.lock().unwrap();
        let target_pos = *self.target_position.lock().unwrap();

        // Get latest simulation outputs (updated externally)
        let sim_outputs = self.simulation_outputs.lock().unwrap();

        // Extract input chunk (pad if needed, as in your process_audio_chunks)
        let input_chunk = if num_frames < chunk_size {
            let mut padded = vec![0.0; chunk_size];
            padded[..num_frames]
                .copy_from_slice(&self.input_buf[self.cursor_pos..self.cursor_pos + num_frames]);
            padded
        } else {
            self.input_buf[self.cursor_pos..self.cursor_pos + chunk_size].to_vec()
        };

        let direct_processed = self.apply_direct_effect(self.frame_window_size, 1, &input_chunk);
        let binaural_processed = self.apply_binaural_effect(&direct_processed);

        // Advance position (loop if needed)
        self.cursor_pos += num_frames;
        if self.cursor_pos >= self.input_buf.len() {
            self.cursor_pos = 0; // Loop
        }

        log::debug!("len of binaural_processed: {}", binaural_processed.len());

        // construct output frames in place
        for i in 0..num_frames {
            out[i] = KiraFrame {
                left: binaural_processed[i * 2],
                right: binaural_processed[i * 2 + 1],
            };
        }
    }

    fn finished(&self) -> bool {
        false // Looping sound
    }
}

// Add methods to the struct
impl RealTimeSpatialSound {
    fn new(context: Context, frame_window_size: usize) -> Result<Self> {
        let (input_buf, sample_rate, number_of_frames) =
            get_audio_data("assets/sfx/leaf_rustling.wav");

        log::debug!("using sample_rate: {}", sample_rate);

        let audio_settings = AudioSettings {
            sampling_rate: sample_rate as usize,
            frame_size: frame_window_size,
        };

        let hrtf = create_hrtf(&context, &audio_settings).unwrap();
        let (direct_effect, binaural_effect) =
            create_effects(&context, &audio_settings, &hrtf).unwrap();

        let mut simulator = create_simulator(&context, frame_window_size, sample_rate).unwrap();
        let scene = Scene::try_new(&context, &SceneSettings::default()).unwrap();
        simulator.set_scene(&scene);
        simulator.commit(); // must be called after set_scene

        let source = Source::try_new(
            &simulator,
            &SourceSettings {
                flags: SimulationFlags::DIRECT,
            },
        )
        .unwrap();
        simulator.add_source(&source);
        simulator.commit(); // must be called after add_source

        return Ok(Self {
            context,
            audio_settings,
            hrtf,
            frame_window_size,
            sample_rate,
            input_buf,
            number_of_frames,
            direct_effect,
            binaural_effect,
            simulator: Arc::new(Mutex::new(simulator)),
            player_position: Arc::new(Mutex::new(Vec3::ZERO)),
            target_position: Arc::new(Mutex::new(Vec3::ZERO)),
            cursor_pos: 0,
            simulation_outputs: Arc::new(Mutex::new(SimulationOutputs::default())),
            source,
        });

        /// Returns: (input, sample_rate, number_of_frames)
        fn get_audio_data(path: &str) -> (Vec<Sample>, u32, usize) {
            let audio_data = StaticSoundData::from_file(path).expect("Failed to load audio file");
            let loaded_frames = &audio_data.frames;

            let input: Vec<Sample> = loaded_frames
                .into_iter()
                .map(|frame| frame.left) // use left channel for mono input
                .collect();

            let input_len = input.len();
            return (input, audio_data.sample_rate, input_len);
        }

        fn create_hrtf(context: &Context, audio_settings: &AudioSettings) -> Result<Hrtf> {
            let hrtf = Hrtf::try_new(context, audio_settings, &HrtfSettings::default())?;
            Ok(hrtf)
        }

        fn create_effects(
            context: &Context,
            audio_settings: &AudioSettings,
            hrtf: &Hrtf,
        ) -> Result<(DirectEffect, BinauralEffect)> {
            let direct_effect = DirectEffect::try_new(
                context,
                audio_settings,
                &DirectEffectSettings { num_channels: 1 },
            )?;

            let binaural_effect = BinauralEffect::try_new(
                &context,
                &audio_settings,
                &BinauralEffectSettings { hrtf: &hrtf },
            )?;
            Ok((direct_effect, binaural_effect))
        }

        fn create_simulator(
            context: &Context,
            frame_window_size: usize,
            sample_rate: u32,
        ) -> Result<Simulator<Direct>> {
            let simulator = Simulator::builder(
                SceneParams::Default,
                sample_rate as usize,
                frame_window_size,
            )
            .with_direct(DirectSimulationSettings {
                max_num_occlusion_samples: 32,
            })
            .try_build(context)?;
            Ok(simulator)
        }
    }

    fn apply_direct_effect(
        &self,
        frame_window_size: usize,
        output_number_of_channels: usize,
        input: &[Sample],
    ) -> Vec<f32> {
        let wrapped_output_buffer =
            WrappedAudioBuffer::new(&self.context, frame_window_size, output_number_of_channels)
                .unwrap();

        let simulation_outputs = self.simulation_outputs.lock().unwrap();
        let direct_outputs = simulation_outputs.direct();

        let direct_effect_params = DirectEffectParams {
            distance_attenuation: direct_outputs.distance_attenuation,
            air_absorption: None, // Can't clone Equalizer<3>
            directivity: direct_outputs.directivity,
            occlusion: direct_outputs.occlusion,
            transmission: None, // Can't clone Transmission
        };

        let input_buffer = AudioBuffer::try_with_data(input).unwrap();
        let _effect_state = self.direct_effect.apply(
            &direct_effect_params,
            &input_buffer,
            &wrapped_output_buffer.as_raw(),
        );
        let interleaved_frame_output = wrapped_output_buffer.to_interleaved();
        return interleaved_frame_output;
    }

    fn apply_binaural_effect(&self, input: &[Sample]) -> Vec<f32> {
        let wrapped_output_buffer =
            WrappedAudioBuffer::new(&self.context, self.frame_window_size, 1).unwrap();

        let player_position = *self.player_position.lock().unwrap();
        let target_position = *self.target_position.lock().unwrap();

        let normalized_direction = (target_position - player_position).normalize();
        let binaural_effect_params = BinauralEffectParams {
            direction: Direction::new(
                normalized_direction.x,
                normalized_direction.y,
                normalized_direction.z,
            ),
            interpolation: HrtfInterpolation::Nearest,
            spatial_blend: 1.0,
            hrtf: &self.hrtf,
            peak_delays: None,
        };

        let input_buffer = AudioBuffer::try_with_data(input).unwrap();
        let _effect_state = self.binaural_effect.apply(
            &binaural_effect_params,
            &input_buffer,
            &wrapped_output_buffer.as_raw(),
        );
        let interleaved_frame_output = wrapped_output_buffer.to_interleaved();

        return interleaved_frame_output;
    }
}
