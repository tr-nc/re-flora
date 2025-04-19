use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule, Texture, TextureDesc};
use ash::vk;

pub struct TracerResources {
    pub gui_input: Buffer,
    pub camera_info: Buffer,
    pub scene_info: Buffer,

    pub shader_write_tex: Texture,
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

        let gui_input_layout = tracer_sm.get_buffer_layout("U_GuiInput").unwrap();
        let gui_input = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            gui_input_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let camera_info_layout = tracer_sm.get_buffer_layout("U_CameraInfo").unwrap();
        let camera_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            camera_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let scene_info_layout = tracer_sm.get_buffer_layout("U_SceneInfo").unwrap();
        let scene_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            scene_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        Self {
            gui_input,
            camera_info,
            scene_info,
            shader_write_tex,
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
