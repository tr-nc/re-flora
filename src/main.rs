pub mod app;
pub mod window;

use app::App;
use winit::event_loop::EventLoop;

// /// Enable backtrace on panic.
// #[allow(unused)]
// fn backtrace_on() {
//     use std::env;
//     env::set_var("RUST_BACKTRACE", "1");
// }

pub fn main() {
    // backtrace_on();

    let mut app = App::default();
    let event_loop = EventLoop::builder().build().unwrap();
    let result = event_loop.run_app(&mut app);

    match result {
        Ok(_) => println!("Application exited successfully"),
        Err(e) => println!("Application exited with error: {:?}", e),
    }
}
