//! Model pixels are published as complete compute/draw pairs. Tracer schedules
//! semantic adapters and draws their opaque results; it never recreates storage
//! identities, object-buffer sizes, tile ranges or compute/display bindings.
//!
//! FrameManager still establishes fence readiness. PipelineTopology and the VKN
//! pipelines still own descriptor/extent retirement. Shared immutable surfaces
//! feed frame-local relighting tiles; instance count never multiplies baking.
use super::{
    butterfly_mesh::{ButterflyMeshRenderer, ButterflyMeshSettings, LeafModelSettings},
    dynamic_fruit_resources::DynamicFruitRendererResources,
    model_pixel_cache::ModelPixelCache,
    model_pixel_tiles::{ModelPixelBatch, ModelPixelStorage, ParticleInputs},
    pipeline_builder::{ComputePipelines, GraphicsPipelines},
    resources::TracerResources,
    PushConstantFlora,
};
use crate::particles::ParticleSnapshot;
use anyhow::Result;
use glam::{Mat4, Vec3};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, ComputePipeline,
    DescriptorResource, Device, Extent3D, GraphicsPipeline, MemoryLocation,
    PreparedDrawDescriptors, PushConstantInfo, VulkanContext,
};
use std::sync::Arc;

/// Native pipelines for a concrete pose/material adapter.
/// This is a dependency, not a list of rendering steps returned to the host.
pub(super) struct PixelPass<'a> {
    pub compute: &'a ComputePipeline,
    pub display: &'a GraphicsPipeline,
}

struct PixelDraw {
    descriptors: PreparedDrawDescriptors,
    first: u32,
    count: u32,
    push: Option<PushConstantInfo>,
}

/// Created only after compute and descriptor preparation succeed. The same
/// pipeline, draw range, object data and (for trees) pose push survive to draw.
/// Use within the current frame, after entering its render pass.
pub(super) struct PreparedModelPixels {
    pipeline: GraphicsPipeline,
    draws: Vec<PixelDraw>,
    push: Option<PushConstantInfo>,
    instance_indices: Option<Arc<Buffer>>,
}
impl PreparedModelPixels {
    pub fn record(&self, cmdbuf: &CommandBuffer, index_count: u32) {
        if let Some(indices) = &self.instance_indices {
            cmdbuf.bind_vertex_buffers(1, &[indices]);
        }
        for draw in &self.draws {
            self.pipeline.record_indexed_with_prepared_descriptors(
                cmdbuf,
                &draw.descriptors,
                index_count,
                draw.count,
                0,
                0,
                draw.first,
                draw.push.as_ref().or(self.push.as_ref()),
            );
        }
    }
}

pub(super) struct PreparedFlowerModels {
    pixels: PreparedModelPixels,
    stems: Option<PreparedModelPixels>,
    stem_index_count: u32,
}
impl PreparedFlowerModels {
    pub fn record(&self, cmdbuf: &CommandBuffer, resources: &TracerResources) {
        if let Some(stems) = &self.stems {
            cmdbuf.bind_vertex_buffers(0, &[&resources.flower_models.flower_stem_vertices]);
            cmdbuf.bind_index_buffer_u32(&resources.flower_models.flower_stem_indices);
            stems.record(cmdbuf, self.stem_index_count);
        }
        // This is the existing shared four-corner model-display quad, not apple geometry.
        cmdbuf.bind_vertex_buffers(0, &[&resources.apple_pixel.apple_pixel_quad_vertices]);
        cmdbuf.bind_index_buffer_u32(&resources.apple_pixel.apple_pixel_quad_indices);
        self.pixels.record(cmdbuf, 6);
    }
}

pub(super) struct ModelPixelFrame {
    device: Device,
    allocator: Allocator,
    storage: ModelPixelStorage<Arc<Buffer>>,
    particles: ButterflyMeshRenderer,
    cache: ModelPixelCache,
    particle_resolutions: [u32; 2],
}
impl ModelPixelFrame {
    pub fn new(context: &VulkanContext, allocator: Allocator) -> Self {
        Self {
            device: context.device().clone(),
            cache: ModelPixelCache::new(context, allocator.clone()),
            particle_resolutions: [16, 22],
            allocator,
            storage: ModelPixelStorage::default(),
            particles: ButterflyMeshRenderer::default(),
        }
    }

    /// Called exactly where the host has already waited for this slot's fence.
    /// Transient descriptor allocation remains delegated to the native pipelines.
    pub fn begin_frame(
        &mut self,
        slot: usize,
        compute: &ComputePipelines,
        graphics: &GraphicsPipelines,
    ) {
        self.storage.begin_frame(slot);
        for pipeline in [
            &compute.butterfly_tile_ppl,
            &compute.apple_pixel_tree_ppl,
            &compute.apple_pixel_dynamic_ppl,
            &compute.flower_pixel_ppl,
        ] {
            pipeline.begin_transient_descriptor_frame(slot);
        }
        // Other model graphics pipelines are included in GraphicsPipelines' usual
        // frame begin. Fallen fruit has its own transient set.
        graphics
            .apple_pixel_dynamic_ppl
            .begin_transient_descriptor_frame(slot);
    }

    pub fn prepare_cache(
        &mut self,
        cmd: &CommandBuffer,
        pipeline: &ComputePipeline,
        views: u32,
        apple_resolution: u32,
        flower_resolution: u32,
    ) -> Result<()> {
        self.cache.prepare(
            self.storage.frame_slot(),
            cmd,
            pipeline,
            views,
            [
                self.particle_resolutions[0],
                apple_resolution,
                self.particle_resolutions[1],
                flower_resolution,
            ],
        )
    }

    pub fn finish_cache(&self, cmd: &CommandBuffer) {
        self.cache.finish(self.storage.frame_slot(), cmd);
    }

    pub fn particle_count(&self) -> u32 {
        self.particles.count()
    }

    pub fn validate_completed_particles(
        &mut self,
        context: &VulkanContext,
        resources: &TracerResources,
    ) -> Result<()> {
        self.particles
            .validate_completed_tiles(context, self.allocator.clone(), resources)
    }

    pub fn prepare_particle_models(
        &mut self,
        snapshots: &[ParticleSnapshot],
        butterflies: ButterflyMeshSettings,
        leaves: LeafModelSettings,
        camera_position: Vec3,
        discrete_views: bool,
    ) -> Result<()> {
        self.particle_resolutions = [
            leaves.resolution.clamp(8, 64),
            butterflies.resolution.clamp(8, 64),
        ];
        // CPU pose preparation deliberately does not publish partial GPU metadata.
        self.particles.prepare_frame_models(
            snapshots,
            butterflies,
            leaves,
            camera_position,
            discrete_views,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn particles(
        &mut self,
        cmdbuf: &CommandBuffer,
        pass: PixelPass<'_>,
        view: Mat4,
        projection: Mat4,
        discrete_views: bool,
    ) -> Result<PreparedModelPixels> {
        anyhow::ensure!(
            self.particles.uses_cached_surfaces() == discrete_views,
            "particle pose and surface-cache mode must use the same frame settings"
        );
        let slot = self.storage.frame_slot();
        let cache = self.cache.frame(slot);
        let mut inputs = None;
        self.particles.prepare_repair_frame(
            view,
            projection,
            self.device.clone(),
            self.allocator.clone(),
            slot,
            |instances, triangles, indices| {
                inputs = Some(self.storage.particle_inputs(|previous| {
                    let upload = |previous, bytes| {
                        upload_input(&self.device, &self.allocator, previous, bytes)
                    };
                    Ok(ParticleInputs {
                        instances: upload(previous.map(|p| &p.instances), instances)?,
                        triangles: upload(previous.map(|p| &p.triangles), triangles)?,
                        indices: upload(previous.map(|p| &p.indices), indices)?,
                    })
                })?);
                Ok(())
            },
        )?;
        let Some(inputs) = inputs else {
            return Ok(PreparedModelPixels {
                pipeline: pass.display.clone(),
                draws: Vec::new(),
                push: None,
                instance_indices: None,
            });
        };
        let models = &self.particles;
        let repairs = models.repair_buffer(slot);
        let count = models.count();
        let batches = self.storage.particles(
            &models.tile_layout,
            allocator(&self.device, &self.allocator),
        )?;
        let mut draws = Vec::with_capacity(batches.len());
        for batch in batches {
            draws.push(batch.publish(
                |batch| {
                    let mut descriptors = cache.bindings().to_vec();
                    descriptors.extend([
                        (
                            "butterfly_mesh_instances",
                            DescriptorResource::Buffer(&inputs.instances),
                        ),
                        (
                            "butterfly_mesh_triangles",
                            DescriptorResource::Buffer(&inputs.triangles),
                        ),
                        ("draw_indices", DescriptorResource::Buffer(&inputs.indices)),
                        (
                            "model_object_samples",
                            DescriptorResource::Buffer(&batch.objects),
                        ),
                        (
                            "model_pixel_tiles",
                            DescriptorResource::Buffer(&batch.tiles),
                        ),
                        (
                            "particle_model_repairs",
                            DescriptorResource::Buffer(&repairs),
                        ),
                        (
                            "particle_model_repair_output",
                            DescriptorResource::Buffer(&repairs),
                        ),
                    ]);
                    if !models.repair_nodes.is_empty() {
                        pass.compute.record_with_descriptors(
                            cmdbuf,
                            &descriptors,
                            Extent3D::new(8, 8, count.div_ceil(64)),
                            Some(bytemuck::bytes_of(&[0u32, count, 0, 0])),
                        )?;
                    }
                    if models.compute_count > 0 {
                        if discrete_views {
                            pass.compute.record_with_descriptors(
                                cmdbuf,
                                &descriptors,
                                Extent3D::new(1, 1, batch.range.count),
                                Some(bytemuck::bytes_of(&[4u32, count, batch.range.first, 0])),
                            )?;
                        }
                        pass.compute.record_with_descriptors(
                            cmdbuf,
                            &descriptors,
                            Extent3D::new(
                                models.dispatch_resolution,
                                models.dispatch_resolution,
                                batch.range.count,
                            ),
                            Some(bytemuck::bytes_of(&[
                                models.tile_compute_mode(),
                                count,
                                batch.range.first,
                                models.reference_tile_offset().unwrap_or(0) * 4,
                            ])),
                        )?;
                        if let Some(base) = models.reference_tile_offset() {
                            pass.compute.record_with_descriptors(
                                cmdbuf,
                                &descriptors,
                                Extent3D::new(
                                    models.dispatch_resolution,
                                    models.dispatch_resolution,
                                    models.compute_count,
                                ),
                                Some(bytemuck::bytes_of(&[2u32, count, base, 0])),
                            )?;
                        }
                    }
                    Ok(())
                },
                |batch| {
                    prepare_draw(
                        cmdbuf,
                        pass.display,
                        batch,
                        &[(
                            "butterfly_mesh_instances",
                            DescriptorResource::Buffer(&inputs.instances),
                        )],
                    )
                },
            )?);
        }
        cmdbuf.use_buffer(&inputs.indices, BufferUse::VertexRead);
        Ok(PreparedModelPixels {
            pipeline: pass.display.clone(),
            draws,
            push: None,
            instance_indices: Some(inputs.indices),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn attached_apples(
        &mut self,
        cmdbuf: &CommandBuffer,
        pass: PixelPass<'_>,
        tree: u32,
        count: u32,
        resolution: u32,
        discrete_views: bool,
        pose_resources: &[(&str, DescriptorResource<'_>)],
        push: PushConstantFlora,
    ) -> Result<PreparedModelPixels> {
        let cache = self.cache.frame(self.storage.frame_slot());
        let batch = self.storage.attached_apples(
            tree,
            count,
            resolution,
            allocator(&self.device, &self.allocator),
        )?;
        let mut draws = Vec::new();
        if let Some(batch) = batch {
            draws.push(batch.publish(
                |batch| {
                    let mut descriptors = pose_resources.to_vec();
                    descriptors.extend([
                        (
                            "model_object_samples",
                            DescriptorResource::Buffer(&batch.objects),
                        ),
                        (
                            "model_pixel_tiles",
                            DescriptorResource::Buffer(&batch.tiles),
                        ),
                    ]);
                    descriptors.extend(cache.bindings());
                    if discrete_views {
                        let mut prepare = push;
                        prepare.model_object_prepare = 1;
                        pass.compute.record_with_descriptors(
                            cmdbuf,
                            &descriptors,
                            Extent3D::new(1, 1, batch.range.count),
                            Some(bytemuck::bytes_of(&prepare)),
                        )?;
                    }
                    pass.compute.record_with_descriptors(
                        cmdbuf,
                        &descriptors,
                        Extent3D::new(resolution, resolution, batch.range.count),
                        Some(bytemuck::bytes_of(&push)),
                    )
                },
                |batch| prepare_draw(cmdbuf, pass.display, batch, pose_resources),
            )?);
        }
        Ok(PreparedModelPixels {
            pipeline: pass.display.clone(),
            draws,
            push: Some(PushConstantInfo {
                shader_stage: vk::ShaderStageFlags::VERTEX,
                push_constants: bytemuck::bytes_of(&push).to_vec(),
            }),
            instance_indices: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn flowers(
        &mut self,
        cmdbuf: &CommandBuffer,
        pass: PixelPass<'_>,
        stem_pipeline: &GraphicsPipeline,
        count: u32,
        mut push: crate::generated::gpu_structs::PushConstantFlowerPixel,
        discrete_views: bool,
        pose_resources: &[(&str, DescriptorResource<'_>)],
    ) -> Result<PreparedFlowerModels> {
        let cache = self.cache.frame(self.storage.frame_slot());
        let species = push.species;
        let model = &crate::flora::models::flowers()
            [(species - crate::flora::MODEL_FLOWER_FIRST_SPECIES) as usize];
        let parts = if push.heads_only != 0 {
            model.heads.len() as u32
        } else {
            1
        };
        let tile_count = count
            .checked_mul(parts)
            .ok_or_else(|| anyhow::anyhow!("flower tile count overflow"))?;
        let batches = self.storage.flowers(
            push.chunk_world_offset,
            species,
            tile_count,
            push.resolution,
            allocator(&self.device, &self.allocator),
        )?;
        let mut draws = Vec::new();
        for batch in batches {
            push.tile_first = batch.range.first;
            draws.push(batch.publish(
                |batch| {
                    let mut descriptors = pose_resources.to_vec();
                    descriptors.extend([
                        (
                            "model_object_samples",
                            DescriptorResource::Buffer(&batch.objects),
                        ),
                        (
                            "model_pixel_tiles",
                            DescriptorResource::Buffer(&batch.tiles),
                        ),
                    ]);
                    descriptors.extend(cache.bindings());
                    if discrete_views {
                        let mut prepare = push;
                        prepare.prepare_object = 1;
                        pass.compute.record_with_descriptors(
                            cmdbuf,
                            &descriptors,
                            Extent3D::new(1, 1, batch.range.count),
                            Some(bytemuck::bytes_of(&prepare)),
                        )?;
                    }
                    pass.compute.record_with_descriptors(
                        cmdbuf,
                        &descriptors,
                        Extent3D::new(push.resolution, push.resolution, batch.range.count),
                        Some(bytemuck::bytes_of(&push)),
                    )
                },
                |batch| {
                    let mut draw = prepare_draw(cmdbuf, pass.display, batch, pose_resources)?;
                    draw.push = Some(PushConstantInfo {
                        shader_stage: vk::ShaderStageFlags::VERTEX,
                        push_constants: bytemuck::bytes_of(&push).to_vec(),
                    });
                    Ok(draw)
                },
            )?);
        }
        let stems = if push.heads_only != 0 && count > 0 {
            Some(PreparedModelPixels {
                pipeline: stem_pipeline.clone(),
                draws: vec![PixelDraw {
                    descriptors: stem_pipeline.prepare_draw_descriptors(cmdbuf, pose_resources)?,
                    first: 0,
                    count,
                    push: None,
                }],
                push: Some(PushConstantInfo {
                    shader_stage: vk::ShaderStageFlags::VERTEX,
                    push_constants: bytemuck::bytes_of(&push).to_vec(),
                }),
                instance_indices: None,
            })
        } else {
            None
        };
        Ok(PreparedFlowerModels {
            pixels: PreparedModelPixels {
                pipeline: pass.display.clone(),
                draws,
                push: None,
                instance_indices: None,
            },
            stems,
            stem_index_count: model.stem_triangles * 3,
        })
    }

    pub fn fallen_apples(
        &mut self,
        cmdbuf: &CommandBuffer,
        pass: PixelPass<'_>,
        fruit: &DynamicFruitRendererResources,
        resolution: u32,
        discrete_views: bool,
    ) -> Result<PreparedModelPixels> {
        let cache = self.cache.frame(self.storage.frame_slot());
        let batch = self.storage.fallen_apples(
            fruit.instance_count,
            resolution,
            allocator(&self.device, &self.allocator),
        )?;
        let mut draws = Vec::new();
        if let Some(batch) = batch {
            draws.push(batch.publish(
                |batch| {
                    let mut descriptors = cache.bindings().to_vec();
                    descriptors.extend([
                        (
                            "model_pixel_tiles",
                            DescriptorResource::Buffer(&batch.tiles),
                        ),
                        (
                            "model_object_samples",
                            DescriptorResource::Buffer(&batch.objects),
                        ),
                        (
                            "dynamic_fruit_pixel_instances",
                            DescriptorResource::Buffer(&fruit.instances),
                        ),
                    ]);
                    if discrete_views {
                        pass.compute.record_with_descriptors(
                            cmdbuf,
                            &descriptors,
                            Extent3D::new(1, 1, batch.range.count),
                            Some(bytemuck::bytes_of(&1u32)),
                        )?;
                    }
                    pass.compute.record_with_descriptors(
                        cmdbuf,
                        &descriptors,
                        Extent3D::new(resolution, resolution, batch.range.count),
                        Some(bytemuck::bytes_of(&0u32)),
                    )
                },
                |batch| prepare_draw(cmdbuf, pass.display, batch, &[]),
            )?);
        }
        Ok(PreparedModelPixels {
            pipeline: pass.display.clone(),
            draws,
            push: None,
            instance_indices: None,
        })
    }
}

// Host-authored streams use the same ready-slot lifetime as paired tiles.
// Descriptor initializers retain tiny placeholders; no fixed population-sized
// allocation or resource binding can become a model admission limit.
fn upload_input(
    device: &Device,
    allocator: &Allocator,
    previous: Option<&Arc<Buffer>>,
    bytes: &[u8],
) -> Result<Arc<Buffer>> {
    let capacity = input_capacity(bytes.len())?;
    let buffer = if let Some(previous) = previous.filter(|b| b.get_size_bytes() >= capacity as u64)
    {
        previous.clone()
    } else {
        Arc::new(Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::VERTEX_BUFFER,
            ),
            MemoryLocation::CpuToGpu,
            capacity as u64,
        ))
    };
    if !bytes.is_empty() {
        buffer.fill_range_with_raw_u8(0, bytes)?;
    }
    Ok(buffer)
}

fn input_capacity(bytes: usize) -> Result<usize> {
    let capacity = bytes
        .max(16)
        .checked_next_power_of_two()
        .ok_or_else(|| anyhow::anyhow!("model input allocation overflow"))?;
    anyhow::ensure!(
        capacity <= 128 * 1024 * 1024,
        "model inputs exceed portable storage-buffer range"
    );
    Ok(capacity)
}

fn allocator(device: &Device, allocator: &Allocator) -> impl FnMut(usize) -> Result<Arc<Buffer>> {
    let (device, allocator) = (device.clone(), allocator.clone());
    move |texels| {
        Ok(Arc::new(Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::GpuOnly,
            (texels * 16) as u64,
        )))
    }
}

fn prepare_draw(
    cmdbuf: &CommandBuffer,
    display: &GraphicsPipeline,
    batch: &ModelPixelBatch<Arc<Buffer>>,
    pose_resources: &[(&str, DescriptorResource<'_>)],
) -> Result<PixelDraw> {
    let mut descriptors = pose_resources.to_vec();
    descriptors.extend([
        (
            "model_pixel_tiles",
            DescriptorResource::Buffer(&batch.tiles),
        ),
        // Read-only graphics declaration of the SAME four-float4 object data.
        (
            "model_object_view_samples",
            DescriptorResource::Buffer(&batch.objects),
        ),
    ]);
    Ok(PixelDraw {
        descriptors: display.prepare_draw_descriptors(cmdbuf, &descriptors)?,
        first: batch.range.first,
        count: batch.range.count,
        push: None,
    })
}

#[cfg(test)]
mod tests {
    use super::super::model_pixel_tiles::{
        ModelPixelBatch, ModelPixelStorage, ParticleTiles, TileBatch, BATCH_TEXELS,
    };
    use anyhow::Result;
    use std::{cell::RefCell, sync::Arc};

    // No GPU emulation: these are identifiable allocation lifetimes. The same
    // storage/publication interface is used by all three native adapters above.
    type Allocation = Arc<usize>;
    fn allocate(texels: usize) -> Result<Allocation> {
        Ok(Arc::new(texels))
    }
    fn publish(batch: ModelPixelBatch<Allocation>) -> (TileBatch, Allocation, Allocation) {
        let computed = RefCell::new(None);
        batch
            .publish(
                |batch| {
                    computed.replace(Some((
                        batch.range,
                        batch.tiles.clone(),
                        batch.objects.clone(),
                    )));
                    Ok(())
                },
                |batch| {
                    let (range, tiles, objects) = computed.borrow_mut().take().unwrap();
                    assert_eq!(batch.range, range);
                    assert!(Arc::ptr_eq(&batch.tiles, &tiles));
                    assert!(Arc::ptr_eq(&batch.objects, &objects));
                    Ok((batch.range, tiles, objects))
                },
            )
            .unwrap()
    }

    #[test]
    fn particle_inputs_have_one_publication_and_ready_slot_retirement() {
        use super::ParticleInputs;
        let mut frame = ModelPixelStorage::default();
        let make = |_: Option<&ParticleInputs<Allocation>>| -> Result<ParticleInputs<Allocation>> {
            Ok(ParticleInputs {
                instances: allocate(32769 * 96)?,
                triangles: allocate(2048 * 128)?,
                indices: allocate(32769 * 4)?,
            })
        };
        frame.begin_frame(0);
        let first = frame.particle_inputs(make).unwrap();
        let weak = Arc::downgrade(&first.instances);
        assert!(frame
            .particle_inputs(|_| panic!("duplicate must fail before upload"))
            .is_err());
        frame.begin_frame(1);
        let other = frame.particle_inputs(make).unwrap();
        assert!(!Arc::ptr_eq(&first.instances, &other.instances));
        frame.begin_frame(0);
        let reused = frame
            .particle_inputs(|previous| Ok(previous.unwrap().clone()))
            .unwrap();
        assert!(Arc::ptr_eq(&first.instances, &reused.instances));
        assert!(Arc::ptr_eq(&first.triangles, &reused.triangles));
        assert!(Arc::ptr_eq(&first.indices, &reused.indices));
        drop((first, reused));
        frame.begin_frame(1);
        assert!(
            weak.upgrade().is_some(),
            "other slot must not retire in-flight inputs"
        );
        frame.begin_frame(0); // Empty frame retains last-use storage until next return.
        assert!(weak.upgrade().is_some());
        frame.begin_frame(0);
        assert!(weak.upgrade().is_none());
        assert!(frame
            .particle_inputs(|previous| {
                assert!(previous.is_none());
                anyhow::bail!("upload failed")
            })
            .is_err());
        frame.particle_inputs(make).unwrap(); // Failed publication does not poison a retry.
    }

    #[test]
    fn flower_batches_keep_global_head_indices_and_isolate_species_chunks_and_slots() {
        for resolution in [8, 32, 64] {
            let mut storage = ModelPixelStorage::default();
            storage.begin_frame(0);
            let count = 120_000;
            let draws = storage
                .flowers([0, 0, 256], 4, count, resolution, allocate)
                .unwrap();
            let mut next = 0;
            for batch in draws {
                let (range, tiles, objects) = publish(batch);
                assert_eq!(range.first, next);
                next += range.count;
                assert_eq!(
                    range.texels,
                    (range.count * resolution * resolution) as usize
                );
                assert!(range.count <= 65_535 && range.texels <= BATCH_TEXELS);
                assert!(*tiles >= range.texels);
                assert!(*objects >= count as usize * 4);
            }
            assert_eq!(next, count);
            let (_, a, _) = publish(
                storage
                    .flowers([0, 0, 256], 5, 1, resolution, allocate)
                    .unwrap()
                    .remove(0),
            );
            let (_, b, _) = publish(
                storage
                    .flowers([256, 0, 256], 5, 1, resolution, allocate)
                    .unwrap()
                    .remove(0),
            );
            assert!(!Arc::ptr_eq(&a, &b));
            storage.begin_frame(1);
            let (_, c, _) = publish(
                storage
                    .flowers([0, 0, 256], 5, 1, resolution, allocate)
                    .unwrap()
                    .remove(0),
            );
            assert!(!Arc::ptr_eq(&a, &c));
            assert!(storage
                .flowers([0, 0, 0], 4, 0, resolution, allocate)
                .unwrap()
                .is_empty());
            assert!(storage.flowers([0, 0, 0], 4, 1, 0, allocate).is_err());
        }
    }

    #[test]
    fn host_input_demand_has_only_storage_guards_not_a_leaf_population_cap() {
        assert_eq!(super::input_capacity(0).unwrap(), 16);
        assert_eq!(super::input_capacity(17).unwrap(), 32);
        assert!(super::input_capacity(200_000 * 96).unwrap() >= 200_000 * 96);
        assert_eq!(
            super::input_capacity(128 * 1024 * 1024).unwrap(),
            128 * 1024 * 1024
        );
        assert!(super::input_capacity(128 * 1024 * 1024 + 1).is_err());
        assert!(super::input_capacity(usize::MAX).is_err());
    }

    #[test]
    fn low_resolution_and_invisible_populations_split_at_the_dispatch_limit() {
        for visible in [false, true] {
            let count = 200_000;
            let order = (0..count as u32).rev().collect::<Vec<_>>();
            let layout = ParticleTiles::pack(&vec![(8, visible); count], &order);
            let mut frame = ModelPixelStorage::default();
            frame.begin_frame(0);
            let draws = frame.particles(&layout, allocate).unwrap();
            assert_eq!(
                draws.iter().map(|b| b.range.count as usize).sum::<usize>(),
                count
            );
            for draw in draws {
                let (range, _, _) = publish(draw);
                assert!(range.count <= 65_535 && range.texels <= BATCH_TEXELS);
                assert_eq!(layout.offsets()[order[range.first as usize] as usize], 0);
            }
        }
    }

    #[test]
    fn full_particle_frame_keeps_sorted_compute_draw_pairing_across_64_mib_batches() {
        let count = crate::particles::INITIAL_PARTICLE_CAPACITY * 2 + 256;
        let order = (0..count as u32).rev().collect::<Vec<_>>();
        let layout = ParticleTiles::pack(&vec![(64, true); count], &order);
        let mut frame = ModelPixelStorage::default();
        frame.begin_frame(0);
        let draws = frame
            .particles(&layout, allocate)
            .unwrap()
            .into_iter()
            .map(publish)
            .collect::<Vec<_>>();
        assert!(draws.len() > 1);
        assert_eq!(
            draws.iter().map(|(r, ..)| r.count as usize).sum::<usize>(),
            count
        );
        let mut next = 0;
        for (range, tiles, objects) in draws {
            assert_eq!(range.first, next);
            next += range.count;
            assert!(range.texels <= BATCH_TEXELS);
            assert_eq!(*tiles, range.texels.next_power_of_two());
            assert_eq!(*objects, (count * 4).next_power_of_two());
            for (tile, &instance) in order[range.first as usize..next as usize]
                .iter()
                .enumerate()
            {
                assert_eq!(layout.offsets()[instance as usize], tile as u32 * 4096);
            }
        }
        assert_eq!(
            next as usize, count,
            "never silently drop a model at the batch limit"
        );
    }

    #[test]
    fn mixed_resolution_and_offscreen_tiles_follow_the_published_draw_order() {
        let layout = ParticleTiles::pack(
            &[(8, true), (64, false), (16, true), (32, true)],
            &[3, 1, 0, 2],
        );
        assert_eq!(layout.offsets(), [1024, 1024, 1088, 0]);
        let mut frame = ModelPixelStorage::default();
        frame.begin_frame(1);
        let draws = frame.particles(&layout, allocate).unwrap();
        assert_eq!(draws.len(), 1);
        let (range, tiles, objects) = publish(draws.into_iter().next().unwrap());
        assert_eq!(
            range,
            TileBatch {
                first: 0,
                count: 4,
                texels: 1344
            }
        );
        assert_eq!((*tiles, *objects), (2048, 16));

        frame.begin_frame(1);
        let layout = ParticleTiles::pack(&[(64, false)], &[0]);
        let (range, _, _) = publish(frame.particles(&layout, allocate).unwrap().remove(0));
        assert_eq!(range.count, 1);
        assert_eq!(range.texels, 0);
        assert_eq!(layout.offsets(), [0]);
    }

    #[test]
    fn particle_tree_and_fallen_adapters_never_alias_even_with_matching_ids() {
        let mut frame = ModelPixelStorage::default();
        frame.begin_frame(0);
        let particle = publish(
            frame
                .particles(&ParticleTiles::pack(&[(8, true)], &[0]), allocate)
                .unwrap()
                .remove(0),
        );
        let tree = publish(frame.attached_apples(0, 1, 8, allocate).unwrap().unwrap());
        let fallen = publish(frame.fallen_apples(1, 8, allocate).unwrap().unwrap());
        for (a, b) in [(&particle, &tree), (&tree, &fallen), (&fallen, &particle)] {
            assert!(!Arc::ptr_eq(&a.1, &b.1));
            assert!(!Arc::ptr_eq(&a.2, &b.2));
            assert!(!Arc::ptr_eq(&a.1, &a.2));
        }
        assert!(
            frame.attached_apples(0, 1, 8, allocate).is_err(),
            "no second publication may overwrite a live batch"
        );
    }

    #[test]
    fn resolution_changes_grow_only_the_ready_slot_and_reuse_smaller_frames() {
        let mut frame = ModelPixelStorage::default();
        frame.begin_frame(0);
        let (_, first_tiles, first_objects) =
            publish(frame.attached_apples(7, 17, 8, allocate).unwrap().unwrap());
        frame.begin_frame(1);
        let (_, in_flight_tiles, _) =
            publish(frame.attached_apples(7, 17, 8, allocate).unwrap().unwrap());
        assert!(!Arc::ptr_eq(&first_tiles, &in_flight_tiles));
        frame.begin_frame(0);
        let (_, large_tiles, large_objects) =
            publish(frame.attached_apples(7, 17, 64, allocate).unwrap().unwrap());
        assert!(!Arc::ptr_eq(&first_tiles, &large_tiles));
        assert!(Arc::ptr_eq(&first_objects, &large_objects));
        assert_eq!(*in_flight_tiles, (17usize * 64).next_power_of_two());
        frame.begin_frame(0);
        let (_, small_tiles, _) =
            publish(frame.attached_apples(7, 17, 16, allocate).unwrap().unwrap());
        assert!(Arc::ptr_eq(&large_tiles, &small_tiles));
        frame.begin_frame(1);
        let (_, reused, _) = publish(frame.attached_apples(7, 17, 8, allocate).unwrap().unwrap());
        assert!(Arc::ptr_eq(&in_flight_tiles, &reused));
    }

    #[test]
    fn vanished_batches_and_removed_trees_retire_only_after_their_slot_returns() {
        let mut frame = ModelPixelStorage::default();
        let large = ParticleTiles::pack(&vec![(64, true); 1100], &(0..1100).collect::<Vec<_>>());
        frame.begin_frame(0);
        let mut particles = frame.particles(&large, allocate).unwrap();
        let (_, removed_tiles, removed_objects) = publish(particles.pop().unwrap());
        let weak_tiles = Arc::downgrade(&removed_tiles);
        let weak_objects = Arc::downgrade(&removed_objects);
        drop((particles, removed_tiles, removed_objects));
        let (_, tree_tiles, tree_objects) = publish(
            frame
                .attached_apples(42, 17, 32, allocate)
                .unwrap()
                .unwrap(),
        );
        let weak_tree_tiles = Arc::downgrade(&tree_tiles);
        let weak_tree_objects = Arc::downgrade(&tree_objects);
        drop((tree_tiles, tree_objects));

        frame.begin_frame(1);
        assert!(frame
            .particles(&ParticleTiles::default(), allocate)
            .unwrap()
            .is_empty());
        assert!(frame
            .attached_apples(42, 0, 32, allocate)
            .unwrap()
            .is_none());
        assert!(frame.fallen_apples(0, 32, allocate).unwrap().is_none());
        assert!(weak_tiles.upgrade().is_some());
        assert!(weak_tree_tiles.upgrade().is_some());

        frame.begin_frame(0);
        let smaller = ParticleTiles::pack(&[(16, true)], &[0]);
        assert_eq!(frame.particles(&smaller, allocate).unwrap().len(), 1);
        // Last-use allocations remain reusable until this slot's next begin.
        assert!(weak_tiles.upgrade().is_some());
        frame.begin_frame(0);
        assert!(weak_tiles.upgrade().is_none());
        assert!(weak_objects.upgrade().is_none());
        assert!(weak_tree_tiles.upgrade().is_none());
        assert!(weak_tree_objects.upgrade().is_none());
        assert!(frame
            .attached_apples(42, 17, 32, allocate)
            .unwrap()
            .is_some());
    }

    #[test]
    fn empty_particle_frames_cannot_republish_stale_draws() {
        let mut frame = ModelPixelStorage::default();
        frame.begin_frame(0);
        let (_, tiles, objects) = publish(
            frame
                .particles(&ParticleTiles::pack(&[(64, true)], &[0]), allocate)
                .unwrap()
                .remove(0),
        );
        let (weak_tiles, weak_objects) = (Arc::downgrade(&tiles), Arc::downgrade(&objects));
        drop((tiles, objects));
        frame.begin_frame(0);
        assert!(frame
            .particles(&ParticleTiles::pack(&[], &[]), |_| panic!(
                "empty frame must not allocate"
            ))
            .unwrap()
            .is_empty());
        frame.begin_frame(0);
        assert!(weak_tiles.upgrade().is_none());
        assert!(weak_objects.upgrade().is_none());
    }

    #[test]
    fn apple_capacity_and_resolution_guards_precede_allocation() {
        let mut frame = ModelPixelStorage::default();
        frame.begin_frame(0);
        for (count, resolution) in [(2049, 64), (u32::MAX, 64), (1, 7), (1, 65)] {
            assert!(frame
                .fallen_apples(count, resolution, |_| panic!(
                    "invalid demand must not allocate"
                ))
                .is_err());
        }
        let (range, tiles, objects) =
            publish(frame.fallen_apples(2048, 64, allocate).unwrap().unwrap());
        assert_eq!(range.count, 2048);
        assert_eq!(*tiles * 16, 128 * 1024 * 1024);
        assert_eq!(*objects, 2048 * 4);
    }

    #[test]
    fn allocation_or_compute_failure_cannot_publish_a_draw_or_partial_pair() {
        let mut frame = ModelPixelStorage::default();
        frame.begin_frame(0);
        let mut allocations = 0;
        assert!(frame
            .fallen_apples(1, 16, |texels| {
                allocations += 1;
                anyhow::ensure!(allocations == 1, "object allocation failed");
                allocate(texels)
            })
            .is_err());
        let batch = frame.fallen_apples(1, 16, allocate).unwrap().unwrap();
        assert!(batch
            .publish(
                |_| anyhow::bail!("compute failed"),
                |_| -> Result<()> { panic!("must not publish uncomputed pixels") }
            )
            .is_err());
    }
}
