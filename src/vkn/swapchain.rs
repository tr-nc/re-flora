use ash::{khr::swapchain, vk, Device};

use super::context::VulkanContext;

pub struct Swapchain {
    pub render_pass: vk::RenderPass,
    pub loader: swapchain::Device,
    pub khr: vk::SwapchainKHR,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub extent: vk::Extent2D,
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
}

impl Swapchain {
    pub fn new(context: &VulkanContext, window_size: &[u32; 2]) -> Self {
        let (loader, khr, extent, format, images, image_views) =
            create_vulkan_swapchain(&context, window_size);

        let render_pass = create_vulkan_render_pass(&context.device, format);

        let framebuffers =
            create_vulkan_framebuffers(&context.device, render_pass, extent, &image_views);

        Self {
            loader,
            extent,
            khr,
            images,
            image_views,
            render_pass,
            framebuffers,
        }
    }

    pub fn recreate(&mut self, context: &VulkanContext, window_size: &[u32; 2]) {
        log::info!("Recreating vulkan swapchain");

        unsafe { context.device.device_wait_idle().unwrap() };

        self.destroy(context);

        let (loader, khr, extent, format, images, image_views) =
            create_vulkan_swapchain(context, &window_size);

        let render_pass = create_vulkan_render_pass(&context.device, format);

        let framebuffers =
            create_vulkan_framebuffers(&context.device, render_pass, extent, &image_views);

        self.loader = loader;
        self.extent = extent;
        self.khr = khr;
        self.images = images;
        self.image_views = image_views;
        self.render_pass = render_pass;
        self.framebuffers = framebuffers;
    }

    pub fn destroy(&mut self, context: &VulkanContext) {
        unsafe {
            // TODO: check if commented out, validation error will occur or not (it should be)
            self.framebuffers
                .iter()
                .for_each(|fb| context.device.destroy_framebuffer(*fb, None));
            self.framebuffers.clear();
            context.device.destroy_render_pass(self.render_pass, None);
            self.image_views
                .iter()
                .for_each(|v| context.device.destroy_image_view(*v, None));
            self.image_views.clear();
            self.loader.destroy_swapchain(self.khr, None);
        }
    }
}

fn create_vulkan_swapchain(
    context: &VulkanContext,
    window_size: &[u32; 2],
) -> (
    swapchain::Device,
    vk::SwapchainKHR,
    vk::Extent2D,
    vk::Format,
    Vec<vk::Image>,
    Vec<vk::ImageView>,
) {
    log::debug!("Creating vulkan swapchain");
    // Swapchain format
    let format = {
        let formats = unsafe {
            context
                .surface
                .get_physical_device_surface_formats(context.physical_device, context.surface_khr)
                .expect("Failed to get physical device surface formats")
        };

        *formats
            .iter()
            .find(|format| {
                format.format == vk::Format::R8G8B8A8_SRGB
                    && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .unwrap_or(&formats[0])
    };
    log::debug!("Swapchain format: {format:?}");

    // Swapchain present mode
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
        if present_modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
            vk::PresentModeKHR::IMMEDIATE
        } else {
            vk::PresentModeKHR::FIFO
        }
    };
    log::debug!("Swapchain present mode: {present_mode:?}");

    let capabilities = unsafe {
        context
            .surface
            .get_physical_device_surface_capabilities(context.physical_device, context.surface_khr)
            .expect("Failed to get physical device surface capabilities")
    };

    // Swapchain extent
    let extent = {
        if capabilities.current_extent.width != std::u32::MAX {
            capabilities.current_extent
        } else {
            let min = capabilities.min_image_extent;
            let max = capabilities.max_image_extent;
            let width = window_size[0].min(max.width).max(min.width);
            let height = window_size[1].min(max.height).max(min.height);
            vk::Extent2D { width, height }
        }
    };
    log::debug!("Swapchain extent: {extent:?}");

    // Swapchain image count
    let image_count = capabilities.min_image_count;
    log::debug!("Swapchain image count: {image_count:?}");

    // Swapchain
    // let families_indices = [context.graphics_q_index, context.present_q_index];
    let create_info = {
        let mut builder = vk::SwapchainCreateInfoKHR::default()
            .surface(context.surface_khr)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT);

        // builder = if context.graphics_q_index != context.present_q_index {
        //     builder
        //         .image_sharing_mode(vk::SharingMode::CONCURRENT)
        //         .queue_family_indices(&families_indices)
        // } else {
        //     builder.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        // };

        builder = builder.image_sharing_mode(vk::SharingMode::EXCLUSIVE);

        builder
            .pre_transform(capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
    };

    let swapchain = swapchain::Device::new(&context.instance, &context.device);
    let swapchain_khr = unsafe {
        swapchain
            .create_swapchain(&create_info, None)
            .expect("Failed to create swapchain")
    };

    // Swapchain images and image views
    let images = unsafe {
        swapchain
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
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    (
        swapchain,
        swapchain_khr,
        extent,
        format.format,
        images,
        views,
    )
}

fn create_vulkan_render_pass(device: &Device, format: vk::Format) -> vk::RenderPass {
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
    device: &Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    image_views: &[vk::ImageView],
) -> Vec<vk::Framebuffer> {
    log::debug!("Creating vulkan framebuffers");
    image_views
        .iter()
        .map(|view| [*view])
        .map(|attachments| {
            let framebuffer_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            unsafe { device.create_framebuffer(&framebuffer_info, None) }
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to create framebuffers")
}
