use crate::audio::audio_clip_cache::AudioClipCache;
use crate::gameplay::camera::vectors::CameraVectors;
use anyhow::Result;
use glam::Vec3;
use petalsonic::{
    AcousticDiscardReason, AcousticExtentTelemetry, AcousticSceneSnapshot,
    AcousticTelemetryDiagnostics, AcousticTelemetryEvent, BusParams, Emitter, EmitterDesc,
    EmitterSpatialState, EnvironmentalAcousticsBudget, LatencyProfile, OcclusionProfile,
    OutputDevicePolicy, PetalSonicWorld, PetalSonicWorldDesc, PlayOptions, Pose, Quat as PetalQuat,
    ResidentClip, RuntimeDiagnostics, RuntimeState, SourceExtent, SpatialFrame, SpatialQuality,
    Vec3 as PetalVec3,
};
use rand::RngExt;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use uuid::Uuid;

const SPATIAL_FRAME_PUBLISH_INTERVAL_SECONDS: f64 = 1.0 / 30.0;

struct SourceInfo {
    emitter: Emitter,
    volume_db: f32,
    position: Option<Vec3>,
    extent: SourceExtent,
    occlusion_profile: OcclusionProfile,
}

struct OneShotEmitter {
    emitter: Emitter,
    volume_db: f32,
}

pub enum SpatialAcousticTelemetryEvent {
    ExtentResponse {
        source_uuid: Option<Uuid>,
        response: AcousticExtentTelemetry,
    },
    SolveDiscarded {
        spatial_revision: u64,
        geometry_version: u64,
    },
}

/// Re:Flora's narrow adapter around the world-owned PetalSonic runtime.
#[derive(Clone)]
pub struct SpatialSoundManager {
    world: Arc<PetalSonicWorld>,
    clip_cache: Arc<AudioClipCache>,
    uuid_to_source: Arc<Mutex<HashMap<Uuid, SourceInfo>>>,
    one_shot_emitters: Arc<Mutex<HashMap<String, OneShotEmitter>>>,
    listener_state: Arc<Mutex<ListenerState>>,
    global_volume_gain_db: Arc<Mutex<f32>>,
    health_log_state: Arc<Mutex<AudioHealthLogState>>,
    spatial_frame_revision: Arc<AtomicU64>,
    spatial_frame_publish_cadence: Arc<Mutex<SpatialFramePublishCadence>>,
    published_acoustic_scene_version: Arc<AtomicU64>,
}

#[derive(Debug, Default)]
struct SpatialFramePublishCadence {
    last_published_sim_time_seconds: Option<f64>,
    dirty: bool,
}

impl SpatialFramePublishCadence {
    fn should_publish(&self, sim_time_seconds: f64) -> bool {
        let Some(last_published) = self.last_published_sim_time_seconds else {
            return true;
        };
        self.dirty
            || !sim_time_seconds.is_finite()
            || sim_time_seconds < last_published
            || sim_time_seconds - last_published >= SPATIAL_FRAME_PUBLISH_INTERVAL_SECONDS
    }

    fn mark_published(&mut self, sim_time_seconds: f64) {
        self.last_published_sim_time_seconds = Some(sim_time_seconds);
        self.dirty = false;
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

#[derive(Default)]
struct AudioHealthLogState {
    runtime_state: Option<RuntimeState>,
    device_generation: u64,
    underruns: usize,
    rejected_commands: u64,
    dropped_events: u64,
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
        environmental_acoustics_budget: EnvironmentalAcousticsBudget,
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
            environmental_acoustics_budget,
            ..PetalSonicWorldDesc::default()
        })?;

        Ok(Self {
            world: Arc::new(world),
            clip_cache: Arc::new(AudioClipCache::new()?),
            uuid_to_source: Arc::new(Mutex::new(HashMap::new())),
            one_shot_emitters: Arc::new(Mutex::new(HashMap::new())),
            listener_state: Arc::new(Mutex::new(ListenerState::default())),
            global_volume_gain_db: Arc::new(Mutex::new(0.0)),
            health_log_state: Arc::new(Mutex::new(AudioHealthLogState::default())),
            spatial_frame_revision: Arc::new(AtomicU64::new(0)),
            spatial_frame_publish_cadence: Arc::new(Mutex::new(
                SpatialFramePublishCadence::default(),
            )),
            published_acoustic_scene_version: Arc::new(AtomicU64::new(
                initial_acoustic_scene_version,
            )),
        })
    }

    fn emitter_desc(
        position: Option<Vec3>,
        volume_db: f32,
        extent: SourceExtent,
        occlusion_profile: OcclusionProfile,
    ) -> EmitterDesc {
        match position {
            Some(position) => {
                Self::spatial_emitter_desc(position, volume_db, extent, occlusion_profile)
            }
            None => EmitterDesc::non_spatial().with_gain_db(volume_db),
        }
    }

    fn spatial_emitter_desc(
        position: Vec3,
        volume_db: f32,
        extent: SourceExtent,
        occlusion_profile: OcclusionProfile,
    ) -> EmitterDesc {
        EmitterDesc::spatial(Self::pose(position))
            .with_gain_db(volume_db)
            .with_extent(extent)
            .with_occlusion_profile(occlusion_profile)
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
        initial_phase: Option<f32>,
        shuffle_phase: bool,
        extent: SourceExtent,
        occlusion_profile: OcclusionProfile,
    ) -> Result<Uuid> {
        let emitter = self.world.create_emitter(
            clip,
            Self::emitter_desc(position, volume_db, extent.clone(), occlusion_profile),
        )?;
        if let Err(error) = self.world.play(emitter, PlayOptions::looping()) {
            let _ = self.world.destroy_emitter(emitter);
            return Err(error.into());
        }
        let initial_phase = initial_phase
            .map(|phase| phase.clamp(0.0, 1.0))
            .or_else(|| shuffle_phase.then(|| rand::rng().random_range(0.0..1.0)));
        if let Some(initial_phase) = initial_phase {
            if let Err(error) = self.world.seek_emitter(emitter, initial_phase) {
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
                extent,
                occlusion_profile,
            },
        );
        self.mark_spatial_frame_dirty();
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
            None,
            shuffle_phase,
            SourceExtent::Point,
            OcclusionProfile::PointExact,
        )
    }

    pub fn add_looping_spatial_clip_with_extent_at_phase(
        &self,
        clip: ResidentClip,
        volume_db: f32,
        position: Vec3,
        initial_phase: f32,
        extent: SourceExtent,
        occlusion_profile: OcclusionProfile,
    ) -> Result<Uuid> {
        self.add_looping_clip_source(
            clip,
            volume_db,
            Some(position),
            Some(initial_phase),
            false,
            extent,
            occlusion_profile,
        )
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

    /// Publish one complete listener + spatial Emitter generation when its cadence is due.
    pub fn publish_spatial_frame(&self, sim_time_seconds: f64) -> Result<()> {
        let mut cadence = self.spatial_frame_publish_cadence.lock().unwrap();
        if !cadence.should_publish(sim_time_seconds) {
            return Ok(());
        }
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
        let emitters = self
            .uuid_to_source
            .lock()
            .unwrap()
            .values()
            .filter_map(|source| {
                source.position.map(|position| {
                    EmitterSpatialState::new(source.emitter, Self::pose(position))
                        .with_extent(source.extent.clone())
                        .with_acoustic_priority(Self::acoustic_priority(source.volume_db))
                })
            })
            .collect();
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
        cadence.mark_published(sim_time_seconds);
        drop(cadence);

        for event in self.world.drain_events() {
            log::debug!("PetalSonic event: {event:?}");
        }
        let status = self.world.runtime_status();
        let diagnostics = self.world.diagnostics();
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
        {
            log::warn!(
                "PetalSonic pressure: underruns={}, rejected_commands={}, dropped_events={}, render_p99_us={}",
                diagnostics.underrun_count,
                diagnostics.rejected_commands,
                diagnostics.dropped_events,
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
        previous.acoustic_published_responses = diagnostics.acoustic_published_response_count;
        previous.acoustic_response_geometry_version =
            diagnostics.acoustic_response_geometry_version;
        Ok(())
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

    pub fn acoustic_superseded_solve_count(&self) -> u64 {
        self.world.diagnostics().acoustic_superseded_solve_count
    }

    pub fn drain_acoustic_telemetry(&self) -> Vec<SpatialAcousticTelemetryEvent> {
        let sources = self.uuid_to_source.lock().unwrap();
        self.world
            .drain_acoustic_telemetry()
            .into_iter()
            .filter_map(|event| match event {
                AcousticTelemetryEvent::ExtentResponse(response) => {
                    let source_uuid = sources.iter().find_map(|(&uuid, source)| {
                        (source.emitter == response.emitter).then_some(uuid)
                    });
                    Some(SpatialAcousticTelemetryEvent::ExtentResponse {
                        source_uuid,
                        response: *response,
                    })
                }
                AcousticTelemetryEvent::SolveDiscarded {
                    spatial_revision,
                    geometry_version,
                    reason,
                } => match reason {
                    AcousticDiscardReason::Superseded => {
                        Some(SpatialAcousticTelemetryEvent::SolveDiscarded {
                            spatial_revision,
                            geometry_version,
                        })
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    pub fn acoustic_telemetry_diagnostics(&self) -> AcousticTelemetryDiagnostics {
        self.world.acoustic_telemetry_diagnostics()
    }

    pub fn runtime_diagnostics(&self) -> RuntimeDiagnostics {
        self.world.diagnostics()
    }

    fn acoustic_priority(volume_db: f32) -> f32 {
        if !volume_db.is_finite() {
            return 0.0;
        }
        10.0_f32.powf(volume_db / 20.0).clamp(0.0, 16.0)
    }

    #[allow(dead_code)]
    pub fn update_source_pos(&self, source_uuid: Uuid, target_pos: Vec3) -> Result<()> {
        let mut sources = self.uuid_to_source.lock().unwrap();
        let Some(source) = sources.get_mut(&source_uuid) else {
            return Ok(());
        };
        source.position = Some(target_pos);
        drop(sources);
        self.mark_spatial_frame_dirty();
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
            Self::emitter_desc(
                source.position,
                source.volume_db,
                source.extent.clone(),
                source.occlusion_profile,
            ),
        )?;
        Ok(())
    }

    pub fn replace_looping_clip(
        &self,
        source_uuid: Uuid,
        clip: ResidentClip,
        initial_phase: f32,
    ) -> Result<()> {
        let mut sources = self.uuid_to_source.lock().unwrap();
        let Some(source) = sources.get_mut(&source_uuid) else {
            return Ok(());
        };
        let old_emitter = source.emitter;
        let new_emitter = self.world.create_emitter(
            clip,
            Self::emitter_desc(
                source.position,
                source.volume_db,
                source.extent.clone(),
                source.occlusion_profile,
            ),
        )?;
        if let Err(error) = self.world.play(new_emitter, PlayOptions::looping()) {
            let _ = self.world.destroy_emitter(new_emitter);
            return Err(error.into());
        }
        if let Err(error) = self
            .world
            .seek_emitter(new_emitter, initial_phase.clamp(0.0, 1.0))
        {
            let _ = self.world.destroy_emitter(new_emitter);
            return Err(error.into());
        }
        if let Err(error) = self.world.destroy_emitter(old_emitter) {
            let _ = self.world.destroy_emitter(new_emitter);
            return Err(error.into());
        }
        source.emitter = new_emitter;
        drop(sources);
        self.mark_spatial_frame_dirty();
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
        let source = self.uuid_to_source.lock().unwrap().remove(&id);
        if let Some(source) = source {
            let _ = self.world.destroy_emitter(source.emitter);
            self.mark_spatial_frame_dirty();
        }
    }

    fn mark_spatial_frame_dirty(&self) {
        self.spatial_frame_publish_cadence
            .lock()
            .unwrap()
            .mark_dirty();
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
    use super::{SpatialFramePublishCadence, SpatialSoundManager};
    use glam::Vec3;
    use petalsonic::{
        DistributedOcclusionProfile, ExtentSample, ExtentSampleId, OcclusionProfile, SourceExtent,
    };

    #[test]
    fn acoustic_priority_tracks_linear_source_gain_and_sanitizes_invalid_values() {
        assert!((SpatialSoundManager::acoustic_priority(0.0) - 1.0).abs() < f32::EPSILON);
        assert!((SpatialSoundManager::acoustic_priority(-20.0) - 0.1).abs() < f32::EPSILON);
        assert_eq!(SpatialSoundManager::acoustic_priority(f32::NAN), 0.0);
        assert_eq!(SpatialSoundManager::acoustic_priority(100.0), 16.0);
    }

    #[test]
    fn spatial_frame_cadence_coalesces_camera_frames_but_never_structural_changes() {
        let mut cadence = SpatialFramePublishCadence::default();

        assert!(cadence.should_publish(0.0));
        cadence.mark_published(0.0);
        assert!(!cadence.should_publish(0.01));

        cadence.mark_dirty();
        assert!(cadence.should_publish(0.011));
        cadence.mark_published(0.011);
        assert!(!cadence.should_publish(0.03));
        assert!(cadence.should_publish(0.045));
    }

    #[test]
    fn spatial_frame_cadence_recovers_from_non_monotonic_simulation_time() {
        let mut cadence = SpatialFramePublishCadence::default();
        cadence.mark_published(10.0);

        assert!(cadence.should_publish(1.0));
    }

    #[test]
    fn spatial_emitter_description_preserves_extent_and_occlusion_profile() {
        let extent = SourceExtent::weighted_samples(vec![ExtentSample::new(
            ExtentSampleId(17),
            petalsonic::Vec3::new(0.25, 0.5, -0.75),
            1.0,
        )
        .unwrap()])
        .unwrap();
        let profile = OcclusionProfile::AmbientDistributed(DistributedOcclusionProfile::default());

        let desc = SpatialSoundManager::spatial_emitter_desc(
            Vec3::new(1.0, 2.0, 3.0),
            -12.0,
            extent.clone(),
            profile,
        );

        assert_eq!(desc.extent(), &extent);
        assert_eq!(desc.occlusion_profile(), profile);
        assert_eq!(desc.gain_db(), -12.0);
    }
}
