//! Opt-in native clipboard checks; normal `cargo test` never connects to a display.
#![cfg(target_os = "linux")]

use egui_winit::clipboard::Clipboard;
use std::sync::atomic::{AtomicUsize, Ordering};

struct ClipboardLog;
static ARBOARD_INITIALIZATIONS: AtomicUsize = AtomicUsize::new(0);

impl log::Log for ClipboardLog {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if record.target() == "egui_winit::clipboard" {
            let message = record.args().to_string();
            if message.starts_with("Initializing arboard clipboard") {
                ARBOARD_INITIALIZATIONS.fetch_add(1, Ordering::SeqCst);
            }
            eprintln!("[{}] {message}", record.level());
        }
    }

    fn flush(&self) {}
}

fn capture_clipboard_logs() {
    log::set_logger(&ClipboardLog).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
}

#[test]
#[ignore = "requires a native Wayland session; run this test alone"]
fn wayland_state_does_not_initialize_arboard() {
    use winit::event_loop::EventLoop;
    use winit::platform::wayland::EventLoopBuilderExtWayland;
    use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

    capture_clipboard_logs();
    let event_loop = EventLoop::builder()
        .with_wayland()
        .with_any_thread(true)
        .build()
        .unwrap();
    assert!(matches!(
        event_loop.display_handle().unwrap().as_raw(),
        RawDisplayHandle::Wayland(_)
    ));

    // The same adapter construction as EguiRenderer, without Vulkan or a window.
    // State is dropped before the event loop that owns its Wayland display.
    let _state = egui_winit::State::new(
        egui::Context::default(),
        egui::ViewportId::ROOT,
        &event_loop,
        None,
        None,
        None,
    );
    assert_eq!(
        ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst),
        0,
        "Wayland startup must not connect to the unused X11 clipboard"
    );
}

#[test]
#[ignore = "requires a working X11 DISPLAY; run this test alone"]
fn native_x11_text_and_image_clipboard_still_work() {
    capture_clipboard_logs();
    let mut clipboard = Clipboard::new(None);
    assert_eq!(ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst), 0);

    let text = "Re: Flora clipboard regression 剪贴板 🌿";
    clipboard.set_text(text.to_owned());
    assert_eq!(clipboard.get().as_deref(), Some(text));
    assert_eq!(ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst), 1);

    let rgba = [255, 0, 0, 255, 0, 255, 0, 255];
    clipboard.set_image(&egui::ColorImage::from_rgba_unmultiplied([2, 1], &rgba));
    let image = arboard::Clipboard::new().unwrap().get_image().unwrap();
    assert_eq!((image.width, image.height), (2, 1));
    assert_eq!(image.bytes.as_ref(), rgba);
    assert_eq!(ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst), 1);

    // Image-first and paste-first instances must also initialize on demand.
    let mut image_only = Clipboard::new(None);
    assert_eq!(ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst), 1);
    image_only.set_image(&egui::ColorImage::from_rgba_unmultiplied([2, 1], &rgba));
    let image = arboard::Clipboard::new().unwrap().get_image().unwrap();
    assert_eq!(image.bytes.as_ref(), rgba);
    assert_eq!(ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst), 2);

    clipboard.set_text(text.to_owned());
    let mut paste_only = Clipboard::new(None);
    assert_eq!(ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst), 2);
    assert_eq!(paste_only.get().as_deref(), Some(text));
    assert_eq!(ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst), 3);
}

#[test]
#[ignore = "run alone with DISPLAY=invalid and WAYLAND_DISPLAY unset"]
fn unavailable_clipboard_initializes_once_and_preserves_local_fallback() {
    assert_eq!(std::env::var("DISPLAY").as_deref(), Ok("invalid"));
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    capture_clipboard_logs();
    let mut clipboard = Clipboard::new(None);
    assert_eq!(ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst), 0);

    for text in ["first copy", "第二次复制 🌿"] {
        clipboard.set_text(text.to_owned());
        assert_eq!(clipboard.get().as_deref(), Some(text));
        assert_eq!(ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst), 1);
    }

    // Failed paste-first initialization is cached as well.
    let mut paste_only = Clipboard::new(None);
    assert_eq!(ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst), 1);
    for _ in 0..2 {
        assert_eq!(paste_only.get().as_deref(), Some(""));
        assert_eq!(ARBOARD_INITIALIZATIONS.load(Ordering::SeqCst), 2);
    }
}
