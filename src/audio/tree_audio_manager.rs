use crate::audio::{SpatialSoundManager, TreeAudioSource, TreeRustleControl, TreeRustleFactory};
use crate::util::{cluster_positions, ClusterResult};
use crate::wind::{Wind, WindResponseCurve, WindSource};
use anyhow::Result;
use glam::Vec3;
use log::{debug, warn};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

const TREE_SILENT_VOLUME_DB: f32 = -80.0;
const MAX_TREE_AUDIO_CLUSTER_SOURCES: usize = 8;

/// Keeps track of all looping tree ambience sources so we can later
/// drive them with wind simulations, recluster them, etc.
pub struct TreeAudioManager {
    spatial_sound_manager: SpatialSoundManager,
    wind_volume_db: f32,
    wind_response_curve: WindResponseCurve,
    sources_by_tree: HashMap<u32, Vec<Uuid>>,
    sources: HashMap<Uuid, TreeAudioSource>,
    wind: Wind,
}

impl TreeAudioManager {
    pub fn new(
        spatial_sound_manager: SpatialSoundManager,
        wind_response_curve: WindResponseCurve,
        wind_volume_db: f32,
    ) -> Self {
        Self {
            spatial_sound_manager,
            wind_volume_db,
            wind_response_curve,
            sources_by_tree: HashMap::new(),
            sources: HashMap::new(),
            wind: Wind::new(),
        }
    }

    /// Add audio emitters for the given tree and store their metadata.
    ///
    /// If `per_tree_audio` is true or `leaf_positions` is empty, a single
    /// source is spawned at `tree_position`. Otherwise, the leaf positions
    /// are clustered and one emitter is spawned per cluster.
    ///
    /// Note: Consider using `add_tree_sources_from_clusters()` if you already have
    /// pre-computed clusters to avoid duplicate clustering computation.
    #[allow(dead_code)]
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

        let selected_clusters = Self::select_audio_clusters(&clusters);
        let mut created = Vec::with_capacity(selected_clusters.len().max(1));
        if selected_clusters.is_empty() {
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

        for cluster in selected_clusters {
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

    /// Add audio emitters for the given tree using pre-computed clusters.
    ///
    /// If `per_tree_audio` is true or `clusters` is empty, a single
    /// source is spawned at `tree_position`. Otherwise, one emitter is spawned per cluster.
    pub fn add_tree_sources_from_clusters(
        &mut self,
        tree_id: u32,
        tree_position: Vec3,
        clusters: &[ClusterResult],
        per_tree_audio: bool,
        shuffle_phase: bool,
    ) -> Result<Vec<Uuid>> {
        // Remove any existing emitters for this tree before spawning new ones.
        self.remove_tree(tree_id);

        if per_tree_audio || clusters.is_empty() {
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

        let selected_clusters = Self::select_audio_clusters(clusters);
        let mut created = Vec::with_capacity(selected_clusters.len());

        for cluster in selected_clusters {
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
    pub fn sources(&self) -> impl Iterator<Item = &TreeAudioSource> {
        self.sources.values()
    }

    /// Fetch metadata for a specific source.
    #[allow(dead_code)]
    pub fn source(&self, uuid: &Uuid) -> Option<&TreeAudioSource> {
        self.sources.get(uuid)
    }

    pub fn set_wind_volume_db(&mut self, wind_volume_db: f32) -> Result<()> {
        if (wind_volume_db - self.wind_volume_db).abs() <= f32::EPSILON {
            return Ok(());
        }

        self.wind_volume_db = wind_volume_db;
        for source in self.sources.values_mut() {
            source.set_wind_volume_db(
                Self::clustered_volume_db(self.wind_volume_db, source.cluster_size),
                &self.spatial_sound_manager,
            )?;
        }
        Ok(())
    }

    pub fn set_wind_response_curve(&mut self, wind_response_curve: WindResponseCurve) {
        if wind_response_curve == self.wind_response_curve {
            return;
        }

        self.wind_response_curve = wind_response_curve;
        for source in self.sources.values_mut() {
            source.set_wind_response_curve(self.wind_response_curve);
        }
    }

    pub fn update(
        &mut self,
        time_seconds: f32,
        wind_sources: &[WindSource],
        wind_audio_attack_decay: f32,
        wind_audio_release_decay: f32,
    ) -> Result<()> {
        for source in self.sources.values_mut() {
            source.update(
                &self.wind,
                time_seconds,
                wind_sources,
                wind_audio_attack_decay,
                wind_audio_release_decay,
                &self.spatial_sound_manager,
            )?;
        }
        Ok(())
    }

    fn select_audio_clusters(clusters: &[ClusterResult]) -> Vec<ClusterResult> {
        if clusters.len() <= MAX_TREE_AUDIO_CLUSTER_SOURCES {
            return clusters.to_vec();
        }

        let mut selected = clusters.to_vec();
        selected.sort_by(|a, b| b.items_count.cmp(&a.items_count));
        selected.truncate(MAX_TREE_AUDIO_CLUSTER_SOURCES);
        selected
    }

    fn spawn_looping_source(
        &mut self,
        tree_id: u32,
        position: Vec3,
        cluster_size: u32,
        _shuffle_phase: bool,
    ) -> Result<Uuid> {
        let wind_volume_db = Self::clustered_volume_db(self.wind_volume_db, cluster_size);
        let rustle_control = Arc::new(TreeRustleControl::new());
        let rustle_factory = Arc::new(TreeRustleFactory::dense(
            Self::source_seed(tree_id, position, cluster_size),
            rustle_control.clone(),
        ));
        let uuid = self
            .spatial_sound_manager
            .add_procedural_looping_spatial_source(
                rustle_factory,
                TREE_SILENT_VOLUME_DB,
                position,
            )?;

        self.register_source(
            tree_id,
            uuid,
            position,
            cluster_size,
            wind_volume_db,
            rustle_control,
        );
        Ok(uuid)
    }

    fn register_source(
        &mut self,
        tree_id: u32,
        uuid: Uuid,
        position: Vec3,
        cluster_size: u32,
        wind_volume_db: f32,
        rustle_control: Arc<TreeRustleControl>,
    ) {
        let entry = TreeAudioSource::new(
            uuid,
            tree_id,
            position,
            cluster_size,
            wind_volume_db,
            self.wind_response_curve,
            rustle_control,
        );
        self.sources_by_tree.entry(tree_id).or_default().push(uuid);
        self.sources.insert(uuid, entry);
    }

    fn source_seed(tree_id: u32, position: Vec3, cluster_size: u32) -> u64 {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64 ^ tree_id as u64;
        for value in [position.x, position.y, position.z] {
            seed ^= (value.to_bits() as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            seed = seed.rotate_left(27).wrapping_mul(0x94d0_49bb_1331_11eb);
        }
        seed ^ (cluster_size as u64).wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn clustered_volume_db(volume_db: f32, clustered_amount: u32) -> f32 {
        let n = clustered_amount.max(1) as f32;
        if n <= 1.0 {
            return volume_db;
        }

        let gain_db = 10.0 * n.log10();
        volume_db + gain_db
    }
}
