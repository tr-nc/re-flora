use crate::vkn::{Allocator, CommandBuffer, Device};
use ash::vk::{self, ImageLayout};
use gpu_allocator::{
    vulkan::{AllocationCreateDesc, AllocationScheme},
    MemoryLocation,
};
use std::sync::{Arc, Mutex};

use super::texture::TextureDesc;

struct ImageInner {
    device: Device,
    image: vk::Image,
    allocator: Allocator,
    memory: Mutex<Option<gpu_allocator::vulkan::Allocation>>,
    current_layout: Mutex<vk::ImageLayout>,
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

        let memory = cloned_allocator
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

        Ok(Self(Arc::new(ImageInner {
            device: device.clone(),
            image,
            allocator: cloned_allocator,
            memory: Mutex::new(Some(memory)),
            current_layout: Mutex::new(desc.initial_layout),
        })))
    }

    /// Transition the image layout using a barrier.
    pub fn record_transition(&self, cmdbuf: &CommandBuffer, new_layout: vk::ImageLayout) {
        let device = &self.0.device;
        let mut layout_guard = self.0.current_layout.lock().unwrap();
        let current_layout = *layout_guard;

        let mut barrier = vk::ImageMemoryBarrier::default()
            .old_layout(current_layout)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.0.image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let src_stage;
        let dst_stage;

        match (current_layout, new_layout) {
            (vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => {
                barrier.src_access_mask = vk::AccessFlags::empty();
                barrier.dst_access_mask = vk::AccessFlags::TRANSFER_WRITE;
                src_stage = vk::PipelineStageFlags::TOP_OF_PIPE;
                dst_stage = vk::PipelineStageFlags::TRANSFER;
            }

            (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => {
                barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
                barrier.dst_access_mask = vk::AccessFlags::SHADER_READ;
                src_stage = vk::PipelineStageFlags::TRANSFER;
                dst_stage = vk::PipelineStageFlags::FRAGMENT_SHADER;
            }

            (vk::ImageLayout::UNDEFINED, layout)
                if layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                    || layout == vk::ImageLayout::GENERAL =>
            {
                barrier.src_access_mask = vk::AccessFlags::empty();
                barrier.dst_access_mask = vk::AccessFlags::SHADER_WRITE;
                // or SHADER_READ, depending on how you plan to access it
                src_stage = vk::PipelineStageFlags::TOP_OF_PIPE;
                dst_stage = vk::PipelineStageFlags::COMPUTE_SHADER;
            }

            (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::GENERAL) => {
                barrier.src_access_mask = vk::AccessFlags::TRANSFER_READ;
                barrier.dst_access_mask = vk::AccessFlags::SHADER_READ;
                src_stage = vk::PipelineStageFlags::TRANSFER;
                dst_stage = vk::PipelineStageFlags::COMPUTE_SHADER;
            }

            _ => {
                panic!(
                    "Unsupported layout transition: {:?} -> {:?}",
                    layout_guard, new_layout
                );
            }
        }

        unsafe {
            device.cmd_pipeline_barrier(
                cmdbuf.as_raw(),
                src_stage,
                dst_stage,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            )
        }
        *layout_guard = new_layout;
    }

    pub fn as_raw(&self) -> vk::Image {
        self.0.image
    }

    pub fn get_allocator(&self) -> &Allocator {
        &self.0.allocator
    }
}
