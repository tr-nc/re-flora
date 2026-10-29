//! Shared submission backend. The synchronous API and async transactions use identical
//! admission, dispatch and compaction; only scratch/output ownership and completion differ.
use super::*;
use crate::resource::ResourceContainer;
use re_flora_vkn::{Allocator, Device, MemoryLocation, Texture};

#[derive(Clone, Copy)]
pub struct GrassPaintRequest {
    /// Inclusive affected world-voxel bounds, not chunk coordinates.
    pub voxel_bound: crate::geom::UAabb3,
    pub start: Vec3,
    pub end: Vec3,
    pub radius: f32,
    pub tick: u32,
    pub spawn_time_ms: u32,
    pub selection: species::FloraPaintSelection,
    pub dab_serial: u32,
    pub brush: species::FloraPaintBrushSettings,
}

#[derive(Clone, Copy)]
pub(super) struct EditRequest {
    pub chunk_id: UVec3,
    pub edit_start: Vec3,
    pub edit_end: Vec3,
    pub edit_radius: f32,
    pub flora_tick: u32,
    pub spawn_time_ms: u32,
    pub target_age: u32,
    pub paint_selection: species::FloraPaintSelection,
    pub paint_dab_serial: u32,
    pub paint_brush: species::FloraPaintBrushSettings,
    pub mode: OccupancyEditMode,
}

#[derive(Clone)]
pub(super) struct EditScratch {
    pub surface: Texture,
    pub occupancy_data: Texture,
    pub clear_occupancy_info: Buffer,
    pub instances_to_occupancy_info: Buffer,
    pub edit_occupancy_info: Buffer,
    pub occupancy_to_instances_info: Buffer,
    pub occupancy_to_instances_result: Buffer,
    pub occupancy_edit_dispatch: Buffer,
}
impl ResourceContainer for EditScratch {
    fn resolve_resource(&self, name: &str) -> crate::resource::ResourceLookup<'_> {
        use crate::resource::{DescriptorResource, ResourceLookup};
        match name {
            "surface" => ResourceLookup::Unique(DescriptorResource::Texture(&self.surface)),
            "occupancy_data" => {
                ResourceLookup::Unique(DescriptorResource::Texture(&self.occupancy_data))
            }
            "clear_occupancy_info" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.clear_occupancy_info))
            }
            "instances_to_occupancy_info" => ResourceLookup::Unique(DescriptorResource::Buffer(
                &self.instances_to_occupancy_info,
            )),
            "edit_occupancy_info" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.edit_occupancy_info))
            }
            "occupancy_to_instances_info" => ResourceLookup::Unique(DescriptorResource::Buffer(
                &self.occupancy_to_instances_info,
            )),
            "occupancy_to_instances_result" => ResourceLookup::Unique(DescriptorResource::Buffer(
                &self.occupancy_to_instances_result,
            )),
            "occupancy_edit_dispatch" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.occupancy_edit_dispatch))
            }
            _ => ResourceLookup::Missing,
        }
    }
}
impl EditScratch {
    pub fn shared(r: &SurfaceResources) -> Self {
        Self {
            surface: r.surface.clone(),
            occupancy_data: r.occupancy_data.clone(),
            clear_occupancy_info: r.clear_occupancy_info.clone(),
            instances_to_occupancy_info: r.instances_to_occupancy_info.clone(),
            edit_occupancy_info: r.edit_occupancy_info.clone(),
            occupancy_to_instances_info: r.occupancy_to_instances_info.clone(),
            occupancy_to_instances_result: r.occupancy_to_instances_result.clone(),
            occupancy_edit_dispatch: r.occupancy_edit_dispatch.clone(),
        }
    }
    pub fn private(
        r: &SurfaceResources,
        images: &(Texture, Texture),
        device: &Device,
        allocator: &Allocator,
    ) -> Self {
        let uniform = |b: &Buffer| {
            Buffer::from_uniform_layout(
                device.clone(),
                allocator.clone(),
                b.get_layout().unwrap().clone(),
            )
        };
        let storage = |b: &Buffer| {
            Buffer::from_buffer_layout(
                device.clone(),
                allocator.clone(),
                b.get_layout().unwrap().clone(),
                b.get_usage(),
                MemoryLocation::CpuToGpu,
            )
        };
        Self {
            surface: images.0.clone(),
            occupancy_data: images.1.clone(),
            clear_occupancy_info: uniform(&r.clear_occupancy_info),
            instances_to_occupancy_info: uniform(&r.instances_to_occupancy_info),
            edit_occupancy_info: uniform(&r.edit_occupancy_info),
            occupancy_to_instances_info: uniform(&r.occupancy_to_instances_info),
            occupancy_to_instances_result: storage(&r.occupancy_to_instances_result),
            occupancy_edit_dispatch: storage(&r.occupancy_edit_dispatch),
        }
    }
}

impl EditRequest {
    pub(super) fn timing_passes(&self, has_instances: bool) -> Vec<SurfacePassTimingPass> {
        let mode = self.mode;
        let max_len = u32::from(has_instances);
        let mut flora_edit_timing_passes = if max_len > 0 {
            FLORA_EDIT_TIMING_PASSES_WITH_INSTANCES.to_vec()
        } else {
            FLORA_EDIT_TIMING_PASSES_WITHOUT_INSTANCES.to_vec()
        };
        if mode == OccupancyEditMode::Add {
            flora_edit_timing_passes.insert(
                0,
                SurfacePassTimingPass {
                    label: "make_brush_surface",
                    bench_key: "flora_edit_brush_surface_gpu",
                },
            );
        }

        flora_edit_timing_passes
    }
}

impl SurfaceBuilder {
    pub(super) fn record_occupancy_edit(
        &self,
        request: &EditRequest,
        scratch: &EditScratch,
        chunk_resources: &FloraInstanceResources,
        timed: bool,
    ) -> Result<GpuJobToken> {
        let EditRequest {
            chunk_id,
            edit_start,
            edit_end,
            edit_radius,
            flora_tick,
            spawn_time_ms,
            target_age,
            paint_selection,
            paint_dab_serial,
            paint_brush,
            mode,
        } = *request;
        let chunk_world_offset = chunk_id * self.voxel_dim_per_chunk;
        let edit_start_vox = edit_start * 256.;
        let edit_end_vox = edit_end * 256.;
        let edit_radius_vox = edit_radius * 256.;
        let mut species_len = [0; 4];
        for (s, len) in species_len.iter_mut().enumerate().take(2) {
            *len = chunk_resources.species_len(s);
        }
        let max_len = *species_len.iter().max().unwrap();
        let (_, brush_dim) = brush_region::surface_region(
            edit_start_vox,
            edit_end_vox,
            edit_radius_vox,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
        );
        let edit_dim = match mode {
            OccupancyEditMode::Add | OccupancyEditMode::Remove | OccupancyEditMode::Trim => {
                brush_dim
            }
            _ => self.voxel_dim_per_chunk,
        };
        update_clear_occupancy_info(&scratch.clear_occupancy_info, self.voxel_dim_per_chunk)?;
        update_instances_to_occupancy_info(
            &scratch.instances_to_occupancy_info,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
            species_len,
            0u32, // tick_delta=0 for rebuild
            spawn_time_ms,
        )?;
        update_edit_occupancy_info(
            &scratch.edit_occupancy_info,
            edit_start_vox,
            edit_end_vox,
            edit_radius_vox,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
            mode,
            flora_tick,
            target_age,
            paint_selection,
            paint_dab_serial,
            paint_brush,
        )?;
        update_occupancy_to_instances_info(
            &scratch.occupancy_to_instances_info,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
            spawn_time_ms,
            match mode {
                OccupancyEditMode::RefreshGrowthPotential => 1,
                OccupancyEditMode::ValidateTerrain => 2,
                _ => 0,
            },
        )?;
        cleanup_occupancy_to_instances_result(&scratch.occupancy_to_instances_result)?;

        let mut descriptor_retirements = Vec::new();
        for pipeline in [
            &self.make_brush_surface_ppl,
            &self.clear_occupancy_ppl,
            &self.instances_to_occupancy_ppl,
            &self.edit_occupancy_ppl,
            &self.occupancy_to_instances_ppl,
        ] {
            descriptor_retirements.push(self.bind_edit_descriptors(
                pipeline,
                scratch,
                chunk_resources,
            )?);
        }

        let flora_edit_timing_passes = request.timing_passes(max_len > 0);

        let device = self.vulkan_ctx.device();
        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        cmdbuf.use_buffer(&scratch.clear_occupancy_info, BufferUse::HostWrite);

        let pass_timing = if timed {
            self.pass_timing.as_ref()
        } else {
            None
        };
        if let Some(timing) = pass_timing {
            timing.record_reset(&cmdbuf, flora_edit_timing_passes.len());
        }
        let mut timing_pass_index = 0usize;
        macro_rules! record_timed_flora_edit_pass {
            ($body:block) => {{
                if let Some(timing) = pass_timing {
                    timing.record_start(&cmdbuf, timing_pass_index);
                }
                let result = { $body };
                if let Some(timing) = pass_timing {
                    timing.record_end(&cmdbuf, timing_pass_index);
                    timing_pass_index += 1;
                }
                result
            }};
        }

        // Scratch surface is regenerated from authoritative terrain, only where new
        // roots may be admitted. No persistent cache, invalidation or extra submit/wait.
        if mode == OccupancyEditMode::Add {
            let (_, dim) = brush_region::surface_region(
                edit_start_vox,
                edit_end_vox,
                edit_radius_vox,
                chunk_world_offset,
                self.voxel_dim_per_chunk,
            );
            cmdbuf.use_buffer(&scratch.edit_occupancy_info, BufferUse::HostWrite);
            record_timed_flora_edit_pass!({
                self.make_brush_surface_ppl.record(
                    &cmdbuf,
                    Extent3D::new(dim.x, dim.y, dim.z),
                    None,
                );
            });
        }

        record_timed_flora_edit_pass!({
            self.clear_occupancy_ppl.record(
                &cmdbuf,
                Extent3D::new(
                    self.voxel_dim_per_chunk.x,
                    self.voxel_dim_per_chunk.y,
                    self.voxel_dim_per_chunk.z,
                ),
                None,
            );
        });

        if max_len > 0 {
            cmdbuf.use_buffer(
                &chunk_resources.resource.instances_buf,
                BufferUse::HostWrite,
            );
            record_timed_flora_edit_pass!({
                self.instances_to_occupancy_ppl
                    .record(&cmdbuf, Extent3D::new(max_len, 1, 1), None);
            });
        }

        cmdbuf.use_buffer(&scratch.edit_occupancy_info, BufferUse::HostWrite);
        record_clear_buffer_for_compute(&cmdbuf, &scratch.occupancy_edit_dispatch);

        record_timed_flora_edit_pass!({
            self.edit_occupancy_ppl.record(
                &cmdbuf,
                Extent3D::new(edit_dim.x, edit_dim.y, edit_dim.z),
                None,
            );
        });

        cmdbuf.use_buffer(&scratch.occupancy_to_instances_info, BufferUse::HostWrite);
        cmdbuf.use_buffer(&scratch.occupancy_to_instances_result, BufferUse::HostWrite);

        record_timed_flora_edit_pass!({
            self.occupancy_to_instances_ppl.record_indirect(
                &cmdbuf,
                &scratch.occupancy_edit_dispatch,
                None,
            );
        });
        cmdbuf.use_buffer(&scratch.occupancy_to_instances_result, BufferUse::HostRead);

        cmdbuf.use_buffer(&chunk_resources.resource.instances_buf, BufferUse::HostRead);
        cmdbuf.use_buffer(&scratch.occupancy_edit_dispatch, BufferUse::HostRead);
        if pass_timing.is_some() {
            assert_eq!(timing_pass_index, flora_edit_timing_passes.len());
        }

        cmdbuf.end();
        let mut gpu_job = cmdbuf.submit_gpu_job(
            &self.vulkan_ctx.get_general_queue(),
            if timed {
                "surface.flora_edit"
            } else {
                "surface.flora_edit_async"
            },
        )?;
        for retirement in descriptor_retirements {
            gpu_job.retain_frame_retirement(retirement);
        }
        gpu_job.retain_frame_retirement(FrameRetirement::new(
            "flora.edit.scratch",
            0,
            (
                scratch.clone(),
                chunk_resources.resource.instances_buf.clone(),
                chunk_resources.grass_growth_potential_levels.clone(),
                chunk_resources.plant_hydration.clone(),
            ),
        ));
        Ok(gpu_job)
    }

    fn bind_edit_descriptors(
        &self,
        pipeline: &ComputePipeline,
        scratch: &EditScratch,
        chunk: &FloraInstanceResources,
    ) -> Result<FrameRetirement> {
        let generation = self.next_descriptor_generation.get();
        self.next_descriptor_generation.set(
            generation
                .checked_add(1)
                .expect("descriptor generation overflow"),
        );
        pipeline.publish_descriptors(
            "surface.edit.descriptors",
            generation,
            DescriptorUpdate::All(&[scratch, chunk, &self.terrain_observation]),
        )
    }
}
