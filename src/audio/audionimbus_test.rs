use anyhow::Result;
use audionimbus::*;
use glam::Vec3;
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use kira::Frame;
use kira::{AudioManager, AudioManagerSettings, DefaultBackend};
use std::sync::Arc;

use super::audio_buffer::AudioBuffer as WrappedAudioBuffer;

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

fn make_static_sound_data(interleaved_frames: Vec<f32>, sample_rate: u32) -> StaticSoundData {
    return StaticSoundData {
        sample_rate,
        frames: make_frames(interleaved_frames),
        settings: StaticSoundSettings::default(),
        slice: None,
    };

    fn make_frames(interleaved_frames: Vec<f32>) -> Arc<[Frame]> {
        let mut frames = Vec::new();
        for i in (0..interleaved_frames.len()).step_by(2) {
            frames.push(Frame::new(interleaved_frames[i], interleaved_frames[i + 1]));
        }
        return frames.into_boxed_slice().into();
    }
}

fn apply_binaural_effect(
    context: &Context,
    binaural_effect: &BinauralEffect,
    hrtf: &Hrtf,
    frame_window_size: usize,
    output_number_of_channels: usize,
    input: &[Sample],
    number_of_frames: usize,
    player_position: Vec3,
    target_position: Vec3,
) -> Vec<f32> {
    let wrapped_output_buffer =
        WrappedAudioBuffer::new(&context, frame_window_size, output_number_of_channels).unwrap();

    let normalized_direction = (target_position - player_position).normalize();
    let binaural_effect_params = BinauralEffectParams {
        direction: Direction::new(
            normalized_direction.x,
            normalized_direction.y,
            normalized_direction.z,
        ),
        interpolation: HrtfInterpolation::Nearest,
        spatial_blend: 1.0,
        hrtf: &hrtf,
        peak_delays: None,
    };

    let interleaved_output = process_audio_chunks(
        input,
        frame_window_size,
        number_of_frames,
        output_number_of_channels,
        &wrapped_output_buffer,
        |input_buffer, _i| {
            let _effect_state = binaural_effect.apply(
                &binaural_effect_params,
                input_buffer,
                &wrapped_output_buffer.as_raw(),
            );
        },
    );

    return interleaved_output;
}

// fn test_ambisonics(
//     context: &Context,
//     audio_settings: &AudioSettings,
//     hrtf: &Hrtf,
//     frame_window_size: usize,
//     input: &[Sample],
//     number_of_frames: usize,
// ) -> Vec<f32> {
//     let order_to_channels = HashMap::from([(1, 4), (2, 9)]);
//     const ORDER: usize = 2;
//     let channels = order_to_channels[&ORDER];
//
//     let wrapped_output_buf_encoded =
//         WrappedAudioBuffer::new(&context, frame_window_size, channels).unwrap();
//     let wrapped_output_buf_decoded =
//         WrappedAudioBuffer::new(&context, frame_window_size, channels).unwrap();

//     let interleaved_output = process_audio_chunks(
//         input,
//         frame_window_size,
//         number_of_frames,
//         channels,
//         &wrapped_output_buf_decoded,
//         |input_buffer, _i| {
//             let ambisonics_encode_params = AmbisonicsEncodeEffectParams {
//                 direction: Direction::new(
//                     -1.0, // Right
//                     0.0,  // Up
//                     -1.0, // Behind
//                 ),
//                 order: ORDER,
//             };

//             let _effect_state = ambisonics_encode_effect.apply(
//                 &ambisonics_encode_params,
//                 input_buffer,
//                 &wrapped_output_buf_encoded.as_raw(),
//             );

//             let ambisonics_decode_params = AmbisonicsDecodeEffectParams {
//                 order: ORDER,
//                 hrtf: &hrtf,
//                 orientation: CoordinateSystem::default(),
//                 binaural: true,
//             };
//             let _effect_state = ambisonics_decode_effect.apply(
//                 &ambisonics_decode_params,
//                 &wrapped_output_buf_encoded.as_raw(),
//                 &wrapped_output_buf_decoded.as_raw(),
//             );
//         },
//     );

//     return interleaved_output;
// }

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

fn test_simulation(
    context: &Context,
    frame_window_size: usize,
    player_position: Vec3,
    target_position: Vec3,
    sample_rate: u32,
) -> Result<SimulationOutputs> {
    let mut simulator = create_simulator(context, frame_window_size, sample_rate)?;

    let scene = Scene::try_new(context, &SceneSettings::default()).unwrap();

    simulator.set_scene(&scene);
    simulator.commit(); // must be called after set_scene

    let source_settings = SourceSettings {
        flags: SimulationFlags::DIRECT,
    };
    let mut audio_source = Source::try_new(&simulator, &source_settings).unwrap();

    let simulation_inputs = SimulationInputs {
        source: geometry::CoordinateSystem {
            origin: Point::new(target_position.x, target_position.y, target_position.z),
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
    audio_source.set_inputs(SimulationFlags::DIRECT, simulation_inputs);

    simulator.add_source(&audio_source);
    simulator.commit(); // must be called after add_source

    let simulation_shared_inputs = SimulationSharedInputs {
        listener: geometry::CoordinateSystem {
            origin: Point::new(player_position.x, player_position.y, player_position.z),
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
    return Ok(audio_source.get_outputs(SimulationFlags::DIRECT));
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

    // let ambisonics_encode_effect = AmbisonicsEncodeEffect::try_new(
    //     &context,
    //     &audio_settings,
    //     &AmbisonicsEncodeEffectSettings { max_order: ORDER },
    // )
    // .unwrap();

    // let ambisonics_decode_effect = AmbisonicsDecodeEffect::try_new(
    //     &context,
    //     &audio_settings,
    //     &AmbisonicsDecodeEffectSettings {
    //         max_order: ORDER,
    //         hrtf: &hrtf,
    //         speaker_layout: SpeakerLayout::Stereo,
    //     },
    // )
    // .unwrap();

    Ok((direct_effect, binaural_effect))
}

fn apply_direct_effect(
    context: &Context,
    direct_effect: &DirectEffect,
    simulation_outputs: &SimulationOutputs,
    frame_window_size: usize,
    output_number_of_channels: usize,
    input: &[Sample],
    number_of_frames: usize,
) -> Vec<f32> {
    let wrapped_output_buffer =
        WrappedAudioBuffer::new(&context, frame_window_size, output_number_of_channels).unwrap();

    let direct_outputs = simulation_outputs.direct();

    let direct_effect_params = DirectEffectParams {
        distance_attenuation: direct_outputs.distance_attenuation,
        air_absorption: None, // Can't clone Equalizer<3>
        directivity: direct_outputs.directivity,
        occlusion: direct_outputs.occlusion,
        transmission: None, // Can't clone Transmission
    };

    let interleaved_output = process_audio_chunks(
        input,
        frame_window_size,
        number_of_frames,
        output_number_of_channels,
        &wrapped_output_buffer,
        |input_buffer, _i| {
            let _effect_state = direct_effect.apply(
                &direct_effect_params,
                input_buffer,
                &wrapped_output_buffer.as_raw(),
            );
        },
    );

    return interleaved_output;
}

#[test]
fn test_func() {
    let context = Context::try_new(&ContextSettings::default()).unwrap();

    const FRAME_WINDOW_SIZE: usize = 1024;
    const PLAYER_POSITION: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    const TARGET_POSITION: Vec3 = Vec3::new(-3.0, 3.0, -3.0);

    let (input, sample_rate, number_of_frames) = get_audio_data("assets/sfx/leaf_rustling.wav");

    println!("using sample_rate: {}", sample_rate);

    let audio_settings = AudioSettings {
        sampling_rate: sample_rate as usize,
        frame_size: FRAME_WINDOW_SIZE,
    };

    let hrtf = create_hrtf(&context, &audio_settings).unwrap();
    let (direct_effect, binaural_effect) =
        create_effects(&context, &audio_settings, &hrtf).unwrap();

    let simulation_outputs = test_simulation(
        &context,
        FRAME_WINDOW_SIZE,
        PLAYER_POSITION,
        TARGET_POSITION,
        sample_rate,
    )
    .unwrap();
    println!("simulation_outputs: {:?}", simulation_outputs.direct());

    let direct_processed_data = apply_direct_effect(
        &context,
        &direct_effect,
        &simulation_outputs,
        FRAME_WINDOW_SIZE,
        1,
        &input,
        number_of_frames,
    );

    let binaural_data = apply_binaural_effect(
        &context,
        &binaural_effect,
        &hrtf,
        FRAME_WINDOW_SIZE,
        2,
        &direct_processed_data,
        number_of_frames,
        PLAYER_POSITION,
        TARGET_POSITION,
    );

    let ssd = make_static_sound_data(binaural_data, sample_rate);

    // Optional audio playback - only if audio hardware is available
    if let Ok(mut audio_manager) =
        AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
    {
        if let Ok(_processed_handle) = audio_manager.play(ssd.clone()) {
            std::thread::sleep(std::time::Duration::from_secs(10));
        } else {
            println!("Could not play audio - no audio device available");
        }
    } else {
        println!("Audio manager initialization failed - skipping audio playback");
    }
}

#[test]
fn test_real_time_spatial_sound() {
    use crate::audio::spatial_sound::RealTimeSpatialSound;
    use crate::audio::spatial_sound::RealTimeSpatialSoundData;
    use kira::sound::Sound;

    let context = Context::try_new(&ContextSettings::default()).unwrap();
    const FRAME_WINDOW_SIZE: usize = 1024;

    // Create RealTimeSpatialSound instance
    let mut spatial_sound = RealTimeSpatialSound::new(context, FRAME_WINDOW_SIZE).unwrap();

    // Test position updates
    let player_pos = Vec3::new(0.0, 0.0, 0.0);
    let target_pos = Vec3::new(5.0, 0.0, 0.0);
    spatial_sound.update_positions(player_pos, target_pos);

    // Test simulation update
    spatial_sound.update_simulation().unwrap();

    // Test that the sound is not finished (it should loop)
    assert!(
        !spatial_sound.finished(),
        "RealTimeSpatialSound should not be finished (it loops)"
    );

    let mut audio_manager =
        AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()).unwrap();

    let context2 = Context::try_new(&ContextSettings::default()).unwrap();
    let mut spatial_sound_data =
        RealTimeSpatialSoundData::new(context2, FRAME_WINDOW_SIZE).unwrap();

    spatial_sound_data.update_positions(player_pos, target_pos);
    spatial_sound_data.update_simulation().unwrap();

    if let Ok(_spatial_handle) = audio_manager.play(spatial_sound_data) {
        std::thread::sleep(std::time::Duration::from_secs(3));
    } else {
        println!("Could not play spatial sound - audio device may not be available");
    }
}

// Example of how to properly integrate RealTimeSpatialSound with Kira in a real application
#[test]
fn example_spatial_sound_integration() -> Result<()> {
    use crate::audio::spatial_sound::RealTimeSpatialSoundData;
    // In a real application, you would do this:

    // 1. Create your audio manager (usually done once at app startup)
    let mut audio_manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())?;

    // 2. Create your spatial sound data
    let context = Context::try_new(&ContextSettings::default())?;
    let mut spatial_sound_data = RealTimeSpatialSoundData::new(context, 1024)?;

    // 3. Update positions from your game state
    spatial_sound_data.update_positions(Vec3::new(0.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0));
    spatial_sound_data.update_simulation()?;

    // 4. Play the spatial sound directly with Kira!
    let _handle = audio_manager.play(spatial_sound_data)?;
    std::thread::sleep(std::time::Duration::from_secs(8));

    println!("RealTimeSpatialSound can now be played directly with audio_manager.play()!");
    Ok(())
}
