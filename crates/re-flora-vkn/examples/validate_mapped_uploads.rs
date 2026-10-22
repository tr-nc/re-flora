//! Opt-in native regression, not a normal unit test or a performance benchmark.
//! cargo run --release --manifest-path crates/re-flora-vkn/Cargo.toml \
//!   --target-dir target --example validate_mapped_uploads
//! Creates only an invisible window. Uses real Buffer APIs and GPU copy/readback.
use anyhow::{ensure, Result};
use re_flora_vkn::{
    execute_one_time_gpu_job, Allocator, Buffer, BufferUsage, BufferUse, MemoryLocation,
    VulkanContext, VulkanContextDesc,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

struct ValidationLog;
static ERRORS: AtomicUsize = AtomicUsize::new(0);
impl log::Log for ValidationLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}", record.level(), record.args());
            if record.level() == log::Level::Error {
                ERRORS.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    fn flush(&self) {}
}

fn buffer(
    ctx: &VulkanContext,
    allocator: &Allocator,
    size: usize,
    location: MemoryLocation,
) -> Buffer {
    Buffer::new_sized(
        ctx.device().clone(),
        allocator.clone(),
        BufferUsage::transfer_src().with_transfer_dst(),
        location,
        size as u64,
    )
}

fn gpu_round_trip(ctx: &VulkanContext, allocator: &Allocator, source: &Buffer) -> Result<Vec<u8>> {
    let size = source.get_size_bytes();
    let device = buffer(ctx, allocator, size as usize, MemoryLocation::GpuOnly);
    let readback = buffer(ctx, allocator, size as usize, MemoryLocation::GpuToCpu);
    execute_one_time_gpu_job(
        ctx.device(),
        ctx.command_pool(),
        &ctx.get_general_queue(),
        |cmd| {
            cmd.use_buffer(source, BufferUse::HostWrite);
            source.record_copy_to_buffer(cmd, &device, size, 0, 0);
            device.record_copy_to_buffer(cmd, &readback, size, 0, 0);
            cmd.use_buffer(&readback, BufferUse::HostRead);
        },
    );
    readback.read_back()
}

fn validate(ctx: &VulkanContext) -> Result<()> {
    let allocator = Allocator::new_for_context(ctx);
    let pattern: Vec<u8> = (0..1_048_640).map(|i| (i * 37 + 11) as u8).collect();
    let mut cases = 0;
    // Exercise actual uncached uploads and the allocator's cached readback choice.
    for location in [MemoryLocation::CpuToGpu, MemoryLocation::GpuToCpu] {
        let info = buffer(ctx, &allocator, 4096, location);
        println!(
            "[MAPPED_UPLOAD] location={location:?} properties={:?}",
            info.memory_properties()
        );
        for offset in [0usize, 1, 7, 15, 16, 31, 63] {
            for len in [
                0usize, 1, 7, 15, 16, 17, 63, 64, 65, 2111, 2112, 2113, 65535, 65536, 65537,
                1_048_573,
            ] {
                let size = (offset + len + 64).next_multiple_of(4);
                let upload = buffer(ctx, &allocator, size, location);
                let mut expected = vec![0xa5; size];
                upload.fill_with_raw_u8(&expected)?;
                let source = &pattern[63 - offset..63 - offset + len];
                upload.fill_range_with_raw_u8(offset as u64, source)?;
                expected[offset..offset + len].copy_from_slice(source);
                ensure!(
                    gpu_round_trip(ctx, &allocator, &upload)? == expected,
                    "range copy/guards differ location={location:?} offset={offset} len={len}"
                );
                // Reuse the allocation after the completed GPU job, including a zero write.
                upload.fill_range_with_raw_u8(size as u64, &[])?;
                ensure!(upload.fill_range_with_raw_u8(size as u64, &[1]).is_err());
                ensure!(upload.fill_range_with_raw_u8(u64::MAX, &[1]).is_err());
                ensure!(upload.fill(&vec![0u8; size + 1]).is_err());
                ensure!(
                    gpu_round_trip(ctx, &allocator, &upload)? == expected,
                    "rejected write mutated buffer"
                );
                cases += 1;
            }
        }
        let upload = buffer(ctx, &allocator, 64, location);
        let uniform = [0x12345678u32; 16];
        upload.fill_uniform(&uniform)?;
        ensure!(gpu_round_trip(ctx, &allocator, &upload)? == bytemuck::bytes_of(&uniform));
        upload.fill_element_with_raw_u8(&[0x5a; 64], 0)?;
        ensure!(upload.fill_element_with_raw_u8(&[0x5a; 64], 1).is_err());
        ensure!(gpu_round_trip(ctx, &allocator, &upload)? == [0x5a; 64]);
        let indices: Vec<u32> = (0..4096).collect();
        let upload = buffer(ctx, &allocator, indices.len() * 4, location);
        upload.fill(&indices)?;
        ensure!(
            gpu_round_trip(ctx, &allocator, &upload)? == bytemuck::cast_slice::<_, u8>(&indices)
        );
        // fill<T: Copy> must continue accepting padding: do not turn it into &[u8]
        // or require Pod. Compare only the initialized fields, never padding bytes.
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Padded {
            tag: u8,
            value: u32,
        }
        let padded = [Padded {
            tag: 7,
            value: 0x12345678,
        }; 1024];
        let upload = buffer(ctx, &allocator, std::mem::size_of_val(&padded), location);
        upload.fill(&padded)?;
        let bytes = gpu_round_trip(ctx, &allocator, &upload)?;
        for item in bytes.chunks_exact(std::mem::size_of::<Padded>()) {
            ensure!(item[0] == 7);
            let offset = std::mem::offset_of!(Padded, value);
            ensure!(item[offset..offset + 4] == 0x12345678u32.to_ne_bytes());
        }
        cases += 4;
    }
    println!("[MAPPED_UPLOAD] passed cases={cases} exact_bytes=true guards=true rejected_writes=true padded_copy=true gpu_round_trip=true");
    Ok(())
}

#[derive(Default)]
struct App {
    result: Option<Result<()>>,
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.result = Some((|| {
            let window =
                event_loop.create_window(Window::default_attributes().with_visible(false))?;
            let context = VulkanContext::new(
                &window,
                VulkanContextDesc {
                    name: "mapped upload validation".into(),
                },
            );
            validate(&context)
        })());
        event_loop.exit();
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
fn main() -> Result<()> {
    log::set_logger(&ValidationLog).unwrap();
    log::set_max_level(log::LevelFilter::Warn);
    let mut app = App::default();
    EventLoop::new()?.run_app(&mut app)?;
    app.result.expect("native event loop did not initialize")?;
    ensure!(
        ERRORS.load(Ordering::Relaxed) == 0,
        "native validation logged errors"
    );
    Ok(())
}
