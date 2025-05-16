mod resources;
use ash::vk;
use glam::UVec3;
pub use resources::*;

use crate::{
    util::ShaderCompiler,
    vkn::{
        execute_one_time_command, Allocator, Buffer, ClearValue, CommandBuffer, ComputePipeline,
        DescriptorPool, DescriptorSet, MemoryBarrier, PipelineBarrier, PlainMemberTypeWithData,
        ShaderModule, StructMemberDataBuilder, StructMemberDataReader, Texture, VulkanContext,
        WriteDescriptorSet,
    },
};

use super::PlainBuilderResources;

pub struct SurfaceBuilder {
    vulkan_ctx: VulkanContext,
    resources: SurfaceResources,

    #[allow(dead_code)]
    buffer_setup_ppl: ComputePipeline,
    #[allow(dead_code)]
    make_surface_ppl: ComputePipeline,

    #[allow(dead_code)]
    buffer_setup_ds: DescriptorSet,
    #[allow(dead_code)]
    make_surface_ds: DescriptorSet,

    cmdbuf: CommandBuffer,

    voxel_dim_per_chunk: UVec3,
}

impl SurfaceBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        plain_builder_resources: &PlainBuilderResources,
        voxel_dim_per_chunk: UVec3,
    ) -> Self {
        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let buffer_setup_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/surface/buffer_setup.comp",
            "main",
        )
        .unwrap();
        let make_surface_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/surface/make_surface.comp",
            "main",
        )
        .unwrap();

        let resources = SurfaceResources::new(
            vulkan_ctx.device().clone(),
            allocator,
            voxel_dim_per_chunk,
            &buffer_setup_sm,
        );

        let buffer_setup_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &buffer_setup_sm);
        let make_surface_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &make_surface_sm);

        let buffer_setup_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &buffer_setup_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        buffer_setup_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.make_surface_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.voxel_dim_indirect),
            WriteDescriptorSet::new_buffer_write(2, &resources.make_surface_result),
        ]);
        let make_surface_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &make_surface_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        make_surface_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.make_surface_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.make_surface_result),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.surface,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &plain_builder_resources.chunk_atlas,
                vk::ImageLayout::GENERAL,
            ),
        ]);

        init_surface_image(&vulkan_ctx, &resources.surface);

        let cmdbuf = record_cmdbuf(
            &vulkan_ctx,
            &resources.voxel_dim_indirect,
            &buffer_setup_ppl,
            &make_surface_ppl,
            &buffer_setup_ds,
            &make_surface_ds,
        );

        return Self {
            vulkan_ctx,
            resources,

            buffer_setup_ppl,
            make_surface_ppl,
            buffer_setup_ds,
            make_surface_ds,

            cmdbuf,

            voxel_dim_per_chunk,
        };

        fn init_surface_image(vulkan_context: &VulkanContext, surface: &Texture) {
            execute_one_time_command(
                vulkan_context.device(),
                vulkan_context.command_pool(),
                &vulkan_context.get_general_queue(),
                |cmdbuf| {
                    surface.get_image().record_clear(
                        cmdbuf,
                        Some(vk::ImageLayout::GENERAL),
                        ClearValue::UInt([0, 0, 0, 0]),
                    );
                },
            );
        }

        fn record_cmdbuf(
            vulkan_ctx: &VulkanContext,
            voxel_dim_indirect: &Buffer,
            buffer_setup_ppl: &ComputePipeline,
            frag_img_maker_ppl: &ComputePipeline,
            buffer_setup_ds: &DescriptorSet,
            frag_img_maker_ds: &DescriptorSet,
        ) -> CommandBuffer {
            let shader_access_memory_barrier = MemoryBarrier::new_shader_access();
            let indirect_access_memory_barrier = MemoryBarrier::new_indirect_access();

            let shader_access_pipeline_barrier = PipelineBarrier::new(
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vec![shader_access_memory_barrier],
            );
            let indirect_access_pipeline_barrier = PipelineBarrier::new(
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::DRAW_INDIRECT | vk::PipelineStageFlags::COMPUTE_SHADER,
                vec![indirect_access_memory_barrier],
            );

            let device = vulkan_ctx.device();

            let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());
            cmdbuf.begin(false);

            buffer_setup_ppl.record_bind(&cmdbuf);
            buffer_setup_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(buffer_setup_ds),
                0,
            );
            buffer_setup_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);

            shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
            indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

            frag_img_maker_ppl.record_bind(&cmdbuf);
            frag_img_maker_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(frag_img_maker_ds),
                0,
            );
            frag_img_maker_ppl.record_dispatch_indirect(&cmdbuf, &voxel_dim_indirect);

            cmdbuf.end();
            return cmdbuf;
        }
    }

    /// Returns the number of active voxels in the surface.
    pub fn build_surface(&mut self, atlas_read_offset: UVec3) -> u32 {
        let atlas_read_dim = self.voxel_dim_per_chunk;

        let device = self.vulkan_ctx.device();

        update_buffers(
            &self.resources.make_surface_info,
            atlas_read_offset,
            atlas_read_dim,
            true, // is_crossing_boundary,
        );

        self.cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        return get_active_voxel_len(&self.resources.make_surface_result);

        fn update_buffers(
            make_surface_info: &Buffer,
            atlas_read_offset: UVec3,
            atlas_read_dim: UVec3,
            is_crossing_boundary: bool,
        ) {
            let data = StructMemberDataBuilder::from_buffer(&make_surface_info)
                .set_field(
                    "atlas_read_offset",
                    PlainMemberTypeWithData::UVec3(atlas_read_offset.to_array()),
                )
                .unwrap()
                .set_field(
                    "atlas_read_dim",
                    PlainMemberTypeWithData::UVec3(atlas_read_dim.to_array()),
                )
                .unwrap()
                .set_field(
                    "is_crossing_boundary",
                    PlainMemberTypeWithData::UInt(if is_crossing_boundary { 1 } else { 0 }),
                )
                .unwrap()
                .get_data_u8();
            make_surface_info.fill_with_raw_u8(&data).unwrap();
        }

        fn get_active_voxel_len(frag_img_build_result: &Buffer) -> u32 {
            let layout = &frag_img_build_result.get_layout().unwrap().root_member;
            let raw_data = frag_img_build_result.read_back().unwrap();
            let reader = StructMemberDataReader::new(layout, &raw_data);
            let field_val = reader.get_field("active_voxel_len").unwrap();
            if let PlainMemberTypeWithData::UInt(val) = field_val {
                val
            } else {
                panic!("Expected UInt type for active_voxel_len")
            }
        }
    }

    pub fn get_resources(&self) -> &SurfaceResources {
        &self.resources
    }
}
