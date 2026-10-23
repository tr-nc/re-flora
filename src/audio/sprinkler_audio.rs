use super::{spatial_sound_manager::TransientSpatialEmitter, SpatialSoundManager};
use crate::sprinkler::SpraySnapshot;
use glam::Vec3;
use petalsonic::{PlayOptions, PlaybackTag, ResidentClip, SampleEnvelopeControl};

pub(crate) const MAX_SPRINKLER_VOICES: usize = 16;
#[derive(Clone, Copy, Debug)]
pub(crate) struct SprinklerSource {
    pub id: u32,
    pub position: Vec3,
    pub animation_phase: f32,
}
struct Voice {
    id: u32,
    emitter: Option<TransientSpatialEmitter>,
    envelope: SampleEnvelopeControl,
    started: bool,
    retiring: bool,
    initial_phase: f32,
}
/// Owns every reservation until BOTH emitter destruction and render reclamation finish.
pub(super) struct SprinklerAudio {
    manager: SpatialSoundManager,
    clip: Option<ResidentClip>,
    voices: Vec<Voice>,
    retry_after: std::time::Instant,
    last_anchor: Option<(std::time::Instant, f64)>,
}
impl SprinklerAudio {
    pub fn new(manager: SpatialSoundManager) -> Self {
        // Decode once, including failure: no per-frame reload/retry storm for missing audio.
        let clip = manager
            .cached_clip(super::runtime_assets::SPRINKLER_LOOP_PATH)
            .and_then(|clip| {
                anyhow::ensure!(
                    clip.sample_rate() == crate::sprinkler::LOOP_SAMPLE_RATE
                        && clip.total_frames() == crate::sprinkler::LOOP_FRAMES,
                    "sprinkler clip does not match the authored timing contract"
                );
                log::info!("[SPRINKLER_AUDIO] clip={} frames={} sample_rate={} loop_seconds={} bursts=6 frequency={} duty_percent={} initial_peak_ms=75 source_gain_db=-12",
                    super::runtime_assets::SPRINKLER_LOOP_PATH, clip.total_frames(), clip.sample_rate(),
                    crate::sprinkler::LOOP_SECONDS, crate::sprinkler::PULSE_FREQUENCY,
                    crate::sprinkler::PULSE_DUTY_PERCENT);
                Ok(clip)
            })
            .map_err(|error| log::warn!("[SPRINKLER_AUDIO] unavailable: {error}"))
            .ok();
        Self {
            manager,
            clip,
            voices: Vec::with_capacity(MAX_SPRINKLER_VOICES),
            retry_after: std::time::Instant::now(),
            last_anchor: None,
        }
    }
    pub fn prepare(
        &mut self,
        snapshot: Option<SpraySnapshot>,
        sources: &[SprinklerSource],
        listener: Vec3,
        running: bool,
    ) {
        let now = std::time::Instant::now();
        if let Some(snapshot) = snapshot.filter(|_| running) {
            if let Some((last_time, last_phase)) = self.last_anchor {
                let elapsed = now.duration_since(last_time).as_secs_f64();
                let advanced = (snapshot.audio_loop_seconds - last_phase)
                    .rem_euclid(crate::sprinkler::LOOP_SECONDS);
                if needs_reanchor(elapsed, advanced) {
                    // A stale control, clock reset or device/application interruption needs
                    // a fresh start, not a click-producing corrective seek on a live voice.
                    for voice in &mut self.voices {
                        voice.retiring = true;
                    }
                }
            }
            self.last_anchor = Some((now, snapshot.audio_loop_seconds));
        } else {
            self.last_anchor = None;
        }
        // Only nearest audible candidates. Existing membership gets 20% distance hysteresis;
        // stable identity breaks ties, never vector ordering or hardware design.
        let mut candidates: Vec<_> = sources
            .iter()
            .filter_map(|s| {
                let distance = s.position.distance_squared(listener);
                let retained = self.voices.iter().any(|v| v.id == s.id && !v.retiring);
                (distance.is_finite()
                    && distance < 32.0 * 32.0
                    && running
                    && self.clip.is_some()
                    && snapshot.is_some_and(|s| s.duty > 0.0))
                .then_some((distance * if retained { 0.64 } else { 1.0 }, s))
            })
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
        candidates.truncate(MAX_SPRINKLER_VOICES);
        for voice in &mut self.voices {
            if !candidates.iter().any(|(_, s)| s.id == voice.id) {
                voice.retiring = true;
            }
            if voice.retiring {
                voice.envelope.anchor(0.0, 0.0, 0.0);
                if let Some(emitter) = voice.emitter {
                    match self.manager.destroy_transient_spatial_emitter(emitter) {
                        Ok(()) => voice.emitter = None,
                        Err(e) => {
                            log::warn!("[SPRINKLER_AUDIO] destroy retry id={} error={e}", voice.id)
                        }
                    }
                }
            }
        }
        self.voices
            .retain(|v| !v.retiring || v.emitter.is_some() || v.envelope.has_render_owners());
        for (_, source) in candidates {
            let Some(snapshot) = snapshot else {
                break;
            };
            if self.voices.iter().any(|v| v.id == source.id && v.retiring) {
                continue;
            }
            if !self.voices.iter().any(|v| v.id == source.id) {
                if self.voices.len() == MAX_SPRINKLER_VOICES
                    || std::time::Instant::now() < self.retry_after
                {
                    continue;
                }
                let envelope = SampleEnvelopeControl::default();
                match self.manager.prepare_spatial_clip_emitter(
                    self.clip
                        .as_ref()
                        .unwrap()
                        .clone()
                        .with_sample_envelope(envelope.clone()),
                    super::mixer::AudioCategory::Sprinklers,
                    -12.0,
                    source.position + Vec3::Y * 0.04,
                ) {
                    Ok(emitter) => self.voices.push(Voice {
                        id: source.id,
                        emitter: Some(emitter),
                        envelope,
                        started: false,
                        retiring: false,
                        initial_phase: snapshot.audio_loop_phase(source.animation_phase),
                    }),
                    Err(e) => {
                        self.retry_after =
                            std::time::Instant::now() + std::time::Duration::from_secs(1);
                        log::warn!(
                            "[SPRINKLER_AUDIO] prepare failed id={} error={e}",
                            source.id
                        );
                        continue;
                    }
                }
            }
            let voice = self.voices.iter_mut().find(|v| v.id == source.id).unwrap();
            self.manager
                .move_sprinkler_emitter(voice.emitter.unwrap(), source.position + Vec3::Y * 0.04);
            voice.initial_phase = snapshot.audio_loop_phase(source.animation_phase);
            // The recording already contains the desired texture and six bursts.
            // Continuous envelope is only a bounded stale/retirement gate, not a pulse shaper.
            voice.envelope.anchor(0.0, 1.0, 1.0);
        }
    }
    pub fn finish_publication(&mut self, published: bool) {
        for voice in &mut self.voices {
            if !published {
                voice.envelope.anchor(0.0, 0.0, 0.0);
                voice.retiring = true;
            }
            if voice.retiring || voice.started {
                continue;
            }
            match self.manager.play_controlled_transient(
                voice.emitter.unwrap(),
                PlayOptions::looping(),
                PlaybackTag(0x5350_0000_0000_0000 | u64::from(voice.id)),
            ) {
                Ok(_) => {
                    // One start/seek, never per-frame corrective seeking or per-burst restarts.
                    match self
                        .manager
                        .seek_sprinkler_emitter(voice.emitter.unwrap(), voice.initial_phase)
                    {
                        Ok(()) => voice.started = true,
                        Err(error) => {
                            voice.envelope.anchor(0.0, 0.0, 0.0);
                            voice.retiring = true;
                            self.retry_after =
                                std::time::Instant::now() + std::time::Duration::from_secs(1);
                            log::warn!(
                                "[SPRINKLER_AUDIO] initial seek failed id={} error={error}",
                                voice.id
                            );
                        }
                    }
                }
                Err(e) => {
                    voice.envelope.anchor(0.0, 0.0, 0.0);
                    voice.retiring = true;
                    self.retry_after =
                        std::time::Instant::now() + std::time::Duration::from_secs(1);
                    log::warn!("[SPRINKLER_AUDIO] play failed id={} error={e}", voice.id);
                }
            }
        }
    }
    pub fn clear(&mut self) {
        self.prepare(None, &[], Vec3::ZERO, false);
    }
    pub(super) fn counts(&self) -> (usize, usize, usize, usize) {
        (
            self.voices
                .iter()
                .filter(|v| v.started && !v.retiring)
                .count(),
            self.voices
                .iter()
                .filter(|v| !v.started && !v.retiring)
                .count(),
            self.voices.iter().filter(|v| v.retiring).count(),
            self.voices
                .iter()
                .filter(|v| v.envelope.has_render_owners())
                .count(),
        )
    }
}
impl Drop for SprinklerAudio {
    fn drop(&mut self) {
        self.clear();
    }
}

// Ignore ordinary frame jitter; retire after the envelope's stale horizon or a real
// world-clock discontinuity. Normal playback always runs at its original speed.
fn needs_reanchor(real_seconds: f64, world_seconds: f64) -> bool {
    real_seconds >= petalsonic::ENVELOPE_STALE_SECONDS
        || (real_seconds - world_seconds).abs() > 0.05
}

#[cfg(test)]
mod tests {
    use super::*;
    use petalsonic::{
        AcousticHit, AcousticRay, AcousticRayQuerySnapshot, AcousticSceneSnapshot,
        EnvironmentalAcousticsBudget,
    };
    use std::sync::Arc;
    struct EmptyScene;
    impl AcousticRayQuerySnapshot for EmptyScene {
        fn trace_any_hit_batch(&self, _: &[AcousticRay], _: &[f32], _: &[f32], hits: &mut [bool]) {
            hits.fill(false);
        }
        fn trace_closest_hit_batch(
            &self,
            _: &[AcousticRay],
            _: &[f32],
            _: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            hits.fill(None);
        }
    }
    #[test]
    fn loop_epoch_reanchors_only_after_stale_or_discontinuous_time() {
        assert!(!needs_reanchor(1.0 / 60.0, 1.0 / 60.0));
        assert!(!needs_reanchor(1.0 / 15.0, 1.0 / 15.0 + 0.001));
        assert!(needs_reanchor(0.25, 0.25));
        assert!(needs_reanchor(0.02, 0.3));
        assert!(needs_reanchor(0.1, 0.0));
    }

    fn manager() -> SpatialSoundManager {
        // Pinned nonexistent device never opens an audible output; real engine still runs.
        SpatialSoundManager::new(
            256,
            AcousticSceneSnapshot::new(1, Arc::new(EmptyScene)),
            Some("sprinkler-test-no-such-output-7e94".into()),
            EnvironmentalAcousticsBudget::default(),
        )
        .unwrap()
    }
    fn snapshot() -> SpraySnapshot {
        SpraySnapshot::new([0.0, 0.0, 0.0, 0.0], 20.0, 30.0)
    }
    fn sources() -> Vec<SprinklerSource> {
        (1..=24)
            .map(|id| SprinklerSource {
                id,
                position: Vec3::new(id as f32 * 0.1, 0.0, 0.0),
                animation_phase: 0.0,
            })
            .collect()
    }
    #[test]
    #[ignore = "native engine with pinned nonexistent device; run explicitly under heavy lock"]
    fn real_engine_prepare_publish_play_cap_pause_removal_and_failed_publication() {
        let manager = manager();
        let mut audio = SprinklerAudio::new(manager.clone());
        let sources = sources();
        audio.prepare(Some(snapshot()), &sources, Vec3::ZERO, true);
        assert_eq!(audio.counts(), (0, 16, 0, 16));
        assert_eq!(
            manager.engine_voice_count(),
            0,
            "prepare must not start voices"
        );
        manager.publish_spatial_frame(0.0).unwrap();
        audio.finish_publication(true);
        assert_eq!(audio.counts().0, 16);
        assert_eq!(manager.engine_voice_count(), 16);
        let mut moved = sources.clone();
        let emitter = audio
            .voices
            .iter()
            .find(|v| v.id == moved[0].id)
            .unwrap()
            .emitter
            .unwrap();
        moved[0].position += Vec3::Y * 0.01;
        audio.prepare(Some(snapshot()), &moved, Vec3::ZERO, true);
        audio.finish_publication(true);
        assert_eq!(
            audio
                .voices
                .iter()
                .find(|v| v.id == moved[0].id)
                .unwrap()
                .emitter,
            Some(emitter)
        );
        assert_eq!(
            manager.sprinkler_emitter_position(emitter),
            moved[0].position + Vec3::Y * 0.04
        );
        assert_eq!(
            manager.engine_voice_count(),
            16,
            "movement must not restart or duplicate playback"
        );
        audio.prepare(Some(snapshot()), &sources, Vec3::ZERO, false);
        assert_eq!(audio.counts().0, 0);
        for _ in 0..100 {
            audio.prepare(Some(snapshot()), &sources, Vec3::ZERO, false);
            if audio.voices.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            audio.voices.is_empty(),
            "render retirement must release all ownership"
        );
        audio.prepare(Some(snapshot()), &sources, Vec3::ZERO, true);
        assert!(manager.publish_spatial_frame(f64::NAN).is_err());
        audio.finish_publication(false);
        assert_eq!(audio.counts().0, 0);
        audio.clear();
        assert!(
            audio.voices.is_empty(),
            "failed publication leaves no unplayed resources"
        );
        manager.stop().unwrap();
        audio.prepare(Some(snapshot()), &sources, Vec3::ZERO, true);
        assert!(
            audio.voices.is_empty(),
            "closed engine creation is harmless to source facts"
        );
        assert_eq!(sources.len(), 24);
    }
    #[test]
    #[ignore = "native engine with pinned nonexistent device; run explicitly under heavy lock"]
    fn stable_selection_hysteresis_and_zero_duty() {
        let manager = manager();
        let mut audio = SprinklerAudio::new(manager.clone());
        let mut sources = sources();
        audio.prepare(Some(snapshot()), &sources, Vec3::ZERO, true);
        let selected: Vec<_> = audio.voices.iter().map(|v| v.id).collect();
        sources.reverse();
        audio.prepare(Some(snapshot()), &sources, Vec3::new(0.01, 0.0, 0.0), true);
        assert_eq!(
            selected,
            audio.voices.iter().map(|v| v.id).collect::<Vec<_>>()
        );
        audio.prepare(
            Some(SpraySnapshot::new([0.0; 4], 20.0, 0.0)),
            &sources,
            Vec3::ZERO,
            true,
        );
        assert!(audio.voices.is_empty());
        manager.stop().unwrap();
    }
}
