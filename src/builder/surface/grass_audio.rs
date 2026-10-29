//! Bounded acoustic projection of committed draw streams, owned with vegetation.
use super::{FloraInstanceResources, Instance, SurfaceBuilder};
use crate::flora::grass_acoustics::{visible_height, GrassPatch, PatchBuilder, MAX_CANDIDATES};
use anyhow::Result;
use glam::{UVec3, Vec3};
use std::{cell::Cell, sync::Arc};

#[derive(Default)]
pub(super) struct GrassAudioCache {
    pub structure: u64,
    pub shape: Cell<u64>,
    pub pending_builds: usize,
    built_structure: u64,
    built_shape: u64,
    growth_override: Option<f32>,
    initialized: bool,
    last_refresh: f64,
    patches: Vec<Arc<GrassPatch>>,
}
impl GrassAudioCache {
    pub fn shape_changed(&self) {
        self.shape.set(self.shape.get().wrapping_add(1));
    }
    fn valid(&self, growth_override: Option<f32>) -> bool {
        self.initialized
            && self.pending_builds == 0
            && self.structure == self.built_structure
            && growth_override == self.growth_override
    }
    fn dirty(&self, growth_override: Option<f32>) -> bool {
        !self.valid(growth_override) || self.shape.get() != self.built_shape
    }
}

impl FloraInstanceResources {
    fn rebuild_grass_audio(&mut self, growth_override: Option<f32>, now: f64) -> Result<()> {
        let start = std::time::Instant::now();
        // Every GPU writer transitions to HostRead and completes its fence before
        // publishing revisions. An outstanding async surface job is excluded.
        anyhow::ensure!(
            self.grass_audio.pending_builds == 0,
            "grass stream is not committed"
        );
        anyhow::ensure!(
            self.resource
                .instances_buf
                .memory_properties()
                .contains(re_flora_vkn::vk::MemoryPropertyFlags::HOST_COHERENT),
            "grass acoustic readback requires coherent mapped instance memory"
        );
        let mut builder = PatchBuilder::default();
        let has_roots = (0..2).any(|s| self.species_len(s) > 0);
        let potential = if has_roots {
            self.grass_growth_potential_levels.read_back()?
        } else {
            Vec::new()
        };
        let mut roots = 0;
        for species in 0..2 {
            let count = self.species_len(species) as usize;
            if count == 0 {
                continue;
            }
            anyhow::ensure!(
                self.hydration[species].len() == count,
                "uncommitted grass hydration"
            );
            let bytes = self.resource.instances_buf.read_back_range(
                u64::from(Self::species_offset(species)) * 8,
                count as u64 * 8,
            )?;
            for (i, bytes) in bytes.chunks_exact(8).enumerate() {
                let instance: Instance = bytemuck::pod_read_unaligned(bytes);
                let packed = instance.packed_local_pos;
                let local = UVec3::new(packed & 255, (packed >> 8) & 255, (packed >> 16) & 255);
                let linear = (local.x + 256 * (local.y + 256 * local.z)) as usize;
                let level = (potential[linear / 2] >> ((linear % 2) * 4)) & 15;
                let (root, hydration) = self.hydration[species][i];
                anyhow::ensure!(
                    root == packed & 0xffffff,
                    "grass draw / hydration identity mismatch"
                );
                let world = local + self.chunk_world_offset;
                builder.add(
                    world,
                    visible_height(
                        world,
                        species == 1,
                        packed >> 24,
                        level as f32 / 15.0,
                        hydration,
                        growth_override,
                    ),
                );
            }
            roots += count;
        }
        let cache = &mut self.grass_audio;
        cache.patches = builder.finish();
        cache.built_structure = cache.structure;
        cache.built_shape = cache.shape.get();
        cache.growth_override = growth_override;
        cache.initialized = true;
        cache.last_refresh = now;
        log::debug!(
            "[GRASS_AUDIO_PROJECTION] chunk={:?} roots={roots} patches={} cpu_us={:.0}",
            self.chunk_id,
            cache.patches.len(),
            start.elapsed().as_secs_f64() * 1e6
        );
        Ok(())
    }
}

impl SurfaceBuilder {
    /// Called before recording rendering, after vegetation commits. At most one
    /// chunk (2 × 40,000 instances) is rebuilt per frame. Soft updates are at most
    /// 10 Hz per chunk; structural invalidation bypasses that throttle.
    /// Soft growth/hydration changes retain their previous projection until served;
    /// structural changes never expose stale coverage. Oldest eligible first avoids
    /// a frequently edited near chunk starving the rest of the listening region.
    pub(crate) fn grass_audio_patches(
        &mut self,
        listener: Vec3,
        radius: f32,
        growth_override: Option<f32>,
        now: f64,
    ) -> Result<Vec<Arc<GrassPatch>>> {
        if !listener.is_finite() || !now.is_finite() || !radius.is_finite() {
            return Ok(Vec::new());
        }
        let radius = radius.clamp(0.25, 2.0);
        let growth_override = growth_override
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.0, 1.0));
        let min = ((listener - Vec3::splat(radius + 0.25))
            .floor()
            .max(Vec3::ZERO))
        .as_uvec3();
        let max = ((listener + Vec3::splat(radius + 0.25))
            .floor()
            .max(Vec3::ZERO))
        .as_uvec3();
        let mut chunks = Vec::new();
        // 256-voxel chunks = one world unit. The bounded range visits at most 6³ chunks.
        for z in min.z..=max.z {
            for y in min.y..=max.y {
                for x in min.x..=max.x {
                    let id = UVec3::new(x, y, z);
                    if self.chunk_bound.in_bound(id) {
                        chunks.push(self.get_chunk_resource_index(id)?);
                    }
                }
            }
        }
        let next = chunks
            .iter()
            .copied()
            .filter(|&i| {
                let c = &self.resources.instances.chunk_flora_instances[i]
                    .1
                    .grass_audio;
                c.pending_builds == 0
                    && c.dirty(growth_override)
                    && (!c.valid(growth_override) || now - c.last_refresh >= 0.1)
            })
            .min_by(|&a, &b| {
                let a = &self.resources.instances.chunk_flora_instances[a]
                    .1
                    .grass_audio;
                let b = &self.resources.instances.chunk_flora_instances[b]
                    .1
                    .grass_audio;
                a.last_refresh.total_cmp(&b.last_refresh)
            });
        if let Some(index) = next {
            self.resources.instances.chunk_flora_instances[index]
                .1
                .rebuild_grass_audio(growth_override, now)?;
        }
        let mut patches: Vec<_> = chunks
            .into_iter()
            .flat_map(|i| {
                let c = &self.resources.instances.chunk_flora_instances[i]
                    .1
                    .grass_audio;
                c.patches.iter().filter(move |_| c.valid(growth_override))
            })
            .filter(|p| {
                p.samples
                    .iter()
                    .any(|s| s.position.distance_squared(listener) < radius * radius)
            })
            .cloned()
            .collect();
        patches.sort_by(|a, b| {
            a.center()
                .distance_squared(listener)
                .total_cmp(&b.center().distance_squared(listener))
                .then(a.id.cmp(&b.id))
        });
        patches.truncate(MAX_CANDIDATES);
        Ok(patches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_committed_structure_survives_bounded_soft_refresh() {
        let mut c = GrassAudioCache::default();
        assert!(!c.valid(None));
        c.initialized = true;
        assert!(c.valid(None));
        c.shape_changed();
        assert!(c.valid(None));
        assert!(c.dirty(None));
        c.structure += 1;
        assert!(!c.valid(None));
        c.built_structure = c.structure;
        assert!(c.valid(None));
        c.pending_builds = 2;
        assert!(!c.valid(None));
        c.pending_builds -= 1;
        assert!(!c.valid(None));
        c.pending_builds -= 1;
        assert!(!c.valid(Some(0.0)));
    }
}
