use crate::vkn::{AccelStruct, Buffer, Device, Texture};
use anyhow::Result;
use ash::vk;
use std::sync::Arc;

struct DescriptorSetInner {
    device: Device,
    descriptor_set: vk::DescriptorSet,
}

#[derive(Clone)]
pub struct DescriptorSet(Arc<DescriptorSetInner>);

impl DescriptorSet {
    pub fn new(device: Device, descriptor_set: vk::DescriptorSet) -> Self {
        Self(Arc::new(DescriptorSetInner {
            device,
            descriptor_set,
        }))
    }

    pub fn as_raw(&self) -> vk::DescriptorSet {
        self.0.descriptor_set
    }

    pub fn perform_writes(&self, writes: &mut [WriteDescriptorSet]) {
        if writes.is_empty() {
            return;
        }
        let raw_writes: Vec<_> = writes.iter_mut().map(|w| w.make_raw(self)).collect();
        unsafe { self.0.device.update_descriptor_sets(&raw_writes, &[]) }
    }
}

pub struct WriteDescriptorSet<'a> {
    binding: u32,
    descriptor_type: vk::DescriptorType,

    image_infos: Option<Vec<vk::DescriptorImageInfo>>,
    buffer_infos: Option<Vec<vk::DescriptorBufferInfo>>,
    accel_struct_infos: Option<Vec<vk::WriteDescriptorSetAccelerationStructureKHR<'a>>>,

    _accel_handles: Option<Vec<vk::AccelerationStructureKHR>>,
}

impl<'a> WriteDescriptorSet<'a> {
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
            accel_struct_infos: None,
            _accel_handles: None,
        }
    }

    pub fn new_buffer_write(binding: u32, buffer: &Buffer) -> Self {
        let buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(buffer.as_raw())
            .offset(0)
            .range(buffer.get_size_bytes());

        let descriptor_type =
            Self::descriptor_type_from_usage(buffer.get_usage().as_raw()).unwrap();

        Self {
            binding,
            descriptor_type,
            image_infos: None,
            buffer_infos: Some(vec![buffer_info]),
            accel_struct_infos: None,
            _accel_handles: None,
        }
    }

    #[allow(dead_code)]
    pub fn new_acceleration_structure_write(binding: u32, tlas: &AccelStruct) -> Self {
        let handles = vec![tlas.as_raw()];
        let as_info = vk::WriteDescriptorSetAccelerationStructureKHR {
            acceleration_structure_count: handles.len() as u32,
            p_acceleration_structures: handles.as_ptr(),
            ..Default::default()
        };

        Self {
            binding,
            descriptor_type: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
            image_infos: None,
            buffer_infos: None,
            accel_struct_infos: Some(vec![as_info]),
            _accel_handles: Some(handles),
        }
    }

    fn descriptor_type_from_usage(usage: vk::BufferUsageFlags) -> Result<vk::DescriptorType> {
        if usage.contains(vk::BufferUsageFlags::STORAGE_BUFFER) {
            Ok(vk::DescriptorType::STORAGE_BUFFER)
        } else if usage.contains(vk::BufferUsageFlags::UNIFORM_BUFFER) {
            Ok(vk::DescriptorType::UNIFORM_BUFFER)
        } else {
            Err(anyhow::anyhow!(
                "Unsupported buffer usage for descriptor type: {:?}",
                usage
            ))
        }
    }

    pub fn make_raw(&mut self, descriptor_set: &DescriptorSet) -> vk::WriteDescriptorSet {
        assert!(
            self.image_infos.is_some()
                ^ self.buffer_infos.is_some()
                ^ self.accel_struct_infos.is_some(),
            "A WriteDescriptorSet must contain exactly one of: image_infos, buffer_infos, or accel_struct_infos"
        );

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

        if let Some(accel_infos) = self.accel_struct_infos.as_mut() {
            // compute count before taking a mutable reference for push_next
            let count = accel_infos.len() as u32;
            let accel_info_ptr = &mut accel_infos[0];
            write = write.push_next(accel_info_ptr).descriptor_count(count);
        }

        write
    }
}
