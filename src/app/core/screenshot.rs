use super::App;
use anyhow::{Context, Result};
use verdarium_vkn::{
    Buffer, BufferUsage, ColorReadbackFormat, CommandBuffer, Extent2D, MemoryLocation,
};

pub(super) struct ScreenshotReadback {
    pub(super) path: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: ColorReadbackFormat,
    pub(super) buffer: Buffer,
}

impl App {
    pub(super) fn prepare_screenshot_readback(
        &self,
        path: String,
        render_area: Extent2D,
    ) -> Result<ScreenshotReadback> {
        let output_path = std::path::Path::new(&path);
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                anyhow::bail!("parent directory does not exist: {}", parent.display());
            }
        }

        let width = render_area.width;
        let height = render_area.height;
        let byte_count = width as u64 * height as u64 * 4;
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

        Ok(ScreenshotReadback {
            path,
            width,
            height,
            format: self
                .swapchain
                .color_readback_format()
                .context("unsupported swapchain screenshot format")?,
            buffer,
        })
    }

    pub(super) fn record_screenshot_readback(
        &self,
        cmdbuf: &CommandBuffer,
        image_idx: u32,
        readback: &ScreenshotReadback,
    ) {
        self.swapchain.record_image_readback(
            cmdbuf,
            image_idx,
            &readback.buffer,
            readback.width,
            readback.height,
        );
    }

    pub(super) fn write_screenshot_readback(readback: ScreenshotReadback) {
        match readback.buffer.read_back() {
            Ok(raw_data) => {
                let rgba_data = readback.format.convert_to_rgba(raw_data);
                match image::RgbaImage::from_raw(readback.width, readback.height, rgba_data) {
                    Some(image_data) => match image_data.save(&readback.path) {
                        Ok(()) => log::info!(
                            "[SCREENSHOT] Saved {}x{} to {}",
                            readback.width,
                            readback.height,
                            readback.path
                        ),
                        Err(err) => {
                            log::error!("[SCREENSHOT] Failed to write {}: {}", readback.path, err)
                        }
                    },
                    None => {
                        log::error!("[SCREENSHOT] Invalid image dimensions or pixel buffer size")
                    }
                }
            }
            Err(err) => log::error!("[SCREENSHOT] GPU readback failed: {}", err),
        }
    }
}
