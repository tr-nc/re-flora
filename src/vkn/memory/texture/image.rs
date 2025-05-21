use super::{ImageDesc, TextureRegion};
use crate::vkn::{
    execute_one_time_command, Allocator, Buffer, BufferUsage, CommandBuffer, CommandPool, Device,
    Queue,
};
use ash::vk::{self, ImageLayout};
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme},
    MemoryLocation,
};
use std::sync::{Arc, Mutex};

struct ImageInner {
    device: Device,
    desc: ImageDesc,
    image: vk::Image,
    allocator: Allocator,
    allocated_mem: Allocation,
    current_layout: Mutex<Vec<vk::ImageLayout>>,
    size: vk::DeviceSize,
}

impl Drop for ImageInner {
    fn drop(&mut self) {
        let allocated_mem = std::mem::take(&mut self.allocated_mem);
        self.allocator.destroy_image(self.image, allocated_mem);
    }
}

#[allow(dead_code)]
pub enum ClearValue {
    UInt([u32; 4]),
    Float([f32; 4]),
    Int([i32; 4]),
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
    pub fn new(device: Device, mut allocator: Allocator, desc: &ImageDesc) -> Result<Self, String> {
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
            .array_layers(desc.array_len)
            .format(desc.format)
            .tiling(desc.tilting)
            .initial_layout(ImageLayout::UNDEFINED)
            .usage(desc.usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(desc.samples)
            .flags(vk::ImageCreateFlags::empty());

        let image = unsafe { device.create_image(&image_info, None).unwrap() };
        let requirements = unsafe { device.get_image_memory_requirements(image) };

        let allocated_mem = allocator
            .allocate_memory(&AllocationCreateDesc {
                name: "",
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .expect("Failed to allocate image memory");

        unsafe {
            device
                .bind_image_memory(image, allocated_mem.memory(), allocated_mem.offset())
                .unwrap()
        };

        let size = desc.extent[0] as vk::DeviceSize
            * desc.extent[1] as vk::DeviceSize
            * desc.extent[2] as vk::DeviceSize
            * desc.get_pixel_size() as vk::DeviceSize;

        // initialize one entry per array layer
        let layouts = vec![desc.initial_layout; desc.array_len as usize];

        Ok(Self(Arc::new(ImageInner {
            device: device.clone(),
            image,
            desc: desc.clone(),
            allocator,
            allocated_mem,
            current_layout: Mutex::new(layouts),
            size,
        })))
    }

    pub fn get_desc(&self) -> &ImageDesc {
        &self.0.desc
    }

    #[allow(dead_code)]
    pub fn copy_image_to_buffer(
        &self,
        buffer: &mut Buffer,
        queue: &Queue,
        command_pool: &CommandPool,
        dst_image_layout: vk::ImageLayout,
        array_layer: u32,
        region: TextureRegion,
    ) {
        execute_one_time_command(&self.0.device, command_pool, queue, |cmdbuf| {
            self.record_transition_barrier(
                cmdbuf,
                array_layer,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D {
                    x: region.offset[0],
                    y: region.offset[1],
                    z: region.offset[2],
                })
                .image_extent(vk::Extent3D {
                    width: region.extent[0],
                    height: region.extent[1],
                    depth: region.extent[2],
                });
            unsafe {
                self.0.device.cmd_copy_image_to_buffer(
                    cmdbuf.as_raw(),
                    self.as_raw(),
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    buffer.as_raw(),
                    &[region],
                )
            }
            self.record_transition_barrier(cmdbuf, array_layer, dst_image_layout);
        });
    }

    #[allow(dead_code)]
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

    pub fn get_size(&self) -> vk::DeviceSize {
        self.0.size
    }

    /// Compared to `get_copy_region`, this is a blit region that can take the image color space
    /// into account.
    pub fn get_blit_region(&self) -> vk::ImageBlit {
        let offset_min = vk::Offset3D { x: 0, y: 0, z: 0 };
        let offset_max = vk::Offset3D {
            x: self.0.desc.get_extent().width as i32,
            y: self.0.desc.get_extent().height as i32,
            z: 1,
        };
        let offsets = [offset_min, offset_max];

        vk::ImageBlit {
            src_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            src_offsets: offsets,
            dst_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            dst_offsets: offsets,
        }
    }

    pub fn record_clear(
        &self,
        cmdbuf: &CommandBuffer,
        layout_after_clear: Option<vk::ImageLayout>,
        base_array_layer: u32,
        clear_value: ClearValue,
    ) {
        let target_layout = layout_after_clear.unwrap_or(self.get_layout(base_array_layer));
        const LAYOUT_USED_TO_CLEAR: vk::ImageLayout = vk::ImageLayout::GENERAL;
        self.record_transition_barrier(cmdbuf, base_array_layer, LAYOUT_USED_TO_CLEAR);

        let clear_value = match clear_value {
            ClearValue::UInt(v) => vk::ClearColorValue { uint32: v },
            ClearValue::Float(v) => vk::ClearColorValue { float32: v },
            ClearValue::Int(v) => vk::ClearColorValue { int32: v },
        };

        // imageLayout specifies the current layout of the image subresource ranges to be cleared,
        // and must be VK_IMAGE_LAYOUT_SHARED_PRESENT_KHR, VK_IMAGE_LAYOUT_GENERAL or VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL.
        unsafe {
            self.0.device.cmd_clear_color_image(
                cmdbuf.as_raw(),
                self.0.image,
                LAYOUT_USED_TO_CLEAR,
                &clear_value,
                &[vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer,
                    layer_count: 1,
                }],
            );
        }
        self.record_transition_barrier(cmdbuf, base_array_layer, target_layout);
    }

    /// Transition just `array_layer` from its current layout → `target_layout`
    pub fn record_transition_barrier(
        &self,
        cmdbuf: &CommandBuffer,
        array_layer: u32,
        target_layout: vk::ImageLayout,
    ) {
        let device = &self.0.device;
        let mut layouts = self.0.current_layout.lock().unwrap();
        let idx = array_layer as usize;
        let old_layout = layouts[idx];

        if old_layout == target_layout {
            return;
        }

        // emit a barrier for exactly one layer
        record_image_transition_barrier(
            device.as_raw(),
            cmdbuf.as_raw(),
            old_layout,
            target_layout,
            self.0.image,
            array_layer,
            1, // only one layer
        );

        // update our tracked layout
        layouts[idx] = target_layout;
    }
    
    /// Loads an RGBA image from the given path and checks if it has the same size as the texture.
    fn load_same_sized_image_as_raw_u8(&self, path: &str) -> Result<Vec<u8>, String> {
        let image = image::open(path).map_err(|e| format!("Failed to open image: {}", e))?;
        let rgba_image = image.to_rgba8();
        let (width, height) = rgba_image.dimensions();
        if width != self.0.desc.extent[0] as u32 || height != self.0.desc.extent[1] as u32 {
            return Err(format!(
                "Image size does not match texture size: {}x{} != {}x{}",
                width, height, self.0.desc.extent[0], self.0.desc.extent[1]
            ));
        }
        if self.0.desc.extent[2] != 1 {
            return Err(format!(
                "Image depth must be 1, but got {}",
                self.0.desc.extent[2]
            ));
        }
        let mut data = rgba_image.into_raw();
        data = self
            .convert_rgba_data_to_image_format(&data)
            .map_err(|e| format!("Failed to convert image data: {}", e))?;
        Ok(data)
    }

    fn convert_rgba_data_to_image_format(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        use ash::vk::Format;
        let fmt = self.0.desc.format;
        // data is &[R, G, B, A,  R, G, B, A,  …]
        match fmt {
            Format::R8G8B8A8_UNORM => {
                // Already in RGBA8 – just clone
                Ok(data.to_vec())
            }
            Format::R8_UNORM => {
                // Keep only R
                if data.len() % 4 != 0 {
                    return Err("Input RGBA data length not divisible by 4".into());
                }
                let mut out = Vec::with_capacity(data.len() / 4);
                for pixel in data.chunks_exact(4) {
                    out.push(pixel[0]);
                }
                Ok(out)
            }
            Format::R8G8_UNORM => {
                // Keep R and G
                if data.len() % 4 != 0 {
                    return Err("Input RGBA data length not divisible by 4".into());
                }
                let mut out = Vec::with_capacity(data.len() / 2);
                for pixel in data.chunks_exact(4) {
                    out.push(pixel[0]);
                    out.push(pixel[1]);
                }
                Ok(out)
            }
            other => Err(format!(
                "Unsupported image format for RGBA→raw conversion: {:?}",
                other
            )),
        }
    }

    /// Loads an RGBA image from the given path and fills the texture with it.
    ///
    /// The image is transitioned into `dst_image_layout` after the copy.
    /// If `dst_image_layout` is `None`, the image is transitioned back to where it was before the copy.
    pub fn load_and_fill(
        &self,
        queue: &Queue,
        command_pool: &CommandPool,
        path: &str,
        array_layer: u32,
        dst_image_layout: Option<vk::ImageLayout>,
    ) -> Result<(), String> {
        let data = self.load_same_sized_image_as_raw_u8(path)?;
        let region = TextureRegion::from_image(self);
        self.fill_with_raw_u8(
            queue,
            command_pool,
            region,
            &data,
            array_layer,
            dst_image_layout,
        )
    }

    /// Uploads an RGBA image to the texture. The image is transitioned into `dst_image_layout` after the copy.
    ///
    /// The image is transitioned into `dst_image_layout` after the copy.
    /// If `dst_image_layout` is `None`, the image is transitioned back to where it was before the copy.
    pub fn fill_with_raw_u8(
        &self,
        queue: &Queue,
        command_pool: &CommandPool,
        region: TextureRegion,
        data: &[u8],
        array_layer: u32,
        dst_image_layout: Option<vk::ImageLayout>,
    ) -> Result<(), String> {
        let device = &self.0.device;

        let buffer = Buffer::new_sized(
            device.clone(),
            self.get_allocator().clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_SRC),
            gpu_allocator::MemoryLocation::CpuToGpu,
            data.len() as _,
        );
        buffer
            .fill(data)
            .map_err(|e| format!("Failed to fill buffer: {}", e))?;

        let target_layout = dst_image_layout.unwrap_or(self.get_layout(array_layer));

        execute_one_time_command(device, command_pool, queue, |cmdbuf| {
            self.record_transition_barrier(
                cmdbuf,
                array_layer,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: array_layer,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D {
                    x: region.offset[0],
                    y: region.offset[1],
                    z: region.offset[2],
                })
                .image_extent(vk::Extent3D {
                    width: region.extent[0],
                    height: region.extent[1],
                    depth: region.extent[2],
                });
            unsafe {
                device.cmd_copy_buffer_to_image(
                    cmdbuf.as_raw(),
                    buffer.as_raw(),
                    self.as_raw(),
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                )
            }
            self.record_transition_barrier(cmdbuf, array_layer, target_layout);
        });
        Ok(())
    }

    /// Obtain the image data from the texture of the full image region.
    // TODO: Add support for regions and other formats. Add support for
    // array layers.
    #[allow(dead_code)]
    pub fn fetch_data(&self, queue: &Queue, command_pool: &CommandPool) -> Result<Vec<u8>, String> {
        let device = &self.0.device;

        let buffer = Buffer::new_sized(
            device.clone(),
            self.get_allocator().clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            gpu_allocator::MemoryLocation::GpuToCpu,
            self.get_size() as _,
        );

        execute_one_time_command(device, command_pool, queue, |cmdbuf| {
            self.record_transition_barrier(cmdbuf, 0, vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width: self.get_desc().extent[0],
                    height: self.get_desc().extent[1],
                    depth: self.get_desc().extent[2],
                });
            unsafe {
                device.cmd_copy_image_to_buffer(
                    cmdbuf.as_raw(),
                    self.as_raw(),
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    buffer.as_raw(),
                    &[region],
                )
            }
        });

        let fetched_data = buffer.read_back()?;
        Ok(fetched_data)
    }

    pub fn get_layout(&self, array_layer: u32) -> vk::ImageLayout {
        *self
            .0
            .current_layout
            .lock()
            .unwrap()
            .get(array_layer as usize)
            .unwrap()
    }

    pub fn as_raw(&self) -> vk::Image {
        self.0.image
    }

    pub fn get_allocator(&self) -> &Allocator {
        &self.0.allocator
    }
}

/// Record a transition barrier for one subresource‐range of an image
/// (you now provide base_array_layer + layer_count explicitly).
pub fn record_image_transition_barrier(
    device: &ash::Device,
    cmdbuf: vk::CommandBuffer,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    image: vk::Image,
    base_array_layer: u32,
    layer_count: u32,
) {
    let (src_access, src_stage) = map_src_stage_access_flags(old_layout);
    let (dst_access, dst_stage) = map_dst_stage_access_flags(new_layout);

    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer,
            layer_count,
        })
        .src_access_mask(src_access)
        .dst_access_mask(dst_access);

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
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => {
            (vk::AccessFlags::SHADER_READ, general_shader_stages)
        }
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
