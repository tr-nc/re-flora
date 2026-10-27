//! Complete tree surface preparation/publication. Failed preparation never alters
//! resident buffers, topology, bindings, attachments, or pose history.
use super::{
    raster_tree::{RasterTreeGeometry, RasterTreeMesh},
    tree_lookup::build_tree_lookup,
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

impl PreparedRasterTreePublication {
    pub(crate) fn mesh(&self) -> &RasterTreeMesh {
        &self.geometry.rest_mesh
    }
}

impl Tracer {
    /// Pure validation and new allocations/uploads only. Caller quiesces previous
    /// readers before committing; an error leaves the resident publication intact.
    pub(crate) fn prepare_static_raster_trees<'a>(
        &self,
        mesh: &RasterTreeMesh,
        cells: &[[u32; 4]],
        attachments: Vec<TreeAttachment>,
        poses: impl Fn(u32) -> Option<&'a [crate::tree_gen::pose::BranchPose]>,
        wind_active: bool,
    ) -> Result<Option<PreparedRasterTreePublication>> {
        ensure!(
            cells.len() == mesh.cell_vertex_indices.len(),
            "tree surface lookup topology mismatch"
        );
        use crate::tree_gen::pose::BranchPose;
        let identity = BranchPose {
            rotation: glam::Quat::IDENTITY,
            translation: Vec3::ZERO,
        };
        let mut skin = mesh.gpu_skin()?;
        skin.poses.push(identity);
        for &(id, branch) in &skin.branches {
            let pose = if wind_active {
                *poses(id)
                    .and_then(|branches| branches.get(branch))
                    .context("prepared tree palette refers to missing branch")?
            } else {
                identity
            };
            ensure!(
                pose.rotation.is_finite() && pose.translation.is_finite(),
                "nonfinite prepared tree palette"
            );
            skin.poses.push(pose);
        }
        let attachment_poses = if wind_active {
            attachments
                .iter()
                .map(|attachment| {
                    attachment.pose(
                        poses(attachment.tree_id)
                            .context("prepared tree attachment refers to missing tree")?,
                    )
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            vec![identity; attachments.len()]
        };
        ensure!(
            attachment_poses
                .iter()
                .all(|p| p.rotation.is_finite() && p.translation.is_finite()),
            "nonfinite prepared tree attachment"
        );
        let same_poses = |a: &[BranchPose], b: &[BranchPose]| {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(a, b)| a.rotation == b.rotation && a.translation == b.translation)
        };
        if self.raster_trees.publication_valid
            && self.raster_trees.rest_mesh.same_surface(mesh)
            && self.raster_trees.attachments == attachments
            && self.raster_trees.gpu_wind_active == wind_active
            && (!wind_active
                || (same_poses(&skin.poses, &self.raster_trees.skin.poses)
                    && same_poses(&attachment_poses, &self.raster_trees.attachment_poses)))
        {
            return Ok(None);
        }
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
                let p = attachment.lookup_anchor().to_array();
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

        let static_data: [&[u8]; 9] = [
            bytemuck::cast_slice(&mesh.indices),
            bytemuck::cast_slice(&skin.rest),
            bytemuck::cast_slice(&skin.bindings),
            bytemuck::cast_slice(&refit.records),
            bytemuck::cast_slice(&scene.primitives),
            bytemuck::cast_slice(&rest_cells),
            bytemuck::cast_slice(&mesh.cell_vertex_indices),
            bytemuck::cast_slice(&keys),
            bytemuck::cast_slice(cells),
        ];
        let staging_bytes = super::tree_upload::byte_len(&static_data)?;
        let resident_bytes =
            self.resources.tree.bytes + self.raster_trees.indices.get_size_bytes() as usize;
        layout.validate_complete_budget(resident_bytes, index_bytes, staging_bytes)?;
        let resources = TreeSurfaceResources::allocate(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            layout,
        )?;
        let indices = Buffer::try_new_sized(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            index_bytes as u64,
        )?;
        if std::env::var_os("RE_FLORA_TREE_SCALE_DIAGNOSTIC").is_some() {
            for (name, buffer) in [
                ("skin_rest", &*resources.tree_skin_rest),
                ("skin_bindings", &*resources.tree_skin_bindings),
                ("refit_order", &*resources.tree_refit_order),
                ("primitives", &*resources.tree_scene_primitives),
                ("nodes", &*resources.tree_scene_nodes),
                ("vertices", &*resources.tree_scene_vertices),
                ("indices", &indices),
            ] {
                log::info!(
                    "[TREE][MEMORY] {name} bytes={} properties={:?}",
                    buffer.get_size_bytes(),
                    buffer.memory_properties()
                );
            }
        }
        let targets = [
            &indices,
            &*resources.tree_skin_rest,
            &*resources.tree_skin_bindings,
            &*resources.tree_refit_order,
            &*resources.tree_scene_primitives,
            &*resources.tree_scene_rest_cells,
            &*resources.tree_scene_cell_vertices,
            &*resources.tree_attachment_keys,
            &*resources.raster_tree_cells,
        ];
        let uploads: Vec<_> = targets.into_iter().zip(static_data).collect();
        super::tree_upload::upload(&self.vulkan_ctx, &self.allocator, &uploads)?;
        resources.tree_scene_info.fill(&[[
            u32::from(wind_active),
            scene.nodes.len() as u32,
            0,
            skin.bindings.len() as u32,
        ]])?;
        let palette: Vec<[f32; 4]> = skin
            .poses
            .iter()
            .flat_map(|p| [p.rotation.to_array(), p.translation.extend(0.).to_array()])
            .collect();
        resources.tree_skin_poses.fill(&palette)?;
        if !attachment_poses.is_empty() {
            let palette: Vec<[f32; 4]> = attachment_poses
                .iter()
                .zip(&attachments)
                .flat_map(|(p, a)| {
                    [
                        p.rotation.to_array(),
                        p.translation.extend(0.).to_array(),
                        a.gpu_pivot(),
                    ]
                })
                .collect();
            resources.tree_attachment_poses.fill(&palette)?;
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
            previous_attachment_poses: attachment_poses.clone(),
            attachment_poses,
            attachment_dt: 0.,
            gpu_wind_active: wind_active,
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
        self.surface_picker.invalidate_geometry();
        self.invalidate_local_direct_sun_shadow_histories();
    }
}
