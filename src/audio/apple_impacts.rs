//! Finite spatial sounds for committed first-ground-contact events.
//! Physics owns deduplication; this owner never replays rejected/muted events later.
use super::{
    mixer::AudioCategory, spatial_sound_manager::TransientSpatialEmitter, SpatialSoundManager,
};
use glam::Vec3;
use petalsonic::{PlayOptions, PlaybackTag, ResidentClip, SampleEnvelopeControl};
use rand::{rngs::SmallRng, RngExt, SeedableRng};
use std::time::{Duration, Instant};

const MAX_VOICES: usize = 128;

#[derive(Clone, Copy, Debug)]
pub(crate) struct AppleImpact {
    pub body_id: u64,
    pub position: Vec3,
}

struct Voice {
    body_id: u64,
    emitter: Option<TransientSpatialEmitter>,
    envelope: SampleEnvelopeControl,
    deadline: Instant,
    started: bool,
    retiring: bool,
    #[cfg(test)]
    variant: usize,
}

pub(super) struct AppleImpactAudio {
    manager: SpatialSoundManager,
    clips: Vec<ResidentClip>,
    voices: Vec<Voice>,
    rng: SmallRng,
}

impl AppleImpactAudio {
    pub fn new(manager: SpatialSoundManager) -> Self {
        let clips = super::runtime_assets::APPLE_IMPACT_CLIPS
            .iter()
            .filter_map(|path| {
                manager
                    .cached_clip(path)
                    .map_err(|error| {
                        log::warn!("[APPLE_IMPACT_AUDIO] unavailable path={path}: {error}")
                    })
                    .ok()
            })
            .collect();
        Self {
            manager,
            clips,
            voices: Vec::with_capacity(MAX_VOICES),
            rng: SmallRng::from_rng(&mut rand::rng()),
        }
    }

    pub fn prepare(&mut self, events: &[AppleImpact], enabled: bool) {
        let now = Instant::now();
        for voice in &mut self.voices {
            voice.retiring |= !enabled || now >= voice.deadline;
            if voice.retiring {
                voice.envelope.anchor(0.0, 0.0, 0.0);
                if let Some(emitter) = voice.emitter {
                    match self.manager.destroy_transient_spatial_emitter(emitter) {
                        Ok(()) => voice.emitter = None,
                        Err(error) => log::warn!(
                            "[APPLE_IMPACT_AUDIO] retirement retry body={}: {error}",
                            voice.body_id
                        ),
                    }
                }
            } else {
                voice.envelope.anchor(0.0, 1.0, 1.0);
            }
        }
        // Include queued/render-retirement owners in the cap, not only live emitters.
        let before = self.voices.len();
        self.voices
            .retain(|v| !v.retiring || v.emitter.is_some() || v.envelope.has_render_owners());
        if before > 0 && self.voices.is_empty() {
            log::info!("[APPLE_IMPACT_AUDIO] all voices reclaimed");
        }
        if !enabled || self.clips.is_empty() {
            return;
        }
        for event in events {
            if !event.position.is_finite() {
                continue;
            }
            if self.voices.len() == MAX_VOICES {
                log::warn!(
                    "[APPLE_IMPACT_AUDIO] capacity reached; dropping body={} without backlog",
                    event.body_id
                );
                continue;
            }
            let variant = self.rng.random_range(0..self.clips.len());
            let clip = &self.clips[variant];
            let duration = clip.total_frames() as f64 / f64::from(clip.sample_rate());
            let envelope = SampleEnvelopeControl::default();
            envelope.anchor(0.0, 1.0, 1.0);
            // Compensate only the continuous safety gate's 0.72 steady gain. No PCM edits.
            let gain_db = -20.0 * 0.72_f32.log10();
            match self.manager.prepare_spatial_clip_emitter(
                clip.clone().with_sample_envelope(envelope.clone()),
                AudioCategory::AppleImpacts,
                gain_db,
                // Keep the acoustic origin just above the solver surface, not inside dirt.
                event.position + Vec3::Y * (0.5 / 256.0),
            ) {
                Ok(emitter) => {
                    self.voices.push(Voice {
                        body_id: event.body_id,
                        emitter: Some(emitter),
                        envelope,
                        deadline: now + Duration::from_secs_f64(duration + 0.5),
                        started: false,
                        retiring: false,
                        #[cfg(test)]
                        variant,
                    });
                    log::info!("[APPLE_IMPACT_AUDIO] prepared body={} variant={} position={:?} duration={duration:.3}", event.body_id, variant + 1, event.position);
                }
                Err(error) => log::warn!(
                    "[APPLE_IMPACT_AUDIO] prepare failed body={}: {error}",
                    event.body_id
                ),
            }
        }
    }

    pub fn finish_publication(&mut self, published: bool) {
        for voice in &mut self.voices {
            if !published {
                voice.retiring = true;
            }
            if voice.retiring {
                voice.envelope.anchor(0.0, 0.0, 0.0);
                continue;
            }
            if voice.started {
                continue;
            }
            match self.manager.play_controlled_transient(
                voice.emitter.unwrap(),
                PlayOptions::once(),
                PlaybackTag(0x4150_0000_0000_0000 | voice.body_id),
            ) {
                Ok(_) => {
                    voice.started = true;
                    log::info!("[APPLE_IMPACT_AUDIO] playing body={}", voice.body_id);
                }
                Err(error) => {
                    voice.retiring = true;
                    voice.envelope.anchor(0.0, 0.0, 0.0);
                    log::warn!(
                        "[APPLE_IMPACT_AUDIO] play failed body={}: {error}",
                        voice.body_id
                    );
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.prepare(&[], false);
    }
}
impl Drop for AppleImpactAudio {
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

    #[test]
    #[ignore = "native audio owner acceptance; nonexistent output device, run explicitly"]
    fn finite_impacts_choose_variants_publish_before_play_and_retire_without_backlog() {
        let manager = SpatialSoundManager::new(
            256,
            AcousticSceneSnapshot::new(1, Arc::new(EmptyScene)),
            Some("apple-impact-test-no-output-device".into()),
            EnvironmentalAcousticsBudget::default(),
        )
        .unwrap();
        let mut audio = AppleImpactAudio::new(manager.clone());
        audio.rng = SmallRng::seed_from_u64(71);
        assert_eq!(audio.clips.len(), 3);
        let events: Vec<_> = (0..MAX_VOICES + 3)
            .map(|i| AppleImpact {
                body_id: i as u64,
                position: Vec3::new(i as f32 * 0.01, 0.0, 0.0),
            })
            .collect();
        audio.prepare(&events, true);
        assert_eq!(audio.voices.len(), MAX_VOICES);
        assert_eq!(manager.engine_voice_count(), 0, "prepare must not play");
        for variant in 0..3 {
            assert!(audio.voices.iter().any(|v| v.variant == variant));
        }
        manager.publish_spatial_frame(0.0).unwrap();
        audio.finish_publication(true);
        assert_eq!(manager.engine_voice_count(), MAX_VOICES);
        audio.prepare(&[], true);
        audio.finish_publication(true);
        assert_eq!(
            manager.engine_voice_count(),
            MAX_VOICES,
            "no second play for an existing event"
        );
        // Expiry uses the real owner path, without waiting through every 2-second recording.
        for voice in &mut audio.voices {
            voice.deadline = Instant::now();
        }
        for _ in 0..500 {
            audio.prepare(&[], true);
            if audio.voices.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(audio.voices.is_empty());
        audio.prepare(&events[..1], false);
        audio.prepare(&[], true);
        assert!(
            audio.voices.is_empty(),
            "muted events are consumed, never queued for unmute"
        );
        audio.prepare(&events[..1], true);
        audio.finish_publication(false);
        audio.clear();
        assert!(
            audio.voices.is_empty(),
            "failed publication must discard unplayed resources"
        );
        manager.stop().unwrap();
    }
}
