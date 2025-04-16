use super::DescriptorSetLayout;
use crate::vkn::Device;
use ash::vk;
use std::sync::Arc;

struct DescriptorPoolInner {
    device: Device,
    descriptor_pool: vk::DescriptorPool,
}

impl Drop for DescriptorPoolInner {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

#[derive(Clone)]
pub struct DescriptorPool(Arc<DescriptorPoolInner>);

impl std::ops::Deref for DescriptorPool {
    type Target = vk::DescriptorPool;
    fn deref(&self) -> &Self::Target {
        &self.0.descriptor_pool
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

        Ok(Self(Arc::new(DescriptorPoolInner {
            device: device.clone(),
            descriptor_pool,
        })))
    }

    /// Use this for development stages only. Not recommended for production use.
    pub fn a_big_one(device: &Device) -> Result<Self, String> {
        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 1000,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 1000,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1000,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                descriptor_count: 1000,
            },
        ];
        let create_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(100);
        Self::new(&device, create_info)
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

    pub fn reset(&self) -> Result<(), String> {
        unsafe {
            let res = self.0.device.reset_descriptor_pool(
                self.0.descriptor_pool,
                vk::DescriptorPoolResetFlags::empty(),
            );
            match res {
                Ok(()) => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }

    pub fn as_raw(&self) -> vk::DescriptorPool {
        self.0.descriptor_pool
    }
}
