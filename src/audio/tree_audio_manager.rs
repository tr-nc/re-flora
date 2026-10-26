use crate::audio::{
    CanopyAcousticDescriptor, CanopyAcousticObservation, CanopyAudioLifecycle,
    CanopyAudioTelemetrySnapshot, CanopyAudioTreeTelemetry, CanopyDistributedEmitterAdapter,
    SpatialSoundManager,
};
use crate::wind_field::WindFieldFrame;
use crate::wind_response::WindResponseCurve;
use anyhow::Result;
use uuid::Uuid;

const CANOPY_LAYOUT_CROSSFADE_SECONDS: f32 = 0.35;

/// Coordinates immutable weighted canopy descriptors, generation lifetime, and rustle content.
///
/// Geometry/layout selection lives in `CanopyAcousticDescriptor`, generation mixing lives in
/// `CanopyAudioLifecycle`, and PetalSonic realization lives behind the emitter adapter.
pub struct TreeAudioManager {
    wind_volume_db: f32,
    wind_response_curve: WindResponseCurve,
    lifecycle: CanopyAudioLifecycle,
    emitter_adapter: CanopyDistributedEmitterAdapter,
    kick_rustle: super::tree_kick_rustle::KickRustlePlayer,
}

pub(crate) struct TreeAudioPublicationCheckpoint {
    lifecycle: CanopyAudioLifecycle,
}

impl TreeAudioManager {
    pub(crate) fn publication_checkpoint(&self) -> TreeAudioPublicationCheckpoint {
        TreeAudioPublicationCheckpoint {
            lifecycle: self.lifecycle.clone(),
        }
    }

    #[cfg(test)]
    fn fail_spawn_for_test(&mut self, key: crate::audio::CanopyAudioGenerationKey) {
        self.emitter_adapter.fail_spawn_for_test(key);
    }

    #[cfg(test)]
    fn active_generation_keys_for_test(&self) -> Vec<crate::audio::CanopyAudioGenerationKey> {
        self.emitter_adapter.active_generation_keys_for_test()
    }

    pub(crate) fn restore_publication_checkpoint(
        &mut self,
        checkpoint: TreeAudioPublicationCheckpoint,
        time_seconds: f32,
    ) -> Result<()> {
        self.lifecycle = checkpoint.lifecycle;
        let snapshot = self.lifecycle.snapshot(time_seconds)?;
        self.emitter_adapter.synchronize(
            &snapshot,
            self.wind_volume_db,
            self.wind_response_curve,
            time_seconds,
        )?;
        Ok(())
    }

    pub fn new(
        spatial_sound_manager: SpatialSoundManager,
        wind_response_curve: WindResponseCurve,
        wind_volume_db: f32,
    ) -> Result<Self> {
        Ok(Self {
            wind_volume_db,
            wind_response_curve,
            lifecycle: CanopyAudioLifecycle::new(CANOPY_LAYOUT_CROSSFADE_SECONDS),
            kick_rustle: super::tree_kick_rustle::KickRustlePlayer::new(
                spatial_sound_manager.clone(),
            ),
            emitter_adapter: CanopyDistributedEmitterAdapter::new(spatial_sound_manager)?,
        })
    }

    pub fn upsert_tree(
        &mut self,
        tree_id: u32,
        descriptor: CanopyAcousticDescriptor,
        time_seconds: f32,
    ) -> Result<Vec<Uuid>> {
        self.lifecycle.replace(tree_id, descriptor, time_seconds)?;
        let snapshot = self.lifecycle.snapshot(time_seconds)?;
        self.emitter_adapter.synchronize(
            &snapshot,
            self.wind_volume_db,
            self.wind_response_curve,
            time_seconds,
        )
    }

    /// Begin a bounded fade-out. Physical PetalSonic emitters are retired by `update` after the
    /// lifecycle snapshot no longer contains their generation/sample key.
    pub fn remove_tree(&mut self, tree_id: u32, time_seconds: f32) -> Result<()> {
        self.lifecycle.remove(tree_id, time_seconds)?;
        let snapshot = self.lifecycle.snapshot(time_seconds)?;
        self.emitter_adapter.synchronize(
            &snapshot,
            self.wind_volume_db,
            self.wind_response_curve,
            time_seconds,
        )?;
        Ok(())
    }

    /// Immediately clear every physical source. This is a shutdown/testing escape hatch; normal
    /// tree removal must use the bounded lifecycle above.
    #[allow(dead_code)]
    pub fn remove_all(&mut self) {
        self.emitter_adapter.remove_all();
        self.kick_rustle.clear();
        self.lifecycle = CanopyAudioLifecycle::new(CANOPY_LAYOUT_CROSSFADE_SECONDS);
    }

    pub fn set_canopy_telemetry_enabled(&mut self, enabled: bool) {
        self.emitter_adapter.set_telemetry_enabled(enabled);
    }

    pub(crate) fn observe_canopy_acoustic_telemetry(
        &mut self,
        observations: Vec<CanopyAcousticObservation>,
    ) {
        self.emitter_adapter
            .collect_acoustic_telemetry(observations);
    }

    pub fn canopy_telemetry_snapshot(&self) -> Option<CanopyAudioTelemetrySnapshot> {
        let samples = self.emitter_adapter.telemetry_samples()?;
        let telemetry = self.emitter_adapter.telemetry_diagnostics();
        let petal_telemetry = self.emitter_adapter.petal_acoustic_telemetry_diagnostics();
        let petal_runtime = self.emitter_adapter.petal_runtime_diagnostics();
        let trees = self
            .lifecycle
            .diagnostics_snapshot()
            .into_iter()
            .map(|(tree_id, lifecycle)| CanopyAudioTreeTelemetry { tree_id, lifecycle })
            .collect();
        Some(CanopyAudioTelemetrySnapshot {
            trees,
            samples,
            telemetry,
            petal_superseded_solve_count: self.emitter_adapter.petal_superseded_solve_count(),
            petal_telemetry_queue_depth: petal_telemetry.queue_depth,
            petal_telemetry_queue_high_water: petal_telemetry.queue_high_water,
            petal_telemetry_dropped_events: petal_telemetry.dropped_events,
            petal_active_emitters: petal_runtime.active_emitters,
            petal_active_voices: petal_runtime.active_voices,
            petal_direct_ray_count: petal_runtime.acoustic_direct_ray_count,
            petal_sample_cache_hit_count: petal_runtime.acoustic_sample_cache_hit_count,
            petal_processed_extent_count: petal_runtime.acoustic_processed_extent_count,
            petal_lobe_count: petal_runtime.acoustic_lobe_count,
            petal_retained_response_count: petal_runtime.acoustic_retained_response_count,
            petal_deferred_response_count: petal_runtime.acoustic_deferred_response_count,
            petal_render_rejected_response_count: petal_runtime
                .acoustic_render_rejected_response_count,
        })
    }

    pub fn set_wind_volume_db(&mut self, wind_volume_db: f32) -> Result<()> {
        if (wind_volume_db - self.wind_volume_db).abs() <= f32::EPSILON {
            return Ok(());
        }

        self.wind_volume_db = wind_volume_db;
        self.emitter_adapter.set_base_volume_db(self.wind_volume_db)
    }

    pub fn set_wind_response_curve(&mut self, wind_response_curve: WindResponseCurve) {
        if wind_response_curve == self.wind_response_curve {
            return;
        }

        self.wind_response_curve = wind_response_curve;
        self.emitter_adapter
            .set_wind_response_curve(self.wind_response_curve);
    }

    pub(crate) fn recorded_mix_diagnostics(&self) -> (usize, f32, f32) {
        self.emitter_adapter.recorded_mix_diagnostics()
    }

    pub(crate) fn kick_rustle_voice_count(&self) -> usize {
        self.kick_rustle.voice_count()
    }
    pub(crate) fn clear_kick_rustles(&mut self) {
        self.kick_rustle.clear();
    }

    pub(crate) fn prepare_kick_rustle(&mut self, tuning: super::KickRustleTuning) -> Result<()> {
        self.kick_rustle.prepare(tuning)
    }

    pub(crate) fn play_kick_rustle(
        &mut self,
        tree: u32,
        time: f32,
        volume_db: f32,
    ) -> Result<bool> {
        let snapshot = self.lifecycle.snapshot(time)?;
        let Some(generation) = snapshot
            .generations()
            .iter()
            .filter(|g| g.key().tree_id() == tree)
            .max_by_key(|g| g.key().generation())
        else {
            return Ok(false);
        };
        let descriptor = generation.descriptor();
        if descriptor.samples().is_empty() {
            return Ok(false);
        }
        self.kick_rustle.play(descriptor, volume_db)?;
        Ok(true)
    }

    pub fn update(
        &mut self,
        time_seconds: f32,
        wind: &WindFieldFrame,
        wind_audio_attack_decay: f32,
        wind_audio_release_decay: f32,
    ) -> Result<()> {
        self.kick_rustle.retire(std::time::Instant::now());
        let snapshot = self.lifecycle.snapshot(time_seconds)?;
        self.emitter_adapter.synchronize(
            &snapshot,
            self.wind_volume_db,
            self.wind_response_curve,
            time_seconds,
        )?;
        self.emitter_adapter.update(
            wind,
            time_seconds,
            wind_audio_attack_decay,
            wind_audio_release_decay,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audio::CanopyAudioGenerationKey, tree_gen::LeafPlacement};
    use glam::Vec3;
    use petalsonic::{
        AcousticHit, AcousticRay, AcousticRayQuerySnapshot, AcousticSceneSnapshot,
        EnvironmentalAcousticsBudget,
    };
    use std::sync::Arc;

    struct NoAcousticHits;

    impl AcousticRayQuerySnapshot for NoAcousticHits {
        fn trace_any_hit_batch(
            &self,
            _rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            hits.fill(false);
        }

        fn trace_closest_hit_batch(
            &self,
            _rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            hits.fill(None);
        }
    }

    fn manager() -> TreeAudioManager {
        let spatial = SpatialSoundManager::new(
            64,
            AcousticSceneSnapshot::new(1, Arc::new(NoAcousticHits)),
            Some("re-flora-tree-audio-publication-device-that-does-not-exist".to_owned()),
            EnvironmentalAcousticsBudget::default(),
        )
        .unwrap();
        TreeAudioManager::new(
            spatial,
            WindResponseCurve {
                min_strength: 0.0,
                max_strength: 1.0,
                power: 1.0,
            },
            -15.0,
        )
        .unwrap()
    }

    fn descriptor(generation: u64, x: f32) -> CanopyAcousticDescriptor {
        CanopyAcousticDescriptor::build(
            generation,
            Vec3::new(x, 0.0, 0.0),
            generation,
            &[LeafPlacement {
                position: Vec3::new(4.0, 4.0, 4.0),
                anchor: Vec3::ZERO,
            }],
            &[],
        )
    }

    #[test]
    #[ignore = "explicit audio-runtime integration check; ordinary tests must not open audio devices"]
    fn kick_rustle_has_exact_tree_placement_no_wind_dependency_and_bounded_voice_lifetime() {
        let mut audio = manager();
        let tuning = super::super::KickRustleTuning {
            duration: 0.25,
            brightness: 0.42,
            crackle: 0.18,
            body: 0.28,
            shake_frequency: 2.55,
        };
        audio.prepare_kick_rustle(tuning).unwrap();
        audio.upsert_tree(1, descriptor(1, 1.), 0.).unwrap();
        assert!(!audio.play_kick_rustle(99, 0., -6.).unwrap());
        assert_eq!(audio.kick_rustle_voice_count(), 0);
        // No wind update/threshold is required to play this finite event.
        for _ in 0..12 {
            assert!(audio.play_kick_rustle(1, 0., -6.).unwrap());
        }
        let (_, extent, profile) = audio.kick_rustle.latest_spatial_domain().unwrap();
        assert!(
            extent.weighted().is_some(),
            "the real finite canopy cue must not collapse to one listener-selected point"
        );
        assert!(
            matches!(profile, petalsonic::OcclusionProfile::PointExact),
            "finite cue must keep exact transmission, not ambient attenuation floors"
        );
        assert_eq!(audio.kick_rustle_voice_count(), 8);
        audio
            .kick_rustle
            .retire(std::time::Instant::now() + std::time::Duration::from_secs(1));
        assert_eq!(audio.kick_rustle_voice_count(), 0);
        audio.play_kick_rustle(1, 0., -6.).unwrap();
        audio.clear_kick_rustles();
        assert_eq!(audio.kick_rustle_voice_count(), 0);
    }

    #[test]
    fn publication_failure_and_checkpoint_restore_preserve_the_previous_generation() {
        let mut audio = manager();
        audio.upsert_tree(1, descriptor(1, 1.0), 0.0).unwrap();
        let checkpoint = audio.publication_checkpoint();
        let previous_key = CanopyAudioGenerationKey::new(1, 1);
        let failing_key = CanopyAudioGenerationKey::new(2, 2);
        audio.fail_spawn_for_test(failing_key);

        let error = audio
            .upsert_tree(2, descriptor(2, 2.0), 0.0)
            .expect_err("publication must fail when its physical Voice cannot spawn");
        assert!(format!("{error:#}").contains("injected canopy Voice spawn failure"));
        assert_eq!(audio.active_generation_keys_for_test(), vec![previous_key]);

        audio
            .restore_publication_checkpoint(checkpoint, 0.0)
            .expect("the previous generation should remain physically realizable");
        assert_eq!(audio.active_generation_keys_for_test(), vec![previous_key]);
    }

    #[test]
    fn checkpoint_restore_reports_a_physical_generation_failure() {
        let mut published = manager();
        published.upsert_tree(1, descriptor(1, 1.0), 0.0).unwrap();
        let checkpoint = published.publication_checkpoint();
        let mut recovering = manager();
        recovering.fail_spawn_for_test(CanopyAudioGenerationKey::new(1, 1));

        let error = recovering
            .restore_publication_checkpoint(checkpoint, 0.0)
            .expect_err("restore must not report success without the checkpoint generation");

        assert!(format!("{error:#}").contains("injected canopy Voice spawn failure"));
        assert!(recovering.active_generation_keys_for_test().is_empty());
    }
}
