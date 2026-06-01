use crate::{CommandBuffer, Image, ResourceState, TextureLayout, TextureTransition};
use ash::vk;

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
                self.assert_image_state(image, base_array_layer, layer_count, target_state);
            }
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
