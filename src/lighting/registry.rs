use std::{collections::BTreeMap, sync::Arc};

use super::{LightId, LocalLight, LocalLightRecord, LocalLightSnapshot};

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

    pub(crate) const fn primary(self) -> u64 {
        self.primary
    }

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
}

impl LocalLightRegistry {
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
                    source: Some(entry.source?),
                    light: entry.light?,
                })
            })
            .collect();
        self.snapshot = LocalLightSnapshot {
            revision: self.registry_revision,
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
