struct Swapchain {
    loader: swapchain::Device,
    extent: vk::Extent2D,
    khr: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
}

impl Swapchain {
    fn new(vulkan_context: &VulkanContext) -> Result<Self, Box<dyn Error>> {
        // Swapchain
        let (loader, khr, extent, format, images, image_views) =
            create_vulkan_swapchain(&vulkan_context)?;

        // Renderpass
        let render_pass = create_vulkan_render_pass(&vulkan_context.device, format)?;

        // Framebuffers
        let framebuffers =
            create_vulkan_framebuffers(&vulkan_context.device, render_pass, extent, &image_views)?;

        Ok(Self {
            loader,
            extent,
            khr,
            images,
            image_views,
            render_pass,
            framebuffers,
        })
    }

    fn recreate(&mut self, vulkan_context: &VulkanContext) -> Result<(), Box<dyn Error>> {
        println!("Recreating the swapchain");

        unsafe { vulkan_context.device.device_wait_idle()? };

        self.destroy(vulkan_context);

        // Swapchain
        let (loader, khr, extent, format, images, image_views) =
            create_vulkan_swapchain(vulkan_context)?;

        // Renderpass
        let render_pass = create_vulkan_render_pass(&vulkan_context.device, format)?;

        // Framebuffers
        let framebuffers =
            create_vulkan_framebuffers(&vulkan_context.device, render_pass, extent, &image_views)?;

        self.loader = loader;
        self.extent = extent;
        self.khr = khr;
        self.images = images;
        self.image_views = image_views;
        self.render_pass = render_pass;
        self.framebuffers = framebuffers;

        Ok(())
    }

    fn destroy(&mut self, vulkan_context: &VulkanContext) {
        unsafe {
            self.framebuffers
                .iter()
                .for_each(|fb| vulkan_context.device.destroy_framebuffer(*fb, None));
            self.framebuffers.clear();
            vulkan_context
                .device
                .destroy_render_pass(self.render_pass, None);
            self.image_views
                .iter()
                .for_each(|v| vulkan_context.device.destroy_image_view(*v, None));
            self.image_views.clear();
            self.loader.destroy_swapchain(self.khr, None);
        }
    }

    fn create_vulkan_swapchain(
        vulkan_context: &Context,
    ) -> Result<
        (
            swapchain::Device,
            vk::SwapchainKHR,
            vk::Extent2D,
            vk::Format,
            Vec<vk::Image>,
            Vec<vk::ImageView>,
        ),
        Box<dyn Error>,
    > {
        println!("Creating vulkan swapchain");
        // Swapchain format
        let format = {
            let formats = unsafe {
                vulkan_context.surface.get_physical_device_surface_formats(
                    vulkan_context.physical_device,
                    vulkan_context.surface_khr,
                )?
            };

            *formats
                .iter()
                .find(|format| {
                    format.format == vk::Format::R8G8B8A8_SRGB
                        && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                })
                .unwrap_or(&formats[0])
        };
        println!("Swapchain format: {format:?}");

        // Swapchain present mode
        let present_mode = {
            let present_modes = unsafe {
                vulkan_context
                    .surface
                    .get_physical_device_surface_present_modes(
                        vulkan_context.physical_device,
                        vulkan_context.surface_khr,
                    )?
            };
            if present_modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
                vk::PresentModeKHR::IMMEDIATE
            } else {
                vk::PresentModeKHR::FIFO
            }
        };
        println!("Swapchain present mode: {present_mode:?}");

        let capabilities = unsafe {
            vulkan_context
                .surface
                .get_physical_device_surface_capabilities(
                    vulkan_context.physical_device,
                    vulkan_context.surface_khr,
                )?
        };

        // swapchain extent
        let extent = {
            if capabilities.current_extent.width != std::u32::MAX {
                capabilities.current_extent
            } else {
                let min = capabilities.min_image_extent;
                let max = capabilities.max_image_extent;
                let width = WIDTH.min(max.width).max(min.width);
                let height = HEIGHT.min(max.height).max(min.height);
                vk::Extent2D { width, height }
            }
        };
        println!("Swapchain extent: {extent:?}");

        // Swapchain image count
        let image_count = capabilities.min_image_count;
        println!("Swapchain image count: {image_count:?}");

        // Swapchain
        let families_indices = [
            vulkan_context.graphics_q_index,
            vulkan_context.present_q_index,
        ];
        let create_info = {
            let mut builder = vk::SwapchainCreateInfoKHR::default()
                .surface(vulkan_context.surface_khr)
                .min_image_count(image_count)
                .image_format(format.format)
                .image_color_space(format.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT);

            builder = if vulkan_context.graphics_q_index != vulkan_context.present_q_index {
                builder
                    .image_sharing_mode(vk::SharingMode::CONCURRENT)
                    .queue_family_indices(&families_indices)
            } else {
                builder.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            };

            builder
                .pre_transform(capabilities.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(present_mode)
                .clipped(true)
        };

        let swapchain = swapchain::Device::new(&vulkan_context.instance, &vulkan_context.device);
        let swapchain_khr = unsafe { swapchain.create_swapchain(&create_info, None)? };

        // Swapchain images and image views
        let images = unsafe { swapchain.get_swapchain_images(swapchain_khr)? };
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

                unsafe { vulkan_context.device.create_image_view(&create_info, None) }
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok((
            swapchain,
            swapchain_khr,
            extent,
            format.format,
            images,
            views,
        ))
    }
}
