use crate::audio::audio_clip_cache::AudioClipCache;
use crate::gameplay::camera::vectors::CameraVectors;
use anyhow::Result;
use glam::Vec3;
use petalsonic::{
    AcousticRay, BatchedAnyHitRayTracer,
    config::PetalSonicWorldDesc,
    engine::PetalSonicEngine,
    math::{Pose, Quat as PetalQuat, Vec3 as PetalVec3},
    playback::LoopMode,
    world::PetalSonicWorld,
    DirectOcclusionDebugSnapshot, SourceConfig, SourceId,
};
use rand::Rng;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// Coarser buckets reduce direct-occlusion cache churn from small listener motion.
const AUDIO_RAY_QUERY_QUANTIZATION: f32 = 5.0;
/// Source tracking information
struct SourceInfo {
    source_id: SourceId,
    volume: f32,
    position: Option<Vec3>,
    loop_mode: LoopMode,
}

#[derive(Clone)]
struct AudioRayTracingRequest {
    key: AudioRayQueryKey,
    ray: AcousticRay,
    min_distance: f32,
    max_distance: f32,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct AudioRayQueryKey {
    origin: [i32; 3],
    endpoint: [i32; 3],
    min_distance: i32,
}

impl Hash for AudioRayQueryKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.origin.hash(state);
        self.endpoint.hash(state);
        self.min_distance.hash(state);
    }
}

impl AudioRayQueryKey {
    fn from_ray(ray: AcousticRay, min_distance: f32, max_distance: f32) -> Self {
        let endpoint = petalsonic::math::Vec3::new(
            ray.origin.x + ray.direction.x * max_distance,
            ray.origin.y + ray.direction.y * max_distance,
            ray.origin.z + ray.direction.z * max_distance,
        );

        Self {
            origin: quantize_point(ray.origin),
            endpoint: quantize_point(endpoint),
            min_distance: quantize_scalar(min_distance),
        }
    }
}

#[derive(Default)]
struct AudioRayTracingQueryState {
    last_valid_results: HashMap<AudioRayQueryKey, bool>,
    pending_requests: HashMap<AudioRayQueryKey, AudioRayTracingRequest>,
}

#[derive(Default)]
struct AudioRayTracingRuntimeStats {
    callback_batches: AtomicUsize,
    callback_rays: AtomicUsize,
    reused_last_query_results: AtomicUsize,
    missing_last_query_results: AtomicUsize,
    queued_requests: AtomicUsize,
    serviced_batches: AtomicUsize,
    serviced_rays: AtomicUsize,
    serviced_hits: AtomicUsize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AudioRayTracingRuntimeSnapshot {
    pub callback_batches: usize,
    pub callback_rays: usize,
    pub reused_last_query_results: usize,
    pub missing_last_query_results: usize,
    pub queued_requests: usize,
    pub serviced_batches: usize,
    pub serviced_rays: usize,
    pub serviced_hits: usize,
}

struct AudioRayTracingLogger {
    runtime_stats: Arc<AudioRayTracingRuntimeStats>,
}

impl AudioRayTracingLogger {
    fn take_snapshot(&self) -> AudioRayTracingRuntimeSnapshot {
        AudioRayTracingRuntimeSnapshot {
            callback_batches: self.runtime_stats.callback_batches.swap(0, Ordering::Relaxed),
            callback_rays: self.runtime_stats.callback_rays.swap(0, Ordering::Relaxed),
            reused_last_query_results: self
                .runtime_stats
                .reused_last_query_results
                .swap(0, Ordering::Relaxed),
            missing_last_query_results: self
                .runtime_stats
                .missing_last_query_results
                .swap(0, Ordering::Relaxed),
            queued_requests: self.runtime_stats.queued_requests.swap(0, Ordering::Relaxed),
            serviced_batches: self.runtime_stats.serviced_batches.swap(0, Ordering::Relaxed),
            serviced_rays: self.runtime_stats.serviced_rays.swap(0, Ordering::Relaxed),
            serviced_hits: self.runtime_stats.serviced_hits.swap(0, Ordering::Relaxed),
        }
    }
}

struct AudioRayTracingBackend {
    request_sender: mpsc::Sender<AudioRayTracingRequest>,
    enabled: Arc<AtomicBool>,
    query_state: Arc<Mutex<AudioRayTracingQueryState>>,
    runtime_stats: Arc<AudioRayTracingRuntimeStats>,
}

impl BatchedAnyHitRayTracer for AudioRayTracingBackend {
    fn trace_any_hit_batch(
        &self,
        rays: &[AcousticRay],
        min_distances: &[f32],
        max_distances: &[f32],
    ) -> Vec<bool> {
        if !self.enabled.load(Ordering::Relaxed) {
            return vec![false; rays.len()];
        }

        log_audio_timing_event(&format!(
            "PetalSonic batch ray tracing callback invoked ({} rays)",
            rays.len()
        ));

        self.runtime_stats
            .callback_batches
            .fetch_add(1, Ordering::Relaxed);
        self.runtime_stats
            .callback_rays
            .fetch_add(rays.len(), Ordering::Relaxed);

        let mut query_state = self.query_state.lock().unwrap();
        let mut results = Vec::with_capacity(rays.len());
        let mut pending_requests = Vec::new();

        for ((ray, &min_distance), &max_distance) in rays
            .iter()
            .zip(min_distances.iter())
            .zip(max_distances.iter())
        {
            let key = AudioRayQueryKey::from_ray(*ray, min_distance, max_distance);
            if let Some(&cached) = query_state.last_valid_results.get(&key) {
                self.runtime_stats
                    .reused_last_query_results
                    .fetch_add(1, Ordering::Relaxed);
                results.push(cached);
                continue;
            }

            self.runtime_stats
                .missing_last_query_results
                .fetch_add(1, Ordering::Relaxed);
            results.push(true);

            if let std::collections::hash_map::Entry::Vacant(entry) = query_state.pending_requests.entry(key) {
                pending_requests.push(AudioRayTracingRequest {
                    key,
                    ray: *ray,
                    min_distance,
                    max_distance,
                });
                entry.insert(AudioRayTracingRequest {
                    key,
                    ray: *ray,
                    min_distance,
                    max_distance,
                });
            }
        }
        drop(query_state);

        self.runtime_stats
            .queued_requests
            .fetch_add(pending_requests.len(), Ordering::Relaxed);
        for request in pending_requests {
            let _ = self.request_sender.send(request);
        }

        results
    }
}

struct AudioRayTracingService {
    request_receiver: Arc<Mutex<mpsc::Receiver<AudioRayTracingRequest>>>,
    enabled: Arc<AtomicBool>,
    query_state: Arc<Mutex<AudioRayTracingQueryState>>,
    runtime_stats: Arc<AudioRayTracingRuntimeStats>,
}

/// Spatial sound manager using PetalSonic
pub struct SpatialSoundManager {
    world: Arc<PetalSonicWorld>,
    engine: Arc<Mutex<PetalSonicEngine>>,

    // Audio clip cache for efficient audio data loading
    clip_cache: Arc<AudioClipCache>,

    // Map UUIDs to PetalSonic SourceIds and their metadata
    uuid_to_source: Arc<Mutex<HashMap<Uuid, SourceInfo>>>,

    // Cache listener state to avoid unnecessary updates
    listener_state: Arc<Mutex<ListenerState>>,

    // Global master gain (dB) applied to all sources.
    global_volume_gain_db: Arc<Mutex<f32>>,

    audio_ray_tracing_service: AudioRayTracingService,
    audio_ray_tracing_logger: Arc<AudioRayTracingLogger>,
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
    pub fn new(frame_window_size: usize) -> Result<Self> {
        let sample_rate = 48000;

        // Initialize audio clip cache first
        let clip_cache = Arc::new(AudioClipCache::new()?);

        // Get HRTF path - use the same path structure as before
        let hrtf_path = format!(
            "{}assets/hrtf/hrtf_b_nh172.sofa",
            crate::util::get_project_root()
        );

        // Create PetalSonic world configuration
        let (request_sender, request_receiver) = mpsc::channel();
        let audio_ray_tracing_enabled = Arc::new(AtomicBool::new(true));
        let audio_ray_tracing_query_state = Arc::new(Mutex::new(AudioRayTracingQueryState::default()));
        let audio_ray_tracing_runtime_stats = Arc::new(AudioRayTracingRuntimeStats::default());
        let audio_ray_tracing_logger = Arc::new(AudioRayTracingLogger {
            runtime_stats: audio_ray_tracing_runtime_stats.clone(),
        });
        let audio_ray_tracing_backend = Arc::new(AudioRayTracingBackend {
            request_sender,
            enabled: audio_ray_tracing_enabled.clone(),
            query_state: audio_ray_tracing_query_state.clone(),
            runtime_stats: audio_ray_tracing_runtime_stats.clone(),
        });

        let world_desc = PetalSonicWorldDesc {
            sample_rate,
            block_size: frame_window_size,
            hrtf_path: Some(hrtf_path),
            hrtf_gain: 0.0,
            distance_scaler: 15.0,
            batched_any_hit_ray_tracer: Some(audio_ray_tracing_backend),
            ..Default::default()
        };

        // Create world and engine
        let world = PetalSonicWorld::new(world_desc.clone())?;
        let world_arc = Arc::new(world);
        let mut engine = PetalSonicEngine::new(world_desc, world_arc.clone())?;

        // Start the engine
        engine.start()?;

        // Initialize with default listener position and orientation
        let listener_pose = Pose::new(PetalVec3::new(0.0, 0.0, 0.0), PetalQuat::IDENTITY);
        world_arc.set_listener_pose(listener_pose);

        #[allow(clippy::arc_with_non_send_sync)]
        Ok(Self {
            world: world_arc,
            engine: Arc::new(Mutex::new(engine)),
            clip_cache,
            uuid_to_source: Arc::new(Mutex::new(HashMap::new())),
            listener_state: Arc::new(Mutex::new(ListenerState::default())),
            global_volume_gain_db: Arc::new(Mutex::new(0.0)),
            audio_ray_tracing_service: AudioRayTracingService {
                request_receiver: Arc::new(Mutex::new(request_receiver)),
                enabled: audio_ray_tracing_enabled,
                query_state: audio_ray_tracing_query_state,
                runtime_stats: audio_ray_tracing_runtime_stats,
            },
            audio_ray_tracing_logger,
        })
    }

    fn global_gain_db(&self) -> f32 {
        *self.global_volume_gain_db.lock().unwrap()
    }

    fn add_source(
        &self,
        path: &str,
        volume: f32,
        position: Vec3,
        loop_mode: LoopMode,
    ) -> Result<Uuid> {
        // Get audio data from cache instead of loading from disk
        let audio_data = self
            .clip_cache
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Audio clip not found in cache: {}", path))?;

        // Convert glam::Vec3 to PetalVec3 for PetalSonic API
        let petal_pose = Pose::new(
            PetalVec3::new(position.x, position.y, position.z),
            PetalQuat::IDENTITY,
        );

        let effective_volume_db = volume + self.global_gain_db();

        // Register in PetalSonic world with spatial configuration
        let source_id = self.world.register_audio(
            audio_data,
            SourceConfig::spatial_with_volume_db(petal_pose, effective_volume_db),
        )?;

        // Start playback
        self.world.play(source_id, loop_mode)?;

        // Generate UUID and map to SourceId with metadata
        let uuid = Uuid::new_v4();
        self.uuid_to_source.lock().unwrap().insert(
            uuid,
            SourceInfo {
                source_id,
                volume,
                position: Some(position),
                loop_mode,
            },
        );

        Ok(uuid)
    }

    /// Add a looping spatial source at the given position.
    ///
    /// If `shuffle_phase` is true, the source starts at a random phase in
    /// the clip. This is useful when spawning many identical looping sounds
    /// (e.g. ambience beds) to avoid them being perfectly phase-aligned.
    pub fn add_looping_spatial_source(
        &self,
        path: &str,
        volume_db: f32,
        position: Vec3,
        shuffle_phase: bool,
    ) -> Result<Uuid> {
        let uuid = self.add_source(path, volume_db, position, LoopMode::Infinite)?;

        // Apply random phase offset if shuffle_phase is enabled
        if shuffle_phase {
            let uuid_map = self.uuid_to_source.lock().unwrap();
            if let Some(source_info) = uuid_map.get(&uuid) {
                let random_phase = rand::rng().random_range(0.0..1.0);
                self.world
                    .seek(source_info.source_id, random_phase)
                    .map_err(|e| anyhow::anyhow!("Failed to seek to random phase: {}", e))?;
            }
        }

        Ok(uuid)
    }

    /// Add a one-shot spatial source at the given position.
    #[allow(dead_code)]
    pub fn add_spatial_source(&self, path: &str, volume_db: f32, position: Vec3) -> Result<Uuid> {
        self.add_source(path, volume_db, position, LoopMode::Once)
    }

    /// Compute a volume (in dB) for a clustered source.
    ///
    /// Uses a sublinear scaling so that many clustered emitters do not
    /// increase volume too aggressively. The effective amplitude grows
    /// ~sqrt(n), which in dB corresponds to +10 * log10(n).
    #[allow(dead_code)]
    fn clustered_volume_db(base_volume_db: f32, clustered_amount: u32) -> f32 {
        let n = clustered_amount.max(1) as f32;
        if n <= 1.0 {
            return base_volume_db;
        }

        // amplitude ~ n^0.5 → gain_db = 10 * log10(n)
        let gain_db = 10.0 * n.log10();
        base_volume_db + gain_db
    }

    /// Add a non-spatial audio source (e.g., for UI sounds or player footsteps)
    pub fn add_non_spatial_source(&self, path: &str, volume: f32) -> Result<Uuid> {
        // Get audio data from cache instead of loading from disk
        let audio_data = self
            .clip_cache
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Audio clip not found in cache: {}", path))?;

        let effective_volume_db = volume + self.global_gain_db();

        // Register in PetalSonic world with non-spatial configuration and volume
        let source_id = self.world.register_audio(
            audio_data,
            SourceConfig::non_spatial_with_volume_db(effective_volume_db),
        )?;

        // Start playback with one-shot mode
        self.world.play(source_id, LoopMode::Once)?;

        // Generate UUID and map to SourceId with metadata
        let uuid = Uuid::new_v4();
        self.uuid_to_source.lock().unwrap().insert(
            uuid,
            SourceInfo {
                source_id,
                volume,
                position: None,
                loop_mode: LoopMode::Once,
            },
        );

        Ok(uuid)
    }

    /// Add a looping non-spatial audio source (e.g., for continuous UI/interaction sounds)
    #[allow(dead_code)]
    pub fn add_looping_non_spatial_source(
        &self,
        path: &str,
        volume: f32,
        shuffle_phase: bool,
    ) -> Result<Uuid> {
        let audio_data = self
            .clip_cache
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Audio clip not found in cache: {}", path))?;

        let effective_volume_db = volume + self.global_gain_db();

        let source_id = self.world.register_audio(
            audio_data,
            SourceConfig::non_spatial_with_volume_db(effective_volume_db),
        )?;

        self.world.play(source_id, LoopMode::Infinite)?;

        if shuffle_phase {
            let random_phase = rand::rng().random_range(0.0..1.0);
            self.world
                .seek(source_id, random_phase)
                .map_err(|e| anyhow::anyhow!("Failed to seek to random phase: {}", e))?;
        }

        let uuid = Uuid::new_v4();
        self.uuid_to_source.lock().unwrap().insert(
            uuid,
            SourceInfo {
                source_id,
                volume,
                position: None,
                loop_mode: LoopMode::Infinite,
            },
        );

        Ok(uuid)
    }

    pub fn update_player_pos(
        &self,
        player_pos: Vec3,
        camera_vectors: &CameraVectors,
    ) -> Result<()> {
        let mut listener_state = self.listener_state.lock().unwrap();

        // Check if anything changed
        if listener_state.position == player_pos
            && listener_state.up == camera_vectors.up
            && listener_state.front == camera_vectors.front
            && listener_state.right == camera_vectors.right
        {
            return Ok(());
        }

        // Update cached state
        listener_state.position = player_pos;
        listener_state.up = camera_vectors.up;
        listener_state.front = camera_vectors.front;
        listener_state.right = camera_vectors.right;

        // Convert camera vectors to quaternion rotation using the full camera basis
        // Build a rotation matrix from the camera's right, up, and front vectors
        // glam uses right-handed coordinates where +X=right, +Y=up, +Z=backward (so -Z=forward)
        let rotation_matrix = glam::Mat3::from_cols(
            camera_vectors.right,
            camera_vectors.up,
            -camera_vectors.front, // Negate because glam's +Z points backward
        );
        let rotation_glam = glam::Quat::from_mat3(&rotation_matrix);

        // Convert to PetalQuat
        let rotation = PetalQuat::from_xyzw(
            rotation_glam.x,
            rotation_glam.y,
            rotation_glam.z,
            rotation_glam.w,
        );

        // Convert position to PetalVec3
        let petal_pos = PetalVec3::new(player_pos.x, player_pos.y, player_pos.z);

        // Update listener pose in PetalSonic
        let pose = Pose::new(petal_pos, rotation);
        self.world.set_listener_pose(pose);

        Ok(())
    }

    pub fn set_audio_ray_tracing_enabled(&self, enabled: bool) {
        self.audio_ray_tracing_service
            .enabled
            .store(enabled, Ordering::Relaxed);
    }

    pub fn pump_audio<F>(&self, mut trace_batch: F) -> Result<()>
    where
        F: FnMut(&[AcousticRay], &[f32], &[f32]) -> Result<Vec<bool>>,
    {
        const MAX_RT_SYNC_PASSES: usize = 3;

        for _ in 0..MAX_RT_SYNC_PASSES {
            {
                let mut engine = self.engine.lock().unwrap();
                engine
                    .pump_audio()
                    .map_err(|err| anyhow::anyhow!("Failed to pump audio: {}", err))?;
            }

            if !self.service_audio_ray_tracing_requests(&mut trace_batch)? {
                break;
            }
        }

        Ok(())
    }

    pub fn direct_occlusion_debug_snapshot(&self) -> Option<DirectOcclusionDebugSnapshot> {
        self.engine
            .lock()
            .ok()
            .and_then(|engine| engine.direct_occlusion_debug_snapshot())
    }

    pub fn take_audio_ray_tracing_runtime_snapshot(&self) -> AudioRayTracingRuntimeSnapshot {
        self.audio_ray_tracing_logger.take_snapshot()
    }

    pub fn service_audio_ray_tracing_requests<F>(
        &self,
        mut trace_batch: F,
    ) -> Result<bool>
    where
        F: FnMut(&[AcousticRay], &[f32], &[f32]) -> Result<Vec<bool>>,
    {
        let request_receiver = self.audio_ray_tracing_service.request_receiver.lock().unwrap();
        let mut requests = Vec::new();
        while let Ok(request) = request_receiver.try_recv() {
            requests.push(request);
        }
        drop(request_receiver);

        if requests.is_empty() {
            return Ok(false);
        }

        let rays = requests.iter().map(|request| request.ray).collect::<Vec<_>>();
        let min_distances = requests
            .iter()
            .map(|request| request.min_distance)
            .collect::<Vec<_>>();
        let max_distances = requests
            .iter()
            .map(|request| request.max_distance)
            .collect::<Vec<_>>();

        let rays_processed = requests.len();

        let response = match trace_batch(&rays, &min_distances, &max_distances) {
            Ok(hits) if hits.len() == rays_processed => hits,
            Ok(hits) => {
                log::warn!(
                    "Audio ray tracing backend returned {} hits for {} rays",
                    hits.len(),
                    rays_processed
                );
                vec![false; rays_processed]
            }
            Err(err) => {
                log::warn!("Failed to service audio ray tracing batch: {}", err);
                vec![false; rays_processed]
            }
        };

        let mut query_state = self.audio_ray_tracing_service.query_state.lock().unwrap();
        let mut hit_count = 0usize;
        for (request, result) in requests.into_iter().zip(response.into_iter()) {
            query_state.pending_requests.remove(&request.key);
            if result {
                hit_count += 1;
            }
            query_state.last_valid_results.insert(request.key, result);
        }

        self.audio_ray_tracing_service
            .runtime_stats
            .serviced_batches
            .fetch_add(1, Ordering::Relaxed);
        self.audio_ray_tracing_service
            .runtime_stats
            .serviced_rays
            .fetch_add(rays_processed, Ordering::Relaxed);
        self.audio_ray_tracing_service
            .runtime_stats
            .serviced_hits
            .fetch_add(hit_count, Ordering::Relaxed);

        Ok(true)
    }

    #[allow(dead_code)]
    pub fn update_source_pos(&self, source_uuid: Uuid, target_pos: Vec3) -> Result<()> {
        let global_gain_db = self.global_gain_db();
        let (source_id, volume) = {
            let mut uuid_map = self.uuid_to_source.lock().unwrap();
            if let Some(source_info) = uuid_map.get_mut(&source_uuid) {
                source_info.position = Some(target_pos);
                (source_info.source_id, source_info.volume)
            } else {
                return Ok(());
            }
        };

        // Convert position to PetalVec3
        let petal_pose = Pose::new(
            PetalVec3::new(target_pos.x, target_pos.y, target_pos.z),
            PetalQuat::IDENTITY,
        );

        // Update the source configuration with new position, preserving volume
        self.world.update_source_config(
            source_id,
            SourceConfig::spatial_with_volume_db(petal_pose, volume + global_gain_db),
        )?;

        Ok(())
    }

    pub fn update_source_volume(&self, source_uuid: Uuid, volume_db: f32) -> Result<()> {
        let global_gain_db = self.global_gain_db();
        let (source_id, position_opt) = {
            let mut uuid_map = self.uuid_to_source.lock().unwrap();
            if let Some(source_info) = uuid_map.get_mut(&source_uuid) {
                source_info.volume = volume_db;
                (source_info.source_id, source_info.position)
            } else {
                return Ok(());
            }
        };

        if let Some(position) = position_opt {
            let petal_pose = Pose::new(
                PetalVec3::new(position.x, position.y, position.z),
                PetalQuat::IDENTITY,
            );
            self.world.update_source_config(
                source_id,
                SourceConfig::spatial_with_volume_db(petal_pose, volume_db + global_gain_db),
            )?;
        } else {
            self.world.update_source_config(
                source_id,
                SourceConfig::non_spatial_with_volume_db(volume_db + global_gain_db),
            )?;
        }

        Ok(())
    }

    pub fn set_global_volume_gain_db(&self, gain_db: f32) -> Result<()> {
        {
            let mut global_gain = self.global_volume_gain_db.lock().unwrap();
            if (*global_gain - gain_db).abs() < f32::EPSILON {
                return Ok(());
            }
            *global_gain = gain_db;
        }

        let sources_to_update: Vec<(SourceId, Option<Vec3>, f32)> = self
            .uuid_to_source
            .lock()
            .unwrap()
            .values()
            .map(|source_info| {
                (
                    source_info.source_id,
                    source_info.position,
                    source_info.volume,
                )
            })
            .collect();

        for (source_id, position_opt, base_volume_db) in sources_to_update {
            let effective_volume_db = base_volume_db + gain_db;
            if let Some(position) = position_opt {
                let pose = Pose::new(
                    PetalVec3::new(position.x, position.y, position.z),
                    PetalQuat::IDENTITY,
                );
                self.world.update_source_config(
                    source_id,
                    SourceConfig::spatial_with_volume_db(pose, effective_volume_db),
                )?;
            } else {
                self.world.update_source_config(
                    source_id,
                    SourceConfig::non_spatial_with_volume_db(effective_volume_db),
                )?;
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn remove_source(&self, id: Uuid) {
        if let Some(source_info) = self.uuid_to_source.lock().unwrap().remove(&id) {
            // One-shot sources are often already cleaned up by the mixer.
            // Avoid issuing Stop for them to prevent noisy "not in active playback" warnings.
            if matches!(source_info.loop_mode, LoopMode::Infinite) {
                let _ = self.world.stop(source_info.source_id);
            }
            let _ = self.world.remove_audio_data(source_info.source_id);
        }
    }

}

// Make SpatialSoundManager cloneable
impl Clone for SpatialSoundManager {
    fn clone(&self) -> Self {
        Self {
            world: self.world.clone(),
            engine: self.engine.clone(),
            clip_cache: self.clip_cache.clone(),
            uuid_to_source: self.uuid_to_source.clone(),
            listener_state: self.listener_state.clone(),
            global_volume_gain_db: self.global_volume_gain_db.clone(),
            audio_ray_tracing_service: AudioRayTracingService {
                request_receiver: self.audio_ray_tracing_service.request_receiver.clone(),
                enabled: self.audio_ray_tracing_service.enabled.clone(),
                query_state: self.audio_ray_tracing_service.query_state.clone(),
                runtime_stats: self.audio_ray_tracing_service.runtime_stats.clone(),
            },
            audio_ray_tracing_logger: self.audio_ray_tracing_logger.clone(),
        }
    }
}

fn quantize_scalar(value: f32) -> i32 {
    (value * AUDIO_RAY_QUERY_QUANTIZATION).round() as i32
}

fn quantize_point(value: petalsonic::math::Vec3) -> [i32; 3] {
    [
        quantize_scalar(value.x),
        quantize_scalar(value.y),
        quantize_scalar(value.z),
    ]
}

fn log_audio_timing_event(message: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    println!(
        "[audio-timing] {} at {}.{:03}",
        message,
        timestamp.as_secs(),
        timestamp.subsec_millis()
    );
}
