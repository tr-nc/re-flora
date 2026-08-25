#![cfg_attr(not(test), allow(dead_code))]

use std::collections::{BTreeMap, BTreeSet};

use super::{
    LocalLight, LocalLightInfluenceBound, LocalLightProviderSnapshot,
    LocalLightProviderSnapshotError, ProviderId, SourceLight, SourceLightKey,
};

pub(crate) const RASTER_ENTITY_LIGHT_PROVIDER_ID: ProviderId = ProviderId::new(3);

/// Stable identity owned by a raster-entity domain. Namespacing prevents unrelated entity stores
/// from aliasing when they both use small local counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RasterEntityId {
    namespace: u32,
    slot: u32,
    generation: u32,
}

impl RasterEntityId {
    pub(crate) const fn new(namespace: u32, slot: u32, generation: u32) -> Self {
        Self {
            namespace,
            slot,
            generation,
        }
    }

    const fn packed_slot(self) -> u64 {
        (self.namespace as u64) << 32 | self.slot as u64
    }

    pub(crate) const fn generation(self) -> u32 {
        self.generation
    }

    pub(crate) const fn slot(self) -> u32 {
        self.slot
    }
}

/// Provider-local part identity. This remains stable across instance-buffer rebuilds, LOD changes,
/// and render ordering; it describes a semantic emitter part, not a GPU array index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RasterEmitterPartId(u32);

impl RasterEmitterPartId {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RasterEmitterKey {
    entity: RasterEntityId,
    part: RasterEmitterPartId,
}

impl RasterEmitterKey {
    pub(crate) const fn new(entity: RasterEntityId, part: RasterEmitterPartId) -> Self {
        Self { entity, part }
    }

    #[allow(dead_code)]
    pub(crate) const fn entity(self) -> RasterEntityId {
        self.entity
    }

    #[allow(dead_code)]
    pub(crate) const fn part(self) -> RasterEmitterPartId {
        self.part
    }

    pub(crate) const fn source_key(self) -> SourceLightKey {
        SourceLightKey::new(
            self.entity.packed_slot(),
            (self.entity.generation() as u64) << 32 | self.part.get() as u64,
        )
    }
}

/// Authoritative world-unit emitter component. It intentionally contains no registry identity,
/// GPU descriptor, DDGI revision, render instance index, or LOD state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RasterEmitterComponent {
    light: LocalLight,
}

impl RasterEmitterComponent {
    pub(crate) const fn new(light: LocalLight) -> Self {
        Self { light }
    }

    pub(crate) const fn light(self) -> LocalLight {
        self.light
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct RasterEntityEmitterChange {
    pub changed: bool,
    pub source_revision: u64,
    pub source_count: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub impact_bound_world: Option<LocalLightInfluenceBound>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RasterEntityEmitterProviderError {
    DuplicatePart {
        entity: RasterEntityId,
        part: RasterEmitterPartId,
    },
    DuplicateKey(RasterEmitterKey),
    Snapshot(LocalLightProviderSnapshotError),
}

/// Authoritative provider for optional emitter components attached to raster entities. Entity
/// publications replace all parts of one entity atomically; full-store rebuilds are deterministic
/// and do not churn source revisions when only iteration or instance-buffer ordering changed.
#[derive(Clone, Debug)]
pub(crate) struct RasterEntityEmitterProvider {
    sources: BTreeMap<RasterEmitterKey, RasterEmitterComponent>,
    source_revision: u64,
    snapshot: LocalLightProviderSnapshot,
}

impl Default for RasterEntityEmitterProvider {
    fn default() -> Self {
        Self {
            sources: BTreeMap::new(),
            source_revision: 0,
            snapshot: LocalLightProviderSnapshot::new(RASTER_ENTITY_LIGHT_PROVIDER_ID, 0, [])
                .expect("empty raster emitter provider snapshot is valid"),
        }
    }
}

impl RasterEntityEmitterProvider {
    pub(crate) fn snapshot(&self) -> LocalLightProviderSnapshot {
        self.snapshot.clone()
    }

    pub(crate) fn source_count(&self) -> usize {
        self.sources.len()
    }

    pub(crate) fn publish_entity(
        &mut self,
        entity: RasterEntityId,
        parts: impl IntoIterator<Item = (RasterEmitterPartId, RasterEmitterComponent)>,
    ) -> Result<RasterEntityEmitterChange, RasterEntityEmitterProviderError> {
        let mut incoming = BTreeMap::new();
        for (part, component) in parts {
            if incoming.insert(part, component).is_some() {
                return Err(RasterEntityEmitterProviderError::DuplicatePart { entity, part });
            }
        }

        let mut next = self.sources.clone();
        next.retain(|key, _| key.entity != entity);
        next.extend(
            incoming
                .into_iter()
                .map(|(part, component)| (RasterEmitterKey::new(entity, part), component)),
        );
        self.publish_if_changed(next)
    }

    pub(crate) fn remove_entity(
        &mut self,
        entity: RasterEntityId,
    ) -> Result<RasterEntityEmitterChange, RasterEntityEmitterProviderError> {
        self.publish_entity(entity, [])
    }

    pub(crate) fn remove_entities(
        &mut self,
        entities: impl IntoIterator<Item = RasterEntityId>,
    ) -> Result<RasterEntityEmitterChange, RasterEntityEmitterProviderError> {
        let entities = entities.into_iter().collect::<BTreeSet<_>>();
        let mut next = self.sources.clone();
        next.retain(|key, _| !entities.contains(&key.entity));
        self.publish_if_changed(next)
    }

    pub(crate) fn replace_all(
        &mut self,
        sources: impl IntoIterator<Item = (RasterEmitterKey, RasterEmitterComponent)>,
    ) -> Result<RasterEntityEmitterChange, RasterEntityEmitterProviderError> {
        let mut next = BTreeMap::new();
        for (key, component) in sources {
            if next.insert(key, component).is_some() {
                return Err(RasterEntityEmitterProviderError::DuplicateKey(key));
            }
        }
        self.publish_if_changed(next)
    }

    fn publish_if_changed(
        &mut self,
        next: BTreeMap<RasterEmitterKey, RasterEmitterComponent>,
    ) -> Result<RasterEntityEmitterChange, RasterEntityEmitterProviderError> {
        if next == self.sources {
            return Ok(RasterEntityEmitterChange {
                source_revision: self.source_revision,
                source_count: self.sources.len(),
                ..Default::default()
            });
        }

        let old_keys = self.sources.keys().copied().collect::<BTreeSet<_>>();
        let next_keys = next.keys().copied().collect::<BTreeSet<_>>();
        let added = next_keys.difference(&old_keys).count();
        let removed = old_keys.difference(&next_keys).count();
        let updated_keys = old_keys
            .intersection(&next_keys)
            .copied()
            .filter(|key| self.sources.get(key) != next.get(key))
            .collect::<Vec<_>>();
        let impact_bound_world = old_keys
            .symmetric_difference(&next_keys)
            .copied()
            .chain(updated_keys.iter().copied())
            .flat_map(|key| [self.sources.get(&key).copied(), next.get(&key).copied()])
            .flatten()
            .map(|component| component.light().influence_bound())
            .reduce(LocalLightInfluenceBound::union);
        let source_revision = self.source_revision.wrapping_add(1).max(1);
        let snapshot = LocalLightProviderSnapshot::new(
            RASTER_ENTITY_LIGHT_PROVIDER_ID,
            source_revision,
            next.iter()
                .map(|(key, component)| SourceLight::new(key.source_key(), component.light())),
        )
        .map_err(RasterEntityEmitterProviderError::Snapshot)?;

        self.sources = next;
        self.source_revision = source_revision;
        self.snapshot = snapshot;
        Ok(RasterEntityEmitterChange {
            changed: true,
            source_revision,
            source_count: self.sources.len(),
            added,
            updated: updated_keys.len(),
            removed,
            impact_bound_world,
        })
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;
    use crate::lighting::{LocalLightRegistry, PointLight};

    fn point(position: Vec3, intensity: f32) -> RasterEmitterComponent {
        RasterEmitterComponent::new(LocalLight::Point(
            PointLight::new(position, Vec3::new(1.0, 0.4, 0.1), intensity, 0.02, 0.5)
                .expect("test point light is valid"),
        ))
    }

    #[test]
    fn entity_rebuild_and_part_reorder_preserve_stable_registry_ids() {
        let entity = RasterEntityId::new(7, 42, 1);
        let head = RasterEmitterPartId::new(2);
        let stem = RasterEmitterPartId::new(1);
        let mut provider = RasterEntityEmitterProvider::default();
        let mut registry = LocalLightRegistry::default();

        let first = provider
            .publish_entity(
                entity,
                [(head, point(Vec3::Y, 2.0)), (stem, point(Vec3::X, 1.0))],
            )
            .unwrap();
        assert_eq!((first.added, first.updated, first.removed), (2, 0, 0));
        registry.reconcile(provider.snapshot()).unwrap();
        let head_id = registry
            .light_id(
                RASTER_ENTITY_LIGHT_PROVIDER_ID,
                RasterEmitterKey::new(entity, head).source_key(),
            )
            .unwrap();
        let stem_id = registry
            .light_id(
                RASTER_ENTITY_LIGHT_PROVIDER_ID,
                RasterEmitterKey::new(entity, stem).source_key(),
            )
            .unwrap();
        let provider_revision = first.source_revision;
        let registry_revision = registry.registry_revision();

        let reorder = provider
            .publish_entity(
                entity,
                [(stem, point(Vec3::X, 1.0)), (head, point(Vec3::Y, 2.0))],
            )
            .unwrap();
        assert!(!reorder.changed);
        assert_eq!(reorder.source_revision, provider_revision);
        registry.reconcile(provider.snapshot()).unwrap();
        assert_eq!(registry.registry_revision(), registry_revision);
        assert_eq!(
            registry.light_id(
                RASTER_ENTITY_LIGHT_PROVIDER_ID,
                RasterEmitterKey::new(entity, head).source_key()
            ),
            Some(head_id)
        );
        assert_eq!(
            registry.light_id(
                RASTER_ENTITY_LIGHT_PROVIDER_ID,
                RasterEmitterKey::new(entity, stem).source_key()
            ),
            Some(stem_id)
        );
    }

    #[test]
    fn update_remove_and_entity_disappearance_are_atomic_and_never_stale() {
        let first_entity = RasterEntityId::new(8, 1, 1);
        let second_entity = RasterEntityId::new(8, 2, 1);
        let head = RasterEmitterPartId::new(9);
        let mut provider = RasterEntityEmitterProvider::default();
        let mut registry = LocalLightRegistry::default();

        provider
            .replace_all([
                (
                    RasterEmitterKey::new(first_entity, head),
                    point(Vec3::X, 1.0),
                ),
                (
                    RasterEmitterKey::new(second_entity, head),
                    point(Vec3::Y, 2.0),
                ),
            ])
            .unwrap();
        registry.reconcile(provider.snapshot()).unwrap();
        let first_id = registry
            .light_id(
                RASTER_ENTITY_LIGHT_PROVIDER_ID,
                RasterEmitterKey::new(first_entity, head).source_key(),
            )
            .unwrap();

        let update = provider
            .publish_entity(first_entity, [(head, point(Vec3::Z, 3.0))])
            .unwrap();
        assert_eq!((update.added, update.updated, update.removed), (0, 1, 0));
        registry.reconcile(provider.snapshot()).unwrap();
        assert_eq!(
            registry.light_id(
                RASTER_ENTITY_LIGHT_PROVIDER_ID,
                RasterEmitterKey::new(first_entity, head).source_key(),
            ),
            Some(first_id)
        );

        let removal = provider.remove_entity(first_entity).unwrap();
        assert_eq!((removal.added, removal.updated, removal.removed), (0, 0, 1));
        registry.reconcile(provider.snapshot()).unwrap();
        assert!(registry
            .light_id(
                RASTER_ENTITY_LIGHT_PROVIDER_ID,
                RasterEmitterKey::new(first_entity, head).source_key(),
            )
            .is_none());
        assert_eq!(provider.source_count(), 1);

        let disappearance = provider.replace_all([]).unwrap();
        assert_eq!(disappearance.removed, 1);
        registry.reconcile(provider.snapshot()).unwrap();
        assert!(registry.snapshot().lights().is_empty());
    }

    #[test]
    fn duplicate_parts_and_keys_fail_without_mutating_the_previous_snapshot() {
        let entity = RasterEntityId::new(9, 1, 1);
        let part = RasterEmitterPartId::new(1);
        let mut provider = RasterEntityEmitterProvider::default();
        provider
            .publish_entity(entity, [(part, point(Vec3::X, 1.0))])
            .unwrap();
        let before = provider.snapshot();

        assert_eq!(
            provider.publish_entity(
                entity,
                [(part, point(Vec3::Y, 2.0)), (part, point(Vec3::Z, 3.0))]
            ),
            Err(RasterEntityEmitterProviderError::DuplicatePart { entity, part })
        );
        let key = RasterEmitterKey::new(entity, part);
        assert_eq!(
            provider.replace_all([(key, point(Vec3::Y, 2.0)), (key, point(Vec3::Z, 3.0))]),
            Err(RasterEntityEmitterProviderError::DuplicateKey(key))
        );
        assert_eq!(provider.snapshot(), before);
    }

    #[test]
    fn multi_entity_removal_is_one_atomic_provider_and_registry_publication() {
        let first = RasterEntityId::new(11, 1, 1);
        let second = RasterEntityId::new(11, 2, 1);
        let survivor = RasterEntityId::new(12, 1, 1);
        let part = RasterEmitterPartId::new(1);
        let mut provider = RasterEntityEmitterProvider::default();
        provider
            .replace_all([
                (RasterEmitterKey::new(first, part), point(Vec3::X, 1.0)),
                (RasterEmitterKey::new(second, part), point(Vec3::Y, 1.0)),
                (RasterEmitterKey::new(survivor, part), point(Vec3::Z, 1.0)),
            ])
            .unwrap();
        let mut registry = LocalLightRegistry::default();
        registry.reconcile(provider.snapshot()).unwrap();
        let provider_before = provider.snapshot().source_revision();
        let registry_before = registry.registry_revision();

        let change = provider.remove_entities([second, first, first]).unwrap();
        assert_eq!(change.source_revision, provider_before + 1);
        assert_eq!((change.added, change.updated, change.removed), (0, 0, 2));
        registry.reconcile(provider.snapshot()).unwrap();
        assert_eq!(registry.registry_revision(), registry_before + 1);
        assert_eq!(provider.source_count(), 1);
        assert!(registry
            .light_id(
                RASTER_ENTITY_LIGHT_PROVIDER_ID,
                RasterEmitterKey::new(survivor, part).source_key(),
            )
            .is_some());
    }

    #[test]
    fn entity_generation_and_registry_generation_jointly_prevent_aba() {
        let old_entity = RasterEntityId::new(10, 7, 3);
        let replacement_entity = RasterEntityId::new(10, 7, 4);
        let part = RasterEmitterPartId::new(5);
        let mut provider = RasterEntityEmitterProvider::default();
        let mut registry = LocalLightRegistry::default();

        provider
            .publish_entity(old_entity, [(part, point(Vec3::X, 1.0))])
            .unwrap();
        registry.reconcile(provider.snapshot()).unwrap();
        let old_key = RasterEmitterKey::new(old_entity, part).source_key();
        let old_id = registry
            .light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, old_key)
            .unwrap();

        provider.remove_entity(old_entity).unwrap();
        registry.reconcile(provider.snapshot()).unwrap();
        provider
            .publish_entity(replacement_entity, [(part, point(Vec3::Y, 1.0))])
            .unwrap();
        registry.reconcile(provider.snapshot()).unwrap();
        let replacement_key = RasterEmitterKey::new(replacement_entity, part).source_key();
        let replacement_id = registry
            .light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, replacement_key)
            .unwrap();

        assert_ne!(old_key, replacement_key);
        assert_ne!(old_id, replacement_id);
        assert!(registry
            .light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, old_key)
            .is_none());
    }
}
