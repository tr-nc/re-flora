use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule, Texture, TextureDesc};
use ash::vk;

pub struct TracerResources {
    pub shader_write: Texture,
    pub gui_input: Buffer,
    pub camera_info: Buffer,
}

impl TracerResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        tracer_sm: &ShaderModule,
        screen_extent: &[u32; 2],
    ) -> Self {
        let shader_write_tex = Self::create_shader_write_tex(
            device.clone(),
            allocator.clone(),
            [screen_extent[0], screen_extent[1], 1],
        );

        let gui_input_buf_layout = tracer_sm.get_buffer_layout("GuiInput").unwrap();
        let gui_input_buf = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            gui_input_buf_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let camera_info_layout = tracer_sm.get_buffer_layout("CameraInfo").unwrap();
        let camera_info_buf = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            camera_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        Self {
            shader_write: shader_write_tex,
            gui_input: gui_input_buf,
            camera_info: camera_info_buf,
        }
    }

    pub fn on_resize(&mut self, device: Device, allocator: Allocator, screen_extent: &[u32; 2]) {
        self.shader_write = Self::create_shader_write_tex(
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
