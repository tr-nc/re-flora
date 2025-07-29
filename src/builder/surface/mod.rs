mod resources;
use super::PlainBuilderResources;
use crate::{
    geom::UAabb3,
    util::ShaderCompiler,
    vkn::{
        Allocator, Buffer, ClearValue, ColorClearValue, CommandBuffer, ComputePipeline,
        DescriptorPool, Extent3D, PlainMemberTypeWithData, ShaderModule, StructMemberDataBuilder,
        StructMemberDataReader, VulkanContext, WriteDescriptorSet,
    },
};
use anyhow::Result;
use ash::vk;
use glam::UVec3;
pub use resources::*;

pub struct SurfaceBuilder {
    vulkan_ctx: VulkanContext,
    pub resources: SurfaceResources,

    #[allow(dead_code)]
    pool: DescriptorPool,

    make_surface_ppl: ComputePipeline,

    chunk_bound: UAabb3,
    voxel_dim_per_chunk: UVec3,
}

impl SurfaceBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        plain_builder_resources: &PlainBuilderResources,
        voxel_dim_per_chunk: UVec3,
        chunk_bound: UAabb3,
    ) -> Self {
        let device = vulkan_ctx.device();

        let make_surface_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/make_surface.comp",
            "main",
        )
        .unwrap();

        let resources = SurfaceResources::new(
            device.clone(),
            allocator,
            voxel_dim_per_chunk,
            &make_surface_sm,
            chunk_bound,
        );

        let pool = DescriptorPool::new(device).unwrap();

        let make_surface_ppl = ComputePipeline::new(
            device,
            &make_surface_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );

        Self {
            vulkan_ctx,
            resources,
            pool,
            make_surface_ppl,
            chunk_bound,
            voxel_dim_per_chunk,
        }
    }

    fn update_grass_instance_set(&self, chunk_id: UVec3) {
        let chunk_resources = &self
            .resources
            .instances
            .chunk_flora_instances
            .iter()
            .find(|(_, resources)| resources.chunk_id == chunk_id)
            .unwrap()
            .1;

        self.make_surface_ppl.write_descriptor_set(
            1,
            WriteDescriptorSet::new_buffer_write(
                0,
                &chunk_resources.get(FloraType::Grass).instances_buf,
            ),
        );
        self.make_surface_ppl.write_descriptor_set(
            1,
            WriteDescriptorSet::new_buffer_write(
                1,
                &chunk_resources.get(FloraType::Lavender).instances_buf,
            ),
        );
    }

    /// Returns active_voxel_len
    pub fn build_surface(&mut self, chunk_id: UVec3) -> Result<u32> {
        if !self.chunk_bound.in_bound(chunk_id) {
            return Err(anyhow::anyhow!("Chunk ID out of bounds"));
        }

        let atlas_read_offset = chunk_id * self.voxel_dim_per_chunk;
        let atlas_read_dim = self.voxel_dim_per_chunk;

        let device = self.vulkan_ctx.device();

        update_make_surface_info(
            &self.resources.make_surface_info,
            atlas_read_offset,
            atlas_read_dim,
            true,
        )?;

        cleanup_make_surface_result(&self.resources.make_surface_result)?;

        self.update_grass_instance_set(chunk_id);

        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        self.resources.surface.get_image().record_clear(
            &cmdbuf,
            Some(vk::ImageLayout::GENERAL),
            0,
            ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
        );

        let extent = Extent3D {
            width: self.voxel_dim_per_chunk.x,
            height: self.voxel_dim_per_chunk.y,
            depth: self.voxel_dim_per_chunk.z,
        };

        self.make_surface_ppl.record(&cmdbuf, extent, None);

        cmdbuf.end();

        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);

        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        let (active_voxel_len, grass_instance_len, lavender_instance_len) =
            get_result(&self.resources.make_surface_result);

        let chunk_resources = self
            .resources
            .instances
            .chunk_flora_instances
            .iter_mut()
            .find(|(_, resources)| resources.chunk_id == chunk_id)
            .unwrap();
        chunk_resources.1.get_mut(FloraType::Grass).instances_len = grass_instance_len;
        chunk_resources.1.get_mut(FloraType::Lavender).instances_len = lavender_instance_len;

        return Ok(active_voxel_len);

        fn update_make_surface_info(
            make_surface_info: &Buffer,
            atlas_read_offset: UVec3,
            atlas_read_dim: UVec3,
            is_crossing_boundary: bool,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&make_surface_info)
                .set_field(
                    "atlas_read_offset",
                    PlainMemberTypeWithData::UVec3(atlas_read_offset.to_array()),
                )
                .set_field(
                    "atlas_read_dim",
                    PlainMemberTypeWithData::UVec3(atlas_read_dim.to_array()),
                )
                .set_field(
                    "is_crossing_boundary",
                    PlainMemberTypeWithData::UInt(if is_crossing_boundary { 1 } else { 0 }),
                )
                .build()?;
            make_surface_info.fill_with_raw_u8(&data)?;
            Ok(())
        }

        fn cleanup_make_surface_result(make_surface_result: &Buffer) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&make_surface_result)
                .set_field("active_voxel_len", PlainMemberTypeWithData::UInt(0))
                .set_field("grass_instance_len", PlainMemberTypeWithData::UInt(0))
                .set_field("lavender_instance_len", PlainMemberTypeWithData::UInt(0))
                .build()?;
            make_surface_result.fill_with_raw_u8(&data)?;
            Ok(())
        }

        /// Returns: (active_voxel_len, grass_instance_len, lavender_instance_len)
        fn get_result(frag_img_build_result: &Buffer) -> (u32, u32, u32) {
            let layout = &frag_img_build_result.get_layout().unwrap().root_member;
            let raw_data = frag_img_build_result.read_back().unwrap();
            let reader = StructMemberDataReader::new(layout, &raw_data);

            let active_voxel_len = if let PlainMemberTypeWithData::UInt(val) =
                reader.get_field("active_voxel_len").unwrap()
            {
                val
            } else {
                panic!("Expected UInt type for active_voxel_len")
            };
            let grass_instance_len = if let PlainMemberTypeWithData::UInt(val) =
                reader.get_field("grass_instance_len").unwrap()
            {
                val
            } else {
                panic!("Expected UInt type for grass_instance_len")
            };
            let lavender_instance_len = if let PlainMemberTypeWithData::UInt(val) =
                reader.get_field("lavender_instance_len").unwrap()
            {
                val
            } else {
                panic!("Expected UInt type for lavender_instance_len")
            };
            (active_voxel_len, grass_instance_len, lavender_instance_len)
        }
    }

    pub fn get_resources(&self) -> &SurfaceResources {
        &self.resources
    }
}
