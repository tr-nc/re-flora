use ash::vk;

use super::{DescriptorSetLayout, Device};

pub struct DescriptorPool {
    device: Device,
    descriptor_pool: vk::DescriptorPool,
}

impl Drop for DescriptorPool {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

impl DescriptorPool {
    /// Create a new descriptor pool
    fn new(device: &Device, create_info: vk::DescriptorPoolCreateInfo) -> Result<Self, String> {
        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&create_info, None)
                .map_err(|e| e.to_string())?
        };

        Ok(Self {
            device: device.clone(),
            descriptor_pool,
        })
    }

    pub fn from_descriptor_set_layouts(
        device: &Device,
        descriptor_set_layouts: &[DescriptorSetLayout],
    ) -> Result<Self, String> {
        let mut pool_sizes = Vec::new();
        for layout in descriptor_set_layouts {
            for binding in layout.get_bindings().iter() {
                let pool_size = vk::DescriptorPoolSize {
                    ty: binding.descriptor_type,
                    descriptor_count: binding.descriptor_count,
                };
                pool_sizes.push(pool_size);
            }
        }
        let create_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(descriptor_set_layouts.len() as u32);
        let descriptor_pool = Self::new(&device, create_info)?;
        Ok(descriptor_pool)
    }

    pub fn as_raw(&self) -> vk::DescriptorPool {
        self.descriptor_pool
    }
}
