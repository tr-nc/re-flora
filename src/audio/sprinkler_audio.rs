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
}
/// Owns every reservation until BOTH emitter destruction and render reclamation finish.
pub(super) struct SprinklerAudio {
    manager: SpatialSoundManager,
    clip: ResidentClip,
    voices: Vec<Voice>,
}
impl SprinklerAudio {
    pub fn new(manager: SpatialSoundManager) -> Self {
        Self {
            manager,
            clip: ResidentClip::from_mono_pcm(
                super::sprinkler_synthesis::water_pcm(),
                super::sprinkler_synthesis::SAMPLE_RATE,
            )
            .expect("finite authored PCM"),
            voices: Vec::with_capacity(MAX_SPRINKLER_VOICES),
        }
    }
    pub fn prepare(
        &mut self,
        snapshot: Option<SpraySnapshot>,
        sources: &[SprinklerSource],
        listener: Vec3,
        running: bool,
    ) {
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
                if self.voices.len() == MAX_SPRINKLER_VOICES {
                    continue;
                }
                let envelope = SampleEnvelopeControl::default();
                match self.manager.prepare_sprinkler_emitter(
                    self.clip.clone().with_sample_envelope(envelope.clone()),
                    source.position + Vec3::Y * 0.04,
                ) {
                    Ok(emitter) => self.voices.push(Voice {
                        id: source.id,
                        emitter: Some(emitter),
                        envelope,
                        started: false,
                        retiring: false,
                    }),
                    Err(e) => {
                        log::warn!(
                            "[SPRINKLER_AUDIO] prepare failed id={} error={e}",
                            source.id
                        );
                        continue;
                    }
                }
            }
            let voice = self.voices.iter_mut().find(|v| v.id == source.id).unwrap();
            voice.envelope.anchor(
                snapshot.phase(source.animation_phase),
                f64::from(snapshot.frequency),
                f64::from(snapshot.duty),
            );
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
                Ok(_) => voice.started = true,
                Err(e) => {
                    voice.envelope.anchor(0.0, 0.0, 0.0);
                    voice.retiring = true;
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
