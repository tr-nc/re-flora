use super::recorded_tree_leaves::{RecordedLeafBank, MAKEUP_GAIN_DB};
use crate::audio::{
    ActiveCanopyAcousticGeneration, CanopyAcousticDescriptor, CanopyAcousticObservation,
    CanopyAcousticSampleId, CanopyAcousticSolveStatus, CanopyAudioGenerationKey,
    CanopyAudioLifecycleSnapshot, CanopyAudioSampleTelemetry, CanopyAudioTelemetry,
    CanopyAudioVoice, CanopyExtentAcousticObservation, CanopyOcclusionClassification,
    CanopyRouteAcousticObservation, CanopySampleAcousticObservation, SpatialSoundManager,
};
use crate::wind_field::WindFieldFrame;
use crate::wind_response::WindResponseCurve;
use anyhow::{Context, Result};
use petalsonic::{
    AcousticOcclusionState, AcousticSolveStatus, AcousticTelemetryDiagnostics,
    DistributedOcclusionProfile, ExtentSample, ExtentSampleId, OcclusionProfile,
    RuntimeDiagnostics, SourceExtent,
};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

const TREE_SILENT_VOLUME_DB: f32 = -80.0;
const VOXELS_PER_WORLD_UNIT: f32 = 256.0;

/// PetalSonic realization of immutable weighted canopy generations.
///
/// Geometry and sample selection remain in `CanopyAcousticDescriptor`; this adapter only maps one
/// active `(tree, generation)` layer to one distributed Emitter and one looping Voice.
pub struct CanopyDistributedEmitterAdapter {
    spatial_sound_manager: SpatialSoundManager,
    telemetry: CanopyAudioTelemetry,
    voices: HashMap<CanopyAudioGenerationKey, CanopyAudioVoice>,
    recorded_bank: RecordedLeafBank,
    #[cfg(test)]
    fail_spawn_key: Option<CanopyAudioGenerationKey>,
}

impl CanopyDistributedEmitterAdapter {
    pub fn new(spatial_sound_manager: SpatialSoundManager) -> Result<Self> {
        let recorded_bank = RecordedLeafBank::load(&spatial_sound_manager)
            .context("load required recorded tree-leaf audio")?;
        log::info!(
            "[TREE_LEAF_AUDIO] recorded_only=true selection=per_canopy_generation one_voice=true"
        );
        Ok(Self {
            spatial_sound_manager,
            telemetry: CanopyAudioTelemetry::default(),
            voices: HashMap::new(),
            recorded_bank,
            #[cfg(test)]
            fail_spawn_key: None,
        })
    }

    pub(crate) fn recorded_mix_diagnostics(&self) -> (usize, f32, f32) {
        let positions = self
            .voices
            .values()
            .map(|voice| voice.recorded_blend.position());
        let (count, min, max) = positions.fold((0, 1.0_f32, 0.0_f32), |(n, min, max), x| {
            (n + 1, min.min(x), max.max(x))
        });
        (count, if count == 0 { 0.0 } else { min }, max)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn synchronize(
        &mut self,
        snapshot: &CanopyAudioLifecycleSnapshot,
        base_volume_db: f32,
        wind_response_curve: WindResponseCurve,
        time_seconds: f32,
    ) -> Result<Vec<Uuid>> {
        let active_keys = snapshot
            .generations()
            .iter()
            .map(ActiveCanopyAcousticGeneration::key)
            .collect::<HashSet<_>>();
        let stale_keys = self
            .voices
            .keys()
            .copied()
            .filter(|key| !active_keys.contains(key))
            .collect::<Vec<_>>();
        let mut created = Vec::new();
        let mut created_keys = Vec::new();
        for active in snapshot.generations() {
            if !self.voices.contains_key(&active.key()) {
                let uuid = match self.spawn_voice(
                    active,
                    base_volume_db,
                    wind_response_curve,
                    time_seconds,
                ) {
                    Ok(uuid) => uuid,
                    Err(error) => {
                        self.rollback_created_voices(&created_keys);
                        return Err(error).with_context(|| {
                            format!(
                                "spawning distributed canopy Voice tree={} generation={}",
                                active.key().tree_id(),
                                active.key().generation(),
                            )
                        });
                    }
                };
                created.push(uuid);
                created_keys.push(active.key());
            }
            if let Some(voice) = self.voices.get_mut(&active.key()) {
                let publication = voice
                    .set_wind_volume_db(
                        Self::content_volume_db(base_volume_db),
                        &self.spatial_sound_manager,
                    )
                    .and_then(|()| {
                        voice.set_lifecycle_power(
                            active.lifecycle_power(),
                            &self.spatial_sound_manager,
                        )
                    });
                if let Err(error) = publication {
                    self.rollback_created_voices(&created_keys);
                    return Err(error).with_context(|| {
                        format!(
                            "publishing canopy Voice power tree={} generation={}",
                            active.key().tree_id(),
                            active.key().generation(),
                        )
                    });
                }
            }
        }
        for key in stale_keys {
            self.remove_voice(key);
        }
        Ok(created)
    }

    fn remove_voice(&mut self, key: CanopyAudioGenerationKey) {
        if let Some(voice) = self.voices.remove(&key) {
            self.spatial_sound_manager.remove_source(voice.uuid);
            for sample in voice.descriptor.samples() {
                self.telemetry.remove_source(voice.sample_key(sample.id()));
            }
        }
    }

    fn rollback_created_voices(&mut self, created_keys: &[CanopyAudioGenerationKey]) {
        for &key in created_keys.iter().rev() {
            self.remove_voice(key);
        }
    }

    #[cfg(test)]
    pub(super) fn fail_spawn_for_test(&mut self, key: CanopyAudioGenerationKey) {
        self.fail_spawn_key = Some(key);
    }

    #[cfg(test)]
    pub(super) fn active_generation_keys_for_test(&self) -> Vec<CanopyAudioGenerationKey> {
        let mut keys = self.voices.keys().copied().collect::<Vec<_>>();
        keys.sort();
        keys
    }

    pub fn update(
        &mut self,
        wind: &WindFieldFrame,
        time_seconds: f32,
        wind_audio_attack_decay: f32,
        wind_audio_release_decay: f32,
    ) -> Result<()> {
        for voice in self.voices.values_mut() {
            voice.update(
                wind,
                time_seconds,
                wind_audio_attack_decay,
                wind_audio_release_decay,
                &self.spatial_sound_manager,
            )?;
        }
        Ok(())
    }

    pub fn set_base_volume_db(&mut self, base_volume_db: f32) -> Result<()> {
        for voice in self.voices.values_mut() {
            voice.set_wind_volume_db(
                Self::content_volume_db(base_volume_db),
                &self.spatial_sound_manager,
            )?;
        }
        Ok(())
    }

    pub fn set_wind_response_curve(&mut self, wind_response_curve: WindResponseCurve) {
        for voice in self.voices.values_mut() {
            voice.set_wind_response_curve(wind_response_curve);
        }
    }

    pub fn remove_all(&mut self) {
        for voice in self.voices.values() {
            self.spatial_sound_manager.remove_source(voice.uuid);
        }
        self.voices.clear();
    }

    pub fn set_telemetry_enabled(&mut self, enabled: bool) {
        self.telemetry.set_enabled(enabled);
    }

    pub fn collect_acoustic_telemetry(&mut self, observations: Vec<CanopyAcousticObservation>) {
        for event in observations {
            match event {
                CanopyAcousticObservation::ExtentResponse {
                    generation,
                    response,
                } => {
                    let Some(voice) = self.voices.get(&generation) else {
                        continue;
                    };
                    let key = voice.key;
                    let descriptor = voice.descriptor.clone();
                    self.telemetry.observe_extent_response(
                        key,
                        &descriptor,
                        CanopyExtentAcousticObservation {
                            voice_id: response.voice_id,
                            spatial_revision: response.spatial_revision,
                            geometry_version: response.geometry_version,
                            response_spatial_revision: response.response_spatial_revision,
                            response_geometry_version: response.response_geometry_version,
                            extent_sample_count: response.extent_sample_count,
                            direct: CanopyRouteAcousticObservation {
                                samples: response
                                    .direct
                                    .samples
                                    .iter()
                                    .map(|sample| CanopySampleAcousticObservation {
                                        sample_id: CanopyAcousticSampleId::from_stable_value(
                                            sample.sample_id.0,
                                        ),
                                        normalized_power_weight: sample.normalized_power_weight,
                                        world_position: glam::Vec3::new(
                                            sample.world_position.x,
                                            sample.world_position.y,
                                            sample.world_position.z,
                                        ),
                                        hit: sample.hit,
                                        transmission: sample.transmission,
                                    })
                                    .collect(),
                                ray_count: response.direct.ray_count,
                                cache_hit_count: response.direct.cache_hit_count,
                                hit_count: response.direct.hit_count,
                                visible_fraction: response.direct.visible_fraction,
                                raw_gain: response.direct.raw_gain,
                                filtered_gain: response.direct.filtered_gain,
                                classification: match response.direct.classified_state {
                                    AcousticOcclusionState::Visible => {
                                        CanopyOcclusionClassification::Visible
                                    }
                                    AcousticOcclusionState::Occluded => {
                                        CanopyOcclusionClassification::Occluded
                                    }
                                },
                                dwell_seconds: response.direct.dwell_seconds,
                            },
                            solve_status: match response.solve_status {
                                AcousticSolveStatus::Solved => CanopyAcousticSolveStatus::Solved,
                                AcousticSolveStatus::Retained => {
                                    CanopyAcousticSolveStatus::Retained
                                }
                                AcousticSolveStatus::Deferred => {
                                    CanopyAcousticSolveStatus::Deferred
                                }
                            },
                            cache_age_seconds: response.cache_age_seconds,
                            budget_member: response.budget_member,
                            lobe_count: response.lobes.len(),
                        },
                    );
                }
                CanopyAcousticObservation::SolveDiscarded {
                    spatial_revision,
                    geometry_version,
                } => {
                    self.telemetry
                        .record_solve_discard(spatial_revision, geometry_version);
                }
            }
        }
    }

    pub fn telemetry_samples(&self) -> Option<Vec<CanopyAudioSampleTelemetry>> {
        if !self.telemetry.is_enabled() {
            return None;
        }
        let mut samples = self
            .voices
            .values()
            .flat_map(|voice| {
                voice.descriptor.samples().iter().map(|sample| {
                    let key = voice.sample_key(sample.id());
                    CanopyAudioSampleTelemetry {
                        key,
                        emitter_uuid: voice.uuid,
                        position_tree_voxels: sample.position_tree_voxels(),
                        position_world: voice.descriptor.sample_world_position(sample),
                        clearance_voxels: sample.clearance_voxels(),
                        weight: sample.weight(),
                        lifecycle_power: voice.lifecycle_power(),
                        content_seed: sample.content_seed(),
                        phase: sample.phase(),
                        provenance: sample.provenance(),
                        target_wind_response: voice.target_response(),
                        current_wind_response: voice.current_response(),
                        current_volume_db: voice.current_volume_db(),
                        direct_path: self.telemetry.direct_path(key).cloned(),
                    }
                })
            })
            .collect::<Vec<_>>();
        samples.sort_by_key(|sample| sample.key);
        Some(samples)
    }

    pub fn petal_superseded_solve_count(&self) -> u64 {
        self.spatial_sound_manager.acoustic_superseded_solve_count()
    }

    pub fn telemetry_diagnostics(&self) -> crate::audio::CanopyAudioTelemetryDiagnostics {
        self.telemetry.diagnostics()
    }

    pub fn petal_acoustic_telemetry_diagnostics(&self) -> AcousticTelemetryDiagnostics {
        self.spatial_sound_manager.acoustic_telemetry_diagnostics()
    }

    pub fn petal_runtime_diagnostics(&self) -> RuntimeDiagnostics {
        self.spatial_sound_manager.runtime_diagnostics()
    }

    fn spawn_voice(
        &mut self,
        active: &ActiveCanopyAcousticGeneration,
        base_volume_db: f32,
        wind_response_curve: WindResponseCurve,
        time_seconds: f32,
    ) -> Result<Uuid> {
        #[cfg(test)]
        if self.fail_spawn_key == Some(active.key()) {
            self.fail_spawn_key = None;
            anyhow::bail!(
                "injected canopy Voice spawn failure tree={} generation={}",
                active.key().tree_id(),
                active.key().generation(),
            );
        }
        let descriptor = active.descriptor();
        let prepared = self
            .recorded_bank
            .prepare(active.key(), time_seconds, 0.0)?;
        let uuid = self
            .spatial_sound_manager
            .add_canopy_looping_clip_with_extent_at_phase(
                active.key(),
                prepared.clip,
                TREE_SILENT_VOLUME_DB,
                descriptor.tree_origin_world(),
                prepared.phase,
                Self::source_extent(descriptor)?,
                Self::occlusion_profile(),
            )?;
        let voice = CanopyAudioVoice::new(
            uuid,
            active.key(),
            descriptor.clone(),
            prepared.control,
            Self::content_volume_db(base_volume_db),
            wind_response_curve,
        );
        self.voices.insert(active.key(), voice);
        Ok(uuid)
    }

    fn content_volume_db(base_volume_db: f32) -> f32 {
        base_volume_db + MAKEUP_GAIN_DB
    }

    fn source_extent(descriptor: &CanopyAcousticDescriptor) -> Result<SourceExtent> {
        Self::source_extent_at_anchor(descriptor, glam::Vec3::ZERO)
    }

    /// Same immutable sample IDs/power, rebased about a stable tree-local emission origin.
    /// Ambient callers keep the original zero/tree-root anchor unchanged.
    pub(crate) fn source_extent_at_anchor(
        descriptor: &CanopyAcousticDescriptor,
        anchor_tree_voxels: glam::Vec3,
    ) -> Result<SourceExtent> {
        let samples = descriptor
            .samples()
            .iter()
            .map(|sample| {
                let local =
                    (sample.position_tree_voxels() - anchor_tree_voxels) / VOXELS_PER_WORLD_UNIT;
                ExtentSample::new(
                    ExtentSampleId(sample.id().value()),
                    petalsonic::Vec3::new(local.x, local.y, local.z),
                    sample.weight(),
                )
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(SourceExtent::weighted_samples_with_limit(
            samples,
            descriptor.sample_budget(),
        )?)
    }

    fn occlusion_profile() -> OcclusionProfile {
        OcclusionProfile::AmbientDistributed(DistributedOcclusionProfile::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::CanopyAudioLifecycle;
    use crate::tree_gen::LeafPlacement;
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

    fn test_manager() -> SpatialSoundManager {
        SpatialSoundManager::new(
            64,
            AcousticSceneSnapshot::new(1, Arc::new(NoAcousticHits)),
            Some("re-flora-canopy-publication-device-that-does-not-exist".to_owned()),
            EnvironmentalAcousticsBudget::default(),
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
    fn descriptor_maps_to_one_normalized_weighted_extent() {
        let leaves = [-4.0, 4.0]
            .into_iter()
            .flat_map(|x| {
                [-4.0, 4.0].into_iter().flat_map(move |y| {
                    [-4.0, 4.0].into_iter().map(move |z| LeafPlacement {
                        position: Vec3::new(x, y, z),
                        anchor: Vec3::ZERO,
                    })
                })
            })
            .collect::<Vec<_>>();
        let descriptor = CanopyAcousticDescriptor::build(7, Vec3::ONE, 123, &leaves, &[]);

        let extent = CanopyDistributedEmitterAdapter::source_extent(&descriptor).unwrap();
        let weighted = extent.weighted().unwrap();

        assert_eq!(weighted.samples().len(), 8);
        assert_eq!(weighted.samples().len(), descriptor.samples().len());
        assert!(
            (weighted
                .samples()
                .iter()
                .map(ExtentSample::power_weight)
                .sum::<f32>()
                - 1.0)
                .abs()
                < 1.0e-6
        );
        for (petal, canopy) in weighted.samples().iter().zip(descriptor.samples()) {
            assert_eq!(petal.id(), ExtentSampleId(canopy.id().value()));
            let expected = canopy.position_tree_voxels() / VOXELS_PER_WORLD_UNIT;
            assert_eq!(
                petal.local_position(),
                petalsonic::Vec3::new(expected.x, expected.y, expected.z),
            );
            assert!((petal.power_weight() - canopy.weight()).abs() < 1.0e-6);
        }
        assert!(matches!(
            CanopyDistributedEmitterAdapter::occlusion_profile(),
            OcclusionProfile::AmbientDistributed(_)
        ));
    }

    #[test]
    fn recorded_leaf_gain_preserves_the_enabled_mode_calibration() {
        assert_eq!(
            CanopyDistributedEmitterAdapter::content_volume_db(-15.0),
            -3.0
        );
    }

    #[test]
    fn configurable_budget_reaches_backend_without_eight_point_truncation() {
        let leaves: Vec<_> = (0..64)
            .map(|i| LeafPlacement {
                position: Vec3::X * i as f32 * 4.0,
                anchor: Vec3::ZERO,
            })
            .collect();
        for budget in [1, 8, 16, 32, 64] {
            let descriptor = CanopyAcousticDescriptor::build_with_budget(
                1,
                Vec3::ZERO,
                123,
                &leaves,
                &[],
                budget,
            );
            let extent = CanopyDistributedEmitterAdapter::source_extent(&descriptor).unwrap();
            assert_eq!(extent.sample_count(), budget);
            assert_eq!(descriptor.sample_budget(), budget);
            assert!((descriptor.total_weight() - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    #[ignore = "explicit native audio lifecycle check; opens only the unavailable-device fallback"]
    fn recorded_leaf_mix_keeps_one_source_per_generation_across_wind_and_volume_changes() {
        let manager = test_manager();
        let mut adapter = CanopyDistributedEmitterAdapter::new(manager.clone()).unwrap();
        let mut lifecycle = CanopyAudioLifecycle::new(0.35);
        for tree in 0..8 {
            lifecycle
                .replace(tree, descriptor(1, tree as f32), 0.0)
                .unwrap();
        }
        let snapshot = lifecycle.snapshot(1.0).unwrap();
        let curve = WindResponseCurve {
            min_strength: 0.0,
            max_strength: 1.0,
            power: 1.0,
        };
        adapter.synchronize(&snapshot, -15.0, curve, 1.0).unwrap();
        let identities = adapter
            .voices
            .iter()
            .map(|(key, voice)| (*key, voice.uuid))
            .collect::<HashMap<_, _>>();
        for (step, volume_db) in [-15.0, -10.0, -5.0, -20.0, 0.0].into_iter().enumerate() {
            let time = 10.0 + step as f32 * 20.0;
            adapter.set_base_volume_db(volume_db).unwrap();
            adapter
                .synchronize(&snapshot, volume_db, curve, time)
                .unwrap();
            assert_eq!(adapter.voices.len(), 8);
            for (key, voice) in &adapter.voices {
                assert_eq!(voice.uuid, identities[key]);
                assert_eq!(voice.descriptor.samples().len(), 1);
            }
            for (offset, strength) in [(0.0, 0.0), (5.0, 2.0), (10.0, 8.0)] {
                adapter
                    .update(
                        &WindFieldFrame::uniform(glam::Vec2::X * strength),
                        time + offset,
                        1.0,
                        1.0,
                    )
                    .unwrap();
                for voice in adapter.voices.values() {
                    assert!((voice.recorded_blend.position() - strength / 8.0).abs() < 1e-5);
                    if strength > 0.0 {
                        let expected = volume_db + 12.0 + 20.0 * (strength / 8.0).log10();
                        assert!((voice.current_volume_db() - expected).abs() < 1e-4);
                    }
                }
            }
        }
        // Changing geometry starts only one additional generation, not one per audio layer.
        lifecycle.replace(0, descriptor(2, 0.0), 110.0).unwrap();
        adapter
            .synchronize(&lifecycle.snapshot(110.1).unwrap(), -15.0, curve, 110.1)
            .unwrap();
        assert_eq!(adapter.voices.len(), 9);
        adapter
            .synchronize(&lifecycle.snapshot(111.0).unwrap(), -15.0, curve, 111.0)
            .unwrap();
        assert_eq!(adapter.voices.len(), 8);
        for tree in 0..8 {
            lifecycle.remove(tree, 112.0).unwrap();
        }
        adapter
            .synchronize(&lifecycle.snapshot(113.0).unwrap(), -15.0, curve, 113.0)
            .unwrap();
        assert!(adapter.voices.is_empty());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let diag = manager.runtime_diagnostics();
            if diag.active_emitters == 0 && diag.active_voices == 0 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "retirement did not drain: {diag:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        println!("recorded leaves: 8 generations, 5 volume changes, calm/weak/strong wind, geometry crossfade, complete retirement");
    }

    #[test]
    fn failed_generation_spawn_rolls_back_every_voice_created_by_the_sync() {
        let manager = test_manager();
        let mut adapter = CanopyDistributedEmitterAdapter::new(manager).unwrap();
        let mut lifecycle = CanopyAudioLifecycle::new(0.35);
        lifecycle.replace(1, descriptor(1, 1.0), 0.0).unwrap();
        lifecycle.replace(2, descriptor(2, 2.0), 0.0).unwrap();
        let snapshot = lifecycle.snapshot(0.0).unwrap();
        let failing_key = CanopyAudioGenerationKey::new(2, 2);
        adapter.fail_spawn_for_test(failing_key);

        let error = adapter
            .synchronize(
                &snapshot,
                -10.0,
                WindResponseCurve {
                    min_strength: 0.0,
                    max_strength: 1.0,
                    power: 1.0,
                },
                0.0,
            )
            .expect_err("the second generation spawn should fail the whole sync");

        assert!(format!("{error:#}").contains("injected canopy Voice spawn failure"));
        assert!(adapter.active_generation_keys_for_test().is_empty());
    }
}
