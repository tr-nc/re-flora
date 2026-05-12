use std::collections::{HashSet, VecDeque};

use glam::{UVec3, Vec3};

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

    #[cfg(test)]
    pub(crate) fn pop_next(&mut self) -> Option<UVec3> {
        while let Some(chunk_id) = self.pending.pop_front() {
            if self.queued.remove(&chunk_id) {
                return Some(chunk_id);
            }
        }

        None
    }

    pub(crate) fn pop_nearest_to(&mut self, focus: Vec3, chunk_extent: UVec3) -> Option<UVec3> {
        let chunk_idx = self.nearest_pending_index(focus, chunk_extent)?;
        let chunk_id = self
            .pending
            .remove(chunk_idx)
            .expect("nearest pending chunk index should be valid");
        self.queued.remove(&chunk_id);
        Some(chunk_id)
    }

    pub(crate) fn peek_nearest_to(&self, focus: Vec3, chunk_extent: UVec3) -> Option<UVec3> {
        self.nearest_pending_index(focus, chunk_extent)
            .and_then(|idx| self.pending.get(idx).copied())
    }

    fn nearest_pending_index(&self, focus: Vec3, chunk_extent: UVec3) -> Option<usize> {
        self.pending
            .iter()
            .enumerate()
            .filter(|(_, chunk_id)| self.queued.contains(chunk_id))
            .min_by(|(_, left), (_, right)| {
                compare_chunk_nearness(**left, **right, focus, chunk_extent)
            })
            .map(|(idx, _)| idx)
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

pub(crate) fn compare_chunk_nearness(
    left: UVec3,
    right: UVec3,
    focus: Vec3,
    chunk_extent: UVec3,
) -> std::cmp::Ordering {
    let left_dist = chunk_distance_squared(left, focus, chunk_extent);
    let right_dist = chunk_distance_squared(right, focus, chunk_extent);

    left_dist
        .total_cmp(&right_dist)
        .then_with(|| left.x.cmp(&right.x))
        .then_with(|| left.y.cmp(&right.y))
        .then_with(|| left.z.cmp(&right.z))
}

fn chunk_distance_squared(chunk_id: UVec3, focus: Vec3, chunk_extent: UVec3) -> f32 {
    let extent = chunk_extent.as_vec3();
    let center = chunk_id.as_vec3() * extent + extent * 0.5;
    center.distance_squared(focus)
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

    #[test]
    fn pop_nearest_orders_by_distance_to_chunk_center() {
        let mut queue = ChunkWorkQueue::default();

        queue.push(chunk(3));
        queue.push(chunk(1));
        queue.push(chunk(2));

        let focus = Vec3::new(1.25, 0.5, 0.5);
        let chunk_extent = UVec3::ONE;

        assert_eq!(queue.pop_nearest_to(focus, chunk_extent), Some(chunk(1)));
        assert_eq!(queue.pop_nearest_to(focus, chunk_extent), Some(chunk(2)));
        assert_eq!(queue.pop_nearest_to(focus, chunk_extent), Some(chunk(3)));
    }
}
