use super::{DescriptorSetLayout, Device};
use ash::vk;
use std::{ops::Deref, sync::Arc};

struct PipelineLayoutInner {
    device: Device,
    pipeline_layout: vk::PipelineLayout,

    descriptor_set_layouts: Vec<DescriptorSetLayout>,
    _push_constant_ranges: Vec<vk::PushConstantRange>,
}

impl Drop for PipelineLayoutInner {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}

#[derive(Clone)]
pub struct PipelineLayout(Arc<PipelineLayoutInner>);

impl Deref for PipelineLayout {
    type Target = vk::PipelineLayout;

    fn deref(&self) -> &Self::Target {
        &self.0.pipeline_layout
    }
}

impl PipelineLayout {
    pub fn new(
        device: &Device,
        descriptor_set_layouts: Option<&[DescriptorSetLayout]>,
        push_constant_ranges: Option<&[vk::PushConstantRange]>,
    ) -> Self {
        let mut pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::default();

        let mut raw_layouts = Vec::new();
        if let Some(descriptor_set_layouts) = descriptor_set_layouts {
            raw_layouts = descriptor_set_layouts
                .iter()
                .map(|layout| layout.as_raw())
                .collect::<Vec<_>>();
        }
        pipeline_layout_create_info = pipeline_layout_create_info.set_layouts(&raw_layouts);

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

        Self(Arc::new(PipelineLayoutInner {
            device: device.clone(),
            pipeline_layout,
            descriptor_set_layouts: descriptor_set_layouts
                .map(|layouts| layouts.to_vec())
                .unwrap_or_default(),
            _push_constant_ranges: push_constant_ranges
                .map(|ranges| ranges.to_vec())
                .unwrap_or_default(),
        }))
    }

    pub fn as_raw(&self) -> vk::PipelineLayout {
        self.0.pipeline_layout
    }

    pub fn get_descriptor_set_layouts(&self) -> &[DescriptorSetLayout] {
        &self.0.descriptor_set_layouts
    }

    pub fn _get_push_constant_ranges(&self) -> &[vk::PushConstantRange] {
        &self.0._push_constant_ranges
    }
}
