use std::collections::{HashMap, VecDeque};

use glam::{UVec3, Vec3};

use crate::util::compare_chunk_nearness;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GrowingFloraChunk {
    pub chunk_id: UVec3,
    pub last_flora_tick: u32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct GrowingFloraQueue {
    queued: HashMap<UVec3, u32>, // chunk_id -> last_flora_tick
    pending: VecDeque<UVec3>,
}

impl GrowingFloraQueue {
    /// push a chunk to the queue. If already queued, refreshes last_flora_tick to the new value.
    /// returns true if this is a new entry, false if it was already queued (tick refreshed).
    pub(crate) fn push(&mut self, chunk_id: UVec3, last_flora_tick: u32) -> bool {
        match self.queued.entry(chunk_id) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                // already queued, for example from a fresh edit while still pending
                // refresh tick so new damage gets the correct start point
                entry.insert(last_flora_tick);
                false
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(last_flora_tick);
                self.pending.push_back(chunk_id);
                true
            }
        }
    }

    /// pop the next chunk and its last tick. Returns None if empty.
    #[cfg(test)]
    pub(crate) fn pop_next(&mut self) -> Option<GrowingFloraChunk> {
        self.pop_with(|pending, queued| {
            while let Some(chunk_id) = pending.pop_front() {
                if queued.contains_key(&chunk_id) {
                    return Some(chunk_id);
                }
            }

            None
        })
    }

    pub(crate) fn pop_nearest_to(
        &mut self,
        focus: Vec3,
        chunk_extent: UVec3,
    ) -> Option<GrowingFloraChunk> {
        self.pop_with(|pending, queued| {
            let idx = pending
                .iter()
                .enumerate()
                .filter(|(_, chunk_id)| queued.contains_key(chunk_id))
                .min_by(|(_, left), (_, right)| {
                    compare_chunk_nearness(**left, **right, focus, chunk_extent)
                })
                .map(|(idx, _)| idx)?;

            pending.remove(idx)
        })
    }

    fn pop_with(
        &mut self,
        mut pop_chunk: impl FnMut(&mut VecDeque<UVec3>, &HashMap<UVec3, u32>) -> Option<UVec3>,
    ) -> Option<GrowingFloraChunk> {
        while let Some(chunk_id) = pop_chunk(&mut self.pending, &self.queued) {
            if let Some(last_tick) = self.queued.remove(&chunk_id) {
                return Some(GrowingFloraChunk {
                    chunk_id,
                    last_flora_tick: last_tick,
                });
            }
        }

        None
    }

    pub(crate) fn len(&self) -> usize {
        self.queued.len()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.queued.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(x: u32) -> UVec3 {
        UVec3::new(x, 0, 0)
    }

    #[test]
    fn push_then_pop() {
        let mut queue = GrowingFloraQueue::default();

        assert!(queue.push(chunk(1), 0));
        assert_eq!(
            queue.pop_next(),
            Some(GrowingFloraChunk {
                chunk_id: chunk(1),
                last_flora_tick: 0,
            })
        );
        assert!(queue.is_empty());
    }

    #[test]
    fn duplicate_push_refreshes_tick() {
        let mut queue = GrowingFloraQueue::default();

        assert!(queue.push(chunk(1), 10));
        assert!(!queue.push(chunk(1), 20)); // refresh, not duplicate

        assert_eq!(queue.len(), 1);
        let popped = queue.pop_next().unwrap();
        assert_eq!(popped.chunk_id, chunk(1));
        assert_eq!(popped.last_flora_tick, 20); // refreshed to latest
        assert!(queue.is_empty());
    }

    #[test]
    fn preserves_fifo_order() {
        let mut queue = GrowingFloraQueue::default();

        queue.push(chunk(1), 10);
        queue.push(chunk(2), 20);
        queue.push(chunk(3), 30);

        assert_eq!(queue.pop_next().unwrap().chunk_id, chunk(1));
        assert_eq!(queue.pop_next().unwrap().chunk_id, chunk(2));
        assert_eq!(queue.pop_next().unwrap().chunk_id, chunk(3));
    }

    #[test]
    fn push_while_pending_refreshes_tick() {
        let mut queue = GrowingFloraQueue::default();

        queue.push(chunk(1), 10);
        // simulate: chunk is still pending but user edits it again at tick 20
        queue.push(chunk(1), 20);

        assert_eq!(queue.len(), 1);
        let popped = queue.pop_next().unwrap();
        assert_eq!(popped.last_flora_tick, 20);
    }

    #[test]
    fn pop_nearest_orders_by_distance() {
        let mut queue = GrowingFloraQueue::default();

        queue.push(chunk(3), 30);
        queue.push(chunk(1), 10);
        queue.push(chunk(2), 20);

        let focus = Vec3::new(1.25, 0.5, 0.5);

        assert_eq!(
            queue.pop_nearest_to(focus, UVec3::ONE).unwrap().chunk_id,
            chunk(1)
        );
        assert_eq!(
            queue.pop_nearest_to(focus, UVec3::ONE).unwrap().chunk_id,
            chunk(2)
        );
        assert_eq!(
            queue.pop_nearest_to(focus, UVec3::ONE).unwrap().chunk_id,
            chunk(3)
        );
    }
}
