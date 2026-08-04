use crate::{DescriptorPool, DescriptorSet, DescriptorSetLayout};
use anyhow::{Context, Result};

#[derive(Default)]
pub(super) struct TransientDescriptorSets {
    active_frame_slot: Option<usize>,
    next_slot: usize,
    frame_slots: Vec<TransientDescriptorFrame>,
}

#[derive(Default)]
struct TransientDescriptorFrame {
    slots: Vec<TransientDescriptorSlot>,
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
        self.active_frame_slot = Some(frame_slot);
        self.next_slot = 0;
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
