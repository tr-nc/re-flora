#![cfg_attr(not(test), allow(dead_code))]

use std::{collections::BTreeMap, sync::Arc};

use super::{LightId, LocalLight, LocalLightMutationError, LocalLightRecord, LocalLightSnapshot};

pub(crate) const AUTHORED_LOCAL_LIGHT_PROVIDER_ID: ProviderId = ProviderId::new(1);

/// Stable identity for one authoritative local-light provider. Providers own world-unit
/// emitters; they do not know registry slots, GPU descriptors, selection, or DDGI transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ProviderId(u64);

impl ProviderId {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

/// Provider-local stable key. The two words allow coordinate/component keys without a lossy hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SourceLightKey {
    primary: u64,
    secondary: u64,
}

impl SourceLightKey {
    pub(crate) const fn new(primary: u64, secondary: u64) -> Self {
        Self { primary, secondary }
    }

    #[allow(dead_code)]
    pub(crate) const fn primary(self) -> u64 {
        self.primary
    }

    #[allow(dead_code)]
    pub(crate) const fn secondary(self) -> u64 {
        self.secondary
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LocalLightSourceId {
    provider: ProviderId,
    key: SourceLightKey,
}

impl LocalLightSourceId {
    pub(crate) const fn new(provider: ProviderId, key: SourceLightKey) -> Self {
        Self { provider, key }
    }

    pub(crate) const fn provider(self) -> ProviderId {
        self.provider
    }

    pub(crate) const fn key(self) -> SourceLightKey {
        self.key
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SourceLight {
    key: SourceLightKey,
    light: LocalLight,
}

impl SourceLight {
    pub(crate) const fn new(key: SourceLightKey, light: LocalLight) -> Self {
        Self { key, light }
    }

    pub(crate) const fn key(self) -> SourceLightKey {
        self.key
    }

    pub(crate) const fn light(self) -> LocalLight {
        self.light
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalLightProviderSnapshot {
    provider: ProviderId,
    source_revision: u64,
    sources: Arc<[SourceLight]>,
}

impl LocalLightProviderSnapshot {
    pub(crate) fn new(
        provider: ProviderId,
        source_revision: u64,
        sources: impl IntoIterator<Item = SourceLight>,
    ) -> Result<Self, LocalLightProviderSnapshotError> {
        let mut sources: Vec<_> = sources.into_iter().collect();
        sources.sort_by_key(|source| source.key);
        if let Some(pair) = sources.windows(2).find(|pair| pair[0].key == pair[1].key) {
            return Err(LocalLightProviderSnapshotError::DuplicateSourceKey {
                provider,
                key: pair[0].key,
            });
        }
        Ok(Self {
            provider,
            source_revision,
            sources: sources.into(),
        })
    }

    #[allow(dead_code)]
    pub(crate) const fn provider(&self) -> ProviderId {
        self.provider
    }

    pub(crate) const fn source_revision(&self) -> u64 {
        self.source_revision
    }

    pub(crate) fn sources(&self) -> &[SourceLight] {
        &self.sources
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalLightProviderSnapshotError {
    DuplicateSourceKey {
        provider: ProviderId,
        key: SourceLightKey,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalLightReconcileError {
    StaleSourceRevision {
        provider: ProviderId,
        current: u64,
        incoming: u64,
    },
    SourceRevisionCollision {
        provider: ProviderId,
        revision: u64,
    },
}

pub(crate) trait LocalLightSourceProvider: Clone {
    fn local_light_snapshot(&self) -> LocalLightProviderSnapshot;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LocalLightSourcePublicationError<E> {
    Provider(E),
    Reconcile(LocalLightReconcileError),
    Commit(LocalLightSourceCommitError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalLightSourceCommitError {
    StaleProvider {
        expected_revision: u64,
        current_revision: u64,
    },
    StaleRegistry {
        expected_publication_revision: u64,
        current_publication_revision: u64,
    },
}

/// A fully validated provider mutation and registry reconciliation. Dropping this value leaves
/// both live owners unchanged; committing installs both candidates in one infallible operation.
pub(crate) struct PreparedLocalLightSourcePublication<P, T> {
    base_provider_snapshot: LocalLightProviderSnapshot,
    base_registry_publication_revision: u64,
    provider: P,
    registry: LocalLightRegistry,
    change: T,
    reconcile: LocalLightReconcileOutcome,
}

impl<P: LocalLightSourceProvider, T> PreparedLocalLightSourcePublication<P, T> {
    pub(crate) const fn change(&self) -> &T {
        &self.change
    }

    pub(crate) fn light_id(&self, provider: ProviderId, key: SourceLightKey) -> Option<LightId> {
        self.registry.light_id(provider, key)
    }

    pub(crate) fn commit(
        self,
        provider: &mut P,
        registry: &mut LocalLightRegistry,
    ) -> Result<LocalLightSourcePublication<T>, LocalLightSourceCommitError> {
        let current_provider_snapshot = provider.local_light_snapshot();
        if current_provider_snapshot != self.base_provider_snapshot {
            return Err(LocalLightSourceCommitError::StaleProvider {
                expected_revision: self.base_provider_snapshot.source_revision(),
                current_revision: current_provider_snapshot.source_revision(),
            });
        }
        if registry.source_publication_revision != self.base_registry_publication_revision {
            return Err(LocalLightSourceCommitError::StaleRegistry {
                expected_publication_revision: self.base_registry_publication_revision,
                current_publication_revision: registry.source_publication_revision,
            });
        }
        *provider = self.provider;
        *registry = self.registry;
        Ok(LocalLightSourcePublication {
            change: self.change,
            reconcile: self.reconcile,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalLightSourcePublication<T> {
    pub change: T,
    pub reconcile: LocalLightReconcileOutcome,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LocalLightReconcileOutcome {
    pub provider: Option<ProviderId>,
    pub source_revision: u64,
    pub registry_revision: u64,
    pub provider_source_count: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

#[derive(Clone, Debug)]
struct RegistrySlot {
    generation: u32,
    source: Option<LocalLightSourceId>,
    light: Option<LocalLight>,
}

#[derive(Clone, Debug)]
struct ProviderState {
    source_revision: u64,
    sources: Arc<[SourceLight]>,
}

/// Central authority that assigns stable slot+generation IDs to provider-local source keys.
/// Reconciliation is deterministic and publishes one immutable registry snapshot atomically.
#[derive(Clone, Debug, Default)]
pub(crate) struct LocalLightRegistry {
    registry_revision: u64,
    source_publication_revision: u64,
    providers: BTreeMap<ProviderId, ProviderState>,
    by_source: BTreeMap<LocalLightSourceId, LightId>,
    slots: Vec<RegistrySlot>,
    snapshot: LocalLightSnapshot,
    authored_next_key: u64,
    authored_source_revision: u64,
}

impl LocalLightRegistry {
    /// Convenience lifecycle for explicitly authored lights. Other producers publish immutable
    /// provider snapshots through `reconcile`; this adapter is itself just provider 1.
    pub(crate) fn add(&mut self, light: LocalLight) -> LightId {
        self.authored_next_key = self.authored_next_key.wrapping_add(1).max(1);
        let key = SourceLightKey::new(self.authored_next_key, 0);
        let mut sources = self.authored_sources();
        sources.push(SourceLight::new(key, light));
        self.publish_authored(sources);
        self.light_id(AUTHORED_LOCAL_LIGHT_PROVIDER_ID, key)
            .expect("authored light reconcile must publish the new source")
    }

    pub(crate) fn update(
        &mut self,
        id: LightId,
        light: LocalLight,
    ) -> Result<(), LocalLightMutationError> {
        let source = self.authored_source_for_id(id)?;
        let mut sources = self.authored_sources();
        let entry = sources
            .iter_mut()
            .find(|entry| entry.key == source.key)
            .expect("live authored source must exist in its provider snapshot");
        if entry.light == light {
            return Ok(());
        }
        entry.light = light;
        self.publish_authored(sources);
        Ok(())
    }

    pub(crate) fn remove(&mut self, id: LightId) -> Result<(), LocalLightMutationError> {
        let source = self.authored_source_for_id(id)?;
        let mut sources = self.authored_sources();
        sources.retain(|entry| entry.key != source.key);
        self.publish_authored(sources);
        Ok(())
    }

    pub(crate) fn prepare_source_publication<P, T, E>(
        &self,
        provider: &P,
        publish: impl FnOnce(&mut P) -> Result<T, E>,
    ) -> Result<PreparedLocalLightSourcePublication<P, T>, LocalLightSourcePublicationError<E>>
    where
        P: LocalLightSourceProvider,
    {
        let base_provider_snapshot = provider.local_light_snapshot();
        let mut candidate_provider = provider.clone();
        let change =
            publish(&mut candidate_provider).map_err(LocalLightSourcePublicationError::Provider)?;
        let mut candidate_registry = self.clone();
        let reconcile = candidate_registry
            .reconcile(candidate_provider.local_light_snapshot())
            .map_err(LocalLightSourcePublicationError::Reconcile)?;
        Ok(PreparedLocalLightSourcePublication {
            base_provider_snapshot,
            base_registry_publication_revision: self.source_publication_revision,
            provider: candidate_provider,
            registry: candidate_registry,
            change,
            reconcile,
        })
    }

    pub(crate) fn publish_source<P, T, E>(
        &mut self,
        provider: &mut P,
        publish: impl FnOnce(&mut P) -> Result<T, E>,
    ) -> Result<LocalLightSourcePublication<T>, LocalLightSourcePublicationError<E>>
    where
        P: LocalLightSourceProvider,
    {
        let prepared = self.prepare_source_publication(provider, publish)?;
        prepared
            .commit(provider, self)
            .map_err(LocalLightSourcePublicationError::Commit)
    }

    pub(crate) fn reconcile(
        &mut self,
        incoming: LocalLightProviderSnapshot,
    ) -> Result<LocalLightReconcileOutcome, LocalLightReconcileError> {
        let provider = incoming.provider;
        if let Some(current) = self.providers.get(&provider) {
            if incoming.source_revision < current.source_revision {
                return Err(LocalLightReconcileError::StaleSourceRevision {
                    provider,
                    current: current.source_revision,
                    incoming: incoming.source_revision,
                });
            }
            if incoming.source_revision == current.source_revision {
                if incoming.sources == current.sources {
                    return Ok(self.outcome(provider, incoming.source_revision, 0, 0, 0));
                }
                return Err(LocalLightReconcileError::SourceRevisionCollision {
                    provider,
                    revision: incoming.source_revision,
                });
            }
        }

        let current: BTreeMap<_, _> = self
            .providers
            .get(&provider)
            .into_iter()
            .flat_map(|state| state.sources.iter().copied())
            .map(|source| (source.key, source.light))
            .collect();
        let next: BTreeMap<_, _> = incoming
            .sources
            .iter()
            .copied()
            .map(|source| (source.key, source.light))
            .collect();

        let removed_keys: Vec<_> = current
            .keys()
            .filter(|key| !next.contains_key(key))
            .copied()
            .collect();
        let added_keys: Vec<_> = next
            .keys()
            .filter(|key| !current.contains_key(key))
            .copied()
            .collect();
        let updated_keys: Vec<_> = next
            .iter()
            .filter_map(|(key, light)| (current.get(key) != Some(light)).then_some(*key))
            .filter(|key| current.contains_key(key))
            .collect();

        for key in &removed_keys {
            self.release(LocalLightSourceId::new(provider, *key));
        }
        for key in &updated_keys {
            let source = LocalLightSourceId::new(provider, *key);
            let id = self.by_source[&source];
            self.slots[id.slot as usize].light = Some(next[key]);
        }
        for key in &added_keys {
            self.allocate(LocalLightSourceId::new(provider, *key), next[key]);
        }

        self.source_publication_revision = self.source_publication_revision.wrapping_add(1).max(1);
        self.providers.insert(
            provider,
            ProviderState {
                source_revision: incoming.source_revision,
                sources: incoming.sources,
            },
        );
        if !removed_keys.is_empty() || !updated_keys.is_empty() || !added_keys.is_empty() {
            self.publish_snapshot();
        } else {
            self.snapshot.source_revision = self.source_publication_revision;
        }
        Ok(self.outcome(
            provider,
            incoming.source_revision,
            added_keys.len(),
            updated_keys.len(),
            removed_keys.len(),
        ))
    }

    pub(crate) fn remove_provider(&mut self, provider: ProviderId) -> LocalLightReconcileOutcome {
        let Some(state) = self.providers.remove(&provider) else {
            return self.outcome(provider, 0, 0, 0, 0);
        };
        let removed = state.sources.len();
        for source in state.sources.iter() {
            self.release(LocalLightSourceId::new(provider, source.key));
        }
        self.source_publication_revision = self.source_publication_revision.wrapping_add(1).max(1);
        if removed > 0 {
            self.publish_snapshot();
        }
        self.outcome(provider, state.source_revision, 0, 0, removed)
    }

    pub(crate) const fn registry_revision(&self) -> u64 {
        self.registry_revision
    }

    #[allow(dead_code)]
    pub(crate) const fn source_publication_revision(&self) -> u64 {
        self.source_publication_revision
    }

    pub(crate) fn light_id(&self, provider: ProviderId, key: SourceLightKey) -> Option<LightId> {
        self.by_source
            .get(&LocalLightSourceId::new(provider, key))
            .copied()
    }

    pub(crate) fn snapshot(&self) -> LocalLightSnapshot {
        self.snapshot.clone()
    }

    fn allocate(&mut self, source: LocalLightSourceId, light: LocalLight) -> LightId {
        let slot = self
            .slots
            .iter()
            .position(|slot| slot.light.is_none())
            .unwrap_or_else(|| {
                self.slots.push(RegistrySlot {
                    generation: 1,
                    source: None,
                    light: None,
                });
                self.slots.len() - 1
            });
        let entry = &mut self.slots[slot];
        let id = LightId {
            slot: slot as u32,
            generation: entry.generation,
        };
        entry.source = Some(source);
        entry.light = Some(light);
        self.by_source.insert(source, id);
        id
    }

    fn authored_sources(&self) -> Vec<SourceLight> {
        self.providers
            .get(&AUTHORED_LOCAL_LIGHT_PROVIDER_ID)
            .map_or_else(Vec::new, |state| state.sources.to_vec())
    }

    fn authored_source_for_id(
        &self,
        id: LightId,
    ) -> Result<LocalLightSourceId, LocalLightMutationError> {
        let Some(slot) = self.slots.get(id.slot as usize) else {
            return Err(LocalLightMutationError::StaleId(id));
        };
        let Some(source) = slot
            .source
            .filter(|_| slot.generation == id.generation && slot.light.is_some())
        else {
            return Err(LocalLightMutationError::StaleId(id));
        };
        if source.provider != AUTHORED_LOCAL_LIGHT_PROVIDER_ID {
            return Err(LocalLightMutationError::StaleId(id));
        }
        Ok(source)
    }

    fn publish_authored(&mut self, sources: Vec<SourceLight>) {
        self.authored_source_revision = self.authored_source_revision.wrapping_add(1).max(1);
        let snapshot = LocalLightProviderSnapshot::new(
            AUTHORED_LOCAL_LIGHT_PROVIDER_ID,
            self.authored_source_revision,
            sources,
        )
        .expect("authored light keys are unique by construction");
        self.reconcile(snapshot)
            .expect("authored light source revisions are monotonic");
    }

    fn release(&mut self, source: LocalLightSourceId) {
        let Some(id) = self.by_source.remove(&source) else {
            return;
        };
        let slot = &mut self.slots[id.slot as usize];
        debug_assert_eq!(slot.source, Some(source));
        slot.source = None;
        slot.light = None;
        slot.generation = slot.generation.wrapping_add(1).max(1);
    }

    fn publish_snapshot(&mut self) {
        self.registry_revision = self.registry_revision.wrapping_add(1).max(1);
        let lights: Vec<_> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| {
                Some(LocalLightRecord {
                    id: LightId {
                        slot: slot as u32,
                        generation: entry.generation,
                    },
                    source: entry.source?,
                    light: entry.light?,
                })
            })
            .collect();
        self.snapshot = LocalLightSnapshot {
            source_revision: self.source_publication_revision,
            registry_revision: self.registry_revision,
            lights: lights.into(),
        };
    }

    fn outcome(
        &self,
        provider: ProviderId,
        source_revision: u64,
        added: usize,
        updated: usize,
        removed: usize,
    ) -> LocalLightReconcileOutcome {
        LocalLightReconcileOutcome {
            provider: Some(provider),
            source_revision,
            registry_revision: self.registry_revision,
            provider_source_count: self
                .providers
                .get(&provider)
                .map_or(0, |state| state.sources.len()),
            added,
            updated,
            removed,
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;
    use crate::lighting::PointLight;

    const TEST_PROVIDER_ID: ProviderId = ProviderId::new(91);
    const TEST_SOURCE_KEY: SourceLightKey = SourceLightKey::new(7, 11);

    #[derive(Clone, Debug, PartialEq)]
    struct TestProvider {
        snapshot: LocalLightProviderSnapshot,
    }

    impl Default for TestProvider {
        fn default() -> Self {
            Self {
                snapshot: LocalLightProviderSnapshot::new(TEST_PROVIDER_ID, 0, []).unwrap(),
            }
        }
    }

    impl LocalLightSourceProvider for TestProvider {
        fn local_light_snapshot(&self) -> LocalLightProviderSnapshot {
            self.snapshot.clone()
        }
    }

    impl TestProvider {
        fn replace(
            &mut self,
            sources: impl IntoIterator<Item = SourceLight>,
        ) -> Result<bool, LocalLightProviderSnapshotError> {
            let candidate = LocalLightProviderSnapshot::new(
                TEST_PROVIDER_ID,
                self.snapshot.source_revision().wrapping_add(1).max(1),
                sources,
            )?;
            if candidate.sources() == self.snapshot.sources() {
                return Ok(false);
            }
            self.snapshot = candidate;
            Ok(true)
        }
    }

    fn point(x: f32, intensity: f32) -> LocalLight {
        LocalLight::Point(
            PointLight::new(Vec3::new(x, 0.0, 0.0), Vec3::ONE, intensity, 0.01, 1.0).unwrap(),
        )
    }

    #[test]
    fn source_publication_is_atomic_for_add_update_remove_noop_and_drop() {
        let mut provider = TestProvider::default();
        let mut registry = LocalLightRegistry::default();
        let original_provider = provider.clone();
        let original_registry = registry.snapshot();

        let dropped = registry
            .prepare_source_publication(&provider, |candidate| {
                candidate.replace([SourceLight::new(TEST_SOURCE_KEY, point(0.0, 1.0))])
            })
            .unwrap();
        assert!(*dropped.change());
        drop(dropped);
        assert_eq!(provider, original_provider);
        assert_eq!(registry.snapshot(), original_registry);

        let added = registry
            .publish_source(&mut provider, |candidate| {
                candidate.replace([SourceLight::new(TEST_SOURCE_KEY, point(0.0, 1.0))])
            })
            .unwrap();
        assert!(added.change);
        assert_eq!((added.reconcile.added, added.reconcile.updated), (1, 0));
        let stable_id = registry
            .light_id(TEST_PROVIDER_ID, TEST_SOURCE_KEY)
            .unwrap();
        let provider_revision = provider.snapshot.source_revision();
        let registry_revision = registry.registry_revision();
        let publication_revision = registry.source_publication_revision();

        let noop = registry
            .publish_source(&mut provider, |candidate| {
                candidate.replace([SourceLight::new(TEST_SOURCE_KEY, point(0.0, 1.0))])
            })
            .unwrap();
        assert!(!noop.change);
        assert_eq!(provider.snapshot.source_revision(), provider_revision);
        assert_eq!(registry.registry_revision(), registry_revision);
        assert_eq!(registry.source_publication_revision(), publication_revision);
        assert_eq!(
            registry.light_id(TEST_PROVIDER_ID, TEST_SOURCE_KEY),
            Some(stable_id)
        );

        let updated = registry
            .publish_source(&mut provider, |candidate| {
                candidate.replace([SourceLight::new(TEST_SOURCE_KEY, point(1.0, 2.0))])
            })
            .unwrap();
        assert_eq!(updated.reconcile.updated, 1);
        assert_eq!(
            registry.light_id(TEST_PROVIDER_ID, TEST_SOURCE_KEY),
            Some(stable_id)
        );

        let removed = registry
            .publish_source(&mut provider, |candidate| candidate.replace([]))
            .unwrap();
        assert_eq!(removed.reconcile.removed, 1);
        assert!(registry
            .light_id(TEST_PROVIDER_ID, TEST_SOURCE_KEY)
            .is_none());

        registry
            .publish_source(&mut provider, |candidate| {
                candidate.replace([SourceLight::new(TEST_SOURCE_KEY, point(2.0, 3.0))])
            })
            .unwrap();
        assert_ne!(
            registry.light_id(TEST_PROVIDER_ID, TEST_SOURCE_KEY),
            Some(stable_id)
        );
    }

    #[test]
    fn source_publication_rejects_provider_errors_and_stale_prepared_state_without_mutation() {
        let mut provider = TestProvider::default();
        let mut registry = LocalLightRegistry::default();
        let before_provider = provider.clone();
        let before_registry = registry.snapshot();
        let error = registry.prepare_source_publication(&provider, |_candidate| {
            Err::<(), _>("provider rejected publication")
        });
        assert!(matches!(
            error,
            Err(LocalLightSourcePublicationError::Provider(
                "provider rejected publication"
            ))
        ));
        assert_eq!(provider, before_provider);
        assert_eq!(registry.snapshot(), before_registry);

        let stale_provider = registry
            .prepare_source_publication(&provider, |candidate| {
                candidate.replace([SourceLight::new(TEST_SOURCE_KEY, point(0.0, 1.0))])
            })
            .unwrap();
        provider
            .replace([SourceLight::new(TEST_SOURCE_KEY, point(3.0, 4.0))])
            .unwrap();
        let provider_after_interleave = provider.clone();
        let registry_before_stale_commit = registry.snapshot();
        assert!(matches!(
            stale_provider.commit(&mut provider, &mut registry),
            Err(LocalLightSourceCommitError::StaleProvider { .. })
        ));
        assert_eq!(provider, provider_after_interleave);
        assert_eq!(registry.snapshot(), registry_before_stale_commit);

        let mut provider = TestProvider::default();
        let stale_registry = registry
            .prepare_source_publication(&provider, |candidate| {
                candidate.replace([SourceLight::new(TEST_SOURCE_KEY, point(0.0, 1.0))])
            })
            .unwrap();
        registry.add(point(9.0, 1.0));
        let provider_before_stale_commit = provider.clone();
        let registry_after_interleave = registry.snapshot();
        assert!(matches!(
            stale_registry.commit(&mut provider, &mut registry),
            Err(LocalLightSourceCommitError::StaleRegistry { .. })
        ));
        assert_eq!(provider, provider_before_stale_commit);
        assert_eq!(registry.snapshot(), registry_after_interleave);
    }

    #[test]
    fn authored_noop_and_stale_id_leave_the_registry_unchanged() {
        let mut registry = LocalLightRegistry::default();
        let light = point(0.0, 1.0);
        let id = registry.add(light);
        let revision = registry.registry_revision();
        let publication_revision = registry.source_publication_revision();
        registry.update(id, light).unwrap();
        assert_eq!(registry.registry_revision(), revision);
        assert_eq!(registry.source_publication_revision(), publication_revision);

        registry.remove(id).unwrap();
        let after_remove = registry.snapshot();
        assert_eq!(
            registry.update(id, point(1.0, 2.0)),
            Err(LocalLightMutationError::StaleId(id))
        );
        assert_eq!(
            registry.remove(id),
            Err(LocalLightMutationError::StaleId(id))
        );
        assert_eq!(registry.snapshot(), after_remove);
    }
}
