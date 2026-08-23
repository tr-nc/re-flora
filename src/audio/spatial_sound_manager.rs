use crate::audio::audio_clip_cache::AudioClipCache;
use crate::gameplay::camera::vectors::CameraVectors;
use anyhow::Result;
use glam::Vec3;
use petalsonic::{
    AcousticSceneSnapshot, BusParams, Emitter, EmitterDesc, EmitterSpatialState, LatencyProfile,
    OutputDevicePolicy, PetalSonicEvent, PetalSonicWorld, PetalSonicWorldDesc, PlayOptions,
    PlaybackControl, PlaybackTag, Pose, Quat as PetalQuat, ResidentClip, RuntimeState,
    SpatialFrame, SpatialQuality, Vec3 as PetalVec3, VoiceTelemetryEvent,
};
use rand::RngExt;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use uuid::Uuid;

struct SourceInfo {
    emitter: Emitter,
    volume_db: f32,
    position: Option<Vec3>,
}

struct OneShotEmitter {
    emitter: Emitter,
    volume_db: f32,
}

struct TransientSpatialSource {
    volume_db: f32,
    position: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TransientSpatialEmitter {
    emitter: Emitter,
}

impl TransientSpatialEmitter {
    pub(crate) fn matches(self, emitter: Emitter) -> bool {
        self.emitter == emitter
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpatialFramePublication {
    revision: u64,
}

impl SpatialFramePublication {
    pub(crate) fn revision(self) -> u64 {
        self.revision
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AcousticPipelineSnapshot {
    pub(crate) enabled: bool,
    pub(crate) solve_count: u64,
    pub(crate) superseded_solve_count: u64,
    pub(crate) published_response_count: u64,
    pub(crate) response_spatial_revision: u64,
    pub(crate) response_geometry_version: u64,
    pub(crate) response_age_ms: u64,
    pub(crate) dropped_voice_telemetry_count: u64,
}

impl AcousticPipelineSnapshot {
    pub(crate) fn activity_since(self, earlier: Self) -> AcousticPipelineActivity {
        AcousticPipelineActivity {
            solves: self.solve_count.saturating_sub(earlier.solve_count),
            superseded: self
                .superseded_solve_count
                .saturating_sub(earlier.superseded_solve_count),
            published: self
                .published_response_count
                .saturating_sub(earlier.published_response_count),
            dropped_voice_telemetry: self
                .dropped_voice_telemetry_count
                .saturating_sub(earlier.dropped_voice_telemetry_count),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AcousticPipelineActivity {
    pub(crate) solves: u64,
    pub(crate) superseded: u64,
    pub(crate) published: u64,
    pub(crate) dropped_voice_telemetry: u64,
}

/// Re:Flora's narrow adapter around the world-owned PetalSonic runtime.
#[derive(Clone)]
pub struct SpatialSoundManager {
    world: Arc<PetalSonicWorld>,
    clip_cache: Arc<AudioClipCache>,
    uuid_to_source: Arc<Mutex<HashMap<Uuid, SourceInfo>>>,
    one_shot_emitters: Arc<Mutex<HashMap<String, OneShotEmitter>>>,
    transient_spatial_emitters: Arc<Mutex<HashMap<Emitter, TransientSpatialSource>>>,
    listener_state: Arc<Mutex<ListenerState>>,
    global_volume_gain_db: Arc<Mutex<f32>>,
    health_log_state: Arc<Mutex<AudioHealthLogState>>,
    spatial_frame_revision: Arc<AtomicU64>,
    published_acoustic_scene_version: Arc<AtomicU64>,
}

#[derive(Default)]
struct AudioHealthLogState {
    runtime_state: Option<RuntimeState>,
    device_generation: u64,
    underruns: usize,
    rejected_commands: u64,
    dropped_events: u64,
    dropped_voice_telemetry: u64,
    acoustic_published_responses: u64,
    acoustic_response_geometry_version: u64,
}

#[derive(Clone, Debug)]
struct ListenerState {
    position: Vec3,
    up: Vec3,
    front: Vec3,
    right: Vec3,
}

impl Default for ListenerState {
    fn default() -> Self {
        let mut dummy_vectors = CameraVectors::new();
        dummy_vectors.update(0.0, 0.0);
        Self {
            position: Vec3::ZERO,
            up: dummy_vectors.up,
            front: dummy_vectors.front,
            right: dummy_vectors.right,
        }
    }
}

impl SpatialSoundManager {
    pub fn new(
        frame_window_size: usize,
        acoustic_scene: AcousticSceneSnapshot,
        audio_output_device: Option<String>,
    ) -> Result<Self> {
        let project_root = crate::util::get_project_root();
        let native_hrtf_path = format!("{}assets/hrtf/hrtf_b_nh172.petalhrtf", project_root);
        let initial_acoustic_scene_version = acoustic_scene.version();

        let world = PetalSonicWorld::new(PetalSonicWorldDesc {
            sample_rate: 48_000,
            block_size: frame_window_size,
            max_emitters: 8_192,
            max_voices: 8_192,
            output_device: audio_output_device.map_or(
                OutputDevicePolicy::FollowSystemDefault,
                OutputDevicePolicy::PinnedNameContains,
            ),
            spatial_quality: SpatialQuality::LowLatency,
            latency_profile: LatencyProfile::Balanced,
            native_hrtf_path: Some(native_hrtf_path),
            hrtf_gain: 0.0,
            distance_scaler: 15.0,
            acoustic_scene: Some(acoustic_scene),
            ..PetalSonicWorldDesc::default()
        })?;

        Ok(Self {
            world: Arc::new(world),
            clip_cache: Arc::new(AudioClipCache::new()?),
            uuid_to_source: Arc::new(Mutex::new(HashMap::new())),
            one_shot_emitters: Arc::new(Mutex::new(HashMap::new())),
            transient_spatial_emitters: Arc::new(Mutex::new(HashMap::new())),
            listener_state: Arc::new(Mutex::new(ListenerState::default())),
            global_volume_gain_db: Arc::new(Mutex::new(0.0)),
            health_log_state: Arc::new(Mutex::new(AudioHealthLogState::default())),
            spatial_frame_revision: Arc::new(AtomicU64::new(0)),
            published_acoustic_scene_version: Arc::new(AtomicU64::new(
                initial_acoustic_scene_version,
            )),
        })
    }

    fn emitter_desc(position: Option<Vec3>, volume_db: f32) -> EmitterDesc {
        match position {
            Some(position) => EmitterDesc::spatial(Self::pose(position)).with_gain_db(volume_db),
            None => EmitterDesc::non_spatial().with_gain_db(volume_db),
        }
    }

    fn pose(position: Vec3) -> Pose {
        Pose::new(
            PetalVec3::new(position.x, position.y, position.z),
            PetalQuat::IDENTITY,
        )
    }

    fn add_looping_clip_source(
        &self,
        clip: ResidentClip,
        volume_db: f32,
        position: Option<Vec3>,
        shuffle_phase: bool,
    ) -> Result<Uuid> {
        let emitter = self
            .world
            .create_emitter(clip, Self::emitter_desc(position, volume_db))?;
        if let Err(error) = self.world.play(emitter, PlayOptions::looping()) {
            let _ = self.world.destroy_emitter(emitter);
            return Err(error.into());
        }
        if shuffle_phase {
            if let Err(error) = self
                .world
                .seek_emitter(emitter, rand::rng().random_range(0.0..1.0))
            {
                let _ = self.world.destroy_emitter(emitter);
                return Err(error.into());
            }
        }

        let uuid = Uuid::new_v4();
        self.uuid_to_source.lock().unwrap().insert(
            uuid,
            SourceInfo {
                emitter,
                volume_db,
                position,
            },
        );
        Ok(uuid)
    }

    fn cached_clip(&self, path: &str) -> Result<ResidentClip> {
        self.clip_cache
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Audio clip not found in cache: {path}"))
    }

    pub fn add_looping_spatial_source(
        &self,
        path: &str,
        volume_db: f32,
        position: Vec3,
        shuffle_phase: bool,
    ) -> Result<Uuid> {
        self.add_looping_clip_source(
            self.cached_clip(path)?,
            volume_db,
            Some(position),
            shuffle_phase,
        )
    }

    pub fn add_looping_spatial_clip(
        &self,
        clip: ResidentClip,
        volume_db: f32,
        position: Vec3,
        shuffle_phase: bool,
    ) -> Result<Uuid> {
        self.add_looping_clip_source(clip, volume_db, Some(position), shuffle_phase)
    }

    pub fn add_non_spatial_source(&self, path: &str, volume_db: f32) -> Result<()> {
        let mut emitters = self.one_shot_emitters.lock().unwrap();
        let emitter = if let Some(source) = emitters.get_mut(path) {
            if (source.volume_db - volume_db).abs() > f32::EPSILON {
                self.world.update_emitter(
                    source.emitter,
                    EmitterDesc::non_spatial().with_gain_db(volume_db),
                )?;
                source.volume_db = volume_db;
            }
            source.emitter
        } else {
            let emitter = self.world.create_emitter(
                self.cached_clip(path)?,
                EmitterDesc::non_spatial().with_gain_db(volume_db),
            )?;
            emitters.insert(path.to_owned(), OneShotEmitter { emitter, volume_db });
            emitter
        };
        self.world.play(emitter, PlayOptions::once())?;
        Ok(())
    }

    pub(crate) fn create_transient_spatial_emitter(
        &self,
        path: &str,
        volume_db: f32,
        position: Vec3,
    ) -> Result<TransientSpatialEmitter> {
        let emitter = self.world.create_emitter(
            self.cached_clip(path)?,
            EmitterDesc::spatial(Self::pose(position)).with_gain_db(volume_db),
        )?;
        self.transient_spatial_emitters.lock().unwrap().insert(
            emitter,
            TransientSpatialSource {
                volume_db,
                position,
            },
        );
        Ok(TransientSpatialEmitter { emitter })
    }

    pub(crate) fn transient_clip_duration_seconds(&self, path: &str) -> Result<f64> {
        let clip = self.cached_clip(path)?;
        Ok(clip.total_frames() as f64 / f64::from(clip.sample_rate()))
    }

    pub(crate) fn play_controlled_transient(
        &self,
        emitter: TransientSpatialEmitter,
        options: PlayOptions,
        tag: PlaybackTag,
    ) -> Result<PlaybackControl> {
        Ok(self.world.play_controlled(emitter.emitter, options, tag)?)
    }

    pub(crate) fn stop_controlled_transient(&self, control: PlaybackControl) -> Result<()> {
        Ok(self.world.stop_playback(control)?)
    }

    pub(crate) fn destroy_transient_spatial_emitter(
        &self,
        emitter: TransientSpatialEmitter,
    ) -> Result<()> {
        self.world.destroy_emitter(emitter.emitter)?;
        self.transient_spatial_emitters
            .lock()
            .unwrap()
            .remove(&emitter.emitter);
        Ok(())
    }

    pub(crate) fn drain_events(&self) -> Vec<PetalSonicEvent> {
        self.world.drain_events()
    }

    pub(crate) fn drain_voice_telemetry(&self) -> Vec<VoiceTelemetryEvent> {
        self.world.drain_voice_telemetry()
    }

    pub fn update_player_pos(
        &self,
        player_pos: Vec3,
        camera_vectors: &CameraVectors,
    ) -> Result<()> {
        let mut listener = self.listener_state.lock().unwrap();
        listener.position = player_pos;
        listener.up = camera_vectors.up;
        listener.front = camera_vectors.front;
        listener.right = camera_vectors.right;
        Ok(())
    }

    /// Publish one complete listener + spatial Emitter generation for this game frame.
    pub(crate) fn publish_spatial_frame(
        &self,
        sim_time_seconds: f64,
    ) -> Result<SpatialFramePublication> {
        let listener = self.listener_state.lock().unwrap().clone();
        let rotation_matrix = glam::Mat3::from_cols(listener.right, listener.up, -listener.front);
        let rotation = glam::Quat::from_mat3(&rotation_matrix);
        let listener_pose = Pose::new(
            PetalVec3::new(
                listener.position.x,
                listener.position.y,
                listener.position.z,
            ),
            PetalQuat::from_xyzw(rotation.x, rotation.y, rotation.z, rotation.w),
        );
        let mut emitters: Vec<_> = self
            .uuid_to_source
            .lock()
            .unwrap()
            .values()
            .filter_map(|source| {
                source.position.map(|position| {
                    EmitterSpatialState::new(source.emitter, Self::pose(position))
                        .with_acoustic_priority(Self::acoustic_priority(source.volume_db))
                })
            })
            .collect();
        emitters.extend(self.transient_spatial_emitters.lock().unwrap().iter().map(
            |(emitter, source)| {
                EmitterSpatialState::new(*emitter, Self::pose(source.position))
                    .with_acoustic_priority(Self::acoustic_priority(source.volume_db))
            },
        ));
        let revision = self
            .spatial_frame_revision
            .fetch_add(1, Ordering::Relaxed)
            .checked_add(1)
            .expect("spatial audio frame revision overflowed");
        self.world.publish_spatial_frame(SpatialFrame::new(
            revision,
            sim_time_seconds,
            listener_pose,
            emitters,
        ))?;

        let status = self.world.runtime_status();
        let diagnostics = self.world.diagnostics();
        let voice_telemetry = self.world.voice_telemetry_diagnostics();
        let mut previous = self.health_log_state.lock().unwrap();
        if previous.runtime_state != Some(status.state)
            || previous.device_generation != diagnostics.device_generation
        {
            log::info!(
                "PetalSonic runtime: state={:?}, device={:?}, generation={}, sample_rate={}, channels={}",
                status.state,
                status.active_output_device,
                diagnostics.device_generation,
                diagnostics.output_sample_rate,
                diagnostics.output_channels,
            );
        }
        if diagnostics.underrun_count > previous.underruns
            || diagnostics.rejected_commands > previous.rejected_commands
            || diagnostics.dropped_events > previous.dropped_events
            || voice_telemetry.dropped_events > previous.dropped_voice_telemetry
        {
            log::warn!(
                "PetalSonic pressure: underruns={}, rejected_commands={}, dropped_events={}, dropped_voice_telemetry={}, voice_telemetry_high_water={}, render_p99_us={}",
                diagnostics.underrun_count,
                diagnostics.rejected_commands,
                diagnostics.dropped_events,
                voice_telemetry.dropped_events,
                voice_telemetry.queue_high_water,
                diagnostics.render_time_p99_us,
            );
        }
        if diagnostics.acoustic_published_response_count > previous.acoustic_published_responses
            && diagnostics.acoustic_response_geometry_version
                != previous.acoustic_response_geometry_version
        {
            log::debug!(
                "PetalSonic acoustics: spatial_revision={}, geometry_version={}, solve_us={}, response_age_ms={}",
                diagnostics.acoustic_response_spatial_revision,
                diagnostics.acoustic_response_geometry_version,
                diagnostics.acoustic_last_solve_time_us,
                diagnostics.acoustic_response_age_ms,
            );
        }
        previous.runtime_state = Some(status.state);
        previous.device_generation = diagnostics.device_generation;
        previous.underruns = diagnostics.underrun_count;
        previous.rejected_commands = diagnostics.rejected_commands;
        previous.dropped_events = diagnostics.dropped_events;
        previous.dropped_voice_telemetry = voice_telemetry.dropped_events;
        previous.acoustic_published_responses = diagnostics.acoustic_published_response_count;
        previous.acoustic_response_geometry_version =
            diagnostics.acoustic_response_geometry_version;
        Ok(SpatialFramePublication { revision })
    }

    pub fn publish_acoustic_scene(&self, snapshot: AcousticSceneSnapshot) -> Result<()> {
        let version = snapshot.version();
        if version
            <= self
                .published_acoustic_scene_version
                .load(Ordering::Acquire)
        {
            return Ok(());
        }
        self.world.publish_acoustic_scene(snapshot)?;
        self.published_acoustic_scene_version
            .store(version, Ordering::Release);
        Ok(())
    }

    pub fn set_environmental_acoustics(&self, enabled: bool, quality: f32) -> Result<()> {
        self.world.set_environmental_acoustics_quality(quality)?;
        self.world.set_environmental_acoustics_enabled(enabled)?;
        Ok(())
    }

    pub(crate) fn acoustic_pipeline_snapshot(&self) -> AcousticPipelineSnapshot {
        let diagnostics = self.world.diagnostics();
        let voice_telemetry = self.world.voice_telemetry_diagnostics();
        AcousticPipelineSnapshot {
            enabled: self.world.environmental_acoustics_enabled(),
            solve_count: diagnostics.acoustic_solve_count,
            superseded_solve_count: diagnostics.acoustic_superseded_solve_count,
            published_response_count: diagnostics.acoustic_published_response_count,
            response_spatial_revision: diagnostics.acoustic_response_spatial_revision,
            response_geometry_version: diagnostics.acoustic_response_geometry_version,
            response_age_ms: diagnostics.acoustic_response_age_ms,
            dropped_voice_telemetry_count: voice_telemetry.dropped_events,
        }
    }

    fn acoustic_priority(volume_db: f32) -> f32 {
        if !volume_db.is_finite() {
            return 0.0;
        }
        10.0_f32.powf(volume_db / 20.0).clamp(0.0, 16.0)
    }

    #[allow(dead_code)]
    pub fn update_source_pos(&self, source_uuid: Uuid, target_pos: Vec3) -> Result<()> {
        if let Some(source) = self.uuid_to_source.lock().unwrap().get_mut(&source_uuid) {
            source.position = Some(target_pos);
        }
        Ok(())
    }

    pub fn update_source_volume(&self, source_uuid: Uuid, volume_db: f32) -> Result<()> {
        let mut sources = self.uuid_to_source.lock().unwrap();
        let Some(source) = sources.get_mut(&source_uuid) else {
            return Ok(());
        };
        source.volume_db = volume_db;
        self.world.update_emitter(
            source.emitter,
            Self::emitter_desc(source.position, source.volume_db),
        )?;
        Ok(())
    }

    pub fn replace_looping_clip(&self, source_uuid: Uuid, clip: ResidentClip) -> Result<()> {
        let mut sources = self.uuid_to_source.lock().unwrap();
        let Some(source) = sources.get_mut(&source_uuid) else {
            return Ok(());
        };
        let old_emitter = source.emitter;
        let new_emitter = self
            .world
            .create_emitter(clip, Self::emitter_desc(source.position, source.volume_db))?;
        if let Err(error) = self.world.play(new_emitter, PlayOptions::looping()) {
            let _ = self.world.destroy_emitter(new_emitter);
            return Err(error.into());
        }
        if let Err(error) = self
            .world
            .seek_emitter(new_emitter, rand::rng().random_range(0.0..1.0))
        {
            let _ = self.world.destroy_emitter(new_emitter);
            return Err(error.into());
        }
        if let Err(error) = self.world.destroy_emitter(old_emitter) {
            let _ = self.world.destroy_emitter(new_emitter);
            return Err(error.into());
        }
        source.emitter = new_emitter;
        Ok(())
    }

    pub fn set_global_volume_gain_db(&self, gain_db: f32) -> Result<()> {
        let mut current = self.global_volume_gain_db.lock().unwrap();
        if (*current - gain_db).abs() <= f32::EPSILON {
            return Ok(());
        }
        self.world.set_bus_params(
            self.world.master_bus(),
            BusParams {
                gain_db,
                ..BusParams::default()
            },
        )?;
        *current = gain_db;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn remove_source(&self, id: Uuid) {
        if let Some(source) = self.uuid_to_source.lock().unwrap().remove(&id) {
            let _ = self.world.destroy_emitter(source.emitter);
        }
    }

    pub fn stop(&self) -> Result<()> {
        let diagnostics = self.world.diagnostics();
        log::info!(
            "[AUDIO][ACOUSTICS] enabled={} quality_percent={:.0} solves={} superseded={} published={} spatial_revision={} geometry_version={} solve_us_p50={} solve_us_p95={} solve_us_p99={} solve_us_max={} response_age_ms={}",
            self.world.environmental_acoustics_enabled(),
            self.world.environmental_acoustics_quality() * 100.0,
            diagnostics.acoustic_solve_count,
            diagnostics.acoustic_superseded_solve_count,
            diagnostics.acoustic_published_response_count,
            diagnostics.acoustic_response_spatial_revision,
            diagnostics.acoustic_response_geometry_version,
            diagnostics.acoustic_solve_time_p50_us,
            diagnostics.acoustic_solve_time_p95_us,
            diagnostics.acoustic_solve_time_p99_us,
            diagnostics.acoustic_solve_time_max_us,
            diagnostics.acoustic_response_age_ms,
        );
        self.world.close()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{AcousticPipelineSnapshot, SpatialSoundManager};

    fn acoustic_snapshot(
        enabled: bool,
        solves: u64,
        superseded: u64,
        published: u64,
        dropped_voice_telemetry: u64,
    ) -> AcousticPipelineSnapshot {
        AcousticPipelineSnapshot {
            enabled,
            solve_count: solves,
            superseded_solve_count: superseded,
            published_response_count: published,
            response_spatial_revision: 0,
            response_geometry_version: 0,
            response_age_ms: 0,
            dropped_voice_telemetry_count: dropped_voice_telemetry,
        }
    }

    #[test]
    fn acoustic_priority_tracks_linear_source_gain_and_sanitizes_invalid_values() {
        assert!((SpatialSoundManager::acoustic_priority(0.0) - 1.0).abs() < f32::EPSILON);
        assert!((SpatialSoundManager::acoustic_priority(-20.0) - 0.1).abs() < f32::EPSILON);
        assert_eq!(SpatialSoundManager::acoustic_priority(f32::NAN), 0.0);
        assert_eq!(SpatialSoundManager::acoustic_priority(100.0), 16.0);
    }

    #[test]
    fn acoustic_activity_uses_monotonic_counter_deltas() {
        let start = acoustic_snapshot(true, 100, 90, 10, 2);
        let end = acoustic_snapshot(true, 107, 97, 10, 5);

        let activity = end.activity_since(start);
        assert_eq!(activity.solves, 7);
        assert_eq!(activity.superseded, 7);
        assert_eq!(activity.published, 0);
        assert_eq!(activity.dropped_voice_telemetry, 3);
    }
}
