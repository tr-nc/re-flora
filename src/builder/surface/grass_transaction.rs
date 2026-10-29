//! Optimistic ordinary-grass transactions: private inputs/output, one bounded batch,
//! completion before readback, revision validation, then infallible all-chunk publication.
//! Rendering and other world producers continue using the old complete generation.
use super::occupancy_edit::{EditRequest, EditScratch};
use super::*;
use re_flora_vkn::Texture;

const MAX_TRANSACTION_CHUNKS: u32 = 8;

struct ChunkEdit {
    index: usize,
    source_instances: Buffer,
    source_revision: u64,
    source_potential: Buffer,
    potential_revision: u64,
    lengths: Vec<u32>,
    staged: FloraInstanceResources,
    scratch: EditScratch,
    job: Option<GpuJobToken>,
}

impl Drop for ChunkEdit {
    fn drop(&mut self) {
        // Cancellation, partial submit failure and shutdown cannot free GPU leases early.
        // Normal polling has already observed completion, so this does not block.
        if let Some(job) = self.job.take() {
            if let Err(error) = job.wait_complete() {
                log::error!("[GRASS_EDIT] retire failed: {error}");
            }
        }
    }
}

pub struct GrassEditTransaction {
    request: GrassPaintRequest,
    terrain_revision: u32,
    chunks: Vec<ChunkEdit>,
    submitted: Instant,
}

pub enum GrassEditCompletion {
    Stale(GrassPaintRequest),
    Published {
        retired: FrameRetirement,
        changed_chunks: usize,
        latency_ms: f64,
    },
}

impl GrassEditTransaction {
    pub fn is_ready(&self) -> Result<bool> {
        let mut ready = true;
        for chunk in &self.chunks {
            ready &= chunk.job.as_ref().unwrap().is_complete()?;
        }
        Ok(ready)
    }

    /// Explicit ordering boundary only (save/quit or bounded retry fallback).
    pub fn wait(&self) -> Result<()> {
        for chunk in &self.chunks {
            chunk.job.as_ref().unwrap().wait()?;
        }
        Ok(())
    }
}

impl SurfaceBuilder {
    pub fn supports_grass_transaction(&self, request: &GrassPaintRequest) -> bool {
        let min = request.voxel_bound.min() / self.voxel_dim_per_chunk;
        let max = request.voxel_bound.max() / self.voxel_dim_per_chunk;
        matches!(
            request.selection,
            species::FloraPaintSelection::GrassMix | species::FloraPaintSelection::Species(0 | 1)
        ) && request.voxel_bound.has_size()
            && self.chunk_bound.in_bound(min)
            && self.chunk_bound.in_bound(max)
            && (max - min + UVec3::ONE).element_product() <= MAX_TRANSACTION_CHUNKS
    }

    /// `terrain_revision` comes from the complete Visible Terrain Publication owner.
    /// Submit only between world publications; material writes and this job use the
    /// ordered general queue. Moisture/coverage are deliberately not geometry epochs.
    pub fn submit_grass_transaction(
        &mut self,
        request: GrassPaintRequest,
        terrain_revision: u32,
    ) -> Result<GrassEditTransaction> {
        anyhow::ensure!(
            self.supports_grass_transaction(&request),
            "unsupported grass transaction extent/selection"
        );
        let device = self.vulkan_ctx.device().clone();
        let allocator = self.resources.surface.get_image().get_allocator().clone();
        let images = self
            .grass_edit_images
            .get_or_insert_with(|| {
                let image = || {
                    Texture::new(
                        device.clone(),
                        allocator.clone(),
                        self.resources.surface.get_image().get_desc(),
                        &Default::default(),
                    )
                };
                (image(), image())
            })
            .clone();
        let min = request.voxel_bound.min() / self.voxel_dim_per_chunk;
        let max = request.voxel_bound.max() / self.voxel_dim_per_chunk;
        let mut chunks = Vec::new();
        // Allocate/copy all inputs before any submission. Snapshot buffers are private:
        // later CPU appends/hydration/growth-potential updates cannot race GPU reads.
        for x in min.x..=max.x {
            for y in min.y..=max.y {
                for z in min.z..=max.z {
                    let id = UVec3::new(x, y, z);
                    let index = self.get_chunk_resource_index(id)?;
                    let live = &self.resources.instances.chunk_flora_instances[index].1;
                    let mut staged = FloraInstanceResources::new(
                        device.clone(),
                        allocator.clone(),
                        id,
                        self.voxel_dim_per_chunk,
                    );
                    let lengths: Vec<_> = (0..species::species_count())
                        .map(|s| live.species_len(s))
                        .collect();
                    for (s, &count) in lengths.iter().enumerate() {
                        if count > 0 {
                            let offset = u64::from(FloraInstanceResources::species_offset(s)) * 8;
                            let bytes = if s < 2 {
                                bytemuck::cast_slice(&live.read_grass_instances(s)?).to_vec()
                            } else {
                                live.resource
                                    .instances_buf
                                    .read_back_range(offset, u64::from(count) * 8)?
                            };
                            staged
                                .resource
                                .instances_buf
                                .fill_range_with_raw_u8(offset, &bytes)?;
                        }
                        staged.set_species_len(s, count);
                    }
                    staged
                        .publish_grass_growth_potential(live.grass_growth_potential_cpu.clone())?;
                    chunks.push(ChunkEdit {
                        index,
                        source_instances: (*live.resource.instances_buf).clone(),
                        source_revision: live.resource.instances_buf.write_revision(),
                        source_potential: live.grass_growth_potential_levels.clone(),
                        potential_revision: live.grass_growth_potential_levels.write_revision(),
                        lengths,
                        staged,
                        scratch: EditScratch::private(
                            &self.resources,
                            &images,
                            &device,
                            &allocator,
                        ),
                        job: None,
                    });
                }
            }
        }
        for chunk in &mut chunks {
            let edit = EditRequest {
                chunk_id: chunk.staged.chunk_id,
                edit_start: request.start,
                edit_end: request.end,
                edit_radius: request.radius,
                flora_tick: request.tick,
                spawn_time_ms: request.spawn_time_ms,
                target_age: 0,
                paint_selection: request.selection,
                paint_dab_serial: request.dab_serial,
                paint_brush: request.brush,
                mode: OccupancyEditMode::Add,
            };
            chunk.job =
                Some(self.record_occupancy_edit(&edit, &chunk.scratch, &chunk.staged, false)?);
        }
        Ok(GrassEditTransaction {
            request,
            terrain_revision,
            chunks,
            submitted: Instant::now(),
        })
    }

    pub fn finish_grass_transaction(
        &mut self,
        mut transaction: GrassEditTransaction,
        terrain_revision: u32,
    ) -> Result<GrassEditCompletion> {
        anyhow::ensure!(transaction.is_ready()?, "grass transaction is not ready");
        // The world publication owner supplies its material/geometry revision.
        // Atlas moisture and soil-cover writes are not terrain geometry changes.
        let valid = transaction.terrain_revision == terrain_revision
            && transaction.chunks.iter().all(|chunk| {
                let live = &self.resources.instances.chunk_flora_instances[chunk.index].1;
                live.resource.instances_buf.as_raw() == chunk.source_instances.as_raw()
                    && live.resource.instances_buf.write_revision() == chunk.source_revision
                    && live.grass_growth_potential_levels.as_raw()
                        == chunk.source_potential.as_raw()
                    && live.grass_growth_potential_levels.write_revision()
                        == chunk.potential_revision
                    && chunk
                        .lengths
                        .iter()
                        .enumerate()
                        .all(|(s, &n)| live.species_len(s) == n)
            });
        if !valid {
            return Ok(GrassEditCompletion::Stale(transaction.request));
        }
        let mut changed = Vec::new();
        // All fallible readback / hydration uploads finish before any visible swap.
        // Hydration is reconciled against CURRENT water stores, not the submit snapshot.
        for (i, chunk) in transaction.chunks.iter_mut().enumerate() {
            let dispatch = chunk.scratch.occupancy_edit_dispatch.read_back()?;
            if u32::from_ne_bytes(dispatch[..4].try_into().unwrap()) == 0 {
                continue;
            }
            let result = get_occupancy_to_instances_result(
                &chunk.scratch.occupancy_to_instances_result,
                self.flora_species_count,
            );
            let live = &self.resources.instances.chunk_flora_instances[chunk.index].1;
            for s in 0..2 {
                anyhow::ensure!(
                    result.flora_instance_len[s] <= MAX_FLORA_INSTANCES_PER_SPECIES,
                    "grass output exceeds capacity"
                );
                chunk
                    .staged
                    .set_species_len(s, result.flora_instance_len[s]);
            }
            chunk.staged.hydration = live.hydration.clone();
            chunk.staged.soil_cover_roots = live.soil_cover_roots.clone();
            for s in 2..species::species_count() {
                chunk.staged.upload_hydration(s)?;
            }
            chunk.staged.reconcile_grass_hydration()?;
            changed.push(i);
        }
        let changed_chunks = changed.len();
        let mut retired = Vec::new();
        for i in changed {
            let chunk = &mut transaction.chunks[i];
            let live = &mut self.resources.instances.chunk_flora_instances[chunk.index].1;
            retired.push((
                live.resource.instances_buf.clone(),
                live.plant_hydration.clone(),
            ));
            std::mem::swap(
                &mut live.resource.instances_buf,
                &mut chunk.staged.resource.instances_buf,
            );
            // Publish the CPU observation with its draw stream, never with the
            // previous generation. The staged copy completed in the same job.
            std::mem::swap(
                &mut live.grass_instances_readback,
                &mut chunk.staged.grass_instances_readback,
            );
            std::mem::swap(&mut live.plant_hydration, &mut chunk.staged.plant_hydration);
            for s in 0..2 {
                live.set_species_len(s, chunk.staged.species_len(s));
                std::mem::swap(&mut live.hydration[s], &mut chunk.staged.hydration[s]);
                std::mem::swap(
                    &mut live.soil_cover_roots[s],
                    &mut chunk.staged.soil_cover_roots[s],
                );
            }
            live.soil_cover_changed_roots
                .extend(chunk.staged.soil_cover_changed_roots.iter().copied());
            live.hydration_roots_dirty = true;
            live.grass_audio.shape_changed();
        }
        Ok(GrassEditCompletion::Published {
            retired: FrameRetirement::new("grass.edit.publication", 0, retired),
            changed_chunks,
            latency_ms: transaction.submitted.elapsed().as_secs_f64() * 1000.,
        })
    }
}
