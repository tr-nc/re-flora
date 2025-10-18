use crate::gameplay::camera::vectors::CameraVectors;
use anyhow::Result;
use glam::Vec3;
use petalsonic::{
    audio_data::PetalSonicAudioData,
    config::PetalSonicWorldDesc,
    engine::PetalSonicEngine,
    math::{Pose, Quat as PetalQuat, Vec3 as PetalVec3},
    playback::LoopMode,
    world::PetalSonicWorld,
    SourceConfig, SourceId,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayMode {
    Loop,
    SinglePlay,
}

/// Source tracking information
struct SourceInfo {
    source_id: SourceId,
    volume: f32,
}

/// Spatial sound manager using PetalSonic
pub struct SpatialSoundManager {
    world: Arc<PetalSonicWorld>,
    engine: Arc<Mutex<PetalSonicEngine>>,

    // Map UUIDs to PetalSonic SourceIds and their metadata
    uuid_to_source: Arc<Mutex<HashMap<Uuid, SourceInfo>>>,

    // Cache listener state to avoid unnecessary updates
    listener_state: Arc<Mutex<ListenerState>>,
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
    pub fn new(ring_buffer_size: usize, frame_window_size: usize) -> Result<Self> {
        Self::with_distance_scaler(ring_buffer_size, frame_window_size, 10.0)
    }

    pub fn with_distance_scaler(
        _ring_buffer_size: usize,
        frame_window_size: usize,
        _distance_scaler: f32,
    ) -> Result<Self> {
        // Note: ring_buffer_size and distance_scaler are handled internally by PetalSonic
        let sample_rate = 48000;

        // Get HRTF path - use the same path structure as before
        let hrtf_path = format!(
            "{}assets/hrtf/hrtf_b_nh172.sofa",
            crate::util::get_project_root()
        );

        // Create PetalSonic world configuration
        let world_desc = PetalSonicWorldDesc {
            sample_rate,
            block_size: frame_window_size,
            hrtf_path: Some(hrtf_path),
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

        Ok(Self {
            world: world_arc,
            engine: Arc::new(Mutex::new(engine)),
            uuid_to_source: Arc::new(Mutex::new(HashMap::new())),
            listener_state: Arc::new(Mutex::new(ListenerState::default())),
        })
    }

    fn add_source(
        &self,
        path: &str,
        volume: f32,
        position: Vec3,
        loop_mode: LoopMode,
    ) -> Result<Uuid> {
        // Load audio data (from_path already returns Arc-wrapped data)
        let audio_data = PetalSonicAudioData::from_path(path)?;

        // Convert glam::Vec3 to PetalVec3 for PetalSonic API
        let petal_pos = PetalVec3::new(position.x, position.y, position.z);

        // Register in PetalSonic world with spatial configuration
        let source_id = self.world.register_audio(
            audio_data,
            SourceConfig::spatial_with_volume(petal_pos, volume),
        )?;

        // Start playback
        self.world.play(source_id, loop_mode)?;

        // Generate UUID and map to SourceId with metadata
        let uuid = Uuid::new_v4();
        self.uuid_to_source
            .lock()
            .unwrap()
            .insert(uuid, SourceInfo { source_id, volume });

        Ok(uuid)
    }

    pub fn add_tree_gust_source(&self, tree_pos: Vec3) -> Result<Uuid> {
        self.add_source(
            "assets/sfx/Tree Gusts/WINDGust_Wind, Gust in Trees 01_SARM_Wind.wav",
            1.0,
            tree_pos,
            LoopMode::Infinite,
        )
    }

    pub fn add_single_play_source(&self, path: &str, volume: f32, position: Vec3) -> Result<Uuid> {
        self.add_source(path, volume, position, LoopMode::Once)
    }

    pub fn add_loop_source(&self, path: &str, volume: f32, position: Vec3) -> Result<Uuid> {
        self.add_source(path, volume, position, LoopMode::Infinite)
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

        // Convert camera vectors to quaternion rotation
        let forward_glam: glam::Vec3 = camera_vectors.front;
        let rotation_glam = glam::Quat::from_rotation_arc(glam::Vec3::NEG_Z, forward_glam);

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

    pub fn update_source_pos(&self, source_uuid: Uuid, target_pos: Vec3) -> Result<()> {
        let uuid_map = self.uuid_to_source.lock().unwrap();

        if let Some(source_info) = uuid_map.get(&source_uuid) {
            // Convert position to PetalVec3
            let petal_pos = PetalVec3::new(target_pos.x, target_pos.y, target_pos.z);

            // Update the source configuration with new position, preserving volume
            self.world.update_source_config(
                source_info.source_id,
                SourceConfig::spatial_with_volume(petal_pos, source_info.volume),
            )?;

            log::debug!(
                "Updated source {:?} position to {:?}",
                source_uuid,
                target_pos
            );
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn remove_source(&self, id: Uuid) {
        if let Some(source_info) = self.uuid_to_source.lock().unwrap().remove(&id) {
            // Stop the source and remove from world
            let _ = self.world.stop(source_info.source_id);
            let _ = self.world.remove_audio_data(source_info.source_id);
        }
    }

    /// Poll events from the engine (e.g., for cleanup of completed sources)
    #[allow(dead_code)]
    pub fn poll_events(&self) -> Vec<petalsonic::PetalSonicEvent> {
        self.engine.lock().unwrap().poll_events()
    }
}

// Make SpatialSoundManager cloneable
impl Clone for SpatialSoundManager {
    fn clone(&self) -> Self {
        Self {
            world: self.world.clone(),
            engine: self.engine.clone(),
            uuid_to_source: self.uuid_to_source.clone(),
            listener_state: self.listener_state.clone(),
        }
    }
}

#[test]
fn testing() {}
