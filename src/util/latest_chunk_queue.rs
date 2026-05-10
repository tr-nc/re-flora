use std::collections::{HashMap, VecDeque};

use glam::UVec3;

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
    queued: bool,
    latest_payload: Option<T>,
}

impl<T> Default for LatestChunkState<T> {
    fn default() -> Self {
        Self {
            latest_revision: 0,
            completed_revision: 0,
            active_revision: None,
            queued: false,
            latest_payload: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LatestChunkQueue<T> {
    states: HashMap<UVec3, LatestChunkState<T>>,
    pending: VecDeque<UVec3>,
}

impl<T> LatestChunkQueue<T> {
    pub(crate) fn push(&mut self, chunk_id: UVec3, payload: T) -> u64 {
        let state = self.states.entry(chunk_id).or_default();
        state.latest_revision += 1;
        state.latest_payload = Some(payload);

        if state.active_revision.is_none() && !state.queued {
            state.queued = true;
            self.pending.push_back(chunk_id);
        }

        state.latest_revision
    }

    pub(crate) fn clear(&mut self, chunk_id: UVec3) -> u64 {
        let state = self.states.entry(chunk_id).or_default();
        state.latest_revision += 1;
        state.completed_revision = state.latest_revision;
        state.latest_payload = None;
        state.queued = false;
        state.latest_revision
    }

    pub(crate) fn pop_next(&mut self) -> Option<LatestChunkWork<T>> {
        while let Some(chunk_id) = self.pending.pop_front() {
            let Some(state) = self.states.get_mut(&chunk_id) else {
                continue;
            };
            state.queued = false;

            if state.active_revision.is_some() || state.latest_revision <= state.completed_revision
            {
                continue;
            }

            let Some(payload) = state.latest_payload.take() else {
                continue;
            };
            let revision = state.latest_revision;
            state.active_revision = Some(revision);

            return Some(LatestChunkWork {
                chunk_id,
                revision,
                payload,
            });
        }

        None
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
            if !state.queued {
                state.queued = true;
                self.pending.push_back(chunk_id);
            }
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
}
