use crate::{util::MergeWithEq, vkn::Device};
use anyhow::Result;
use ash::vk;
use std::{collections::HashMap, sync::Arc};

#[derive(Debug)]
struct DescriptorSetLayoutInner {
    device: Device,
    descriptor_set_layout: vk::DescriptorSetLayout,
    bindings: HashMap<u32, DescriptorSetLayoutBinding>,
}

impl Drop for DescriptorSetLayoutInner {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }
}

#[derive(Clone, Debug)]
pub struct DescriptorSetLayout(Arc<DescriptorSetLayoutInner>);

impl std::ops::Deref for DescriptorSetLayout {
    type Target = vk::DescriptorSetLayout;
    fn deref(&self) -> &Self::Target {
        &self.0.descriptor_set_layout
    }
}

impl DescriptorSetLayout {
    /// Use the builder pattern to create a new DescriptorSetLayout
    fn new(device: &Device, bindings: &HashMap<u32, DescriptorSetLayoutBinding>) -> Result<Self> {
        let raw_bindings = bindings.iter().map(|b| b.1.as_raw()).collect::<Vec<_>>();
        let descriptor_set_create_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&raw_bindings);
        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&descriptor_set_create_info, None)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?
        };

        Ok(Self(Arc::new(DescriptorSetLayoutInner {
            device: device.clone(),
            descriptor_set_layout,
            bindings: bindings.clone(),
        })))
    }

    pub fn as_raw(&self) -> vk::DescriptorSetLayout {
        self.0.descriptor_set_layout
    }

    pub fn get_bindings(&self) -> Vec<DescriptorSetLayoutBinding> {
        self.0.bindings.values().cloned().collect::<Vec<_>>()
    }

    pub fn merge(&self, other: &DescriptorSetLayout) -> Result<Self> {
        if self.0.device != other.0.device {
            return Err(anyhow::anyhow!(
                "Cannot merge DescriptorSetLayouts from different devices"
            ));
        }

        let merged_bindings = self.0.bindings.merge_with_eq(&other.0.bindings)?;

        Self::new(&self.0.device, &merged_bindings)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DescriptorSetLayoutBinding {
    pub no: u32,
    pub name: String,
    pub descriptor_type: vk::DescriptorType,
    pub descriptor_count: u32,
    pub stage_flags: vk::ShaderStageFlags,
}

impl DescriptorSetLayoutBinding {
    fn as_raw(&self) -> vk::DescriptorSetLayoutBinding<'_> {
        vk::DescriptorSetLayoutBinding::default()
            .binding(self.no)
            .descriptor_type(self.descriptor_type)
            .descriptor_count(self.descriptor_count)
            .stage_flags(self.stage_flags)
    }
}

pub struct DescriptorSetLayoutBuilder {
    bindings: HashMap<u32, DescriptorSetLayoutBinding>,
}

impl DescriptorSetLayoutBuilder {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    pub fn set_bindings(
        &mut self,
        bindings: HashMap<u32, DescriptorSetLayoutBinding>,
    ) -> &mut Self {
        self.bindings = bindings;
        self
    }

    pub fn build(self, device: &Device) -> Result<DescriptorSetLayout> {
        DescriptorSetLayout::new(device, &self.bindings.clone())
    }
}
