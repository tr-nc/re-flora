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

use anyhow::Result;
use app::AppController;
use audionimbus::*;
use env_logger::Env;
use glam::Vec3;
use kira::{AudioManager, AudioManagerSettings, DefaultBackend};
use winit::event_loop::EventLoop;

#[allow(dead_code)]
fn backtrace_on() {
    use std::env;
    env::set_var("RUST_BACKTRACE", "1");
}

fn test_function() -> Result<()> {
    use crate::audio::spatial_sound::RealTimeSpatialSoundData;
    use crate::audio::spatial_sound_calculator::SpatialSoundCalculator;
    // In a real application, you would do this:

    // 1. Create your audio manager (usually done once at app startup)
    let mut audio_manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings {
        ..Default::default()
    })?;

    // 2. Create the audionimbus context and spatial sound calculator
    let context = Context::try_new(&ContextSettings::default())?;
    let mut spatial_sound_calculator = SpatialSoundCalculator::new(10240, context, 1024);

    // 3. Update positions from your game state
    // spatial_sound_calculator.update_positions(Vec3::new(0.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0));
    // spatial_sound_calculator.update_simulation()?;

    // 4. Create spatial sound data and play it
    let spatial_sound_data = RealTimeSpatialSoundData::new(spatial_sound_calculator)?;
    let _handle = audio_manager.play(spatial_sound_data)?;

    std::thread::sleep(std::time::Duration::from_secs(8));

    Ok(())
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
    // let mut app = AppController::default();
    // let event_loop = EventLoop::builder().build().unwrap();
    // let result = event_loop.run_app(&mut app);

    // match result {
    //     Ok(_) => log::info!("Application exited successfully"),
    //     Err(e) => log::error!("Application exited with error: {:?}", e),
    // }

    test_function().unwrap();
}
