use ash::vk::{self, DescriptorSetLayout};
use std::ops::Deref;

use super::Device;

pub struct PipelineLayout {
    device: ash::Device,
    pipeline_layout: vk::PipelineLayout,
}

impl Drop for PipelineLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}

impl Deref for PipelineLayout {
    type Target = vk::PipelineLayout;

    fn deref(&self) -> &Self::Target {
        &self.pipeline_layout
    }
}

impl PipelineLayout {
    pub fn new(
        device: &Device,
        descriptor_set_layouts: Option<&[DescriptorSetLayout]>,
        push_constant_ranges: Option<&[vk::PushConstantRange]>,
    ) -> Self {
        let mut pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::default();

        if let Some(descriptor_set_layouts) = descriptor_set_layouts {
            pipeline_layout_create_info =
                pipeline_layout_create_info.set_layouts(descriptor_set_layouts);
        }

        if let Some(push_constant_ranges) = push_constant_ranges {
            pipeline_layout_create_info =
                pipeline_layout_create_info.push_constant_ranges(push_constant_ranges);
        }

        let pipeline_layout = unsafe {
            device
                .as_raw()
                .create_pipeline_layout(&pipeline_layout_create_info, None)
                .expect("Failed to create pipeline layout")
        };

        Self {
            device: device.as_raw().clone(),
            pipeline_layout,
        }
    }

    pub fn as_raw(&self) -> vk::PipelineLayout {
        self.pipeline_layout
    }
}
