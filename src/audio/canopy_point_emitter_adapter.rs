use crate::audio::{
    ActiveCanopyAcousticSample, CanopyAudioLifecycleSnapshot, CanopyAudioSampleTelemetry,
    CanopyAudioSourceKey, CanopyAudioTelemetry, CanopyDirectPathTelemetry, SpatialSoundManager,
    TreeAudioSource,
};
use crate::wind::{Wind, WindResponseCurve, WindSource};
use anyhow::Result;
use log::warn;
use petalsonic::ResidentClip;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

const TREE_SILENT_VOLUME_DB: f32 = -80.0;

/// Compatibility adapter that realizes a weighted canopy generation as PetalSonic point emitters.
///
/// The adapter consumes lifecycle snapshots but does not select canopy geometry or own generation
/// transitions. A future PetalSonic distributed-emitter implementation can replace this module
/// without changing tree generation or lifecycle policy.
pub struct CanopyPointEmitterAdapter {
    spatial_sound_manager: SpatialSoundManager,
    telemetry: CanopyAudioTelemetry,
    sources: HashMap<CanopyAudioSourceKey, TreeAudioSource>,
}

impl CanopyPointEmitterAdapter {
    pub fn new(spatial_sound_manager: SpatialSoundManager) -> Self {
        Self {
            spatial_sound_manager,
            telemetry: CanopyAudioTelemetry::default(),
            sources: HashMap::new(),
        }
    }

    pub fn synchronize(
        &mut self,
        snapshot: &CanopyAudioLifecycleSnapshot,
        rustle_clip: &ResidentClip,
        base_volume_db: f32,
        wind_response_curve: WindResponseCurve,
        base_wind: f32,
    ) -> Result<Vec<Uuid>> {
        let active_keys = snapshot
            .samples()
            .iter()
            .map(ActiveCanopyAcousticSample::key)
            .collect::<HashSet<_>>();
        let stale_keys = self
            .sources
            .keys()
            .copied()
            .filter(|key| !active_keys.contains(key))
            .collect::<Vec<_>>();
        for key in stale_keys {
            if let Some(source) = self.sources.remove(&key) {
                self.spatial_sound_manager.remove_source(source.uuid);
            }
            self.telemetry.remove_source(key);
        }

        let mut created = Vec::new();
        for active in snapshot.samples() {
            if !self.sources.contains_key(&active.key()) {
                match self.spawn_source(
                    active,
                    rustle_clip,
                    base_volume_db,
                    wind_response_curve,
                    base_wind,
                ) {
                    Ok(uuid) => created.push(uuid),
                    Err(error) => {
                        warn!(
                            "Failed to spawn canopy audio source tree={} generation={} sample={}: {error:#}",
                            active.key().tree_id(),
                            active.key().generation(),
                            active.key().sample_id().value(),
                        );
                        continue;
                    }
                }
            }
            if let Some(source) = self.sources.get_mut(&active.key()) {
                source
                    .set_lifecycle_power(active.lifecycle_power(), &self.spatial_sound_manager)?;
            }
        }
        Ok(created)
    }

    pub fn update(
        &mut self,
        wind: &Wind,
        time_seconds: f32,
        wind_sources: &[WindSource],
        wind_audio_attack_decay: f32,
        wind_audio_release_decay: f32,
    ) -> Result<()> {
        for source in self.sources.values_mut() {
            source.update(
                wind,
                time_seconds,
                wind_sources,
                wind_audio_attack_decay,
                wind_audio_release_decay,
                &self.spatial_sound_manager,
            )?;
        }
        Ok(())
    }

    pub fn set_base_volume_db(&mut self, base_volume_db: f32) -> Result<()> {
        for source in self.sources.values_mut() {
            source.set_wind_volume_db(base_volume_db, &self.spatial_sound_manager)?;
        }
        Ok(())
    }

    pub fn set_wind_response_curve(&mut self, wind_response_curve: WindResponseCurve) {
        for source in self.sources.values_mut() {
            source.set_wind_response_curve(wind_response_curve);
        }
    }

    pub fn replace_rustle_clip(
        &mut self,
        rustle_clip: &ResidentClip,
        base_wind: f32,
    ) -> Result<()> {
        for source in self.sources.values_mut() {
            self.spatial_sound_manager.replace_looping_clip(
                source.uuid,
                rustle_clip.clone(),
                source.phase,
            )?;
            source.set_base_wind(base_wind);
        }
        Ok(())
    }

    pub fn remove_all(&mut self) {
        for source in self.sources.values() {
            self.spatial_sound_manager.remove_source(source.uuid);
        }
        self.sources.clear();
    }

    pub fn set_telemetry_enabled(&mut self, enabled: bool) {
        self.telemetry.set_enabled(enabled);
    }

    /// Consumer hook for direct-path diagnostics from the forthcoming PetalSonic distributed
    /// emitter API. PetalSonic 0.7 does not expose this per-emitter observation yet.
    #[allow(dead_code)]
    pub fn observe_direct_path(
        &mut self,
        key: CanopyAudioSourceKey,
        observation: CanopyDirectPathTelemetry,
    ) {
        if self.sources.contains_key(&key) {
            self.telemetry.observe_direct_path(key, observation);
        }
    }

    pub fn telemetry_samples(&self) -> Option<Vec<CanopyAudioSampleTelemetry>> {
        if !self.telemetry.is_enabled() {
            return None;
        }
        let mut samples = self
            .sources
            .values()
            .map(|source| CanopyAudioSampleTelemetry {
                key: source.key,
                emitter_uuid: source.uuid,
                position_tree_voxels: source.position_tree_voxels,
                position_world: source.position,
                clearance_voxels: source.clearance_voxels,
                weight: source.sample_weight,
                lifecycle_power: source.lifecycle_power(),
                content_seed: source.content_seed,
                phase: source.phase,
                provenance: source.provenance,
                target_wind_response: source.target_response(),
                current_wind_response: source.current_response(),
                current_volume_db: source.current_volume_db(),
                direct_path: self.telemetry.direct_path(source.key).cloned(),
            })
            .collect::<Vec<_>>();
        samples.sort_by_key(|sample| sample.key);
        Some(samples)
    }

    pub fn petal_superseded_solve_count(&self) -> u64 {
        self.spatial_sound_manager.acoustic_superseded_solve_count()
    }

    fn spawn_source(
        &mut self,
        active: &ActiveCanopyAcousticSample,
        rustle_clip: &ResidentClip,
        base_volume_db: f32,
        wind_response_curve: WindResponseCurve,
        base_wind: f32,
    ) -> Result<Uuid> {
        let sample = active.sample();
        let position = active.world_position();
        let uuid = self
            .spatial_sound_manager
            .add_looping_spatial_clip_at_phase(
                rustle_clip.clone(),
                TREE_SILENT_VOLUME_DB,
                position,
                sample.phase(),
            )?;
        self.sources.insert(
            active.key(),
            TreeAudioSource::new(
                uuid,
                active.key(),
                sample.position_tree_voxels(),
                position,
                sample.weight(),
                sample.clearance_voxels(),
                sample.content_seed(),
                sample.phase(),
                sample.provenance(),
                base_volume_db,
                wind_response_curve,
                base_wind,
            ),
        );
        Ok(uuid)
    }
}
