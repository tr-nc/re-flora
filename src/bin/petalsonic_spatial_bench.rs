use anyhow::{Context, Result};
use petalsonic::audio_data::PetalSonicAudioData;
use petalsonic::math::{Pose, Quat, Vec3};
use petalsonic::playback::LoopMode;
use petalsonic::spatial::{SpatialProcessingMetrics, SpatialProcessor};
use petalsonic::{
    AmbisonicsBackend, DirectPathBackend, HrtfBackend, PlaybackInstance, SourceConfig, SourceId,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

const SAMPLE_RATE: u32 = 48_000;
const FRAME_SIZE: usize = 1024;

#[derive(Debug, Clone, Copy)]
struct BenchMode {
    name: &'static str,
    direct_backend: DirectPathBackend,
    hrtf_backend: HrtfBackend,
    use_ambisonics: bool,
    ambisonics_backend: AmbisonicsBackend,
}

#[derive(Debug, Default, Clone)]
struct BenchStats {
    total_us: Vec<u64>,
    direct_us: Vec<u64>,
    encode_us: Vec<u64>,
    decode_us: Vec<u64>,
    hrtf_us: Vec<u64>,
    native_lookup_us: Vec<u64>,
    native_fir_us: Vec<u64>,
}

fn main() -> Result<()> {
    let args = Args::parse()?;
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let native_hrtf_path = project_root.join("assets/hrtf/hrtf_b_nh172.petalhrtf");
    let steam_hrtf_path = project_root.join("assets/hrtf/hrtf_b_nh172.sofa");
    let clip_path = args
        .clip
        .unwrap_or_else(|| project_root.join("assets/sfx/tree_sound_48k_pregain_40db.wav"));

    let audio_data = PetalSonicAudioData::from_path(
        clip_path
            .to_str()
            .context("benchmark clip path is not valid UTF-8")?,
    )?;

    println!(
        "block_frames={FRAME_SIZE} sample_rate={SAMPLE_RATE} block_budget_ms={:.3} native_hrtf={} steam_hrtf={}",
        FRAME_SIZE as f64 / SAMPLE_RATE as f64 * 1000.0,
        native_hrtf_path.display(),
        steam_hrtf_path.display()
    );
    println!(
        "sources,mode,total_median_us,total_p95_us,total_max_us,direct_median_us,encode_median_us,decode_median_us,hrtf_median_us,native_lookup_median_us,native_fir_median_us"
    );

    let modes = if args.pure_hrtf_only {
        pure_hrtf_benchmark_modes()
    } else {
        benchmark_modes()
    };

    for source_count in &args.source_counts {
        for mode in modes {
            let stats = run_mode(
                *source_count,
                mode,
                &audio_data,
                &native_hrtf_path,
                &steam_hrtf_path,
                args.warmup_blocks,
                args.measure_blocks,
            )?;
            println!(
                "{},{},{},{},{},{},{},{},{},{},{}",
                source_count,
                mode.name,
                percentile(&stats.total_us, 0.50),
                percentile(&stats.total_us, 0.95),
                stats.total_us.iter().copied().max().unwrap_or(0),
                percentile(&stats.direct_us, 0.50),
                percentile(&stats.encode_us, 0.50),
                percentile(&stats.decode_us, 0.50),
                percentile(&stats.hrtf_us, 0.50),
                percentile(&stats.native_lookup_us, 0.50),
                percentile(&stats.native_fir_us, 0.50),
            );
        }
    }

    Ok(())
}

fn pure_hrtf_benchmark_modes() -> &'static [BenchMode] {
    &[
        BenchMode {
            name: "native_direct_native_per_source_hrtf",
            direct_backend: DirectPathBackend::Native,
            hrtf_backend: HrtfBackend::Native,
            use_ambisonics: false,
            ambisonics_backend: AmbisonicsBackend::Native,
        },
        BenchMode {
            name: "native_direct_steam_per_source_hrtf_custom",
            direct_backend: DirectPathBackend::Native,
            hrtf_backend: HrtfBackend::SteamAudio,
            use_ambisonics: false,
            ambisonics_backend: AmbisonicsBackend::Native,
        },
    ]
}

fn benchmark_modes() -> &'static [BenchMode] {
    &[
        BenchMode {
            name: "native_direct_native_per_source_hrtf",
            direct_backend: DirectPathBackend::Native,
            hrtf_backend: HrtfBackend::Native,
            use_ambisonics: false,
            ambisonics_backend: AmbisonicsBackend::Native,
        },
        BenchMode {
            name: "steam_direct_native_per_source_hrtf",
            direct_backend: DirectPathBackend::SteamAudio,
            hrtf_backend: HrtfBackend::Native,
            use_ambisonics: false,
            ambisonics_backend: AmbisonicsBackend::Native,
        },
        BenchMode {
            name: "native_direct_steam_per_source_hrtf_custom",
            direct_backend: DirectPathBackend::Native,
            hrtf_backend: HrtfBackend::SteamAudio,
            use_ambisonics: false,
            ambisonics_backend: AmbisonicsBackend::Native,
        },
        BenchMode {
            name: "native_direct_native_ambi_native_hrtf",
            direct_backend: DirectPathBackend::Native,
            hrtf_backend: HrtfBackend::Native,
            use_ambisonics: true,
            ambisonics_backend: AmbisonicsBackend::Native,
        },
        BenchMode {
            name: "native_direct_steam_ambi_native_hrtf",
            direct_backend: DirectPathBackend::Native,
            hrtf_backend: HrtfBackend::Native,
            use_ambisonics: true,
            ambisonics_backend: AmbisonicsBackend::SteamAudio,
        },
        BenchMode {
            name: "native_direct_native_ambi_steam_hrtf_custom",
            direct_backend: DirectPathBackend::Native,
            hrtf_backend: HrtfBackend::SteamAudio,
            use_ambisonics: true,
            ambisonics_backend: AmbisonicsBackend::Native,
        },
        BenchMode {
            name: "native_direct_steam_ambi_steam_hrtf_custom",
            direct_backend: DirectPathBackend::Native,
            hrtf_backend: HrtfBackend::SteamAudio,
            use_ambisonics: true,
            ambisonics_backend: AmbisonicsBackend::SteamAudio,
        },
    ]
}

fn run_mode(
    source_count: usize,
    mode: &BenchMode,
    audio_data: &Arc<PetalSonicAudioData>,
    native_hrtf_path: &PathBuf,
    steam_hrtf_path: &PathBuf,
    warmup_blocks: usize,
    measure_blocks: usize,
) -> Result<BenchStats> {
    let native_hrtf_path = native_hrtf_path
        .to_str()
        .context("native HRTF path is not valid UTF-8")?;
    let steam_hrtf_path = steam_hrtf_path
        .to_str()
        .context("Steam HRTF path is not valid UTF-8")?;
    let mut processor = SpatialProcessor::new(
        SAMPLE_RATE,
        FRAME_SIZE,
        15.0,
        Some(native_hrtf_path),
        Some(steam_hrtf_path),
        Some(native_hrtf_path),
        0.0,
        mode.hrtf_backend,
        mode.direct_backend,
        mode.use_ambisonics,
        mode.ambisonics_backend,
        None,
        None,
    )?;
    processor.set_listener_pose(Pose::new(Vec3::ZERO, Quat::IDENTITY))?;

    let mut instances = make_instances(source_count, audio_data.clone());
    let mut output = vec![0.0f32; FRAME_SIZE * 2];

    for _ in 0..warmup_blocks {
        output.fill(0.0);
        let mut refs = instances
            .iter_mut()
            .map(|(source_id, instance)| (*source_id, instance))
            .collect::<Vec<_>>();
        processor.process_spatial_sources_with_metrics(&mut refs, &mut output)?;
    }

    let mut stats = BenchStats::default();
    stats.total_us.reserve(measure_blocks);
    for _ in 0..measure_blocks {
        output.fill(0.0);
        let mut refs = instances
            .iter_mut()
            .map(|(source_id, instance)| (*source_id, instance))
            .collect::<Vec<_>>();
        let start = Instant::now();
        let summary = processor.process_spatial_sources_with_metrics(&mut refs, &mut output)?;
        stats.total_us.push(start.elapsed().as_micros() as u64);
        push_metrics(&mut stats, summary.metrics);
    }

    Ok(stats)
}

fn make_instances(
    source_count: usize,
    audio_data: Arc<PetalSonicAudioData>,
) -> Vec<(SourceId, PlaybackInstance)> {
    let radius = 4.0;
    (0..source_count)
        .map(|index| {
            let angle = index as f32 * std::f32::consts::TAU / source_count.max(1) as f32;
            let elevation = ((index % 7) as f32 - 3.0) * 0.25;
            let position = Vec3::new(angle.cos() * radius, elevation, angle.sin() * radius);
            let source_id = SourceId::from(index as u64);
            let config =
                SourceConfig::spatial_with_volume_db(Pose::new(position, Quat::IDENTITY), -6.0);
            let mut instance =
                PlaybackInstance::new(source_id, audio_data.clone(), config, LoopMode::Infinite);
            instance.play_from_beginning();
            instance.seek((index as f32 * 0.618_034).fract());
            (source_id, instance)
        })
        .collect()
}

fn push_metrics(stats: &mut BenchStats, metrics: SpatialProcessingMetrics) {
    stats.direct_us.push(metrics.direct_processing_time_us);
    stats.encode_us.push(metrics.ambisonics_encoding_time_us);
    stats.decode_us.push(metrics.ambisonics_decoding_time_us);
    stats.hrtf_us.push(metrics.hrtf_rendering_time_us);
    stats
        .native_lookup_us
        .push(metrics.native_hrtf_direction_lookup_time_us);
    stats
        .native_fir_us
        .push(metrics.native_hrtf_convolution_time_us);
}

fn percentile(values: &[u64], percentile: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }

    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
}

#[derive(Debug)]
struct Args {
    source_counts: Vec<usize>,
    warmup_blocks: usize,
    measure_blocks: usize,
    clip: Option<PathBuf>,
    pure_hrtf_only: bool,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args = std::env::args().skip(1);
        let mut source_counts = vec![1, 8, 16, 36, 64, 128, 256];
        let mut warmup_blocks = 16;
        let mut measure_blocks = 120;
        let mut clip = None;
        let mut pure_hrtf_only = false;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--sources" => {
                    let value = args
                        .next()
                        .context("--sources requires a comma-separated list")?;
                    source_counts = value
                        .split(',')
                        .map(|part| part.trim().parse::<usize>())
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .context("failed to parse --sources")?;
                }
                "--warmup" => {
                    warmup_blocks = args
                        .next()
                        .context("--warmup requires a block count")?
                        .parse()
                        .context("failed to parse --warmup")?;
                }
                "--blocks" => {
                    measure_blocks = args
                        .next()
                        .context("--blocks requires a block count")?
                        .parse()
                        .context("failed to parse --blocks")?;
                }
                "--clip" => {
                    clip = Some(PathBuf::from(
                        args.next().context("--clip requires a path")?,
                    ));
                }
                "--pure-hrtf-only" => {
                    pure_hrtf_only = true;
                }
                "--help" | "-h" => {
                    println!(
                        "usage: cargo run --release --bin petalsonic_spatial_bench -- [--sources 1,8,36] [--warmup 16] [--blocks 120] [--clip path] [--pure-hrtf-only]"
                    );
                    std::process::exit(0);
                }
                other => anyhow::bail!("unknown argument: {other}"),
            }
        }

        if source_counts.is_empty() {
            anyhow::bail!("--sources must contain at least one count");
        }
        if measure_blocks == 0 {
            anyhow::bail!("--blocks must be greater than zero");
        }

        Ok(Self {
            source_counts,
            warmup_blocks,
            measure_blocks,
            clip,
            pure_hrtf_only,
        })
    }
}
