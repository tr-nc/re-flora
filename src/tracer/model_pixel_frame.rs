//! Model pixels are published as complete compute/draw pairs. Tracer schedules
//! semantic adapters and draws their opaque results; it never recreates storage
//! identities, object-buffer sizes, tile ranges or compute/display bindings.
//!
//! FrameManager still establishes fence readiness. PipelineTopology and the VKN
//! pipelines still own descriptor/extent retirement. This is live tile rendering,
//! not a cache or a transaction for unrelated rendering work.
use super::{
    butterfly_mesh::{
        ButterflyMeshRenderer, ButterflyMeshResources, ButterflyMeshSettings, LeafModelSettings,
    },
    dynamic_fruit_resources::DynamicFruitRendererResources,
    model_pixel_tiles::{ModelPixelBatch, ModelPixelStorage},
    pipeline_builder::{ComputePipelines, GraphicsPipelines},
    resources::TracerResources,
    PushConstantFlora,
};
use crate::particles::ParticleSnapshot;
use anyhow::Result;
use glam::{Mat4, Vec3};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, CommandBuffer, ComputePipeline, DescriptorResource, Device,
    Extent3D, GraphicsPipeline, MemoryLocation, PreparedDrawDescriptors, PushConstantInfo,
    VulkanContext,
};
use std::sync::Arc;

/// Native pipelines for one of the three concrete pose/material adapters.
/// This is a dependency, not a list of rendering steps returned to the host.
pub(super) struct PixelPass<'a> {
    pub compute: &'a ComputePipeline,
    pub display: &'a GraphicsPipeline,
}

struct PixelDraw {
    descriptors: PreparedDrawDescriptors,
    first: u32,
    count: u32,
}

/// Created only after compute and descriptor preparation succeed. The same
/// pipeline, draw range, object data and (for trees) pose push survive to draw.
/// Use within the current frame, after entering its render pass.
pub(super) struct PreparedModelPixels {
    pipeline: GraphicsPipeline,
    draws: Vec<PixelDraw>,
    push: Option<PushConstantInfo>,
}
impl PreparedModelPixels {
    pub fn record(&self, cmdbuf: &CommandBuffer, index_count: u32) {
        for draw in &self.draws {
            self.pipeline.record_indexed_with_prepared_descriptors(
                cmdbuf,
                &draw.descriptors,
                index_count,
                draw.count,
                0,
                0,
                draw.first,
                self.push.as_ref(),
            );
        }
    }
}

pub(super) struct ModelPixelFrame {
    device: Device,
    allocator: Allocator,
    storage: ModelPixelStorage<Arc<Buffer>>,
    particles: ButterflyMeshRenderer,
}
impl ModelPixelFrame {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        Self {
            device,
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
        ] {
            pipeline.begin_transient_descriptor_frame(slot);
        }
        // Other model graphics pipelines are included in GraphicsPipelines' usual
        // frame begin. Fallen fruit has its own transient set.
        graphics
            .apple_pixel_dynamic_ppl
            .begin_transient_descriptor_frame(slot);
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
    ) -> Result<()> {
        // CPU pose preparation deliberately does not publish partial GPU metadata.
        self.particles
            .prepare_frame_models(snapshots, butterflies, leaves, camera_position)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn particles(
        &mut self,
        cmdbuf: &CommandBuffer,
        pass: PixelPass<'_>,
        resources: &ButterflyMeshResources,
        view: Mat4,
        projection: Mat4,
        discrete_views: bool,
    ) -> Result<PreparedModelPixels> {
        let slot = self.storage.frame_slot();
        self.particles.prepare_repair_frame(
            view,
            projection,
            resources,
            self.device.clone(),
            self.allocator.clone(),
            slot,
        )?;
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
                    let descriptors = [
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
                    ];
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
                |batch| prepare_draw(cmdbuf, pass.display, batch, &[]),
            )?);
        }
        Ok(PreparedModelPixels {
            pipeline: pass.display.clone(),
            draws,
            push: None,
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
        let batch = self.storage.fallen_apples(
            fruit.instance_count,
            resolution,
            allocator(&self.device, &self.allocator),
        )?;
        let mut draws = Vec::new();
        if let Some(batch) = batch {
            draws.push(batch.publish(
                |batch| {
                    let descriptors = [
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
                    ];
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
        })
    }
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
    fn full_particle_frame_keeps_sorted_compute_draw_pairing_across_64_mib_batches() {
        let count = crate::particles::PARTICLE_CAPACITY + 256;
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
