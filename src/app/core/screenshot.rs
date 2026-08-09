use super::denoiser_bench::DenoiserBench;
use crate::{tracer::Tracer, ScreenshotOptions};
use anyhow::{Context, Result};
use re_flora_vkn::{
    Buffer, BufferUsage, ColorReadbackFormat, CommandBuffer, Extent2D, MemoryLocation, Swapchain,
    VulkanContext,
};
use std::{borrow::Cow, path::Path, time::Instant};

#[cfg(target_os = "linux")]
use std::time::Duration;

struct ClipboardCopyOutcome {
    backend: &'static str,
    encoded_bytes: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
enum ScreenshotDestination {
    File(String),
    Clipboard,
}

struct SwapchainReadback {
    width: u32,
    height: u32,
    format: ColorReadbackFormat,
    buffer: Buffer,
}

pub(super) struct PendingScreenshot {
    destination: ScreenshotDestination,
    readback: SwapchainReadback,
}

pub(super) struct PendingDenoiserFrame {
    readback: SwapchainReadback,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScreenshotFrameReadiness {
    render_elapsed_seconds: Option<f32>,
    test_scenes_ready: bool,
    environment_lighting_ready: bool,
}

impl ScreenshotFrameReadiness {
    pub(super) fn new(
        render_elapsed_seconds: Option<f32>,
        test_scenes_ready: bool,
        environment_lighting_ready: bool,
    ) -> Self {
        Self {
            render_elapsed_seconds,
            test_scenes_ready,
            environment_lighting_ready,
        }
    }

    fn elapsed_if_ready(self, delay_seconds: f32) -> Option<f32> {
        let elapsed = self.render_elapsed_seconds?;
        (elapsed >= delay_seconds && self.test_scenes_ready && self.environment_lighting_ready)
            .then_some(elapsed)
    }
}

struct ScheduledScreenshot {
    path: String,
    delay_seconds: f32,
    taken: bool,
}

pub(super) struct ScreenshotRuntime {
    scheduled: Option<ScheduledScreenshot>,
    clipboard_requested: bool,
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

impl ScreenshotRuntime {
    pub(super) fn new(options: Option<ScreenshotOptions>) -> Self {
        Self {
            scheduled: options.map(|options| ScheduledScreenshot {
                path: options.path,
                delay_seconds: options.delay,
                taken: false,
            }),
            clipboard_requested: false,
        }
    }

    pub(super) fn is_scheduled(&self) -> bool {
        self.scheduled.is_some()
    }

    pub(super) fn request_clipboard(&mut self) {
        self.clipboard_requested = true;
        log::info!("[SCREENSHOT] P pressed; capturing next frame to clipboard");
    }

    fn claim_destination(
        &mut self,
        readiness: ScreenshotFrameReadiness,
    ) -> Option<ScreenshotDestination> {
        if std::mem::take(&mut self.clipboard_requested) {
            return Some(ScreenshotDestination::Clipboard);
        }

        let scheduled = self.scheduled.as_mut()?;
        if scheduled.taken {
            return None;
        }
        let elapsed = readiness.elapsed_if_ready(scheduled.delay_seconds)?;
        scheduled.taken = true;
        log::info!(
            "[SCREENSHOT] Capturing after {:.2}s to {}",
            elapsed,
            scheduled.path
        );
        Some(ScreenshotDestination::File(scheduled.path.clone()))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_if_ready(
        &mut self,
        tracer: &Tracer,
        vulkan_ctx: &VulkanContext,
        swapchain: &Swapchain,
        cmdbuf: &CommandBuffer,
        image_idx: u32,
        render_area: Extent2D,
        readiness: ScreenshotFrameReadiness,
    ) -> Option<PendingScreenshot> {
        let destination = self.claim_destination(readiness)?;
        let readback = match prepare_swapchain_readback(
            tracer,
            vulkan_ctx,
            swapchain,
            render_area,
            Some(&destination),
        ) {
            Ok(readback) => readback,
            Err(err) => {
                match destination {
                    ScreenshotDestination::Clipboard => {
                        log::error!("[SCREENSHOT] Failed to prepare clipboard capture: {err}")
                    }
                    ScreenshotDestination::File(_) => {
                        log::error!("[SCREENSHOT] Failed to prepare: {err}")
                    }
                }
                return None;
            }
        };
        record_swapchain_readback(swapchain, cmdbuf, image_idx, &readback);
        Some(PendingScreenshot {
            destination,
            readback,
        })
    }

    pub(super) fn complete(&self, readback: PendingScreenshot) {
        std::thread::Builder::new()
            .name("screenshot-readback".to_owned())
            .spawn(move || write_screenshot_readback(readback))
            .unwrap_or_else(|err| {
                log::error!("[SCREENSHOT] Failed to start readback thread: {err}");
                panic!("failed to start screenshot readback thread: {err}");
            });
    }
}

impl PendingDenoiserFrame {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record(
        tracer: &Tracer,
        vulkan_ctx: &VulkanContext,
        swapchain: &Swapchain,
        cmdbuf: &CommandBuffer,
        image_idx: u32,
        render_area: Extent2D,
    ) -> Result<Self> {
        let readback =
            prepare_swapchain_readback(tracer, vulkan_ctx, swapchain, render_area, None)?;
        record_swapchain_readback(swapchain, cmdbuf, image_idx, &readback);
        Ok(Self { readback })
    }

    pub(super) fn complete(self, benchmark: &mut DenoiserBench) -> Result<bool> {
        let width = self.readback.width;
        let height = self.readback.height;
        let rgba = read_swapchain_rgba(&self.readback)?;
        benchmark.record_frame(width, height, &rgba)
    }
}

fn prepare_swapchain_readback(
    tracer: &Tracer,
    vulkan_ctx: &VulkanContext,
    swapchain: &Swapchain,
    render_area: Extent2D,
    destination: Option<&ScreenshotDestination>,
) -> Result<SwapchainReadback> {
    if let Some(ScreenshotDestination::File(path)) = destination {
        let output_path = Path::new(path);
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                anyhow::bail!("parent directory does not exist: {}", parent.display());
            }
        }
    }

    let width = render_area.width;
    let height = render_area.height;
    let byte_count = u64::from(width) * u64::from(height) * 4;
    let allocator = tracer
        .get_screen_output_tex()
        .get_image()
        .get_allocator()
        .clone();
    let buffer = Buffer::new_sized(
        vulkan_ctx.device().clone(),
        allocator,
        BufferUsage::transfer_dst(),
        MemoryLocation::GpuToCpu,
        byte_count,
    );

    Ok(SwapchainReadback {
        width,
        height,
        format: swapchain
            .color_readback_format()
            .context("unsupported swapchain screenshot format")?,
        buffer,
    })
}

fn record_swapchain_readback(
    swapchain: &Swapchain,
    cmdbuf: &CommandBuffer,
    image_idx: u32,
    readback: &SwapchainReadback,
) {
    swapchain.record_image_readback(
        cmdbuf,
        image_idx,
        &readback.buffer,
        readback.width,
        readback.height,
    );
}

fn read_swapchain_rgba(readback: &SwapchainReadback) -> Result<Vec<u8>> {
    let raw_data = readback.buffer.read_back()?;
    Ok(readback.format.convert_to_rgba(raw_data))
}

fn write_screenshot_readback(readback: PendingScreenshot) {
    let PendingScreenshot {
        destination,
        readback,
    } = readback;
    let started = Instant::now();
    match readback.buffer.read_back() {
        Ok(raw_data) => {
            let readback_elapsed = started.elapsed();
            let rgba_data = readback.format.convert_to_rgba(raw_data);
            let convert_elapsed = started.elapsed() - readback_elapsed;
            match destination {
                ScreenshotDestination::File(path) => {
                    match image::RgbaImage::from_raw(readback.width, readback.height, rgba_data) {
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
            }
        }
        Err(err) => log::error!("[SCREENSHOT] GPU readback failed: {}", err),
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    fn ready_after(seconds: f32) -> ScreenshotFrameReadiness {
        ScreenshotFrameReadiness::new(Some(seconds), true, true)
    }

    #[test]
    fn clipboard_preempts_a_scheduled_capture_without_consuming_it() {
        let mut runtime = ScreenshotRuntime::new(Some(ScreenshotOptions {
            path: "scheduled.png".to_owned(),
            delay: 2.0,
        }));
        assert!(runtime.is_scheduled());
        assert_eq!(runtime.claim_destination(ready_after(1.0)), None);

        runtime.request_clipboard();
        assert_eq!(
            runtime.claim_destination(ScreenshotFrameReadiness::default()),
            Some(ScreenshotDestination::Clipboard)
        );
        assert_eq!(
            runtime.claim_destination(ready_after(2.0)),
            Some(ScreenshotDestination::File("scheduled.png".to_owned()))
        );
        assert_eq!(runtime.claim_destination(ready_after(3.0)), None);
    }

    #[test]
    fn scheduled_capture_requires_all_readiness_inputs() {
        let options = || {
            Some(ScreenshotOptions {
                path: "scheduled.png".to_owned(),
                delay: 2.0,
            })
        };
        let mut no_render = ScreenshotRuntime::new(options());
        assert_eq!(
            no_render.claim_destination(ScreenshotFrameReadiness::new(None, true, true)),
            None
        );
        let mut scene_pending = ScreenshotRuntime::new(options());
        assert_eq!(
            scene_pending.claim_destination(ScreenshotFrameReadiness::new(Some(2.0), false, true)),
            None
        );
        let mut lighting_pending = ScreenshotRuntime::new(options());
        assert_eq!(
            lighting_pending.claim_destination(ScreenshotFrameReadiness::new(
                Some(2.0),
                true,
                false
            )),
            None
        );
    }

    #[test]
    fn disabled_runtime_has_no_scheduled_capture() {
        let mut runtime = ScreenshotRuntime::new(None);
        assert!(!runtime.is_scheduled());
        assert_eq!(runtime.claim_destination(ready_after(10.0)), None);
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
