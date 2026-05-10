use std::collections::{HashMap, VecDeque};

use glam::UVec3;

#[derive(Clone, Debug)]
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
    pub(crate) fn push(&mut self, chunk_id: UVec3, last_flora_tick: u32) -> bool {
        match self.queued.entry(chunk_id) {
            std::collections::hash_map::Entry::Occupied(_) => false,
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(last_flora_tick);
                self.pending.push_back(chunk_id);
                true
            }
        }
    }

    /// Pop the next chunk and its last tick. Returns None if empty.
    pub(crate) fn pop_next(&mut self) -> Option<GrowingFloraChunk> {
        while let Some(chunk_id) = self.pending.pop_front() {
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
    fn duplicate_push_keeps_one_entry() {
        let mut queue = GrowingFloraQueue::default();

        assert!(queue.push(chunk(1), 0));
        assert!(!queue.push(chunk(1), 5)); // different tick but same chunk, should be ignored

        assert_eq!(queue.len(), 1);
        let popped = queue.pop_next().unwrap();
        assert_eq!(popped.chunk_id, chunk(1));
        assert_eq!(popped.last_flora_tick, 0); // first tick kept
        assert_eq!(queue.pop_next(), None);
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
}