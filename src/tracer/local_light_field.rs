//! Frame-slot-owned source-space terrain visibility certificates. Independent
//! of DDGI density/history; live trees and uncertain connections remain exact.
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, Device, MemoryLocation,
};
use std::sync::Arc;

pub(super) type CacheKey = (u64, u32);
const CELLS_PER_LIGHT: u64 = 24 * 6 * 32 * 32;
const AUDIT_BYTES: u64 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConsumerMode {
    Original,
    Shared,
    Ris,
    Audit,
}

impl ConsumerMode {
    pub fn primary(shared: bool, ris: bool) -> Self {
        if ris {
            Self::Ris
        } else if shared {
            Self::Shared
        } else {
            Self::Original
        }
    }
}

fn record_count(lights: u32, max_range: u64) -> Option<u32> {
    let records = u64::from(lights)
        .checked_mul(CELLS_PER_LIGHT)?
        .checked_add(2)?;
    (records.checked_mul(16)? <= max_range)
        .then(|| u32::try_from(records).ok())
        .flatten()
}

#[derive(Default)]
pub(super) struct LocalLightField {
    frames: Vec<Option<Frame>>,
    audits: Vec<Option<AuditFrame>>,
    last_audit: Option<CacheKey>,
    audit_clock: u64,
}
struct Frame {
    buffer: Arc<Buffer>,
    records: u32,
    key: Option<CacheKey>,
    next_cell: u32,
}
struct AuditFrame {
    gpu: Arc<Buffer>,
    readback: Buffer,
    pending: Option<CacheKey>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct FieldPush {
    pub first_cell: u32,
    pub total_cells: u32,
    pub light_count: u32,
    pub geometry_revision: u32,
    pub ready: u32,
}

fn next_build_batch(next: &mut u32, total: u32) -> Option<(u32, u32, bool)> {
    if *next >= total {
        return None;
    }
    let first = *next;
    *next = next.saturating_add(8192).min(total);
    Some((first, *next - first, *next == total))
}

impl LocalLightField {
    pub fn audit_enabled() -> bool {
        std::env::var_os("RE_FLORA_LOCAL_LIGHT_FIELD_AUDIT").is_some()
    }

    // Called only after waiting for this frame slot. Unsupported device ranges
    // get a one-record descriptor: shader header validation selects original
    // transport, never partial light lists or out-of-range reads.
    pub fn ensure(
        &mut self,
        device: Device,
        allocator: Allocator,
        slot: usize,
        lights: u32,
        key: Option<CacheKey>,
        max_range: u64,
    ) -> (Arc<Buffer>, Option<(u32, u32, bool)>) {
        let records = key
            .and_then(|_| record_count(lights, max_range))
            .unwrap_or(1);
        self.frames
            .resize_with(self.frames.len().max(slot + 1), || None);
        if self.frames[slot]
            .as_ref()
            .is_none_or(|f| f.records != records)
        {
            log::info!("[LOCAL_VISIBILITY] allocate slot={slot} lights={lights} records={records} bytes={} supported={}",
                u64::from(records)*16, records>=2);
            self.frames[slot] = Some(Frame {
                buffer: Arc::new(Buffer::new_sized(
                    device,
                    allocator,
                    BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                    MemoryLocation::GpuOnly,
                    u64::from(records) * 16,
                )),
                records,
                key: None,
                next_cell: 0,
            });
        }
        let frame = self.frames[slot].as_mut().unwrap();
        if frame.key != key {
            frame.key = key;
            frame.next_cell = 0;
        }
        let batch = if records >= 2 {
            next_build_batch(&mut frame.next_cell, Self::cell_count(lights))
        } else {
            None
        };
        (frame.buffer.clone(), batch)
    }

    pub fn cell_count(lights: u32) -> u32 {
        u32::try_from(u64::from(lights) * CELLS_PER_LIGHT)
            .expect("validated visibility buffer layout")
            .max(1)
    }

    pub fn prepare_audit(
        &mut self,
        device: Device,
        allocator: Allocator,
        slot: usize,
        key: Option<CacheKey>,
        cmdbuf: &CommandBuffer,
    ) -> Arc<Buffer> {
        self.audit_clock = self.audit_clock.wrapping_add(1);
        self.audits
            .resize_with(self.audits.len().max(slot + 1), || None);
        let frame = self.audits[slot].get_or_insert_with(|| AuditFrame {
            gpu: Arc::new(Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
                ),
                MemoryLocation::GpuOnly,
                AUDIT_BYTES,
            )),
            readback: Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
                MemoryLocation::GpuToCpu,
                AUDIT_BYTES,
            ),
            pending: None,
        });
        if let Some(previous) = frame.pending.take() {
            let bytes = frame
                .readback
                .read_back()
                .expect("visibility receiver audit");
            let v: [u32; 16] = bytemuck::pod_read_unaligned(&bytes[..64]);
            log::info!("[LOCAL_VISIBILITY][RECEIVERS] key={previous:?} active_taps={} clear={} opaque={} fallback={} reference_nonzero={} shared_nonzero={} failures={} max_transmission_error={} medium_fallback={} false_clear={} false_opaque={} reference_direct_luma={} shared_direct_luma={} shared_luma_fraction={} rendered_failures={} rendered_max_rgb_error={}",
                v[0],v[1],v[2],v[3],v[4],v[5],v[6],f32::from_bits(v[7]),v[8],v[9],v[10],
                f32::from_bits(v[11]),f32::from_bits(v[12]),
                if v[11]==0 {0.0} else {f32::from_bits(v[12])/f32::from_bits(v[11])}, v[13], f32::from_bits(v[14]));
            assert_eq!(
                v[13], 0,
                "normal grass program differs from same-frame audited output"
            );
            assert!(
                f32::from_bits(v[11]).is_finite() && f32::from_bits(v[12]).is_finite(),
                "visibility audit energy accumulation must stay finite"
            );
            assert_eq!(
                v[6], 0,
                "visibility certificate differs from original receiver transport"
            );
        }
        frame.gpu.record_fill(cmdbuf, 0, AUDIT_BYTES, 0);
        if key.is_some() && (key != self.last_audit || self.audit_clock % 120 == 0) {
            self.last_audit = key;
            frame.pending = key;
            frame.gpu.record_fill(cmdbuf, 60, 4, 1);
        }
        frame.gpu.clone()
    }

    pub fn audit_pending(&self, slot: usize) -> bool {
        self.audits
            .get(slot)
            .and_then(Option::as_ref)
            .is_some_and(|frame| frame.pending.is_some())
    }

    pub fn record_audit_readback(&self, cmdbuf: &CommandBuffer, slot: usize) {
        let Some(frame) = self.audits.get(slot).and_then(Option::as_ref) else {
            return;
        };
        if frame.pending.is_some() {
            frame
                .gpu
                .record_copy_to_buffer(cmdbuf, &frame.readback, AUDIT_BYTES, 0, 0);
            cmdbuf.use_buffer(&frame.readback, BufferUse::HostRead);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primary_renderer_never_selects_the_audit_program() {
        assert_eq!(ConsumerMode::primary(false, false), ConsumerMode::Original);
        assert_eq!(ConsumerMode::primary(true, false), ConsumerMode::Shared);
        for shared in [false, true] {
            assert_eq!(ConsumerMode::primary(shared, true), ConsumerMode::Ris);
            for ris in [false, true] {
                assert_ne!(ConsumerMode::primary(shared, ris), ConsumerMode::Audit);
            }
        }
    }

    #[test]
    fn bounded_build_covers_every_cell_before_publication() {
        let total = LocalLightField::cell_count(8);
        let mut next = 0;
        let mut covered = 0;
        while let Some((first, count, ready)) = next_build_batch(&mut next, total) {
            assert_eq!(first, covered);
            assert!((1..=8192).contains(&count));
            covered += count;
            assert_eq!(ready, covered == total);
        }
        assert_eq!(covered, total);
        assert_eq!(next_build_batch(&mut next, total), None);
        next = 0; // new live-light/geometry identity restarts, never mixes fields
        assert_eq!(next_build_batch(&mut next, total), Some((0, 8192, false)));
        assert_eq!(next_build_batch(&mut 0, 1), Some((0, 1, true)));
    }
    #[test]
    fn certificate_layout_covers_all_selected_lights_and_device_limits() {
        assert_eq!(record_count(8, u64::MAX), Some(1_179_650));
        assert_eq!(record_count(0, 32), Some(2));
        assert_eq!(record_count(8, 18_874_399), None);
        assert_eq!(record_count(u32::MAX, u64::MAX), None);
        assert!(record_count(9, u64::MAX).unwrap() > record_count(8, u64::MAX).unwrap());
    }
}
