mod resources;
pub use resources::*;

use std::collections::HashMap;
use std::time::Instant;

use crate::builder::PlainBuilderResources;
use crate::util::AllocationStrategy;
use crate::util::FirstFitAllocator;
use crate::util::ShaderCompiler;
use crate::util::BENCH;
use crate::vkn::execute_one_time_command;
use crate::vkn::Allocator;
use crate::vkn::ClearValue;
use crate::vkn::CommandBuffer;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::MemoryBarrier;
use crate::vkn::PipelineBarrier;
use crate::vkn::PlainMemberTypeWithData;
use crate::vkn::ShaderModule;
use crate::vkn::StructMemberDataBuilder;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::UVec3;

pub struct ContreeBuilder {
    vulkan_ctx: VulkanContext,
    resources: ContreeBuilderResources,

    frag_img_buffer_setup_ppl: ComputePipeline,
    frag_img_maker_ppl: ComputePipeline,
    // contree_buffer_setup_ppl: ComputePipeline,
    // contree_init_node_ppl: ComputePipeline,
    // contree_tag_node_ppl: ComputePipeline,
    // contree_alloc_node_ppl: ComputePipeline,
    // contree_modify_args_ppl: ComputePipeline,
    frag_img_buffer_setup_ds: DescriptorSet,
    frag_img_maker_ds: DescriptorSet,
    // contree_shared_ds: DescriptorSet,
    // contree_buffer_setup_ds: DescriptorSet,
    // contree_init_node_ds: DescriptorSet,
    // contree_tag_node_ds: DescriptorSet,
    // contree_alloc_node_ds: DescriptorSet,
    // contree_modify_args_ds: DescriptorSet,
    chunk_offset_allocation_table: HashMap<UVec3, u64>,
    contree_buffer_allocator: FirstFitAllocator,

    cmdbuf_table: HashMap<u32, CommandBuffer>,

    frag_img_cmdbuf: CommandBuffer,
}

impl ContreeBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        plain_builder_resources: &PlainBuilderResources,
        voxel_dim_per_chunk: UVec3,
        contree_buffer_pool_size: u64,
    ) -> Self {
        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let frag_img_buffer_setup_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/frag_img_builder/buffer_setup.comp",
            "main",
        )
        .unwrap();
        let frag_img_maker_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/frag_img_builder/frag_img_maker.comp",
            "main",
        )
        .unwrap();

        let resources = ContreeBuilderResources::new(
            vulkan_ctx.device().clone(),
            allocator.clone(),
            voxel_dim_per_chunk,
            contree_buffer_pool_size,
            &frag_img_buffer_setup_sm,
            &frag_img_maker_sm,
        );

        let frag_img_buffer_setup_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &frag_img_buffer_setup_sm);
        let frag_img_maker_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &frag_img_maker_sm);

        let frag_img_buffer_setup_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &frag_img_buffer_setup_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_img_buffer_setup_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.frag_img_maker_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.voxel_dim_indirect),
            WriteDescriptorSet::new_buffer_write(2, &resources.frag_img_build_result),
        ]);
        let frag_img_maker_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &frag_img_maker_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_img_maker_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.frag_img_maker_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.frag_img_build_result),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.frag_img,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &plain_builder_resources.chunk_atlas,
                vk::ImageLayout::GENERAL,
            ),
        ]);

        let contree_buffer_allocator = FirstFitAllocator::new(contree_buffer_pool_size);

        let frag_img_cmdbuf = record_frag_img_cmdbuf(
            &vulkan_ctx,
            &resources,
            &frag_img_buffer_setup_ppl,
            &frag_img_maker_ppl,
            &frag_img_buffer_setup_ds,
            &frag_img_maker_ds,
        );

        init_atlas_images(&vulkan_ctx, &resources);

        return Self {
            vulkan_ctx,
            resources,

            frag_img_buffer_setup_ppl,
            frag_img_maker_ppl,

            frag_img_buffer_setup_ds,
            frag_img_maker_ds,

            chunk_offset_allocation_table: HashMap::new(),
            contree_buffer_allocator,

            cmdbuf_table: HashMap::new(),

            frag_img_cmdbuf,
        };

        fn init_atlas_images(vulkan_context: &VulkanContext, resources: &ContreeBuilderResources) {
            execute_one_time_command(
                vulkan_context.device(),
                vulkan_context.command_pool(),
                &vulkan_context.get_general_queue(),
                |cmdbuf| {
                    resources.frag_img.get_image().record_clear(
                        cmdbuf,
                        Some(vk::ImageLayout::GENERAL),
                        ClearValue::UInt([0, 0, 0, 0]),
                    );
                },
            );
        }

        fn record_frag_img_cmdbuf(
            vulkan_ctx: &VulkanContext,
            resources: &ContreeBuilderResources,
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
            frag_img_maker_ppl.record_dispatch_indirect(&cmdbuf, &resources.voxel_dim_indirect);

            cmdbuf.end();
            return cmdbuf;
        }
    }

    pub fn get_contree_data_size_in_bytes(&self, resources: &ContreeBuilderResources) -> u32 {
        // let layout = &resources
        //     .contree_build_result
        //     .get_layout()
        //     .unwrap()
        //     .root_member;
        // let raw_data = resources.contree_build_result.read_back().unwrap();
        // let reader = StructMemberDataReader::new(layout, &raw_data);
        // let field_val = reader.get_field("size_u32").unwrap();
        // if let PlainMemberTypeWithData::UInt(val) = field_val {
        //     return val * std::mem::size_of::<u32>() as u32;
        // } else {
        //     panic!("Failed to get size_u32 from contree_build_result");
        // }
        todo!();
    }

    fn copy_contree_data_single_to_contree_data(
        &self,
        vulkan_context: &VulkanContext,
        resources: &ContreeBuilderResources,
        write_offset: u64,
        size: u64,
    ) {
        execute_one_time_command(
            vulkan_context.device(),
            vulkan_context.command_pool(),
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                resources.contree_data_single.record_copy_to_buffer(
                    cmdbuf,
                    &resources.contree_data,
                    size,
                    0,
                    write_offset,
                );
            },
        );
    }

    fn build_cmdbuf_for_level(
        &self,
        vulkan_context: &VulkanContext,
        resources: &ContreeBuilderResources,
        voxel_level: u32,
    ) -> CommandBuffer {
        // let shader_access_memory_barrier = MemoryBarrier::new_shader_access();
        // let indirect_access_memory_barrier = MemoryBarrier::new_indirect_access();

        // let shader_access_pipeline_barrier = PipelineBarrier::new(
        //     vk::PipelineStageFlags::COMPUTE_SHADER,
        //     vk::PipelineStageFlags::COMPUTE_SHADER,
        //     vec![shader_access_memory_barrier],
        // );
        // let indirect_access_pipeline_barrier = PipelineBarrier::new(
        //     vk::PipelineStageFlags::COMPUTE_SHADER,
        //     vk::PipelineStageFlags::DRAW_INDIRECT | vk::PipelineStageFlags::COMPUTE_SHADER,
        //     vec![indirect_access_memory_barrier],
        // );

        // //

        // let device = vulkan_context.device();

        // let cmdbuf = CommandBuffer::new(device, vulkan_context.command_pool());
        // cmdbuf.begin(false);

        // cmdbuf.end();
        // cmdbuf

        todo!();
    }

    fn build_frag_img(
        &self,
        resources: &ContreeBuilderResources,
        atlas_read_offset: UVec3,
        atlas_read_dim: UVec3,
    ) {
        let device = self.vulkan_ctx.device();

        update_buffers(
            resources,
            atlas_read_offset,
            atlas_read_dim,
            true, // is_crossing_boundary,
        );

        self.frag_img_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        fn update_buffers(
            resources: &ContreeBuilderResources,
            atlas_read_offset: UVec3,
            atlas_read_dim: UVec3,
            is_crossing_boundary: bool,
        ) {
            let data = StructMemberDataBuilder::from_buffer(&resources.frag_img_maker_info)
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
            resources
                .frag_img_maker_info
                .fill_with_raw_u8(&data)
                .unwrap();
        }
    }

    fn get_fraglist_length(&self) -> u32 {
        // let layout = &self
        //     .resources
        //     .frag_img_build_result
        //     .get_layout()
        //     .unwrap()
        //     .root_member;
        // let raw_data = self.resources.frag_img_build_result.read_back().unwrap();
        // let reader = StructMemberDataReader::new(layout, &raw_data);
        // let field_val = reader.get_field("frag_img_len").unwrap();
        // if let PlainMemberTypeWithData::UInt(val) = field_val {
        //     val
        // } else {
        //     panic!("Expected UInt type for frag_img_len")
        // }

        todo!();
    }

    pub fn get_resources(&self) -> &ContreeBuilderResources {
        &self.resources
    }

    fn build_contree(&mut self, frag_img_len: u32, atlas_dim: UVec3) {
        todo!();

        // let device = self.vulkan_ctx.device();

        // update_buffers(&self.resources, frag_img_len, atlas_dim.x);

        // let level = get_level(atlas_dim);
        // let cmdbuf = if let Some(cmdbuf) = self.cmdbuf_table.get(&level) {
        //     cmdbuf.clone()
        // } else {
        //     let newly_created =
        //         self.build_cmdbuf_for_level(&self.vulkan_ctx, &self.resources, level);
        //     self.cmdbuf_table.insert(level, newly_created.clone());
        //     newly_created
        // };

        // cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);
        // device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        // fn get_level(voxel_dim: UVec3) -> u32 {
        //     return log2(voxel_dim.x);

        //     fn log2(x: u32) -> u32 {
        //         31 - x.leading_zeros()
        //     }
        // }

        // fn update_buffers(
        //     resources: &ContreeBuilderResources,
        //     frag_img_len: u32,
        //     voxel_dim_xyz: u32,
        // ) {
        //     let data = StructMemberDataBuilder::from_buffer(&resources.contree_build_info)
        //         .set_field(
        //             "frag_img_len",
        //             PlainMemberTypeWithData::UInt(frag_img_len),
        //         )
        //         .unwrap()
        //         .set_field(
        //             "voxel_dim_xyz",
        //             PlainMemberTypeWithData::UInt(voxel_dim_xyz),
        //         )
        //         .unwrap()
        //         .get_data_u8();
        //     resources
        //         .contree_build_info
        //         .fill_with_raw_u8(&data)
        //         .unwrap();
        // }
    }

    pub fn build_and_alloc(
        &mut self,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
    ) -> Result<Option<u64>, String> {
        let t_start = Instant::now();

        // check_dim is trivial, but you can time it too if you want
        check_dim(atlas_dim)?;

        // 1) build_frag_img
        let t1 = Instant::now();
        self.build_frag_img(&self.resources, atlas_offset, atlas_dim);
        BENCH.lock().unwrap().record("build_frag_img", t1.elapsed());

        // if nothing to do early-exit
        // let frag_img_len = self.get_fraglist_length();
        // if frag_img_len == 0 {
        //     log::debug!("No fragments found, skipping contree build.");
        //     // record total so far
        //     BENCH
        //         .lock()
        //         .unwrap()
        //         .record("build_and_alloc_total", t_start.elapsed());
        //     return Ok(None);
        // }

        return Ok(None); // todo!();

        // // 2) build_contree
        // let t2 = Instant::now();
        // self.build_contree(frag_img_len, atlas_dim);
        // BENCH.lock().unwrap().record("build_contree", t2.elapsed());

        // // 3) allocate & copy
        // let contree_size = self.get_contree_data_size_in_bytes(&self.resources);
        // assert!(contree_size > 0);
        // let write_offset = self.allocate_chunk(contree_size as u64, atlas_offset);

        // let t3 = Instant::now();
        // self.copy_contree_data_single_to_contree_data(
        //     &self.vulkan_ctx,
        //     &self.resources,
        //     write_offset,
        //     contree_size as u64,
        // );
        // BENCH
        //     .lock()
        //     .unwrap()
        //     .record("copy_contree_data", t3.elapsed());

        // // total for this call
        // BENCH
        //     .lock()
        //     .unwrap()
        //     .record("build_and_alloc_total", t_start.elapsed());

        // return Ok(Some(write_offset));

        // ───────── helper ─────────
        fn check_dim(voxel_dim: UVec3) -> Result<(), String> {
            if voxel_dim.x != voxel_dim.y || voxel_dim.y != voxel_dim.z {
                return Err(format!(
                    "Voxel dimension must be equal in all dimensions, but got: {}",
                    voxel_dim
                ));
            }
            if !voxel_dim.x.is_power_of_two() {
                return Err(format!(
                    "Voxel dimension must be a power of two, but got: {}",
                    voxel_dim
                ));
            }
            Ok(())
        }
    }
    /// Allocate a chunk of contree data and store the allocation id in the offset_allocation_table.
    ///
    /// If the chunk already exists, deallocate it first.
    fn allocate_chunk(&mut self, buffer_size: u64, atlas_offset: UVec3) -> u64 {
        if self
            .chunk_offset_allocation_table
            .contains_key(&atlas_offset)
        {
            let allocation_id = self
                .chunk_offset_allocation_table
                .remove(&atlas_offset)
                .unwrap();
            self.contree_buffer_allocator
                .deallocate(allocation_id)
                .unwrap();
        }
        let allocation = self.contree_buffer_allocator.allocate(buffer_size).unwrap();
        self.chunk_offset_allocation_table
            .insert(atlas_offset, allocation.id);
        return allocation.offset;
    }

    // pub fn update_contree_offset_atlas_tex(
    //     &mut self,
    //     vulkan_context: &VulkanContext,
    //     resources: &ContreeBuilderResources,
    //     voxel_dim: UVec3,
    //     visible_chunk_dim: UVec3,
    // ) {
    //     let mut offset_table = vec![];
    //     for (chunk_pos, allocation_id) in self.chunk_offset_allocation_table.iter() {
    //         let allocation = self.contree_buffer_allocator.lookup(*allocation_id).unwrap();
    //         let offset_in_bytes = allocation.offset;
    //         let offset_in_u32 = offset_in_bytes / std::mem::size_of::<u32>() as u64;
    //         offset_table.push((*chunk_pos, offset_in_u32));
    //     }

    //     // TODO: implement further
    //     // for now, just a simple logic, to fit all chunk offsets stored inside the table into a fixed size buffer.
    //     let mut offset_data: Vec<u32> = vec![
    //         0;
    //         visible_chunk_dim.x as usize
    //             * visible_chunk_dim.y as usize
    //             * visible_chunk_dim.z as usize
    //     ];

    //     // update offset_data accordingly
    //     for (atlas_offset, offset_in_u32) in offset_table.iter() {
    //         let chunk_offset = atlas_offset_to_chunk_offset(*atlas_offset, voxel_dim);

    //         assert!(in_bounds(chunk_offset, visible_chunk_dim));
    //         let linear_index = to_linear_index(chunk_offset, visible_chunk_dim);
    //         // write with an offset of 1, because 0 is reserved for empty chunk
    //         offset_data[linear_index as usize] = (*offset_in_u32 + 1) as u32;

    //         fn atlas_offset_to_chunk_offset(atlas_offset: UVec3, voxel_dim: UVec3) -> UVec3 {
    //             atlas_offset / voxel_dim
    //         }
    //         fn in_bounds(chunk_pos: UVec3, dim: UVec3) -> bool {
    //             chunk_pos.x < dim.x && chunk_pos.y < dim.y && chunk_pos.z < dim.z
    //         }
    //         fn to_linear_index(chunk_pos: UVec3, dim: UVec3) -> u32 {
    //             chunk_pos.x + chunk_pos.y * dim.x + chunk_pos.z * dim.x * dim.y
    //         }
    //     }

    //     // fill the texture
    //     resources
    //         .contree_offset_atlas_tex
    //         .get_image()
    //         .fill_with_raw_u32(
    //             &vulkan_context.get_general_queue(),
    //             vulkan_context.command_pool(),
    //             TextureRegion::from_image(&resources.contree_offset_atlas_tex.get_image()),
    //             &offset_data,
    //             None,
    //         )
    //         .unwrap();
    // }
}
