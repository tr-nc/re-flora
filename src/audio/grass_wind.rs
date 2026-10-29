//! World-anchored grass patches: source power from visible biomass and local wind,
//! never listener-following ambience or a voice per blade.
use super::{
    mixer::AudioCategory, spatial_sound_manager::TransientSpatialEmitter, SpatialSoundManager,
};
use crate::{
    flora::grass_acoustics::{GrassPatch, PatchId},
    wind_field::WindFieldFrame,
};
use glam::Vec3;
use petalsonic::{
    ExtentSample, ExtentSampleId, PlayOptions, PlaybackTag, ResidentClip, SampleEnvelopeControl,
    SourceExtent,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const STEADY_VOICES: usize = 12;
const TOTAL_VOICES: usize = 16; // includes prepared, fading and audio-thread retirement ownership
const REFERENCE_MASS: f32 = 256.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct GrassWindSettings {
    pub trim_db: f32,
    pub full_wind: f32,
    pub radius: f32,
    pub attack: f32,
    pub release: f32,
}
impl GrassWindSettings {
    pub fn sanitized(self) -> Self {
        fn safe(v: f32, default: f32, min: f32, max: f32) -> f32 {
            if v.is_finite() {
                v.clamp(min, max)
            } else {
                default
            }
        }
        Self {
            trim_db: safe(self.trim_db, 12.0, -24.0, 36.0),
            full_wind: safe(self.full_wind, 1.0, 0.1, 5.0),
            radius: safe(self.radius, 1.5, 0.25, 2.0),
            attack: safe(self.attack, 0.1, 0.02, 1.0),
            release: safe(self.release, 0.25, 0.05, 2.0),
        }
    }
}

pub(crate) struct GrassWindFrame<'a> {
    pub patches: &'a [Arc<GrassPatch>],
    pub wind: &'a WindFieldFrame,
    pub settings: GrassWindSettings,
    pub dt: f32,
    pub enabled: bool,
}

fn sample_power(mass: f32, wind: Vec3, full_wind: f32) -> f32 {
    if !wind.is_finite() || !mass.is_finite() {
        return 0.0;
    }
    let power = mass.max(0.0) * (wind.length() / full_wind).clamp(0.0, 1.0).powi(2);
    if power > 1e-6 {
        power
    } else {
        0.0
    }
}
fn power(patch: &GrassPatch, wind: &WindFieldFrame, settings: GrassWindSettings) -> f32 {
    let total: f32 = patch
        .samples
        .iter()
        .map(|s| sample_power(s.mass, wind.sample_world(s.position), settings.full_wind))
        .sum();
    // One inaudibility threshold shared by selection, fade and extent construction.
    if total > 1e-4 {
        total
    } else {
        0.0
    }
}
fn extent(
    patch: &GrassPatch,
    wind: &WindFieldFrame,
    settings: GrassWindSettings,
) -> anyhow::Result<SourceExtent> {
    let mut samples = Vec::new();
    for s in &patch.samples {
        let power = sample_power(s.mass, wind.sample_world(s.position), settings.full_wind);
        if power > 1e-6 {
            let p = s.position - patch.center();
            samples.push(ExtentSample::new(
                ExtentSampleId(s.id),
                petalsonic::Vec3::new(p.x, p.y, p.z),
                power,
            )?);
        }
    }
    Ok(SourceExtent::live_weighted_samples(samples)?)
}
fn phase(id: PatchId) -> f32 {
    let hash = id.0.into_iter().fold(0x9e37_79b9_7f4a_7c15u64, |h, v| {
        (h ^ u64::from(v)).wrapping_mul(0xbf58_476d_1ce4_e5b9)
    });
    (hash >> 40) as f32 / 16_777_216.0
}
fn select(
    patches: &[Arc<GrassPatch>],
    wind: &WindFieldFrame,
    listener: Vec3,
    settings: GrassWindSettings,
    retained: &[PatchId],
) -> Vec<PatchId> {
    let mut candidates: Vec<_> = patches
        .iter()
        .filter_map(|p| {
            let distance = p
                .samples
                .iter()
                .map(|s| s.position.distance_squared(listener))
                .fold(f32::INFINITY, f32::min);
            let power = power(p, wind, settings);
            (power > 1e-4 && distance < settings.radius * settings.radius).then_some((
                // Ranking only; actual propagation/occlusion is applied once by PetalSonic.
                power / (1.0 + distance * 225.0) * if retained.contains(&p.id) { 1.3 } else { 1.0 },
                p.id,
            ))
        })
        .collect();
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    candidates.truncate(STEADY_VOICES);
    candidates.into_iter().map(|(_, id)| id).collect()
}

struct Voice {
    id: PatchId,
    emitter: Option<TransientSpatialEmitter>,
    envelope: SampleEnvelopeControl,
    started: bool,
    retiring: bool,
    amplitude: f32,
    release_elapsed: f32,
}

pub(super) struct GrassWindAudio {
    manager: SpatialSoundManager,
    clip: Option<ResidentClip>,
    voices: Vec<Voice>,
    selected: Vec<PatchId>,
    selection_age: f32,
    retry_after: Instant,
    epoch: Instant,
    starts: u64,
}
impl GrassWindAudio {
    pub fn new(manager: SpatialSoundManager) -> Self {
        let clip = manager
            .cached_clip(super::runtime_assets::GRASS_WIND_LOOP_PATH)
            .map_err(|e| log::warn!("[GRASS_WIND_AUDIO] unavailable: {e:#}"))
            .ok();
        if let Some(c) = &clip {
            log::info!(
                "[GRASS_WIND_AUDIO] clip={} frames={} sample_rate={} steady={STEADY_VOICES} reservations={TOTAL_VOICES}",
                super::runtime_assets::GRASS_WIND_LOOP_PATH,
                c.total_frames(),
                c.sample_rate()
            );
        }
        Self {
            manager,
            clip,
            voices: Vec::with_capacity(TOTAL_VOICES),
            selected: Vec::new(),
            selection_age: 1.0,
            retry_after: Instant::now(),
            epoch: Instant::now(),
            starts: 0,
        }
    }
    pub fn prepare(
        &mut self,
        patches: &[Arc<GrassPatch>],
        wind: &WindFieldFrame,
        listener: Vec3,
        settings: GrassWindSettings,
        dt: f32,
        enabled: bool,
    ) {
        let settings = settings.sanitized();
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        if !enabled || self.clip.is_none() || !listener.is_finite() {
            self.clear();
            return;
        }
        self.selection_age += dt;
        if self.selection_age >= 0.1 {
            let retained: Vec<_> = self
                .voices
                .iter()
                .filter(|v| !v.retiring)
                .map(|v| v.id)
                .collect();
            self.selected = select(patches, wind, listener, settings, &retained);
            self.selection_age = 0.0;
        }
        for voice in &mut self.voices {
            let patch = patches.iter().find(|p| p.id == voice.id);
            let target = if !voice.retiring && self.selected.contains(&voice.id) {
                patch
                    .filter(|p| {
                        p.samples.iter().any(|s| {
                            s.position.distance_squared(listener) < settings.radius.powi(2)
                        })
                    })
                    .map_or(0.0, |p| (power(p, wind, settings) / REFERENCE_MASS).sqrt())
            } else {
                0.0
            };
            if target == 0.0 {
                voice.retiring = true;
            }
            let time = if target > voice.amplitude {
                settings.attack
            } else {
                settings.release
            };
            // Controls are time-to-99%, not frame-count dependent interpolation.
            voice.amplitude += (target - voice.amplitude) * (1.0 - (-4.60517 * dt / time).exp());
            if voice.retiring {
                voice.release_elapsed += dt;
            }
            if voice.retiring
                && (voice.amplitude < 1e-4 || voice.release_elapsed >= settings.release * 2.0)
            {
                voice.envelope.anchor(0.0, 0.0, 0.0);
                Self::destroy(&self.manager, voice);
                continue;
            }
            voice.envelope.anchor_gain(
                f64::from(target * 10.0f32.powf(settings.trim_db / 20.0)),
                f64::from(settings.attack),
                f64::from(settings.release),
            );
            if let Some(emitter) = voice.emitter {
                if target > 0.0 {
                    if let Some(patch) = patch {
                        match extent(patch, wind, settings) {
                            Ok(extent) => self.manager.set_transient_extent(emitter, extent),
                            Err(error) => {
                                log::warn!("[GRASS_WIND_AUDIO] extent failed: {error:#}");
                                voice.retiring = true;
                            }
                        }
                    }
                }
                let db = settings.trim_db + 20.0 * voice.amplitude.max(1e-6).log10();
                self.manager.set_transient_priority(emitter, db);
            }
        }
        self.reap();
        for &id in &self.selected {
            if self.voices.len() >= TOTAL_VOICES
                || self.voices.iter().filter(|v| !v.retiring).count() >= STEADY_VOICES
                || self.voices.iter().any(|v| v.id == id)
                || Instant::now() < self.retry_after
            {
                continue;
            }
            let Some(patch) = patches.iter().find(|p| p.id == id) else {
                continue;
            };
            if power(patch, wind, settings) <= 1e-4 {
                continue;
            }
            let envelope = SampleEnvelopeControl::default();
            let result = extent(patch, wind, settings).and_then(|extent| {
                let emitter = self.manager.prepare_spatial_clip_emitter(
                    self.clip
                        .as_ref()
                        .unwrap()
                        .clone()
                        .with_sample_envelope(envelope.clone()),
                    AudioCategory::GrassRustle,
                    0.0,
                    patch.center(),
                )?;
                self.manager.set_transient_extent(emitter, extent);
                Ok(emitter)
            });
            match result {
                Ok(emitter) => {
                    let amplitude = (power(patch, wind, settings) / REFERENCE_MASS).sqrt();
                    let gain = amplitude * 10.0f32.powf(settings.trim_db / 20.0);
                    envelope.anchor_gain(
                        f64::from(gain),
                        f64::from(settings.attack),
                        f64::from(settings.release),
                    );
                    self.manager.set_transient_priority(
                        emitter,
                        settings.trim_db + 20.0 * amplitude.max(1e-6).log10(),
                    );
                    self.voices.push(Voice {
                        id,
                        emitter: Some(emitter),
                        envelope,
                        started: false,
                        retiring: false,
                        amplitude: 0.0,
                        release_elapsed: 0.0,
                    });
                }
                Err(error) => {
                    self.retry_after = Instant::now() + Duration::from_secs(1);
                    log::warn!("[GRASS_WIND_AUDIO] prepare failed: {error:#}");
                }
            }
        }
    }
    fn destroy(manager: &SpatialSoundManager, voice: &mut Voice) {
        if let Some(emitter) = voice.emitter {
            match manager.destroy_transient_spatial_emitter(emitter) {
                Ok(()) => voice.emitter = None,
                Err(error) => log::warn!("[GRASS_WIND_AUDIO] destroy retry: {error:#}"),
            }
        }
    }
    fn reap(&mut self) {
        self.voices
            .retain(|v| !v.retiring || v.emitter.is_some() || v.envelope.has_render_owners());
    }
    pub fn finish_publication(&mut self, published: bool) {
        if !published {
            self.clear();
            return;
        }
        for voice in &mut self.voices {
            if voice.started || voice.retiring {
                continue;
            }
            let emitter = voice.emitter.unwrap();
            let result = self
                .manager
                .play_controlled_transient(
                    emitter,
                    PlayOptions::looping(),
                    PlaybackTag(0x4752_4153_5300_0000),
                )
                .and_then(|_| {
                    let clip = self.clip.as_ref().unwrap();
                    let seconds = clip.total_frames() as f64 / f64::from(clip.sample_rate());
                    let phase = (f64::from(phase(voice.id))
                        + self.epoch.elapsed().as_secs_f64() / seconds)
                        .fract() as f32;
                    self.manager.seek_transient_emitter(emitter, phase)
                });
            match result {
                Ok(()) => {
                    voice.started = true;
                    self.starts += 1;
                    log::debug!("[GRASS_WIND_AUDIO] started patch={:?}", voice.id);
                }
                Err(error) => {
                    voice.envelope.anchor(0.0, 0.0, 0.0);
                    voice.retiring = true;
                    Self::destroy(&self.manager, voice);
                    self.retry_after = Instant::now() + Duration::from_secs(1);
                    log::warn!("[GRASS_WIND_AUDIO] start failed: {error:#}");
                }
            }
        }
    }
    pub fn clear(&mut self) {
        self.selected.clear();
        self.selection_age = 1.0;
        self.epoch = Instant::now();
        for voice in &mut self.voices {
            voice.retiring = true;
            voice.amplitude = 0.0;
            voice.envelope.anchor(0.0, 0.0, 0.0);
            Self::destroy(&self.manager, voice);
        }
        self.reap();
    }
    pub fn diagnostics(&self) -> (usize, usize, u64) {
        (
            self.voices.iter().filter(|v| !v.retiring).count(),
            self.voices.len(),
            self.starts,
        )
    }
}
impl Drop for GrassWindAudio {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flora::grass_acoustics::GrassSample;
    use glam::Vec2;
    fn settings() -> GrassWindSettings {
        GrassWindSettings {
            trim_db: 12.0,
            full_wind: 1.0,
            radius: 1.5,
            attack: 0.1,
            release: 0.25,
        }
    }
    fn patch(id: u32, mass: f32) -> Arc<GrassPatch> {
        Arc::new(GrassPatch {
            id: PatchId([id, 0, 0]),
            samples: vec![GrassSample {
                id: 0,
                position: Vec3::new(id as f32 / 4.0, 0.1, 0.0),
                mass,
            }],
        })
    }
    #[test]
    #[ignore = "native engine lifecycle; pinned nonexistent output, run explicitly"]
    fn real_grass_loops_publish_retain_fade_budget_pause_clear_and_failure() {
        use petalsonic::{
            AcousticHit, AcousticRay, AcousticRayQuerySnapshot, AcousticSceneSnapshot,
            EnvironmentalAcousticsBudget,
        };
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
            Some("grass-test-no-such-output-f890".into()),
            EnvironmentalAcousticsBudget::default(),
        )
        .unwrap();
        let mut audio = GrassWindAudio::new(manager.clone());
        assert_eq!(audio.clip.as_ref().unwrap().total_frames(), 93_600);
        let patches = |offset: u32| -> Vec<_> {
            (0..24u32)
                .map(|i| {
                    let id = PatchId([i % 6, 0, i / 6 + offset]);
                    let center = (glam::UVec3::from_array(id.0).as_vec3() + Vec3::splat(0.5)) / 4.0;
                    Arc::new(GrassPatch {
                        id,
                        samples: vec![
                            GrassSample {
                                id: 0,
                                position: center - Vec3::X * 0.05,
                                mass: 100.0,
                            },
                            GrassSample {
                                id: 3,
                                position: center + Vec3::X * 0.05,
                                mass: 100.0,
                            },
                        ],
                    })
                })
                .collect()
        };
        let sources = patches(0);
        let wind = WindFieldFrame::uniform(Vec2::X);
        let listener = Vec3::new(0.5, 0.3, 0.5);
        audio.prepare(&sources, &wind, listener, settings(), 0.02, true);
        assert_eq!(audio.diagnostics(), (12, 12, 0));
        assert_eq!(
            manager.engine_voice_count(),
            0,
            "must publish before playback"
        );
        manager.publish_spatial_frame(0.0).unwrap();
        audio.finish_publication(true);
        assert_eq!(audio.diagnostics(), (12, 12, 12));
        let id = audio.voices[0].id;
        let emitter = audio.voices[0].emitter.unwrap();
        let position = manager.transient_emitter_position(emitter);
        let SourceExtent::WeightedSamples(extent) = manager.transient_emitter_extent(emitter)
        else {
            panic!()
        };
        assert_eq!(extent.samples().len(), 2);
        let mut reversed = sources.clone();
        reversed.reverse();
        audio.prepare(
            &reversed,
            &wind,
            listener + Vec3::X * 0.001,
            settings(),
            0.1,
            true,
        );
        manager.publish_spatial_frame(0.1).unwrap();
        audio.finish_publication(true);
        assert_eq!(
            audio.starts, 12,
            "listener movement/reordering must not restart stable patches"
        );
        assert_eq!(
            audio.voices.iter().find(|v| v.id == id).unwrap().emitter,
            Some(emitter)
        );
        assert_eq!(
            manager.transient_emitter_position(emitter),
            position,
            "patch must not follow listener"
        );
        let old_gain = audio.voices.iter().find(|v| v.id == id).unwrap().amplitude;
        let sparse: Vec<_> = sources
            .iter()
            .map(|p| {
                let mut p = (**p).clone();
                for s in &mut p.samples {
                    s.mass *= 0.25;
                }
                Arc::new(p)
            })
            .collect();
        for _ in 0..10 {
            audio.prepare(&sparse, &wind, listener, settings(), 0.05, true);
        }
        assert!(
            audio.voices.iter().find(|v| v.id == id).unwrap().amplitude < old_gain,
            "thinning must reduce absolute power"
        );
        let replacement = patches(4);
        audio.prepare(&replacement, &wind, listener, settings(), 0.1, true);
        assert!(audio.voices.len() <= 16);
        assert!(audio.voices.iter().filter(|v| !v.retiring).count() <= 12);
        for _ in 0..100 {
            audio.prepare(&replacement, &wind, listener, settings(), 0.01, true);
            assert!(
                audio.voices.len() <= 16,
                "render-retirement ownership belongs to the cap"
            );
        }
        let drain = |audio: &mut GrassWindAudio| {
            for _ in 0..250 {
                audio.clear();
                if audio.voices.is_empty() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            panic!("grass render owners did not retire");
        };
        audio.prepare(&replacement, &wind, listener, settings(), 0.01, false);
        assert_eq!(audio.diagnostics().0, 0);
        drain(&mut audio);
        audio.prepare(
            &sources,
            &WindFieldFrame::default(),
            listener,
            settings(),
            0.1,
            true,
        );
        assert!(audio.voices.is_empty(), "calm must allocate no voices");
        audio.prepare(&sources, &wind, listener, settings(), 0.1, true);
        assert!(manager.publish_spatial_frame(f64::NAN).is_err());
        audio.finish_publication(false);
        drain(&mut audio);
        manager.stop().unwrap();
        audio.prepare(&sources, &wind, listener, settings(), 0.1, true);
        assert!(
            audio.voices.is_empty(),
            "closed engine must not leak a reservation"
        );
    }

    #[test]
    fn grass_power_is_additive_not_normalized_after_mowing_or_representative_changes() {
        let wind = WindFieldFrame::uniform(Vec2::X);
        let a = patch(0, 100.0);
        let mut b = (*a).clone();
        b.samples[0].mass = 50.0;
        assert_eq!(
            power(&a, &wind, settings()),
            2.0 * power(&b, &wind, settings())
        );
        let mut s = b.samples[0].clone();
        s.id = 1;
        b.samples.push(s);
        assert_eq!(power(&a, &wind, settings()), power(&b, &wind, settings()));
        let SourceExtent::WeightedSamples(samples) = extent(&b, &wind, settings()).unwrap() else {
            panic!()
        };
        assert_eq!(
            samples
                .samples()
                .iter()
                .map(|s| s.power_weight())
                .sum::<f32>(),
            1.0
        );
        assert_eq!(power(&a, &WindFieldFrame::default(), settings()), 0.0);
        assert_eq!(
            power(&a, &WindFieldFrame::uniform(Vec2::X * 0.5), settings()),
            25.0
        );
    }
    #[test]
    fn local_wind_weights_follow_occupied_samples_and_tiny_wind_is_silent() {
        let mut wind = WindFieldFrame::uniform(Vec2::X);
        let side = wind.domain[2] as usize;
        for i in 0..wind.cells.len() * 2 {
            if i % side < side / 2 {
                wind.cells[i / 2][(i % 2) * 2] = 0.0;
            }
        }
        let p = GrassPatch {
            id: PatchId([0, 0, 0]),
            samples: vec![
                GrassSample {
                    id: 0,
                    position: Vec3::new(0.25, 0.2, 0.5),
                    mass: 100.0,
                },
                GrassSample {
                    id: 1,
                    position: Vec3::new(1.75, 0.2, 0.5),
                    mass: 100.0,
                },
            ],
        };
        assert_eq!(power(&p, &wind, settings()), 100.0);
        let e = extent(&p, &wind, settings()).unwrap();
        let samples = e.weighted().unwrap().samples();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].id(), ExtentSampleId(1));
        assert_eq!(
            power(&p, &WindFieldFrame::uniform(Vec2::X * 1e-7), settings()),
            0.0
        );
    }

    #[test]
    fn bounded_selection_calm_empty_distance_and_stable_phase() {
        let patches: Vec<_> = (0..40).map(|i| patch(i, 100.0)).collect();
        let wind = WindFieldFrame::uniform(Vec2::X);
        assert!(select(
            &patches,
            &WindFieldFrame::default(),
            Vec3::ZERO,
            settings(),
            &[]
        )
        .is_empty());
        assert!(select(&[], &wind, Vec3::ZERO, settings(), &[]).is_empty());
        let ids = select(&patches, &wind, Vec3::ZERO, settings(), &[]);
        assert!(!ids.is_empty());
        assert!(ids.len() <= STEADY_VOICES);
        assert_eq!(ids[0], patches[0].id);
        assert_ne!(phase(PatchId([0, 0, 0])), phase(PatchId([1, 0, 0])));
        assert!(phase(PatchId([1, 0, 0])) < 1.0);
        assert!(select(&patches, &wind, Vec3::splat(100.0), settings(), &[]).is_empty());
    }
}
