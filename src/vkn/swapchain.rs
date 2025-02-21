use std::sync::Arc;

use ash::{
    khr::swapchain,
    prelude::VkResult,
    vk::{self, Extent2D, PresentModeKHR, SurfaceCapabilitiesKHR, SurfaceFormatKHR},
};

use super::context::VulkanContext;

/// The preference for the swapchain.
///
/// Preferences are considered every time the swapchain is (re)created.
pub struct SwapchainPreference {
    format: vk::Format,
    color_space: vk::ColorSpaceKHR,
    present_mode: vk::PresentModeKHR,
}

impl Default for SwapchainPreference {
    fn default() -> Self {
        Self {
            format: vk::Format::R8G8B8A8_SRGB,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
            present_mode: vk::PresentModeKHR::MAILBOX,
        }
    }
}

pub struct Swapchain {
    vulkan_context: Arc<VulkanContext>,

    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    image_views: Vec<vk::ImageView>,

    swapchain_khr: vk::SwapchainKHR,
    swapchain_device: swapchain::Device,

    swapchain_preference: SwapchainPreference,
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        log::info!("Dropping vulkan swapchain");
        self.clean_up();
    }
}

impl Swapchain {
    pub fn new(
        context: &Arc<VulkanContext>,
        window_size: &[u32; 2],
        swapchain_preference: SwapchainPreference,
    ) -> Self {
        let (swapchain_device, swapchain_khr, image_views, render_pass, framebuffers) =
            create_vulkan_swapchain(&context, window_size, &swapchain_preference);

        Self {
            vulkan_context: context.clone(),
            render_pass,
            framebuffers,
            image_views,
            swapchain_khr,
            swapchain_device,
            swapchain_preference,
        }
    }

    pub fn recreate(&mut self, context: &VulkanContext, window_size: &[u32; 2]) {
        log::info!("Recreating vulkan swapchain");
        self.clean_up();
        let (swapchain_device, swapchain_khr, image_views, render_pass, framebuffers) =
            create_vulkan_swapchain(&context, window_size, &self.swapchain_preference);

        self.swapchain_device = swapchain_device;
        self.swapchain_khr = swapchain_khr;
        self.render_pass = render_pass;
        self.framebuffers = framebuffers;
        self.image_views = image_views;
    }

    pub fn clean_up(&mut self) {
        log::info!("Cleaning up vulkan swapchain");

        let device = &self.vulkan_context.device;

        unsafe {
            device.device_wait_idle().unwrap();

            // renderpass
            device.destroy_render_pass(self.render_pass, None);

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

            // images are not destroyed explicitly, because they are owned by the swapchain,
            // and should be managed by swapchain functions

            self.swapchain_device
                .destroy_swapchain(self.swapchain_khr, None);
        }
    }

    pub fn get_swapchain_device(&self) -> &swapchain::Device {
        &self.swapchain_device
    }

    pub fn acquire_next_image(
        &mut self,
        image_available_semaphore: &vk::Semaphore,
    ) -> VkResult<(u32, bool)> {
        let timeout = u64::MAX;
        let fence = vk::Fence::null();
        unsafe {
            self.swapchain_device.acquire_next_image(
                self.swapchain_khr,
                timeout,
                *image_available_semaphore,
                fence,
            )
        }
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
            self.swapchain_device
                .queue_present(self.vulkan_context.get_general_queue(), &present_info)
        }
    }

    pub fn get_render_pass(&self) -> vk::RenderPass {
        self.render_pass
    }

    pub fn record_begin_render_pass_cmdbuf(
        &self,
        command_buffer: vk::CommandBuffer,
        image_index: u32,
        render_area: &vk::Extent2D,
    ) {
        let render_pass_begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index as usize])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: *render_area,
            })
            .clear_values(&[vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.007, 0.007, 0.007, 1.0],
                },
            }]);

        unsafe {
            self.vulkan_context.device.cmd_begin_render_pass(
                command_buffer,
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
                .surface
                .get_physical_device_surface_formats(context.physical_device, context.surface_khr)
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
                .surface
                .get_physical_device_surface_present_modes(
                    context.physical_device,
                    context.surface_khr,
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
            .surface(context.surface_khr)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT);

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

    let swapchain_device = swapchain::Device::new(&context.instance, &context.device);
    let swapchain_khr = unsafe {
        swapchain_device
            .create_swapchain(&create_info, None)
            .expect("Failed to create swapchain")
    };

    (swapchain_device, swapchain_khr)
}

fn create_vulkan_swapchain(
    context: &VulkanContext,
    window_size: &[u32; 2],
    swapchain_preference: &SwapchainPreference,
) -> (
    swapchain::Device,
    vk::SwapchainKHR,
    Vec<vk::ImageView>,
    vk::RenderPass,
    Vec<vk::Framebuffer>,
) {
    let format = choose_surface_format(
        context,
        swapchain_preference.format,
        swapchain_preference.color_space,
    );
    let present_mode = choose_present_mode(context, swapchain_preference.present_mode);

    let extent = Extent2D {
        width: window_size[0],
        height: window_size[1],
    };

    let capabilities: SurfaceCapabilitiesKHR = unsafe {
        context
            .surface
            .get_physical_device_surface_capabilities(context.physical_device, context.surface_khr)
            .expect("Failed to get physical device surface capabilities")
    };

    let image_count = capabilities.min_image_count;
    log::debug!("Swapchain image count: {image_count:?}");

    let (swapchain_device, swapchain_khr) = create_swapchain_device_khr(
        context,
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

    let views = images
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

            unsafe { context.device.create_image_view(&create_info, None) }
        })
        .collect::<VkResult<Vec<vk::ImageView>>>()
        .unwrap();

    let render_pass = create_vulkan_render_pass(&context.device, format.format);

    let framebuffers =
        create_vulkan_framebuffers(&context.device, render_pass, &views, window_size);

    (
        swapchain_device,
        swapchain_khr,
        views,
        render_pass,
        framebuffers,
    )
}

fn create_vulkan_render_pass(device: &ash::Device, format: vk::Format) -> vk::RenderPass {
    log::debug!("Creating vulkan render pass");
    let attachment_descs = [vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];

    let color_attachment_refs = [vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];

    let subpass_descs = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_attachment_refs)];

    let subpass_deps = [vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        )];

    let render_pass_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachment_descs)
        .subpasses(&subpass_descs)
        .dependencies(&subpass_deps);

    unsafe {
        device
            .create_render_pass(&render_pass_info, None)
            .expect("Failed to create render pass")
    }
}

fn create_vulkan_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    image_views: &[vk::ImageView],
    window_size: &[u32; 2],
) -> Vec<vk::Framebuffer> {
    log::debug!("Creating vulkan framebuffers");
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
