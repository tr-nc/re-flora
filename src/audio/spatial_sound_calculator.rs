use crate::{
    audio::audio_buffer::AudioBuffer as WrappedAudioBuffer,
    gameplay::camera::vectors::CameraVectors, util::get_project_root,
};
use anyhow::Result;
use audionimbus::*;
use glam::Vec3;
use kira::sound::static_sound::StaticSoundData;
use kira::Frame as KiraFrame;
use ringbuf::traits::*;
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
    let sofa_path = format!("{}assets/hrtf/hrtf_b_nh172.sofa", get_project_root());
    let hrtf_data = std::fs::read(&sofa_path)?;

    let hrtf = Hrtf::try_new(
        context,
        audio_settings,
        &HrtfSettings {
            volume_normalization: VolumeNormalization::RootMeanSquared,
            sofa_information: Some(Sofa::Buffer(hrtf_data)),
            ..Default::default()
        },
    )?;
    Ok(hrtf)
}

fn create_effects(
    context: &Context,
    audio_settings: &AudioSettings,
    hrtf: &Hrtf,
) -> Result<(
    DirectEffect,
    BinauralEffect,
    AmbisonicsEncodeEffect,
    AmbisonicsDecodeEffect,
)> {
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

    let ambisonics_encode_effect = AmbisonicsEncodeEffect::try_new(
        &context,
        audio_settings,
        &AmbisonicsEncodeEffectSettings { max_order: 2 },
    )?;

    let ambisonics_decode_effect = AmbisonicsDecodeEffect::try_new(
        &context,
        audio_settings,
        &AmbisonicsDecodeEffectSettings {
            max_order: 2,
            speaker_layout: SpeakerLayout::Stereo,
            hrtf: &hrtf,
        },
    )?;

    Ok((
        direct_effect,
        binaural_effect,
        ambisonics_encode_effect,
        ambisonics_decode_effect,
    ))
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

struct SpatialSoundCalculatorInner {
    ring_buffer: HeapRb<RingBufferSample>,
    input_cursor_pos: usize,
    available_samples: usize,

    update_frame_window_size: usize,

    #[allow(dead_code)]
    context: Context,

    hrtf: Hrtf,

    #[allow(dead_code)]
    audio_settings: AudioSettings,
    #[allow(dead_code)]
    source: Source,
    #[allow(dead_code)]
    number_of_frames: usize,
    #[allow(dead_code)]
    sample_rate: u32,

    direct_effect: DirectEffect,
    binaural_effect: BinauralEffect,
    ambisonics_encode_effect: AmbisonicsEncodeEffect,
    ambisonics_decode_effect: AmbisonicsDecodeEffect,

    simulator: Arc<Mutex<Simulator<Direct>>>,

    player_position: Arc<Mutex<Vec3>>,
    target_position: Arc<Mutex<Vec3>>,
    player_vectors: Arc<Mutex<CameraVectors>>,

    input_buf: Vec<Sample>,

    cached_input_buf: WrappedAudioBuffer,
    cached_direct_buf: WrappedAudioBuffer,
    cached_binaural_buf: WrappedAudioBuffer,
    cached_ambisonics_encode_buf: WrappedAudioBuffer,
    cached_ambisonics_decode_buf: WrappedAudioBuffer,

    distance_attenuation: f32,
    air_absorption: Vec3,
}

#[derive(Clone)]
pub struct SpatialSoundCalculator(Arc<Mutex<SpatialSoundCalculatorInner>>);

impl SpatialSoundCalculator {
    pub fn new(ring_buffer_size: usize, context: Context, update_frame_window_size: usize) -> Self {
        let ring_buffer = HeapRb::<RingBufferSample>::new(ring_buffer_size);

        let (input_buf, sample_rate, number_of_frames) =
            get_audio_data("assets/sfx/Tree Gusts/WINDGust_Wind, Gust in Trees 01_SARM_Wind.wav");

        log::debug!("using sample_rate: {}", sample_rate);

        let audio_settings = AudioSettings {
            sampling_rate: sample_rate as usize,
            frame_size: update_frame_window_size,
        };

        let hrtf = create_hrtf(&context, &audio_settings).unwrap();
        let (direct_effect, binaural_effect, ambisonics_encode_effect, ambisonics_decode_effect) =
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

        let cached_input_buf =
            WrappedAudioBuffer::new(context.clone(), update_frame_window_size, 1).unwrap();
        let cached_direct_buf =
            WrappedAudioBuffer::new(context.clone(), update_frame_window_size, 1).unwrap();
        let cached_binaural_buf =
            WrappedAudioBuffer::new(context.clone(), update_frame_window_size, 2).unwrap();
        // 9 channels for order 2
        let cached_ambisonics_encode_buf =
            WrappedAudioBuffer::new(context.clone(), update_frame_window_size, 9).unwrap();
        let cached_ambisonics_decode_buf =
            WrappedAudioBuffer::new(context.clone(), update_frame_window_size, 2).unwrap();

        let inner = SpatialSoundCalculatorInner {
            ring_buffer,
            update_frame_window_size,
            available_samples: 0,
            context,
            audio_settings,
            hrtf,
            sample_rate,
            input_buf,
            number_of_frames,
            cached_input_buf,
            cached_direct_buf,
            cached_binaural_buf,
            cached_ambisonics_encode_buf,
            cached_ambisonics_decode_buf,
            direct_effect,
            binaural_effect,
            ambisonics_encode_effect,
            ambisonics_decode_effect,
            simulator: Arc::new(Mutex::new(simulator)),
            player_position: Arc::new(Mutex::new(Vec3::ZERO)),
            target_position: Arc::new(Mutex::new(Vec3::ZERO)),
            player_vectors: Arc::new(Mutex::new(CameraVectors::new())),
            input_cursor_pos: 0,
            distance_attenuation: 1.0,
            air_absorption: Vec3::ZERO,
            source,
        };

        Self(Arc::new(Mutex::new(inner)))
    }

    /// Calling this function will return a slice of RingBufferSample, obtained from the ring buffer.
    ///
    /// The slice will begin at the last retrieved sample position's end, and
    /// will be of length num_samples.
    ///
    /// When the ring buffer has not enough fresh samples, this function will automatically
    /// call the update function to have enough fresh samples.
    pub fn fill_samples(&self, out: &mut [kira::Frame], device_sampling_rate: f64) {
        // target sampling rate may be different from the device sampling rate
        // we have to use a resampler like robato later on for this case
        // but for now, we just assert that the device sampling rate is the same as the target sampling rate

        let sampling_rate = self.0.lock().unwrap().sample_rate;
        assert_eq!(device_sampling_rate, sampling_rate as f64);

        let num_samples = out.len();

        while !self.has_enough_samples(num_samples) {
            let start_time = std::time::Instant::now();
            self.update();
            let end_time = std::time::Instant::now();
            let duration = end_time.duration_since(start_time);
            // log::debug!("update time: {:?}", duration);
        }

        let mut inner = self.0.lock().unwrap();
        let (_, mut consumer) = inner.ring_buffer.split_ref();

        // pop samples from ring buffer into temp buffer
        let mut samples_consumed = 0;
        for i in 0..num_samples {
            if let Some(sample) = consumer.try_pop() {
                out[i] = sample.frame;
                samples_consumed += 1;
            } else {
                // shouldn't happen since we checked has_enough_samples
                break;
            }
        }
        // drop the consumer to release the borrow before updating available_samples
        drop(consumer);
        inner.available_samples = inner.available_samples.saturating_sub(samples_consumed);
    }

    pub fn has_enough_samples(&self, num_samples: usize) -> bool {
        let inner = self.0.lock().unwrap();
        inner.available_samples >= num_samples
    }

    /// Calling this function will update the ring buffer at the current cursor position, with update_frame_window_size frames.
    pub fn update(&self) {
        let mut inner = self.0.lock().unwrap();
        let frames_to_copy = inner.update_frame_window_size.min(inner.input_buf.len());

        let mut input_chunk = Vec::with_capacity(frames_to_copy);

        for i in 0..frames_to_copy {
            let input_index = (inner.input_cursor_pos + i) % inner.input_buf.len();
            input_chunk.push(inner.input_buf[input_index]);
        }

        // use cached input buffer and set its data
        inner.cached_input_buf.set_data(&input_chunk).unwrap();

        inner.apply_direct_effect();
        // inner.apply_binaural_effect();
        // let binaural_processed = inner.cached_binaural_buf.to_interleaved();

        inner.apply_ambisonics_encode_effect();
        inner.apply_ambisonics_decode_effect();
        let binaural_processed = inner.cached_ambisonics_decode_buf.to_interleaved();

        // capture values before borrowing ring buffer
        let input_buf_len = inner.input_buf.len();
        let current_cursor = inner.input_cursor_pos;

        // now get the ring buffer producer after processing
        let (mut producer, _) = inner.ring_buffer.split_ref();

        // convert processed audio to ring buffer samples
        let max_frames = (binaural_processed.len() / 2).min(frames_to_copy);
        let mut samples_added = 0;
        for i in 0..max_frames {
            let ring_buffer_sample = RingBufferSample {
                frame: KiraFrame {
                    left: binaural_processed[i * 2],
                    right: binaural_processed[i * 2 + 1],
                },
            };

            if producer.try_push(ring_buffer_sample).is_ok() {
                samples_added += 1;
            } else {
                // ring buffer is full
                break;
            }
        }

        // update available samples and cursor position after the ring buffer operations
        drop(producer);
        inner.available_samples += samples_added;
        inner.input_cursor_pos = (current_cursor + frames_to_copy) % input_buf_len;
    }
}

impl SpatialSoundCalculatorInner {
    fn apply_direct_effect(&mut self) {
        let direct_effect_params = DirectEffectParams {
            distance_attenuation: Some(self.distance_attenuation),
            air_absorption: Some(Equalizer([
                self.air_absorption.x,
                self.air_absorption.y,
                self.air_absorption.z,
            ])),
            directivity: None,
            occlusion: None,
            transmission: None,
        };

        let _effect_state = self.direct_effect.apply(
            &direct_effect_params,
            &self.cached_input_buf.as_raw(),
            &self.cached_direct_buf.as_raw(),
        );
    }

    fn apply_binaural_effect(&mut self) {
        let normalized_direction = self.get_target_direction().normalize();

        let binaural_effect_params = BinauralEffectParams {
            direction: Direction::new(
                normalized_direction.x,
                normalized_direction.y,
                normalized_direction.z,
            ),
            interpolation: HrtfInterpolation::Bilinear,
            spatial_blend: 1.0,
            hrtf: &self.hrtf,
            peak_delays: None,
        };

        let _effect_state = self.binaural_effect.apply(
            &binaural_effect_params,
            &self.cached_direct_buf.as_raw(),
            &self.cached_binaural_buf.as_raw(),
        );
    }

    fn apply_ambisonics_encode_effect(&mut self) {
        // don't need to normalize here, the lib will do it for us
        let dir = self.get_target_direction();

        let ambisonics_encode_effect_params = AmbisonicsEncodeEffectParams {
            direction: Direction::new(dir.x, dir.y, dir.z),
            order: 2,
        };

        let _effect_state = self.ambisonics_encode_effect.apply(
            &ambisonics_encode_effect_params,
            &self.cached_direct_buf.as_raw(),
            &self.cached_ambisonics_encode_buf.as_raw(),
        );
    }

    fn apply_ambisonics_decode_effect(&mut self) {
        let ambisonics_decode_effect_params = AmbisonicsDecodeEffectParams {
            order: 2,
            hrtf: &self.hrtf,
            orientation: CoordinateSystem {
                // written in the document
                ahead: Vector3::new(0.0, 0.0, -1.0),
                ..Default::default()
            },
            binaural: true,
        };
        let _effect_state = self.ambisonics_decode_effect.apply(
            &ambisonics_decode_effect_params,
            &self.cached_ambisonics_encode_buf.as_raw(),
            &self.cached_ambisonics_decode_buf.as_raw(),
        );
    }

    fn get_target_direction(&self) -> Vec3 {
        let player_position = *self.player_position.lock().unwrap();
        let target_position = *self.target_position.lock().unwrap();
        let player_vectors = self.player_vectors.lock().unwrap();

        let target_direction = (target_position - player_position).normalize();

        let dir = Vec3::new(
            target_direction.dot(player_vectors.right),
            target_direction.dot(player_vectors.up),
            target_direction.dot(player_vectors.front),
        );

        return dir;
    }
}

impl SpatialSoundCalculator {
    pub fn update_player_pos(
        &self,
        player_pos: Vec3,
        camera_vectors: &CameraVectors,
    ) -> Result<()> {
        let inner = self.0.lock().unwrap();
        let old_pos = *inner.player_position.lock().unwrap();
        let old_vectors = inner.player_vectors.lock().unwrap().clone();

        if old_pos != player_pos || old_vectors != *camera_vectors {
            *inner.player_position.lock().unwrap() = player_pos;
            *inner.player_vectors.lock().unwrap() = camera_vectors.clone();
            drop(inner);
            self.update_simulation()?;
        }
        Ok(())
    }

    pub fn update_target_pos(&self, target_pos: Vec3) -> Result<()> {
        let inner = self.0.lock().unwrap();
        let old_pos = *inner.target_position.lock().unwrap();
        if old_pos != target_pos {
            *inner.target_position.lock().unwrap() = target_pos;
            log::debug!("Target position updated: {:?} -> {:?}", old_pos, target_pos);
            drop(inner);
            self.update_simulation()?;
        }
        Ok(())
    }

    fn update_simulation(&self) -> Result<()> {
        let mut inner = self.0.lock().unwrap();
        let player_pos = *inner.player_position.lock().unwrap();
        let target_pos = *inner.target_position.lock().unwrap();
        let player_vectors = inner.player_vectors.lock().unwrap().clone();

        let mut simulator = inner.simulator.lock().unwrap();

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
                right: Vector3::new(
                    player_vectors.right.x,
                    player_vectors.right.y,
                    player_vectors.right.z,
                ),
                up: Vector3::new(
                    player_vectors.up.x,
                    player_vectors.up.y,
                    player_vectors.up.z,
                ),
                ahead: Vector3::new(
                    player_vectors.front.x,
                    player_vectors.front.y,
                    player_vectors.front.z,
                ),
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

        // drop the simulator guard to release the borrow
        drop(simulator);

        // update cached parameters
        inner.distance_attenuation = direct_outputs
            .distance_attenuation
            .ok_or(anyhow::anyhow!("Distance attenuation is None"))?;
        let air_absorption = direct_outputs
            .air_absorption
            .as_ref()
            .ok_or(anyhow::anyhow!("Air absorption is None"))?;
        inner.air_absorption = Vec3::new(air_absorption[0], air_absorption[1], air_absorption[2]);

        Ok(())
    }
}
