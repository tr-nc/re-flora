use crate::audio::audio_clip_cache::AudioClipCache;
use crate::builder::ContreeAnyHitRayTracer;
use crate::gameplay::camera::vectors::CameraVectors;
use anyhow::Result;
use glam::Vec3;
use petalsonic::{
    AcousticSceneSnapshot, BatchedAnyHitRayTracer, BatchedClosestHitRayTracer, BusParams, Emitter,
    EmitterDesc, EmitterSpatialState, LatencyProfile, OutputDevicePolicy, PetalSonicWorld,
    PetalSonicWorldDesc, PlayOptions, Pose, Quat as PetalQuat, ResidentClip, SpatialFrame,
    SpatialQuality, Vec3 as PetalVec3,
};
use rand::RngExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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

/// Re:Flora's narrow adapter around the world-owned PetalSonic runtime.
#[derive(Clone)]
pub struct SpatialSoundManager {
    world: Arc<PetalSonicWorld>,
    audio_ray_tracer: Arc<ContreeAnyHitRayTracer>,
    clip_cache: Arc<AudioClipCache>,
    uuid_to_source: Arc<Mutex<HashMap<Uuid, SourceInfo>>>,
    one_shot_emitters: Arc<Mutex<HashMap<String, OneShotEmitter>>>,
    listener_state: Arc<Mutex<ListenerState>>,
    global_volume_gain_db: Arc<Mutex<f32>>,
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
        audio_ray_tracer: Arc<ContreeAnyHitRayTracer>,
        audio_output_device: Option<String>,
    ) -> Result<Self> {
        let project_root = crate::util::get_project_root();
        let native_hrtf_path = format!("{}assets/hrtf/hrtf_b_nh172.petalhrtf", project_root);
        let steam_hrtf_path = format!("{}assets/hrtf/hrtf_b_nh172.sofa", project_root);
        let any_hit: Arc<dyn BatchedAnyHitRayTracer> = audio_ray_tracer.clone();
        let closest_hit: Arc<dyn BatchedClosestHitRayTracer> = audio_ray_tracer.clone();
        let acoustic_scene = AcousticSceneSnapshot::new(1, Some(any_hit), Some(closest_hit));

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
            steam_hrtf_path: Some(steam_hrtf_path),
            native_hrtf_path: Some(native_hrtf_path),
            hrtf_gain: 0.0,
            distance_scaler: 15.0,
            acoustic_scene: Some(acoustic_scene),
            ..PetalSonicWorldDesc::default()
        })?;

        Ok(Self {
            world: Arc::new(world),
            audio_ray_tracer,
            clip_cache: Arc::new(AudioClipCache::new()?),
            uuid_to_source: Arc::new(Mutex::new(HashMap::new())),
            one_shot_emitters: Arc::new(Mutex::new(HashMap::new())),
            listener_state: Arc::new(Mutex::new(ListenerState::default())),
            global_volume_gain_db: Arc::new(Mutex::new(0.0)),
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
    pub fn publish_spatial_frame(&self) -> Result<()> {
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
                source
                    .position
                    .map(|position| EmitterSpatialState::new(source.emitter, Self::pose(position)))
            })
            .collect();
        self.world
            .publish_spatial_frame(SpatialFrame::new(listener_pose, emitters))?;

        for event in self.world.drain_events() {
            log::debug!("PetalSonic event: {event:?}");
        }
        Ok(())
    }

    pub fn set_audio_ray_tracing_enabled(&self, enabled: bool) {
        self.audio_ray_tracer.set_enabled(enabled);
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
        self.world.close()?;
        Ok(())
    }
}
