use ash::{
    khr::swapchain,
    prelude::VkResult,
    vk::{self, Extent2D, PresentModeKHR, SurfaceCapabilitiesKHR, SurfaceFormatKHR},
};

use crate::vkn::{
    RenderPass, RenderPassDesc, {AttachmentDesc, AttachmentReference, SubpassDesc},
};

use super::{
    context::VulkanContext, record_image_transition_barrier, CommandBuffer, Device, Image,
    Semaphore,
};

/// The preference for the swapchain.
///
/// Preferences are considered every time the swapchain is (re)created.
pub struct SwapchainDesc {
    pub format: vk::Format,
    pub color_space: vk::ColorSpaceKHR,
    pub present_mode: vk::PresentModeKHR,
}

impl Default for SwapchainDesc {
    fn default() -> Self {
        Self {
            format: vk::Format::B8G8R8A8_SRGB,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
            present_mode: vk::PresentModeKHR::MAILBOX,
        }
    }
}

pub struct Swapchain {
    vulkan_context: VulkanContext,

    swapchain_device: swapchain::Device,

    render_pass: RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
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
    pub fn new(context: VulkanContext, window_size: &[u32; 2], desc: SwapchainDesc) -> Self {
        let (swapchain_device, swapchain_khr, image_views, render_pass, framebuffers) =
            create_vulkan_swapchain(&context, window_size, &desc);

        Self {
            vulkan_context: context,
            render_pass,
            framebuffers,
            image_views,
            swapchain_khr,
            swapchain_device,
            desc,
        }
    }

    pub fn on_resize(&mut self, window_size: &[u32; 2]) {
        self.clean_up();

        let (swapchain_device, swapchain_khr, image_views, render_pass, framebuffers) =
            create_vulkan_swapchain(&self.vulkan_context, window_size, &self.desc);

        self.swapchain_device = swapchain_device;
        self.swapchain_khr = swapchain_khr;
        self.render_pass = render_pass;
        self.framebuffers = framebuffers;
        self.image_views = image_views;
    }

    pub fn get_image(&self, index: u32) -> vk::Image {
        unsafe {
            self.swapchain_device
                .get_swapchain_images(self.swapchain_khr)
                .unwrap()[index as usize]
        }
    }

    fn clean_up(&mut self) {
        let device = &self.vulkan_context.device();
        unsafe {
            // frame buffers
            self.framebuffers
                .iter()
                .for_each(|fb| device.destroy_framebuffer(*fb, None));
            self.framebuffers.clear();

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

    pub fn acquire_next(&mut self, image_available_semaphore: &Semaphore) -> VkResult<(u32, bool)> {
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

    /// Blits the source image to the destination image.
    /// The layout of src_img is transferred to GENERAL.
    pub fn record_blit(&self, src_img: &Image, cmdbuf: &CommandBuffer, image_idx: u32) {
        // the swapchain image is not wrapped because it is handled by the swapchain
        let dst_raw_img = self.get_image(image_idx);
        let device = self.vulkan_context.device();

        src_img.record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);

        // transition dst using the raw function
        // from UNDEFINED, because the image is just being available
        record_image_transition_barrier(
            device.as_raw(),
            cmdbuf.as_raw(),
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            dst_raw_img,
            src_img.get_desc().get_aspect_mask(),
            0,
            1,
        );

        unsafe {
            device.cmd_blit_image(
                cmdbuf.as_raw(),
                src_img.as_raw(),
                vk::ImageLayout::GENERAL,
                dst_raw_img,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[src_img.get_blit_region()],
                vk::Filter::LINEAR,
            );
        }

        // transition dst using the raw function
        record_image_transition_barrier(
            device.as_raw(),
            cmdbuf.as_raw(),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            dst_raw_img,
            src_img.get_desc().get_aspect_mask(),
            0,
            1,
        );
    }

    /// Present the image to the swapchain with the given index.
    pub fn present(
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

    pub fn get_render_pass(&self) -> &RenderPass {
        &self.render_pass
    }

    pub fn get_image_views(&self) -> &[vk::ImageView] {
        &self.image_views
    }

    pub fn record_begin_render_pass_cmdbuf(
        &self,
        cmdbuf: &CommandBuffer,
        image_index: u32,
        render_area: &vk::Extent2D,
    ) {
        let render_pass_begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass.as_raw())
            .framebuffer(self.framebuffers[image_index as usize])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: *render_area,
            })
            .clear_values(&[vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
            }]);

        unsafe {
            self.vulkan_context.device().cmd_begin_render_pass(
                cmdbuf.as_raw(),
                &render_pass_begin_info,
                vk::SubpassContents::INLINE,
            )
        };
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

    println!("{}", table);
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
    desired_present_mode: PresentModeKHR,
) -> PresentModeKHR {
    //guaranteed to be available
    const FALLBACK_PRESENT_MODE: PresentModeKHR = PresentModeKHR::FIFO;

    let present_mode = {
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
        if present_modes.contains(&desired_present_mode) {
            desired_present_mode
        } else {
            FALLBACK_PRESENT_MODE
        }
    };

    log::info!("Swapchain present mode: {:?}", present_mode);
    present_mode
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
            .image_extent(extent)
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

    let swapchain_device = swapchain::Device::new(&context.instance().as_raw(), &context.device());
    let swapchain_khr = unsafe {
        swapchain_device
            .create_swapchain(&create_info, None)
            .expect("Failed to create swapchain")
    };

    (swapchain_device, swapchain_khr)
}

fn create_vulkan_swapchain(
    vulkan_context: &VulkanContext,
    window_size: &[u32; 2],
    swapchain_preference: &SwapchainDesc,
) -> (
    swapchain::Device,
    vk::SwapchainKHR,
    Vec<vk::ImageView>,
    RenderPass,
    Vec<vk::Framebuffer>,
) {
    let format = choose_surface_format(
        vulkan_context,
        swapchain_preference.format,
        swapchain_preference.color_space,
    );
    let present_mode = choose_present_mode(vulkan_context, swapchain_preference.present_mode);

    let extent = Extent2D {
        width: window_size[0],
        height: window_size[1],
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

    let image_count = capabilities.min_image_count;

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
        vulkan_context.device(),
        render_pass.as_raw(),
        &image_views,
        window_size,
    );

    (
        swapchain_device,
        swapchain_khr,
        image_views,
        render_pass,
        framebuffers,
    )
}

fn create_vulkan_render_pass(device: Device, format: vk::Format) -> RenderPass {
    let color_attachment = AttachmentDesc {
        format,
        samples: vk::SampleCountFlags::TYPE_1,
        load_op: vk::AttachmentLoadOp::LOAD,
        store_op: vk::AttachmentStoreOp::STORE,
        stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
        stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
        initial_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
    };

    let subpass = SubpassDesc {
        color_attachments: vec![AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
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
    device: &Device,
    render_pass: vk::RenderPass,
    image_views: &[vk::ImageView],
    window_size: &[u32; 2],
) -> Vec<vk::Framebuffer> {
    image_views
        .iter()
        .map(|view| [*view])
        .map(|attachments| {
            let framebuffer_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(window_size[0])
                .height(window_size[1])
                .layers(1);
            unsafe { device.create_framebuffer(&framebuffer_info, None) }
        })
        .collect::<VkResult<Vec<vk::Framebuffer>>>()
        .expect("Failed to create framebuffers")
}
