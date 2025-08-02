use crate::{
    audio::{
        audio_buffer::AudioBuffer, audio_decoder::get_audio_data,
        spatial_sound_calculator::RingBufferSample,
    },
    util::get_project_root,
};
use anyhow::Result;
use audionimbus::{
    geometry, AirAbsorptionModel, AmbisonicsDecodeEffect, AmbisonicsDecodeEffectParams,
    AmbisonicsDecodeEffectSettings, AmbisonicsEncodeEffect, AmbisonicsEncodeEffectParams,
    AmbisonicsEncodeEffectSettings, AudioSettings, BinauralEffect, BinauralEffectSettings, Context,
    CoordinateSystem, Direct, DirectEffect, DirectEffectParams, DirectEffectSettings,
    DirectSimulationParameters, DirectSimulationSettings, Direction, DistanceAttenuationModel,
    Equalizer, Hrtf, HrtfSettings, Point, Scene, SceneParams, SceneSettings, SimulationFlags,
    SimulationInputs, SimulationSharedInputs, Simulator, Sofa, Source, SourceSettings,
    SpeakerLayout, Vector3, VolumeNormalization,
};
use glam::Vec3;
use kira::Frame as KiraFrame;
use ringbuf::{
    traits::{Producer, SplitRef},
    HeapRb,
};
use std::collections::HashMap;
use uuid::Uuid;

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

pub struct SpatialSoundSource {
    position: Vec3,
    volume: f32,
    samples: Vec<f32>,
    sample_rate: u32,
    number_of_frames: usize,
    simulation_result: SimulationResult,
    cursor_pos: usize,

    source: Source,
}

impl SpatialSoundSource {
    pub fn new(
        simulator: &Simulator<Direct>,
        path: &str,
        volume: f32,
        position: Vec3,
    ) -> Result<Self> {
        let (samples, sample_rate, number_of_frames) = get_audio_data(path)
            .map_err(|e| anyhow::anyhow!("Failed to load audio file: {}", e))?;

        Ok(Self {
            position,
            volume,
            samples,
            sample_rate,
            number_of_frames,
            simulation_result: SimulationResult::default(),
            cursor_pos: 0,
            source: Source::try_new(
                &simulator,
                &SourceSettings {
                    flags: SimulationFlags::DIRECT,
                },
            )?,
        })
    }
}

pub struct SpatialSoundManager {
    ring_buffer: HeapRb<RingBufferSample>,
    available_samples: usize,

    sources: HashMap<Uuid, SpatialSoundSource>,

    context: Context,
    frame_window_size: usize,

    hrtf: Hrtf,

    direct_effect: DirectEffect,
    binaural_effect: BinauralEffect,
    ambisonics_encode_effect: AmbisonicsEncodeEffect,
    ambisonics_decode_effect: AmbisonicsDecodeEffect,

    cached_input_buf: AudioBuffer,
    cached_direct_buf: AudioBuffer,
    cached_binaural_buf: AudioBuffer,
    cached_summed_encoded_buf: AudioBuffer,
    cached_ambisonics_encode_buf: AudioBuffer,
    cached_ambisonics_decode_buf: AudioBuffer,

    listener_position: Vec3,
    listener_up: Vec3,
    listener_front: Vec3,
    listener_right: Vec3,

    simulator: Simulator<Direct>,
    scene: Scene,
}

impl SpatialSoundManager {
    pub fn new(
        context: Context,
        ring_buffer_size: usize,
        frame_window_size: usize,
        sample_rate: u32,
    ) -> Self {
        let ring_buffer = HeapRb::<RingBufferSample>::new(ring_buffer_size);

        let audio_settings = AudioSettings {
            sampling_rate: sample_rate as usize,
            frame_size: frame_window_size,
        };

        let hrtf = create_hrtf(&context, &audio_settings).unwrap();
        let (direct_effect, binaural_effect, ambisonics_encode_effect, ambisonics_decode_effect) =
            create_effects(&context, &audio_settings, &hrtf).unwrap();

        let mut simulator = create_simulator(&context, frame_window_size, sample_rate).unwrap();
        let scene = Scene::try_new(&context, &SceneSettings::default()).unwrap();
        simulator.set_scene(&scene);
        simulator.commit(); // must be called after set_scene

        let cached_input_buf = AudioBuffer::new(context.clone(), frame_window_size, 1).unwrap();
        let cached_direct_buf = AudioBuffer::new(context.clone(), frame_window_size, 1).unwrap();
        let cached_binaural_buf = AudioBuffer::new(context.clone(), frame_window_size, 2).unwrap();
        // 9 channels for order 2
        let cached_summed_encoded_buf =
            AudioBuffer::new(context.clone(), frame_window_size, 9).unwrap();
        let cached_ambisonics_encode_buf =
            AudioBuffer::new(context.clone(), frame_window_size, 9).unwrap();
        let cached_ambisonics_decode_buf =
            AudioBuffer::new(context.clone(), frame_window_size, 2).unwrap();

        return Self {
            ring_buffer,
            available_samples: 0,
            sources: HashMap::new(),
            context,
            frame_window_size,
            hrtf,
            direct_effect,
            binaural_effect,
            ambisonics_encode_effect,
            ambisonics_decode_effect,
            cached_input_buf,
            cached_direct_buf,
            cached_binaural_buf,
            cached_summed_encoded_buf,
            cached_ambisonics_encode_buf,
            cached_ambisonics_decode_buf,
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
    }

    pub fn add_source(&mut self, source: SpatialSoundSource) -> Uuid {
        let mut id = Uuid::new_v4();
        while self.sources.contains_key(&id) {
            log::warn!("Source with this UUID already exists, generating new UUID");
            id = Uuid::new_v4();
        }
        let source = self.sources.insert(id, source).unwrap();
        self.simulator.add_source(&source.source);
        id
    }

    pub fn get_source(&self, id: Uuid) -> Option<&SpatialSoundSource> {
        self.sources.get(&id)
    }

    pub fn remove_source(&mut self, id: Uuid) {
        self.sources.remove(&id);
    }

    fn update(&mut self) {
        let mut input_chunk = Vec::with_capacity(self.frame_window_size);
        let all_ids: Vec<Uuid> = self.sources.keys().cloned().collect();

        let encoded_buffer_size = self.frame_window_size * 9;
        let mut summed_encoded_buffer = vec![0.0; encoded_buffer_size];

        for id in all_ids {
            let source = self.get_source(id).unwrap();

            // make input buffer
            for i in 0..self.frame_window_size {
                let input_index = (source.cursor_pos + i) % source.samples.len();
                input_chunk.push(source.samples[input_index]);
            }
            self.cached_input_buf.set_data(&input_chunk).unwrap();

            self.apply_direct_effect(id);
            self.apply_ambisonics_encode_effect(id);

            // sum encoded buffer
            let data = self.cached_ambisonics_encode_buf.get_data();
            assert_eq!(data.len(), encoded_buffer_size);
            for i in 0..encoded_buffer_size {
                summed_encoded_buffer[i] += data[i];
            }

            // update cursor position
            let source_mut = self.sources.get_mut(&id).unwrap();
            source_mut.cursor_pos =
                (source_mut.cursor_pos + self.frame_window_size) % source_mut.samples.len();
        }
        self.cached_summed_encoded_buf
            .set_data(&summed_encoded_buffer)
            .unwrap();
        self.apply_ambisonics_decode_effect();

        let binaural_processed = self.cached_ambisonics_decode_buf.to_interleaved();

        // now get the ring buffer producer after processing
        let (mut producer, _) = self.ring_buffer.split_ref();

        // convert processed audio to ring buffer samples
        let max_frames = (binaural_processed.len() / 2).min(self.frame_window_size);
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
        self.available_samples += samples_added;
    }

    fn apply_direct_effect(&mut self, source_id: Uuid) {
        let source = self.get_source(source_id).unwrap();

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

        let _effect_state = self.direct_effect.apply(
            &direct_effect_params,
            &self.cached_input_buf.as_raw(),
            &self.cached_direct_buf.as_raw(),
        );
    }

    // fn apply_binaural_effect(&mut self) {
    //     let normalized_direction = self.get_target_direction().normalize();

    //     let binaural_effect_params = BinauralEffectParams {
    //         direction: Direction::new(
    //             normalized_direction.x,
    //             normalized_direction.y,
    //             normalized_direction.z,
    //         ),
    //         interpolation: HrtfInterpolation::Bilinear,
    //         spatial_blend: 1.0,
    //         hrtf: &self.hrtf,
    //         peak_delays: None,
    //     };

    //     let _effect_state = self.binaural_effect.apply(
    //         &binaural_effect_params,
    //         &self.cached_direct_buf.as_raw(),
    //         &self.cached_binaural_buf.as_raw(),
    //     );
    // }

    fn apply_ambisonics_encode_effect(&mut self, source_id: Uuid) {
        // don't need to normalize here, the lib will do it for us
        let dir = self.get_target_direction(source_id);

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
            let simulation_inputs = SimulationInputs {
                source: geometry::CoordinateSystem {
                    origin: Point::new(source.position.x, source.position.y, source.position.z),
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

        let simulation_shared_inputs = SimulationSharedInputs {
            listener: geometry::CoordinateSystem {
                origin: Point::new(
                    self.listener_position.x,
                    self.listener_position.y,
                    self.listener_position.z,
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

#[test]
fn testing() {}
