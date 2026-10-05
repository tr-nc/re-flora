use crate::{DescriptorPool, DescriptorSet, DescriptorSetLayout};
use anyhow::{Context, Result};

#[derive(Default)]
pub(super) struct TransientDescriptorSets {
    active_frame_slot: Option<usize>,
    next_slot: usize,
    frame_slots: Vec<TransientDescriptorFrame>,
}

struct TransientDescriptorFrame<T = TransientDescriptorSlot> {
    slots: Vec<T>,
    used: usize,
}
impl<T> Default for TransientDescriptorFrame<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            used: 0,
        }
    }
}
impl<T> TransientDescriptorFrame<T> {
    /// This slot's fence has completed. Drop unused tail descriptors (and their
    /// resource owners), including all sets after an idle bake frame. Keeping a
    /// high-water tail forever pins replaced geometry/direction buffers.
    fn begin(&mut self) {
        self.slots.truncate(self.used);
        self.used = 0;
    }

    /// Only sets already used in this recording may be shared. A cached set
    /// from the previous fence cycle must first claim a new recording slot.
    fn previous_active(&self) -> Option<&T> {
        self.slots.get(self.used.checked_sub(1)?)
    }
}

struct TransientDescriptorSlot {
    set_no: u32,
    descriptor_set: DescriptorSet,
}

impl TransientDescriptorSets {
    pub(super) fn begin_frame(&mut self, frame_slot: usize) {
        if self.frame_slots.len() <= frame_slot {
            self.frame_slots
                .resize_with(frame_slot + 1, TransientDescriptorFrame::default);
        }
        self.frame_slots[frame_slot].begin();
        self.active_frame_slot = Some(frame_slot);
        self.next_slot = 0;
    }

    pub(super) fn previous_descriptor_set(&self, set_no: u32) -> Option<DescriptorSet> {
        let frame = self.frame_slots.get(self.active_frame_slot?)?;
        let slot = frame.previous_active()?;
        (slot.set_no == set_no).then(|| slot.descriptor_set.clone())
    }

    pub(super) fn next_descriptor_set(
        &mut self,
        set_no: u32,
        descriptor_pool: &DescriptorPool,
        layout: &DescriptorSetLayout,
        pipeline_name: &str,
    ) -> Result<DescriptorSet> {
        let frame_slot = self.active_frame_slot.unwrap_or_else(|| {
            panic!(
                "{pipeline_name}::begin_transient_descriptor_frame must be called before recording with transient descriptors"
            )
        });
        let draw_slot = self.next_slot;
        self.next_slot += 1;

        let frame = self
            .frame_slots
            .get_mut(frame_slot)
            .expect("active manual descriptor frame slot was not initialized");
        frame.used = self.next_slot;
        if let Some(slot) = frame.slots.get(draw_slot) {
            if slot.set_no == set_no {
                return Ok(slot.descriptor_set.clone());
            }
        }

        let descriptor_set = descriptor_pool.allocate_set(layout).with_context(|| {
            format!(
                "failed to allocate transient descriptor set for pipeline={pipeline_name} frame_slot={frame_slot} draw_slot={draw_slot} set={set_no}"
            )
        })?;
        let slot = TransientDescriptorSlot {
            set_no,
            descriptor_set: descriptor_set.clone(),
        };
        if draw_slot == frame.slots.len() {
            frame.slots.push(slot);
        } else {
            frame.slots[draw_slot] = slot;
        }

        Ok(descriptor_set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn previous_active_set_never_crosses_frame_or_unclaimed_slots() {
        let mut frame = TransientDescriptorFrame {
            slots: vec![11, 22, 33],
            used: 0,
        };
        assert_eq!(frame.previous_active(), None);
        frame.used = 1;
        assert_eq!(frame.previous_active(), Some(&11));
        frame.used = 2;
        assert_eq!(frame.previous_active(), Some(&22));
        frame.begin();
        assert_eq!(frame.slots, vec![11, 22]);
        assert_eq!(frame.previous_active(), None);
    }

    #[test]
    fn idle_bakes_and_shrinking_draws_release_owners_only_in_ready_slots() {
        let old = Arc::new(());
        let weak = Arc::downgrade(&old);
        let mut frames: [TransientDescriptorFrame<Arc<()>>; 2] =
            std::array::from_fn(|_| TransientDescriptorFrame {
                slots: vec![old.clone(); 3],
                used: 3,
            });
        drop(old);
        // Slot 0 becomes ready, then uses just one descriptor with new resources.
        frames[0].begin();
        frames[0].slots[0] = Arc::new(());
        frames[0].used = 1;
        frames[0].begin();
        assert_eq!(frames[0].slots.len(), 1);
        assert!(
            weak.upgrade().is_some(),
            "pending slot still owns old resources"
        );
        // Slot 1 becomes ready and records no bake. Its next ready cycle trims
        // all idle sets rather than pinning their source/direction buffers forever.
        frames[1].begin();
        assert!(weak.upgrade().is_some());
        frames[1].begin();
        assert!(frames[1].slots.is_empty());
        assert!(weak.upgrade().is_none());
    }
}
