use crate::{
    audio::audio_decoder::get_audio_data, gameplay::camera::vectors::CameraVectors,
    util::get_project_root,
};
use anyhow::Result;
use audionimbus::{
    audio_buffer::AudioBuffer as AudioNimbusAudioBuffer, geometry, AirAbsorptionModel,
    AmbisonicsDecodeEffect, AmbisonicsDecodeEffectParams, AmbisonicsDecodeEffectSettings,
    AmbisonicsEncodeEffect, AmbisonicsEncodeEffectParams, AmbisonicsEncodeEffectSettings,
    AudioBufferSettings, AudioSettings, Context, CoordinateSystem, Direct, DirectEffect,
    DirectEffectParams, DirectEffectSettings, DirectSimulationParameters, DirectSimulationSettings,
    Direction, DistanceAttenuationModel, Equalizer, Hrtf, HrtfSettings, Point, Scene, SceneParams,
    SceneSettings, SimulationFlags, SimulationInputs, SimulationSharedInputs, Simulator, Sofa,
    Source, SourceSettings, SpeakerLayout, Vector3, VolumeNormalization,
};
use glam::Vec3;
use kira::Frame as KiraFrame;
use ringbuf::{
    traits::{Consumer, Producer, SplitRef},
    HeapRb,
};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayMode {
    Loop,
    SinglePlay,
}

#[derive(Clone, Debug)]
struct SimulationResult {
    distance_attenuation: f32,
    air_absorption: Vec3,
}

impl Default for SimulationResult {
    fn default() -> Self {
        Self {
            distance_attenuation: 1.0,
            air_absorption: Vec3::ONE,
        }
    }
}

#[derive(Clone)]
pub struct SpatialSoundSource {
    position: Vec3,
    volume: f32,
    samples: Vec<f32>,
    sample_rate: u32,
    simulation_result: SimulationResult,
    cursor_pos: usize,
    play_mode: PlayMode,

    source: Source,

    direct_effect: DirectEffect,
    ambisonics_encode_effect: AmbisonicsEncodeEffect,
}

impl Debug for SpatialSoundSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // don't write the samples
        write!(f, "SpatialSoundSource {{ position: {:?}, volume: {:?}, sample_rate: {:?}, simulation_result: {:?}, cursor_pos: {:?}, play_mode: {:?} }}", self.position, self.volume, self.sample_rate, self.simulation_result, self.cursor_pos, self.play_mode)
    }
}

impl SpatialSoundSource {
    pub fn new(
        simulator: &Simulator<Direct>,
        context: &Context,
        frame_window_size: usize,
        path: &str,
        volume: f32,
        position: Vec3,
        play_mode: PlayMode,
    ) -> Result<Self> {
        let (samples, sample_rate) = get_audio_data(path)
            .map_err(|e| anyhow::anyhow!("Failed to load audio file: {}", e))?;

        let audio_settings = AudioSettings {
            sampling_rate: sample_rate as usize,
            frame_size: frame_window_size,
        };

        let direct_effect = DirectEffect::try_new(
            context,
            &audio_settings,
            &DirectEffectSettings { num_channels: 1 },
        )?;

        let ambisonics_encode_effect = AmbisonicsEncodeEffect::try_new(
            context,
            &audio_settings,
            &AmbisonicsEncodeEffectSettings { max_order: 2 },
        )?;

        Ok(Self {
            position,
            volume,
            samples,
            sample_rate,
            simulation_result: SimulationResult::default(),
            cursor_pos: 0,
            play_mode,
            source: Source::try_new(
                &simulator,
                &SourceSettings {
                    flags: SimulationFlags::DIRECT,
                },
            )?,
            direct_effect,
            ambisonics_encode_effect,
        })
    }
}

pub struct SpatialSoundManagerInner {
    ring_buffer: HeapRb<KiraFrame>,
    available_samples: usize,

    sources: HashMap<Uuid, SpatialSoundSource>,

    #[allow(dead_code)]
    context: Context,

    frame_window_size: usize,
    distance_scaler: f32,

    hrtf: Hrtf,

    ambisonics_decode_effect: AmbisonicsDecodeEffect,

    cached_input_buf: Vec<f32>,
    cached_direct_buf: Vec<f32>,
    cached_summed_encoded_buf: Vec<f32>,
    cached_ambisonics_encode_buf: Vec<f32>,
    cached_ambisonics_decode_buf: Vec<f32>,
    cached_binaural_processed: Vec<f32>,

    listener_position: Vec3,
    listener_up: Vec3,
    listener_front: Vec3,
    listener_right: Vec3,

    simulator: Simulator<Direct>,
    scene: Scene,
}

#[derive(Clone)]
pub struct SpatialSoundManager(Arc<Mutex<SpatialSoundManagerInner>>);

impl SpatialSoundManagerInner {
    pub fn new(
        context: Context,
        ring_buffer_size: usize,
        frame_window_size: usize,
        sample_rate: u32,
        distance_scaler: f32,
    ) -> Self {
        let ring_buffer = HeapRb::<KiraFrame>::new(ring_buffer_size);

        let audio_settings = AudioSettings {
            sampling_rate: sample_rate as usize,
            frame_size: frame_window_size,
        };

        let hrtf = create_hrtf(&context, &audio_settings).unwrap();

        let ambisonics_decode_effect = AmbisonicsDecodeEffect::try_new(
            &context,
            &audio_settings,
            &AmbisonicsDecodeEffectSettings {
                max_order: 2,
                speaker_layout: SpeakerLayout::Stereo,
                hrtf: &hrtf,
            },
        )
        .unwrap();

        let mut simulator = create_simulator(&context, frame_window_size, sample_rate).unwrap();
        let scene = Scene::try_new(&context, &SceneSettings::default()).unwrap();
        simulator.set_scene(&scene);
        simulator.commit(); // must be called after set_scene

        let cached_input_buf = vec![0.0; frame_window_size];
        let cached_direct_buf = vec![0.0; frame_window_size];
        // 9 channels for order 2
        let cached_summed_encoded_buf = vec![0.0; frame_window_size * 9];
        let cached_ambisonics_encode_buf = vec![0.0; frame_window_size * 9];
        let cached_ambisonics_decode_buf = vec![0.0; frame_window_size * 2];
        let cached_binaural_processed = vec![0.0; frame_window_size * 2];

        return Self {
            ring_buffer,
            available_samples: 0,
            sources: HashMap::new(),
            context,
            frame_window_size,
            distance_scaler,
            hrtf,
            ambisonics_decode_effect,
            cached_input_buf,
            cached_direct_buf,
            cached_summed_encoded_buf,
            cached_ambisonics_encode_buf,
            cached_ambisonics_decode_buf,
            cached_binaural_processed,
            simulator,
            scene,
            listener_position: Vec3::ZERO,
            listener_up: Vec3::ZERO,
            listener_front: Vec3::ZERO,
            listener_right: Vec3::ZERO,
        };

        fn create_hrtf(context: &Context, audio_settings: &AudioSettings) -> Result<Hrtf> {
            let sofa_path = format!("{}assets/hrtf/hrtf_b_nh172.sofa", get_project_root());
            let hrtf_data = std::fs::read(&sofa_path)?;

            let hrtf = Hrtf::try_new(
                context,
                audio_settings,
                &HrtfSettings {
                    volume_normalization: VolumeNormalization::None,
                    sofa_information: Some(Sofa::Buffer(hrtf_data)),
                    ..Default::default()
                },
            )?;
            Ok(hrtf)
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

    pub fn add_source(&mut self, source: SpatialSoundSource) -> Uuid {
        let mut id = Uuid::new_v4();
        while self.sources.contains_key(&id) {
            log::warn!("Source with this UUID already exists, generating new UUID");
            id = Uuid::new_v4();
        }
        self.sources.insert(id, source);
        let source_ref = self.sources.get(&id).unwrap();
        self.simulator.add_source(&source_ref.source);
        id
    }

    pub fn get_source(&self, id: Uuid) -> Option<&SpatialSoundSource> {
        self.sources.get(&id)
    }

    pub fn remove_source(&mut self, id: Uuid) {
        self.sources.remove(&id);
    }

    fn update(&mut self) -> Result<()> {
        self.simulate()?;

        let all_ids: Vec<Uuid> = self.sources.keys().cloned().collect();
        let mut sources_to_remove = Vec::new();

        self.cached_summed_encoded_buf.fill(0.0);
        self.cached_binaural_processed.fill(0.0);

        for id in &all_ids {
            let (play_mode, cursor_pos, samples, volume) = {
                let source = self.get_source(*id).unwrap();
                (
                    source.play_mode.clone(),
                    source.cursor_pos,
                    source.samples.clone(),
                    source.volume,
                )
            };

            for i in 0..self.frame_window_size {
                let sample = match play_mode {
                    PlayMode::Loop => {
                        let idx = (cursor_pos + i) % samples.len();
                        samples[idx] * volume
                    }
                    PlayMode::SinglePlay => {
                        let idx = cursor_pos + i;
                        if idx < samples.len() {
                            samples[idx] * volume
                        } else {
                            0.0
                        }
                    }
                };
                self.cached_input_buf[i] = sample;
            }

            self.apply_direct_effect(*id);
            self.apply_ambisonics_encode_effect(*id);

            // update cursor position
            let new_cursor_pos = cursor_pos + self.frame_window_size;
            match play_mode {
                PlayMode::Loop => {
                    let source_mut = self.sources.get_mut(id).unwrap();
                    source_mut.cursor_pos = new_cursor_pos % samples.len();
                }
                PlayMode::SinglePlay => {
                    if new_cursor_pos >= samples.len() {
                        sources_to_remove.push(*id);
                    } else {
                        let source_mut = self.sources.get_mut(id).unwrap();
                        source_mut.cursor_pos = new_cursor_pos;
                    }
                }
            }
        }

        for id in sources_to_remove {
            self.remove_source(id);
        }

        // Perform single decode operation on all accumulated encoded sources
        self.apply_ambisonics_decode_effect();

        let decoded_buf = AudioNimbusAudioBuffer::try_with_data_and_settings(
            &mut self.cached_ambisonics_decode_buf,
            &AudioBufferSettings {
                num_channels: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        decoded_buf.interleave(&self.context, &mut self.cached_binaural_processed);

        // now get the ring buffer producer after processing
        let (mut producer, _) = self.ring_buffer.split_ref();

        // convert processed audio to ring buffer samples
        let max_frames = (self.cached_binaural_processed.len() / 2).min(self.frame_window_size);
        let mut samples_added = 0;
        for i in 0..max_frames {
            let ring_buffer_sample = KiraFrame {
                left: self.cached_binaural_processed[i * 2],
                right: self.cached_binaural_processed[i * 2 + 1],
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
        self.available_samples += samples_added;

        Ok(())
    }

    fn apply_direct_effect(&mut self, source_id: Uuid) {
        let source = self.sources.get_mut(&source_id).unwrap();

        let direct_effect_params = DirectEffectParams {
            distance_attenuation: Some(source.simulation_result.distance_attenuation),
            air_absorption: Some(Equalizer([
                source.simulation_result.air_absorption.x,
                source.simulation_result.air_absorption.y,
                source.simulation_result.air_absorption.z,
            ])),
            directivity: None,
            occlusion: None,
            transmission: None,
        };

        let input_buf = AudioNimbusAudioBuffer::try_with_data_and_settings(
            &self.cached_input_buf,
            &AudioBufferSettings {
                num_channels: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        let direct_buf = AudioNimbusAudioBuffer::try_with_data_and_settings(
            &mut self.cached_direct_buf,
            &AudioBufferSettings {
                num_channels: Some(1),
                ..Default::default()
            },
        )
        .unwrap();

        let _effect_state =
            source
                .direct_effect
                .apply(&direct_effect_params, &input_buf, &direct_buf);
    }

    fn apply_ambisonics_encode_effect(&mut self, source_id: Uuid) {
        // don't need to normalize here, the lib will do it for us
        let dir = self.get_target_direction(source_id);

        let ambisonics_encode_effect_params = AmbisonicsEncodeEffectParams {
            direction: Direction::new(dir.x, dir.y, dir.z),
            order: 2,
        };

        let source = self.sources.get_mut(&source_id).unwrap();

        let input_buf = AudioNimbusAudioBuffer::try_with_data_and_settings(
            &self.cached_direct_buf,
            &AudioBufferSettings {
                num_channels: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        let output_buf = AudioNimbusAudioBuffer::try_with_data_and_settings(
            &mut self.cached_ambisonics_encode_buf,
            &AudioBufferSettings {
                num_channels: Some(9),
                ..Default::default()
            },
        )
        .unwrap();

        let _effect_state = source.ambisonics_encode_effect.apply(
            &ambisonics_encode_effect_params,
            &input_buf,
            &output_buf,
        );

        // Add encoded output to summed buffer
        for i in 0..self.cached_ambisonics_encode_buf.len() {
            self.cached_summed_encoded_buf[i] += self.cached_ambisonics_encode_buf[i];
        }
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

        let input_buf = AudioNimbusAudioBuffer::try_with_data_and_settings(
            &self.cached_summed_encoded_buf,
            &AudioBufferSettings {
                num_channels: Some(9),
                ..Default::default()
            },
        )
        .unwrap();
        let output_buf = AudioNimbusAudioBuffer::try_with_data_and_settings(
            &mut self.cached_ambisonics_decode_buf,
            &AudioBufferSettings {
                num_channels: Some(2),
                ..Default::default()
            },
        )
        .unwrap();

        let _effect_state = self.ambisonics_decode_effect.apply(
            &ambisonics_decode_effect_params,
            &input_buf,
            &output_buf,
        );
    }

    fn get_target_direction(&self, target_id: Uuid) -> Vec3 {
        let target = self.get_source(target_id).unwrap();
        let target_position = target.position;
        let target_direction = (target_position - self.listener_position).normalize();
        let dir = Vec3::new(
            target_direction.dot(self.listener_right),
            target_direction.dot(self.listener_up),
            target_direction.dot(self.listener_front),
        );

        return dir;
    }

    fn simulate(&mut self) -> Result<()> {
        for source in self.sources.values_mut() {
            let scaled_position = source.position * self.distance_scaler;
            let simulation_inputs = SimulationInputs {
                source: geometry::CoordinateSystem {
                    origin: Point::new(scaled_position.x, scaled_position.y, scaled_position.z),
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
            source
                .source
                .set_inputs(SimulationFlags::DIRECT, simulation_inputs);
            self.simulator.commit();
        }

        let scaled_listener_position = self.listener_position * self.distance_scaler;
        let simulation_shared_inputs = SimulationSharedInputs {
            listener: geometry::CoordinateSystem {
                origin: Point::new(
                    scaled_listener_position.x,
                    scaled_listener_position.y,
                    scaled_listener_position.z,
                ),
                right: Vector3::new(
                    self.listener_right.x,
                    self.listener_right.y,
                    self.listener_right.z,
                ),
                up: Vector3::new(self.listener_up.x, self.listener_up.y, self.listener_up.z),
                ahead: Vector3::new(
                    self.listener_front.x,
                    self.listener_front.y,
                    self.listener_front.z,
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

        self.simulator
            .set_shared_inputs(SimulationFlags::DIRECT, &simulation_shared_inputs);
        self.simulator.run_direct();

        for source in self.sources.values_mut() {
            let outputs = source.source.get_outputs(SimulationFlags::DIRECT);
            let direct_outputs = outputs.direct();

            // update cached parameters
            source.simulation_result.distance_attenuation = direct_outputs
                .distance_attenuation
                .ok_or(anyhow::anyhow!("Distance attenuation is None"))?;
            let air_absorption = direct_outputs
                .air_absorption
                .as_ref()
                .ok_or(anyhow::anyhow!("Air absorption is None"))?;
            source.simulation_result.air_absorption =
                Vec3::new(air_absorption[0], air_absorption[1], air_absorption[2]);
        }

        Ok(())
    }
}

impl SpatialSoundManager {
    pub fn new(ring_buffer_size: usize, frame_window_size: usize) -> Result<Self> {
        Self::with_distance_scaler(ring_buffer_size, frame_window_size, 10.0)
    }

    pub fn with_distance_scaler(
        ring_buffer_size: usize,
        frame_window_size: usize,
        distance_scaler: f32,
    ) -> Result<Self> {
        let context = Context::try_new(&audionimbus::ContextSettings::default())?;
        let sample_rate = 48000;
        let mut inner = SpatialSoundManagerInner::new(
            context,
            ring_buffer_size,
            frame_window_size,
            sample_rate,
            distance_scaler,
        );

        // Initialize with default listener position and orientation
        let mut dummy_vectors = CameraVectors::new();
        dummy_vectors.update(0.0, 0.0); // Default forward-facing orientation
        inner.listener_position = Vec3::new(0.0, 0.0, 0.0);
        inner.listener_up = dummy_vectors.up;
        inner.listener_front = dummy_vectors.front;
        inner.listener_right = dummy_vectors.right;
        inner.simulate()?;

        Ok(Self(Arc::new(Mutex::new(inner))))
    }

    pub fn add_tree_gust_source(&self, tree_pos: Vec3) -> Result<Uuid> {
        let mut inner = self.0.lock().unwrap();
        let source = SpatialSoundSource::new(
            &inner.simulator,
            &inner.context,
            inner.frame_window_size,
            "assets/sfx/Tree Gusts/WINDGust_Wind, Gust in Trees 01_SARM_Wind.wav",
            1.0,
            tree_pos,
            PlayMode::Loop,
        )?;

        let source_id = inner.add_source(source);
        Ok(source_id)
    }

    pub fn add_single_play_source(&self, path: &str, volume: f32, position: Vec3) -> Result<Uuid> {
        let mut inner = self.0.lock().unwrap();
        let source = SpatialSoundSource::new(
            &inner.simulator,
            &inner.context,
            inner.frame_window_size,
            path,
            volume,
            position,
            PlayMode::SinglePlay,
        )?;

        let source_id = inner.add_source(source);
        Ok(source_id)
    }

    pub fn add_loop_source(&self, path: &str, volume: f32, position: Vec3) -> Result<Uuid> {
        let mut inner = self.0.lock().unwrap();
        let source = SpatialSoundSource::new(
            &inner.simulator,
            &inner.context,
            inner.frame_window_size,
            path,
            volume,
            position,
            PlayMode::Loop,
        )?;

        let source_id = inner.add_source(source);
        Ok(source_id)
    }

    pub fn fill_samples(&self, out: &mut [kira::Frame], device_sampling_rate: f64) {
        // Ensure device sampling rate matches our expected rate
        assert_eq!(
            device_sampling_rate, 48000.0,
            "Device sampling rate must be 48000 Hz"
        );

        let num_samples = out.len();

        while !self.has_enough_samples(num_samples) {
            let now = std::time::Instant::now();
            self.update().unwrap();
            let duration = now.elapsed();
            log::debug!("Simulation time: {:?}", duration);
        }

        let mut inner = self.0.lock().unwrap();
        let (_, mut consumer) = inner.ring_buffer.split_ref();

        let mut samples_consumed = 0;
        for i in 0..num_samples {
            if let Some(sample) = consumer.try_pop() {
                out[i] = sample;
                samples_consumed += 1;
            } else {
                break;
            }
        }
        drop(consumer);
        inner.available_samples = inner.available_samples.saturating_sub(samples_consumed);
    }

    pub fn has_enough_samples(&self, num_samples: usize) -> bool {
        let inner = self.0.lock().unwrap();
        inner.available_samples >= num_samples
    }

    pub fn update(&self) -> Result<()> {
        let mut inner = self.0.lock().unwrap();
        inner.update()
    }

    pub fn update_player_pos(
        &self,
        player_pos: Vec3,
        camera_vectors: &CameraVectors,
    ) -> Result<()> {
        let mut inner = self.0.lock().unwrap();
        let old_pos = inner.listener_position;
        let old_up = inner.listener_up;
        let old_front = inner.listener_front;
        let old_right = inner.listener_right;

        if old_pos != player_pos
            || old_up != camera_vectors.up
            || old_front != camera_vectors.front
            || old_right != camera_vectors.right
        {
            inner.listener_position = player_pos;
            inner.listener_up = camera_vectors.up;
            inner.listener_front = camera_vectors.front;
            inner.listener_right = camera_vectors.right;
            inner.simulate()?;
        }
        Ok(())
    }

    pub fn update_source_pos(&self, source_id: Uuid, target_pos: Vec3) -> Result<()> {
        log::debug!(
            "Updating source {:?} position to {:?}",
            source_id,
            target_pos
        );

        let mut inner = self.0.lock().unwrap();
        if let Some(source) = inner.sources.get_mut(&source_id) {
            let old_pos = source.position;
            if old_pos != target_pos {
                source.position = target_pos;
                log::debug!(
                    "Source {:?} position updated: {:?} -> {:?}",
                    source_id,
                    old_pos,
                    target_pos
                );
                inner.simulate()?;
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn add_source(&self, source: SpatialSoundSource) -> Uuid {
        let mut inner = self.0.lock().unwrap();
        inner.add_source(source)
    }

    #[allow(dead_code)]
    pub fn get_source(&self, id: Uuid) -> Option<SpatialSoundSource> {
        let inner = self.0.lock().unwrap();
        inner.get_source(id).cloned()
    }

    #[allow(dead_code)]
    pub fn remove_source(&self, id: Uuid) {
        let mut inner = self.0.lock().unwrap();
        inner.remove_source(id);
    }
}

#[test]
fn testing() {}
