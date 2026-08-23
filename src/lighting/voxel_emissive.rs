use std::collections::{BTreeMap, BTreeSet};

use glam::{UVec3, Vec3};

use super::{
    LocalLight, LocalLightInfluenceBound, LocalLightProviderSnapshot,
    LocalLightProviderSnapshotError, PointLight, ProviderId, SourceLight, SourceLightKey,
};

pub(crate) const EMISSIVE_VOXEL_PROVIDER_ID: ProviderId = ProviderId::new(2);
pub(crate) const EMISSIVE_VOXEL_CLUSTER_DIM: UVec3 = UVec3::new(16, 16, 16);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EmissiveVoxelEmitter {
    pub color: Vec3,
    pub intensity: f32,
    /// Spherical near-field clamp in authoritative world units.
    pub source_radius_world: f32,
    /// Spherical finite support in authoritative world units.
    pub range_world: f32,
}

impl EmissiveVoxelEmitter {
    pub(crate) fn new(
        color: Vec3,
        intensity: f32,
        source_radius_world: f32,
        range_world: f32,
    ) -> Result<Self, EmissiveVoxelProviderError> {
        PointLight::new(
            Vec3::ZERO,
            color,
            intensity,
            source_radius_world,
            range_world,
        )
        .map_err(|_| EmissiveVoxelProviderError::InvalidEmitter)?;
        Ok(Self {
            color,
            intensity,
            source_radius_world,
            range_world,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct EmissiveVoxelProviderChange {
    pub changed: bool,
    pub source_revision: u64,
    pub voxel_count: usize,
    pub aggregate_count: usize,
    pub dirty_cluster_count: usize,
    pub impact_bound_world: Option<LocalLightInfluenceBound>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmissiveVoxelProviderError {
    InvalidVoxelsPerWorldUnit,
    InvalidEmitter,
    DuplicateVoxel(UVec3),
    MissingVoxel(UVec3),
    DestinationOccupied(UVec3),
    Snapshot(LocalLightProviderSnapshotError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct VoxelCoord([u32; 3]);

impl VoxelCoord {
    fn new(value: UVec3) -> Self {
        Self(value.to_array())
    }

    fn get(self) -> UVec3 {
        UVec3::from_array(self.0)
    }

    fn cluster(self) -> ClusterCoord {
        ClusterCoord::new(self.get() / EMISSIVE_VOXEL_CLUSTER_DIM)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ClusterCoord([u32; 3]);

impl ClusterCoord {
    fn new(value: UVec3) -> Self {
        Self(value.to_array())
    }

    fn get(self) -> UVec3 {
        UVec3::from_array(self.0)
    }

    fn source_key(self) -> SourceLightKey {
        let value = self.get();
        SourceLightKey::new(
            (u64::from(value.x) << 32) | u64::from(value.y),
            u64::from(value.z),
        )
    }
}

/// Incremental voxel-emission aggregator. Exact voxel sources remain CPU-authoritative; only one
/// stable point emitter per non-empty 16^3 cell is exposed to the central registry.
#[derive(Clone, Debug)]
pub(crate) struct EmissiveVoxelProvider {
    voxels_per_world_unit: Vec3,
    cells: BTreeMap<ClusterCoord, BTreeMap<VoxelCoord, EmissiveVoxelEmitter>>,
    aggregates: BTreeMap<ClusterCoord, PointLight>,
    voxel_count: usize,
    source_revision: u64,
    snapshot: LocalLightProviderSnapshot,
}

impl EmissiveVoxelProvider {
    pub(crate) fn new(voxels_per_world_unit: Vec3) -> Result<Self, EmissiveVoxelProviderError> {
        if !voxels_per_world_unit.is_finite() || voxels_per_world_unit.min_element() <= 0.0 {
            return Err(EmissiveVoxelProviderError::InvalidVoxelsPerWorldUnit);
        }
        Ok(Self {
            voxels_per_world_unit,
            cells: BTreeMap::new(),
            aggregates: BTreeMap::new(),
            voxel_count: 0,
            source_revision: 0,
            snapshot: LocalLightProviderSnapshot::new(EMISSIVE_VOXEL_PROVIDER_ID, 0, [])
                .expect("empty provider snapshot is valid"),
        })
    }

    pub(crate) fn voxel_count(&self) -> usize {
        self.voxel_count
    }

    pub(crate) fn snapshot(&self) -> LocalLightProviderSnapshot {
        self.snapshot.clone()
    }

    pub(crate) fn source_key_for_voxel(voxel: UVec3) -> SourceLightKey {
        VoxelCoord::new(voxel).cluster().source_key()
    }

    pub(crate) fn set_voxel(
        &mut self,
        voxel: UVec3,
        emitter: Option<EmissiveVoxelEmitter>,
    ) -> Result<EmissiveVoxelProviderChange, EmissiveVoxelProviderError> {
        let voxel = VoxelCoord::new(voxel);
        let cell = voxel.cluster();
        let current = self
            .cells
            .get(&cell)
            .and_then(|sources| sources.get(&voxel))
            .copied();
        if current == emitter {
            return Ok(self.no_change());
        }
        let old_bound = self.aggregate_bound(cell);
        match emitter {
            Some(emitter) => {
                let inserted = self.cells.entry(cell).or_default().insert(voxel, emitter);
                if inserted.is_none() {
                    self.voxel_count += 1;
                }
            }
            None => {
                let removed = self
                    .cells
                    .get_mut(&cell)
                    .and_then(|sources| sources.remove(&voxel));
                if removed.is_some() {
                    self.voxel_count -= 1;
                }
                if self.cells.get(&cell).is_some_and(BTreeMap::is_empty) {
                    self.cells.remove(&cell);
                }
            }
        }
        self.publish_dirty(BTreeSet::from([cell]), old_bound)
    }

    pub(crate) fn move_voxel(
        &mut self,
        from: UVec3,
        to: UVec3,
        emitter: EmissiveVoxelEmitter,
    ) -> Result<EmissiveVoxelProviderChange, EmissiveVoxelProviderError> {
        if from == to {
            return self.set_voxel(from, Some(emitter));
        }
        let from = VoxelCoord::new(from);
        let to = VoxelCoord::new(to);
        let from_cell = from.cluster();
        let to_cell = to.cluster();
        if !self
            .cells
            .get(&from_cell)
            .is_some_and(|sources| sources.contains_key(&from))
        {
            return Err(EmissiveVoxelProviderError::MissingVoxel(from.get()));
        }
        if self
            .cells
            .get(&to_cell)
            .is_some_and(|sources| sources.contains_key(&to))
        {
            return Err(EmissiveVoxelProviderError::DestinationOccupied(to.get()));
        }
        let dirty = BTreeSet::from([from_cell, to_cell]);
        let old_bound = self.aggregate_bounds(dirty.iter().copied());
        self.cells
            .get_mut(&from_cell)
            .expect("validated source cell must exist")
            .remove(&from);
        if self.cells[&from_cell].is_empty() {
            self.cells.remove(&from_cell);
        }
        self.cells.entry(to_cell).or_default().insert(to, emitter);
        self.publish_dirty(dirty, old_bound)
    }

    pub(crate) fn replace_all(
        &mut self,
        emitters: impl IntoIterator<Item = (UVec3, EmissiveVoxelEmitter)>,
    ) -> Result<EmissiveVoxelProviderChange, EmissiveVoxelProviderError> {
        let mut next: BTreeMap<ClusterCoord, BTreeMap<VoxelCoord, EmissiveVoxelEmitter>> =
            BTreeMap::new();
        let mut next_count = 0;
        for (voxel, emitter) in emitters {
            let voxel = VoxelCoord::new(voxel);
            if next
                .entry(voxel.cluster())
                .or_default()
                .insert(voxel, emitter)
                .is_some()
            {
                return Err(EmissiveVoxelProviderError::DuplicateVoxel(voxel.get()));
            }
            next_count += 1;
        }
        if next == self.cells {
            return Ok(self.no_change());
        }
        let dirty: BTreeSet<_> = self
            .cells
            .keys()
            .chain(next.keys())
            .copied()
            .filter(|cell| self.cells.get(cell) != next.get(cell))
            .collect();
        let old_bound = self.aggregate_bounds(dirty.iter().copied());
        self.cells = next;
        self.voxel_count = next_count;
        self.publish_dirty(dirty, old_bound)
    }

    fn no_change(&self) -> EmissiveVoxelProviderChange {
        EmissiveVoxelProviderChange {
            source_revision: self.source_revision,
            voxel_count: self.voxel_count,
            aggregate_count: self.aggregates.len(),
            ..Default::default()
        }
    }

    fn publish_dirty(
        &mut self,
        dirty: BTreeSet<ClusterCoord>,
        old_bound: Option<LocalLightInfluenceBound>,
    ) -> Result<EmissiveVoxelProviderChange, EmissiveVoxelProviderError> {
        for cell in dirty.iter().copied() {
            match self.aggregate_cell(cell) {
                Some(light) => {
                    self.aggregates.insert(cell, light);
                }
                None => {
                    self.aggregates.remove(&cell);
                }
            }
        }
        let new_bound = self.aggregate_bounds(dirty.iter().copied());
        self.source_revision = self.source_revision.wrapping_add(1).max(1);
        self.snapshot = LocalLightProviderSnapshot::new(
            EMISSIVE_VOXEL_PROVIDER_ID,
            self.source_revision,
            self.aggregates.iter().map(|(cell, light)| {
                SourceLight::new(cell.source_key(), LocalLight::Point(*light))
            }),
        )
        .map_err(EmissiveVoxelProviderError::Snapshot)?;
        Ok(EmissiveVoxelProviderChange {
            changed: true,
            source_revision: self.source_revision,
            voxel_count: self.voxel_count,
            aggregate_count: self.aggregates.len(),
            dirty_cluster_count: dirty.len(),
            impact_bound_world: union_optional_bounds(old_bound, new_bound),
        })
    }

    fn aggregate_cell(&self, cell: ClusterCoord) -> Option<PointLight> {
        let sources = self.cells.get(&cell)?;
        let mut weighted_position = Vec3::ZERO;
        let mut radiometric_color = Vec3::ZERO;
        let mut total_intensity = 0.0;
        let mut fallback_position = Vec3::ZERO;
        for (voxel, emitter) in sources {
            let position = (voxel.get().as_vec3() + Vec3::splat(0.5)) / self.voxels_per_world_unit;
            weighted_position += position * emitter.intensity;
            fallback_position += position;
            radiometric_color += emitter.color * emitter.intensity;
            total_intensity += emitter.intensity;
        }
        let position = if total_intensity > 0.0 {
            weighted_position / total_intensity
        } else {
            fallback_position / sources.len() as f32
        };
        let color = if total_intensity > 0.0 {
            radiometric_color / total_intensity
        } else {
            sources
                .values()
                .fold(Vec3::ZERO, |sum, emitter| sum + emitter.color)
                / sources.len() as f32
        };
        let mut source_radius: f32 = 0.0;
        let mut range: f32 = 0.0;
        // The GPU visibility endpoint is spherical in world space. Under non-uniform voxel
        // density the half-diagonal is the conservative radius that contains one voxel cell.
        let voxel_half_diagonal_world = (Vec3::splat(0.5) / self.voxels_per_world_unit).length();
        for (voxel, emitter) in sources {
            let emitter_position =
                (voxel.get().as_vec3() + Vec3::splat(0.5)) / self.voxels_per_world_unit;
            let offset = emitter_position.distance(position);
            source_radius = source_radius
                .max(emitter.source_radius_world.max(voxel_half_diagonal_world) + offset);
            range = range.max(emitter.range_world + offset);
        }
        range = range.max(source_radius);
        PointLight::new(position, color, total_intensity, source_radius, range).ok()
    }

    fn aggregate_bound(&self, cell: ClusterCoord) -> Option<LocalLightInfluenceBound> {
        self.aggregates
            .get(&cell)
            .copied()
            .map(|light| LocalLight::Point(light).influence_bound())
    }

    fn aggregate_bounds(
        &self,
        cells: impl IntoIterator<Item = ClusterCoord>,
    ) -> Option<LocalLightInfluenceBound> {
        cells
            .into_iter()
            .filter_map(|cell| self.aggregate_bound(cell))
            .reduce(LocalLightInfluenceBound::union)
    }
}

fn union_optional_bounds(
    a: Option<LocalLightInfluenceBound>,
    b: Option<LocalLightInfluenceBound>,
) -> Option<LocalLightInfluenceBound> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.union(b)),
        (Some(bound), None) | (None, Some(bound)) => Some(bound),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{UVec3, Vec3};

    fn emitter(color: Vec3, intensity: f32) -> EmissiveVoxelEmitter {
        // Deliberately smaller than one voxel half-diagonal; aggregation must enlarge it.
        EmissiveVoxelEmitter::new(color, intensity, 0.001, 0.75)
            .expect("test emitter must be valid")
    }

    #[test]
    fn nonuniform_voxel_scale_builds_one_world_unit_cluster_light() {
        let mut provider =
            EmissiveVoxelProvider::new(Vec3::new(256.0, 128.0, 64.0)).expect("scale must be valid");
        provider
            .set_voxel(
                UVec3::new(16, 16, 16),
                Some(emitter(Vec3::new(1.0, 0.5, 0.25), 4.0)),
            )
            .expect("insert must succeed");
        provider
            .set_voxel(
                UVec3::new(17, 16, 16),
                Some(emitter(Vec3::new(0.5, 1.0, 0.25), 2.0)),
            )
            .expect("insert must succeed");

        let snapshot = provider.snapshot();
        assert_eq!(provider.voxel_count(), 2);
        assert_eq!(snapshot.sources().len(), 1, "one aggregate per cell");
        let source = snapshot.sources()[0];
        assert_eq!(source.key(), SourceLightKey::new((1_u64 << 32) | 1, 1));
        let LocalLight::Point(light) = source.light() else {
            panic!("voxel aggregates must currently produce point lights")
        };
        assert!(light.position.x > 16.5 / 256.0);
        assert!(light.position.x < 17.5 / 256.0);
        assert_eq!(light.position.y, 16.5 / 128.0);
        assert_eq!(light.position.z, 16.5 / 64.0);
        assert_eq!(light.intensity, 6.0);
        assert_eq!(light.color, Vec3::new(5.0 / 6.0, 4.0 / 6.0, 0.25));
        let far_center_world = Vec3::new(17.5 / 256.0, 16.5 / 128.0, 16.5 / 64.0);
        let voxel_half_diagonal_world = (Vec3::splat(0.5) / Vec3::new(256.0, 128.0, 64.0)).length();
        assert!(
            light.source_radius
                >= far_center_world.distance(light.position) + voxel_half_diagonal_world
        );
        assert!(
            light.range
                >= far_center_world.distance(light.position) + emitter(Vec3::ONE, 1.0).range_world
        );
        assert!(light.range >= light.source_radius);
    }

    #[test]
    fn dirty_cell_updates_preserve_unrelated_source_keys_and_remove_empty_cells() {
        let mut provider = EmissiveVoxelProvider::new(Vec3::splat(256.0)).unwrap();
        let a = UVec3::new(1, 2, 3);
        let b = UVec3::new(33, 2, 3);
        provider
            .set_voxel(a, Some(emitter(Vec3::ONE, 1.0)))
            .unwrap();
        provider.set_voxel(b, Some(emitter(Vec3::X, 2.0))).unwrap();
        let before = provider.snapshot();
        let b_key = EmissiveVoxelProvider::source_key_for_voxel(b);

        let change = provider.set_voxel(a, Some(emitter(Vec3::Y, 3.0))).unwrap();
        assert_eq!(change.dirty_cluster_count, 1);
        assert_eq!(provider.snapshot().sources().len(), 2);
        assert!(provider
            .snapshot()
            .sources()
            .iter()
            .any(|source| source.key() == b_key));
        assert_ne!(
            before.source_revision(),
            provider.snapshot().source_revision()
        );

        let removal = provider.set_voxel(a, None).unwrap();
        assert_eq!(removal.dirty_cluster_count, 1);
        assert_eq!(provider.snapshot().sources().len(), 1);
        assert_eq!(provider.snapshot().sources()[0].key(), b_key);
    }

    #[test]
    fn rebuild_reorder_is_a_noop_and_move_dirties_old_and_new_cells_once() {
        let mut provider = EmissiveVoxelProvider::new(Vec3::splat(256.0)).unwrap();
        let a = (UVec3::new(1, 1, 1), emitter(Vec3::ONE, 1.0));
        let b = (UVec3::new(40, 1, 1), emitter(Vec3::X, 2.0));
        provider.replace_all([a, b]).unwrap();
        let revision = provider.snapshot().source_revision();
        let keys: Vec<_> = provider
            .snapshot()
            .sources()
            .iter()
            .map(|source| source.key())
            .collect();

        let noop = provider.replace_all([b, a]).unwrap();
        assert!(!noop.changed);
        assert_eq!(provider.snapshot().source_revision(), revision);
        assert_eq!(
            provider
                .snapshot()
                .sources()
                .iter()
                .map(|source| source.key())
                .collect::<Vec<_>>(),
            keys
        );

        let moved = provider.move_voxel(a.0, UVec3::new(80, 1, 1), a.1).unwrap();
        assert!(moved.changed);
        assert_eq!(moved.dirty_cluster_count, 2);
        assert_eq!(provider.voxel_count(), 2);
    }
}
