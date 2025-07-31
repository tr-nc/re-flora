mod app;
mod audio;
mod builder;
mod egui_renderer;
mod gameplay;
mod geom;
mod procedual_placer;
mod resource;
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

fn init_env_logger() {
    env_logger::Builder::from_env(
        Env::default().default_filter_or("debug,symphonia_core=warn,symphonia_format_riff=warn"),
    )
    .format(|buf, record| {
        use std::io::Write;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let local_time = chrono::DateTime::from_timestamp_millis(now as i64)
            .unwrap()
            .with_timezone(&chrono::Local);

        writeln!(
            buf,
            "[{} {} {}] {}",
            local_time.format("%H:%M:%S%.3f"),
            record.level(),
            record.module_path().unwrap_or("<unknown>"),
            record.args()
        )
    })
    .init();
}
pub fn main() {
    // backtrace_on();

    init_env_logger();
    let mut app = AppController::default();
    let event_loop = EventLoop::builder().build().unwrap();
    let result = event_loop.run_app(&mut app);

    match result {
        Ok(_) => log::info!("Application exited successfully"),
        Err(e) => log::error!("Application exited with error: {:?}", e),
    }
}
