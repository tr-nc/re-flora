use super::App;
use anyhow::{Context, Result};
use re_flora_vkn::{
    Buffer, BufferUsage, ColorReadbackFormat, CommandBuffer, Extent2D, MemoryLocation,
};
use std::{borrow::Cow, path::Path, time::Instant};

#[cfg(target_os = "linux")]
use std::time::Duration;

struct ClipboardCopyOutcome {
    backend: &'static str,
    encoded_bytes: Option<usize>,
}

pub(super) enum ScreenshotDestination {
    File(String),
    Clipboard,
    DenoiserBenchmark,
}

pub(super) struct ScreenshotReadback {
    pub(super) destination: ScreenshotDestination,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: ColorReadbackFormat,
    pub(super) buffer: Buffer,
}

#[cfg(target_os = "linux")]
fn encode_clipboard_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut png_data = Vec::with_capacity(rgba.len() / 2);
    {
        let mut encoder = png::Encoder::new(&mut png_data, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        // Fastest uses PNG's Up filter and fdeflate, keeping 4K clipboard images
        // small enough for paste targets without bringing back slow DEFLATE levels.
        encoder.set_compression(png::Compression::Fastest);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(rgba)?;
    }
    Ok(png_data)
}

#[cfg(target_os = "linux")]
fn wait_for_wayland_clipboard_image(expected_png: &[u8]) -> Result<()> {
    use std::process::Command;

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match Command::new("wl-paste")
            .args(["--type", "image/png"])
            .output()
        {
            Ok(output) if output.status.success() && output.stdout == expected_png => {
                return Ok(());
            }
            Ok(_) => {}
            Err(err) => return Err(err).context("failed to query Wayland clipboard types"),
        }

        if Instant::now() >= deadline {
            anyhow::bail!("Wayland clipboard did not serve the copied image/png within 3 seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "linux")]
fn copy_rgba_to_wayland_clipboard(width: u32, height: u32, rgba: &[u8]) -> Result<usize> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        anyhow::bail!("not running under Wayland");
    }

    let png_data = encode_clipboard_png(width, height, rgba)?;

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
    // wl-copy forks its clipboard owner. Its parent can exit successfully before
    // the daemon has actually replaced the previous selection, so paste the PNG
    // back and verify its bytes before reporting success to the user.
    wait_for_wayland_clipboard_image(&png_data)?;
    Ok(png_data.len())
}

fn copy_rgba_to_clipboard(width: u32, height: u32, rgba: Vec<u8>) -> Result<ClipboardCopyOutcome> {
    #[cfg(target_os = "linux")]
    match copy_rgba_to_wayland_clipboard(width, height, &rgba) {
        Ok(encoded_bytes) => {
            return Ok(ClipboardCopyOutcome {
                backend: "wl-copy",
                encoded_bytes: Some(encoded_bytes),
            });
        }
        Err(err) => log::warn!(
            "[SCREENSHOT] Wayland image clipboard failed; trying native fallback: {err:#}"
        ),
    }

    arboard::Clipboard::new().and_then(|mut clipboard| {
        clipboard.set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Owned(rgba),
        })
    })?;
    Ok(ClipboardCopyOutcome {
        backend: "arboard",
        encoded_bytes: None,
    })
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

    pub(super) fn prepare_denoiser_benchmark_readback(
        &self,
        render_area: Extent2D,
    ) -> Result<ScreenshotReadback> {
        let mut readback = self.prepare_screenshot_readback(String::new(), render_area)?;
        readback.destination = ScreenshotDestination::DenoiserBenchmark;
        Ok(readback)
    }

    pub(super) fn read_screenshot_rgba(readback: &ScreenshotReadback) -> Result<Vec<u8>> {
        let raw_data = readback.buffer.read_back()?;
        Ok(readback.format.convert_to_rgba(raw_data))
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
                            Ok(outcome) => log::info!(
                                "[SCREENSHOT] Copied {}x{} image to clipboard via {} in {:.1}ms (PNG {}, readback {:.1}ms, BGRA conversion {:.1}ms)",
                                readback.width,
                                readback.height,
                                outcome.backend,
                                started.elapsed().as_secs_f64() * 1000.0,
                                outcome
                                    .encoded_bytes
                                    .map(|bytes| format!("{:.1} MiB", bytes as f64 / 1_048_576.0))
                                    .unwrap_or_else(|| "size unavailable".to_owned()),
                                readback_elapsed.as_secs_f64() * 1000.0,
                                convert_elapsed.as_secs_f64() * 1000.0,
                            ),
                            Err(err) => log::error!(
                                "[SCREENSHOT] Failed to copy image to clipboard: {}",
                                err
                            ),
                        }
                    }
                    ScreenshotDestination::DenoiserBenchmark => {
                        log::error!(
                            "[DENOISER_BENCH] benchmark readback reached asynchronous screenshot writer"
                        );
                    }
                }
            }
            Err(err) => log::error!("[SCREENSHOT] GPU readback failed: {}", err),
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::encode_clipboard_png;

    #[test]
    fn clipboard_png_is_compressed_and_round_trips() {
        let width = 256;
        let height = 256;
        let mut rgba = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&[
                    ((x / 16) * 11) as u8,
                    ((y / 16) * 13) as u8,
                    (((x + y) / 32) * 17) as u8,
                    255,
                ]);
            }
        }

        let encoded = encode_clipboard_png(width as u32, height as u32, &rgba).unwrap();
        assert!(encoded.len() < rgba.len() / 4);

        let decoded = image::load_from_memory(&encoded).unwrap().into_rgba8();
        assert_eq!(decoded.dimensions(), (width as u32, height as u32));
        assert_eq!(decoded.into_raw(), rgba);
    }
}
