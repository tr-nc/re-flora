use super::{DescriptorPool, DescriptorSetLayout};
use crate::vkn::{Buffer, Device, Texture, Tlas};
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

    pub fn perform_writes(&self, writes: &mut [WriteDescriptorSet]) {
        let writes = writes
            .iter_mut()
            .map(|w| w.make_raw(self))
            .collect::<Vec<_>>();
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

pub struct WriteDescriptorSet<'a> {
    binding: u32,
    descriptor_type: vk::DescriptorType,

    image_infos: Option<Vec<vk::DescriptorImageInfo>>,
    buffer_infos: Option<Vec<vk::DescriptorBufferInfo>>,
    accel_struct_infos: Option<Vec<vk::WriteDescriptorSetAccelerationStructureKHR<'a>>>,

    // for avoiding dangling pointer
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

        Self {
            binding,
            descriptor_type: Self::get_descriptor_type_from_buffer_usage(
                buffer.get_usage().as_raw(),
            ),

            image_infos: None,
            buffer_infos: Some(vec![buffer_info]),
            accel_struct_infos: None,

            _accel_handles: None,
        }
    }

    pub fn new_acceleration_structure_write(binding: u32, tlas: &Tlas) -> Self {
        let handles = vec![tlas.as_raw()];

        // take caution of this pitfall:
        // let tmp: vk::AccelerationStructureKHR = tlas.as_raw(); // copy out the u64 handle
        // let ptr: *const vk::AccelerationStructureKHR = &tmp; // take address of that local `tmp`
        // p_acceleration_structures = ptr;

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

    pub fn make_raw(&mut self, descriptor_set: &DescriptorSet) -> vk::WriteDescriptorSet {
        // we can only have one of these at a time
        assert!(
            self.image_infos.is_some()
                ^ self.buffer_infos.is_some()
                ^ self.accel_struct_infos.is_some()
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

        if let Some(accel_infos) = &mut self.accel_struct_infos {
            let len = accel_infos.len();
            write = write
                .push_next(&mut accel_infos[0])
                .descriptor_count(len as u32);
        }

        write
    }
}
