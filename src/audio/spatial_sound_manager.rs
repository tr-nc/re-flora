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
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Instant;
use uuid::Uuid;

/// Source tracking information
struct SourceInfo {
    source_id: SourceId,
    volume: f32,
    position: Option<Vec3>,
    loop_mode: LoopMode,
}

trait DirectAudioRayTracerCallback {
    fn trace_any_hit_batch(
        &mut self,
        rays: &[AcousticRay],
        min_distances: &[f32],
        max_distances: &[f32],
    ) -> Result<Vec<bool>>;
}

impl<F> DirectAudioRayTracerCallback for F
where
    F: FnMut(&[AcousticRay], &[f32], &[f32]) -> Result<Vec<bool>>,
{
    fn trace_any_hit_batch(
        &mut self,
        rays: &[AcousticRay],
        min_distances: &[f32],
        max_distances: &[f32],
    ) -> Result<Vec<bool>> {
        self(rays, min_distances, max_distances)
    }
}

#[derive(Clone, Copy)]
struct ActiveAudioRayTracingCallback {
    data: NonNull<()>,
    stats: NonNull<AudioFillTraceStats>,
    trace_any_hit_batch: unsafe fn(*mut (), &[AcousticRay], &[f32], &[f32]) -> Result<Vec<bool>>,
}

impl ActiveAudioRayTracingCallback {
    unsafe fn trace_any_hit_batch(
        &mut self,
        rays: &[AcousticRay],
        min_distances: &[f32],
        max_distances: &[f32],
    ) -> Result<Vec<bool>> {
        unsafe { (self.trace_any_hit_batch)(self.data.as_ptr(), rays, min_distances, max_distances) }
    }
}

#[derive(Default)]
struct AudioFillTraceStats {
    total_time_us: u64,
    batch_count: usize,
    total_rays: usize,
    min_batch_time_us: u64,
    max_batch_time_us: u64,
}

impl AudioFillTraceStats {
    fn record_batch(&mut self, elapsed_us: u64, rays: usize) {
        if self.batch_count == 0 {
            self.min_batch_time_us = elapsed_us;
            self.max_batch_time_us = elapsed_us;
        } else {
            self.min_batch_time_us = self.min_batch_time_us.min(elapsed_us);
            self.max_batch_time_us = self.max_batch_time_us.max(elapsed_us);
        }

        self.total_time_us += elapsed_us;
        self.batch_count += 1;
        self.total_rays += rays;
    }

    fn avg_batch_time_us(&self) -> u64 {
        if self.batch_count == 0 {
            0
        } else {
            self.total_time_us / self.batch_count as u64
        }
    }

    fn avg_rays_per_batch(&self) -> f32 {
        if self.batch_count == 0 {
            0.0
        } else {
            self.total_rays as f32 / self.batch_count as f32
        }
    }
}

thread_local! {
    static ACTIVE_AUDIO_RT_CALLBACK: RefCell<Option<ActiveAudioRayTracingCallback>> = RefCell::new(None);
    static MISSING_AUDIO_RT_CALLBACK_WARNED: Cell<bool> = const { Cell::new(false) };
}

struct ActiveAudioRayTracingGuard {
    previous: Option<ActiveAudioRayTracingCallback>,
}

impl Drop for ActiveAudioRayTracingGuard {
    fn drop(&mut self) {
        ACTIVE_AUDIO_RT_CALLBACK.with(|slot| {
            slot.replace(self.previous.take());
        });
    }
}

fn install_active_audio_rt_callback<F>(
    callback: &mut F,
    stats: &mut AudioFillTraceStats,
) -> ActiveAudioRayTracingGuard
where
    F: DirectAudioRayTracerCallback,
{
    let callback = ActiveAudioRayTracingCallback {
        data: NonNull::from(callback).cast(),
        stats: NonNull::from(stats),
        trace_any_hit_batch: |data, rays, min_distances, max_distances| unsafe {
            (&mut *(data as *mut F)).trace_any_hit_batch(rays, min_distances, max_distances)
        },
    };
    let previous = ACTIVE_AUDIO_RT_CALLBACK.with(|slot| slot.replace(Some(callback)));
    ActiveAudioRayTracingGuard { previous }
}

#[derive(Default)]
struct AudioRayTracingRuntimeStats {
    callback_batches: AtomicUsize,
    callback_rays: AtomicUsize,
    traced_batches: AtomicUsize,
    traced_rays: AtomicUsize,
    traced_hits: AtomicUsize,
    callback_failures: AtomicUsize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AudioRayTracingRuntimeSnapshot {
    pub callback_batches: usize,
    pub callback_rays: usize,
    pub traced_batches: usize,
    pub traced_rays: usize,
    pub traced_hits: usize,
    pub callback_failures: usize,
}

struct AudioRayTracingLogger {
    runtime_stats: Arc<AudioRayTracingRuntimeStats>,
}

impl AudioRayTracingLogger {
    fn take_snapshot(&self) -> AudioRayTracingRuntimeSnapshot {
        AudioRayTracingRuntimeSnapshot {
            callback_batches: self.runtime_stats.callback_batches.swap(0, Ordering::Relaxed),
            callback_rays: self.runtime_stats.callback_rays.swap(0, Ordering::Relaxed),
            traced_batches: self.runtime_stats.traced_batches.swap(0, Ordering::Relaxed),
            traced_rays: self.runtime_stats.traced_rays.swap(0, Ordering::Relaxed),
            traced_hits: self.runtime_stats.traced_hits.swap(0, Ordering::Relaxed),
            callback_failures: self.runtime_stats.callback_failures.swap(0, Ordering::Relaxed),
        }
    }
}

struct AudioRayTracingBackend {
    enabled: Arc<AtomicBool>,
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

        ACTIVE_AUDIO_RT_CALLBACK.with(|slot| {
            let Some(mut callback) = *slot.borrow() else {
                MISSING_AUDIO_RT_CALLBACK_WARNED.with(|warned| {
                    if !warned.replace(true) {
                        log::warn!(
                            "Audio ray tracing callback invoked without an active direct tracer"
                        );
                    }
                });
                self.runtime_stats
                    .callback_failures
                    .fetch_add(1, Ordering::Relaxed);
                return vec![false; rays.len()];
            };

            let trace_start = Instant::now();
            let results = unsafe { callback.trace_any_hit_batch(rays, min_distances, max_distances) };
            let elapsed_us = trace_start.elapsed().as_micros() as u64;
            unsafe {
                callback
                    .stats
                    .as_mut()
                    .record_batch(elapsed_us, rays.len());
            }

            match results {
                Ok(results) if results.len() == rays.len() => {
                    let hit_count = results.iter().filter(|&&hit| hit).count();
                    self.runtime_stats
                        .traced_batches
                        .fetch_add(1, Ordering::Relaxed);
                    self.runtime_stats
                        .traced_rays
                        .fetch_add(rays.len(), Ordering::Relaxed);
                    self.runtime_stats
                        .traced_hits
                        .fetch_add(hit_count, Ordering::Relaxed);
                    results
                }
                Ok(results) => {
                    log::warn!(
                        "Audio ray tracing callback returned {} hits for {} rays",
                        results.len(),
                        rays.len()
                    );
                    self.runtime_stats
                        .callback_failures
                        .fetch_add(1, Ordering::Relaxed);
                    vec![false; rays.len()]
                }
                Err(err) => {
                    log::warn!("Direct audio ray tracing callback failed: {}", err);
                    self.runtime_stats
                        .callback_failures
                        .fetch_add(1, Ordering::Relaxed);
                    vec![false; rays.len()]
                }
            }
        })
    }
}

struct AudioRayTracingService {
    enabled: Arc<AtomicBool>,
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
        let audio_ray_tracing_enabled = Arc::new(AtomicBool::new(true));
        let audio_ray_tracing_runtime_stats = Arc::new(AudioRayTracingRuntimeStats::default());
        let audio_ray_tracing_logger = Arc::new(AudioRayTracingLogger {
            runtime_stats: audio_ray_tracing_runtime_stats.clone(),
        });
        let audio_ray_tracing_backend = Arc::new(AudioRayTracingBackend {
            enabled: audio_ray_tracing_enabled.clone(),
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
                enabled: audio_ray_tracing_enabled,
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
        let mut trace_stats = AudioFillTraceStats::default();
        let _active_rt_callback = install_active_audio_rt_callback(&mut trace_batch, &mut trace_stats);
        let pump_start = Instant::now();
        let mut engine = self.engine.lock().unwrap();
        engine
            .pump_audio()
            .map_err(|err| anyhow::anyhow!("Failed to pump audio: {}", err))?;
        let timing_events = engine.poll_timing_events();
        drop(engine);

        let wall_time_us = pump_start.elapsed().as_micros() as u64;
        let engine_batch_count = timing_events.len();
        let engine_total_time_us: u64 = timing_events.iter().map(|event| event.total_time_us).sum();
        let engine_spatial_time_us: u64 = timing_events
            .iter()
            .map(|event| event.spatial_time_us + event.spatial_simulation_time_us)
            .sum();
        let engine_mix_time_us: u64 = timing_events
            .iter()
            .map(|event| event.mixing_time_us + event.direct_mixing_time_us)
            .sum();
        let engine_resample_time_us: u64 =
            timing_events.iter().map(|event| event.resampling_time_us).sum();

        log::info!(
            "[audio-fill] wall={:.3}ms engine_batches={} engine_total={:.3}ms engine_mix={:.3}ms engine_spatial={:.3}ms engine_resample={:.3}ms trace_total={:.3}ms trace_batches={} trace_batch_ms(min/avg/max)={:.3}/{:.3}/{:.3} avg_rays_per_batch={:.2}",
            wall_time_us as f64 / 1000.0,
            engine_batch_count,
            engine_total_time_us as f64 / 1000.0,
            engine_mix_time_us as f64 / 1000.0,
            engine_spatial_time_us as f64 / 1000.0,
            engine_resample_time_us as f64 / 1000.0,
            trace_stats.total_time_us as f64 / 1000.0,
            trace_stats.batch_count,
            trace_stats.min_batch_time_us as f64 / 1000.0,
            trace_stats.avg_batch_time_us() as f64 / 1000.0,
            trace_stats.max_batch_time_us as f64 / 1000.0,
            trace_stats.avg_rays_per_batch(),
        );

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
                enabled: self.audio_ray_tracing_service.enabled.clone(),
                runtime_stats: self.audio_ray_tracing_service.runtime_stats.clone(),
            },
            audio_ray_tracing_logger: self.audio_ray_tracing_logger.clone(),
        }
    }
}
fn log_audio_timing_event(message: &str) {
    log::trace!("[audio-timing] {}", message);
}
