//! Real processor/cursor regressions, no window/device/random benchmark.
use super::*;
use crate::acoustic_propagation::{DirectAcousticResponse, DirectSolveStatus};
use crate::audio_data::PetalSonicAudioData;
use crate::domain::{
    DirectPath, EmitterDesc, EnvironmentSend, ExtentSample, ExtentSampleId, PlayOptions,
};
use crate::playback::prepare_test_voice;
use std::time::Duration;

const WOOD: [f32; 3] = [0.08, 0.035, 0.015];
const BLOCK: usize = 1024;

fn fixture(
    extent: SourceExtent,
    options: PlayOptions,
) -> (
    SpatialProcessor,
    HashMap<VoiceId, PlaybackInstance>,
    VoiceId,
) {
    let id = VoiceId::from(1);
    let pcm: Vec<_> = (0..48_000)
        .map(|i| {
            let t = i as f32 / 48_000.;
            (1. - (-t / 0.009).exp())
                * (-5. * t).exp()
                * ((std::f32::consts::TAU * 6000. * t).sin()
                    + 0.2 * (std::f32::consts::TAU * 600. * t).sin())
        })
        .collect();
    let voice = prepare_test_voice(
        id,
        Emitter {
            world_id: 1,
            index: 0,
            generation: 1,
        },
        Arc::new(PetalSonicAudioData::new(
            pcm,
            48_000,
            1,
            Duration::from_secs(1),
        )),
        EmitterDesc::spatial(Pose::from_position(Vec3::Z)).with_extent(extent),
        options.with_play_command_id(PlayCommandId(71)),
        None,
        0,
        BLOCK,
    )
    .start()
    .1;
    let processor = SpatialProcessor::new(SpatialProcessorConfig {
        sample_rate: 48_000,
        frame_size: BLOCK,
        max_voices: 1,
        distance_scaler: 1.,
        native_hrtf_path: None,
        hrtf_gain: 0.,
        use_ambisonics: false,
        environmental_acoustics_enabled: Arc::new(AtomicBool::new(true)),
    })
    .unwrap();
    (processor, HashMap::from([(id, voice)]), id)
}
fn response(
    id: VoiceId,
    status: DirectSolveStatus,
    lobes: Vec<DirectLobeTarget>,
) -> Arc<AcousticResponse> {
    Arc::new(AcousticResponse {
        spatial_revision: 1,
        geometry_version: 1,
        direct: vec![DirectAcousticResponse {
            voice_id: id,
            routing_generation: 0,
            gain: WOOD,
            environment_gain: WOOD,
            direct_lobes: lobes,
            environment_representatives: vec![],
            solve_status: status,
            cache_age_seconds: 0.,
            early_reflections: vec![],
        }]
        .into(),
        late_reverb: LateReverbParameters::SILENT,
        published_at: Instant::now(),
        solve_time_us: 1,
    })
}
fn quantum(
    processor: &mut SpatialProcessor,
    voices: &mut HashMap<VoiceId, PlaybackInstance>,
    id: VoiceId,
) -> Vec<VoiceTelemetryEvent> {
    let mut events = Vec::with_capacity(4);
    processor
        .process_spatial_sources_with_metrics(
            &[id],
            voices,
            &mut vec![0.; BLOCK * 2],
            SpatialRenderContext::default(),
            &mut events,
        )
        .unwrap();
    events
}
#[test]
fn acoustic_start_keeps_attack_pending_and_bootstraps_point_wood_without_unity_burst() {
    let (mut p, mut voices, id) = fixture(
        SourceExtent::Point,
        PlayOptions::once().with_acoustic_start(),
    );
    for _ in 0..2 {
        assert!(quantum(&mut p, &mut voices, id).is_empty());
        assert_eq!(
            voices[&id].info.current_frame, 0,
            "pending acoustics must not consume the attack"
        );
        assert!(p.cached_binaural_processed.iter().all(|s| *s == 0.));
    }
    p.replace_acoustic_response(response(id, DirectSolveStatus::Solved, vec![]));
    let events = quantum(&mut p, &mut voices, id);
    assert_eq!(voices[&id].info.current_frame, BLOCK);
    assert!(events.iter().any(
        |e| matches!(e,VoiceTelemetryEvent::FirstRendered(f) if f.environment_response.is_some())
    ));
    assert_eq!(p.native_direct_source_states[&id].current_gain, WOOD);
    let mut expected = vec![0.; BLOCK];
    let mut state = NativeDirectSourceState::new();
    state.current_gain = WOOD;
    apply_three_band_gain(
        &voices[&id].audio_data.samples()[..BLOCK],
        &mut expected,
        &mut state,
        ThreeBandCoefficients {
            low: p.direct_low_coefficient,
            low_mid: p.direct_low_mid_coefficient,
            smoothing: p.direct_gain_smoothing_coefficient,
        },
        WOOD,
        native_air_absorption(1.),
    );
    assert_eq!(
        p.cached_direct_buf, expected,
        "first real block must use wood, not a unity-to-wood ramp"
    );
    // Complete the original PCM, rather than cutting its delayed onset/tail.
    for _ in 0..47 {
        quantum(&mut p, &mut voices, id);
    }
    assert_eq!(voices[&id].info.current_frame, 48_000);
    assert!(voices.get_mut(&id).unwrap().take_reclaim().is_some());
}
#[test]
fn acoustic_start_ignores_deferred_response_and_times_out_without_cursor_or_completion() {
    let (mut p, mut voices, id) = fixture(
        SourceExtent::Point,
        PlayOptions::once().with_acoustic_start(),
    );
    p.replace_acoustic_response(response(id, DirectSolveStatus::Deferred, vec![]));
    let mut all = vec![];
    for _ in 0..6 {
        all.extend(quantum(&mut p, &mut voices, id));
    }
    assert_eq!(voices[&id].info.current_frame, 0);
    assert!(
        !all.iter()
            .any(|e| matches!(e, VoiceTelemetryEvent::FirstRendered(_)))
    );
    assert_eq!(
        all.iter()
            .filter(|e| matches!(e, VoiceTelemetryEvent::AcousticStartTimedOut { .. }))
            .count(),
        1
    );
    let reclaim = voices
        .get_mut(&id)
        .unwrap()
        .take_reclaim()
        .expect("wait must be bounded");
    assert_eq!(reclaim.completion_tag, None);
}
#[test]
fn acoustic_start_distributed_bootstrap_uses_solved_lobes_and_environment_from_onset() {
    let extent = SourceExtent::weighted_samples(vec![
        ExtentSample::new(ExtentSampleId(1), Vec3::X, 1.).unwrap(),
    ])
    .unwrap();
    let (mut p, mut voices, id) = fixture(extent, PlayOptions::once().with_acoustic_start());
    quantum(&mut p, &mut voices, id);
    assert_eq!(voices[&id].info.current_frame, 0);
    p.replace_acoustic_response(response(
        id,
        DirectSolveStatus::Solved,
        vec![DirectLobeTarget {
            lobe_id: 1,
            direction: Vec3::X,
            gain: WOOD,
            power: 1.,
        }],
    ));
    quantum(&mut p, &mut voices, id);
    assert_eq!(
        p.native_direct_lobe_source_states[&id].slots[1]
            .filter
            .current_gain,
        WOOD
    );
    assert_eq!(p.native_environment_source_states[&id].current_gain, WOOD);
    assert_eq!(
        p.native_direct_lobe_source_states[&id].slots[1].current_direction,
        Vec3::X
    );
}
#[test]
fn acoustic_start_muted_route_does_not_eat_pcm_or_wait_forever() {
    let (_, mut voices, id) = fixture(
        SourceExtent::Point,
        PlayOptions::once().with_acoustic_start(),
    );
    let voice = voices.get_mut(&id).unwrap();
    assert!(!voice.advance_silently(BLOCK));
    assert_eq!(voice.info.current_frame, 0);
    for _ in 0..3 {
        assert!(!voice.advance_silently(BLOCK));
    }
    assert!(voice.advance_silently(BLOCK));
    assert!(!voice.advance_silently(BLOCK)); // no duplicate timeout
    assert_eq!(voice.info.current_frame, 0);
    assert!(voice.take_reclaim().is_some());
}

#[test]
fn acoustic_start_bypass_only_route_has_no_wait() {
    let (mut p, mut voices, id) = fixture(
        SourceExtent::Point,
        PlayOptions::once()
            .with_acoustic_start()
            .with_direct_path(DirectPath::world().with_geometry(DirectGeometry::BypassTransmission))
            .with_environment_send(EnvironmentSend::disabled()),
    );
    quantum(&mut p, &mut voices, id);
    assert_eq!(voices[&id].info.current_frame, BLOCK);
}
/// Captured live bad-audio-case geometry + actual game PCM. Explicit offline audio
/// review, never normal tests/device I/O; caller supplies the exported .f32 directory.
#[test]
#[ignore = "explicit captured-game-PCM replay; set RE_FLORA_KICK_PCM_DIR"]
fn captured_canopy_pcm_keeps_its_temporal_tail_and_output_headroom() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("RE_FLORA_KICK_PCM_DIR")
            .expect("supply current game variant-0..3.f32 files"),
    );
    for variant in 0..4 {
        let bytes = std::fs::read(directory.join(format!("variant-{variant}.f32"))).unwrap();
        let pcm = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(pcm.len(), 96_000);
        let (mut p, mut voices, id) = fixture(
            SourceExtent::weighted_samples(vec![
                ExtentSample::new(ExtentSampleId(1), Vec3::ZERO, 1.).unwrap(),
            ])
            .unwrap(),
            PlayOptions::once().with_acoustic_start(),
        );
        let gain = 135.03580f32;
        let voice = voices.get_mut(&id).unwrap();
        voice.audio_data = Arc::new(PetalSonicAudioData::new(
            pcm.clone(),
            48_000,
            1,
            Duration::from_secs(2),
        ));
        voice.info = crate::playback::PlaybackInfo::new(pcm.len());
        voice.resume();
        voice
            .render_state
            .set_spatial_pose(Pose::from_position(Vec3::new(
                1.0274838, 0.7554647, 1.0493407,
            )));
        voice.set_mix_parameters(crate::domain::BusParams {
            gain_db: 20. * gain.log10(),
            ..Default::default()
        });
        p.distance_scaler = 15.;
        p.listener_position = Vec3::new(0.9601637, 0.49553716, 0.9404505);
        p.listener_front = Vec3::new(0.6255688, 0.19353987, 0.7557817).normalize();
        p.listener_right = p.listener_front.cross(Vec3::Y).normalize();
        p.listener_up = p.listener_right.cross(p.listener_front).normalize();
        p.hrtf_gain_linear = 1.;
        p.replace_acoustic_response(response(
            id,
            DirectSolveStatus::Solved,
            vec![
                DirectLobeTarget {
                    lobe_id: 1,
                    direction: Vec3::new(0.072954774, 0.9480273, 0.30971232),
                    gain: [0.4883019, 0.48702994, 0.4867837],
                    power: 0.43002132,
                },
                DirectLobeTarget {
                    lobe_id: 2,
                    direction: Vec3::new(0.51902246, 0.83996063, -0.15837201),
                    gain: [0.1902786, 0.1891306, 0.18890797],
                    power: 0.06496983,
                },
                DirectLobeTarget {
                    lobe_id: 3,
                    direction: Vec3::new(0.025963921, 0.7900866, 0.61244524),
                    gain: [0.52860695, 0.5279952, 0.5278769],
                    power: 0.5050089,
                },
            ],
        ));
        let mut stereo = Vec::new();
        let mut events = Vec::new();
        for _ in 0..96 {
            let mut block = vec![0.; BLOCK * 2];
            p.process_spatial_sources_with_metrics(
                &[id],
                &mut voices,
                &mut block,
                SpatialRenderContext::default(),
                &mut events,
            )
            .unwrap();
            stereo.extend(block.into_iter().map(|s| s * 10f32.powf(-6. / 20.)));
        }
        let energy = |samples: &[f32]| samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
        let sum = energy(&stereo);
        let peak = stereo.iter().fold(0_f32, |a, b| a.max(b.abs()));
        let early = energy(&stereo[..9600]) / sum;
        let tail = energy(&stereo[24_000..]) / sum;
        let mut accumulated = 0.;
        let t95 = stereo
            .chunks_exact(2)
            .position(|f| {
                accumulated += energy(f);
                accumulated >= sum * 0.95
            })
            .unwrap() as f32
            / 48_000.;
        eprintln!(
            "[KICK_CANOPY_PCM] variant={variant} peak={peak:.6} clipped={} first100ms={early:.6} after250ms={tail:.6} t95={t95:.6}",
            stereo.iter().filter(|s| s.abs() > 1.).count()
        );
        assert!(
            early < 0.6 && tail > 0.1,
            "finite waveform must not collapse into a cold-start burst"
        );
        assert!(
            peak < 1.,
            "isolated captured-viewpoint cue must preserve output headroom"
        );
        assert_eq!(voices[&id].info.current_frame, 96_000);
    }
}

#[test]
fn acoustic_start_explicit_stop_cancels_pending_voice_without_wait_or_pcm() {
    let (mut p, mut voices, id) = fixture(
        SourceExtent::Point,
        PlayOptions::once().with_acoustic_start(),
    );
    quantum(&mut p, &mut voices, id);
    voices.get_mut(&id).unwrap().begin_fade_out();
    quantum(&mut p, &mut voices, id);
    assert_eq!(voices[&id].info.current_frame, 0);
    assert!(voices.get_mut(&id).unwrap().take_reclaim().is_some());
}
