use audionimbus::*;
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use kira::{AudioManager, AudioManagerSettings, DefaultBackend, Frame};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// Returns: (input, sample_rate, padded_frame_size)
fn get_audio_data(path: &str, frame_window_size: usize) -> (Vec<Sample>, u32, usize) {
    let audio_data = StaticSoundData::from_file(path).expect("Failed to load audio file");
    let loaded_frames = &audio_data.frames;
    let padded_frame_size = loaded_frames.len().div_ceil(frame_window_size) * frame_window_size;

    let mut input: Vec<Sample> = loaded_frames
        .into_iter()
        .map(|frame| frame.left) // use left channel for mono input
        .collect();

    // pad it to the nearest multiple of frame_window_size
    input.resize(padded_frame_size, 0.0);

    return (input, audio_data.sample_rate, padded_frame_size);
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

#[test]
fn test_func() {
    let test_start = Instant::now();

    let context = Context::try_new(&ContextSettings::default()).unwrap();

    const OUTPUT_NUMBER_OF_CHANNELS: usize = 2;
    const FRAME_WINDOW_SIZE: usize = 1024;

    // Audio loading timing
    let audio_load_start = Instant::now();
    let (input, sample_rate, number_of_frames) =
        get_audio_data("assets/sfx/leaf_rustling.wav", FRAME_WINDOW_SIZE);
    let audio_load_duration = audio_load_start.elapsed();
    println!(
        "Audio loading took: {:.3}ms",
        audio_load_duration.as_secs_f64() * 1000.0
    );

    println!("using sample_rate: {}", sample_rate);

    let audio_settings = AudioSettings {
        sampling_rate: sample_rate as usize,
        frame_size: FRAME_WINDOW_SIZE,
    };

    // Create effect timing
    let effect_create_start = Instant::now();
    let hrtf = Hrtf::try_new(&context, &audio_settings, &HrtfSettings::default()).unwrap();
    let binaural_effect = BinauralEffect::try_new(
        &context,
        &audio_settings,
        &BinauralEffectSettings { hrtf: &hrtf },
    )
    .unwrap();
    let effect_create_duration = effect_create_start.elapsed();
    println!(
        "Create effect took: {:.3}ms",
        effect_create_duration.as_secs_f64() * 1000.0
    );

    let mut output = vec![0.0; FRAME_WINDOW_SIZE * OUTPUT_NUMBER_OF_CHANNELS];
    let output_buffer = AudioBuffer::try_with_data_and_settings(
        &mut output,
        &AudioBufferSettings {
            num_channels: Some(OUTPUT_NUMBER_OF_CHANNELS),
            ..Default::default()
        },
    )
    .unwrap();

    // Apply effect to clip timing
    let apply_effect_start = Instant::now();
    let binaural_effect_params = BinauralEffectParams {
        direction: Direction::new(
            -1.0, // Right
            0.0, // Up
            -1.0, // Behind
        ),
        interpolation: HrtfInterpolation::Nearest,
        spatial_blend: 1.0,
        hrtf: &hrtf,
        peak_delays: None,
    };

    let mut interleaved_output = vec![0.0; number_of_frames * OUTPUT_NUMBER_OF_CHANNELS];

    let num_of_iterations = number_of_frames.div_ceil(FRAME_WINDOW_SIZE);
    println!("num_of_iterations: {}", num_of_iterations);

    for i in 0..num_of_iterations {
        let input_slice = &input[i * FRAME_WINDOW_SIZE..(i + 1) * FRAME_WINDOW_SIZE];
        let input_buffer = AudioBuffer::try_with_data(input_slice).unwrap();
        let _effect_state =
            binaural_effect.apply(&binaural_effect_params, &input_buffer, &output_buffer);

        let mut interleaved_frame_output = vec![0.0; FRAME_WINDOW_SIZE * OUTPUT_NUMBER_OF_CHANNELS];
        output_buffer.interleave(&context, &mut interleaved_frame_output);

        // write interleaved_frame_output to interleaved_output
        interleaved_output[i * FRAME_WINDOW_SIZE * OUTPUT_NUMBER_OF_CHANNELS
            ..(i + 1) * FRAME_WINDOW_SIZE * OUTPUT_NUMBER_OF_CHANNELS]
            .copy_from_slice(&interleaved_frame_output);
    }

    let apply_effect_duration = apply_effect_start.elapsed();
    println!(
        "Apply effect to clip took: {:.3}ms",
        apply_effect_duration.as_secs_f64() * 1000.0
    );

    let mut audio_manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
        .expect("Failed to create audio manager");

    // Make static sound data timing
    let make_static_start = Instant::now();
    let audio_data = make_static_sound_data(interleaved_output, sample_rate);
    let make_static_duration = make_static_start.elapsed();
    println!(
        "make_static_sound_data took: {:.3}ms",
        make_static_duration.as_secs_f64() * 1000.0
    );

    let total_duration = test_start.elapsed();
    println!(
        "Total processing time: {:.3}ms",
        total_duration.as_secs_f64() * 1000.0
    );

    println!("Audionimbus processing completed successfully!");
    println!(
        "Processed {} samples with binaural effect",
        audio_data.frames.len()
    );

    // Play the original audio to verify the system works
    println!("Playing original leaf rustling audio...");
    let _original_handle = audio_manager
        .play(audio_data.clone())
        .expect("Failed to play original audio");
    thread::sleep(Duration::from_secs(10));
}
