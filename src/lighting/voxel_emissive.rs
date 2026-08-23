use std::collections::{BTreeMap, BTreeSet};

use glam::{UVec3, Vec3};

use crate::geom::UAabb3;

use super::{
    LocalLight, LocalLightInfluenceBound, LocalLightProviderSnapshot,
    LocalLightProviderSnapshotError, PointLight, ProviderId, SourceLight, SourceLightKey,
};

pub(crate) const EMISSIVE_VOXEL_PROVIDER_ID: ProviderId = ProviderId::new(2);
pub(crate) const EMISSIVE_VOXEL_CLUSTER_DIM: UVec3 = UVec3::new(16, 16, 16);
pub(crate) const EMISSIVE_VOXEL_COLOR_RGB8: [u8; 3] = [255, 92, 20];
pub(crate) const EMISSIVE_VOXEL_COLOR_SRGB: Vec3 = Vec3::new(
    EMISSIVE_VOXEL_COLOR_RGB8[0] as f32 / 255.0,
    EMISSIVE_VOXEL_COLOR_RGB8[1] as f32 / 255.0,
    EMISSIVE_VOXEL_COLOR_RGB8[2] as f32 / 255.0,
);
pub(crate) const EMISSIVE_VOXEL_SURFACE_RADIANCE: f32 = 4.0;

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
    InvalidRegion(UAabb3),
    EmitterOutsideRegion { voxel: UVec3, region: UAabb3 },
    InvalidAggregate(SourceLightKey),
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
        let mut next_cell = self.cells.get(&cell).cloned().unwrap_or_default();
        let mut next_voxel_count = self.voxel_count;
        match emitter {
            Some(emitter) => {
                let inserted = next_cell.insert(voxel, emitter);
                if inserted.is_none() {
                    next_voxel_count += 1;
                }
            }
            None => {
                let removed = next_cell.remove(&voxel);
                if removed.is_some() {
                    next_voxel_count -= 1;
                }
            }
        }
        self.publish_replacements(BTreeMap::from([(cell, next_cell)]), next_voxel_count)
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
        let mut replacements = BTreeMap::new();
        if from_cell == to_cell {
            let mut sources = self.cells[&from_cell].clone();
            sources.remove(&from);
            sources.insert(to, emitter);
            replacements.insert(from_cell, sources);
        } else {
            let mut from_sources = self.cells[&from_cell].clone();
            from_sources.remove(&from);
            let mut to_sources = self.cells.get(&to_cell).cloned().unwrap_or_default();
            to_sources.insert(to, emitter);
            replacements.insert(from_cell, from_sources);
            replacements.insert(to_cell, to_sources);
        }
        self.publish_replacements(replacements, self.voxel_count)
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
        let replacements = dirty
            .into_iter()
            .map(|cell| (cell, next.get(&cell).cloned().unwrap_or_default()))
            .collect();
        self.publish_replacements(replacements, next_count)
    }

    /// Replaces the authoritative emissive voxels in one half-open voxel region as a single
    /// publication. Voxels outside the region are preserved, including neighbours that share a
    /// 16^3 aggregate cell. All validation and aggregate construction completes before mutation.
    pub(crate) fn replace_region(
        &mut self,
        region: UAabb3,
        emitters: impl IntoIterator<Item = (UVec3, EmissiveVoxelEmitter)>,
    ) -> Result<EmissiveVoxelProviderChange, EmissiveVoxelProviderError> {
        if region.min().cmpge(region.max()).any() {
            return Err(EmissiveVoxelProviderError::InvalidRegion(region));
        }
        let in_region =
            |voxel: UVec3| voxel.cmpge(region.min()).all() && voxel.cmplt(region.max()).all();
        let mut incoming_by_cell: BTreeMap<
            ClusterCoord,
            BTreeMap<VoxelCoord, EmissiveVoxelEmitter>,
        > = BTreeMap::new();
        for (voxel, emitter) in emitters {
            if !in_region(voxel) {
                return Err(EmissiveVoxelProviderError::EmitterOutsideRegion { voxel, region });
            }
            let voxel = VoxelCoord::new(voxel);
            if incoming_by_cell
                .entry(voxel.cluster())
                .or_default()
                .insert(voxel, emitter)
                .is_some()
            {
                return Err(EmissiveVoxelProviderError::DuplicateVoxel(voxel.get()));
            }
        }

        let mut replacements = BTreeMap::new();
        let mut old_dirty_count = 0usize;
        // Enumerate the half-open region's intersecting aggregate cells directly. This path is
        // bounded by the replaced region and never scans unrelated authoritative emitter cells.
        for cell in clusters_intersecting_region(region) {
            let current = self.cells.get(&cell).cloned().unwrap_or_default();
            old_dirty_count += current.len();
            let mut next = current
                .iter()
                .filter(|(voxel, _)| !in_region(voxel.get()))
                .map(|(voxel, emitter)| (*voxel, *emitter))
                .collect::<BTreeMap<_, _>>();
            if let Some(incoming) = incoming_by_cell.get(&cell) {
                next.extend(incoming.iter().map(|(voxel, emitter)| (*voxel, *emitter)));
            }
            replacements.insert(cell, next);
        }
        let next_dirty_count = replacements.values().map(BTreeMap::len).sum::<usize>();
        if replacements.iter().all(|(cell, sources)| {
            self.cells
                .get(cell)
                .map_or(sources.is_empty(), |current| current == sources)
        }) {
            return Ok(self.no_change());
        }
        let next_voxel_count = self.voxel_count - old_dirty_count + next_dirty_count;
        self.publish_replacements(replacements, next_voxel_count)
    }

    fn no_change(&self) -> EmissiveVoxelProviderChange {
        EmissiveVoxelProviderChange {
            source_revision: self.source_revision,
            voxel_count: self.voxel_count,
            aggregate_count: self.aggregates.len(),
            ..Default::default()
        }
    }

    /// Prepares every changed aggregate and the immutable provider snapshot before committing any
    /// mutable state. A numeric or snapshot error therefore leaves the old publication intact.
    fn publish_replacements(
        &mut self,
        replacements: BTreeMap<ClusterCoord, BTreeMap<VoxelCoord, EmissiveVoxelEmitter>>,
        next_voxel_count: usize,
    ) -> Result<EmissiveVoxelProviderChange, EmissiveVoxelProviderError> {
        let old_bound = self.aggregate_bounds(replacements.keys().copied());
        let mut next_aggregates = self.aggregates.clone();
        for (cell, sources) in &replacements {
            match self.aggregate_sources(*cell, sources)? {
                Some(light) => {
                    next_aggregates.insert(*cell, light);
                }
                None => {
                    next_aggregates.remove(cell);
                }
            }
        }
        let new_bound = aggregate_bounds_in(&next_aggregates, replacements.keys().copied());
        let next_source_revision = self.source_revision.wrapping_add(1).max(1);
        let next_snapshot = LocalLightProviderSnapshot::new(
            EMISSIVE_VOXEL_PROVIDER_ID,
            next_source_revision,
            next_aggregates.iter().map(|(cell, light)| {
                SourceLight::new(cell.source_key(), LocalLight::Point(*light))
            }),
        )
        .map_err(EmissiveVoxelProviderError::Snapshot)?;
        let dirty_cluster_count = replacements.len();
        for (cell, sources) in replacements {
            if sources.is_empty() {
                self.cells.remove(&cell);
            } else {
                self.cells.insert(cell, sources);
            }
        }
        self.aggregates = next_aggregates;
        self.voxel_count = next_voxel_count;
        self.source_revision = next_source_revision;
        self.snapshot = next_snapshot;
        Ok(EmissiveVoxelProviderChange {
            changed: true,
            source_revision: self.source_revision,
            voxel_count: self.voxel_count,
            aggregate_count: self.aggregates.len(),
            dirty_cluster_count,
            impact_bound_world: union_optional_bounds(old_bound, new_bound),
        })
    }

    fn aggregate_sources(
        &self,
        cell: ClusterCoord,
        sources: &BTreeMap<VoxelCoord, EmissiveVoxelEmitter>,
    ) -> Result<Option<PointLight>, EmissiveVoxelProviderError> {
        if sources.is_empty() {
            return Ok(None);
        }
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
        PointLight::new(position, color, total_intensity, source_radius, range)
            .map(Some)
            .map_err(|_| EmissiveVoxelProviderError::InvalidAggregate(cell.source_key()))
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

fn clusters_intersecting_region(region: UAabb3) -> impl Iterator<Item = ClusterCoord> {
    let first = region.min() / EMISSIVE_VOXEL_CLUSTER_DIM;
    let last = (region.max() - UVec3::ONE) / EMISSIVE_VOXEL_CLUSTER_DIM;
    (first.z..=last.z).flat_map(move |z| {
        (first.y..=last.y).flat_map(move |y| {
            (first.x..=last.x).map(move |x| ClusterCoord::new(UVec3::new(x, y, z)))
        })
    })
}

fn aggregate_bounds_in(
    aggregates: &BTreeMap<ClusterCoord, PointLight>,
    cells: impl IntoIterator<Item = ClusterCoord>,
) -> Option<LocalLightInfluenceBound> {
    cells
        .into_iter()
        .filter_map(|cell| aggregates.get(&cell).copied())
        .map(|light| LocalLight::Point(light).influence_bound())
        .reduce(LocalLightInfluenceBound::union)
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

    #[test]
    fn aggregate_failure_leaves_provider_publication_transaction_unchanged() {
        let mut provider = EmissiveVoxelProvider::new(Vec3::splat(256.0)).unwrap();
        let stable = UVec3::new(1, 1, 1);
        provider
            .set_voxel(stable, Some(emitter(Vec3::ONE, 1.0)))
            .unwrap();
        let before_snapshot = provider.snapshot();
        let before_voxels = provider.voxel_count();
        let invalid = EmissiveVoxelEmitter {
            color: Vec3::splat(f32::MAX),
            intensity: f32::MAX,
            source_radius_world: 0.001,
            range_world: 0.75,
        };

        assert!(matches!(
            provider.set_voxel(UVec3::new(2, 1, 1), Some(invalid)),
            Err(EmissiveVoxelProviderError::InvalidAggregate(_))
        ));
        assert_eq!(provider.voxel_count(), before_voxels);
        assert_eq!(provider.snapshot(), before_snapshot);
        assert_eq!(provider.cells[&VoxelCoord::new(stable).cluster()].len(), 1);
        assert_eq!(provider.aggregates.len(), 1);
    }

    #[test]
    fn region_replacement_is_atomic_removes_stale_voxels_and_preserves_neighbours() {
        let mut provider = EmissiveVoxelProvider::new(Vec3::splat(256.0)).unwrap();
        let old_inside = UVec3::new(1, 1, 1);
        let outside = UVec3::new(20, 1, 1);
        provider
            .replace_all([
                (old_inside, emitter(Vec3::X, 1.0)),
                (outside, emitter(Vec3::Y, 2.0)),
            ])
            .unwrap();
        let before = provider.snapshot();
        let region = UAabb3::new(UVec3::ZERO, UVec3::splat(16));
        let replacement = (UVec3::new(2, 2, 2), emitter(Vec3::Z, 3.0));

        let changed = provider.replace_region(region, [replacement]).unwrap();

        assert!(changed.changed);
        assert_eq!(changed.dirty_cluster_count, 1);
        assert_eq!(provider.voxel_count(), 2);
        assert_ne!(
            provider.snapshot().source_revision(),
            before.source_revision()
        );
        assert!(provider.cells[&VoxelCoord::new(replacement.0).cluster()]
            .contains_key(&VoxelCoord::new(replacement.0)));
        assert!(!provider.cells[&VoxelCoord::new(old_inside).cluster()]
            .contains_key(&VoxelCoord::new(old_inside)));
        assert!(provider.cells[&VoxelCoord::new(outside).cluster()]
            .contains_key(&VoxelCoord::new(outside)));

        let revision = provider.snapshot().source_revision();
        let noop = provider.replace_region(region, [replacement]).unwrap();
        assert!(!noop.changed);
        assert_eq!(provider.snapshot().source_revision(), revision);
    }

    #[test]
    fn invalid_region_replacement_keeps_the_previous_publication() {
        let mut provider = EmissiveVoxelProvider::new(Vec3::splat(256.0)).unwrap();
        provider
            .set_voxel(UVec3::new(1, 1, 1), Some(emitter(Vec3::ONE, 1.0)))
            .unwrap();
        let before = provider.clone();
        let region = UAabb3::new(UVec3::ZERO, UVec3::splat(16));
        let outside = UVec3::new(16, 1, 1);

        assert!(matches!(
            provider.replace_region(region, [(outside, emitter(Vec3::X, 2.0))]),
            Err(EmissiveVoxelProviderError::EmitterOutsideRegion { voxel, .. })
                if voxel == outside
        ));
        assert_eq!(provider.cells, before.cells);
        assert_eq!(provider.aggregates, before.aggregates);
        assert_eq!(provider.snapshot(), before.snapshot());
    }

    #[test]
    fn region_replacement_enumerates_only_intersecting_cells_with_many_unrelated_sources() {
        let mut provider = EmissiveVoxelProvider::new(Vec3::splat(256.0)).unwrap();
        let unrelated = (1..=1_024).map(|cell| {
            (
                UVec3::new(cell * EMISSIVE_VOXEL_CLUSTER_DIM.x, 64, 64),
                emitter(Vec3::X, 1.0),
            )
        });
        provider.replace_all(unrelated).unwrap();
        let unrelated_before = provider.cells.clone();
        let target = UVec3::new(2, 2, 2);
        let region = UAabb3::new(UVec3::ZERO, EMISSIVE_VOXEL_CLUSTER_DIM);

        let change = provider
            .replace_region(region, [(target, emitter(Vec3::Y, 2.0))])
            .unwrap();

        assert_eq!(change.dirty_cluster_count, 1);
        assert_eq!(provider.voxel_count(), 1_025);
        for (cell, sources) in unrelated_before {
            assert_eq!(provider.cells.get(&cell), Some(&sources));
        }
        assert_eq!(
            clusters_intersecting_region(region).collect::<Vec<_>>(),
            vec![ClusterCoord::new(UVec3::ZERO)]
        );
    }
}
