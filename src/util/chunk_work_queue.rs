use std::collections::{HashSet, VecDeque};

use glam::UVec3;

#[derive(Clone, Debug, Default)]
pub(crate) struct ChunkWorkQueue {
    queued: HashSet<UVec3>,
    pending: VecDeque<UVec3>,
}

impl ChunkWorkQueue {
    pub(crate) fn push(&mut self, chunk_id: UVec3) -> bool {
        if !self.queued.insert(chunk_id) {
            return false;
        }

        self.pending.push_back(chunk_id);
        true
    }

    pub(crate) fn pop_next(&mut self) -> Option<UVec3> {
        while let Some(chunk_id) = self.pending.pop_front() {
            if self.queued.remove(&chunk_id) {
                return Some(chunk_id);
            }
        }

        None
    }

    pub(crate) fn remove(&mut self, chunk_id: UVec3) -> bool {
        if !self.queued.remove(&chunk_id) {
            return false;
        }

        self.pending
            .retain(|pending_chunk_id| *pending_chunk_id != chunk_id);
        true
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
        let mut queue = ChunkWorkQueue::default();

        assert!(queue.push(chunk(1)));
        assert_eq!(queue.pop_next(), Some(chunk(1)));
        assert!(queue.is_empty());
    }

    #[test]
    fn duplicate_push_keeps_one_entry() {
        let mut queue = ChunkWorkQueue::default();

        assert!(queue.push(chunk(1)));
        assert!(!queue.push(chunk(1)));

        assert_eq!(queue.len(), 1);
        assert_eq!(queue.pop_next(), Some(chunk(1)));
        assert_eq!(queue.pop_next(), None);
    }

    #[test]
    fn preserves_fifo_order() {
        let mut queue = ChunkWorkQueue::default();

        queue.push(chunk(1));
        queue.push(chunk(2));
        queue.push(chunk(3));

        assert_eq!(queue.pop_next(), Some(chunk(1)));
        assert_eq!(queue.pop_next(), Some(chunk(2)));
        assert_eq!(queue.pop_next(), Some(chunk(3)));
    }

    #[test]
    fn remove_drops_pending_entry() {
        let mut queue = ChunkWorkQueue::default();

        queue.push(chunk(1));
        queue.push(chunk(2));

        assert!(queue.remove(chunk(1)));
        assert_eq!(queue.pop_next(), Some(chunk(2)));
        assert_eq!(queue.pop_next(), None);
    }
}
