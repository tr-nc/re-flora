use ash::{
    khr::swapchain,
    prelude::VkResult,
    vk::{self, PresentModeKHR, SurfaceCapabilitiesKHR, SurfaceFormatKHR},
};

use crate::{
    AttachmentDesc, AttachmentReference, Extent2D, Framebuffer, RenderPass, RenderPassDesc,
    RenderTarget, SubpassDesc, TextureLayout, TextureTransition,
};

use super::{
    context::VulkanContext, record_image_transition_barrier, Buffer, CommandBuffer, Device, Image,
    PresentDesc, PresentWait, Semaphore,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentMode {
    Mailbox,
    Immediate,
    Fifo,
    FifoRelaxed,
}

impl PresentMode {
    fn as_raw(self) -> vk::PresentModeKHR {
        match self {
            Self::Mailbox => vk::PresentModeKHR::MAILBOX,
            Self::Immediate => vk::PresentModeKHR::IMMEDIATE,
            Self::Fifo => vk::PresentModeKHR::FIFO,
            Self::FifoRelaxed => vk::PresentModeKHR::FIFO_RELAXED,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SwapchainFrameError {
    OutOfDate,
    Vulkan(String),
}

impl std::fmt::Display for SwapchainFrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfDate => write!(f, "swapchain is out of date"),
            Self::Vulkan(err) => write!(f, "vulkan swapchain error: {err}"),
        }
    }
}

impl std::error::Error for SwapchainFrameError {}

impl From<vk::Result> for SwapchainFrameError {
    fn from(value: vk::Result) -> Self {
        match value {
            vk::Result::ERROR_OUT_OF_DATE_KHR => Self::OutOfDate,
            other => Self::Vulkan(format!("{other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorReadbackFormat {
    Bgra8,
    Rgba8,
}

impl ColorReadbackFormat {
    fn from_raw(format: vk::Format) -> Option<Self> {
        match format {
            vk::Format::B8G8R8A8_SRGB | vk::Format::B8G8R8A8_UNORM => Some(Self::Bgra8),
            vk::Format::R8G8B8A8_SRGB | vk::Format::R8G8B8A8_UNORM => Some(Self::Rgba8),
            _ => None,
        }
    }

    pub fn convert_to_rgba(self, mut data: Vec<u8>) -> Vec<u8> {
        match self {
            Self::Bgra8 => {
                for pixel in data.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                    pixel[3] = 255;
                }
            }
            Self::Rgba8 => {
                for pixel in data.chunks_exact_mut(4) {
                    pixel[3] = 255;
                }
            }
        }
        data
    }
}

/// The preference for the swapchain.
///
/// Preferences are considered every time the swapchain is (re)created.
pub struct SwapchainDesc {
    pub format: vk::Format,
    pub color_space: vk::ColorSpaceKHR,
    pub present_mode: Option<PresentMode>,
    /// Override image count. None = auto (max(min_image_count, 3)).
    pub image_count_override: Option<u32>,
}

impl Default for SwapchainDesc {
    fn default() -> Self {
        Self {
            format: vk::Format::B8G8R8A8_SRGB,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
            present_mode: None,
            image_count_override: None,
        }
    }
}

pub struct Swapchain {
    vulkan_context: VulkanContext,

    swapchain_device: swapchain::Device,

    render_target: RenderTarget,
    image_views: Vec<vk::ImageView>,
    swapchain_khr: vk::SwapchainKHR,

    desc: SwapchainDesc,
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        self.clean_up();
    }
}

impl Swapchain {
    pub fn new(context: VulkanContext, window_extent: Extent2D, desc: SwapchainDesc) -> Self {
        let (swapchain_device, swapchain_khr, image_views, render_target) =
            create_vulkan_swapchain(&context, window_extent, &desc);

        Self {
            vulkan_context: context,
            render_target,
            image_views,
            swapchain_khr,
            swapchain_device,
            desc,
        }
    }

    pub fn on_resize(&mut self, window_extent: Extent2D) {
        self.clean_up();

        let (swapchain_device, swapchain_khr, image_views, render_target) =
            create_vulkan_swapchain(&self.vulkan_context, window_extent, &self.desc);

        self.swapchain_device = swapchain_device;
        self.swapchain_khr = swapchain_khr;
        self.render_target = render_target;
        self.image_views = image_views;
    }

    pub fn get_image(&self, index: u32) -> vk::Image {
        unsafe {
            self.swapchain_device
                .get_swapchain_images(self.swapchain_khr)
                .unwrap()[index as usize]
        }
    }

    pub fn image_count(&self) -> usize {
        self.image_views.len()
    }

    pub fn color_readback_format(&self) -> Option<ColorReadbackFormat> {
        ColorReadbackFormat::from_raw(self.render_target.get_desc().attachments[0].format)
    }

    fn clean_up(&mut self) {
        let device = &self.vulkan_context.device();
        unsafe {
            // framebuffers are now managed by RenderTarget and will be automatically cleaned up

            // image views
            self.image_views
                .iter()
                .for_each(|v| device.destroy_image_view(*v, None));
            self.image_views.clear();

            // images are owned by the swapchain, and are destroyed when the swapchain is destroyed

            self.swapchain_device
                .destroy_swapchain(self.swapchain_khr, None);
        }
    }

    #[allow(dead_code)]
    pub fn get_swapchain_device(&self) -> &swapchain::Device {
        &self.swapchain_device
    }

    fn acquire_next(&mut self, image_available_semaphore: &Semaphore) -> VkResult<(u32, bool)> {
        let timeout = u64::MAX;
        let fence = vk::Fence::null();
        unsafe {
            self.swapchain_device.acquire_next_image(
                self.swapchain_khr,
                timeout,
                image_available_semaphore.as_raw(),
                fence,
            )
        }
    }

    pub(crate) fn acquire_next_image(
        &mut self,
        image_available_semaphore: &Semaphore,
    ) -> Result<u32, SwapchainFrameError> {
        self.acquire_next(image_available_semaphore)
            .map(|(image_index, _)| image_index)
            .map_err(SwapchainFrameError::from)
    }

    /// Blits the source image to the destination image.
    /// The layout of src_img is transferred to GENERAL.
    pub fn record_blit(
        &self,
        src_img: &Image,
        cmdbuf: &CommandBuffer,
        image_idx: u32,
        dst_extent: Extent2D,
    ) {
        // the swapchain image is not wrapped because it is handled by the swapchain
        let dst_raw_img = self.get_image(image_idx);
        let device = self.vulkan_context.device();

        src_img.record_transition_barrier(cmdbuf, 0, TextureLayout::TRANSFER_SRC);

        // Transition the acquired swapchain image from UNDEFINED because it has just become available.
        record_image_transition_barrier(
            device.as_raw(),
            cmdbuf.as_raw(),
            TextureTransition::from_layouts(TextureLayout::UNDEFINED, TextureLayout::TRANSFER_DST),
            dst_raw_img,
            src_img.get_desc().get_aspect_mask(),
            0,
            1,
        );

        unsafe {
            device.cmd_blit_image(
                cmdbuf.as_raw(),
                src_img.as_raw(),
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                dst_raw_img,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageBlit {
                    dst_offsets: [
                        vk::Offset3D::default(),
                        vk::Offset3D {
                            x: dst_extent.width as i32,
                            y: dst_extent.height as i32,
                            z: 1,
                        },
                    ],
                    ..src_img.get_blit_region()
                }],
                vk::Filter::LINEAR,
            );
        }

        // Transition the swapchain image for the following GUI render pass.
        record_image_transition_barrier(
            device.as_raw(),
            cmdbuf.as_raw(),
            TextureTransition::from_layouts(
                TextureLayout::TRANSFER_DST,
                TextureLayout::COLOR_ATTACHMENT,
            ),
            dst_raw_img,
            src_img.get_desc().get_aspect_mask(),
            0,
            1,
        );

        // for now, just transition src to general
        src_img.record_transition_barrier(cmdbuf, 0, TextureLayout::GENERAL);
    }

    /// Present the image to the swapchain with the given index.
    fn present(
        &mut self,
        waiting_for_semaphores: &[vk::Semaphore],
        image_index: u32,
    ) -> VkResult<bool> {
        let swapchains = [self.swapchain_khr];
        let image_indices = [image_index];

        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(waiting_for_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        unsafe {
            self.swapchain_device.queue_present(
                self.vulkan_context.get_general_queue().as_raw(),
                &present_info,
            )
        }
    }

    pub fn present_desc(&mut self, desc: PresentDesc<'_>) -> VkResult<bool> {
        desc.assert_supported_sizes();
        crate::sync::diagnostics::record_present(&desc);

        let (raw_wait_semaphores, wait_count) = desc.raw_waits();
        let wait_semaphores = &raw_wait_semaphores[..wait_count];
        self.present(wait_semaphores, desc.image_index)
    }

    pub(crate) fn present_after(
        &mut self,
        waiting_for_semaphore: &Semaphore,
        image_index: u32,
    ) -> Result<bool, SwapchainFrameError> {
        let waits = [PresentWait::new("frame.render_finished", waiting_for_semaphore)];
        let desc = PresentDesc::new("swapchain.present", image_index, &waits);
        self.present_desc(desc).map_err(SwapchainFrameError::from)
    }

    pub fn record_downscaled_image_readback(
        &self,
        cmdbuf: &CommandBuffer,
        image_idx: u32,
        dst_image: &Image,
        readback_buffer: &Buffer,
        source_width: u32,
        source_height: u32,
    ) {
        let device = self.vulkan_context.device();
        let swapchain_image = self.get_image(image_idx);
        let dst_extent = dst_image.get_desc().extent;

        record_image_transition_barrier(
            device.as_raw(),
            cmdbuf.as_raw(),
            TextureTransition::from_layouts(TextureLayout::PRESENT_SRC, TextureLayout::TRANSFER_SRC),
            swapchain_image,
            vk::ImageAspectFlags::COLOR,
            0,
            1,
        );
        dst_image.record_transition_barrier(cmdbuf, 0, TextureLayout::TRANSFER_DST);

        let blit = vk::ImageBlit::default()
            .src_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: source_width as i32,
                    y: source_height as i32,
                    z: 1,
                },
            ])
            .dst_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .dst_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: dst_extent.width as i32,
                    y: dst_extent.height as i32,
                    z: 1,
                },
            ]);
        unsafe {
            device.cmd_blit_image(
                cmdbuf.as_raw(),
                swapchain_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                dst_image.as_raw(),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[blit],
                vk::Filter::LINEAR,
            );
        }

        dst_image.record_copy_to_buffer(
            cmdbuf,
            readback_buffer,
            TextureLayout::GENERAL,
        );
        record_image_transition_barrier(
            device.as_raw(),
            cmdbuf.as_raw(),
            TextureTransition::from_layouts(TextureLayout::TRANSFER_SRC, TextureLayout::PRESENT_SRC),
            swapchain_image,
            vk::ImageAspectFlags::COLOR,
            0,
            1,
        );
    }

    pub fn record_image_readback(
        &self,
        cmdbuf: &CommandBuffer,
        image_idx: u32,
        readback_buffer: &Buffer,
        width: u32,
        height: u32,
    ) {
        let device = self.vulkan_context.device();
        let swapchain_image = self.get_image(image_idx);

        record_image_transition_barrier(
            device.as_raw(),
            cmdbuf.as_raw(),
            TextureTransition::from_layouts(TextureLayout::PRESENT_SRC, TextureLayout::TRANSFER_SRC),
            swapchain_image,
            vk::ImageAspectFlags::COLOR,
            0,
            1,
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
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });

        unsafe {
            device.as_raw().cmd_copy_image_to_buffer(
                cmdbuf.as_raw(),
                swapchain_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                readback_buffer.as_raw(),
                &[region],
            );
        }

        record_image_transition_barrier(
            device.as_raw(),
            cmdbuf.as_raw(),
            TextureTransition::from_layouts(TextureLayout::TRANSFER_SRC, TextureLayout::PRESENT_SRC),
            swapchain_image,
            vk::ImageAspectFlags::COLOR,
            0,
            1,
        );
    }

    pub fn get_render_pass(&self) -> &RenderPass {
        self.render_target.get_render_pass()
    }

    pub fn record_begin_render_pass_cmdbuf(
        &self,
        cmdbuf: &CommandBuffer,
        image_index: u32,
        _render_area: Extent2D,
    ) {
        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 0.0],
            },
        }];

        self.render_target
            .record_begin_with_index(cmdbuf, image_index as usize, &clear_values);
    }

    pub fn record_prepare_image_for_render_pass(&self, cmdbuf: &CommandBuffer, image_index: u32) {
        let image = self.get_image(image_index);
        record_image_transition_barrier(
            self.vulkan_context.device().as_raw(),
            cmdbuf.as_raw(),
            TextureTransition::from_layouts(TextureLayout::UNDEFINED, TextureLayout::COLOR_ATTACHMENT),
            image,
            vk::ImageAspectFlags::COLOR,
            0,
            1,
        );
    }
}

fn print_swapchain_format_and_color_space(
    desired_format: vk::Format,
    desired_color_space: vk::ColorSpaceKHR,
    using_format: vk::Format,
    using_color_space: vk::ColorSpaceKHR,
) {
    let mut table = comfy_table::Table::new();
    table.set_header(vec!["Desired", "Using"]);

    table.add_row(vec![
        &format!("{:?}", desired_format),
        &format!("{:?}", using_format),
    ]);
    table.add_row(vec![
        &format!("{:?}", desired_color_space),
        &format!("{:?}", using_color_space),
    ]);

    log::info!("{}", table);
}

fn choose_surface_format(
    context: &VulkanContext,
    desired_format: vk::Format,
    desired_color_space: vk::ColorSpaceKHR,
) -> SurfaceFormatKHR {
    let format = {
        let formats = unsafe {
            context
                .surface()
                .surface_instance()
                .get_physical_device_surface_formats(
                    context.physical_device().as_raw(),
                    context.surface().surface_khr(),
                )
                .unwrap()
        };

        *formats
            .iter()
            .find(|format| {
                format.format == desired_format && format.color_space == desired_color_space
            })
            .unwrap_or(&formats[0])
    };
    print_swapchain_format_and_color_space(
        desired_format,
        desired_color_space,
        format.format,
        format.color_space,
    );
    format
}

fn choose_present_mode(
    context: &VulkanContext,
    requested_present_mode: Option<PresentMode>,
) -> PresentModeKHR {
    let present_modes = unsafe {
        context
            .surface()
            .surface_instance()
            .get_physical_device_surface_present_modes(
                context.physical_device().as_raw(),
                context.surface().surface_khr(),
            )
            .expect("Failed to get physical device surface present modes")
    };
    let supported_present_modes = present_modes
        .iter()
        .copied()
        .filter(|mode| {
            matches!(
                *mode,
                PresentModeKHR::MAILBOX
                    | PresentModeKHR::IMMEDIATE
                    | PresentModeKHR::FIFO
                    | PresentModeKHR::FIFO_RELAXED
            )
        })
        .collect::<Vec<_>>();
    log::info!("Available present modes: {:?}", supported_present_modes);

    let chosen_present_mode = if let Some(requested_present_mode) = requested_present_mode {
        let requested_present_mode_raw = requested_present_mode.as_raw();
        log::info!(
            "Preferred swapchain present mode: {:?}",
            requested_present_mode_raw
        );

        if !supported_present_modes.contains(&requested_present_mode_raw) {
            panic!(
                "Preferred swapchain present mode {:?} is not supported by this surface. Available present modes: {:?}",
                requested_present_mode_raw,
                supported_present_modes
            );
        }

        requested_present_mode_raw
    } else {
        log::info!("Preferred swapchain present mode: AUTO (MAILBOX -> FIFO -> first supported)");

        supported_present_modes
            .iter()
            .copied()
            .find(|mode| matches!(*mode, PresentModeKHR::MAILBOX | PresentModeKHR::FIFO))
            .or_else(|| supported_present_modes.first().copied())
            .expect("No supported common swapchain present mode available")
    };

    log::info!("Chosen swapchain present mode: {:?}", chosen_present_mode);
    chosen_present_mode
}

fn create_swapchain_device_khr(
    context: &VulkanContext,
    image_count: u32,
    format: SurfaceFormatKHR,
    extent: Extent2D,
    present_mode: PresentModeKHR,
    capabilities: SurfaceCapabilitiesKHR,
) -> (swapchain::Device, vk::SwapchainKHR) {
    let create_info = {
        let mut builder = vk::SwapchainCreateInfoKHR::default()
            .surface(context.surface().surface_khr())
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent.as_raw())
            .image_array_layers(1)
            .image_usage(
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST,
            );

        // if context.graphics_q_index != context.present_q_index, you may want to use concurrent mode
        // let families_indices = [context.graphics_q_index, context.present_q_index];
        //         .image_sharing_mode(vk::SharingMode::CONCURRENT)
        //         .queue_family_indices(&families_indices)

        builder = builder.image_sharing_mode(vk::SharingMode::EXCLUSIVE);

        builder
            .pre_transform(capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
    };

    let swapchain_device = swapchain::Device::new(context.instance().as_raw(), context.device());
    let swapchain_khr = unsafe {
        swapchain_device
            .create_swapchain(&create_info, None)
            .expect("Failed to create swapchain")
    };

    (swapchain_device, swapchain_khr)
}

fn create_vulkan_swapchain(
    vulkan_context: &VulkanContext,
    window_extent: Extent2D,
    swapchain_preference: &SwapchainDesc,
) -> (
    swapchain::Device,
    vk::SwapchainKHR,
    Vec<vk::ImageView>,
    RenderTarget,
) {
    let format = choose_surface_format(
        vulkan_context,
        swapchain_preference.format,
        swapchain_preference.color_space,
    );
    let present_mode = choose_present_mode(vulkan_context, swapchain_preference.present_mode);

    let extent = Extent2D {
        width: window_extent.width,
        height: window_extent.height,
    };

    let capabilities: SurfaceCapabilitiesKHR = unsafe {
        vulkan_context
            .surface()
            .surface_instance()
            .get_physical_device_surface_capabilities(
                vulkan_context.physical_device().as_raw(),
                vulkan_context.surface().surface_khr(),
            )
            .expect("Failed to get physical device surface capabilities")
    };

    let mut image_count = if let Some(override_count) = swapchain_preference.image_count_override {
        override_count.max(capabilities.min_image_count)
    } else {
        let preferred_default = if present_mode == PresentModeKHR::MAILBOX {
            2
        } else {
            3
        };
        capabilities.min_image_count.max(preferred_default)
    };
    if capabilities.max_image_count > 0 {
        image_count = image_count.min(capabilities.max_image_count);
    }
    log::info!(
        "Swapchain image count: min={}, max={}, using={} (override={:?})",
        capabilities.min_image_count,
        capabilities.max_image_count,
        image_count,
        swapchain_preference.image_count_override,
    );

    let (swapchain_device, swapchain_khr) = create_swapchain_device_khr(
        vulkan_context,
        image_count,
        format,
        extent,
        present_mode,
        capabilities,
    );

    let images = unsafe {
        swapchain_device
            .get_swapchain_images(swapchain_khr)
            .expect("Failed to get swapchain images")
    };

    let image_views = images
        .iter()
        .map(|image| {
            let create_info = vk::ImageViewCreateInfo::default()
                .image(*image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format.format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            unsafe {
                vulkan_context
                    .device()
                    .as_raw()
                    .create_image_view(&create_info, None)
            }
        })
        .collect::<VkResult<Vec<vk::ImageView>>>()
        .unwrap();

    let render_pass = create_vulkan_render_pass(vulkan_context.device().clone(), format.format);

    let framebuffers = create_vulkan_framebuffers(
        vulkan_context.clone(),
        &render_pass,
        &image_views,
        window_extent,
    );

    let render_target = RenderTarget::new(render_pass, framebuffers);

    (swapchain_device, swapchain_khr, image_views, render_target)
}

fn create_vulkan_render_pass(device: Device, format: vk::Format) -> RenderPass {
    let color_attachment = AttachmentDesc {
        format,
        samples: vk::SampleCountFlags::TYPE_1,
        load_op: vk::AttachmentLoadOp::LOAD,
        store_op: vk::AttachmentStoreOp::STORE,
        stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
        stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
        initial_layout: TextureLayout::COLOR_ATTACHMENT,
        final_layout: TextureLayout::PRESENT_SRC,
    };

    let subpass = SubpassDesc {
        color_attachments: vec![AttachmentReference {
            attachment: 0,
            layout: TextureLayout::COLOR_ATTACHMENT,
        }],
        depth_stencil_attachment: None,
    };

    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        );

    let desc = RenderPassDesc {
        attachments: vec![color_attachment],
        subpasses: vec![subpass],
        dependencies: vec![dependency],
    };

    RenderPass::from_desc(device, desc)
}

fn create_vulkan_framebuffers(
    vulkan_context: VulkanContext,
    render_pass: &RenderPass,
    image_views: &[vk::ImageView],
    window_extent: Extent2D,
) -> Vec<Framebuffer> {
    image_views
        .iter()
        .map(|view| {
            Framebuffer::new(vulkan_context.clone(), render_pass, &[*view], window_extent)
                .expect("Failed to create framebuffer")
        })
        .collect()
}
