use crate::vkn::{Allocator, Device};
use ash::vk::{self, ImageLayout};
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme},
    MemoryLocation,
};
use std::sync::{Arc, Mutex};

use super::texture::TextureDesc;

struct ImageInner {
    device: Device,
    image: vk::Image,
    allocator: Allocator,
    memory: Mutex<Option<gpu_allocator::vulkan::Allocation>>,
}

impl Drop for ImageInner {
    fn drop(&mut self) {
        if let Some(memory) = self.memory.lock().unwrap().take() {
            self.allocator.destroy_image(self.image, memory);
        }
    }
}

#[derive(Clone)]
pub struct Image(Arc<ImageInner>);

impl std::ops::Deref for Image {
    type Target = vk::Image;
    fn deref(&self) -> &Self::Target {
        &self.0.image
    }
}

impl Image {
    pub fn new(device: &Device, allocator: &Allocator, desc: &TextureDesc) -> Result<Self, String> {
        let mut cloned_allocator = allocator.clone();
        let (image, memory) = create_image(device, &mut cloned_allocator, desc)?;

        Ok(Self(Arc::new(ImageInner {
            device: device.clone(),
            image,
            allocator: cloned_allocator,
            memory: Mutex::new(Some(memory)),
        })))
    }

    // TODO:
    pub fn transition_layout(&self, new_layout: vk::ImageLayout) {
        // self.0.device.t
    }

    pub fn as_raw(&self) -> vk::Image {
        self.0.image
    }

    pub fn get_allocator(&self) -> &Allocator {
        &self.0.allocator
    }
}

pub fn create_image(
    device: &Device,
    allocator: &mut Allocator,
    desc: &TextureDesc,
) -> Result<(vk::Image, Allocation), String> {
    // for vulkan spec, initial_layout must be either UNDEFINED or PREINITIALIZED,

    if desc.initial_layout != ImageLayout::UNDEFINED
        && desc.initial_layout != ImageLayout::PREINITIALIZED
    {
        return Err("Initial layout must be UNDEFINED".to_string());
    }

    let image_info = vk::ImageCreateInfo::default()
        .extent(desc.get_extent())
        .image_type(desc.get_image_type())
        .mip_levels(1)
        .array_layers(1)
        .format(desc.format)
        .tiling(desc.tilting)
        .initial_layout(ImageLayout::UNDEFINED)
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

    Ok((image, memory))
}
