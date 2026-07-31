use super::App;
use crate::ddgi::DdgiDebugView;
use crate::environment_lighting::EnvironmentLightingBackend;
use anyhow::{Context, Result};
use re_flora_vkn::{Buffer, BufferUsage, CommandBuffer, Extent2D, MemoryLocation};
use std::io::Write;
use std::path::Path;

const CAPTURE_MAGIC: &[u8; 8] = b"RFIRR001";
const CAPTURE_VERSION: u32 = 2;
const CAPTURE_CHANNEL_COUNT: u32 = 4;

pub(super) struct EnvironmentIrradianceCaptureReadback {
    path: String,
    extent: Extent2D,
    backend: EnvironmentLightingBackend,
    spacing_voxels: u32,
    debug_view: DdgiDebugView,
    buffer: Buffer,
}

impl App {
    pub(super) fn prepare_environment_irradiance_capture_readback(
        &self,
        path: String,
    ) -> Result<EnvironmentIrradianceCaptureReadback> {
        let output_path = Path::new(&path);
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                anyhow::bail!("parent directory does not exist: {}", parent.display());
            }
        }

        let extent = self.tracer.environment_irradiance_capture_extent();
        let byte_count = u64::from(extent.width)
            * u64::from(extent.height)
            * std::mem::size_of::<[f32; 4]>() as u64;
        let allocator = self
            .tracer
            .get_screen_output_tex()
            .get_image()
            .get_allocator()
            .clone();
        let buffer = Buffer::new_sized(
            self.vulkan_ctx.device().clone(),
            allocator,
            BufferUsage::transfer_dst(),
            MemoryLocation::GpuToCpu,
            byte_count,
        );
        Ok(EnvironmentIrradianceCaptureReadback {
            path,
            extent,
            backend: self.tracer.environment_lighting_backend(),
            spacing_voxels: self.tracer.environment_probe_status().grid.spacing_voxels(),
            debug_view: self.tracer.ddgi_debug_view(),
            buffer,
        })
    }

    pub(super) fn record_environment_irradiance_capture_readback(
        &self,
        cmdbuf: &CommandBuffer,
        readback: &EnvironmentIrradianceCaptureReadback,
    ) {
        self.tracer
            .record_environment_irradiance_capture_readback(cmdbuf, &readback.buffer);
    }

    pub(super) fn write_environment_irradiance_capture_readback(
        readback: EnvironmentIrradianceCaptureReadback,
    ) -> Result<()> {
        let raw = readback.buffer.read_back()?;
        let expected_bytes = readback.extent.width as usize
            * readback.extent.height as usize
            * std::mem::size_of::<[f32; 4]>();
        if raw.len() != expected_bytes {
            anyhow::bail!(
                "capture byte count mismatch: got {}, expected {}",
                raw.len(),
                expected_bytes,
            );
        }

        let mut file = std::fs::File::create(&readback.path)
            .with_context(|| format!("create {}", readback.path))?;
        file.write_all(CAPTURE_MAGIC)?;
        for value in [
            CAPTURE_VERSION,
            readback.extent.width,
            readback.extent.height,
            CAPTURE_CHANNEL_COUNT,
            readback.backend.as_u32(),
            readback.spacing_voxels,
            readback.debug_view.as_u32(),
        ] {
            file.write_all(&value.to_le_bytes())?;
        }
        file.write_all(&raw)?;
        file.flush()?;
        log::info!(
            "[ENV_IRRADIANCE_CAPTURE] saved path={} extent={}x{} backend={} spacing_voxels={} view={} samples={} format=float4-linear-rgb-hit",
            readback.path,
            readback.extent.width,
            readback.extent.height,
            readback.backend.label(),
            readback.spacing_voxels,
            readback.debug_view.label(),
            readback.extent.width * readback.extent.height,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_header_is_fixed_width_and_self_describing() {
        assert_eq!(CAPTURE_MAGIC.len(), 8);
        assert_eq!(CAPTURE_VERSION, 1);
        assert_eq!(CAPTURE_CHANNEL_COUNT, 4);
        assert_eq!(8 + 7 * std::mem::size_of::<u32>(), 36);
    }
}
