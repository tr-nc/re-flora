//! Frame-owned RGB connection transmission on the unchanged consumer DDGI grid.
//! Receiver attenuation/direction remain analytic. Slots rebuild from live lights
//! and geometry, never accumulating history or entering recursive transport.
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, Device, MemoryLocation,
    VulkanContext,
};
use std::sync::Arc;

pub(super) struct LocalLightField {
    frames: Vec<Option<(Arc<Buffer>, FieldLayout)>>,
    audits: Vec<Option<AuditFrame>>,
    last_audit: Option<AuditKey>,
    max_storage_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FieldLayout {
    probes: u32,
    lights: u32,
    bytes: u64,
}

impl FieldLayout {
    fn new(probes: u32, lights: u32, max_bytes: u64) -> Option<Self> {
        // Two uint2 header records, then 24 RGB16 transmission records per light.
        let records = u64::from(probes)
            .checked_mul(u64::from(lights))?
            .checked_mul(24)?
            .checked_add(2)?;
        let bytes = records.checked_mul(8)?;
        (probes > 0 && records <= u64::from(u32::MAX) && bytes <= max_bytes).then_some(Self {
            probes,
            lights,
            bytes,
        })
    }
}

pub(super) type AuditKey = (u64, u32, u64, bool, u32);
struct AuditFrame {
    gpu: Arc<Buffer>,
    readback: Buffer,
    pending: Option<AuditKey>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct FieldPush {
    pub first_probe: u32,
    pub probe_count: u32,
    pub light_count: u32,
    pub audit: u32,
}

impl LocalLightField {
    pub fn new(context: &VulkanContext) -> Self {
        let properties = unsafe {
            context
                .instance()
                .as_raw()
                .get_physical_device_properties(context.physical_device().as_raw())
        };
        Self {
            frames: Vec::new(),
            audits: Vec::new(),
            last_audit: None,
            max_storage_bytes: u64::from(properties.limits.max_storage_buffer_range),
        }
    }

    pub fn audit_enabled() -> bool {
        std::env::var_os("RE_FLORA_LOCAL_LIGHT_FIELD_AUDIT").is_some()
    }

    pub fn prepare_audit(
        &mut self,
        device: Device,
        allocator: Allocator,
        slot: usize,
        key: Option<AuditKey>,
    ) -> (Arc<Buffer>, bool) {
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
                48,
            )),
            readback: Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
                MemoryLocation::GpuToCpu,
                48,
            ),
            pending: None,
        });
        if let Some(previous) = frame.pending.take() {
            let bytes = frame
                .readback
                .read_back()
                .expect("direct field audit readback");
            let values: [u32; 12] = bytemuck::pod_read_unaligned(&bytes[..48]);
            log::info!("[LOCAL_LIGHT_FIELD][AUDIT] key={previous:?} samples={} failures={} max_absolute_error={} nonzero={}",
                values[0], values[1], f32::from_bits(values[2]), values[3]);
            log::info!("[LOCAL_LIGHT_FIELD][RECEIVERS] sampled_lights={} shared_nonzero={} original_nonzero={} shared_accepted={} original_queries={} errors_over_5pct={} max_absolute_error={} max_relative_error={}",
                values[4], values[5], values[6], values[10], values[11], values[7], f32::from_bits(values[8]), f32::from_bits(values[9]));
            assert_eq!(
                values[1], 0,
                "direct field differs from original probe-position shading"
            );
        }
        let audit = key.is_some() && key != self.last_audit;
        if audit {
            frame.pending = key;
            self.last_audit = key;
        }
        (frame.gpu.clone(), audit)
    }

    pub fn record_audit_readback(&self, cmdbuf: &CommandBuffer, slot: usize) {
        let frame = self.audits[slot].as_ref().unwrap();
        if frame.pending.is_some() {
            frame
                .gpu
                .record_copy_to_buffer(cmdbuf, &frame.readback, 48, 0, 0);
            cmdbuf.use_buffer(&frame.readback, BufferUse::HostRead);
        }
    }

    // The caller has waited for the slot fence. Unsupported storage ranges use
    // a descriptor-only buffer; its short header forces exact receiver fallback.
    pub fn ensure(
        &mut self,
        device: Device,
        allocator: Allocator,
        slot: usize,
        probe_count: u32,
        light_count: u32,
        enabled: bool,
    ) -> (Arc<Buffer>, bool) {
        self.frames
            .resize_with(self.frames.len().max(slot + 1), || None);
        let requested = enabled
            .then(|| FieldLayout::new(probe_count, light_count, self.max_storage_bytes))
            .flatten();
        let required = requested.unwrap_or(FieldLayout {
            probes: 0,
            lights: 0,
            bytes: 8,
        });
        if self.frames[slot]
            .as_ref()
            .is_none_or(|(_, old)| *old != required)
        {
            if enabled && requested.is_none() {
                log::warn!("[LOCAL_LIGHT_FIELD] storage_range_unsupported probes={probe_count} lights={light_count} max_bytes={} fallback=original", self.max_storage_bytes);
            }
            let buffer = Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::GpuOnly,
                required.bytes,
            );
            log::info!("[LOCAL_LIGHT_FIELD] representation=connection_transmission slot={slot} probes={} lights={} bytes={} grid_unchanged=true",
                required.probes, required.lights, required.bytes);
            self.frames[slot] = Some((Arc::new(buffer), required));
        }
        (
            self.frames[slot].as_ref().unwrap().0.clone(),
            requested.is_some(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::FieldLayout;

    #[test]
    fn fixed_grid_storage_scales_with_live_lights_and_preserves_zero_light_header() {
        let one = FieldLayout::new(18_513, 1, u64::MAX).unwrap();
        let eight = FieldLayout::new(18_513, 8, u64::MAX).unwrap();
        assert_eq!(one.probes, eight.probes);
        assert_eq!(eight.bytes - 16, (one.bytes - 16) * 8);
        assert_eq!(FieldLayout::new(18_513, 0, 16).unwrap().bytes, 16);
        assert_eq!(FieldLayout::new(18_513, 8, eight.bytes), Some(eight));
    }

    #[test]
    fn unsupported_counts_or_device_ranges_cannot_create_an_out_of_bounds_field() {
        assert!(FieldLayout::new(0, 8, u64::MAX).is_none());
        assert!(FieldLayout::new(u32::MAX, u32::MAX, u64::MAX).is_none());
        assert!(FieldLayout::new(u32::MAX, 1, u64::MAX).is_none());
        let layout = FieldLayout::new(18_513, 8, u64::MAX).unwrap();
        assert!(FieldLayout::new(18_513, 8, layout.bytes - 1).is_none());
        // There is no independently imposed eight-light cap.
        assert!(FieldLayout::new(100, 128, 128 * 1024 * 1024).is_some());
    }
}
