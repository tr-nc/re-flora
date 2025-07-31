use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use std::sync::{Arc, Mutex};

/// Play audio samples using cpal
pub fn play_audio_samples(samples: Vec<f32>, sample_rate: u32) -> Result<()> {
    let frames = samples.len();

    println!(
        "Playing {} frames at {} Hz ({} samples)",
        frames,
        sample_rate,
        samples.len()
    );

    // Get the default host and device
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No default output device available"))?;

    println!("Using audio device: {}", device.name()?);

    // Get the default output config
    let config = device.default_output_config()?;
    println!("Default output config: {:?}", config);

    // Prepare audio data
    let audio_data = Arc::new(Mutex::new((samples, 0usize))); // (samples, current_position)

    // Create the audio stream based on the config format
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            run_stream::<f32>(&device, &config.into(), audio_data, sample_rate)?
        }
        cpal::SampleFormat::I16 => {
            run_stream::<i16>(&device, &config.into(), audio_data, sample_rate)?
        }
        cpal::SampleFormat::U16 => {
            run_stream::<u16>(&device, &config.into(), audio_data, sample_rate)?
        }
        _ => return Err(anyhow::anyhow!("Unsupported sample format")),
    };

    // Start the stream
    stream.play()?;

    println!("Playing audio... (press Ctrl+C to stop)");

    // Keep the stream alive for the duration of the audio
    let duration_secs = frames as f64 / sample_rate as f64;
    std::thread::sleep(std::time::Duration::from_secs_f64(duration_secs + 1.0));

    println!("Playback finished");

    Ok(())
}

fn run_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio_data: Arc<Mutex<(Vec<f32>, usize)>>,
    target_sample_rate: u32,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let device_sample_rate = config.sample_rate.0;

    // Calculate resampling ratio if needed
    let resample_ratio = target_sample_rate as f64 / device_sample_rate as f64;

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut audio_guard = audio_data.lock().unwrap();
            let (samples, position) = &mut *audio_guard;

            // Fill the output buffer
            for frame in data.chunks_mut(channels) {
                let sample = if *position < samples.len() {
                    // Simple resampling by adjusting position increment
                    let current_sample = samples[*position];
                    *position = (*position as f64 + resample_ratio) as usize;
                    current_sample
                } else {
                    0.0 // Silence when we've played all samples
                };

                // Fill all channels with the same sample (mono to stereo/multi-channel)
                for channel_sample in frame.iter_mut() {
                    *channel_sample = T::from_sample(sample);
                }
            }
        },
        move |err| {
            eprintln!("Audio stream error: {}", err);
        },
        None,
    )?;

    Ok(stream)
}
