use crate::{
    tracer::{grass_construct::generate_indexed_voxel_grass_blade, DenoiserResources, Vertex},
    util::{get_project_root, Timer},
    vkn::{
        Allocator, Buffer, BufferUsage, Device, Extent2D, Extent3D, ImageDesc, ShaderModule,
        Texture, VulkanContext,
    },
};
use ash::vk;

pub struct TracerResources {
    pub gui_input: Buffer,
    pub camera_info: Buffer,
    pub camera_info_prev_frame: Buffer,
    pub shadow_camera_info: Buffer,
    pub env_info: Buffer,
    pub grass_info: Buffer,
    pub post_processing_info: Buffer,

    pub vertices: Buffer,
    pub indices: Buffer,
    pub indices_len: u32,

    // gfx
    pub gfx_depth_tex: Texture,
    pub gfx_output_tex: Texture,

    // god ray
    pub god_ray_output_tex: Texture,

    // compute
    pub compute_depth_tex: Texture,
    pub compute_output_tex: Texture,

    pub denoiser_resources: DenoiserResources,

    // screen
    pub screen_output_tex: Texture,

    // shadow map
    pub shadow_map_tex: Texture,
    pub shadow_map_tex_for_vsm_ping: Texture,
    pub shadow_map_tex_for_vsm_pong: Texture,

    // noises
    pub scalar_bn: Texture,
    pub unit_vec2_bn: Texture,
    pub unit_vec3_bn: Texture,
    pub weighted_cosine_bn: Texture,
    pub fast_unit_vec3_bn: Texture,
    pub fast_weighted_cosine_bn: Texture,
}

impl TracerResources {
    pub fn new(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        vert_sm: &ShaderModule,
        tracer_sm: &ShaderModule,
        tracer_shadow_sm: &ShaderModule,
        post_processing_sm: &ShaderModule,
        rendering_extent: Extent2D,
        screen_extent: Extent2D,
        shadow_map_extent: Extent2D,
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

        let camera_info_prev_frame_layout = tracer_sm
            .get_buffer_layout("U_CameraInfoPrevFrame")
            .unwrap();
        let camera_info_prev_frame = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            camera_info_prev_frame_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let shadow_camera_info_layout = tracer_shadow_sm
            .get_buffer_layout("U_ShadowCameraInfo")
            .unwrap();
        let shadow_camera_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            shadow_camera_info_layout.clone(),
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

        let grass_info_layout = vert_sm.get_buffer_layout("U_GrassInfo").unwrap();

        let post_processing_info_layout = post_processing_sm
            .get_buffer_layout("U_PostProcessingInfo")
            .unwrap();
        let post_processing_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            post_processing_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let grass_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            grass_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let gfx_depth_tex =
            Self::create_gfx_depth_tex(device.clone(), allocator.clone(), rendering_extent.into());
        let compute_depth_tex = Self::create_compute_depth_tex(
            device.clone(),
            allocator.clone(),
            rendering_extent.into(),
        );

        let compute_output_tex =
            Self::create_compute_output_tex(device.clone(), allocator.clone(), rendering_extent);

        let gfx_output_tex =
            Self::create_gfx_output_tex(device.clone(), allocator.clone(), rendering_extent);

        let god_ray_output_tex =
            Self::create_god_ray_output_tex(device.clone(), allocator.clone(), rendering_extent);

        let screen_output_tex =
            Self::create_screen_output_tex(device.clone(), allocator.clone(), screen_extent);

        let shadow_map_tex = Self::create_shadow_map_tex(
            device.clone(),
            allocator.clone(),
            shadow_map_extent.into(),
        );
        let shadow_map_tex_for_vsm_ping = Self::create_shadow_map_tex_for_vsm_pingpong(
            device.clone(),
            allocator.clone(),
            shadow_map_extent.into(),
        );
        let shadow_map_tex_for_vsm_pong = Self::create_shadow_map_tex_for_vsm_pingpong(
            device.clone(),
            allocator.clone(),
            shadow_map_extent.into(),
        );

        let timer = Timer::new();
        let scalar_bn = create_bn(
            &vulkan_ctx,
            allocator.clone(),
            vk::Format::R8_UNORM,
            "stbn/scalar_2d_1d_1d/stbn_scalar_2Dx1Dx1D_128x128x64x1_",
        );
        let unit_vec2_bn = create_bn(
            &vulkan_ctx,
            allocator.clone(),
            vk::Format::R8G8_UNORM,
            "stbn/unitvec2_2d_1d/stbn_unitvec2_2Dx1D_128x128x64_",
        );
        let unit_vec3_bn = create_bn(
            &vulkan_ctx,
            allocator.clone(),
            vk::Format::R8G8B8A8_UNORM,
            "stbn/unitvec3_2d_1d/stbn_unitvec3_2Dx1D_128x128x64_",
        );
        let weighted_cosine_bn = create_bn(
            &vulkan_ctx,
            allocator.clone(),
            vk::Format::R8G8B8A8_UNORM,
            "stbn/unitvec3_cosine_2d_1d/stbn_unitvec3_cosine_2Dx1D_128x128x64_",
        );
        let fast_unit_vec3_bn = create_bn(
            &vulkan_ctx,
            allocator.clone(),
            vk::Format::R8G8B8A8_UNORM,
            "fast/unit_vec3/out_",
        );
        let fast_weighted_cosine_bn = create_bn(
            &vulkan_ctx,
            allocator.clone(),
            vk::Format::R8G8B8A8_UNORM,
            "fast/weighted_cosine/out_",
        );
        log::debug!("Blue noise texture load time: {:?}", timer.elapsed());

        // --- Generate and create indexed vertex and index buffers ---
        const GRASS_BLADE_VOXEL_LENGTH: u32 = 8;
        // Define bottom and tip colors, converting from [0, 255] RGB to [0.0, 1.0] Vec3.
        let bottom_color = glam::vec3(52.0 / 255.0, 116.0 / 255.0, 51.0 / 255.0);
        let tip_color = glam::vec3(182.0 / 255.0, 245.0 / 255.0, 0.0 / 255.0);

        // 1. Generate the indexed data with the color gradient.
        let (vertices_data, indices_data) =
            generate_indexed_voxel_grass_blade(GRASS_BLADE_VOXEL_LENGTH, bottom_color, tip_color);

        let indices_len = indices_data.len() as u32;

        log::debug!("vertices len: {}", vertices_data.len());
        log::debug!("indices len: {}", indices_data.len());

        // 2. Create and fill the vertex buffer.
        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            gpu_allocator::MemoryLocation::CpuToGpu,
            (std::mem::size_of::<Vertex>() * vertices_data.len()) as u64,
        );
        vertices.fill(&vertices_data).unwrap();

        // 3. Create and fill the index buffer.
        let indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            gpu_allocator::MemoryLocation::CpuToGpu,
            (std::mem::size_of::<u32>() * indices_data.len()) as u64,
        );
        indices.fill(&indices_data).unwrap();

        return Self {
            gui_input,
            camera_info,
            camera_info_prev_frame,
            shadow_camera_info,
            env_info,
            grass_info,
            post_processing_info,

            vertices,
            indices,
            indices_len,
            gfx_depth_tex,
            compute_depth_tex,
            compute_output_tex,
            gfx_output_tex,
            god_ray_output_tex,
            screen_output_tex,
            shadow_map_tex,
            shadow_map_tex_for_vsm_ping,
            shadow_map_tex_for_vsm_pong,
            scalar_bn,
            unit_vec2_bn,
            unit_vec3_bn,
            weighted_cosine_bn,
            fast_unit_vec3_bn,
            fast_weighted_cosine_bn,
            denoiser_resources: DenoiserResources::new(
                device.clone(),
                allocator.clone(),
                rendering_extent,
            ),
        };

        fn create_bn(
            vulkan_ctx: &VulkanContext,
            allocator: Allocator,
            format: vk::Format,
            relative_path: &str,
        ) -> Texture {
            const BLUE_NOISE_LEN: u32 = 64;

            let img_desc = ImageDesc {
                extent: Extent3D::new(128, 128, 1),
                array_len: BLUE_NOISE_LEN,
                format,
                usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
                initial_layout: vk::ImageLayout::UNDEFINED,
                aspect: vk::ImageAspectFlags::COLOR,
                ..Default::default()
            };
            let sam_desc = Default::default();
            let tex = Texture::new(vulkan_ctx.device().clone(), allocator, &img_desc, &sam_desc);

            let base_path = get_project_root() + "/texture/";
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

    pub fn on_resize(
        &mut self,
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
        screen_extent: Extent2D,
    ) {
        self.gfx_depth_tex =
            Self::create_gfx_depth_tex(device.clone(), allocator.clone(), rendering_extent);
        self.compute_depth_tex =
            Self::create_compute_depth_tex(device.clone(), allocator.clone(), rendering_extent);
        self.compute_output_tex =
            Self::create_compute_output_tex(device.clone(), allocator.clone(), rendering_extent);
        self.gfx_output_tex =
            Self::create_gfx_output_tex(device.clone(), allocator.clone(), rendering_extent);
        self.god_ray_output_tex =
            Self::create_god_ray_output_tex(device.clone(), allocator.clone(), rendering_extent);
        self.screen_output_tex =
            Self::create_screen_output_tex(device.clone(), allocator.clone(), screen_extent);

        self.denoiser_resources =
            DenoiserResources::new(device.clone(), allocator.clone(), rendering_extent);
    }

    fn create_god_ray_output_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R32_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }

    fn create_gfx_depth_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::D32_SFLOAT,
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::STORAGE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::DEPTH,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }

    fn create_compute_depth_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::D32_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::DEPTH,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }

    fn create_gfx_output_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::COLOR_ATTACHMENT,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }

    fn create_compute_output_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }

    /// The only screen extent texture in this resource set
    fn create_screen_output_tex(
        device: Device,
        allocator: Allocator,
        screen_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: screen_extent.into(),
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::COLOR_ATTACHMENT,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }

    fn create_shadow_map_tex(
        device: Device,
        allocator: Allocator,
        shadow_map_extent: Extent3D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: shadow_map_extent,
            format: vk::Format::D32_SFLOAT,
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                | vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::DEPTH,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }

    fn create_shadow_map_tex_for_vsm_pingpong(
        device: Device,
        allocator: Allocator,
        shadow_map_extent: Extent3D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: shadow_map_extent,
            format: vk::Format::R32G32B32A32_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }
}
