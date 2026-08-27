mod app;
mod audio;
mod branch_skeleton;
mod branching_gui;
mod builder;
mod cli;
mod ddgi;
mod egui_renderer;
mod environment_lighting;
mod environment_probes;
mod flora;
mod game_time;
mod gameplay;
#[path = "auto-generated/mod.rs"]
mod generated;
mod geom;
mod lighting;
#[macro_use]
mod gui_adjustables;
mod particles;
mod procedual_placer;
mod resource;
mod run_log;
#[allow(dead_code)]
mod terrain_persistence;
mod tracer;
mod tree_gen;
mod util;
mod voxel_material;
mod wind;
mod window;

use app::AppController;
pub use cli::{
    AppOptions, DenoiserBenchOptions, DenoiserBenchScene, EnvironmentLightingTestCase,
    MonitorScorePreference, PresentModePreference, RenderFlags, ScreenshotOptions,
    WaterProfilePreference,
};
use env_logger::{Env, Target};
use std::{
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use winit::event_loop::EventLoop;

#[allow(dead_code)]
fn backtrace_on() {
    use std::env;
    env::set_var("RUST_BACKTRACE", "1");
}

fn init_env_logger() -> Option<PathBuf> {
    let run_log = match run_log::create_run_log_file() {
        Ok(run_log) => Some(run_log),
        Err(err) => {
            eprintln!("Failed to create run log file: {err}");
            None
        }
    };

    let mut builder = env_logger::Builder::from_env(Env::default().default_filter_or(
        "info,winit=warn,sctk=warn,wayland_client=warn,x11rb=warn,calloop=error,symphonia_format_riff=warn",
    ));

    let log_path = if let Some((path, file)) = run_log {
        builder.target(Target::Pipe(Box::new(run_log::TeeLogWriter::new(file))));
        Some(path)
    } else {
        None
    };

    builder
        .format(|buf, record| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
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

    if let Some(path) = &log_path {
        log::info!("Writing run log to {}", path.display());
    }

    log_path
}

fn handle_camera_snapshot_query_options(options: &AppOptions) -> bool {
    if !options.list_camera_snapshots {
        return false;
    }

    match app::camera_snapshots::CameraSnapshotLibrary::load_default() {
        Ok(library) => {
            for name in library.names_for_cli() {
                println!("{name}");
            }
        }
        Err(err) => {
            eprintln!("Failed to load camera snapshots: {err}");
            std::process::exit(1);
        }
    }

    true
}

fn validate_requested_camera_snapshot(options: &AppOptions) -> Result<(), String> {
    let Some(requested_name) = options.camera_snapshot.as_deref() else {
        return Ok(());
    };

    let library = app::camera_snapshots::CameraSnapshotLibrary::load_default().map_err(|err| {
        format!(
            "Failed to load camera snapshots: {err}\n{}",
            cli::CAMERA_SNAPSHOT_LIST_HINT
        )
    })?;

    if library.is_cli_name_available(requested_name) {
        return Ok(());
    }

    Err(format!(
        "Camera snapshot '{}' not found. Available camera snapshots: {}\n{}",
        requested_name,
        library.names_for_cli().join(", "),
        cli::CAMERA_SNAPSHOT_LIST_HINT
    ))
}

fn handle_log_query_options(options: &AppOptions) -> bool {
    if !options.print_log_dir && !options.latest_log && options.tail_latest_log.is_none() {
        return false;
    }

    if options.print_log_dir {
        println!("{}", run_log::run_log_dir().display());
    }

    if options.latest_log || options.tail_latest_log.is_some() {
        match run_log::latest_run_log_path() {
            Ok(Some(path)) => {
                if options.latest_log {
                    println!("{}", path.display());
                }
                if let Some(line_count) = options.tail_latest_log {
                    eprintln!(
                        "Tailing latest run log: {} (last {} lines)",
                        path.display(),
                        line_count
                    );
                    if let Err(err) = run_log::tail_log_file(&path, line_count) {
                        eprintln!("Failed to read latest run log {}: {}", path.display(), err);
                        std::process::exit(1);
                    }
                }
            }
            Ok(None) => {
                eprintln!("No run logs found in {}", run_log::run_log_dir().display());
                std::process::exit(1);
            }
            Err(err) => {
                eprintln!(
                    "Failed to inspect run logs in {}: {}",
                    run_log::run_log_dir().display(),
                    err
                );
                std::process::exit(1);
            }
        }
    }

    true
}

// fn play_audio_with_cpal() -> Result<()> {
//     use crate::audio::{get_audio_data, play_audio_samples};

//     // Step 1: Decode audio data using symphonia
//     let audio_path = "assets/sfx/Tree Gusts/WINDGust_Wind, Gust in Trees 01_SARM_Wind.wav";
//     let (samples, sample_rate) = get_audio_data(audio_path)?;

//     // Step 2: Play audio data using cpal
//     play_audio_samples(samples, sample_rate)?;

//     Ok(())
// }

pub fn main() {
    // backtrace_on();

    let options = match AppOptions::try_from_args() {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };
    if options.help {
        cli::print_help();
        return;
    }
    if handle_log_query_options(&options) {
        return;
    }
    if handle_camera_snapshot_query_options(&options) {
        return;
    }
    if let Err(err) = validate_requested_camera_snapshot(&options) {
        eprintln!("{err}");
        std::process::exit(1);
    }

    let run_log_path = init_env_logger();

    let mut app = AppController::new(options);
    let event_loop = EventLoop::builder().build().unwrap();
    let result = event_loop.run_app(&mut app);
    drop(app);

    match result {
        Ok(_) => log::info!("Application exited successfully"),
        Err(e) => log::error!("Application exited with error: {:?}", e),
    }

    if let Some(path) = &run_log_path {
        log::info!("Run log saved to {}", path.display());
    }
}
