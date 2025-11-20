use crate::audio::cluster_positions;
use crate::audio::SpatialSoundManager;
use anyhow::Result;
use glam::Vec3;
use log::{debug, warn};
use std::collections::HashMap;
use uuid::Uuid;

const TREE_LOOP_PATH: &str = "assets/sfx/tree_sound_48k.wav";
const DEFAULT_BASE_VOLUME_DB: f32 = -16.0;

/// Metadata tracked for each managed tree audio source.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ManagedTreeAudioSource {
    pub uuid: Uuid,
    pub tree_id: u32,
    pub position: Vec3,
    pub cluster_size: u32,
}

/// Keeps track of all looping tree ambience sources so we can later
/// drive them with wind simulations, recluster them, etc.
pub struct TreeAudioManager {
    spatial_sound_manager: SpatialSoundManager,
    base_volume_db: f32,
    sources_by_tree: HashMap<u32, Vec<Uuid>>,
    sources: HashMap<Uuid, ManagedTreeAudioSource>,
}

impl TreeAudioManager {
    pub fn new(spatial_sound_manager: SpatialSoundManager) -> Self {
        Self {
            spatial_sound_manager,
            base_volume_db: DEFAULT_BASE_VOLUME_DB,
            sources_by_tree: HashMap::new(),
            sources: HashMap::new(),
        }
    }

    /// Add audio emitters for the given tree and store their metadata.
    ///
    /// If `per_tree_audio` is true or `leaf_positions` is empty, a single
    /// source is spawned at `tree_position`. Otherwise, the leaf positions
    /// are clustered and one emitter is spawned per cluster.
    pub fn add_tree_sources(
        &mut self,
        tree_id: u32,
        tree_position: Vec3,
        leaf_positions: &[Vec3],
        per_tree_audio: bool,
        cluster_distance: f32,
        shuffle_phase: bool,
    ) -> Result<Vec<Uuid>> {
        // Remove any existing emitters for this tree before spawning new ones.
        self.remove_tree(tree_id);

        if per_tree_audio || leaf_positions.is_empty() {
            let mut created = Vec::new();
            match self.spawn_looping_source(tree_id, tree_position, 1, shuffle_phase) {
                Ok(uuid) => created.push(uuid),
                Err(err) => {
                    warn!(
                        "Failed to spawn per-tree audio for tree {} at {:?}: {}",
                        tree_id, tree_position, err
                    );
                }
            }
            return Ok(created);
        }

        let clusters = cluster_positions(leaf_positions, cluster_distance);
        let input_count = leaf_positions.len();
        let output_count = clusters.len();

        if input_count > 0 && output_count > 0 {
            let compression = input_count as f32 / output_count as f32;
            debug!(
                "Tree {} audio clustering at {:?}: inputs={} clusters={} compression={:.2}x",
                tree_id, tree_position, input_count, output_count, compression
            );
        }

        let mut created = Vec::with_capacity(output_count.max(1));
        if clusters.is_empty() {
            match self.spawn_looping_source(tree_id, tree_position, 1, shuffle_phase) {
                Ok(uuid) => created.push(uuid),
                Err(err) => {
                    warn!(
                        "Failed to spawn fallback tree audio for tree {} at {:?}: {}",
                        tree_id, tree_position, err
                    );
                }
            }
            return Ok(created);
        }

        for cluster in clusters {
            match self.spawn_looping_source(
                tree_id,
                cluster.pos,
                cluster.items_count,
                shuffle_phase,
            ) {
                Ok(uuid) => created.push(uuid),
                Err(err) => {
                    warn!(
                        "Failed to spawn clustered tree audio for tree {} at {:?}: {}",
                        tree_id, cluster.pos, err
                    );
                }
            }
        }

        Ok(created)
    }

    /// Remove all emitters that belong to the provided tree ID.
    pub fn remove_tree(&mut self, tree_id: u32) {
        if let Some(uuids) = self.sources_by_tree.remove(&tree_id) {
            for uuid in uuids {
                self.sources.remove(&uuid);
                self.spatial_sound_manager.remove_source(uuid);
            }
        }
    }

    /// Remove every registered tree emitter.
    pub fn remove_all(&mut self) {
        let tree_ids: Vec<u32> = self.sources_by_tree.keys().copied().collect();
        for tree_id in tree_ids {
            self.remove_tree(tree_id);
        }
        self.sources.clear();
    }

    /// Iterate all tracked sources.
    #[allow(dead_code)]
    pub fn sources(&self) -> impl Iterator<Item = &ManagedTreeAudioSource> {
        self.sources.values()
    }

    /// Fetch metadata for a specific source.
    #[allow(dead_code)]
    pub fn source(&self, uuid: &Uuid) -> Option<&ManagedTreeAudioSource> {
        self.sources.get(uuid)
    }

    fn spawn_looping_source(
        &mut self,
        tree_id: u32,
        position: Vec3,
        cluster_size: u32,
        shuffle_phase: bool,
    ) -> Result<Uuid> {
        let volume_db = Self::clustered_volume_db(self.base_volume_db, cluster_size);
        let uuid = self.spatial_sound_manager.add_looping_spatial_source(
            TREE_LOOP_PATH,
            volume_db,
            position,
            shuffle_phase,
        )?;

        self.register_source(tree_id, uuid, position, cluster_size);
        Ok(uuid)
    }

    fn register_source(&mut self, tree_id: u32, uuid: Uuid, position: Vec3, cluster_size: u32) {
        let entry = ManagedTreeAudioSource {
            uuid,
            tree_id,
            position,
            cluster_size,
        };
        self.sources_by_tree.entry(tree_id).or_default().push(uuid);
        self.sources.insert(uuid, entry);
    }

    fn clustered_volume_db(base_volume_db: f32, clustered_amount: u32) -> f32 {
        let n = clustered_amount.max(1) as f32;
        if n <= 1.0 {
            return base_volume_db;
        }

        let gain_db = 10.0 * n.log10();
        base_volume_db + gain_db
    }
}
