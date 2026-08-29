use crate::audio::{
    CanopyAcousticDescriptor, CanopyAcousticObservation, CanopyAudioLifecycle,
    CanopyAudioTelemetrySnapshot, CanopyAudioTreeTelemetry, CanopyDistributedEmitterAdapter,
    SpatialSoundManager, TreeRustleControl, TreeRustleFactory, TreeRustleParams,
};
use crate::wind::{Wind, WindResponseCurve, WindSource};
use anyhow::Result;
use petalsonic::ResidentClip;
use std::sync::Arc;
use uuid::Uuid;

// The old tree loop asset was baked with a large pregain
// (`tree_sound_48k_pregain_40db.wav`). The procedural generator is tuned at
// reference-like raw levels, so add runtime makeup gain before the normal tree
// volume and normalized canopy sample weights are applied.
const PROCEDURAL_RUSTLE_MAKEUP_GAIN_DB: f32 = 36.0;
const CANOPY_LAYOUT_CROSSFADE_SECONDS: f32 = 0.35;
const TREE_RUSTLE_SAMPLE_RATE: u32 = 48_000;
const TREE_RUSTLE_LOOP_SECONDS: f32 = 12.0;
const TREE_RUSTLE_CLIP_SEED: u64 = 0x94d0_49bb_1331_11eb;

/// Coordinates immutable weighted canopy descriptors, generation lifetime, and rustle content.
///
/// Geometry/layout selection lives in `CanopyAcousticDescriptor`, generation mixing lives in
/// `CanopyAudioLifecycle`, and PetalSonic realization lives behind the emitter adapter.
pub struct TreeAudioManager {
    wind_volume_db: f32,
    wind_response_curve: WindResponseCurve,
    rustle_params: TreeRustleParams,
    rustle_clip: ResidentClip,
    lifecycle: CanopyAudioLifecycle,
    emitter_adapter: CanopyDistributedEmitterAdapter,
    wind: Wind,
}

impl TreeAudioManager {
    pub fn new(
        spatial_sound_manager: SpatialSoundManager,
        wind_response_curve: WindResponseCurve,
        wind_volume_db: f32,
        rustle_params: TreeRustleParams,
    ) -> Result<Self> {
        let rustle_clip = Self::render_rustle_clip(rustle_params)?;
        Ok(Self {
            wind_volume_db,
            wind_response_curve,
            rustle_params,
            rustle_clip,
            lifecycle: CanopyAudioLifecycle::new(CANOPY_LAYOUT_CROSSFADE_SECONDS),
            emitter_adapter: CanopyDistributedEmitterAdapter::new(spatial_sound_manager),
            wind: Wind::new(),
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
            &self.rustle_clip,
            Self::base_volume_db(self.wind_volume_db),
            self.wind_response_curve,
            self.rustle_params.base_wind,
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
            &self.rustle_clip,
            Self::base_volume_db(self.wind_volume_db),
            self.wind_response_curve,
            self.rustle_params.base_wind,
            time_seconds,
        )?;
        Ok(())
    }

    /// Immediately clear every physical source. This is a shutdown/testing escape hatch; normal
    /// tree removal must use the bounded lifecycle above.
    #[allow(dead_code)]
    pub fn remove_all(&mut self) {
        self.emitter_adapter.remove_all();
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
        self.emitter_adapter
            .set_base_volume_db(Self::base_volume_db(self.wind_volume_db))
    }

    pub fn set_wind_response_curve(&mut self, wind_response_curve: WindResponseCurve) {
        if wind_response_curve == self.wind_response_curve {
            return;
        }

        self.wind_response_curve = wind_response_curve;
        self.emitter_adapter
            .set_wind_response_curve(self.wind_response_curve);
    }

    pub fn set_rustle_params(&mut self, rustle_params: TreeRustleParams) -> Result<()> {
        if rustle_params == self.rustle_params {
            return Ok(());
        }

        let rustle_clip = Self::render_rustle_clip(rustle_params)?;
        self.emitter_adapter
            .replace_rustle_clip(&rustle_clip, rustle_params.base_wind)?;
        self.rustle_params = rustle_params;
        self.rustle_clip = rustle_clip;
        Ok(())
    }

    pub fn update(
        &mut self,
        time_seconds: f32,
        wind_sources: &[WindSource],
        wind_audio_attack_decay: f32,
        wind_audio_release_decay: f32,
    ) -> Result<()> {
        let snapshot = self.lifecycle.snapshot(time_seconds)?;
        self.emitter_adapter.synchronize(
            &snapshot,
            &self.rustle_clip,
            Self::base_volume_db(self.wind_volume_db),
            self.wind_response_curve,
            self.rustle_params.base_wind,
            time_seconds,
        )?;
        self.emitter_adapter.update(
            &self.wind,
            time_seconds,
            wind_sources,
            wind_audio_attack_decay,
            wind_audio_release_decay,
        )
    }

    fn render_rustle_clip(params: TreeRustleParams) -> Result<ResidentClip> {
        let control = Arc::new(TreeRustleControl::with_params(params));
        TreeRustleFactory::new(TREE_RUSTLE_CLIP_SEED, control)
            .render_resident_clip(TREE_RUSTLE_SAMPLE_RATE, TREE_RUSTLE_LOOP_SECONDS)
            .map_err(Into::into)
    }

    fn base_volume_db(volume_db: f32) -> f32 {
        volume_db + PROCEDURAL_RUSTLE_MAKEUP_GAIN_DB
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_volume_keeps_procedural_rustle_makeup_without_leaf_count_gain() {
        assert_eq!(TreeAudioManager::base_volume_db(-15.0), 21.0);
    }
}
