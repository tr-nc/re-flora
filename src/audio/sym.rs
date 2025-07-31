use std::{fs::File, io::BufReader, path::Path};

use symphonia::{
    core::{
        audio::{AudioBufferRef, SampleBuffer},
        codecs::{Decoder, DecoderOptions},
        errors::Error,
        formats::{FormatOptions, FormatReader},
        io::{MediaSourceStream, MediaSource},
        meta::MetadataOptions,
        probe::Hint,
    },
    default::{get_codecs, get_probe},
};

/// Load `path` with Symphonia.
///
/// Returns (samples_mono_left, sample_rate_hz, num_frames).
pub fn get_audio_data<P: AsRef<Path>>(
    path: P,
) -> Result<(Vec<f32>, u32, usize), Error> {
    // ---------- open & probe -------------------------------------------------
    let file = File::open(&path).map_err(|_| Error::IoError)?;
    let mss  = MediaSourceStream::new(Box::new(BufReader::new(file)), Default::default());

    // Give Symphonia a hint based on the extension (helps probing).
    let mut hint = Hint::new();
    if let Some(ext) = path.as_ref().extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probe = get_probe();
    let probed = probe.format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format  = probed.format;

    // ---------- select track & create decoder -------------------------------
    let track = format
        .default_track()
        .ok_or_else(|| Error::DecodeError("no default audio track"))?;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| Error::DecodeError("sample-rate not found"))? as u32;

    let mut decoder = get_codecs().make(
        &track.codec_params,
        &DecoderOptions::default(),
    )?;

    // ---------- decode -------------------------------------------------------
    let mut mono_samples: Vec<f32> = Vec::new();

    loop {
        // Read the next packet from the container.
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError) => break, // end‐of‐file
            Err(e) => return Err(e),
        };

        // Decode the packet into audio samples.
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(Error::IoError) => break, // also EOF in some formats
            Err(Error::DecodeError(_)) => continue, // recoverable corruption
            Err(e) => return Err(e),
        };

        // Convert the sample buffer into f32 interleaved samples.
        // We do this through an intermediate SampleBuffer to simplify type handling.
        match decoded {
            AudioBufferRef::F32(buf) => {
                // Already f32; copy only left channel (index 0).
                mono_samples.extend_from_slice(buf.chan(0));
            }

            // 16-bit signed PCM
            AudioBufferRef::S16(buf) => {
                let mut tmp = SampleBuffer::<f32>::new(
                    buf.frames(),
                    *buf.spec(),
                );
                tmp.copy_interleaved_ref(buf);
                mono_samples.extend(
                    tmp.chan(0)
                        .iter()
                        .map(|&s| s), // already in –1.0…1.0 range
                );
            }

            // 8-bit unsigned PCM
            AudioBufferRef::U8(buf) => {
                let mut tmp = SampleBuffer::<f32>::new(
                    buf.frames(),
                    *buf.spec(),
                );
                tmp.copy_interleaved_ref(buf);
                mono_samples.extend(
                    tmp.chan(0)
                        .iter()
                        .map(|&s| s), // converted to f32 0.0…1.0 then shifted
                );
            }

            // Add other sample types if you need them…
            _ => return Err(Error::DecodeError("unsupported sample format")),
        }
    }

    let frames = mono_samples.len(); // one sample per mono frame
    Ok((mono_samples, sample_rate, frames))
}

//--------------------------------------------------------------
// Small demo
//--------------------------------------------------------------
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (samples, rate, frames) = get_audio_data("assets/sound.wav")?;
    println!("Loaded {frames} frames at {rate} Hz ({} samples)", samples.len());
    Ok(())
}