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
    desc: TextureDesc,
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
            desc: desc.clone(),
            allocator: cloned_allocator,
            memory: Mutex::new(Some(memory)),
            current_layout: Mutex::new(desc.initial_layout),
        })))
    }

    pub fn get_desc(&self) -> &TextureDesc {
        &self.0.desc
    }

    /// Copy the image to another image. Tracks the layout transitions. And does sufficient validations.
    pub fn record_copy_to(&self, cmdbuf: &CommandBuffer, dst_image: &Image) -> Result<(), String> {
        self.record_transition_barrier(cmdbuf, vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        dst_image.record_transition_barrier(cmdbuf, vk::ImageLayout::TRANSFER_DST_OPTIMAL);

        let extent = self.0.desc.get_extent();
        if extent != dst_image.0.desc.get_extent() {
            return Err("Extent mismatch".to_string());
        }

        unsafe {
            self.0.device.cmd_copy_image(
                cmdbuf.as_raw(),
                self.0.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                dst_image.0.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[self.get_copy_region()],
            );
        }
        Ok(())
    }

    pub fn get_copy_region(&self) -> vk::ImageCopy {
        vk::ImageCopy {
            src_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            src_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            dst_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            dst_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            extent: self.0.desc.get_extent(),
        }
    }

    pub fn record_transition_barrier(&self, cmdbuf: &CommandBuffer, new_layout: vk::ImageLayout) {
        let device = &self.0.device;
        let mut layout_guard = self.0.current_layout.lock().unwrap();

        let current_layout = *layout_guard;

        if current_layout == new_layout {
            return;
        }

        image_transition_barrier(
            device.as_raw(),
            cmdbuf.as_raw(),
            current_layout,
            new_layout,
            self.0.image,
        );

        *layout_guard = new_layout;
    }

    pub fn as_raw(&self) -> vk::Image {
        self.0.image
    }

    pub fn get_allocator(&self) -> &Allocator {
        &self.0.allocator
    }
}

/// Record a transition barrier for an image.
pub fn image_transition_barrier(
    device: &ash::Device,
    cmdbuf: vk::CommandBuffer,
    current_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    image: vk::Image,
) {
    let (src_access_mask, src_stage) = map_src_stage_access_flags(current_layout);
    let (dst_access_mask, dst_stage) = map_dst_stage_access_flags(new_layout);

    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(current_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .src_access_mask(src_access_mask)
        .dst_access_mask(dst_access_mask);
    unsafe {
        device.cmd_pipeline_barrier(
            cmdbuf,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        )
    }
}

/// A helper for determining the appropriate source (old layout) access mask
/// and pipeline stage.
///
/// - SrcStage represents what stage(s) we are waiting for.
/// - SrcAccessMask represents which part of writes performed should be made available.
///
/// Note that READ access is redundant for SrcAccessMask, as reading never
/// requires a cache flush.
///
/// See: https://themaister.net/blog/2019/08/14/yet-another-blog-explaining-vulkan-synchronization/
fn map_src_stage_access_flags(
    old_layout: vk::ImageLayout,
) -> (vk::AccessFlags, vk::PipelineStageFlags) {
    let general_shader_stages: vk::PipelineStageFlags =
        vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER;

    match old_layout {
        vk::ImageLayout::UNDEFINED => (
            vk::AccessFlags::empty(),
            vk::PipelineStageFlags::TOP_OF_PIPE,
        ),
        vk::ImageLayout::GENERAL => (vk::AccessFlags::SHADER_WRITE, general_shader_stages),
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL => {
            (vk::AccessFlags::empty(), vk::PipelineStageFlags::TRANSFER)
        }
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => (
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::TRANSFER,
        ),
        layout => {
            panic!("Unsupported old_layout transition from: {:?}", layout);
        }
    }
}

/// A helper for determining the appropriate destination (new layout) access mask
/// and pipeline stage.
///
/// - DstStage represents what stage(s) we are blocking.
/// - DstAccessMask represents which part of available memory to be made visible.
/// (By invalidating caches)
///
/// See: https://themaister.net/blog/2019/08/14/yet-another-blog-explaining-vulkan-synchronization/
fn map_dst_stage_access_flags(
    new_layout: vk::ImageLayout,
) -> (vk::AccessFlags, vk::PipelineStageFlags) {
    let general_shader_stages: vk::PipelineStageFlags =
        vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER;

    match new_layout {
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => {
            (vk::AccessFlags::SHADER_READ, general_shader_stages)
        }
        vk::ImageLayout::GENERAL => (
            vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            general_shader_stages,
        ),
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL => (
            vk::AccessFlags::TRANSFER_READ,
            vk::PipelineStageFlags::TRANSFER,
        ),
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => (
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::TRANSFER,
        ),
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => (
            vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        ),

        layout => {
            panic!("Unsupported new_layout transition to: {:?}", layout);
        }
    }
}
