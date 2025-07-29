use audionimbus::*;

#[test]
fn test_func() {
    // Initialize the audio context.
    let context = Context::try_new(&ContextSettings::default()).unwrap();

    let audio_settings = AudioSettings {
        sampling_rate: 48000,
        frame_size: 1024,
    };

    // Set up HRTF for binaural rendering.
    let hrtf = Hrtf::try_new(&context, &audio_settings, &HrtfSettings::default()).unwrap();

    // Create a binaural effect.
    let binaural_effect = BinauralEffect::try_new(
        &context,
        &audio_settings,
        &BinauralEffectSettings { hrtf: &hrtf },
    )
    .unwrap();

    // Generate an input frame (in thise case, a single-channel sine wave).
    let input: Vec<Sample> = (0..audio_settings.frame_size)
        .map(|i| {
            (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / audio_settings.sampling_rate as f32)
                .sin()
        })
        .collect();
    // Create an audio buffer over the input data.
    let input_buffer = AudioBuffer::try_with_data(&input).unwrap();

    let num_channels: usize = 2; // Stereo
                                 // Allocate memory to store processed samples.
    let mut output = vec![0.0; audio_settings.frame_size * num_channels];
    // Create another audio buffer over the output container.
    let output_buffer = AudioBuffer::try_with_data_and_settings(
        &mut output,
        &AudioBufferSettings {
            num_channels: Some(num_channels),
            ..Default::default()
        },
    )
    .unwrap();

    // Apply a binaural audio effect.
    let binaural_effect_params = BinauralEffectParams {
        direction: Direction::new(
            1.0, // Right
            0.0, // Up
            0.0, // Behind
        ),
        interpolation: HrtfInterpolation::Nearest,
        spatial_blend: 1.0,
        hrtf: &hrtf,
        peak_delays: None,
    };
    let _effect_state =
        binaural_effect.apply(&binaural_effect_params, &input_buffer, &output_buffer);

    // `output` now contains the processed samples in a deinterleaved format (i.e., left channel
    // samples followed by right channel samples).

    // Note: most audio engines expect interleaved audio (alternating samples for each channel). If
    // required, use the `AudioBuffer::interleave` method to convert the format.
}
