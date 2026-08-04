use crate::{ddgi::DdgiFieldIdentity, geom::UAabb3, resource::Resource};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, CommandBuffer, Device, MemoryLocation};
use resource_container_derive::ResourceContainer;

pub const TERRAIN_LIGHTING_CACHE_CAPACITY: u32 = 1 << 20;
const TERRAIN_LIGHTING_CACHE_METADATA_BYTES_PER_ENTRY: u64 = 16;
const TERRAIN_LIGHTING_CACHE_IRRADIANCE_BYTES_PER_ENTRY: u64 = 16;
const TERRAIN_LIGHTING_CACHE_REVISION_MAX: u32 = 0x7fff_ffff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainLightingCacheIdentity {
    pub published_field: Option<DdgiFieldIdentity>,
    pub environment_revision: u32,
    pub global_sky_revision: u32,
    pub consumer_visibility: u32,
    pub hard_origin: u32,
    pub invalidation_voxel_bound: Option<UAabb3>,
}

#[derive(ResourceContainer)]
pub struct TerrainLightingCache {
    pub terrain_ddgi_cache_metadata: Resource<Buffer>,
    pub terrain_ddgi_cache_irradiance: Resource<Buffer>,
    identity: Option<TerrainLightingCacheIdentity>,
    revision: u32,
    needs_clear: bool,
}

impl TerrainLightingCache {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let metadata_bytes = u64::from(TERRAIN_LIGHTING_CACHE_CAPACITY)
            * TERRAIN_LIGHTING_CACHE_METADATA_BYTES_PER_ENTRY;
        let irradiance_bytes = u64::from(TERRAIN_LIGHTING_CACHE_CAPACITY)
            * TERRAIN_LIGHTING_CACHE_IRRADIANCE_BYTES_PER_ENTRY;
        let metadata = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            metadata_bytes,
        );
        let irradiance = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::GpuOnly,
            irradiance_bytes,
        );
        log::info!(
            "[DDGI][TERRAIN_CACHE] allocated entries={} metadata_bytes={} irradiance_bytes={} total_mib={:.2}",
            TERRAIN_LIGHTING_CACHE_CAPACITY,
            metadata_bytes,
            irradiance_bytes,
            (metadata_bytes + irradiance_bytes) as f64 / (1024.0 * 1024.0),
        );
        Self {
            terrain_ddgi_cache_metadata: Resource::new(metadata),
            terrain_ddgi_cache_irradiance: Resource::new(irradiance),
            identity: None,
            revision: 0,
            needs_clear: true,
        }
    }

    pub fn observe(&mut self, identity: TerrainLightingCacheIdentity) -> u32 {
        if self.identity != Some(identity) {
            self.identity = Some(identity);
            let (revision, wrapped) = next_cache_revision(self.revision);
            self.revision = revision;
            if wrapped {
                self.needs_clear = true;
            }
        }
        self.revision.max(1)
    }

    pub fn force_clear_before_next_trace(&mut self) {
        self.identity = None;
        self.needs_clear = true;
    }

    pub fn record_clear_if_needed(&mut self, cmdbuf: &CommandBuffer) -> bool {
        if !std::mem::take(&mut self.needs_clear) {
            return false;
        }
        self.terrain_ddgi_cache_metadata.record_fill(
            cmdbuf,
            0,
            u64::from(TERRAIN_LIGHTING_CACHE_CAPACITY)
                * TERRAIN_LIGHTING_CACHE_METADATA_BYTES_PER_ENTRY,
            0,
        );
        log::info!(
            "[DDGI][TERRAIN_CACHE] cleared entries={} revision={}",
            TERRAIN_LIGHTING_CACHE_CAPACITY,
            self.revision,
        );
        true
    }
}

fn next_cache_revision(current: u32) -> (u32, bool) {
    if current >= TERRAIN_LIGHTING_CACHE_REVISION_MAX {
        (1, true)
    } else {
        (current + 1, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_revision_stays_nonzero_and_reports_wrap_for_a_required_clear() {
        assert_eq!(next_cache_revision(0), (1, false));
        assert_eq!(next_cache_revision(41), (42, false));
        assert_eq!(
            next_cache_revision(TERRAIN_LIGHTING_CACHE_REVISION_MAX),
            (1, true)
        );
    }

    #[test]
    fn shader_cache_uses_the_canonical_voxel_key_and_only_reuses_prior_frames() {
        let shader = include_str!("../../shader/slang/tracer.slang");

        assert!(shader.contains("voxelCoordinate.z << 18u"));
        assert!(shader.contains("packedKey + 1u"));
        assert!(shader.contains("metadata.ready_frame != env_info.frame_serial_idx"));
        assert!(shader.contains("environment_irradiance_capture_enabled == 0u"));
        assert!(shader.contains("environment_irradiance_capture_unpublished == 0u"));
        assert!(shader.contains("sampleDdgiDiffuseEnvironment("));
    }
}
