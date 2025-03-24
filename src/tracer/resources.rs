use crate::vkn::{Allocator, Buffer, Device, ShaderModule, Texture, TextureDesc};
use ash::vk;

pub struct TracerResources {
    pub shader_write_tex: Texture,
    pub gui_input_buf: Buffer,
    pub camera_info_buf: Buffer,
}

impl TracerResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        tracer_sm: &ShaderModule,
        screen_extent: &[u32; 2],
    ) -> Self {
        let shader_write_tex = Self::create_shader_write_texture(
            [screen_extent[0], screen_extent[1], 1],
            device.clone(),
            allocator.clone(),
        );

        let gui_input_buf_layout = tracer_sm.get_buffer_layout("GuiInput").unwrap();
        let gui_input_buf = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            gpu_allocator::MemoryLocation::CpuToGpu,
            gui_input_buf_layout.get_size() as _,
        );

        let camera_info_layout = tracer_sm.get_buffer_layout("CameraInfo").unwrap();
        let camera_info_buf = Buffer::new_sized(
            device.clone(),
            allocator,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            gpu_allocator::MemoryLocation::CpuToGpu,
            camera_info_layout.get_size() as _,
        );

        Self {
            shader_write_tex,
            gui_input_buf,
            camera_info_buf,
        }
    }

    pub fn on_resize(&mut self, device: Device, allocator: Allocator, screen_extent: &[u32; 2]) {
        self.shader_write_tex = Self::create_shader_write_texture(
            [screen_extent[0], screen_extent[1], 1],
            device,
            allocator,
        );
    }

    fn create_shader_write_texture(
        screen_extent: [u32; 3],
        device: Device,
        allocator: Allocator,
    ) -> Texture {
        let tex_desc = TextureDesc {
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
}
