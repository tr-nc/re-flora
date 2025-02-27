use std::collections::HashMap;

use ash::vk;

use super::Device;

pub struct DescriptorSetLayout {
    device: ash::Device,
    descriptor_set_layout: vk::DescriptorSetLayout,

    // lut of bindings
    bindings: HashMap<u32, DescriptorSetLayoutBinding>,
}

impl Drop for DescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }
}

impl DescriptorSetLayout {
    /// Use the builder pattern to create a new DescriptorSetLayout
    fn new(device: &Device, bindings: &[DescriptorSetLayoutBinding]) -> Result<Self, String> {
        let raw_bindings = bindings.iter().map(|b| b.as_raw()).collect::<Vec<_>>();
        let descriptor_set_create_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&raw_bindings);
        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&descriptor_set_create_info, None)
                .map_err(|e| e.to_string())?
        };

        let mut bindings_map = HashMap::new();
        for &binding in bindings {
            if let Some(_) = bindings_map.insert(binding.no, binding) {
                return Err(format!(
                    "Duplicate binding no {} found in descriptor set layout",
                    binding.no
                ));
            }
        }

        Ok(Self {
            device: device.as_raw().clone(),
            descriptor_set_layout,
            bindings: bindings_map,
        })
    }

    pub fn as_raw(&self) -> vk::DescriptorSetLayout {
        self.descriptor_set_layout
    }

    pub fn get_binding(&self, no: u32) -> Option<&DescriptorSetLayoutBinding> {
        self.bindings.get(&no)
    }

    pub fn get_bindings(&self) -> Vec<DescriptorSetLayoutBinding> {
        self.bindings.values().cloned().collect::<Vec<_>>()
    }
}

#[derive(Copy, Clone)]
pub struct DescriptorSetLayoutBinding {
    pub no: u32,
    pub descriptor_type: vk::DescriptorType,
    pub descriptor_count: u32,
    pub stage_flags: vk::ShaderStageFlags,
}

impl DescriptorSetLayoutBinding {
    fn as_raw(&self) -> vk::DescriptorSetLayoutBinding {
        vk::DescriptorSetLayoutBinding::default()
            .binding(self.no)
            .descriptor_type(self.descriptor_type)
            .descriptor_count(self.descriptor_count)
            .stage_flags(self.stage_flags)
    }
}

pub struct DescriptorSetLayoutBuilder {
    bindings: Vec<DescriptorSetLayoutBinding>,
}

impl DescriptorSetLayoutBuilder {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub fn add_binding(&mut self, binding: DescriptorSetLayoutBinding) -> &mut Self {
        self.bindings.push(binding);
        self
    }

    pub fn build(self, device: &Device) -> Result<DescriptorSetLayout, String> {
        DescriptorSetLayout::new(device, &self.bindings)
    }
}
