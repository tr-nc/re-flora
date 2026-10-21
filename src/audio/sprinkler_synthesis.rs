//! Original, deterministic water texture. All timbre constants live here.
pub(super) const SAMPLE_RATE: u32 = 48_000;
pub(super) fn water_pcm() -> Vec<f32> {
    let n = SAMPLE_RATE as usize * 4;
    let mut pcm = Vec::with_capacity(n);
    let mut rng = 0xa73f_9201_u32;
    let mut low = 0.0;
    let mut body = 0.0;
    // Periodic Fourier-free filtered noise; warm up before recording.
    for i in 0..n + 4096 {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        let noise = rng as f64 / u32::MAX as f64 * 2.0 - 1.0;
        low += 0.12 * (noise - low);
        body += 0.025 * (noise - body);
        if i >= 4096 {
            pcm.push(((low - body) * 0.12) as f32);
        }
    }
    // Smooth periodic window at the join (quiet 10 ms seam, no sample discontinuity).
    let seam = 480;
    for i in 0..seam {
        let gain = (i as f32 / seam as f32 * std::f32::consts::FRAC_PI_2)
            .sin()
            .powi(2);
        pcm[i] *= gain;
        pcm[n - 1 - i] *= gain;
    }
    let mean = pcm.iter().map(|v| f64::from(*v)).sum::<f64>() / n as f64;
    for v in &mut pcm {
        *v -= mean as f32;
    }
    pcm
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_pcm_is_bounded_dc_free_and_seam_continuous() {
        let pcm = water_pcm();
        assert!(pcm.iter().all(|x| x.is_finite() && x.abs() < 0.06));
        assert!((pcm.iter().map(|x| f64::from(*x)).sum::<f64>() / pcm.len() as f64).abs() < 1e-8);
        assert!((pcm[0] - pcm[pcm.len() - 1]).abs() < 1e-7);
        // Conservative coherent sum before existing spatial/master/category gain.
        assert!(
            pcm.iter().map(|x| x.abs()).fold(0.0_f32, f32::max)
                * 16.0
                * 10.0_f32.powf(-12.0 / 20.0)
                < 0.25
        );
    }
}

#[cfg(test)]
mod export_tests {
    use super::*;
    use petalsonic::{SampleEnvelopeControl, SampleEnvelopeRenderer};
    #[test]
    #[ignore = "offline WAV export, no device; run explicitly under heavy lock"]
    fn export_production_sprinkler_previews() {
        let path = std::path::Path::new("target/garden-delivery/audio");
        std::fs::create_dir_all(path).unwrap();
        let bed = water_pcm();
        for (name, frequency, duty, lifecycle) in [
            ("continuous", 5.0, 1.0, false),
            ("saved-5hz-30pct", 5.0, 0.3, false),
            ("fast-20hz", 20.0, 0.3, false),
            ("lifecycle", 5.0, 0.3, true),
        ] {
            let control = SampleEnvelopeControl::default();
            let mut render = SampleEnvelopeRenderer::new(control.clone());
            let mut samples = Vec::new();
            let mut phase = 0.0;
            for frame in 0..360 {
                let active =
                    !lifecycle || (30..120).contains(&frame) || (180..300).contains(&frame);
                control.anchor(phase, if active { frequency } else { 0.0 }, duty);
                render.begin_block();
                for sample in 0..800 {
                    samples.push(
                        bed[(frame * 800 + sample) % bed.len()] * render.next_gain(SAMPLE_RATE),
                    );
                }
                if active {
                    phase = (phase + frequency / 60.0) % 1.0;
                }
            }
            let mut wav = Vec::new();
            let bytes = (samples.len() * 2) as u32;
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&(36 + bytes).to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
            wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
            wav.extend_from_slice(&2u16.to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&bytes.to_le_bytes());
            for sample in &samples {
                wav.extend_from_slice(&((*sample * i16::MAX as f32).round() as i16).to_le_bytes());
            }
            std::fs::write(path.join(format!("{name}.wav")), wav).unwrap();
            println!(
                "preview={name} mode=direct-production-synthesis-envelope seconds=6 peak={:.6}",
                samples.iter().map(|v| v.abs()).fold(0.0_f32, f32::max)
            );
        }
    }
}
