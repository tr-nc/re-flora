use ash::vk;

use super::{DescriptorSetLayout, Device};

pub struct DescriptorPool {
    device: ash::Device,
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
    fn new(
        device: &ash::Device,
        create_info: vk::DescriptorPoolCreateInfo,
    ) -> Result<Self, String> {
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
        let descriptor_pool = Self::new(&device.as_raw(), create_info)?;
        Ok(descriptor_pool)
    }

    pub fn as_raw(&self) -> vk::DescriptorPool {
        self.descriptor_pool
    }
}

// pub struct DescriptorPoolBuilder {
//     device: ash::Device,

//     pool_sizes: Vec<vk::DescriptorPoolSize>,
// }

// impl DescriptorPoolBuilder {
//     pub fn new(device: &Device) -> Self {
//         Self {
//             device: device.as_raw().clone(),
//             pool_sizes: Vec::new(),
//         }
//     }

//     /// Append the pool size for the entire descriptor set to use:
//     ///
//     /// `pool_size` describes how many descriptors of a certain type will be allocated in total from a given pool.
//     /// appending with the same descriptor type will increase the descriptor count.
//     pub fn append_pool_size(&mut self, descriptor_type: vk::DescriptorType, descriptor_count: u32) {
//         self.pool_sizes.push(vk::DescriptorPoolSize {
//             ty: descriptor_type,
//             descriptor_count,
//         });
//     }

//     /// Build the descriptor pool with the given maximum set count and optional flags.
//     pub fn build(
//         &self,
//         max_set_count: u32,
//         flags: Option<vk::DescriptorPoolCreateFlags>,
//     ) -> Result<DescriptorPool, String> {
//         let mut create_info = vk::DescriptorPoolCreateInfo::default()
//             .pool_sizes(&self.pool_sizes)
//             .max_sets(max_set_count);
//         if let Some(flags) = flags {
//             create_info = create_info.flags(flags);
//         }
//         let descriptor_pool = DescriptorPool::new(&self.device, create_info)?;
//         Ok(descriptor_pool)
//     }
// }
