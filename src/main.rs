pub mod app;
pub mod renderer;
pub mod vkn;
pub mod window;

use app::App;
use simple_logger::SimpleLogger;
use winit::event_loop::EventLoop;

/// Enable backtrace on panic
#[allow(unused)]
fn backtrace_on() {
    use std::env;
    env::set_var("RUST_BACKTRACE", "1");
}

pub fn main() {
    // backtrace_on();

    SimpleLogger::new()
        .with_local_timestamps()
        .with_timestamp_format(time::macros::format_description!(
            "[hour]:[minute]:[second].[subsecond digits:3]Z"
        ))
        .init()
        .unwrap();

    let mut app = App::default();
    let event_loop = EventLoop::builder().build().unwrap();
    let result = event_loop.run_app(&mut app);

    match result {
        Ok(_) => log::info!("Application exited successfully"),
        Err(e) => log::error!("Application exited with error: {:?}", e),
    }
}
