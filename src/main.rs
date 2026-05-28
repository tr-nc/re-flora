mod app;
mod audio;
mod builder;
mod cli;
mod egui_renderer;
mod flora;
mod game_time;
mod gameplay;
#[path = "auto-generated/mod.rs"]
mod generated;
mod geom;
#[macro_use]
mod gui_adjustables;
pub mod model;
mod particles;
mod procedual_placer;
mod resource;
mod tracer;
mod tree_gen;
mod util;
mod wind;
mod window;

use app::AppController;
pub use cli::{
    AppOptions, MonitorScorePreference, PresentModePreference, RenderFlags, WaterProfilePreference,
};
use env_logger::{Env, Target};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use winit::event_loop::EventLoop;

#[allow(dead_code)]
fn backtrace_on() {
    use std::env;
    env::set_var("RUST_BACKTRACE", "1");
}

const RUN_LOG_DIR_NAME: &str = "re-flora-logs";
const RUN_LOG_FILE_PREFIX: &str = "re-flora-";
const RUN_LOG_FILE_SUFFIX: &str = ".log";
const RUN_LOG_LATEST_POINTER_FILE_NAME: &str = "latest-run-log.txt";
const MAX_RUN_LOG_FILES: usize = 10;

struct TeeLogWriter {
    file: File,
    stderr: io::Stderr,
}

impl TeeLogWriter {
    fn new(file: File) -> Self {
        Self {
            file,
            stderr: io::stderr(),
        }
    }
}

impl Write for TeeLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let stderr_result = self.stderr.write_all(buf);
        let file_result = self.file.write_all(buf);
        stderr_result?;
        file_result?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let stderr_result = self.stderr.flush();
        let file_result = self.file.flush();
        stderr_result?;
        file_result
    }
}

fn run_log_dir() -> PathBuf {
    re_flora_vkn::project_root()
        .join("target")
        .join(RUN_LOG_DIR_NAME)
}

fn latest_run_log_pointer_path(dir: &Path) -> PathBuf {
    dir.join(RUN_LOG_LATEST_POINTER_FILE_NAME)
}

fn is_run_log_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    file_name.starts_with(RUN_LOG_FILE_PREFIX) && file_name.ends_with(RUN_LOG_FILE_SUFFIX)
}

fn write_latest_run_log_pointer(dir: &Path, log_path: &Path) -> io::Result<()> {
    fs::write(
        latest_run_log_pointer_path(dir),
        format!("{}\n", log_path.display()),
    )
}

fn scan_latest_run_log_path(dir: &Path) -> io::Result<Option<PathBuf>> {
    if !dir.exists() {
        return Ok(None);
    }

    let mut logs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if !is_run_log_file(&path) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() {
            logs.push(path);
        }
    }

    logs.sort();
    Ok(logs.pop())
}

fn latest_run_log_path() -> io::Result<Option<PathBuf>> {
    let dir = run_log_dir();
    let pointer_path = latest_run_log_pointer_path(&dir);
    if let Ok(contents) = fs::read_to_string(&pointer_path) {
        let path = PathBuf::from(contents.trim());
        if is_run_log_file(&path) && path.is_file() {
            return Ok(Some(path));
        }
    }

    scan_latest_run_log_path(&dir)
}

fn tail_log_file(path: &Path, line_count: usize) -> io::Result<()> {
    let content = fs::read_to_string(path)?;
    let lines = content.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(line_count);
    for line in &lines[start..] {
        println!("{line}");
    }
    Ok(())
}

fn prune_old_run_logs(dir: &Path, current_path: &Path) -> io::Result<()> {
    let mut logs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if !is_run_log_file(&path) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        logs.push(path);
    }

    if logs.len() <= MAX_RUN_LOG_FILES {
        return Ok(());
    }

    logs.sort();

    let remove_count = logs.len() - MAX_RUN_LOG_FILES;
    for path in logs
        .into_iter()
        .filter(|path| path != current_path)
        .take(remove_count)
    {
        if let Err(err) = fs::remove_file(&path) {
            eprintln!("Failed to remove old run log {}: {}", path.display(), err);
        }
    }

    Ok(())
}

fn create_run_log_file() -> io::Result<(PathBuf, File)> {
    let dir = run_log_dir();
    fs::create_dir_all(&dir)?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f");
    let process_id = std::process::id();
    for attempt in 0..100 {
        let attempt_suffix = if attempt == 0 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let path = dir.join(format!(
            "{RUN_LOG_FILE_PREFIX}{timestamp}-{process_id}{attempt_suffix}{RUN_LOG_FILE_SUFFIX}"
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                if let Err(err) = write_latest_run_log_pointer(&dir, &path) {
                    eprintln!(
                        "Failed to update latest run log pointer {}: {}",
                        latest_run_log_pointer_path(&dir).display(),
                        err
                    );
                }
                if let Err(err) = prune_old_run_logs(&dir, &path) {
                    eprintln!("Failed to prune old run logs in {}: {}", dir.display(), err);
                }
                return Ok((path, file));
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "failed to reserve a unique run log path in {}",
            dir.display()
        ),
    ))
}

fn init_env_logger() -> Option<PathBuf> {
    let run_log = match create_run_log_file() {
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
        builder.target(Target::Pipe(Box::new(TeeLogWriter::new(file))));
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

fn handle_log_query_options(options: &AppOptions) -> bool {
    if !options.print_log_dir && !options.latest_log && options.tail_latest_log.is_none() {
        return false;
    }

    if options.print_log_dir {
        println!("{}", run_log_dir().display());
    }

    if options.latest_log || options.tail_latest_log.is_some() {
        match latest_run_log_path() {
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
                    if let Err(err) = tail_log_file(&path, line_count) {
                        eprintln!("Failed to read latest run log {}: {}", path.display(), err);
                        std::process::exit(1);
                    }
                }
            }
            Ok(None) => {
                eprintln!("No run logs found in {}", run_log_dir().display());
                std::process::exit(1);
            }
            Err(err) => {
                eprintln!(
                    "Failed to inspect run logs in {}: {}",
                    run_log_dir().display(),
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

    let options = AppOptions::from_args();
    if options.help {
        cli::print_help();
        return;
    }
    if handle_log_query_options(&options) {
        return;
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
