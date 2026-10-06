//! Complete tree surface preparation/publication. Failed preparation never alters
//! resident buffers, topology, bindings, attachments, or pose history.
use super::{
    raster_tree::{RasterTreeGeometry, RasterTreeMesh},
    tree_lookup::{build_tree_lookup, TREE_PUBLICATION_BUDGET_BYTES},
    tree_scene::{TreeAttachment, TreeScene},
    tree_surface_resources::{TreeBufferLayout, TreeSurfaceCounts, TreeSurfaceResources},
    Tracer,
};
use anyhow::{ensure, Context, Result};
use glam::Vec3;
use re_flora_vkn::{vk, Buffer, BufferUsage, MemoryLocation, Resource};

pub(crate) struct PreparedRasterTreePublication {
    geometry: RasterTreeGeometry,
    resources: TreeSurfaceResources,
    descriptors: super::pipeline_builder::PreparedTreeResourceDescriptors,
}

impl Tracer {
    /// Pure validation and new allocations/uploads only. Caller quiesces previous
    /// readers before committing; an error leaves the resident publication intact.
    pub(crate) fn prepare_static_raster_trees(
        &self,
        mesh: &RasterTreeMesh,
        cells: &[[u32; 4]],
        attachments: Vec<TreeAttachment>,
    ) -> Result<Option<PreparedRasterTreePublication>> {
        if self.raster_trees.publication_valid
            && self.raster_trees.rest_mesh.same_surface(mesh)
            && self.raster_trees.attachments == attachments
        {
            return Ok(None);
        }
        ensure!(
            cells.len() == mesh.cell_vertex_indices.len(),
            "tree surface lookup topology mismatch"
        );
        let skin = mesh.gpu_skin()?;
        let positions: Vec<Vec3> = mesh.rest_positions().collect();
        let scene = TreeScene::new(&mesh.indices, &positions)?;
        let refit = scene.refit_schedule();
        let rest_entries: Vec<_> = mesh
            .solid_cells
            .iter()
            .map(|&p| [p[0], p[1], p[2], 1])
            .collect();
        let rest_cells = build_tree_lookup(&rest_entries)?.table;
        let attachment_entries: Vec<_> = attachments
            .iter()
            .enumerate()
            .map(|(i, attachment)| {
                let p = attachment.anchor.to_array();
                Ok([p[0], p[1], p[2], u32::try_from(i + 1)?])
            })
            .collect::<Result<_>>()?;
        let keys = build_tree_lookup(&attachment_entries)?.table;
        let properties = unsafe {
            self.vulkan_ctx
                .instance()
                .as_raw()
                .get_physical_device_properties(self.vulkan_ctx.physical_device().as_raw())
        };
        let layout = TreeBufferLayout::new(
            TreeSurfaceCounts {
                vertices: mesh.vertices.len(),
                primitives: scene.primitives.len(),
                nodes: scene.nodes.len(),
                bones: skin.branches.len(),
                attachments: attachments.len(),
                surface_lookup: cells.len(),
                solid_lookup: rest_cells.len(),
                attachment_lookup: keys.len(),
            },
            properties.limits.max_storage_buffer_range as usize,
        )?;
        let index_count = u32::try_from(mesh.indices.len())?;
        let index_bytes = mesh
            .indices
            .len()
            .checked_mul(4)
            .context("tree index byte size overflow")?
            .max(4);
        ensure!(
            layout
                .bytes
                .checked_add(index_bytes)
                .is_some_and(|n| n <= TREE_PUBLICATION_BUDGET_BYTES),
            "complete tree publication exceeds memory budget"
        );
        let resident_bytes =
            self.resources.tree.bytes + self.raster_trees.indices.get_size_bytes() as usize;
        ensure!(
            resident_bytes
                .checked_add(layout.bytes)
                .and_then(|n| n.checked_add(index_bytes))
                .is_some_and(|n| n <= 2 * TREE_PUBLICATION_BUDGET_BYTES),
            "tree publication peak memory budget exceeded"
        );
        let resources = TreeSurfaceResources::allocate(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            layout,
        )?;
        let indices = Buffer::try_new_sized(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            index_bytes as u64,
        )?;
        if index_count > 0 {
            indices.fill(&mesh.indices)?;
        }
        resources.tree_scene_info.fill(&[[
            0u32,
            scene.nodes.len() as u32,
            0,
            skin.bindings.len() as u32,
        ]])?;
        resources.raster_tree_cells.fill(cells)?;
        resources.tree_scene_rest_cells.fill(&rest_cells)?;
        resources
            .tree_scene_cell_vertices
            .fill(&mesh.cell_vertex_indices)?;
        resources.tree_attachment_keys.fill(&keys)?;
        let mut palette: Vec<[f32; 4]> = Vec::with_capacity((skin.branches.len() + 1) * 2);
        for _ in 0..=skin.branches.len() {
            palette.extend([[0., 0., 0., 1.], [0.; 4]]);
        }
        resources.tree_skin_poses.fill(&palette)?;
        if !skin.bindings.is_empty() {
            resources.tree_skin_rest.fill(&skin.rest)?;
            resources.tree_skin_bindings.fill(&skin.bindings)?;
        }
        if !refit.records.is_empty() {
            resources.tree_refit_order.fill(&refit.records)?;
        }
        if !scene.primitives.is_empty() {
            resources.tree_scene_primitives.fill(&scene.primitives)?;
        }
        let geometry = RasterTreeGeometry {
            skin,
            indices: Resource::new(indices),
            index_count,
            publication_valid: true,
            source: Default::default(),
            enabled: true,
            color_draws: 0,
            rest_mesh: mesh.clone(),
            scene,
            refit,
            attachments,
            attachment_poses: Vec::new(),
            previous_attachment_poses: Vec::new(),
            attachment_dt: 0.,
            posed_surface: None,
        };
        self.descriptor_generation
            .checked_add(1)
            .context("tree descriptor generation overflow")?;
        let descriptors = self
            .pipeline_topology
            .prepare_tree_resource_descriptors(&resources)?;
        Ok(Some(PreparedRasterTreePublication {
            geometry,
            resources,
            descriptors,
        }))
    }

    /// Infallible, quiesced swap. There is no partial resident upload or fallible
    /// operation after the first mutation; unchanged publications retain pose history.
    pub(crate) fn commit_static_raster_trees(&mut self, prepared: PreparedRasterTreePublication) {
        let PreparedRasterTreePublication {
            mut geometry,
            resources,
            descriptors,
        } = prepared;
        let generation = self.next_descriptor_generation();
        self.pipeline_topology
            .publish_tree_resource_descriptors(descriptors, generation);
        geometry.source = std::mem::take(&mut self.raster_trees.source);
        geometry.color_draws = self.raster_trees.color_draws;
        let [fallback, transition, reliable] = geometry.rest_mesh.confidence_counts();
        log::info!("[TREE][NORMAL_CONFIDENCE] fallback={fallback} transition={transition} reliable={reliable} single_voxel_cross_sections={} rest_fingerprint={:016x}",
            geometry.rest_mesh.single_voxel_cross_sections(), geometry.rest_mesh.rest_fingerprint());
        log::info!("[TREE][RESOURCES] vertices={} triangles={} nodes={} surface_lookup={} solid_lookup={} attachments={} bytes={}",
            geometry.rest_mesh.vertices.len(), geometry.scene.primitives.len(), geometry.scene.nodes.len(),
            resources.raster_tree_cells.get_size_bytes()/16, resources.tree_scene_rest_cells.get_size_bytes()/16,
            geometry.attachments.len(), resources.bytes);
        self.resources.tree = resources;
        self.raster_trees = geometry;
        self.invalidate_local_direct_sun_shadow_histories();
    }
}
