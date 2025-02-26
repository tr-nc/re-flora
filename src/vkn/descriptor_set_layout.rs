use ash::vk;

use super::Device;

pub struct DescriptorSetLayout {
    device: ash::Device,
    descriptor_set_layout: vk::DescriptorSetLayout,
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
    fn new(device: &Device, bindings: &[vk::DescriptorSetLayoutBinding]) -> Result<Self, String> {
        let descriptor_set_create_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&descriptor_set_create_info, None)
                .map_err(|e| e.to_string())?
        };
        Ok(Self {
            device: device.as_raw().clone(),
            descriptor_set_layout,
        })
    }

    pub fn as_raw(&self) -> vk::DescriptorSetLayout {
        self.descriptor_set_layout
    }
}

pub struct DescriptorSetLayoutBuilder<'a> {
    bindings: Vec<vk::DescriptorSetLayoutBinding<'a>>,
}

impl DescriptorSetLayoutBuilder<'_> {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub fn add_binding(
        mut self,
        number: u32,
        count: u32,
        descriptor_type: vk::DescriptorType,
        shader_stage_flags: vk::ShaderStageFlags,
    ) -> Self {
        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(number)
            .descriptor_type(descriptor_type)
            .descriptor_count(count)
            .stage_flags(shader_stage_flags);
        self.bindings.push(binding);
        self
    }

    pub fn build(self, device: &Device) -> Result<DescriptorSetLayout, String> {
        DescriptorSetLayout::new(device, &self.bindings)
    }
}
