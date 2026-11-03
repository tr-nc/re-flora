//! Final-receiver-only local illumination on the consumer-visible DDGI grid.
//! Each reused frame slot is rebuilt from live lights/geometry before sampling:
//! no temporal accumulation, stale light-off tails, or recursive bounce input.
use crate::ddgi::{DdgiAtlasLayout, DDGI_IRRADIANCE_INTERIOR_SIDE};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, Device, Extent3D, ImageDesc,
    MemoryLocation, SamplerDesc, Texture, TextureLayout,
};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct LocalLightField {
    frames: Vec<Option<(Arc<Texture>, u32)>>,
    audits: Vec<Option<AuditFrame>>,
    last_audit: Option<AuditKey>,
}

pub(super) type AuditKey = (u64, u32, u64, bool);
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
    pub tile_columns: u32,
    pub audit: u32,
}

impl LocalLightField {
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
                16,
            )),
            readback: Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
                MemoryLocation::GpuToCpu,
                16,
            ),
            pending: None,
        });
        if let Some(previous) = frame.pending.take() {
            let bytes = frame
                .readback
                .read_back()
                .expect("direct field audit readback");
            let values: [u32; 4] = bytemuck::pod_read_unaligned(&bytes[..16]);
            log::info!("[LOCAL_LIGHT_FIELD][AUDIT] key={previous:?} samples={} failures={} max_absolute_error={} nonzero={}",
                values[0], values[1], f32::from_bits(values[2]), values[3]);
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
                .record_copy_to_buffer(cmdbuf, &frame.readback, 16, 0, 0);
            cmdbuf.use_buffer(&frame.readback, BufferUse::HostRead);
        }
    }

    // The caller has waited for the frame-slot fence. Off mode needs a valid
    // descriptor but never samples it; do not allocate a full atlas until enabled.
    pub fn ensure(
        &mut self,
        device: Device,
        allocator: Allocator,
        slot: usize,
        probe_count: u32,
        enabled: bool,
    ) -> Arc<Texture> {
        self.frames
            .resize_with(self.frames.len().max(slot + 1), || None);
        let required = if enabled { probe_count } else { 1 };
        if self.frames[slot]
            .as_ref()
            .is_none_or(|(_, count)| enabled && *count != required)
        {
            let layout = DdgiAtlasLayout::new(required, DDGI_IRRADIANCE_INTERIOR_SIDE)
                .expect("validated DDGI grid has a valid direct atlas");
            let extent = layout.extent();
            let texture = Texture::new(
                device,
                allocator,
                &ImageDesc {
                    extent: Extent3D::new(extent.x, extent.y, 1),
                    image_type_override: Some(vk::ImageType::TYPE_2D),
                    format: vk::Format::R32G32B32A32_SFLOAT,
                    usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
                    initial_layout: TextureLayout::UNDEFINED,
                    aspect: vk::ImageAspectFlags::COLOR,
                    ..Default::default()
                },
                &SamplerDesc {
                    min_filter: vk::Filter::LINEAR,
                    mag_filter: vk::Filter::LINEAR,
                    ..Default::default()
                },
            );
            log::info!(
                "[LOCAL_LIGHT_FIELD] allocated slot={slot} probes={required} extent={}x{} bytes={}",
                extent.x,
                extent.y,
                u64::from(extent.x) * u64::from(extent.y) * 16
            );
            self.frames[slot] = Some((Arc::new(texture), required));
        }
        self.frames[slot].as_ref().unwrap().0.clone()
    }
}
