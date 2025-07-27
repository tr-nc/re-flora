mod resources;
pub use resources::*;

use super::SurfaceResources;
use crate::util::AllocationStrategy;
use crate::util::FirstFitAllocator;
use crate::util::ShaderCompiler;
use crate::vkn::Allocator;
use crate::vkn::Buffer;
use crate::vkn::CommandBuffer;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::Extent3D;
use crate::vkn::MemoryBarrier;
use crate::vkn::PipelineBarrier;
use crate::vkn::PlainMemberTypeWithData;
use crate::vkn::ShaderModule;
use crate::vkn::StructMemberDataBuilder;
use crate::vkn::StructMemberDataReader;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use anyhow::Result;
use ash::vk;
use glam::UVec3;
use std::collections::HashMap;

const SIZE_OF_NODE_ELEMENT: u64 = 3 * std::mem::size_of::<u32>() as u64;
const SIZE_OF_LEAF_ELEMENT: u64 = 1 * std::mem::size_of::<u32>() as u64;

pub struct ContreeBuilder {
    vulkan_ctx: VulkanContext,
    resources: ContreeBuilderResources,

    #[allow(dead_code)]
    contree_buffer_setup_ppl: ComputePipeline,
    #[allow(dead_code)]
    contree_leaf_write_ppl: ComputePipeline,
    #[allow(dead_code)]
    contree_tree_write_ppl: ComputePipeline,
    #[allow(dead_code)]
    contree_buffer_update_ppl: ComputePipeline,
    #[allow(dead_code)]
    contree_last_buffer_update_ppl: ComputePipeline,
    #[allow(dead_code)]
    contree_concat_ppl: ComputePipeline,

    #[allow(dead_code)]
    fixed_pool: DescriptorPool,

    /// Atlas offset <-> (node_alloc_id, leaf_alloc_id)
    chunk_offset_allocation_table: HashMap<UVec3, (u64, u64)>,

    contree_cmdbuf: CommandBuffer,

    leaf_allocator: FirstFitAllocator,
    node_allocator: FirstFitAllocator,

    voxel_dim_per_chunk: UVec3,
}

impl ContreeBuilder {
    fn update_contree_buffer_setup_ds(ds: &DescriptorSet, resources: &ContreeBuilderResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.contree_build_state),
            WriteDescriptorSet::new_buffer_write(2, &resources.level_dispatch_indirect),
            WriteDescriptorSet::new_buffer_write(3, &resources.counter_for_levels),
            WriteDescriptorSet::new_buffer_write(4, &resources.node_offset_for_levels),
            WriteDescriptorSet::new_buffer_write(5, &resources.contree_build_result),
        ]);
    }

    fn update_contree_leaf_write_ds(
        ds: &DescriptorSet,
        resources: &ContreeBuilderResources,
        surfacer_resources: &SurfaceResources,
    ) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.contree_build_state),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &surfacer_resources.surface,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_buffer_write(3, &resources.node_offset_for_levels),
            WriteDescriptorSet::new_buffer_write(4, &resources.sparse_nodes),
            WriteDescriptorSet::new_buffer_write(5, &resources.contree_leaf_data),
            WriteDescriptorSet::new_buffer_write(6, &resources.contree_build_result),
        ]);
    }

    fn update_contree_tree_write_ds(ds: &DescriptorSet, resources: &ContreeBuilderResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_state),
            WriteDescriptorSet::new_buffer_write(1, &resources.node_offset_for_levels),
            WriteDescriptorSet::new_buffer_write(2, &resources.sparse_nodes),
            WriteDescriptorSet::new_buffer_write(3, &resources.dense_nodes),
            WriteDescriptorSet::new_buffer_write(4, &resources.counter_for_levels),
            WriteDescriptorSet::new_buffer_write(5, &resources.contree_build_result),
        ]);
    }

    fn update_contree_buffer_update_ds(ds: &DescriptorSet, resources: &ContreeBuilderResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_state),
            WriteDescriptorSet::new_buffer_write(1, &resources.level_dispatch_indirect),
        ]);
    }

    fn update_contree_last_buffer_update_ds(
        ds: &DescriptorSet,
        resources: &ContreeBuilderResources,
    ) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_result),
            WriteDescriptorSet::new_buffer_write(1, &resources.concat_dispatch_indirect),
            WriteDescriptorSet::new_buffer_write(2, &resources.sparse_nodes),
            WriteDescriptorSet::new_buffer_write(3, &resources.dense_nodes),
            WriteDescriptorSet::new_buffer_write(4, &resources.counter_for_levels),
        ]);
    }

    fn update_contree_concat_ds(ds: &DescriptorSet, resources: &ContreeBuilderResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.node_offset_for_levels),
            WriteDescriptorSet::new_buffer_write(2, &resources.dense_nodes),
            WriteDescriptorSet::new_buffer_write(3, &resources.counter_for_levels),
            WriteDescriptorSet::new_buffer_write(4, &resources.contree_node_data),
            WriteDescriptorSet::new_buffer_write(5, &resources.contree_build_result),
        ]);
    }

    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        surfacer_resources: &SurfaceResources,
        voxel_dim_per_chunk: UVec3,
        node_pool_size_in_bytes: u64,
        leaf_pool_size_in_bytes: u64,
    ) -> Self {
        assert!(
            voxel_dim_per_chunk.x == voxel_dim_per_chunk.y
                && voxel_dim_per_chunk.x == voxel_dim_per_chunk.z,
            "ContreeBuilder: voxel_dim_per_chunk must be a cube"
        );
        assert!(is_power_of_four(voxel_dim_per_chunk.x));

        let device = vulkan_ctx.device();

        // --- Shader Modules ---
        let contree_buffer_setup_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/buffer_setup.comp",
            "main",
        )
        .unwrap();
        let contree_leaf_write_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/leaf_write.comp",
            "main",
        )
        .unwrap();
        let contree_tree_write_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/tree_write.comp",
            "main",
        )
        .unwrap();
        let contree_buffer_update_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/buffer_update.comp",
            "main",
        )
        .unwrap();
        let contree_last_buffer_update_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/last_buffer_update.comp",
            "main",
        )
        .unwrap();
        let contree_concat_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/concat.comp",
            "main",
        )
        .unwrap();

        // --- Resources ---
        let resources = ContreeBuilderResources::new(
            device.clone(),
            allocator.clone(),
            voxel_dim_per_chunk,
            node_pool_size_in_bytes,
            leaf_pool_size_in_bytes,
            &contree_buffer_setup_sm,
            &contree_leaf_write_sm,
            &contree_tree_write_sm,
            &contree_last_buffer_update_sm,
        );

        // --- Pipelines ---
        let contree_buffer_setup_ppl = ComputePipeline::new_tmp(device, &contree_buffer_setup_sm);
        let contree_leaf_write_ppl = ComputePipeline::new_tmp(device, &contree_leaf_write_sm);
        let contree_tree_write_ppl = ComputePipeline::new_tmp(device, &contree_tree_write_sm);
        let contree_buffer_update_ppl = ComputePipeline::new_tmp(device, &contree_buffer_update_sm);
        let contree_last_buffer_update_ppl =
            ComputePipeline::new_tmp(device, &contree_last_buffer_update_sm);
        let contree_concat_ppl = ComputePipeline::new_tmp(device, &contree_concat_sm);

        // --- Descriptor Sets ---
        let fixed_pool = DescriptorPool::new(device).unwrap();
        let alloc_set_fn = |ppl: &ComputePipeline| -> DescriptorSet {
            fixed_pool
                .allocate_set(&ppl.get_layout().get_descriptor_set_layouts()[&0])
                .unwrap()
        };

        let contree_buffer_setup_ds = alloc_set_fn(&contree_buffer_setup_ppl);
        let contree_leaf_write_ds = alloc_set_fn(&contree_leaf_write_ppl);
        let contree_tree_write_ds = alloc_set_fn(&contree_tree_write_ppl);
        let contree_buffer_update_ds = alloc_set_fn(&contree_buffer_update_ppl);
        let contree_last_buffer_update_ds = alloc_set_fn(&contree_last_buffer_update_ppl);
        let contree_concat_ds = alloc_set_fn(&contree_concat_ppl);

        Self::update_contree_buffer_setup_ds(&contree_buffer_setup_ds, &resources);
        Self::update_contree_leaf_write_ds(&contree_leaf_write_ds, &resources, surfacer_resources);
        Self::update_contree_tree_write_ds(&contree_tree_write_ds, &resources);
        Self::update_contree_buffer_update_ds(&contree_buffer_update_ds, &resources);
        Self::update_contree_last_buffer_update_ds(&contree_last_buffer_update_ds, &resources);
        Self::update_contree_concat_ds(&contree_concat_ds, &resources);

        contree_buffer_setup_ppl.set_descriptor_sets(vec![contree_buffer_setup_ds]);
        contree_leaf_write_ppl.set_descriptor_sets(vec![contree_leaf_write_ds]);
        contree_tree_write_ppl.set_descriptor_sets(vec![contree_tree_write_ds]);
        contree_buffer_update_ppl.set_descriptor_sets(vec![contree_buffer_update_ds]);
        contree_last_buffer_update_ppl.set_descriptor_sets(vec![contree_last_buffer_update_ds]);
        contree_concat_ppl.set_descriptor_sets(vec![contree_concat_ds]);

        // --- Command Buffer Recording ---
        let contree_cmdbuf = Self::record_cmdbuf(
            &vulkan_ctx,
            &resources,
            get_level(voxel_dim_per_chunk),
            &contree_buffer_setup_ppl,
            &contree_leaf_write_ppl,
            &contree_tree_write_ppl,
            &contree_buffer_update_ppl,
            &contree_last_buffer_update_ppl,
            &contree_concat_ppl,
        );

        let node_allocator = FirstFitAllocator::new(node_pool_size_in_bytes);
        let leaf_allocator = FirstFitAllocator::new(leaf_pool_size_in_bytes);

        Self {
            vulkan_ctx,
            resources,
            contree_buffer_setup_ppl,
            contree_leaf_write_ppl,
            contree_tree_write_ppl,
            contree_buffer_update_ppl,
            contree_last_buffer_update_ppl,
            contree_concat_ppl,
            fixed_pool,
            chunk_offset_allocation_table: HashMap::new(),
            contree_cmdbuf,
            node_allocator,
            leaf_allocator,
            voxel_dim_per_chunk,
        }
    }

    fn record_cmdbuf(
        vulkan_ctx: &VulkanContext,
        resources: &ContreeBuilderResources,
        total_levels: u32,
        contree_buffer_setup_ppl: &ComputePipeline,
        contree_leaf_write_ppl: &ComputePipeline,
        contree_tree_write_ppl: &ComputePipeline,
        contree_buffer_update_ppl: &ComputePipeline,
        contree_last_buffer_update_ppl: &ComputePipeline,
        contree_concat_ppl: &ComputePipeline,
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

        let dispatch_1x1x1 = Extent3D {
            width: 1,
            height: 1,
            depth: 1,
        };

        contree_buffer_setup_ppl.record(&cmdbuf, dispatch_1x1x1, None);

        shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
        indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

        contree_leaf_write_ppl.record_indirect(&cmdbuf, &resources.level_dispatch_indirect, None);

        shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

        contree_buffer_update_ppl.record(&cmdbuf, dispatch_1x1x1, None);

        shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
        indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

        for i in 0..(total_levels - 2) {
            contree_tree_write_ppl.record_indirect(
                &cmdbuf,
                &resources.level_dispatch_indirect,
                None,
            );

            shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

            if i != total_levels - 3 {
                contree_buffer_update_ppl.record(&cmdbuf, dispatch_1x1x1, None);
            } else {
                contree_last_buffer_update_ppl.record(&cmdbuf, dispatch_1x1x1, None);
            }

            shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
            indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
        }

        contree_concat_ppl.record_indirect(&cmdbuf, &resources.concat_dispatch_indirect, None);

        cmdbuf.end();
        cmdbuf
    }

    /// Returns: (node_size_in_bytes, leaf_size_in_bytes)
    pub fn get_contree_size_info(&self, resources: &ContreeBuilderResources) -> (u64, u64) {
        let layout = &resources
            .contree_build_result
            .get_layout()
            .unwrap()
            .root_member;
        let raw_data = resources.contree_build_result.read_back().unwrap();
        let reader = StructMemberDataReader::new(layout, &raw_data);

        let leaf_len = reader.get_field("leaf_len").unwrap();
        let leaf_size_in_bytes = if let PlainMemberTypeWithData::UInt(val) = leaf_len {
            val as u64 * SIZE_OF_LEAF_ELEMENT
        } else {
            panic!("Expected UInt type for leaf_len")
        };

        let node_len = reader.get_field("node_len").unwrap();
        let node_size_in_bytes = if let PlainMemberTypeWithData::UInt(val) = node_len {
            val as u64 * SIZE_OF_NODE_ELEMENT
        } else {
            panic!("Expected UInt type for node_len")
        };

        (node_size_in_bytes, leaf_size_in_bytes)
    }

    pub fn get_resources(&self) -> &ContreeBuilderResources {
        &self.resources
    }

    fn build_contree(
        &mut self,
        contree_dim: UVec3,
        node_write_offset: u64,
        leaf_write_offset: u64,
    ) -> Result<()> {
        let device = self.vulkan_ctx.device();

        update_buffers(
            &self.resources.contree_build_info,
            contree_dim,
            get_level(contree_dim),
            node_write_offset as u32,
            leaf_write_offset as u32,
        )?;

        let cmdbuf = self.contree_cmdbuf.clone();
        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        return Ok(());

        fn update_buffers(
            contree_build_info: &Buffer,
            contree_dim: UVec3,
            max_level: u32,
            node_write_offset: u32,
            leaf_write_offset: u32,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(contree_build_info)
                .set_field("dim", PlainMemberTypeWithData::UInt(contree_dim.x))
                .set_field("max_level", PlainMemberTypeWithData::UInt(max_level))
                .set_field(
                    "node_write_offset",
                    PlainMemberTypeWithData::UInt(node_write_offset),
                )
                .set_field(
                    "leaf_write_offset",
                    PlainMemberTypeWithData::UInt(leaf_write_offset),
                )
                .build()?;
            contree_build_info.fill_with_raw_u8(&data)?;
            Ok(())
        }
    }

    /// Returns: (node_alloc_offset, leaf_alloc_offset)
    pub fn build_and_alloc(&mut self, atlas_offset: UVec3) -> Result<Option<(u64, u64)>> {
        let atlas_dim = self.voxel_dim_per_chunk;

        // preallocate 10MB for both the currentl node and leaf buffer to be built
        const MAX_NODE_BUFFER_SIZE_IN_BYTES: u64 = 10 * 1024 * 1024;
        const MAX_LEAF_BUFFER_SIZE_IN_BYTES: u64 = 10 * 1024 * 1024;
        let (node_alloc_offset_in_bytes, leaf_alloc_offset_in_bytes) = self.pre_allocate_chunk(
            MAX_NODE_BUFFER_SIZE_IN_BYTES,
            MAX_LEAF_BUFFER_SIZE_IN_BYTES,
            atlas_offset,
        );
        // the offset's unit is in bytes, we need to convert it to array idx, each element is a 3*u32
        let node_alloc_offset = node_alloc_offset_in_bytes / SIZE_OF_NODE_ELEMENT;
        // the element of leaf data is a u32
        let leaf_alloc_offset = leaf_alloc_offset_in_bytes / SIZE_OF_LEAF_ELEMENT;

        self.build_contree(atlas_dim, node_alloc_offset, leaf_alloc_offset)?;

        let (confirmed_node_buffer_size_in_bytes, confirmed_leaf_buffer_size_in_bytes) =
            self.get_contree_size_info(&self.resources);

        self.confirm_allocation_of_chunk(
            confirmed_node_buffer_size_in_bytes,
            confirmed_leaf_buffer_size_in_bytes,
            atlas_offset,
        );

        Ok(Some((node_alloc_offset, leaf_alloc_offset)))
    }

    /// Allocate a chunk of data and store the allocation id in the offset_allocation_table.
    ///
    /// Returns: (node_alloc_offset_in_bytes, leaf_alloc_offset_in_bytes)
    /// If the chunk already exists, deallocate it first.
    fn pre_allocate_chunk(
        &mut self,
        max_node_buffer_size_in_bytes: u64,
        max_leaf_buffer_size_in_bytes: u64,
        atlas_offset: UVec3,
    ) -> (u64, u64) {
        if self
            .chunk_offset_allocation_table
            .contains_key(&atlas_offset)
        {
            let (node_alloc_id, leaf_alloc_id) = self
                .chunk_offset_allocation_table
                .remove(&atlas_offset)
                .unwrap();
            self.node_allocator.deallocate(node_alloc_id).unwrap();
            self.leaf_allocator.deallocate(leaf_alloc_id).unwrap();
        }
        let node_allocation = self
            .node_allocator
            .allocate(max_node_buffer_size_in_bytes)
            .unwrap();
        let leaf_allocation = self
            .leaf_allocator
            .allocate(max_leaf_buffer_size_in_bytes)
            .unwrap();

        self.chunk_offset_allocation_table
            .insert(atlas_offset, (node_allocation.id, leaf_allocation.id));
        (node_allocation.offset, leaf_allocation.offset)
    }

    fn confirm_allocation_of_chunk(
        &mut self,
        confirmed_node_buffer_size_in_bytes: u64,
        confirmed_leaf_buffer_size_in_bytes: u64,
        atlas_offset: UVec3,
    ) {
        let (node_alloc_id, leaf_alloc_id) = self
            .chunk_offset_allocation_table
            .get(&atlas_offset)
            .expect("Chunk not found in allocation table");

        self.node_allocator
            .resize(*node_alloc_id, confirmed_node_buffer_size_in_bytes)
            .unwrap();
        self.leaf_allocator
            .resize(*leaf_alloc_id, confirmed_leaf_buffer_size_in_bytes)
            .unwrap();
    }
}

/// Returns true if `n` is a power of four (1, 4, 16, 64, …).
///
/// Uses two bit-tricks:
/// 1. `n & (n - 1) == 0` ensures `n` is a power of two (only one bit set).
/// 2. `0x5555_5555` has 1s in all even bit positions (0,2,4,…).
///    Masking with it ensures the single bit of `n` is in an even position.
fn is_power_of_four(n: u32) -> bool {
    n != 0
        && (n & (n - 1)) == 0         // power of two?
        && (n & 0x5555_5555) != 0 // bit in an even position?
}

fn log_4(n: u32) -> u32 {
    // trailing_zeros gives 2*k, so divide by 2:
    n.trailing_zeros() / 2
}

fn get_level(contree_dim: UVec3) -> u32 {
    log_4(contree_dim.x) + 1
}
