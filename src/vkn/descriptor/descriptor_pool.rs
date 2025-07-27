use super::DescriptorSetLayout;
use crate::vkn::Device;
use anyhow::Result;
use ash::vk;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::DescriptorSet;

struct DescriptorPoolInner {
    device: Device,
    descriptor_pool: vk::DescriptorPool,
    sets: Mutex<Vec<vk::DescriptorSet>>,
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
    /// Convenient but large pool, intended for development only.
    pub fn new(device: &Device) -> Result<Self> {
        const MAX_DESCRIPTORS: u32 = 10_000;
        const MAX_SETS: u32 = 100;

        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: MAX_DESCRIPTORS,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: MAX_DESCRIPTORS,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: MAX_DESCRIPTORS,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                descriptor_count: MAX_DESCRIPTORS,
            },
        ];

        let create_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(MAX_SETS);

        Self::from_create_info(device, create_info)
    }

    /// Creates a pool sized according to the provided descriptor set layouts.
    /// MARK: this function is archived, it follows a good practice, but it's not handy to use.
    #[allow(unused)]
    pub fn from_descriptor_set_layouts(
        device: &Device,
        descriptor_set_layouts: &HashMap<u32, DescriptorSetLayout>,
    ) -> Result<Self> {
        let mut pool_sizes = Vec::new();
        for layout in descriptor_set_layouts.values() {
            for binding in layout.get_bindings() {
                pool_sizes.push(vk::DescriptorPoolSize {
                    ty: binding.descriptor_type,
                    descriptor_count: binding.descriptor_count,
                });
            }
        }

        let create_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(descriptor_set_layouts.len() as u32);

        Self::from_create_info(device, create_info)
    }

    /// Allocates a descriptor set from this pool and stores it internally.
    pub fn allocate_set(&self, layout: &DescriptorSetLayout) -> Result<DescriptorSet> {
        let set_layouts = [layout.as_raw()];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.0.descriptor_pool)
            .set_layouts(&set_layouts);

        let set = unsafe {
            self.0
                .device
                .allocate_descriptor_sets(&alloc_info)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?[0]
        };

        // record handle for future introspection / debug if needed
        self.0.sets.lock().unwrap().push(set);

        Ok(DescriptorSet::new(self.0.device.clone(), set))
    }

    /// Allows manual pool reset.
    #[allow(unused)]
    pub fn reset(&self) -> Result<()> {
        unsafe {
            self.0
                .device
                .reset_descriptor_pool(
                    self.0.descriptor_pool,
                    vk::DescriptorPoolResetFlags::empty(),
                )
                .map_err(|e| anyhow::anyhow!(e.to_string()))
        }
    }

    /// Internal helper.
    fn from_create_info(
        device: &Device,
        create_info: vk::DescriptorPoolCreateInfo,
    ) -> Result<Self> {
        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&create_info, None)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?
        };

        Ok(Self(Arc::new(DescriptorPoolInner {
            device: device.clone(),
            descriptor_pool,
            sets: Mutex::new(Vec::new()),
        })))
    }
}
