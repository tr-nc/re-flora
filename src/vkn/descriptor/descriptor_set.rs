use super::{DescriptorPool, DescriptorSetLayout};
use crate::vkn::{Buffer, Device, Texture};
use ash::vk;

pub struct DescriptorSet {
    device: Device,
    descriptor_set: vk::DescriptorSet,
    _pool: DescriptorPool,
}

impl DescriptorSet {
    pub fn new(
        device: Device,
        descriptor_set_layout: &DescriptorSetLayout,
        descriptor_pool: DescriptorPool,
    ) -> Self {
        let descriptor_set =
            create_descriptor_set(&device, &descriptor_pool, descriptor_set_layout);
        Self {
            device: device.clone(),
            descriptor_set,
            _pool: descriptor_pool.clone(),
        }
    }

    pub fn as_raw(&self) -> vk::DescriptorSet {
        self.descriptor_set
    }

    pub fn perform_writes(&self, writes: &[WriteDescriptorSet]) {
        let writes = writes.iter().map(|w| w.make_raw(self)).collect::<Vec<_>>();
        unsafe { self.device.update_descriptor_sets(&writes, &[]) }
    }
}

fn create_descriptor_set(
    device: &Device,
    descriptor_pool: &DescriptorPool,
    set_layout: &DescriptorSetLayout,
) -> vk::DescriptorSet {
    let set_layouts = [set_layout.as_raw()];
    let allocate_info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(descriptor_pool.as_raw())
        .set_layouts(&set_layouts);
    unsafe {
        device
            .allocate_descriptor_sets(&allocate_info)
            .expect("Failed to allocate descriptor set(s)")[0]
    }
}

pub struct WriteDescriptorSet {
    binding: u32,
    descriptor_type: vk::DescriptorType,
    image_infos: Option<Vec<vk::DescriptorImageInfo>>,
    buffer_infos: Option<Vec<vk::DescriptorBufferInfo>>,
}

impl WriteDescriptorSet {
    pub fn new_texture_write(
        binding: u32,
        descriptor_type: vk::DescriptorType,
        texture: &Texture,
        image_layout: vk::ImageLayout,
    ) -> Self {
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(image_layout)
            .image_view(texture.get_image_view().as_raw())
            .sampler(texture.get_sampler().as_raw());
        Self {
            binding,
            descriptor_type,
            image_infos: Some(vec![image_info]),
            buffer_infos: None,
        }
    }

    pub fn new_buffer_write(binding: u32, buffer: &Buffer) -> Self {
        let buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(buffer.as_raw())
            .offset(0)
            .range(buffer.get_size_bytes());

        Self {
            binding,
            descriptor_type: Self::get_descriptor_type_from_buffer_usage(
                buffer.get_usage().as_raw(),
            ),
            image_infos: None,
            buffer_infos: Some(vec![buffer_info]),
        }
    }

    fn get_descriptor_type_from_buffer_usage(
        buffer_usage: vk::BufferUsageFlags,
    ) -> vk::DescriptorType {
        if buffer_usage.contains(vk::BufferUsageFlags::STORAGE_BUFFER) {
            vk::DescriptorType::STORAGE_BUFFER
        } else if buffer_usage.contains(vk::BufferUsageFlags::UNIFORM_BUFFER) {
            vk::DescriptorType::UNIFORM_BUFFER
        } else {
            panic!("Unsupported buffer usage for descriptor type")
        }
    }

    /// Only one of the following should be Some
    fn validate(
        image_infos: &Option<Vec<vk::DescriptorImageInfo>>,
        buffer_infos: &Option<Vec<vk::DescriptorBufferInfo>>,
    ) {
        assert_eq!(image_infos.is_some() ^ buffer_infos.is_some(), true);
    }

    pub fn make_raw(&self, descriptor_set: &DescriptorSet) -> vk::WriteDescriptorSet {
        Self::validate(&self.image_infos, &self.buffer_infos);

        let mut write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set.as_raw())
            .dst_binding(self.binding)
            .descriptor_type(self.descriptor_type);

        if let Some(image_info) = &self.image_infos {
            write = write.image_info(image_info);
        }
        if let Some(buffer_info) = &self.buffer_infos {
            write = write.buffer_info(buffer_info);
        }
        write
    }
}
