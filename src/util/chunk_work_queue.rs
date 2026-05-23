use std::collections::{HashMap, VecDeque};

use glam::{UVec3, Vec3};

const AGE_BONUS_PER_POP_CHUNKS: f32 = 0.5;

#[derive(Clone, Debug, Default)]
pub(crate) struct ChunkWorkQueue {
    queued: HashMap<UVec3, u64>,
    pending: VecDeque<UVec3>,
    queue_clock: u64,
    pop_clock: u64,
}

impl ChunkWorkQueue {
    pub(crate) fn push(&mut self, chunk_id: UVec3) -> bool {
        if self.queued.contains_key(&chunk_id) {
            return false;
        }

        self.queue_clock = self.queue_clock.wrapping_add(1);
        self.queued.insert(chunk_id, self.pop_clock);
        self.pending.push_back(chunk_id);
        true
    }

    #[cfg(test)]
    pub(crate) fn pop_next(&mut self) -> Option<UVec3> {
        while let Some(chunk_id) = self.pending.pop_front() {
            if self.queued.remove(&chunk_id).is_some() {
                self.pop_clock = self.pop_clock.wrapping_add(1);
                return Some(chunk_id);
            }
        }

        None
    }

    pub(crate) fn pop_nearest_to(&mut self, focus: Vec3, chunk_extent: UVec3) -> Option<UVec3> {
        self.pop_nearest_to_if(focus, chunk_extent, |_| true)
    }

    pub(crate) fn pop_nearest_to_if(
        &mut self,
        focus: Vec3,
        chunk_extent: UVec3,
        mut is_ready: impl FnMut(UVec3) -> bool,
    ) -> Option<UVec3> {
        let nearest_idx = self.nearest_pending_index_if(focus, chunk_extent, &mut is_ready)?;
        let aged_idx = self.aged_pending_index_if(focus, chunk_extent, &mut is_ready)?;
        let nearest_chunk_before_pop = self.pending.get(nearest_idx).copied();
        let chunk_idx = aged_idx;
        let chunk_id = self
            .pending
            .remove(chunk_idx)
            .expect("nearest pending chunk index should be valid");
        let queued_at = self
            .queued
            .remove(&chunk_id)
            .expect("popped chunk should still be queued");
        let age = self.pop_clock.saturating_sub(queued_at);

        if aged_idx != nearest_idx {
            if let Some(nearest_chunk) = nearest_chunk_before_pop {
                log::debug!(
                    "[QUEUE][AGING] selected {:?} age={} over nearest {:?} pending={} focus={:?}",
                    chunk_id,
                    age,
                    nearest_chunk,
                    self.queued.len(),
                    focus,
                );
            }
        }

        self.pop_clock = self.pop_clock.wrapping_add(1);
        Some(chunk_id)
    }

    fn nearest_pending_index_if(
        &self,
        focus: Vec3,
        chunk_extent: UVec3,
        mut is_ready: impl FnMut(UVec3) -> bool,
    ) -> Option<usize> {
        self.pending
            .iter()
            .enumerate()
            .filter(|(_, chunk_id)| self.queued.contains_key(chunk_id) && is_ready(**chunk_id))
            .min_by(|(_, left), (_, right)| {
                compare_chunk_nearness(**left, **right, focus, chunk_extent)
            })
            .map(|(idx, _)| idx)
    }

    fn aged_pending_index_if(
        &self,
        focus: Vec3,
        chunk_extent: UVec3,
        mut is_ready: impl FnMut(UVec3) -> bool,
    ) -> Option<usize> {
        self.pending
            .iter()
            .enumerate()
            .filter(|(_, chunk_id)| self.queued.contains_key(chunk_id) && is_ready(**chunk_id))
            .min_by(|(_, left), (_, right)| {
                compare_aged_chunk_priority(
                    **left,
                    **right,
                    focus,
                    chunk_extent,
                    self.chunk_age_pops(**left),
                    self.chunk_age_pops(**right),
                )
            })
            .map(|(idx, _)| idx)
    }

    fn chunk_age_pops(&self, chunk_id: UVec3) -> u64 {
        self.queued
            .get(&chunk_id)
            .map(|queued_at| self.pop_clock.saturating_sub(*queued_at))
            .unwrap_or(0)
    }

    pub(crate) fn remove(&mut self, chunk_id: UVec3) -> bool {
        if self.queued.remove(&chunk_id).is_none() {
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

fn compare_aged_chunk_priority(
    left: UVec3,
    right: UVec3,
    focus: Vec3,
    chunk_extent: UVec3,
    left_age_pops: u64,
    right_age_pops: u64,
) -> std::cmp::Ordering {
    let left_score = aged_chunk_priority_score(left, focus, chunk_extent, left_age_pops);
    let right_score = aged_chunk_priority_score(right, focus, chunk_extent, right_age_pops);

    left_score
        .total_cmp(&right_score)
        .then_with(|| right_age_pops.cmp(&left_age_pops))
        .then_with(|| left.x.cmp(&right.x))
        .then_with(|| left.y.cmp(&right.y))
        .then_with(|| left.z.cmp(&right.z))
}

fn aged_chunk_priority_score(
    chunk_id: UVec3,
    focus: Vec3,
    chunk_extent: UVec3,
    age_pops: u64,
) -> f32 {
    let distance_chunks = chunk_distance_squared(chunk_id, focus, chunk_extent).sqrt()
        / chunk_extent.as_vec3().max_element().max(1.0);
    distance_chunks - age_pops as f32 * AGE_BONUS_PER_POP_CHUNKS
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
    fn pop_nearest_orders_by_distance_to_chunk_center_for_young_chunks() {
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

    #[test]
    fn pop_nearest_if_skips_unready_chunks_without_removing_them() {
        let mut queue = ChunkWorkQueue::default();

        queue.push(chunk(1));
        queue.push(chunk(2));

        let focus = Vec3::new(1.25, 0.5, 0.5);

        assert_eq!(
            queue.pop_nearest_to_if(focus, UVec3::ONE, |chunk_id| chunk_id == chunk(2)),
            Some(chunk(2))
        );
        assert_eq!(queue.pop_nearest_to(focus, UVec3::ONE), Some(chunk(1)));
    }

    #[test]
    fn pop_nearest_if_returns_none_when_no_chunks_are_ready() {
        let mut queue = ChunkWorkQueue::default();

        queue.push(chunk(1));

        let focus = Vec3::new(1.25, 0.5, 0.5);

        assert_eq!(queue.pop_nearest_to_if(focus, UVec3::ONE, |_| false), None);
        assert_eq!(queue.pop_nearest_to(focus, UVec3::ONE), Some(chunk(1)));
    }

    #[test]
    fn old_far_chunk_eventually_beats_fresh_near_chunk() {
        let mut queue = ChunkWorkQueue::default();
        queue.push(chunk(10));
        queue.pop_clock = 30;
        queue.push(chunk(1));

        let focus = Vec3::new(1.25, 0.5, 0.5);

        assert_eq!(queue.pop_nearest_to(focus, UVec3::ONE), Some(chunk(10)));
    }

    #[test]
    fn duplicate_enqueue_preserves_original_age() {
        let mut queue = ChunkWorkQueue::default();
        queue.push(chunk(10));
        queue.pop_clock = 30;
        assert!(!queue.push(chunk(10)));
        queue.push(chunk(1));

        let focus = Vec3::new(1.25, 0.5, 0.5);

        assert_eq!(queue.pop_nearest_to(focus, UVec3::ONE), Some(chunk(10)));
    }
}
