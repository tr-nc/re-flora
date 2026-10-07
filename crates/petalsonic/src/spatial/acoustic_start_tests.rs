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
