use crate::{
    audio_data::PetalSonicAudioData,
    error::{PetalSonicError, Result},
};
use std::fs::File;
use std::path::Path;
use std::time::Duration;
use symphonia::{
    core::{
        audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions,
        io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
    },
    default::{get_codecs, get_probe},
};

/// Decode one supported audio file into owned interleaved PCM.
///
/// The caller decides when that owned PCM becomes a shared resident allocation.
pub(crate) fn decode_file(path: &str) -> Result<PetalSonicAudioData> {
    let file = File::open(path).map_err(|error| {
        PetalSonicError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, error))
    })?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        hint.with_extension(extension);
    }

    let probed = get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| {
            PetalSonicError::AudioLoading(format!("Failed to probe audio format: {error:?}"))
        })?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| PetalSonicError::AudioLoading("No default audio track found".to_string()))?;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| PetalSonicError::AudioLoading("Sample rate not found".to_string()))?;
    let channels = track
        .codec_params
        .channels
        .ok_or_else(|| PetalSonicError::AudioLoading("Channel count not found".to_string()))?
        .count() as u16;
    let mut decoder = get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| {
            PetalSonicError::AudioLoading(format!("Failed to create decoder: {error:?}"))
        })?;
    let mut samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(_)) => break,
            Err(error) => {
                return Err(PetalSonicError::AudioLoading(format!(
                    "Error reading packet: {error:?}"
                )));
            }
        };
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(Error::IoError(_)) => break,
            Err(Error::DecodeError(_)) => continue,
            Err(error) => {
                return Err(PetalSonicError::AudioLoading(format!(
                    "Error decoding packet: {error:?}"
                )));
            }
        };
        let mut decoded_samples =
            SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        decoded_samples.copy_interleaved_ref(decoded);
        samples.extend_from_slice(decoded_samples.samples());
    }

    let duration =
        Duration::from_secs_f64(samples.len() as f64 / (sample_rate * channels as u32) as f64);
    Ok(PetalSonicAudioData::new(
        samples,
        sample_rate,
        channels,
        duration,
    ))
}
