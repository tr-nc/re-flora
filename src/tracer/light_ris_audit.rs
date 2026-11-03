//! Opt-in, separate-program evidence from real grass receivers. Never timed as
//! production and never substituted for the normal RIS output.
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, Device, MemoryLocation,
};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct LightRisAudit {
    frames: Vec<Option<Frame>>,
}
struct Frame {
    gpu: Arc<Buffer>,
    readback: Buffer,
    pending: bool,
}
impl LightRisAudit {
    pub fn enabled() -> bool {
        std::env::var_os("RE_FLORA_LIGHT_RIS_AUDIT").is_some()
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
                64,
            )),
            readback: Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
                MemoryLocation::GpuToCpu,
                64,
            ),
            pending: false,
        });
        if frame.pending {
            let data = frame.readback.read_back().expect("RIS audit readback");
            let v: [u32; 16] = bytemuck::pod_read_unaligned(&data[..64]);
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
        frame.gpu.record_fill(cmdbuf, 0, 64, 0);
        frame.pending = true;
        frame.gpu.clone()
    }
    pub fn record_readback(&self, cmdbuf: &CommandBuffer, slot: usize) {
        let frame = self.frames[slot].as_ref().unwrap();
        frame
            .gpu
            .record_copy_to_buffer(cmdbuf, &frame.readback, 64, 0, 0);
        cmdbuf.use_buffer(&frame.readback, BufferUse::HostRead);
    }
}
