use crate::vkn::{Allocator, Device};
use ash::vk;
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme},
    MemoryLocation,
};

use super::texture::TextureDesc;

pub struct Image {
    image: vk::Image,
    allocator: Allocator,
    memory: gpu_allocator::vulkan::Allocation,
}

impl Drop for Image {
    fn drop(&mut self) {
        self.allocator
            .destroy_image(self.image, std::mem::take(&mut self.memory));
    }
}

impl Image {
    pub fn new(device: &Device, allocator: &mut Allocator, desc: &TextureDesc) -> Self {
        let (image, memory) = create_image(device, allocator, desc);
        Self {
            image,
            allocator: allocator.clone(),
            memory,
        }
    }

    pub fn as_raw(&self) -> vk::Image {
        self.image
    }

    pub fn get_allocator_mut(&mut self) -> &mut Allocator {
        &mut self.allocator
    }
}

pub fn create_image(
    device: &Device,
    allocator: &mut Allocator,
    desc: &TextureDesc,
) -> (vk::Image, Allocation) {
    let image_info = vk::ImageCreateInfo::default()
        .extent(desc.get_extent())
        .image_type(desc.get_image_type())
        .mip_levels(1)
        .array_layers(1)
        .format(desc.format)
        .tiling(desc.tilting)
        .initial_layout(desc.initial_layout)
        .usage(desc.usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .samples(desc.samples)
        .flags(vk::ImageCreateFlags::empty());

    let image = unsafe { device.create_image(&image_info, None).unwrap() };
    let requirements = unsafe { device.get_image_memory_requirements(image) };

    let memory = allocator
        .allocate_memory(&AllocationCreateDesc {
            name: "",
            requirements,
            location: MemoryLocation::GpuOnly,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        })
        .expect("Failed to allocate image memory");

    unsafe {
        device
            .bind_image_memory(image, memory.memory(), memory.offset())
            .unwrap()
    };

    (image, memory)
}
