use crate::{
    Buffer, BufferState, BufferUse, CommandBuffer, Image, ResourceState, TextureLayout,
    TextureTransition,
};
use ash::vk;

/// Image states tentatively produced while one command buffer is being recorded.
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
        tracked.image.record_state_transition_from_states(
            cmdbuf,
            base_array_layer,
            layer_count,
            target_state,
            &mut tracked.current,
        );
    }

    pub(crate) fn state(&self, image: &Image, array_layer: u32) -> Option<ResourceState> {
        self.images
            .iter()
            .find(|tracked| tracked.image.state_transaction_key() == image.state_transaction_key())
            .map(|tracked| {
                *tracked
                    .current
                    .get(array_layer as usize)
                    .expect("image state transaction layer out of bounds")
            })
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

/// Policy for automatic image state transitions recorded by vkn helpers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceStatePolicy {
    /// Record needed barriers automatically.
    Automatic,
    /// Do not record barriers automatically. Callers can still use explicit transitions.
    Manual,
    /// Require resources to already be in the requested state.
    Assert,
}

/// Linear image resource-state tracker used by vkn command helpers.
///
/// This is intentionally smaller than a full render graph: callers still record commands in order,
/// but vkn owns the transition/barrier operation for declared image use.
#[derive(Clone, Debug)]
pub struct ResourceStateTracker {
    policy: ResourceStatePolicy,
}

impl Default for ResourceStateTracker {
    fn default() -> Self {
        Self::automatic()
    }
}

impl ResourceStateTracker {
    pub fn automatic() -> Self {
        Self {
            policy: ResourceStatePolicy::Automatic,
        }
    }

    pub fn manual() -> Self {
        Self {
            policy: ResourceStatePolicy::Manual,
        }
    }

    pub fn assert_only() -> Self {
        Self {
            policy: ResourceStatePolicy::Assert,
        }
    }

    pub fn policy(&self) -> ResourceStatePolicy {
        self.policy
    }

    pub fn with_policy(mut self, policy: ResourceStatePolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn set_policy(&mut self, policy: ResourceStatePolicy) {
        self.policy = policy;
    }

    pub fn transition_image_layout(
        &self,
        cmdbuf: &CommandBuffer,
        image: &Image,
        array_layer: u32,
        target_layout: TextureLayout,
    ) {
        self.transition_image(cmdbuf, image, array_layer, ResourceState::from_layout(target_layout));
    }

    pub fn transition_image(
        &self,
        cmdbuf: &CommandBuffer,
        image: &Image,
        array_layer: u32,
        target_state: ResourceState,
    ) {
        self.transition_image_layers(cmdbuf, image, array_layer, 1, target_state);
    }

    pub fn transition_image_layers(
        &self,
        cmdbuf: &CommandBuffer,
        image: &Image,
        base_array_layer: u32,
        layer_count: u32,
        target_state: ResourceState,
    ) {
        match self.policy {
            ResourceStatePolicy::Automatic => {
                image.record_state_transition(cmdbuf, base_array_layer, layer_count, target_state);
            }
            ResourceStatePolicy::Manual => {}
            ResourceStatePolicy::Assert => {
                self.assert_image_state_for_recording(
                    cmdbuf,
                    image,
                    base_array_layer,
                    layer_count,
                    target_state,
                );
            }
        }
    }

    fn assert_image_state_for_recording(
        &self,
        cmdbuf: &CommandBuffer,
        image: &Image,
        base_array_layer: u32,
        layer_count: u32,
        expected_state: ResourceState,
    ) {
        for layer in base_array_layer..base_array_layer + layer_count {
            let actual_state = cmdbuf
                .recorded_image_state(image, layer)
                .unwrap_or_else(|| image.get_state(layer));
            assert_eq!(
                actual_state.layout(),
                expected_state.layout(),
                "image layer {} is in {:?}, expected {:?}",
                layer,
                actual_state.layout(),
                expected_state.layout()
            );
        }
    }

    pub fn assert_image_layout(
        &self,
        image: &Image,
        array_layer: u32,
        expected_layout: TextureLayout,
    ) {
        self.assert_image_state(
            image,
            array_layer,
            1,
            ResourceState::from_layout(expected_layout),
        );
    }

    pub fn assert_image_state(
        &self,
        image: &Image,
        base_array_layer: u32,
        layer_count: u32,
        expected_state: ResourceState,
    ) {
        for layer in base_array_layer..base_array_layer + layer_count {
            let actual_state = image.get_state(layer);
            assert_eq!(
                actual_state.layout(),
                expected_state.layout(),
                "image layer {} is in {:?}, expected {:?}",
                layer,
                actual_state.layout(),
                expected_state.layout()
            );
        }
    }

    pub fn assume_image_layout(&self, image: &Image, array_layer: u32, layout: TextureLayout) {
        image.set_layout(array_layer, layout);
    }

    pub fn assume_image_state(&self, image: &Image, array_layer: u32, state: ResourceState) {
        image.set_state(array_layer, state);
    }
}

/// Record a transition barrier for one subresource-range of an image.
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
