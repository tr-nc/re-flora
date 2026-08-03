use crate::{
    Buffer, BufferState, BufferUse, CommandBuffer, Image, ResourceState, TextureTransition,
};
use ash::vk;

/// Resource states tentatively produced while one command buffer is being recorded.
///
/// The Vulkan barriers are recorded immediately, but the host-side state is not committed until
/// the command buffer is accepted by `queue_submit`. This keeps abandoned and failed recordings
/// from poisoning the next recording's source layout.
pub(crate) struct ResourceStateTransaction {
    images: Vec<TrackedImageState>,
    buffers: Vec<TrackedBufferState>,
}

struct TrackedImageState {
    image: Image,
    initial: Vec<ResourceState>,
    current: Vec<ResourceState>,
}

struct TrackedBufferState {
    buffer: Buffer,
    initial: BufferState,
    current: BufferState,
}

impl ResourceStateTransaction {
    pub(crate) fn new() -> Self {
        Self {
            images: Vec::new(),
            buffers: Vec::new(),
        }
    }

    pub(crate) fn transition_image(
        &mut self,
        cmdbuf: &CommandBuffer,
        image: &Image,
        base_array_layer: u32,
        layer_count: u32,
        target_state: ResourceState,
    ) -> bool {
        let image_index = self
            .images
            .iter()
            .position(|tracked| tracked.image.state_transaction_key() == image.state_transaction_key())
            .unwrap_or_else(|| {
                let initial = image.snapshot_states();
                self.images.push(TrackedImageState {
                    image: image.clone(),
                    current: initial.clone(),
                    initial,
                });
                self.images.len() - 1
            });
        let tracked = &mut self.images[image_index];
        tracked.image.record_state_transition_from_states(
            cmdbuf,
            base_array_layer,
            layer_count,
            target_state,
            &mut tracked.current,
        );
        true
    }

    pub(crate) fn assume_image_state(
        &mut self,
        image: &Image,
        base_array_layer: u32,
        layer_count: u32,
        state: ResourceState,
    ) {
        let image_index = self
            .images
            .iter()
            .position(|tracked| tracked.image.state_transaction_key() == image.state_transaction_key())
            .unwrap_or_else(|| {
                let initial = image.snapshot_states();
                self.images.push(TrackedImageState {
                    image: image.clone(),
                    current: initial.clone(),
                    initial,
                });
                self.images.len() - 1
            });
        let tracked = &mut self.images[image_index];
        let start = base_array_layer as usize;
        let end = start + layer_count as usize;
        assert!(
            end <= tracked.current.len(),
            "image state assumption layer range {}..{} exceeds array length {}",
            base_array_layer,
            base_array_layer + layer_count,
            tracked.current.len()
        );
        tracked.current[start..end].fill(state);
    }

    pub(crate) fn use_buffer(
        &mut self,
        cmdbuf: &CommandBuffer,
        buffer: &Buffer,
        usage: BufferUse,
    ) {
        let buffer_index = self
            .buffers
            .iter()
            .position(|tracked| {
                tracked.buffer.state_transaction_key() == buffer.state_transaction_key()
            })
            .unwrap_or_else(|| {
                let initial = buffer.snapshot_state();
                self.buffers.push(TrackedBufferState {
                    buffer: buffer.clone(),
                    initial,
                    current: initial,
                });
                self.buffers.len() - 1
            });
        let tracked = &mut self.buffers[buffer_index];
        tracked.buffer.record_state_transition_from_states(
            cmdbuf,
            usage.state(),
            &mut tracked.current,
        );
    }

    pub(crate) fn commit(self) {
        for tracked in self.images {
            tracked
                .image
                .commit_state_snapshot(&tracked.initial, tracked.current);
        }
        for tracked in self.buffers {
            tracked
                .buffer
                .commit_state_snapshot(tracked.initial, tracked.current);
        }
    }
}

/// Record the narrow explicit transition used by external swapchain images and one-time copy
/// operations that are outside the normal command-recording resource transaction.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_image_transition_barrier(
    device: &ash::Device,
    cmdbuf: vk::CommandBuffer,
    transition: TextureTransition,
    image: vk::Image,
    aspect_mask: vk::ImageAspectFlags,
    base_array_layer: u32,
    layer_count: u32,
) {
    crate::sync::diagnostics::record_texture_transition(
        image,
        transition,
        aspect_mask,
        base_array_layer,
        layer_count,
    );

    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(transition.old_layout())
        .new_layout(transition.new_layout())
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer,
            layer_count,
        })
        .src_access_mask(transition.src_access())
        .dst_access_mask(transition.dst_access());

    unsafe {
        device.cmd_pipeline_barrier(
            cmdbuf,
            transition.src_stage(),
            transition.dst_stage(),
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        )
    }
}
