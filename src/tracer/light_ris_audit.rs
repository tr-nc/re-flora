//! Opt-in, separate-program evidence from real grass receivers. Never timed as
//! production and never substituted for the normal RIS output.
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, Device, MemoryLocation,
};
use std::sync::Arc;
mod temporal;
use temporal::{TemporalAudit, BUFFER_BYTES, HEADER_WORDS, MAX_RECORDS, RECORD_WORDS};

#[derive(Default)]
pub(super) struct LightRisAudit {
    frames: Vec<Option<Frame>>,
    temporal: TemporalAudit,
}
struct Frame {
    gpu: Arc<Buffer>,
    readback: Buffer,
    pending: bool,
}
impl LightRisAudit {
    pub fn enabled() -> bool {
        std::env::var_os("RE_FLORA_LIGHT_RIS_AUDIT").is_some() || Self::temporal_enabled()
    }
    fn temporal_enabled() -> bool {
        std::env::var_os("RE_FLORA_LIGHT_RIS_TEMPORAL_AUDIT").is_some()
    }
    // Called after the owning frame-slot fence, just like the output cache.
    pub fn prepare(
        &mut self,
        device: Device,
        allocator: Allocator,
        slot: usize,
        cmdbuf: &CommandBuffer,
    ) -> Arc<Buffer> {
        self.frames
            .resize_with(self.frames.len().max(slot + 1), || None);
        let frame = self.frames[slot].get_or_insert_with(|| Frame {
            gpu: Arc::new(Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
                ),
                MemoryLocation::GpuOnly,
                BUFFER_BYTES,
            )),
            readback: Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
                MemoryLocation::GpuToCpu,
                BUFFER_BYTES,
            ),
            pending: false,
        });
        if frame.pending {
            let data = frame.readback.read_back().expect("RIS audit readback");
            let v: [u32; 16] = bytemuck::pod_read_unaligned(&data[..64]);
            if Self::temporal_enabled() {
                assert!((v[12] as usize) <= MAX_RECORDS, "temporal audit overflow");
                let words: Vec<u32> = data
                    .chunks_exact(4)
                    .map(bytemuck::pod_read_unaligned)
                    .collect();
                let end = HEADER_WORDS + v[12] as usize * RECORD_WORDS;
                let observation = self.temporal.observe(&words[HEADER_WORDS..end]);
                log::info!("[LIGHT_RIS][TEMPORAL] records={} matched={} stationary={} lit_stationary={} changed={} pool_changed={} max_relative_delta={} mean_error_delta={} lit_matched={} moving_lit={} max_moving_error_delta={} mean_lit_error_delta={} max_moving_total_error_delta={}",
                    v[12], observation.matched, observation.stationary, observation.lit_stationary,
                    observation.changed, observation.pool_changed, observation.max_relative_delta,
                    observation.error_delta_sum / observation.matched.max(1) as f32,
                    observation.lit_matched, observation.moving_lit, observation.max_moving_error_delta,
                    observation.lit_error_delta_sum / observation.lit_matched.max(1) as f32,
                    observation.max_moving_total_error_delta);
            }
            let f = |i| f32::from_bits(v[i]);
            log::info!("[LIGHT_RIS][RECEIVERS] samples={} non_finite={} rendered_mismatches={} reference_nonzero={} ris_nonzero={} reference_rgb={:?} ris_rgb={:?} sum_abs_luma_error={} max_rendered_error={} positive_proposals={} selected={}",
                v[0],v[1],v[2],v[3],v[13],[f(4),f(5),f(6)],[f(7),f(8),f(9)],f(10),f(11),v[14],v[15]);
            assert_eq!(v[1], 0, "RIS must produce finite nonnegative irradiance");
            assert!(
                v[15] <= v[0] * 8,
                "RIS exceeded the maximum receiver ray budget"
            );
            assert!(
                v[14] == 0 || v[15] > 0,
                "positive proposals must reach visibility selection"
            );
            assert_eq!(
                v[2], 0,
                "audited RIS must match the real normal-program output"
            );
        }
        frame.gpu.record_fill(cmdbuf, 0, BUFFER_BYTES, 0);
        frame
            .gpu
            .record_fill(cmdbuf, 64, 4, u32::from(Self::temporal_enabled()));
        frame.pending = true;
        frame.gpu.clone()
    }
    pub fn record_readback(&self, cmdbuf: &CommandBuffer, slot: usize) {
        let frame = self.frames[slot].as_ref().unwrap();
        frame
            .gpu
            .record_copy_to_buffer(cmdbuf, &frame.readback, BUFFER_BYTES, 0, 0);
        cmdbuf.use_buffer(&frame.readback, BufferUse::HostRead);
    }
}
