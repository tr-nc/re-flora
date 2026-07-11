use super::App;
use anyhow::{Context, Result};
use std::{borrow::Cow, path::Path, time::Instant};
use verdarium_vkn::{
    Buffer, BufferUsage, ColorReadbackFormat, CommandBuffer, Extent2D, MemoryLocation,
};

pub(super) enum ScreenshotDestination {
    File(String),
    Clipboard,
}

pub(super) struct ScreenshotReadback {
    pub(super) destination: ScreenshotDestination,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: ColorReadbackFormat,
    pub(super) buffer: Buffer,
}

#[cfg(target_os = "linux")]
fn copy_rgba_to_wayland_clipboard(width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        anyhow::bail!("not running under Wayland");
    }

    let mut png_data = Vec::with_capacity(rgba.len() / 2);
    {
        let mut encoder = png::Encoder::new(&mut png_data, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        // Clipboard latency matters more than transfer size; 4K DEFLATE compression
        // dominated the capture at roughly one second on the target machine.
        encoder.set_compression(png::Compression::NoCompression);
        encoder.set_filter(png::Filter::NoFilter);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(rgba)?;
    }

    let mut child = Command::new("wl-copy")
        .args(["--type", "image/png"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .context("wl-copy stdin unavailable")?
        .write_all(&png_data)?;
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("wl-copy exited with {status}");
    }
    Ok(())
}

fn copy_rgba_to_clipboard(width: u32, height: u32, rgba: Vec<u8>) -> Result<()> {
    #[cfg(target_os = "linux")]
    if copy_rgba_to_wayland_clipboard(width, height, &rgba).is_ok() {
        return Ok(());
    }

    arboard::Clipboard::new()
        .and_then(|mut clipboard| {
            clipboard.set_image(arboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: Cow::Owned(rgba),
            })
        })
        .map_err(Into::into)
}

impl App {
    pub(super) fn prepare_screenshot_readback(
        &self,
        path: String,
        render_area: Extent2D,
    ) -> Result<ScreenshotReadback> {
        let output_path = Path::new(&path);
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
            destination: ScreenshotDestination::File(path),
            width,
            height,
            format: self
                .swapchain
                .color_readback_format()
                .context("unsupported swapchain screenshot format")?,
            buffer,
        })
    }

    pub(super) fn prepare_clipboard_screenshot_readback(
        &self,
        render_area: Extent2D,
    ) -> Result<ScreenshotReadback> {
        let mut readback = self.prepare_screenshot_readback(String::new(), render_area)?;
        readback.destination = ScreenshotDestination::Clipboard;
        Ok(readback)
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
        let started = Instant::now();
        match readback.buffer.read_back() {
            Ok(raw_data) => {
                let readback_elapsed = started.elapsed();
                let rgba_data = readback.format.convert_to_rgba(raw_data);
                let convert_elapsed = started.elapsed() - readback_elapsed;
                match readback.destination {
                    ScreenshotDestination::File(path) => {
                        match image::RgbaImage::from_raw(readback.width, readback.height, rgba_data)
                        {
                            Some(image_data) => match image_data.save(&path) {
                                Ok(()) => log::info!(
                                    "[SCREENSHOT] Saved {}x{} to {}",
                                    readback.width,
                                    readback.height,
                                    path
                                ),
                                Err(err) => {
                                    log::error!("[SCREENSHOT] Failed to write {}: {}", path, err)
                                }
                            },
                            None => log::error!(
                                "[SCREENSHOT] Invalid image dimensions or pixel buffer size"
                            ),
                        }
                    }
                    ScreenshotDestination::Clipboard => {
                        match copy_rgba_to_clipboard(readback.width, readback.height, rgba_data) {
                            Ok(()) => log::info!(
                                "[SCREENSHOT] Copied {}x{} image to clipboard in {:.1}ms (readback {:.1}ms, BGRA conversion {:.1}ms)",
                                readback.width,
                                readback.height,
                                started.elapsed().as_secs_f64() * 1000.0,
                                readback_elapsed.as_secs_f64() * 1000.0,
                                convert_elapsed.as_secs_f64() * 1000.0,
                            ),
                            Err(err) => log::error!(
                                "[SCREENSHOT] Failed to copy image to clipboard: {}",
                                err
                            ),
                        }
                    }
                }
            }
            Err(err) => log::error!("[SCREENSHOT] GPU readback failed: {}", err),
        }
    }
}
