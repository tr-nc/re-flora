use super::{ImageDesc, TextureRegion};
use crate::{
    execute_one_time_command, record_image_transition_barrier, Allocator, Buffer, BufferUsage,
    CommandBuffer, CommandPool, Device, MemoryLocation, Queue, ResourceState, TextureLayout,
    TextureTransition,
};
use anyhow::Result;
use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};
use std::fmt;
use std::sync::{Arc, Mutex};

struct ImageInner {
    device: Device,
    desc: ImageDesc,
    image: vk::Image,
    allocator: Allocator,
    allocated_mem: Allocation,
    current_state: Mutex<Vec<ResourceState>>,
    size: vk::DeviceSize,
}

impl Drop for ImageInner {
    fn drop(&mut self) {
        let allocated_mem = std::mem::take(&mut self.allocated_mem);
        self.allocator.destroy_image(self.image, allocated_mem);
    }
}

pub enum ColorClearValue {
    #[allow(dead_code)]
    UInt([u32; 4]),
    #[allow(dead_code)]
    Float([f32; 4]),
    #[allow(dead_code)]
    Int([i32; 4]),
}

pub enum DepthOrStencilClearValue {
    #[allow(dead_code)]
    DepthAndStencil(f32, u32),
    #[allow(dead_code)]
    Depth(f32),
    #[allow(dead_code)]
    Stencil(u32),
}

pub enum ClearValue {
    Color(ColorClearValue),
    DepthStencil(DepthOrStencilClearValue),
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
    pub fn new(device: Device, mut allocator: Allocator, desc: &ImageDesc) -> Result<Self> {
        // for vulkan spec, initial_layout must be either UNDEFINED or PREINITIALIZED,
        if desc.initial_layout != TextureLayout::UNDEFINED
            && desc.initial_layout != TextureLayout::PREINITIALIZED
        {
            return Err(anyhow::anyhow!("Initial layout must be UNDEFINED"));
        }

        let image_info = vk::ImageCreateInfo::default()
            .extent(desc.extent.as_raw())
            .image_type(desc.get_image_type())
            .mip_levels(1)
            .array_layers(desc.array_len)
            .format(desc.format)
            .tiling(desc.tilting)
            .initial_layout(TextureLayout::UNDEFINED.as_raw())
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
                location: MemoryLocation::GpuOnly.into(),
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .expect("Failed to allocate image memory");

        unsafe {
            device
                .bind_image_memory(image, allocated_mem.memory(), allocated_mem.offset())
                .unwrap()
        };

        let size = desc.extent.width as vk::DeviceSize
            * desc.extent.height as vk::DeviceSize
            * desc.extent.depth as vk::DeviceSize
            * desc.get_pixel_size() as vk::DeviceSize;

        // initialize one entry per array layer
        let states = vec![ResourceState::from_layout(desc.initial_layout); desc.array_len as usize];

        Ok(Self(Arc::new(ImageInner {
            device: device.clone(),
            image,
            desc: *desc,
            allocator,
            allocated_mem,
            current_state: Mutex::new(states),
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
        dst_image_layout: TextureLayout,
        array_layer: u32,
        region: TextureRegion,
    ) {
        execute_one_time_command(&self.0.device, command_pool, queue, |cmdbuf| {
            self.record_transition_barrier(cmdbuf, array_layer, TextureLayout::TRANSFER_SRC);
            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: self.0.desc.get_aspect_mask(),
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
                    width: region.extent.width,
                    height: region.extent.height,
                    depth: region.extent.depth,
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

    pub fn record_blit_to(&self, cmdbuf: &CommandBuffer, dst_img: &Image, filter: vk::Filter) {
        self.record_transition_barrier(cmdbuf, 0, TextureLayout::TRANSFER_SRC);
        dst_img.record_transition_barrier(cmdbuf, 0, TextureLayout::TRANSFER_DST);
        let src = self.get_desc().extent;
        let dst = dst_img.get_desc().extent;
        let region = vk::ImageBlit::default()
            .src_subresource(vk::ImageSubresourceLayers {
                aspect_mask: self.0.desc.get_aspect_mask(),
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: src.width as i32,
                    y: src.height as i32,
                    z: src.depth as i32,
                },
            ])
            .dst_subresource(vk::ImageSubresourceLayers {
                aspect_mask: dst_img.get_desc().get_aspect_mask(),
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .dst_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: dst.width as i32,
                    y: dst.height as i32,
                    z: dst.depth as i32,
                },
            ]);
        unsafe {
            self.0.device.cmd_blit_image(
                cmdbuf.as_raw(),
                self.as_raw(),
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                dst_img.as_raw(),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
                filter,
            );
        }
        self.record_transition_barrier(cmdbuf, 0, TextureLayout::GENERAL);
        dst_img.record_transition_barrier(cmdbuf, 0, TextureLayout::GENERAL);
    }

    pub fn record_copy_to_buffer(
        &self,
        cmdbuf: &CommandBuffer,
        buffer: &Buffer,
        final_layout: TextureLayout,
    ) {
        self.record_transition_barrier(cmdbuf, 0, TextureLayout::TRANSFER_SRC);
        let region = vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: self.0.desc.get_aspect_mask(),
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_extent(vk::Extent3D {
                width: self.0.desc.extent.width,
                height: self.0.desc.extent.height,
                depth: self.0.desc.extent.depth,
            });
        unsafe {
            self.0.device.cmd_copy_image_to_buffer(
                cmdbuf.as_raw(),
                self.as_raw(),
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                buffer.as_raw(),
                &[region],
            );
        }
        self.record_transition_barrier(cmdbuf, 0, final_layout);
    }

    pub fn record_copy_to(
        &self,
        cmdbuf: &CommandBuffer,
        dst_img: &Image,
        src_img_dst_layout: TextureLayout,
        dst_img_dst_layout: TextureLayout,
    ) {
        self.record_transition_barrier(cmdbuf, 0, TextureLayout::TRANSFER_SRC);
        dst_img.record_transition_barrier(cmdbuf, 0, TextureLayout::TRANSFER_DST);

        unsafe {
            self.0.device.cmd_copy_image(
                cmdbuf.as_raw(),
                self.as_raw(),
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                dst_img.as_raw(),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[self.get_copy_region()],
            );
        }

        self.record_transition_barrier(cmdbuf, 0, src_img_dst_layout);
        dst_img.record_transition_barrier(cmdbuf, 0, dst_img_dst_layout);
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
            extent: self.0.desc.extent.as_raw(),
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
            x: self.0.desc.extent.width as i32,
            y: self.0.desc.extent.height as i32,
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
        layout_after_clear: Option<TextureLayout>,
        base_array_layer: u32,
        clear_value: ClearValue,
    ) {
        let target_layout = layout_after_clear.unwrap_or_else(|| self.get_layout(base_array_layer));
        const LAYOUT_USED_TO_CLEAR: TextureLayout = TextureLayout::TRANSFER_DST;
        self.record_transition_barrier(cmdbuf, base_array_layer, LAYOUT_USED_TO_CLEAR);

        if let ClearValue::Color(color_clear_value) = &clear_value {
            let clear_value = match color_clear_value {
                ColorClearValue::UInt(v) => vk::ClearColorValue { uint32: *v },
                ColorClearValue::Float(v) => vk::ClearColorValue { float32: *v },
                ColorClearValue::Int(v) => vk::ClearColorValue { int32: *v },
            };
            // imageLayout specifies the current layout of the image subresource ranges to be cleared,
            // and must be VK_IMAGE_LAYOUT_SHARED_PRESENT_KHR, VK_IMAGE_LAYOUT_GENERAL or VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL.
            unsafe {
                self.0.device.cmd_clear_color_image(
                    cmdbuf.as_raw(),
                    self.0.image,
                    LAYOUT_USED_TO_CLEAR.as_raw(),
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
        }
        if let ClearValue::DepthStencil(depth_stencil_clear_value) = &clear_value {
            let (clear_value, aspect_mask) = match depth_stencil_clear_value {
                DepthOrStencilClearValue::DepthAndStencil(depth, stencil) => (
                    vk::ClearDepthStencilValue {
                        depth: *depth,
                        stencil: *stencil,
                    },
                    vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
                ),
                DepthOrStencilClearValue::Depth(depth) => (
                    vk::ClearDepthStencilValue {
                        depth: *depth,
                        stencil: 0,
                    },
                    vk::ImageAspectFlags::DEPTH,
                ),
                DepthOrStencilClearValue::Stencil(stencil) => (
                    vk::ClearDepthStencilValue {
                        depth: 0.0,
                        stencil: *stencil,
                    },
                    vk::ImageAspectFlags::STENCIL,
                ),
            };
            unsafe {
                self.0.device.cmd_clear_depth_stencil_image(
                    cmdbuf.as_raw(),
                    self.0.image,
                    LAYOUT_USED_TO_CLEAR.as_raw(),
                    &clear_value,
                    &[vk::ImageSubresourceRange {
                        aspect_mask,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer,
                        layer_count: 1,
                    }],
                );
            }
        }

        self.record_transition_barrier(cmdbuf, base_array_layer, target_layout);
    }

    /// Transition just `array_layer` from its current layout to `target_layout`.
    pub fn record_transition(
        &self,
        cmdbuf: &CommandBuffer,
        array_layer: u32,
        target_layout: TextureLayout,
    ) {
        self.record_transition_barrier(cmdbuf, array_layer, target_layout);
    }

    /// Transition just `array_layer` from its current layout → `target_layout`
    pub(crate) fn record_transition_barrier(
        &self,
        cmdbuf: &CommandBuffer,
        array_layer: u32,
        target_layout: TextureLayout,
    ) {
        self.record_state_transition(
            cmdbuf,
            array_layer,
            1,
            ResourceState::from_layout(target_layout),
        );
    }

    /// Transition one or more array layers from their tracked states to `target_state`.
    pub(crate) fn record_state_transition(
        &self,
        cmdbuf: &CommandBuffer,
        base_array_layer: u32,
        layer_count: u32,
        target_state: ResourceState,
    ) {
        let device = &self.0.device;
        let mut states = self.0.current_state.lock().unwrap();
        let start = base_array_layer as usize;
        let end = start + layer_count as usize;
        assert!(
            end <= states.len(),
            "image state transition layer range {}..{} exceeds array length {}",
            base_array_layer,
            base_array_layer + layer_count,
            states.len()
        );

        let mut run_start = base_array_layer;
        let mut run_len = 0_u32;
        let mut run_old_state = None;

        for layer in base_array_layer..base_array_layer + layer_count {
            let old_state = states[layer as usize];
            if old_state == target_state {
                if let Some(old_state) = run_old_state.take() {
                    record_image_transition_barrier(
                        device.as_raw(),
                        cmdbuf.as_raw(),
                        TextureTransition::new(old_state, target_state),
                        self.0.image,
                        self.0.desc.get_aspect_mask(),
                        run_start,
                        run_len,
                    );
                    run_len = 0;
                }
                continue;
            }

            if run_old_state == Some(old_state) {
                run_len += 1;
            } else {
                if let Some(prev_old_state) = run_old_state {
                    record_image_transition_barrier(
                        device.as_raw(),
                        cmdbuf.as_raw(),
                        TextureTransition::new(prev_old_state, target_state),
                        self.0.image,
                        self.0.desc.get_aspect_mask(),
                        run_start,
                        run_len,
                    );
                }
                run_start = layer;
                run_len = 1;
                run_old_state = Some(old_state);
            }
        }

        if let Some(old_state) = run_old_state {
            record_image_transition_barrier(
                device.as_raw(),
                cmdbuf.as_raw(),
                TextureTransition::new(old_state, target_state),
                self.0.image,
                self.0.desc.get_aspect_mask(),
                run_start,
                run_len,
            );
        }

        for state in &mut states[start..end] {
            *state = target_state;
        }
    }

    /// Force set the layout for the given array layer.
    #[allow(dead_code)]
    pub fn set_layout(&self, array_layer: u32, new_layout: TextureLayout) {
        self.set_state(array_layer, ResourceState::from_layout(new_layout));
    }

    /// Force set the tracked state for the given array layer.
    pub fn set_state(&self, array_layer: u32, new_state: ResourceState) {
        let mut states = self.0.current_state.lock().unwrap();
        states[array_layer as usize] = new_state;
    }

    /// Loads an RGBA image from the given path and checks if it has the same size as the texture.
    fn load_same_sized_image_as_raw_u8(&self, path: &str) -> Result<Vec<u8>> {
        let image =
            image::open(path).map_err(|e| anyhow::anyhow!("Failed to open image: {}", e))?;
        let rgba_image = image.to_rgba8();
        let (width, height) = rgba_image.dimensions();
        if width != self.0.desc.extent.width || height != self.0.desc.extent.height {
            return Err(anyhow::anyhow!(
                "Image size does not match texture size: {}x{} != {}x{}",
                width,
                height,
                self.0.desc.extent.width,
                self.0.desc.extent.height
            ));
        }
        if self.0.desc.extent.depth != 1 {
            return Err(anyhow::anyhow!(
                "Image depth must be 1, but got {}",
                self.0.desc.extent.depth
            ));
        }
        let mut data = rgba_image.into_raw();
        data = self
            .convert_rgba_data_to_image_format(&data)
            .map_err(|e| anyhow::anyhow!("Failed to convert image data: {}", e))?;
        Ok(data)
    }

    fn convert_rgba_data_to_image_format(&self, data: &[u8]) -> Result<Vec<u8>> {
        use ash::vk::Format;
        let fmt = self.0.desc.format;
        // data is &[R, G, B, A,  R, G, B, A,  …]
        match fmt {
            Format::R8G8B8A8_UNORM => {
                // already in RGBA8 – just clone
                Ok(data.to_vec())
            }
            Format::R8_UNORM => {
                // keep only R
                if !data.len().is_multiple_of(4) {
                    return Err(anyhow::anyhow!("Input RGBA data length not divisible by 4"));
                }
                let mut out = Vec::with_capacity(data.len() / 4);
                for pixel in data.chunks_exact(4) {
                    out.push(pixel[0]);
                }
                Ok(out)
            }
            Format::R8G8_UNORM => {
                // keep R and G
                if !data.len().is_multiple_of(4) {
                    return Err(anyhow::anyhow!("Input RGBA data length not divisible by 4"));
                }
                let mut out = Vec::with_capacity(data.len() / 2);
                for pixel in data.chunks_exact(4) {
                    out.push(pixel[0]);
                    out.push(pixel[1]);
                }
                Ok(out)
            }
            other => Err(anyhow::anyhow!(
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
        dst_image_layout: Option<TextureLayout>,
    ) -> Result<()> {
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
        dst_image_layout: Option<TextureLayout>,
    ) -> Result<()> {
        let device = &self.0.device;

        let buffer = Buffer::new_sized(
            device.clone(),
            self.get_allocator().clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_SRC),
            MemoryLocation::CpuToGpu,
            data.len() as _,
        );
        buffer
            .fill(data)
            .map_err(|e| anyhow::anyhow!("Failed to fill buffer: {}", e))?;

        let target_layout = dst_image_layout.unwrap_or_else(|| self.get_layout(array_layer));

        execute_one_time_command(device, command_pool, queue, |cmdbuf| {
            self.record_transition_barrier(cmdbuf, array_layer, TextureLayout::TRANSFER_DST);
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
                .image_extent(region.extent.as_raw());
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
    pub fn fetch_data(&self, queue: &Queue, command_pool: &CommandPool) -> Result<Vec<u8>> {
        let device = &self.0.device;

        let buffer = Buffer::new_sized(
            device.clone(),
            self.get_allocator().clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            self.get_size() as _,
        );

        execute_one_time_command(device, command_pool, queue, |cmdbuf| {
            self.record_transition_barrier(cmdbuf, 0, TextureLayout::TRANSFER_SRC);
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
                .image_extent(self.get_desc().extent.as_raw());
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

    pub fn get_layout(&self, array_layer: u32) -> TextureLayout {
        self.get_state(array_layer).layout()
    }

    pub fn get_state(&self, array_layer: u32) -> ResourceState {
        *self
            .0
            .current_state
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

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("image", &self.0.image)
            .field("desc", &self.0.desc)
            .field("size", &self.0.size)
            .field("current_state", &self.0.current_state)
            .finish()
    }
}
