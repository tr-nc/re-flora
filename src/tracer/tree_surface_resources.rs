//! One complete, fallibly prepared tree resource set. No resident buffer is mutated
//! until preparation succeeds; a quiesced publication swaps this set as one value.
use super::tree_lookup::{MIN_TREE_LOOKUP_CAPACITY, TREE_PUBLICATION_BUDGET_BYTES};
use anyhow::{ensure, Context, Result};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation, Resource, VulkanContext,
};
use resource_container_derive::ResourceContainer;

#[derive(Clone, Copy, Debug, Default)]
pub struct TreeSurfaceCounts {
    pub vertices: usize,
    pub primitives: usize,
    pub nodes: usize,
    pub bones: usize,
    pub attachments: usize,
    pub surface_lookup: usize,
    pub solid_lookup: usize,
    pub attachment_lookup: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct TreeBufferLayout {
    sizes: [usize; 14],
    pub bytes: usize,
}

fn array_bytes(count: usize, stride: usize) -> Result<usize> {
    count
        .max(1)
        .checked_next_power_of_two()
        .and_then(|n| n.checked_mul(stride))
        .context("tree resource size overflow")
}

impl TreeBufferLayout {
    pub fn new(counts: TreeSurfaceCounts, max_storage_buffer_bytes: usize) -> Result<Self> {
        for count in [
            counts.vertices,
            counts.primitives,
            counts.nodes,
            counts.bones,
            counts.attachments,
        ] {
            ensure!(count < u32::MAX as usize, "tree resource index exceeds u32");
        }
        let lookup = |count: usize, stride: usize| -> Result<usize> {
            let count = count.max(MIN_TREE_LOOKUP_CAPACITY);
            ensure!(
                count.is_power_of_two(),
                "tree lookup size must be a power of two"
            );
            count
                .checked_mul(stride)
                .context("tree lookup byte size overflow")
        };
        let sizes = [
            16,
            array_bytes(counts.vertices, 48)?,
            array_bytes(counts.vertices, 16)?,
            array_bytes(
                counts
                    .bones
                    .checked_add(1)
                    .context("tree palette overflow")?,
                32,
            )?,
            array_bytes(counts.nodes, 16)?,
            lookup(counts.solid_lookup, 16)?,
            lookup(counts.attachment_lookup, 16)?,
            array_bytes(counts.attachments, 48)?,
            array_bytes(counts.nodes, 32)?,
            array_bytes(counts.primitives, 16)?,
            array_bytes(counts.vertices, 32)?,
            lookup(counts.surface_lookup, 4)?,
            lookup(counts.surface_lookup, 16)?,
            lookup(counts.surface_lookup, 16)?,
        ];
        let bytes = sizes
            .iter()
            .try_fold(0usize, |sum, &n| sum.checked_add(n))
            .context("tree publication byte size overflow")?;
        ensure!(
            sizes.iter().all(|&n| n <= max_storage_buffer_bytes),
            "tree resource exceeds Vulkan maxStorageBufferRange ({max_storage_buffer_bytes} bytes)"
        );
        ensure!(
            bytes <= TREE_PUBLICATION_BUDGET_BYTES,
            "tree resources require {bytes} bytes, exceeding the {} byte publication budget",
            TREE_PUBLICATION_BUDGET_BYTES
        );
        Ok(Self { sizes, bytes })
    }

    pub(super) fn validate_complete_budget(
        &self,
        resident: usize,
        indices: usize,
        staging: usize,
    ) -> Result<()> {
        ensure!(
            self.bytes
                .checked_add(indices)
                .is_some_and(|n| n <= TREE_PUBLICATION_BUDGET_BYTES),
            "complete tree publication exceeds memory budget"
        );
        ensure!(
            resident
                .checked_add(self.bytes)
                .and_then(|n| n.checked_add(indices))
                .and_then(|n| n.checked_add(staging))
                .is_some_and(|n| n <= 2 * TREE_PUBLICATION_BUDGET_BYTES),
            "tree publication peak memory budget exceeded (including temporary staging)"
        );
        Ok(())
    }
}

#[derive(ResourceContainer)]
pub struct TreeSurfaceResources {
    pub tree_scene_info: Resource<Buffer>,
    pub tree_skin_rest: Resource<Buffer>,
    pub tree_skin_bindings: Resource<Buffer>,
    pub tree_skin_poses: Resource<Buffer>,
    pub tree_refit_order: Resource<Buffer>,
    pub tree_scene_rest_cells: Resource<Buffer>,
    pub tree_attachment_keys: Resource<Buffer>,
    pub tree_attachment_poses: Resource<Buffer>,
    pub tree_scene_nodes: Resource<Buffer>,
    pub tree_scene_primitives: Resource<Buffer>,
    pub tree_scene_vertices: Resource<Buffer>,
    pub tree_scene_cell_vertices: Resource<Buffer>,
    pub raster_tree_cells: Resource<Buffer>,
    pub raster_tree_light_cache: Resource<Buffer>,
    pub bytes: usize,
}

impl TreeSurfaceResources {
    pub fn empty(context: &VulkanContext, allocator: Allocator) -> Result<Self> {
        let layout = TreeBufferLayout::new(TreeSurfaceCounts::default(), usize::MAX)?;
        let resources = Self::allocate(context.device().clone(), allocator.clone(), layout)?;
        resources.tree_scene_info.fill(&[[0u32; 4]])?;
        let zeros = vec![[0u32; 4]; MIN_TREE_LOOKUP_CAPACITY];
        let data = bytemuck::cast_slice(&zeros);
        super::tree_upload::upload(
            context,
            &allocator,
            &[
                (&resources.raster_tree_cells, data),
                (&resources.tree_scene_rest_cells, data),
                (&resources.tree_attachment_keys, data),
            ],
        )?;
        Ok(resources)
    }

    pub fn allocate(
        device: Device,
        allocator: Allocator,
        layout: TreeBufferLayout,
    ) -> Result<Self> {
        let host = |index: usize| -> Result<Resource<Buffer>> {
            Ok(Resource::new(Buffer::try_new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::CpuToGpu,
                layout.sizes[index] as u64,
            )?))
        };
        let gpu = |index: usize| -> Result<Resource<Buffer>> {
            Ok(Resource::new(Buffer::try_new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
                ),
                MemoryLocation::GpuOnly,
                layout.sizes[index] as u64,
            )?))
        };
        Ok(Self {
            tree_scene_info: host(0)?,
            tree_skin_rest: gpu(1)?,
            tree_skin_bindings: gpu(2)?,
            tree_skin_poses: host(3)?,
            tree_refit_order: gpu(4)?,
            tree_scene_rest_cells: gpu(5)?,
            tree_attachment_keys: gpu(6)?,
            tree_attachment_poses: host(7)?,
            tree_scene_nodes: gpu(8)?,
            tree_scene_primitives: gpu(9)?,
            tree_scene_vertices: gpu(10)?,
            tree_scene_cell_vertices: gpu(11)?,
            raster_tree_cells: gpu(12)?,
            raster_tree_light_cache: gpu(13)?,
            bytes: layout.bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_handles_six_tree_workload_without_an_old_vertex_cap() {
        let layout = TreeBufferLayout::new(
            TreeSurfaceCounts {
                vertices: 146904,
                primitives: 101120,
                nodes: 202239,
                bones: 8000,
                attachments: 2000,
                surface_lookup: 65536,
                solid_lookup: 131072,
                attachment_lookup: 8192,
            },
            128 * 1024 * 1024,
        )
        .unwrap();
        assert!(layout.sizes[1] >= 146904 * 48);
        assert!(layout.sizes[8] >= 202239 * 32);
        assert!(layout.bytes < TREE_PUBLICATION_BUDGET_BYTES);
    }
    #[test]
    fn temporary_staging_is_included_in_the_old_plus_new_peak_budget() {
        let layout = TreeBufferLayout::new(TreeSurfaceCounts::default(), usize::MAX).unwrap();
        let resident = 2 * TREE_PUBLICATION_BUDGET_BYTES - layout.bytes - 4;
        layout.validate_complete_budget(resident, 4, 0).unwrap();
        assert!(layout.validate_complete_budget(resident, 4, 4).is_err());
        assert!(layout.validate_complete_budget(usize::MAX, 4, 4).is_err());
        assert!(layout
            .validate_complete_budget(0, TREE_PUBLICATION_BUDGET_BYTES, 0)
            .is_err());
    }
    #[test]
    fn device_and_total_budgets_are_preflighted_before_allocation() {
        assert!(TreeBufferLayout::new(
            TreeSurfaceCounts {
                vertices: 10000,
                ..Default::default()
            },
            1024
        )
        .is_err());
        assert!(TreeBufferLayout::new(
            TreeSurfaceCounts {
                vertices: usize::MAX,
                ..Default::default()
            },
            usize::MAX
        )
        .is_err());
        assert!(TreeBufferLayout::new(
            TreeSurfaceCounts {
                vertices: 1 << 24,
                ..Default::default()
            },
            usize::MAX
        )
        .is_err());
        assert!(TreeBufferLayout::new(
            TreeSurfaceCounts {
                surface_lookup: 100,
                ..Default::default()
            },
            usize::MAX
        )
        .is_err());
    }
}
