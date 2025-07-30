use crate::audio::audio_buffer::AudioBuffer as WrappedAudioBuffer;
use anyhow::Result;
use audionimbus::*;
use glam::Vec3;
use kira::info::Info;
use kira::sound::static_sound::StaticSoundData;
use kira::sound::Sound;
use kira::Frame as KiraFrame;
use ringbuf::*;
use std::sync::{Arc, Mutex};

/// Returns: (input, sample_rate, number_of_frames)
fn get_audio_data(path: &str) -> (Vec<Sample>, u32, usize) {
    let audio_data = StaticSoundData::from_file(path).expect("Failed to load audio file");
    let loaded_frames = &audio_data.frames;

    let input: Vec<Sample> = loaded_frames
        .into_iter()
        .map(|frame| frame.left) // use left channel for mono input
        .collect();

    let input_len = input.len();
    (input, audio_data.sample_rate, input_len)
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

pub struct RingBufferSample {
    pub frame: kira::Frame,
}

pub struct SpatialSoundCalculator {
    ring_buffer: HeapRb<RingBufferSample>,
    update_cursor_pos: usize,
    retrieve_cursor_pos: usize,

    update_frame_window_size: usize,

    //
    context: Context,
    audio_settings: AudioSettings,
    hrtf: Hrtf,
    source: Source, // The audio source in the simulator
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

    // Direct effect params (updated periodically)
    distance_attenuation: Option<f32>,
    directivity: Option<f32>,
    occlusion: Option<f32>,
}

impl SpatialSoundCalculator {
    pub fn new(ring_buffer_size: usize, context: Context, update_frame_window_size: usize) -> Self {
        let ring_buffer = HeapRb::<RingBufferSample>::new(ring_buffer_size);

        let (input_buf, sample_rate, number_of_frames) =
            get_audio_data("assets/sfx/leaf_rustling.wav");

        log::debug!("using sample_rate: {}", sample_rate);

        let audio_settings = AudioSettings {
            sampling_rate: sample_rate as usize,
            frame_size: update_frame_window_size,
        };

        let hrtf = create_hrtf(&context, &audio_settings).unwrap();
        let (direct_effect, binaural_effect) =
            create_effects(&context, &audio_settings, &hrtf).unwrap();

        let mut simulator =
            create_simulator(&context, update_frame_window_size, sample_rate).unwrap();
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

        Self {
            ring_buffer,
            update_frame_window_size,
            context,
            audio_settings,
            hrtf,
            sample_rate,
            input_buf,
            number_of_frames,
            direct_effect,
            binaural_effect,
            simulator: Arc::new(Mutex::new(simulator)),
            player_position: Arc::new(Mutex::new(Vec3::ZERO)),
            target_position: Arc::new(Mutex::new(Vec3::ZERO)),
            update_cursor_pos: 0,
            retrieve_cursor_pos: 0,
            distance_attenuation: Some(1.0),
            directivity: Some(1.0),
            occlusion: Some(1.0),
            source,
        }
    }

    /// Calling this function will return a slice of RingBufferSample, obtained from the ring buffer.
    ///
    /// The slice will begin at the last retrieved sample position's end, and
    /// will be of length num_samples.
    ///
    /// When the ring buffer has not enough fresh samples, this function will automatically
    /// call the update function to have enough fresh samples.
    pub fn get_samples(&self, num_samples: usize) -> &[RingBufferSample] {
        // TODO:
        todo!()
    }

    pub fn has_enough_samples(&self, num_samples: usize) -> bool {
        // TODO:
        todo!()
    }

    /// Calling this function will update the ring buffer at the current cursor position, with update_frame_window_size frames.
    pub fn update(&mut self) {
        // TODO: for simplicity, just carry the input_buf as is to the ring buffer, each time, carry update_frame_window_size frames.
        // no effect should be applied for now.
        todo!()

        // the following code should be commented out for now, don't change it.
        // // Get current positions (locked briefly)
        // let _player_pos = *self.player_position.lock().unwrap();
        // let _target_pos = *self.target_position.lock().unwrap();

        // // Use cached simulation parameters
        // let _sim_outputs = ();

        // // Extract input chunk (pad if needed, as in your process_audio_chunks)
        // let input_chunk = if num_frames < chunk_size {
        //     let mut padded = vec![0.0; chunk_size];
        //     padded[..num_frames]
        //         .copy_from_slice(&self.input_buf[self.cursor_pos..self.cursor_pos + num_frames]);
        //     padded
        // } else {
        //     self.input_buf[self.cursor_pos..self.cursor_pos + chunk_size].to_vec()
        // };

        // let direct_processed = self.apply_direct_effect(self.frame_window_size, 1, &input_chunk);
        // let binaural_processed = self.apply_binaural_effect(&direct_processed);

        // // Advance position (loop if needed)
        // self.cursor_pos += num_frames;
        // if self.cursor_pos >= self.input_buf.len() {
        //     self.cursor_pos = 0; // Loop
        // }

        // // construct output frames in place - ensure we don't go out of bounds
        // let max_frames = (binaural_processed.len() / 2)
        //     .min(num_frames)
        //     .min(out.len());
        // for i in 0..max_frames {
        //     out[i] = KiraFrame {
        //         left: binaural_processed[i * 2],
        //         right: binaural_processed[i * 2 + 1],
        //     };
        // }
        // // Zero out any remaining frames if we didn't fill the entire output buffer
        // for i in max_frames..out.len() {
        //     out[i] = KiraFrame::ZERO;
        // }
    }

    fn apply_direct_effect(&self, output_number_of_channels: usize, input: &[Sample]) -> Vec<f32> {
        let wrapped_output_buffer = WrappedAudioBuffer::new(
            &self.context,
            self.update_frame_window_size,
            output_number_of_channels,
        )
        .unwrap();

        let direct_effect_params = DirectEffectParams {
            distance_attenuation: self.distance_attenuation,
            air_absorption: None, // Can't clone Equalizer<3>
            directivity: self.directivity,
            occlusion: self.occlusion,
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
            WrappedAudioBuffer::new(&self.context, self.update_frame_window_size, 2).unwrap();

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

    pub fn update_positions(&self, player_pos: Vec3, target_pos: Vec3) {
        *self.player_position.lock().unwrap() = player_pos;
        *self.target_position.lock().unwrap() = target_pos;
    }

    pub fn update_simulation(&mut self) -> Result<()> {
        let player_pos = *self.player_position.lock().unwrap();
        let target_pos = *self.target_position.lock().unwrap();

        let mut simulator = self.simulator.lock().unwrap();

        let simulation_inputs = SimulationInputs {
            source: geometry::CoordinateSystem {
                origin: Point::new(target_pos.x, target_pos.y, target_pos.z),
                ..Default::default()
            },
            direct_simulation: Some(DirectSimulationParameters {
                distance_attenuation: Some(DistanceAttenuationModel::Default),
                air_absorption: Some(AirAbsorptionModel::Default),
                directivity: None,
                occlusion: None,
            }),
            reflections_simulation: None,
            pathing_simulation: None,
        };

        let mut source = Source::try_new(
            &*simulator,
            &SourceSettings {
                flags: SimulationFlags::DIRECT,
            },
        )?;
        source.set_inputs(SimulationFlags::DIRECT, simulation_inputs);

        simulator.add_source(&source);
        simulator.commit();

        let simulation_shared_inputs = SimulationSharedInputs {
            listener: geometry::CoordinateSystem {
                origin: Point::new(player_pos.x, player_pos.y, player_pos.z),
                ..Default::default()
            },
            num_rays: 1024,
            num_bounces: 10,
            duration: 3.0,
            order: 2,
            irradiance_min_distance: 1.0,
            pathing_visualization_callback: None,
        };

        simulator.set_shared_inputs(SimulationFlags::DIRECT, &simulation_shared_inputs);
        simulator.run_direct();

        let outputs = source.get_outputs(SimulationFlags::DIRECT);
        let direct_outputs = outputs.direct();

        // Update cached parameters (avoiding storing SimulationOutputs due to Send issues)
        self.distance_attenuation = direct_outputs.distance_attenuation;
        self.directivity = direct_outputs.directivity;
        self.occlusion = direct_outputs.occlusion;

        Ok(())
    }
}
