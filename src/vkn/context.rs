#[cfg(any(target_os = "macos", target_os = "ios"))]
use ash::vk::{KhrGetPhysicalDeviceProperties2Fn, KhrPortabilityEnumerationFn};

use ash::{ext::debug_utils, khr::surface, vk, Device, Entry, Instance};
use winit::window::Window;

use super::context_builder;
use std::error::Error;

pub struct ContextCreateInfo {
    pub name: String,
}

pub struct QueueFamilyIndices {
    /// Guaranteed to support GRAPHICS + PRESENT + COMPUTE + TRANSFER,
    /// and should be used for all main tasks
    pub general: u32,
    /// Exclusive to transfer operations, may be slower, but enables parallelism for
    /// background transfer operations
    pub transfer_only: u32,
}

impl QueueFamilyIndices {
    pub fn get_all_indices(&self) -> Vec<u32> {
        vec![self.general, self.transfer_only]
    }
}

pub struct VulkanContext {
    pub instance: Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: Device,
    pub command_pool: vk::CommandPool,
    pub surface: surface::Instance,
    pub surface_khr: vk::SurfaceKHR,
    debug_utils: debug_utils::Instance,
    debug_utils_messenger: vk::DebugUtilsMessengerEXT,
    queue_family_indices: QueueFamilyIndices,
}

impl VulkanContext {
    pub fn new(window: &Window, create_info: ContextCreateInfo) -> Self {
        let entry = Entry::linked();

        let (instance, debug_utils, debug_utils_messenger) =
            context_builder::instance::create_vulkan_instance(&entry, window, &create_info.name);

        let (surface_khr, surface) =
            context_builder::surface::create_surface(&entry, &instance, window);

        let (physical_device, queue_family_indices) =
            context_builder::physical_device::create_physical_device(
                &instance,
                &surface,
                surface_khr,
            );

        let device = context_builder::device::create_device(
            &instance,
            physical_device,
            &queue_family_indices,
        );

        let command_pool = context_builder::command_pool::create_command_pool(
            &device,
            queue_family_indices.general,
        );

        Self {
            instance,
            debug_utils,
            debug_utils_messenger,
            surface,
            surface_khr,
            physical_device,
            queue_family_indices,
            device,
            command_pool,
        }
    }

    /// Obtains the general queue from the device
    pub fn get_general_queue(&self) -> vk::Queue {
        unsafe {
            self.device
                .get_device_queue(self.queue_family_indices.general, 0)
        }
    }

    /// Obtains the transfer-only queue from the device
    pub fn get_transfer_only_queue(&self) -> vk::Queue {
        unsafe {
            self.device
                .get_device_queue(self.queue_family_indices.transfer_only, 0)
        }
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        log::info!("Destroying Vulkan Context");
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.surface.destroy_surface(self.surface_khr, None);
            self.debug_utils
                .destroy_debug_utils_messenger(self.debug_utils_messenger, None);
            self.instance.destroy_instance(None);
        }
    }
}

fn create_vulkan_render_pass(
    device: &Device,
    format: vk::Format,
) -> Result<vk::RenderPass, Box<dyn Error>> {
    log::info!("Creating vulkan render pass");
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

    Ok(unsafe { device.create_render_pass(&render_pass_info, None)? })
}

fn create_vulkan_framebuffers(
    device: &Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    image_views: &[vk::ImageView],
) -> Result<Vec<vk::Framebuffer>, Box<dyn Error>> {
    log::info!("Creating vulkan framebuffers");
    Ok(image_views
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
        .collect::<Result<Vec<_>, _>>()?)
}

// fn record_command_buffers(
//     device: &Device,
//     command_pool: vk::CommandPool,
//     command_buffer: vk::CommandBuffer,
//     framebuffer: vk::Framebuffer,
//     render_pass: vk::RenderPass,
//     extent: vk::Extent2D,
//     pixels_per_point: f32,
//     renderer: &mut Renderer,

//     clipped_primitives: &[ClippedPrimitive],
// ) -> Result<(), Box<dyn Error>> {
//     unsafe { device.reset_command_pool(command_pool, vk::CommandPoolResetFlags::empty())? };

//     let command_buffer_begin_info =
//         vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::SIMULTANEOUS_USE);
//     unsafe { device.begin_command_buffer(command_buffer, &command_buffer_begin_info)? };

//     let render_pass_begin_info = vk::RenderPassBeginInfo::default()
//         .render_pass(render_pass)
//         .framebuffer(framebuffer)
//         .render_area(vk::Rect2D {
//             offset: vk::Offset2D { x: 0, y: 0 },
//             extent,
//         })
//         .clear_values(&[vk::ClearValue {
//             color: vk::ClearColorValue {
//                 float32: [0.007, 0.007, 0.007, 1.0],
//             },
//         }]);

//     unsafe {
//         device.cmd_begin_render_pass(
//             command_buffer,
//             &render_pass_begin_info,
//             vk::SubpassContents::INLINE,
//         )
//     };

//     renderer.cmd_draw(command_buffer, extent, pixels_per_point, clipped_primitives)?;

//     unsafe { device.cmd_end_render_pass(command_buffer) };

//     unsafe { device.end_command_buffer(command_buffer)? };

//     Ok(())
// }
