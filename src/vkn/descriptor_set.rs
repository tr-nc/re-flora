use ash::vk;

use super::{Buffer, DescriptorPool, DescriptorSetLayout, Device, Texture};

pub struct DescriptorSet {
    _device: Device,
    descriptor_set: vk::DescriptorSet,
    _descriptor_set_layouts: Vec<DescriptorSetLayout>,
    _pool: DescriptorPool,
}

impl DescriptorSet {
    pub fn new(
        device: &Device,
        descriptor_set_layouts: &[DescriptorSetLayout],
        descriptor_pool: DescriptorPool,
    ) -> Self {
        let descriptor_set =
            create_descriptor_set(device, &descriptor_pool, descriptor_set_layouts);
        Self {
            _device: device.clone(),
            descriptor_set,
            _descriptor_set_layouts: descriptor_set_layouts.to_vec(),
            _pool: descriptor_pool.clone(),
        }
    }

    pub fn as_raw(&self) -> vk::DescriptorSet {
        self.descriptor_set
    }

    pub fn perform_writes(&self, writes: &[WriteDescriptorSet]) {
        let writes = writes.iter().map(|w| w.make_raw(self)).collect::<Vec<_>>();
        unsafe { self._device.update_descriptor_sets(&writes, &[]) }
    }
}

fn create_descriptor_set(
    device: &Device,
    descriptor_pool: &DescriptorPool,
    set_layouts: &[DescriptorSetLayout],
) -> vk::DescriptorSet {
    let set_layouts = set_layouts.iter().map(|l| l.as_raw()).collect::<Vec<_>>();
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
    // pub fn new(binding: u32, descriptor_type: vk::DescriptorType) -> Self {
    //     Self {
    //         binding,
    //         descriptor_type,
    //         image_infos: None,
    //         buffer_infos: None,
    //     }
    // }

    // pub fn add_texture(&mut self, texture: &Texture, image_layout: vk::ImageLayout) -> &mut Self {
    //     let image_info = vk::DescriptorImageInfo::default()
    //         .image_layout(image_layout)
    //         .image_view(texture.get_image_view().as_raw())
    //         .sampler(texture.get_sampler().as_raw());

    //     if self.image_infos.is_none() {
    //         self.image_infos = Some(Vec::new());
    //     }
    //     self.image_infos.as_mut().unwrap().push(image_info);
    //     self
    // }

    // pub fn add_buffer(&mut self, buffer: &Buffer) -> &mut Self {
    //     let buffer_info = vk::DescriptorBufferInfo::default()
    //         .buffer(buffer.as_raw())
    //         .offset(0)
    //         .range(buffer.get_size());

    //     if self.buffer_infos.is_none() {
    //         self.buffer_infos = Some(Vec::new());
    //     }
    //     self.buffer_infos.as_mut().unwrap().push(buffer_info);
    //     self
    // }

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

    pub fn new_buffer_write(
        binding: u32,
        descriptor_type: vk::DescriptorType,
        buffer: &Buffer,
    ) -> Self {
        let buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(buffer.as_raw())
            .offset(0)
            .range(buffer.get_size());

        Self {
            binding,
            descriptor_type,
            image_infos: None,
            buffer_infos: Some(vec![buffer_info]),
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
