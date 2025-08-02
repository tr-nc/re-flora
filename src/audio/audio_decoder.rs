use anyhow::Result;
use std::{fs::File, path::Path};
use symphonia::{
    core::{
        audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions,
        io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
    },
    default::{get_codecs, get_probe},
};

/// Returns: (samples, sample_rate)
pub fn get_audio_data(path: &str) -> Result<(Vec<f32>, u32)> {
    let file = File::open(&path).expect("Failed to open audio file");
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probe = get_probe();
    let probed = probe
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| anyhow::anyhow!("Failed to probe audio format"))?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or(anyhow::anyhow!("No default audio track found"))?;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or(anyhow::anyhow!("Sample rate not found"))? as u32;

    let mut decoder = get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut mono_samples: Vec<f32> = Vec::new();

    loop {
        // Read the next packet from the container.
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(_)) => break, // end‐of‐file
            Err(e) => return Err(anyhow::anyhow!("Error reading packet: {:?}", e)),
        };

        // Decode the packet into audio samples.
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(Error::IoError(_)) => break, // also EOF in some formats
            Err(Error::DecodeError(_)) => continue, // recoverable corruption
            Err(e) => return Err(anyhow::anyhow!("Error decoding packet: {:?}", e)),
        };

        // Convert the sample buffer into f32 samples using SampleBuffer
        let spec = *decoded.spec();
        let mut tmp = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        tmp.copy_interleaved_ref(decoded);

        // Extract only the left channel samples
        let channels = spec.channels.count();
        if channels > 0 {
            mono_samples.extend(
                tmp.samples().chunks(channels).map(|frame| frame[0]), // take left channel
            );
        }
    }
    Ok((mono_samples, sample_rate))
}
