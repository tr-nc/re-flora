use std::collections::HashMap;

use glam::UVec3;
#[cfg(test)]
use glam::Vec3;

use crate::util::{ChunkPopMode, ChunkWorkQueue};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LatestChunkWork<T> {
    pub(crate) chunk_id: UVec3,
    pub(crate) revision: u64,
    pub(crate) payload: T,
}

#[derive(Clone, Debug)]
struct LatestChunkState<T> {
    latest_revision: u64,
    completed_revision: u64,
    active_revision: Option<u64>,
    latest_payload: Option<T>,
}

impl<T> Default for LatestChunkState<T> {
    fn default() -> Self {
        Self {
            latest_revision: 0,
            completed_revision: 0,
            active_revision: None,
            latest_payload: None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LatestChunkQueue<T> {
    states: HashMap<UVec3, LatestChunkState<T>>,
    pending: ChunkWorkQueue,
}

impl<T> Default for LatestChunkQueue<T> {
    fn default() -> Self {
        Self {
            states: HashMap::new(),
            pending: ChunkWorkQueue::default(),
        }
    }
}

impl<T> LatestChunkQueue<T> {
    pub(crate) fn push(&mut self, chunk_id: UVec3, payload: T) -> u64 {
        self.push_with(chunk_id, payload, |_, new_payload| new_payload)
    }

    pub(crate) fn push_coalesced(
        &mut self,
        chunk_id: UVec3,
        payload: T,
        coalesce: impl FnMut(T, T) -> T,
    ) -> u64 {
        self.push_with(chunk_id, payload, coalesce)
    }

    fn push_with(
        &mut self,
        chunk_id: UVec3,
        payload: T,
        mut merge_payload: impl FnMut(T, T) -> T,
    ) -> u64 {
        let state = self.states.entry(chunk_id).or_default();
        state.latest_revision += 1;
        state.latest_payload = Some(match state.latest_payload.take() {
            Some(existing_payload) => merge_payload(existing_payload, payload),
            None => payload,
        });

        if state.active_revision.is_none() {
            self.pending.push(chunk_id);
        }

        state.latest_revision
    }

    pub(crate) fn clear(&mut self, chunk_id: UVec3) -> u64 {
        let state = self.states.entry(chunk_id).or_default();
        state.latest_revision += 1;
        state.completed_revision = state.latest_revision;
        state.latest_payload = None;
        self.pending.remove(chunk_id);
        state.latest_revision
    }

    #[allow(dead_code)]
    pub(crate) fn pop_next(&mut self) -> Option<LatestChunkWork<T>> {
        self.pop(ChunkPopMode::Fifo)
    }

    pub(crate) fn pop(&mut self, mode: ChunkPopMode) -> Option<LatestChunkWork<T>> {
        self.pop_with(|pending| pending.pop(mode))
    }

    #[allow(dead_code)]
    pub(crate) fn pop_if(
        &mut self,
        mode: ChunkPopMode,
        mut is_ready: impl FnMut(UVec3) -> bool,
    ) -> Option<LatestChunkWork<T>> {
        self.pop_with(|pending| pending.pop_if(mode, &mut is_ready))
    }

    #[cfg(test)]
    pub(crate) fn pop_nearest_to(
        &mut self,
        focus: Vec3,
        chunk_extent: UVec3,
    ) -> Option<LatestChunkWork<T>> {
        self.pop(ChunkPopMode::NearestWithAging {
            focus,
            chunk_extent,
        })
    }

    #[cfg(test)]
    pub(crate) fn pop_nearest_to_if(
        &mut self,
        focus: Vec3,
        chunk_extent: UVec3,
        is_ready: impl FnMut(UVec3) -> bool,
    ) -> Option<LatestChunkWork<T>> {
        self.pop_if(
            ChunkPopMode::NearestWithAging {
                focus,
                chunk_extent,
            },
            is_ready,
        )
    }

    pub(crate) fn pop_if_payload(
        &mut self,
        mode: ChunkPopMode,
        mut is_ready: impl FnMut(UVec3, &T) -> bool,
    ) -> Option<LatestChunkWork<T>> {
        let states = &self.states;
        let chunk_id = self.pending.pop_if(mode, |chunk_id| {
            states.get(&chunk_id).is_some_and(|state| {
                state.active_revision.is_none()
                    && state.latest_revision > state.completed_revision
                    && state
                        .latest_payload
                        .as_ref()
                        .is_some_and(|payload| is_ready(chunk_id, payload))
            })
        })?;
        self.take_popped_chunk(chunk_id)
    }

    #[cfg(test)]
    pub(crate) fn pop_nearest_to_if_payload(
        &mut self,
        focus: Vec3,
        chunk_extent: UVec3,
        is_ready: impl FnMut(UVec3, &T) -> bool,
    ) -> Option<LatestChunkWork<T>> {
        self.pop_if_payload(
            ChunkPopMode::NearestWithAging {
                focus,
                chunk_extent,
            },
            is_ready,
        )
    }

    fn pop_with(
        &mut self,
        mut pop_chunk: impl FnMut(&mut ChunkWorkQueue) -> Option<UVec3>,
    ) -> Option<LatestChunkWork<T>> {
        while let Some(chunk_id) = pop_chunk(&mut self.pending) {
            if let Some(work) = self.take_popped_chunk(chunk_id) {
                return Some(work);
            }
        }

        None
    }

    fn take_popped_chunk(&mut self, chunk_id: UVec3) -> Option<LatestChunkWork<T>> {
        let state = self.states.get_mut(&chunk_id)?;
        if state.active_revision.is_some() || state.latest_revision <= state.completed_revision {
            return None;
        }

        let payload = state.latest_payload.take()?;
        let revision = state.latest_revision;
        state.active_revision = Some(revision);

        Some(LatestChunkWork {
            chunk_id,
            revision,
            payload,
        })
    }

    pub(crate) fn complete(&mut self, chunk_id: UVec3, revision: u64) {
        let Some(state) = self.states.get_mut(&chunk_id) else {
            return;
        };

        if state.active_revision == Some(revision) {
            state.active_revision = None;
        }
        state.completed_revision = state.completed_revision.max(revision);

        if state.latest_revision > revision {
            self.pending.push(chunk_id);
        } else if state.active_revision.is_none() {
            state.latest_payload = None;
        }
    }

    pub(crate) fn is_latest_revision(&self, chunk_id: UVec3, revision: u64) -> bool {
        self.states
            .get(&chunk_id)
            .map(|state| state.latest_revision == revision)
            .unwrap_or(false)
    }

    #[allow(dead_code)]
    pub(crate) fn has_unfinished_work(&self, chunk_id: UVec3) -> bool {
        self.states
            .get(&chunk_id)
            .map(|state| {
                state.active_revision.is_some() || state.latest_revision > state.completed_revision
            })
            .unwrap_or(false)
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.pending.is_empty()
            && self
                .states
                .values()
                .all(|state| state.active_revision.is_none())
    }

    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.pending.len()
    }

    pub(crate) fn active_len(&self) -> usize {
        self.states
            .values()
            .filter(|state| state.active_revision.is_some())
            .count()
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(x: u32) -> UVec3 {
        UVec3::new(x, 0, 0)
    }

    #[test]
    fn enqueue_then_pop() {
        let mut queue = LatestChunkQueue::default();
        let revision = queue.push(chunk(1), "a");

        let work = queue.pop_next().unwrap();

        assert_eq!(revision, 1);
        assert_eq!(work.chunk_id, chunk(1));
        assert_eq!(work.revision, 1);
        assert_eq!(work.payload, "a");
        assert!(queue.is_empty());
    }

    #[test]
    fn duplicate_enqueue_before_pop_keeps_latest_payload() {
        let mut queue = LatestChunkQueue::default();
        queue.push(chunk(1), "old");
        let revision = queue.push(chunk(1), "new");

        let work = queue.pop_next().unwrap();

        assert_eq!(revision, 2);
        assert_eq!(work.revision, 2);
        assert_eq!(work.payload, "new");
        assert!(queue.pop_next().is_none());
    }

    #[test]
    fn enqueue_while_active_requeues_latest_after_complete() {
        let mut queue = LatestChunkQueue::default();
        queue.push(chunk(1), "old");
        let old_work = queue.pop_next().unwrap();

        let new_revision = queue.push(chunk(1), "new");
        assert_eq!(new_revision, 2);
        assert!(queue.pop_next().is_none());

        queue.complete(old_work.chunk_id, old_work.revision);
        let new_work = queue.pop_next().unwrap();

        assert_eq!(new_work.revision, 2);
        assert_eq!(new_work.payload, "new");
    }

    #[test]
    fn completing_latest_work_marks_it_done() {
        let mut queue = LatestChunkQueue::default();
        queue.push(chunk(1), "a");
        let work = queue.pop_next().unwrap();

        queue.complete(work.chunk_id, work.revision);

        assert!(queue.pop_next().is_none());
        assert!(queue.is_empty());
        assert!(queue.is_idle());
    }

    #[test]
    fn clear_invalidates_active_work_without_requeue() {
        let mut queue = LatestChunkQueue::default();
        queue.push(chunk(1), "old");
        let work = queue.pop_next().unwrap();

        let clear_revision = queue.clear(chunk(1));
        queue.complete(work.chunk_id, work.revision);

        assert_eq!(clear_revision, 2);
        assert!(!queue.is_latest_revision(work.chunk_id, work.revision));
        assert!(queue.pop_next().is_none());
        assert!(queue.is_idle());
    }

    #[test]
    fn duplicate_enqueue_can_coalesce_payloads() {
        let mut queue = LatestChunkQueue::default();
        queue.push_coalesced(chunk(1), 10, u32::min);
        let revision = queue.push_coalesced(chunk(1), 20, u32::min);

        let work = queue.pop_next().unwrap();

        assert_eq!(revision, 2);
        assert_eq!(work.revision, 2);
        assert_eq!(work.payload, 10);
    }

    #[test]
    fn coalesced_enqueue_while_active_merges_waiting_payloads() {
        let mut queue = LatestChunkQueue::default();
        queue.push_coalesced(chunk(1), 30, u32::min);
        let active_work = queue.pop_next().unwrap();

        queue.push_coalesced(chunk(1), 20, u32::min);
        let revision = queue.push_coalesced(chunk(1), 10, u32::min);
        assert!(queue.pop_next().is_none());

        queue.complete(active_work.chunk_id, active_work.revision);
        let waiting_work = queue.pop_next().unwrap();

        assert_eq!(revision, 3);
        assert_eq!(waiting_work.revision, 3);
        assert_eq!(waiting_work.payload, 10);
    }

    #[test]
    fn duplicate_enqueue_preserves_fifo_order() {
        let mut queue = LatestChunkQueue::default();
        queue.push(chunk(1), "old-a");
        queue.push(chunk(2), "b");
        queue.push(chunk(1), "new-a");

        let first = queue.pop_next().unwrap();
        let second = queue.pop_next().unwrap();

        assert_eq!(first.chunk_id, chunk(1));
        assert_eq!(first.payload, "new-a");
        assert_eq!(second.chunk_id, chunk(2));
        assert_eq!(second.payload, "b");
    }

    #[test]
    fn pop_nearest_preserves_latest_payload() {
        let mut queue = LatestChunkQueue::default();
        queue.push(chunk(3), "c");
        queue.push(chunk(1), "old-a");
        queue.push(chunk(2), "b");
        queue.push(chunk(1), "new-a");

        let focus = Vec3::new(1.25, 0.5, 0.5);
        let first = queue.pop_nearest_to(focus, UVec3::ONE).unwrap();
        let second = queue.pop_nearest_to(focus, UVec3::ONE).unwrap();
        let third = queue.pop_nearest_to(focus, UVec3::ONE).unwrap();

        assert_eq!(first.chunk_id, chunk(1));
        assert_eq!(first.payload, "new-a");
        assert_eq!(second.chunk_id, chunk(2));
        assert_eq!(third.chunk_id, chunk(3));
    }

    #[test]
    fn pop_nearest_if_skips_unready_latest_work() {
        let mut queue = LatestChunkQueue::default();
        queue.push(chunk(1), "a");
        queue.push(chunk(2), "b");

        let focus = Vec3::new(1.25, 0.5, 0.5);
        let work = queue
            .pop_nearest_to_if(focus, UVec3::ONE, |chunk_id| chunk_id == chunk(2))
            .unwrap();

        assert_eq!(work.chunk_id, chunk(2));
        assert_eq!(work.payload, "b");
        queue.complete(work.chunk_id, work.revision);

        let remaining = queue.pop_nearest_to(focus, UVec3::ONE).unwrap();
        assert_eq!(remaining.chunk_id, chunk(1));
    }

    #[test]
    fn pop_nearest_payload_predicate_keeps_unready_work_pending() {
        let mut queue = LatestChunkQueue::default();
        queue.push(chunk(1), 10);
        queue.push(chunk(2), 20);

        let focus = Vec3::new(1.25, 0.5, 0.5);
        let work = queue
            .pop_nearest_to_if_payload(focus, UVec3::ONE, |_, payload| *payload == 20)
            .unwrap();

        assert_eq!(work.chunk_id, chunk(2));
        assert_eq!(work.payload, 20);
        queue.complete(work.chunk_id, work.revision);

        let remaining = queue.pop_nearest_to(focus, UVec3::ONE).unwrap();
        assert_eq!(remaining.chunk_id, chunk(1));
        assert_eq!(remaining.payload, 10);
    }

    #[test]
    fn has_unfinished_work_tracks_pending_and_active_revisions() {
        let mut queue = LatestChunkQueue::default();
        assert!(!queue.has_unfinished_work(chunk(1)));

        queue.push(chunk(1), "a");
        assert!(queue.has_unfinished_work(chunk(1)));

        let work = queue.pop_next().unwrap();
        assert!(queue.has_unfinished_work(chunk(1)));

        queue.complete(work.chunk_id, work.revision);
        assert!(!queue.has_unfinished_work(chunk(1)));
    }
}
