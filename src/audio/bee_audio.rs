//! One bounded positional loop per selected flying resident, never a hive ambience bed.
use super::{
    mixer::AudioCategory, spatial_sound_manager::TransientSpatialEmitter, SpatialSoundManager,
};
use crate::particles::bees::BeeFlight;
use glam::Vec3;
use petalsonic::{PlayOptions, PlaybackTag, ResidentClip, SampleEnvelopeControl};
use std::time::{Duration, Instant};

const MAX_VOICES: usize = 16;
const AUDIBLE_RADIUS: f32 = 2.0;

struct Voice {
    id: u64,
    emitter: Option<TransientSpatialEmitter>,
    envelope: SampleEnvelopeControl,
    started: bool,
    retiring: bool,
}

pub(super) struct BeeAudio {
    manager: SpatialSoundManager,
    clip: Option<ResidentClip>,
    voices: Vec<Voice>,
    retry_after: Instant,
    starts: u64,
}

// Stable identity breaks ties; existing voices get 20% distance hysteresis so
// crossing bees do not continually steal each other's slots. Invalid poses never
// reach the spatial frame. The domain emits each resident at most once.
fn select<'a>(sources: &'a [BeeFlight], listener: Vec3, retained: &[u64]) -> Vec<&'a BeeFlight> {
    let mut candidates: Vec<_> = sources
        .iter()
        .filter_map(|source| {
            let distance = source.position.distance_squared(listener);
            (distance.is_finite() && distance < AUDIBLE_RADIUS * AUDIBLE_RADIUS).then_some((
                distance
                    * if retained.contains(&source.id) {
                        0.64
                    } else {
                        1.0
                    },
                source,
            ))
        })
        .collect();
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
    candidates.truncate(MAX_VOICES);
    candidates.into_iter().map(|(_, source)| source).collect()
}

fn phase(id: u64) -> f32 {
    // De-correlate the identical recording without changing pitch or playback speed.
    let hash = id.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    (hash >> 40) as f32 / 16_777_216.0
}

impl BeeAudio {
    pub fn new(manager: SpatialSoundManager) -> Self {
        let clip = manager
            .cached_clip(super::runtime_assets::BEE_LOOP_PATH)
            .inspect(|clip| {
                log::info!(
                    "[BEE_AUDIO] clip={} frames={} sample_rate={} loop_seconds={} max_voices={MAX_VOICES}",
                    super::runtime_assets::BEE_LOOP_PATH,
                    clip.total_frames(),
                    clip.sample_rate(),
                    clip.total_frames() as f64 / f64::from(clip.sample_rate()),
                );
            })
            .map_err(|error| log::warn!("[BEE_AUDIO] unavailable: {error}"))
            .ok();
        Self {
            manager,
            clip,
            voices: Vec::with_capacity(MAX_VOICES),
            retry_after: Instant::now(),
            starts: 0,
        }
    }

    pub fn prepare(&mut self, sources: &[BeeFlight], listener: Vec3, enabled: bool) {
        let retained: Vec<_> = self
            .voices
            .iter()
            .filter(|v| !v.retiring)
            .map(|v| v.id)
            .collect();
        let selected = if enabled && self.clip.is_some() {
            select(sources, listener, &retained)
        } else {
            Vec::new()
        };
        for voice in &mut self.voices {
            if !selected.iter().any(|source| source.id == voice.id) {
                voice.retiring = true;
            }
            if voice.retiring {
                voice.envelope.anchor(0.0, 0.0, 0.0);
                if let Some(emitter) = voice.emitter {
                    match self.manager.destroy_transient_spatial_emitter(emitter) {
                        Ok(()) => voice.emitter = None,
                        Err(error) => {
                            log::warn!("[BEE_AUDIO] destroy retry id={} error={error}", voice.id)
                        }
                    }
                }
            }
        }
        // Queued/render-retiring ownership counts against the cap too. Never
        // replace a resident's slot while the audio thread still owns its old loop.
        self.voices
            .retain(|v| !v.retiring || v.emitter.is_some() || v.envelope.has_render_owners());
        for source in selected {
            if let Some(voice) = self.voices.iter_mut().find(|v| v.id == source.id) {
                if !voice.retiring {
                    self.manager
                        .move_transient_emitter(voice.emitter.unwrap(), source.position);
                    voice.envelope.anchor(0.0, 1.0, 1.0);
                }
                continue;
            }
            if self.voices.len() >= MAX_VOICES || Instant::now() < self.retry_after {
                continue;
            }
            let envelope = SampleEnvelopeControl::default();
            match self.manager.prepare_spatial_clip_emitter(
                self.clip
                    .as_ref()
                    .unwrap()
                    .clone()
                    .with_sample_envelope(envelope.clone()),
                AudioCategory::Bees,
                0.0,
                source.position,
            ) {
                Ok(emitter) => {
                    envelope.anchor(0.0, 1.0, 1.0);
                    self.voices.push(Voice {
                        id: source.id,
                        emitter: Some(emitter),
                        envelope,
                        started: false,
                        retiring: false,
                    });
                }
                Err(error) => {
                    self.retry_after = Instant::now() + Duration::from_secs(1);
                    log::warn!("[BEE_AUDIO] prepare failed id={} error={error}", source.id);
                }
            }
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
            let emitter = voice.emitter.unwrap();
            let result = self
                .manager
                .play_controlled_transient(
                    emitter,
                    PlayOptions::looping(),
                    PlaybackTag(0x4245_0000_0000_0000 | voice.id),
                )
                .and_then(|_| {
                    self.manager
                        .seek_transient_emitter(emitter, phase(voice.id))
                });
            match result {
                Ok(()) => {
                    voice.started = true;
                    self.starts += 1;
                    log::debug!(
                        "[BEE_AUDIO] started id={} phase={}",
                        voice.id,
                        phase(voice.id)
                    );
                }
                Err(error) => {
                    voice.envelope.anchor(0.0, 0.0, 0.0);
                    voice.retiring = true;
                    self.retry_after = Instant::now() + Duration::from_secs(1);
                    log::warn!("[BEE_AUDIO] start failed id={} error={error}", voice.id);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.prepare(&[], Vec3::ZERO, false);
    }

    pub fn diagnostics(&self) -> (usize, u64) {
        (
            self.voices
                .iter()
                .filter(|v| v.started && !v.retiring)
                .count(),
            self.starts,
        )
    }
}

impl Drop for BeeAudio {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_selection_is_bounded_stable_and_rejects_invalid_positions() {
        let mut sources: Vec<_> = (0..64)
            .map(|id| BeeFlight {
                id,
                position: Vec3::X * (0.1 + id as f32 * 0.01),
            })
            .collect();
        sources.extend([
            BeeFlight {
                id: 64,
                position: Vec3::NAN,
            },
            BeeFlight {
                id: 65,
                position: Vec3::X * AUDIBLE_RADIUS,
            },
        ]);
        let ids = |s: &[BeeFlight], retained: &[u64]| {
            select(s, Vec3::ZERO, retained)
                .iter()
                .map(|b| b.id)
                .collect::<Vec<_>>()
        };
        let selected = ids(&sources, &[]);
        assert_eq!(selected, (0..16).collect::<Vec<_>>());
        sources.reverse();
        assert_eq!(ids(&sources, &[]), selected);
        // A challenger only slightly nearer must not displace retained membership.
        sources.iter_mut().find(|b| b.id == 16).unwrap().position = Vec3::X * 0.249;
        assert_eq!(ids(&sources, &selected), selected);
        sources.iter_mut().find(|b| b.id == 16).unwrap().position = Vec3::X * 0.01;
        assert!(ids(&sources, &selected).contains(&16));
        assert!(select(&sources, Vec3::NAN, &[]).is_empty());
        for source in &mut sources {
            source.position = Vec3::X;
        }
        assert_eq!(ids(&sources, &[]), selected);
    }

    #[test]
    #[ignore = "native engine lifecycle; pinned nonexistent output, run explicitly"]
    fn real_bee_loop_prepare_publish_move_pause_remove_and_failed_publication() {
        use petalsonic::{
            AcousticHit, AcousticRay, AcousticRayQuerySnapshot, AcousticSceneSnapshot,
            EnvironmentalAcousticsBudget,
        };
        use std::sync::Arc;
        struct EmptyScene;
        impl AcousticRayQuerySnapshot for EmptyScene {
            fn trace_any_hit_batch(
                &self,
                _: &[AcousticRay],
                _: &[f32],
                _: &[f32],
                hits: &mut [bool],
            ) {
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
        let manager = SpatialSoundManager::new(
            256,
            AcousticSceneSnapshot::new(1, Arc::new(EmptyScene)),
            Some("bee-test-no-such-output-72b8".into()),
            EnvironmentalAcousticsBudget::default(),
        )
        .unwrap();
        let mut audio = BeeAudio::new(manager.clone());
        let clip = audio.clip.as_ref().unwrap();
        assert_eq!(clip.total_frames(), 93_600);
        assert_eq!(clip.sample_rate(), 48_000);
        let mut sources: Vec<_> = (0..24)
            .map(|id| BeeFlight {
                id,
                position: Vec3::X * (0.1 + id as f32 * 0.01),
            })
            .collect();
        audio.prepare(&sources, Vec3::ZERO, true);
        assert_eq!(audio.voices.len(), MAX_VOICES);
        assert_eq!(
            manager.engine_voice_count(),
            0,
            "no playback before publication"
        );
        manager.publish_spatial_frame(0.0).unwrap();
        audio.finish_publication(true);
        assert_eq!(audio.diagnostics(), (16, 16));
        assert_eq!(manager.engine_voice_count(), 16);
        let emitter = audio
            .voices
            .iter()
            .find(|v| v.id == 0)
            .unwrap()
            .emitter
            .unwrap();
        sources[0].position += Vec3::Y * 0.01;
        sources.reverse();
        audio.prepare(&sources, Vec3::ZERO, true);
        manager.publish_spatial_frame(0.01).unwrap();
        audio.finish_publication(true);
        assert_eq!(
            audio.diagnostics(),
            (16, 16),
            "movement does not restart loops"
        );
        assert_eq!(
            audio.voices.iter().find(|v| v.id == 0).unwrap().emitter,
            Some(emitter)
        );
        assert_eq!(
            manager.transient_emitter_position(emitter),
            Vec3::new(0.1, 0.01, 0.0)
        );

        let drain = |audio: &mut BeeAudio| {
            for _ in 0..250 {
                audio.clear();
                if audio.voices.is_empty() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            panic!("retired render ownership was not released");
        };
        // The enable gate covers pause and channel mute independently of source facts.
        audio.prepare(&sources, Vec3::ZERO, false);
        assert_eq!(audio.diagnostics().0, 0);
        drain(&mut audio);
        audio.prepare(&sources, Vec3::ZERO, true);
        manager.publish_spatial_frame(0.02).unwrap();
        audio.finish_publication(true);
        assert_eq!(audio.diagnostics(), (16, 32));
        // Missing source facts mean landing, returning home, nest deletion or world clear.
        audio.prepare(&[], Vec3::ZERO, true);
        assert_eq!(audio.diagnostics().0, 0);
        drain(&mut audio);
        audio.prepare(&sources, Vec3::ZERO, true);
        assert!(manager.publish_spatial_frame(f64::NAN).is_err());
        audio.finish_publication(false);
        assert_eq!(audio.diagnostics(), (0, 32));
        drain(&mut audio);
        manager.stop().unwrap();
        audio.prepare(&sources, Vec3::ZERO, true);
        assert!(
            audio.voices.is_empty(),
            "closed engine must not leak reservations"
        );
    }

    #[test]
    fn residents_use_stable_distinct_loop_phases() {
        let phases: Vec<_> = (0..8).map(|slot| phase((1 << 8) | slot)).collect();
        for (i, p) in phases.iter().enumerate() {
            assert!((0.0..1.0).contains(p));
            assert_eq!(*p, phase((1 << 8) | i as u64));
            assert!(!phases[..i].contains(p));
        }
        assert_ne!(phase(1 << 8), phase(2 << 8));
    }
}
