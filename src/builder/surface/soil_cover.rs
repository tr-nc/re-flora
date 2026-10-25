//! Incremental ordinary-grass appearance. CPU tracks root identity deltas only;
//! GPU samples exposure and changes bit 6 without reading terrain back to the CPU.
use super::*;
use crate::{builder::PlainBuilder, resource::Resource};
use re_flora_vkn::{vk, BufferUsage, MemoryLocation, Texture};
use std::collections::HashSet;

const TILE: u32 = 16;

#[derive(Default)]
pub(super) struct SoilCover {
    roots: HashSet<UVec3>,
    tiles: HashMap<UVec3, HashSet<UVec3>>,
    pending: HashSet<UVec3>,
    pub dirty: bool,
    pub terrain_changes: Vec<UAabb3>,
    gpu: Option<CoverGpu>,
}

fn neighborhood(p: UVec3, extent: UVec3) -> impl Iterator<Item = UVec3> {
    (-1..=1).flat_map(move |z| {
        (-1..=1).filter_map(move |x| {
            let q = p.as_ivec3() + IVec3::new(x, 0, z);
            (q.cmpge(IVec3::ZERO).all() && q.cmplt(extent.as_ivec3()).all()).then_some(q.as_uvec3())
        })
    })
}

impl SoilCover {
    fn root_changed(&mut self, p: UVec3, present: bool, extent: UVec3) {
        let changed = if present {
            self.roots.insert(p)
        } else {
            self.roots.remove(&p)
        };
        if !changed {
            return;
        }
        let tile = p / TILE;
        if present {
            self.tiles.entry(tile).or_default().insert(p);
        } else if let Some(roots) = self.tiles.get_mut(&tile) {
            roots.remove(&p);
            if roots.is_empty() {
                self.tiles.remove(&tile);
            }
        }
        self.pending.extend(neighborhood(p, extent));
    }

    fn invalidate_terrain(&mut self, bounds: UAabb3, extent: UVec3) {
        // Terrain changes may affect a destination, root support or its headroom.
        // Include nearby roots, but never scan unrelated parts of the garden.
        let min = bounds.min().saturating_sub(UVec3::ONE);
        let max = (bounds.max() + UVec3::new(1, 0, 1)).min(extent - UVec3::ONE);
        let first = min / TILE;
        let last = max / TILE;
        for z in first.z..=last.z {
            for y in first.y..=last.y {
                for x in first.x..=last.x {
                    if let Some(roots) = self.tiles.get(&UVec3::new(x, y, z)) {
                        for &root in roots {
                            if root.cmpge(min).all() && root.cmple(max).all() {
                                self.pending.extend(neighborhood(root, extent));
                            }
                        }
                    }
                }
            }
        }
    }

    fn patches(&self, extent: UVec3) -> Vec<[u32; 4]> {
        self.pending
            .iter()
            .map(|&p| {
                let mut mask = 0;
                for q in neighborhood(p, extent) {
                    if self.roots.contains(&q) {
                        let delta = q.as_ivec3() - p.as_ivec3();
                        mask |= 1 << ((delta.z + 1) * 3 + delta.x + 1);
                    }
                }
                [p.x, p.y, p.z, mask]
            })
            .collect()
    }
}

#[derive(resource_container_derive::ResourceContainer)]
struct CoverResources {
    soil_cover_patches: Resource<Buffer>,
    chunk_atlas: Resource<Texture>,
}
struct CoverGpu {
    pipeline: ComputePipeline,
    resources: CoverResources,
    _pool: DescriptorPool,
    generation: u64,
}
impl CoverGpu {
    fn buffer(ctx: &VulkanContext, atlas: &Texture, count: usize) -> Buffer {
        Buffer::new_sized(
            ctx.device().clone(),
            atlas.get_image().get_allocator().clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            (count.max(1024).next_power_of_two() * 16) as u64,
        )
    }
    fn new(ctx: &VulkanContext, atlas: &Texture, count: usize) -> Result<Self> {
        let sm = ShaderModule::from_precompiled(
            ctx.device(),
            "shader/builder/surface/soil_cover.comp",
            "main",
        )
        .map_err(anyhow::Error::msg)?;
        let pool = DescriptorPool::new(ctx.device())?;
        let resources = CoverResources {
            soil_cover_patches: Resource::new(Self::buffer(ctx, atlas, count)),
            chunk_atlas: Resource::new(atlas.clone()),
        };
        let pipeline = ComputePipeline::new(ctx.device(), &sm, &pool, &[&resources]);
        Ok(Self {
            pipeline,
            resources,
            _pool: pool,
            generation: 0,
        })
    }
    fn apply(&mut self, ctx: &VulkanContext, patches: &[[u32; 4]]) -> Result<()> {
        let bytes = bytemuck::cast_slice(patches);
        let retirement = if bytes.len() as u64 > self.resources.soil_cover_patches.get_size_bytes()
        {
            self.resources.soil_cover_patches = Resource::new(Self::buffer(
                ctx,
                &self.resources.chunk_atlas,
                patches.len(),
            ));
            self.generation += 1;
            Some(self.pipeline.publish_descriptors(
                "soil_cover.patches",
                self.generation,
                DescriptorUpdate::All(&[&self.resources]),
            )?)
        } else {
            None
        };
        self.resources
            .soil_cover_patches
            .fill_range_with_raw_u8(0, bytes)?;
        let cmdbuf = CommandBuffer::new(ctx.device(), ctx.command_pool());
        cmdbuf.begin(true);
        cmdbuf.use_buffer(&self.resources.soil_cover_patches, BufferUse::HostWrite);
        // Evaluate all exposure before any atlas writes. Reflected resource use
        // inserts the compute read/write barriers between phases and consumers.
        for phase in 0..2u32 {
            for offset in (0..patches.len()).step_by(65535 * 64) {
                let count = (patches.len() - offset).min(65535 * 64) as u32;
                let push = [count, phase, u32::try_from(offset)?, 0];
                self.pipeline.record(
                    &cmdbuf,
                    Extent3D::new(count, 1, 1),
                    Some(bytemuck::bytes_of(&push)),
                );
            }
        }
        cmdbuf.end();
        let mut job = cmdbuf.submit_gpu_job(&ctx.get_general_queue(), "surface.soil_cover")?;
        if let Some(retirement) = retirement {
            job.retain_frame_retirement(retirement);
        }
        let _completed = job.wait_complete()?;
        Ok(())
    }
}

impl SurfaceBuilder {
    pub fn refresh_grass_soil_cover(&mut self, plain: &mut PlainBuilder) -> Result<()> {
        let started = Instant::now();
        let extent = self.chunk_bound.max() * self.voxel_dim_per_chunk;
        for (_, r) in &mut self.resources.instances.chunk_flora_instances {
            for root in r.soil_cover_changed_roots.drain() {
                let p = r.chunk_world_offset
                    + UVec3::new(root & 255, (root >> 8) & 255, (root >> 16) & 255);
                let present = r.soil_cover_roots.iter().any(|roots| roots.contains(&root));
                self.soil_cover.root_changed(p, present, extent);
            }
        }
        if self.soil_cover.dirty {
            for &root in &self.soil_cover.roots {
                self.soil_cover.pending.extend(neighborhood(root, extent));
            }
            self.soil_cover.dirty = false;
        }
        for bounds in std::mem::take(&mut self.soil_cover.terrain_changes) {
            self.soil_cover.invalidate_terrain(bounds, extent);
        }
        if self.soil_cover.pending.is_empty() {
            return Ok(());
        }
        let patches = self.soil_cover.patches(extent);
        if self.soil_cover.gpu.is_none() {
            self.soil_cover.gpu = Some(CoverGpu::new(
                &self.vulkan_ctx,
                &plain.get_resources().chunk_atlas,
                patches.len(),
            )?);
        }
        self.soil_cover
            .gpu
            .as_mut()
            .unwrap()
            .apply(&self.vulkan_ctx, &patches)?;
        self.soil_cover.pending.clear();
        log::debug!("[SOIL_COVER] roots={} updated_cells={} uploaded_bytes={} readback_bytes=0 elapsed_ms={:.3}",
            self.soil_cover.roots.len(), patches.len(), patches.len()*16, started.elapsed().as_secs_f64()*1000.);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_root_delta_never_revisits_distant_roots() {
        let extent = UVec3::splat(512);
        let mut cover = SoilCover::default();
        for z in (1..512).step_by(2) {
            for x in (1..512).step_by(2) {
                cover.root_changed(UVec3::new(x, 32, z), true, extent);
            }
        }
        cover.pending.clear();
        cover.root_changed(UVec3::new(255, 32, 255), false, extent);
        assert_eq!(cover.pending.len(), 9);
        assert!(cover
            .pending
            .iter()
            .all(|p| p.y == 32 && (254..=256).contains(&p.x) && (254..=256).contains(&p.z)));
        cover.pending.clear();
        cover.root_changed(UVec3::new(255, 32, 255), false, extent);
        assert!(
            cover.pending.is_empty(),
            "unchanged publication must do no work"
        );
    }
    #[test]
    fn support_and_headroom_edits_invalidate_neighbors_across_tiles() {
        let extent = UVec3::splat(64);
        let mut cover = SoilCover::default();
        let root = UVec3::new(15, 15, 15);
        cover.root_changed(root, true, extent);
        cover.root_changed(UVec3::new(45, 15, 45), true, extent);
        cover.pending.clear();
        cover.invalidate_terrain(UAabb3::new(root + UVec3::Y, root + UVec3::Y), extent);
        assert_eq!(cover.pending.len(), 9);
        assert!(cover.pending.contains(&UVec3::new(16, 15, 16)));
        cover.pending.clear();
        cover.invalidate_terrain(UAabb3::new(UVec3::splat(60), UVec3::splat(61)), extent);
        assert!(cover.pending.is_empty());
    }
    #[test]
    fn root_membership_crosses_chunk_boundaries_without_wraparound() {
        let extent = UVec3::splat(512);
        let mut cover = SoilCover::default();
        cover.root_changed(UVec3::new(255, 32, 255), true, extent);
        cover.root_changed(UVec3::new(257, 32, 255), true, extent);
        let patches = cover.patches(extent);
        let middle = patches.iter().find(|p| p[..3] == [256, 32, 255]).unwrap();
        assert_eq!(middle[3], (1 << 3) | (1 << 5));
        assert_eq!(neighborhood(UVec3::ZERO, extent).count(), 4);
        assert_eq!(neighborhood(extent - UVec3::ONE, extent).count(), 4);
    }
}
