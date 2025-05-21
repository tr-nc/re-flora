use crate::{
    util::{get_project_root, Timer},
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

    pub scalar_bn: Texture,
    pub unit_vec2_bn: Texture,
    pub unit_vec3_bn: Texture,
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

        let timer = Timer::new();
        let scalar_bn = create_bn(
            &vulkan_ctx,
            allocator.clone(),
            vk::Format::R8_UNORM,
            "scalar_2d_1d_1d/stbn_scalar_2Dx1Dx1D_128x128x64x1_",
        );
        let unit_vec2_bn = create_bn(
            &vulkan_ctx,
            allocator.clone(),
            vk::Format::R8G8_UNORM,
            "unitvec2_2d_1d/stbn_unitvec2_2Dx1D_128x128x64_",
        );
        let unit_vec3_bn = create_bn(
            &vulkan_ctx,
            allocator.clone(),
            vk::Format::R8G8B8A8_UNORM,
            "unitvec3_2d_1d/stbn_unitvec3_2Dx1D_128x128x64_",
        );
        let weighted_cosine_bn = create_bn(
            &vulkan_ctx,
            allocator.clone(),
            vk::Format::R8G8B8A8_UNORM,
            "unitvec3_cosine_2d_1d/stbn_unitvec3_cosine_2Dx1D_128x128x64_",
        );
        log::debug!("Blue noise texture load time: {:?}", timer.elapsed());

        return Self {
            gui_input,
            camera_info,
            env_info,
            shader_write_tex,

            scalar_bn,
            unit_vec2_bn,
            unit_vec3_bn,
            weighted_cosine_bn,
        };

        fn create_bn(
            vulkan_ctx: &VulkanContext,
            allocator: Allocator,
            format: vk::Format,
            relative_path: &str,
        ) -> Texture {
            const BLUE_NOISE_LEN: u32 = 64;

            let img_desc = ImageDesc {
                extent: [128, 128, 1],
                array_len: BLUE_NOISE_LEN,
                format,
                usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
                initial_layout: vk::ImageLayout::UNDEFINED,
                aspect: vk::ImageAspectFlags::COLOR,
                ..Default::default()
            };
            let sam_desc = Default::default();
            let tex = Texture::new(vulkan_ctx.device().clone(), allocator, &img_desc, &sam_desc);

            let base_path = get_project_root() + "/texture/stbn/";
            for i in 0..BLUE_NOISE_LEN {
                let path = format!("{}{}{}.png", base_path, relative_path, i);
                tex.get_image()
                    .load_and_fill(
                        &vulkan_ctx.get_general_queue(),
                        vulkan_ctx.command_pool(),
                        &path,
                        i,
                        Some(vk::ImageLayout::GENERAL),
                    )
                    .unwrap();
            }
            tex
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
}
