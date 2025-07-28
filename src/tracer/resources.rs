use crate::{
    resource::Resource,
    tracer::{
        flora_construct::{gen_grass, gen_lavender},
        leaves_construct::generate_indexed_voxel_leaves,
        DenoiserResources, ExtentDependentResources, Vertex,
    },
    util::get_project_root,
    vkn::{
        Allocator, Buffer, BufferUsage, Device, Extent2D, Extent3D, ImageDesc, ShaderModule,
        Texture, VulkanContext,
    },
};
use ash::vk;
use resource_container_derive::ResourceContainer;

#[derive(ResourceContainer)]
pub struct GrassBladeResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
}

impl GrassBladeResources {
    pub fn new(device: Device, allocator: Allocator, is_lod_used: bool) -> Self {
        let (vertices_data, indices_data) = gen_grass(is_lod_used).unwrap();
        let indices_len = indices_data.len() as u32;

        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            gpu_allocator::MemoryLocation::CpuToGpu,
            (std::mem::size_of::<Vertex>() * vertices_data.len()) as u64,
        );
        vertices.fill(&vertices_data).unwrap();

        let indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            gpu_allocator::MemoryLocation::CpuToGpu,
            (std::mem::size_of::<u32>() * indices_data.len()) as u64,
        );
        indices.fill(&indices_data).unwrap();

        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len,
        }
    }
}

#[derive(ResourceContainer)]
pub struct LavenderResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
}

impl LavenderResources {
    pub fn new(device: Device, allocator: Allocator, is_lod_used: bool) -> Self {
        let (vertices_data, indices_data) = gen_lavender(is_lod_used).unwrap();
        let indices_len = indices_data.len() as u32;

        let vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            gpu_allocator::MemoryLocation::CpuToGpu,
            (std::mem::size_of::<Vertex>() * vertices_data.len()) as u64,
        );
        vertices.fill(&vertices_data).unwrap();

        let indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            gpu_allocator::MemoryLocation::CpuToGpu,
            (std::mem::size_of::<u32>() * indices_data.len()) as u64,
        );
        indices.fill(&indices_data).unwrap();

        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len,
        }
    }
}

#[derive(ResourceContainer)]
pub struct LeavesResources {
    pub vertices: Resource<Buffer>,
    pub indices: Resource<Buffer>,
    pub indices_len: u32,
}

impl LeavesResources {
    pub fn new(device: Device, allocator: Allocator, is_lod_used: bool) -> Self {
        // use default parameters for initial leaf generation
        Self::new_with_params(device, allocator, 0.5, 0.25, 8.0, 16.0, is_lod_used)
    }

    pub fn new_with_params(
        device: Device,
        allocator: Allocator,
        inner_density: f32,
        outer_density: f32,
        inner_radius: f32,
        outer_radius: f32,
        is_lod_used: bool,
    ) -> Self {
        // 1. Generate the indexed data for hollow sphere-shaped leaves.
        let (mut vertices_data, mut indices_data) = generate_indexed_voxel_leaves(
            inner_density,
            outer_density,
            inner_radius,
            outer_radius,
            is_lod_used,
        )
        .unwrap();

        // guard against empty data - create minimal buffers to avoid Vulkan validation errors
        if vertices_data.is_empty() {
            vertices_data.push(Vertex { packed_data: 0 }); // Dummy vertex
        }
        if indices_data.is_empty() {
            indices_data.push(0); // Dummy index
        }

        let indices_len = if indices_data.len() == 1 && indices_data[0] == 0 {
            0 // Don't render anything if this was a dummy index
        } else {
            indices_data.len() as u32
        };

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

        Self {
            vertices: Resource::new(vertices),
            indices: Resource::new(indices),
            indices_len,
        }
    }
}

#[derive(ResourceContainer)]
pub struct TracerResources {
    pub gui_input: Resource<Buffer>,
    pub sun_info: Resource<Buffer>,
    pub shading_info: Resource<Buffer>,
    pub camera_info: Resource<Buffer>,
    pub camera_info_prev_frame: Resource<Buffer>,
    pub shadow_camera_info: Resource<Buffer>,
    pub env_info: Resource<Buffer>,
    pub starlight_info: Resource<Buffer>,
    // pub grass_info: Resource<Buffer>,
    // pub lavender_info: Resource<Buffer>,
    // pub leaves_info: Resource<Buffer>,
    pub voxel_colors: Resource<Buffer>,
    pub taa_info: Resource<Buffer>,
    pub god_ray_info: Resource<Buffer>,
    pub post_processing_info: Resource<Buffer>,
    pub player_collider_info: Resource<Buffer>,
    pub player_collision_result: Resource<Buffer>,
    pub terrain_query_count: Resource<Buffer>,
    pub terrain_query_info: Resource<Buffer>,
    pub terrain_query_result: Resource<Buffer>,

    pub grass_blade_resources: GrassBladeResources,
    pub lavender_resources: LavenderResources,
    pub leaves_resources: LeavesResources,

    pub grass_blade_resources_lod: GrassBladeResources,
    pub lavender_resources_lod: LavenderResources,
    pub leaves_resources_lod: LeavesResources,

    pub shadow_map_tex: Resource<Texture>,
    pub shadow_map_tex_for_vsm_ping: Resource<Texture>,
    pub shadow_map_tex_for_vsm_pong: Resource<Texture>,

    pub star_noise_tex: Resource<Texture>,

    pub scalar_bn: Resource<Texture>,
    pub unit_vec2_bn: Resource<Texture>,
    pub unit_vec3_bn: Resource<Texture>,
    pub weighted_cosine_bn: Resource<Texture>,
    pub fast_unit_vec3_bn: Resource<Texture>,
    pub fast_weighted_cosine_bn: Resource<Texture>,

    pub extent_dependent_resources: ExtentDependentResources,
    pub denoiser_resources: DenoiserResources,
}

impl TracerResources {
    pub fn new(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        tracer_sm: &ShaderModule,
        tracer_shadow_sm: &ShaderModule,
        composition_sm: &ShaderModule,
        temporal_sm: &ShaderModule,
        spatial_sm: &ShaderModule,
        taa_sm: &ShaderModule,
        god_ray_sm: &ShaderModule,
        post_processing_sm: &ShaderModule,
        player_collider_sm: &ShaderModule,
        terrain_query_sm: &ShaderModule,
        rendering_extent: Extent2D,
        screen_extent: Extent2D,
        shadow_map_extent: Extent2D,
        max_terrain_queries: u32,
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

        let sun_info_layout = tracer_sm.get_buffer_layout("U_SunInfo").unwrap();
        let sun_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            sun_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let shading_info_layout = tracer_sm.get_buffer_layout("U_ShadingInfo").unwrap();
        let shading_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            shading_info_layout.clone(),
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

        let starlight_info_layout = composition_sm.get_buffer_layout("U_StarlightInfo").unwrap();
        let starlight_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            starlight_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let voxel_colors_layout = tracer_sm.get_buffer_layout("U_VoxelColors").unwrap();
        let voxel_colors = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            voxel_colors_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let taa_info_layout = taa_sm.get_buffer_layout("U_TaaInfo").unwrap();
        let taa_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            taa_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let god_ray_info_layout = god_ray_sm.get_buffer_layout("U_GodRayInfo").unwrap();
        let god_ray_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            god_ray_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

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

        let player_collider_info_layout = player_collider_sm
            .get_buffer_layout("U_PlayerColliderInfo")
            .unwrap();
        let player_collider_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            player_collider_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let player_collision_result_layout = player_collider_sm
            .get_buffer_layout("B_PlayerCollisionResult")
            .unwrap();

        let player_collision_result = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            player_collision_result_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let terrain_query_count_layout = terrain_query_sm
            .get_buffer_layout("U_TerrainQueryCount")
            .unwrap();
        let terrain_query_count = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            terrain_query_count_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let terrain_query_info = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::CpuToGpu,
            (max_terrain_queries * 2 * std::mem::size_of::<f32>() as u32) as u64,
        );

        let terrain_query_result = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::CpuToGpu,
            (max_terrain_queries * std::mem::size_of::<f32>() as u32) as u64,
        );

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

        let star_noise_tex =
            Self::create_star_noise_tex(&vulkan_ctx, allocator.clone(), Extent2D::new(128, 128));

        let extent_dependent_resources = ExtentDependentResources::new(
            device.clone(),
            allocator.clone(),
            rendering_extent,
            screen_extent,
        );

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

        let grass_blade_resources =
            GrassBladeResources::new(device.clone(), allocator.clone(), false);
        let lavender_resources = LavenderResources::new(device.clone(), allocator.clone(), false);
        let leaves_resources = LeavesResources::new(device.clone(), allocator.clone(), false);
        let grass_blade_resources_lod =
            GrassBladeResources::new(device.clone(), allocator.clone(), true);
        let lavender_resources_lod =
            LavenderResources::new(device.clone(), allocator.clone(), true);
        let leaves_resources_lod = LeavesResources::new(device.clone(), allocator.clone(), true);

        return Self {
            gui_input: Resource::new(gui_input),
            sun_info: Resource::new(sun_info),
            shading_info: Resource::new(shading_info),
            camera_info: Resource::new(camera_info),
            camera_info_prev_frame: Resource::new(camera_info_prev_frame),
            shadow_camera_info: Resource::new(shadow_camera_info),
            env_info: Resource::new(env_info),
            starlight_info: Resource::new(starlight_info),
            // grass_info: Resource::new(grass_info),
            // lavender_info: Resource::new(lavender_info),
            // leaves_info: Resource::new(leaves_info),
            voxel_colors: Resource::new(voxel_colors),
            taa_info: Resource::new(taa_info),
            god_ray_info: Resource::new(god_ray_info),
            post_processing_info: Resource::new(post_processing_info),
            player_collider_info: Resource::new(player_collider_info),
            player_collision_result: Resource::new(player_collision_result),
            terrain_query_count: Resource::new(terrain_query_count),
            terrain_query_info: Resource::new(terrain_query_info),
            terrain_query_result: Resource::new(terrain_query_result),
            grass_blade_resources,
            lavender_resources,
            leaves_resources,
            grass_blade_resources_lod,
            lavender_resources_lod,
            leaves_resources_lod,
            extent_dependent_resources,
            shadow_map_tex: Resource::new(shadow_map_tex),
            shadow_map_tex_for_vsm_ping: Resource::new(shadow_map_tex_for_vsm_ping),
            shadow_map_tex_for_vsm_pong: Resource::new(shadow_map_tex_for_vsm_pong),
            star_noise_tex: Resource::new(star_noise_tex),
            scalar_bn: Resource::new(scalar_bn),
            unit_vec2_bn: Resource::new(unit_vec2_bn),
            unit_vec3_bn: Resource::new(unit_vec3_bn),
            weighted_cosine_bn: Resource::new(weighted_cosine_bn),
            fast_unit_vec3_bn: Resource::new(fast_unit_vec3_bn),
            fast_weighted_cosine_bn: Resource::new(fast_weighted_cosine_bn),
            denoiser_resources: DenoiserResources::new(
                device.clone(),
                allocator.clone(),
                rendering_extent,
                temporal_sm,
                spatial_sm,
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
        self.extent_dependent_resources.on_resize(
            device,
            allocator,
            rendering_extent,
            screen_extent,
        );
        self.denoiser_resources.on_resize(rendering_extent);
    }

    fn create_star_noise_tex(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        extent: Extent2D,
    ) -> Texture {
        let img_desc = ImageDesc {
            extent: extent.into(),
            array_len: 1,
            format: vk::Format::R8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(vulkan_ctx.device().clone(), allocator, &img_desc, &sam_desc);

        let base_path = get_project_root() + "/texture/";
        let path = format!("{}{}.png", base_path, "out_u8");
        tex.get_image()
            .load_and_fill(
                &vulkan_ctx.get_general_queue(),
                vulkan_ctx.command_pool(),
                &path,
                0,
                Some(vk::ImageLayout::GENERAL),
            )
            .unwrap();
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
