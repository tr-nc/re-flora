//! Frame-slot-owned GPU visibility. The source streams and simulation indices never move.
use super::{
    flora_frame_plan::{FloraFrameBatch, FloraFramePlan},
    stem_band_mesh,
};
mod validation;
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use std::sync::Arc;

// VkDrawIndexedIndirectCommand, followed by at most source_count original local indices.
pub(super) const COMMAND_WORDS: u32 = 5;

/// Diagnostic reference only; production always culls. No saved settings are changed.
pub(super) fn culling_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        let enabled = std::env::var("RE_FLORA_GRASS_VISIBILITY").as_deref() != Ok("0");
        log::info!("[GRASS_VISIBILITY] enabled={enabled} identity=source_index first_instance=0");
        enabled
    })
}

pub(super) fn validation_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("RE_FLORA_GRASS_VISIBILITY_AUDIT").is_some())
}

pub(super) fn readers() -> &'static [re_flora_vkn::BufferUse] {
    use re_flora_vkn::BufferUse::{HostRead, IndirectRead, ShaderRead};
    if validation_enabled() {
        &[ShaderRead, IndirectRead, HostRead]
    } else {
        &[ShaderRead, IndirectRead]
    }
}

#[derive(Default)]
pub(super) struct GrassVisibility {
    frames: Vec<Option<Frame>>,
}
struct Frame {
    buffer: Arc<Buffer>,
    capacity: u32,
    audit: Vec<FloraFrameBatch>,
}
impl GrassVisibility {
    /// Called only after the frame-slot fence has completed, like the pose cache.
    pub(super) fn prepare(
        &mut self,
        device: Device,
        allocator: Allocator,
        slot: usize,
        plan: &FloraFramePlan,
    ) -> Arc<Buffer> {
        self.frames
            .resize_with(self.frames.len().max(slot + 1), || None);
        let words = plan.visibility_words().max(1);
        if let Some(frame) = &self.frames[slot] {
            validation::check_completed(&frame.buffer, &frame.audit);
        }
        if self.frames[slot]
            .as_ref()
            .is_none_or(|frame| frame.capacity < words)
        {
            let capacity = words.next_power_of_two();
            self.frames[slot] = Some(Frame {
                buffer: Arc::new(Buffer::new_sized(
                    device,
                    allocator,
                    BufferUsage::from_flags(
                        vk::BufferUsageFlags::STORAGE_BUFFER
                            | vk::BufferUsageFlags::INDIRECT_BUFFER,
                    ),
                    MemoryLocation::CpuToGpu,
                    u64::from(capacity) * 4,
                )),
                capacity,
                audit: Vec::new(),
            });
        }
        let frame = self.frames[slot].as_mut().unwrap();
        if validation_enabled() && frame.audit.is_empty() {
            log::info!(
                "[GRASS_VISIBILITY_BUFFER] slot={slot} words={} memory={:?}",
                frame.capacity,
                frame.buffer.memory_properties()
            );
        }
        if validation_enabled() {
            frame.audit = plan.batches().to_vec();
        }
        let buffer = frame.buffer.clone();
        for batch in plan.batches() {
            // FirstInstance remains zero: no optional drawIndirectFirstInstance feature.
            let command = [
                stem_band_mesh::grass_index_count(batch.mesh_voxel_count()),
                0,
                0,
                0,
                0,
            ];
            buffer
                .fill_range_with_raw_u8(
                    u64::from(batch.visibility_offset()) * 4,
                    bytemuck::cast_slice(&command),
                )
                .expect("reset completed-slot grass indirect command");
        }
        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geom::Aabb3, tracer::flora_frame_plan::FloraFramePlanConfig};
    use glam::{Mat4, Vec3};

    #[test]
    fn commands_and_source_indices_have_disjoint_capacity_even_for_single_roots() {
        let plan = FloraFramePlan::build(
            FloraFramePlanConfig {
                camera_position: Vec3::ZERO,
                view_projection: Mat4::IDENTITY,
                lod_distance: 1.,
                draw_distance: 10.,
                chunk_count: 3,
                species_count: 2,
                max_cache_entries: 1 << 22,
            },
            |_| Aabb3::new(Vec3::ZERO, Vec3::splat(0.5)),
            |chunk, species| [[1, 0], [0, 1], [40000, 40000]][chunk][species],
            |_| true,
            |species, _| if species == 0 { 8 } else { 4 },
        );
        let mut end = 0;
        for batch in plan.batches() {
            assert_eq!(batch.visibility_offset(), end);
            end += COMMAND_WORDS + batch.instance_count();
        }
        assert_eq!(end, plan.visibility_words());
        assert_eq!(end, 80002 + 4 * COMMAND_WORDS);
        assert_eq!(
            COMMAND_WORDS as usize * 4,
            std::mem::size_of::<vk::DrawIndexedIndirectCommand>()
        );
        assert_eq!(FloraFramePlan::default().visibility_words(), 0);
    }
}
