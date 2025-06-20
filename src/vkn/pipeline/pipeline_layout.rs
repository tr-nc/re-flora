use crate::vkn::{DescriptorSetLayout, Device, ShaderModule};
use anyhow::Result;
use ash::vk;
use std::{collections::HashMap, ops::Deref, sync::Arc};

struct PipelineLayoutInner {
    device: Device,
    pipeline_layout: vk::PipelineLayout,

    descriptor_set_layouts: HashMap<u32, DescriptorSetLayout>,
    push_constant_ranges: HashMap<u32, vk::PushConstantRange>,
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
    pub fn from_shader_module(device: &Device, shader_module: &ShaderModule) -> Self {
        let descriptor_set_layouts = shader_module.get_descriptor_set_layouts();
        let push_constant_ranges = shader_module.get_push_constant_ranges();
        PipelineLayout::new(device, &descriptor_set_layouts, &push_constant_ranges)
    }

    fn new(
        device: &Device,
        descriptor_set_layouts: &HashMap<u32, DescriptorSetLayout>,
        push_constant_ranges: &HashMap<u32, vk::PushConstantRange>,
    ) -> Self {
        let mut pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::default();

        let raw_layouts = make_dense_layouts(descriptor_set_layouts);
        pipeline_layout_create_info = pipeline_layout_create_info.set_layouts(&raw_layouts);

        let mut raw_ranges = Vec::new();
        let mut ordered_keys: Vec<u32> = push_constant_ranges.keys().cloned().collect();
        ordered_keys.sort_unstable();
        for key in ordered_keys {
            raw_ranges.push(push_constant_ranges[&key]);
        }

        pipeline_layout_create_info = pipeline_layout_create_info.push_constant_ranges(&raw_ranges);

        let pipeline_layout = unsafe {
            device
                .as_raw()
                .create_pipeline_layout(&pipeline_layout_create_info, None)
                .expect("Failed to create pipeline layout")
        };

        return Self(Arc::new(PipelineLayoutInner {
            device: device.clone(),
            pipeline_layout,
            descriptor_set_layouts: descriptor_set_layouts.clone(),
            push_constant_ranges: push_constant_ranges.clone(),
        }));

        /// Convert a HashMap<u32, DescriptorSetLayout> to a Vec<vk::DescriptorSetLayout>
        /// Where the key is the set number and the value is the descriptor set layout.
        /// The Vec is dense, meaning that it has a value for every set number.
        /// The Vec is ordered by the set number.
        /// If a set number is not present, the value is vk::DescriptorSetLayout::null().
        fn make_dense_layouts(
            map: &HashMap<u32, DescriptorSetLayout>,
        ) -> Vec<vk::DescriptorSetLayout> {
            if map.is_empty() {
                return Vec::new();
            }

            let max_key = *map.keys().max().unwrap();
            let mut vec = vec![vk::DescriptorSetLayout::null(); (max_key + 1) as usize];

            for (&set_no, layout) in map {
                vec[set_no as usize] = layout.as_raw();
            }
            vec
        }
    }

    pub fn as_raw(&self) -> vk::PipelineLayout {
        self.0.pipeline_layout
    }

    pub fn get_descriptor_set_layouts(&self) -> &HashMap<u32, DescriptorSetLayout> {
        &self.0.descriptor_set_layouts
    }

    pub fn merge(&self, other: &PipelineLayout) -> Result<Self> {
        if self.0.device != other.0.device {
            return Err(anyhow::anyhow!(
                "Cannot merge PipelineLayouts from different devices"
            ));
        }

        // merge descriptor set layouts
        let mut merged_descriptor_set_layouts = HashMap::new();
        for (set_no, layout) in self.0.descriptor_set_layouts.iter() {
            if !other.0.descriptor_set_layouts.contains_key(set_no) {
                merged_descriptor_set_layouts.insert(*set_no, layout.clone());
            } else {
                let other_layout = other.0.descriptor_set_layouts.get(set_no).unwrap();
                let merged_layout = layout.merge(other_layout)?;
                merged_descriptor_set_layouts.insert(*set_no, merged_layout);
            }
        }
        for (set_no, layout) in other.0.descriptor_set_layouts.iter() {
            if !self.0.descriptor_set_layouts.contains_key(set_no) {
                merged_descriptor_set_layouts.insert(*set_no, layout.clone());
            }
        }

        // merge push constant ranges
        let mut merged_push_constant_ranges = HashMap::new();
        for (offset, range) in self.0.push_constant_ranges.iter() {
            if !other.0.push_constant_ranges.contains_key(offset) {
                merged_push_constant_ranges.insert(*offset, *range);
            } else {
                // compare the ranges
                let other_range = other.0.push_constant_ranges.get(offset).unwrap();
                if range.size != other_range.size {
                    return Err(anyhow::anyhow!(
                        "Push constant ranges at offset {} do not match: {} != {}",
                        offset,
                        range.size,
                        other_range.size
                    ));
                }
                merged_push_constant_ranges.insert(*offset, *range);
            }
        }

        for (offset, range) in other.0.push_constant_ranges.iter() {
            if !self.0.push_constant_ranges.contains_key(offset) {
                merged_push_constant_ranges.insert(*offset, *range);
            }
        }

        Ok(PipelineLayout::new(
            &self.0.device,
            &merged_descriptor_set_layouts,
            &merged_push_constant_ranges,
        ))
    }
}
