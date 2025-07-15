mod app;
mod audio;
mod builder;
mod egui_renderer;
mod gameplay;
mod geom;
mod procedual_placer;
mod tracer;
mod tree_gen;
mod util;
mod vkn;
mod window;

use app::AppController;
use env_logger::Env;
use winit::event_loop::EventLoop;

#[allow(dead_code)]
fn backtrace_on() {
    use std::env;
    env::set_var("RUST_BACKTRACE", "1");
}

pub fn main() {
    // backtrace_on();

    env_logger::Builder::from_env(
        Env::default().default_filter_or("info,symphonia_core=warn,symphonia_format_riff=warn"),
    )
    .format_timestamp_millis()
    .init();

    let mut app = AppController::default();
    let event_loop = EventLoop::builder().build().unwrap();
    let result = event_loop.run_app(&mut app);

    match result {
        Ok(_) => log::info!("Application exited successfully"),
        Err(e) => log::error!("Application exited with error: {:?}", e),
    }
}
