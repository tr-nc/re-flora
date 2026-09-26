//! Attached-leaf adapter for the existing shared model pipeline. It consumes
//! live socket indices and GPU wind poses, never creates CPU particles or reads
//! their poses back. Allocation/batching is owned by model_pixel_tiles.
use super::*;

pub(super) struct PreparedLeafModels {
    pub count: u32,
    pub descriptors: PreparedDrawDescriptors,
}

impl Tracer {
    pub(super) fn prepare_leaf_models(
        &mut self,
        cmd: &CommandBuffer,
        frame: usize,
        batch: TreeFoliageBatch,
        tree: &TreeLeavesInstance,
        time: f32,
        colors: FloraHeightColorTables,
    ) -> Result<Vec<PreparedLeafModels>> {
        // The bounded continuous numerical oracle tests particle model sources;
        // it does not allocate the production canonical bank for an entire tree.
        if butterfly_mesh::native_review() {
            return Ok(Vec::new());
        }
        let sources: Vec<u32> = tree.resources.leaf_state.live_indices().collect();
        let resolution = self.leaf_models.resolution;
        let (_, batches) =
            model_pixel_tiles::pack_tiles(sources.iter().map(|_| (resolution, true)));
        let mut prepared = Vec::with_capacity(batches.len());
        for (i, part) in batches.iter().enumerate() {
            let key = |name| (name, batch.tree_id(), i as u32);
            let device = self.vulkan_ctx.device().clone();
            let allocator = self.allocator.clone();
            let source = self.model_pixel_tiles.upload_stream(
                frame,
                key("leaf.sources"),
                bytemuck::cast_slice(
                    &sources[part.first as usize..(part.first + part.count) as usize],
                ),
                device.clone(),
                allocator.clone(),
            )?;
            let models = self.model_pixel_tiles.get_stream(
                frame,
                key("leaf.models"),
                part.count as usize * 7,
                device.clone(),
                allocator.clone(),
            )?;
            let order = self.model_pixel_tiles.get_stream(
                frame,
                key("leaf.order"),
                (part.count as usize).div_ceil(4),
                device.clone(),
                allocator.clone(),
            )?;
            let tiles = self.model_pixel_tiles.get_stream(
                frame,
                key("leaf.tiles"),
                part.texels,
                device.clone(),
                allocator.clone(),
            )?;
            let light = self.model_pixel_tiles.get_stream(
                frame,
                key("leaf.light"),
                part.count as usize * 4,
                device.clone(),
                allocator.clone(),
            )?;
            let mut push =
                flora_push_constant(time, LEAF_INSTANCE_TYPE, tree.chunk_world_offset, colors);
            push.instance_ty = flora_lighting_cache_instance_ty(LEAF_INSTANCE_TYPE, part.count);
            push.response_offset = self.vegetation_response.leaf_offset(batch.tree_id());
            self.pipeline_topology
                .compute()
                .tree_leaf_model_ppl
                .record_with_descriptors(
                    cmd,
                    &[
                        (
                            "tree_leaf_instances",
                            DescriptorResource::Buffer(&tree.resources.instances_buf),
                        ),
                        (
                            "tree_leaf_state",
                            DescriptorResource::Buffer(tree.resources.leaf_state.buffer()),
                        ),
                        ("leaf_model_sources", DescriptorResource::Buffer(&source)),
                        ("leaf_model_instances", DescriptorResource::Buffer(&models)),
                        (
                            "leaf_model_draw_indices",
                            DescriptorResource::Buffer(&order),
                        ),
                        self.vegetation_response.descriptors()[0],
                        self.vegetation_response.descriptors()[1],
                    ],
                    Extent3D::new(part.count, 1, 1),
                    Some(bytemuck::bytes_of(&push)),
                )?;
            let cache = self.model_pixel_cache.frame(frame);
            let mut bindings = vec![
                (
                    "butterfly_mesh_triangles",
                    DescriptorResource::Buffer(
                        &self.resources.butterfly_mesh.butterfly_mesh_triangles,
                    ),
                ),
                (
                    "particle_model_repairs",
                    DescriptorResource::Buffer(
                        &self.resources.butterfly_mesh.particle_model_repairs,
                    ),
                ),
                (
                    "particle_model_repair_output",
                    DescriptorResource::Buffer(
                        &self.resources.butterfly_mesh.particle_model_repair_output,
                    ),
                ),
                (
                    "butterfly_mesh_instances",
                    DescriptorResource::Buffer(&models),
                ),
                ("draw_indices", DescriptorResource::Buffer(&order)),
                ("model_pixel_tiles", DescriptorResource::Buffer(&tiles)),
                ("model_object_samples", DescriptorResource::Buffer(&light)),
            ];
            bindings.extend(cache.bindings());
            let compute = &self.pipeline_topology.compute().butterfly_tile_ppl;
            compute.record_with_descriptors(
                cmd,
                &bindings,
                Extent3D::new(1, 1, part.count),
                Some(bytemuck::bytes_of(&[4u32, part.count, 0])),
            )?;
            compute.record_with_descriptors(
                cmd,
                &bindings,
                Extent3D::new(resolution, resolution, part.count),
                Some(bytemuck::bytes_of(&[3u32, part.count, 0])),
            )?;
            let pipeline = match batch.lod_state() {
                LodState::Lod0 => &self.pipeline_topology.graphics().leaves_ppl,
                LodState::Lod1 => &self.pipeline_topology.graphics().leaves_lod_ppl,
            };
            prepared.push(PreparedLeafModels {
                count: part.count,
                descriptors: pipeline.prepare_draw_descriptors(
                    cmd,
                    &[
                        (
                            "butterfly_mesh_instances",
                            DescriptorResource::Buffer(&models),
                        ),
                        ("model_pixel_tiles", DescriptorResource::Buffer(&tiles)),
                        (
                            "model_object_view_samples",
                            DescriptorResource::Buffer(&light),
                        ),
                    ],
                )?,
            });
        }
        Ok(prepared)
    }
}
