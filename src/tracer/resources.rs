use crate::{
    util::get_project_root,
    vkn::{
        Allocator, Buffer, BufferUsage, Device, ImageDesc, ShaderModule, Texture, VulkanContext,
    },
};
use ash::vk;

pub struct TracerResources {
    pub gui_input: Buffer,
    pub camera_info: Buffer,
    pub env_info: Buffer,
    pub shader_write_tex: Texture,

    pub weighted_cosine_bn: Texture,
}

impl TracerResources {
    pub fn new(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        tracer_sm: &ShaderModule,
        screen_extent: &[u32; 2],
    ) -> Self {
        let device = vulkan_ctx.device();

        let gui_input_layout = tracer_sm.get_buffer_layout("U_GuiInput").unwrap();
        let gui_input = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            gui_input_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let camera_info_layout = tracer_sm.get_buffer_layout("U_CameraInfo").unwrap();
        let camera_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            camera_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let env_info_layout = tracer_sm.get_buffer_layout("U_EnvInfo").unwrap();
        let env_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            env_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let shader_write_tex = Self::create_shader_write_tex(
            device.clone(),
            allocator.clone(),
            [screen_extent[0], screen_extent[1], 1],
        );

        let weighted_cosine_bn = Self::create_bn(&vulkan_ctx, allocator.clone());

        Self {
            gui_input,
            camera_info,
            env_info,
            shader_write_tex,
            weighted_cosine_bn,
        }
    }

    pub fn on_resize(&mut self, device: Device, allocator: Allocator, screen_extent: &[u32; 2]) {
        self.shader_write_tex = Self::create_shader_write_tex(
            device,
            allocator,
            [screen_extent[0], screen_extent[1], 1],
        );
    }

    fn create_shader_write_tex(
        device: Device,
        allocator: Allocator,
        screen_extent: [u32; 3],
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: screen_extent,
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }

    fn create_bn(vulkan_ctx: &VulkanContext, allocator: Allocator) -> Texture {
        let tex_desc = ImageDesc {
            extent: [128, 128, 1],
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(vulkan_ctx.device().clone(), allocator, &tex_desc, &sam_desc);

        let path = get_project_root()
            + "/texture/stbn/unitvec3_cosine_2d_1d/stbn_unitvec3_cosine_2Dx1D_128x128x64_0.png";
        tex.get_image()
            .load_and_fill(
                &vulkan_ctx.get_general_queue(),
                vulkan_ctx.command_pool(),
                &path,
                Some(vk::ImageLayout::GENERAL),
            )
            .unwrap();
        tex
    }
}
