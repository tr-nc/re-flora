mod resources;
pub use resources::*;

use super::SurfaceResources;
use crate::generated::gpu_structs::ContreeBuildInfo;
use crate::util::AllocationStrategy;
use crate::util::FirstFitAllocator;
use crate::util::ShaderCompiler;
use crate::vkn::Allocator;
use crate::vkn::Buffer;
use crate::vkn::BufferUsage;
use crate::vkn::CommandBuffer;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::Extent3D;
use crate::vkn::MemoryBarrier;
use crate::vkn::PipelineBarrier;
use crate::vkn::ShaderModule;
use crate::vkn::VulkanContext;
use crate::vkn::execute_one_time_command_with_fence;
use anyhow::Result;
use ash::vk;
use glam::{UVec3, Vec2, Vec3};
use std::collections::HashMap;

const SIZE_OF_NODE_ELEMENT: u64 = 3 * std::mem::size_of::<u32>() as u64;
const SIZE_OF_LEAF_ELEMENT: u64 = std::mem::size_of::<u32>() as u64;
const MAX_NODE_BUFFER_SIZE_IN_BYTES: u64 = 10 * 1024 * 1024;
const MAX_LEAF_BUFFER_SIZE_IN_BYTES: u64 = 10 * 1024 * 1024;

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
    cpu_bridge_buffers: CpuChunkBridgeBuffers,

    voxel_dim_per_chunk: UVec3,
    cpu_chunk_zero_cache: Option<CpuChunkCache>,
}

#[derive(Clone, Copy, Debug)]
struct CpuContreeNode {
    packed_0: u32,
    child_mask_lo: u32,
    child_mask_hi: u32,
}

#[derive(Clone, Debug)]
struct CpuChunkCache {
    atlas_offset: UVec3,
    nodes: Vec<CpuContreeNode>,
    leaves: Vec<u32>,
}

struct CpuChunkBridgeBuffers {
    node_readback: Buffer,
    leaf_readback: Buffer,
}

impl ContreeBuilder {
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

        let fixed_pool = DescriptorPool::new(device).unwrap();

        let contree_buffer_setup_ppl =
            ComputePipeline::new(device, &contree_buffer_setup_sm, &fixed_pool, &[&resources]);
        let contree_leaf_write_ppl = ComputePipeline::new(
            device,
            &contree_leaf_write_sm,
            &fixed_pool,
            &[&resources, surfacer_resources],
        );
        let contree_tree_write_ppl =
            ComputePipeline::new(device, &contree_tree_write_sm, &fixed_pool, &[&resources]);
        let contree_buffer_update_ppl = ComputePipeline::new(
            device,
            &contree_buffer_update_sm,
            &fixed_pool,
            &[&resources],
        );
        let contree_last_buffer_update_ppl = ComputePipeline::new(
            device,
            &contree_last_buffer_update_sm,
            &fixed_pool,
            &[&resources],
        );
        let contree_concat_ppl =
            ComputePipeline::new(device, &contree_concat_sm, &fixed_pool, &[&resources]);

        // // --- Descriptor Sets ---
        // let alloc_set_fn = |ppl: &ComputePipeline| -> DescriptorSet {
        //     fixed_pool
        //         .allocate_set(&ppl.get_layout().get_descriptor_set_layouts()[&0])
        //         .unwrap()
        // };

        // let contree_buffer_setup_ds = alloc_set_fn(&contree_buffer_setup_ppl);
        // let contree_leaf_write_ds = alloc_set_fn(&contree_leaf_write_ppl);
        // let contree_tree_write_ds = alloc_set_fn(&contree_tree_write_ppl);
        // let contree_buffer_update_ds = alloc_set_fn(&contree_buffer_update_ppl);
        // let contree_last_buffer_update_ds = alloc_set_fn(&contree_last_buffer_update_ppl);
        // let contree_concat_ds = alloc_set_fn(&contree_concat_ppl);

        // Self::update_contree_buffer_setup_ds(&contree_buffer_setup_ds, &resources);
        // Self::update_contree_leaf_write_ds(&contree_leaf_write_ds, &resources, surfacer_resources);
        // Self::update_contree_tree_write_ds(&contree_tree_write_ds, &resources);
        // Self::update_contree_buffer_update_ds(&contree_buffer_update_ds, &resources);
        // Self::update_contree_last_buffer_update_ds(&contree_last_buffer_update_ds, &resources);
        // Self::update_contree_concat_ds(&contree_concat_ds, &resources);

        // contree_buffer_setup_ppl.set_descriptor_sets(vec![contree_buffer_setup_ds]);
        // contree_leaf_write_ppl.set_descriptor_sets(vec![contree_leaf_write_ds]);
        // contree_tree_write_ppl.set_descriptor_sets(vec![contree_tree_write_ds]);
        // contree_buffer_update_ppl.set_descriptor_sets(vec![contree_buffer_update_ds]);
        // contree_last_buffer_update_ppl.set_descriptor_sets(vec![contree_last_buffer_update_ds]);
        // contree_concat_ppl.set_descriptor_sets(vec![contree_concat_ds]);

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
        let cpu_bridge_buffers = CpuChunkBridgeBuffers::new(device.clone(), allocator.clone());

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
            cpu_bridge_buffers,
            voxel_dim_per_chunk,
            cpu_chunk_zero_cache: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
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
        let raw_data = resources.contree_build_result.read_back().unwrap();
        assert!(
            raw_data.len() >= 8,
            "contree_build_result buffer too small: expected at least 8 bytes, got {}",
            raw_data.len()
        );
        let node_len = u32::from_ne_bytes(raw_data[0..4].try_into().unwrap()) as u64;
        let leaf_len = u32::from_ne_bytes(raw_data[4..8].try_into().unwrap()) as u64;
        let leaf_size_in_bytes = leaf_len * SIZE_OF_LEAF_ELEMENT;
        let node_size_in_bytes = node_len * SIZE_OF_NODE_ELEMENT;

        (node_size_in_bytes, leaf_size_in_bytes)
    }

    pub fn get_resources(&self) -> &ContreeBuilderResources {
        &self.resources
    }

    pub fn debug_query_chunk_zero_cpu_ray(&self, origin: Vec3, direction: Vec3) -> Option<Vec3> {
        let cache = self.cpu_chunk_zero_cache.as_ref()?;
        self.query_cached_chunk_cpu_ray(cache, origin, direction)
    }

    fn query_cached_chunk_cpu_ray(
        &self,
        cache: &CpuChunkCache,
        origin: Vec3,
        direction: Vec3,
    ) -> Option<Vec3> {
        if direction.length_squared() <= f32::EPSILON || cache.nodes.is_empty() {
            return None;
        }

        let local_origin = origin - cache.atlas_offset.as_vec3() + Vec3::ONE;
        let local_dir = direction.normalize();
        let local_hit = march_contree_cpu(local_origin, local_dir, &cache.nodes, &cache.leaves)?;
        Some(local_hit + cache.atlas_offset.as_vec3() - Vec3::ONE)
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
            contree_build_info.fill_uniform(&ContreeBuildInfo {
                dim: contree_dim.x,
                max_level,
                node_write_offset,
                leaf_write_offset,
            })
        }
    }

    /// Returns: (node_alloc_offset, leaf_alloc_offset)
    pub fn build_and_alloc(&mut self, atlas_offset: UVec3) -> Result<Option<(u64, u64)>> {
        let atlas_dim = self.voxel_dim_per_chunk;

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

        if atlas_offset == UVec3::ZERO {
            self.cpu_chunk_zero_cache = Some(self.read_back_chunk_cpu_cache(
                atlas_offset,
                node_alloc_offset,
                leaf_alloc_offset,
                confirmed_node_buffer_size_in_bytes,
                confirmed_leaf_buffer_size_in_bytes,
            )?);
        }

        Ok(Some((node_alloc_offset, leaf_alloc_offset)))
    }

    fn read_back_chunk_cpu_cache(
        &self,
        atlas_offset: UVec3,
        node_alloc_offset: u64,
        leaf_alloc_offset: u64,
        node_size_in_bytes: u64,
        leaf_size_in_bytes: u64,
    ) -> Result<CpuChunkCache> {
        assert!(node_size_in_bytes <= MAX_NODE_BUFFER_SIZE_IN_BYTES);
        assert!(leaf_size_in_bytes <= MAX_LEAF_BUFFER_SIZE_IN_BYTES);

        execute_one_time_command_with_fence(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.resources.contree_node_data.record_copy_to_buffer(
                    cmdbuf,
                    &self.cpu_bridge_buffers.node_readback,
                    node_size_in_bytes,
                    node_alloc_offset * SIZE_OF_NODE_ELEMENT,
                    0,
                );
                self.resources.contree_leaf_data.record_copy_to_buffer(
                    cmdbuf,
                    &self.cpu_bridge_buffers.leaf_readback,
                    leaf_size_in_bytes,
                    leaf_alloc_offset * SIZE_OF_LEAF_ELEMENT,
                    0,
                );
            },
        );

        let node_bytes = self.cpu_bridge_buffers.node_readback.read_back()?;
        let leaf_bytes = self.cpu_bridge_buffers.leaf_readback.read_back()?;
        let node_bytes = &node_bytes[..node_size_in_bytes as usize];
        let leaf_bytes = &leaf_bytes[..leaf_size_in_bytes as usize];

        let nodes = node_bytes
            .chunks_exact(SIZE_OF_NODE_ELEMENT as usize)
            .map(|chunk| CpuContreeNode {
                packed_0: u32::from_ne_bytes(chunk[0..4].try_into().unwrap()),
                child_mask_lo: u32::from_ne_bytes(chunk[4..8].try_into().unwrap()),
                child_mask_hi: u32::from_ne_bytes(chunk[8..12].try_into().unwrap()),
            })
            .collect();
        let leaves = leaf_bytes
            .chunks_exact(SIZE_OF_LEAF_ELEMENT as usize)
            .map(|chunk| u32::from_ne_bytes(chunk[0..4].try_into().unwrap()))
            .collect();

        Ok(CpuChunkCache {
            atlas_offset,
            nodes,
            leaves,
        })
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

impl CpuChunkBridgeBuffers {
    fn new(device: crate::vkn::Device, allocator: Allocator) -> Self {
        Self {
            node_readback: Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
                gpu_allocator::MemoryLocation::GpuToCpu,
                MAX_NODE_BUFFER_SIZE_IN_BYTES,
            ),
            leaf_readback: Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
                gpu_allocator::MemoryLocation::GpuToCpu,
                MAX_LEAF_BUFFER_SIZE_IN_BYTES,
            ),
        }
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

fn march_contree_cpu(
    origin: Vec3,
    dir: Vec3,
    nodes: &[CpuContreeNode],
    leaves: &[u32],
) -> Option<Vec3> {
    if nodes.is_empty() {
        return None;
    }

    let mut stack = [0u32; 11];
    let mut scale_exp = 21i32;
    let mut node_idx = 0u32;
    let mut node = *nodes.get(node_idx as usize)?;

    let slab = slabs(Vec3::ONE, Vec3::splat(1.999_999_9), origin, reciprocal(dir));
    if slab.x > slab.y || slab.y < 0.0 {
        return None;
    }
    let origin = origin + dir * slab.x.max(0.0);

    let mut mirror_mask = 0u32;
    if dir.x > 0.0 {
        mirror_mask |= 3u32 << 0;
    }
    if dir.y > 0.0 {
        mirror_mask |= 3u32 << 4;
    }
    if dir.z > 0.0 {
        mirror_mask |= 3u32 << 2;
    }

    let origin = get_mirrored_pos(origin, dir, true);
    let mut pos = origin.clamp(Vec3::ONE, Vec3::splat(1.999_999_9));
    let inv_dir = -reciprocal(dir.abs());

    for _ in 0..1024 {
        let mut child_idx = (get_node_cell_index(pos, scale_exp)? as u32) ^ mirror_mask;

        while child_mask_test(node, child_idx) && !is_leaf(node) {
            stack[(scale_exp >> 1) as usize] = node_idx;

            let bits = child_mask_bitcount_below(node, child_idx);
            node_idx = (node.packed_0 >> 1) + bits;
            node = *nodes.get(node_idx as usize)?;

            scale_exp -= 2;
            child_idx = (get_node_cell_index(pos, scale_exp)? as u32) ^ mirror_mask;
        }

        if child_mask_test(node, child_idx) && is_leaf(node) {
            let pos = get_mirrored_pos(pos, dir, false);
            let child_idx = get_node_cell_index(pos, scale_exp)? as u32;
            let bits = child_mask_bitcount_below(node, child_idx);
            let voxel_addr = ((node.packed_0 >> 1) + bits) as usize;
            if voxel_addr < leaves.len() {
                return Some(pos);
            }
            return None;
        }

        let mut adv_scale_exp = scale_exp;
        let shifted_idx = child_idx & 0x2A;
        let has_neighbor = if shifted_idx < 32 {
            ((node.child_mask_lo >> shifted_idx) & 0x0033_0033) != 0
        } else {
            ((node.child_mask_hi >> (shifted_idx - 32)) & 0x0033_0033) != 0
        };
        if !has_neighbor {
            adv_scale_exp += 1;
        }

        let cell_min = floor_scale(pos, adv_scale_exp);
        let side_dist = (cell_min - origin) * inv_dir;
        let tmax = side_dist.x.min(side_dist.y.min(side_dist.z));

        let side_mask = [tmax >= side_dist.x, tmax >= side_dist.y, tmax >= side_dist.z];
        let base = [
            cell_min.x.to_bits() as i32,
            cell_min.y.to_bits() as i32,
            cell_min.z.to_bits() as i32,
        ];
        let off = (1 << adv_scale_exp) - 1;
        let neighbor_max = [
            base[0] + if side_mask[0] { -1 } else { off },
            base[1] + if side_mask[1] { -1 } else { off },
            base[2] + if side_mask[2] { -1 } else { off },
        ];

        pos = (origin - dir.abs() * tmax).min(Vec3::new(
            f32::from_bits(neighbor_max[0] as u32),
            f32::from_bits(neighbor_max[1] as u32),
            f32::from_bits(neighbor_max[2] as u32),
        ));

        let combined = ((pos.x.to_bits() ^ cell_min.x.to_bits())
            | (pos.y.to_bits() ^ cell_min.y.to_bits())
            | (pos.z.to_bits() ^ cell_min.z.to_bits()))
            & 0xFFAA_AAAA;
        let diff_exp = find_msb(combined);
        if diff_exp > scale_exp {
            scale_exp = diff_exp;
            if diff_exp > 21 {
                break;
            }
            node_idx = stack[(scale_exp >> 1) as usize];
            node = *nodes.get(node_idx as usize)?;
        }
    }

    None
}

fn slabs(min_bound: Vec3, max_bound: Vec3, origin: Vec3, inv_dir: Vec3) -> Vec2 {
    let t0 = (min_bound - origin) * inv_dir;
    let t1 = (max_bound - origin) * inv_dir;
    let tmin = t0.min(t1);
    let tmax = t0.max(t1);
    Vec2::new(tmin.max_element(), tmax.min_element())
}

fn reciprocal(v: Vec3) -> Vec3 {
    Vec3::new(1.0 / v.x, 1.0 / v.y, 1.0 / v.z)
}

fn get_mirrored_pos(pos: Vec3, dir: Vec3, range_check: bool) -> Vec3 {
    let mirrored = Vec3::new(
        f32::from_bits(pos.x.to_bits() ^ 0x007F_FFFF),
        f32::from_bits(pos.y.to_bits() ^ 0x007F_FFFF),
        f32::from_bits(pos.z.to_bits() ^ 0x007F_FFFF),
    );

    let mirrored = if range_check
        && (pos.cmplt(Vec3::ONE).any() || pos.cmpge(Vec3::splat(2.0)).any())
    {
        Vec3::splat(3.0) - pos
    } else {
        mirrored
    };

    Vec3::new(
        if dir.x > 0.0 { mirrored.x } else { pos.x },
        if dir.y > 0.0 { mirrored.y } else { pos.y },
        if dir.z > 0.0 { mirrored.z } else { pos.z },
    )
}

fn get_node_cell_index(pos: Vec3, scale_exp: i32) -> Option<i32> {
    let shift = u32::try_from(scale_exp).ok()?;
    let px = (pos.x.to_bits() >> shift) & 3;
    let py = (pos.y.to_bits() >> shift) & 3;
    let pz = (pos.z.to_bits() >> shift) & 3;
    Some((px + pz * 4 + py * 16) as i32)
}

fn floor_scale(pos: Vec3, scale_exp: i32) -> Vec3 {
    let mask = !0u32 << (scale_exp as u32);
    Vec3::new(
        f32::from_bits(pos.x.to_bits() & mask),
        f32::from_bits(pos.y.to_bits() & mask),
        f32::from_bits(pos.z.to_bits() & mask),
    )
}

fn is_leaf(node: CpuContreeNode) -> bool {
    (node.packed_0 & 1) != 0
}

fn child_mask_test(node: CpuContreeNode, idx: u32) -> bool {
    if idx < 32 {
        (node.child_mask_lo & (1 << idx)) != 0
    } else {
        (node.child_mask_hi & (1 << (idx - 32))) != 0
    }
}

fn child_mask_bitcount_below(node: CpuContreeNode, idx: u32) -> u32 {
    if idx < 32 {
        (node.child_mask_lo & ((1 << idx) - 1)).count_ones()
    } else {
        node.child_mask_lo.count_ones() + (node.child_mask_hi & ((1 << (idx - 32)) - 1)).count_ones()
    }
}

fn find_msb(value: u32) -> i32 {
    if value == 0 {
        -1
    } else {
        31 - value.leading_zeros() as i32
    }
}
