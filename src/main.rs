mod app;
mod audio;
mod builder;
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
mod vkn;
mod wind;
mod window;

use app::AppController;
use ash::vk;
use env_logger::{Env, Target};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use winit::event_loop::EventLoop;

#[derive(Clone, Copy, Debug)]
pub enum PresentModePreference {
    Mailbox,
    Immediate,
    Fifo,
    FifoRelaxed,
}

#[derive(Clone, Copy, Debug)]
pub enum WaterProfilePreference {
    Default,
    Performance,
}

impl PresentModePreference {
    fn from_cli_value(value: &str) -> Option<Self> {
        match value {
            "mailbox" => Some(Self::Mailbox),
            "immediate" => Some(Self::Immediate),
            "fifo" => Some(Self::Fifo),
            "fifo_relaxed" => Some(Self::FifoRelaxed),
            _ => None,
        }
    }

    pub fn as_vk(self) -> vk::PresentModeKHR {
        match self {
            Self::Mailbox => vk::PresentModeKHR::MAILBOX,
            Self::Immediate => vk::PresentModeKHR::IMMEDIATE,
            Self::Fifo => vk::PresentModeKHR::FIFO,
            Self::FifoRelaxed => vk::PresentModeKHR::FIFO_RELAXED,
        }
    }
}

impl WaterProfilePreference {
    fn from_cli_value(value: &str) -> Option<Self> {
        match value {
            "default" => Some(Self::Default),
            "performance" => Some(Self::Performance),
            _ => None,
        }
    }
}

/// Application launch options parsed from CLI arguments.
#[derive(Clone, Debug)]
pub struct AppOptions {
    /// Run in windowed mode instead of borderless fullscreen.
    pub windowed: bool,
    /// Create the native window hidden while keeping the normal render/swapchain path.
    pub hidden: bool,
    /// Disable shadow rendering pass.
    pub no_shadows: bool,
    /// Disable denoiser passes.
    pub no_denoise: bool,
    /// Disable god ray pass.
    pub no_god_rays: bool,
    /// Disable lens flare passes.
    pub no_lens_flare: bool,
    /// Disable main tracer (black screen, for isolating other passes).
    pub no_tracer: bool,
    /// Disable particle simulation (butterflies, leaves).
    pub no_particles: bool,
    /// Disable flora/leaves graphics passes (grass, tree leaves).
    pub no_flora: bool,
    /// Preferred swapchain present mode override.
    pub present_mode: Option<PresentModePreference>,
    /// Override swapchain image count. None = auto (max(min_image_count, 3)).
    pub swapchain_images: Option<u32>,
    /// Path to save a screenshot after rendering starts. None = no screenshot.
    pub screenshot_path: Option<String>,
    /// Delay in seconds after rendering starts before taking the screenshot.
    pub screenshot_delay: f32,
    /// Auto-exit N seconds after rendering starts. None = don't auto-exit.
    pub auto_exit_delay: Option<f32>,
    /// Enable per-frame performance timing output to console.
    pub perf: bool,
    /// Select a named water MLS-MPM configuration profile.
    pub water_profile: Option<WaterProfilePreference>,
    /// Override water MLS-MPM particle count.
    pub water_particles: Option<usize>,
    /// Override water MLS-MPM cubic grid dimension.
    pub water_grid: Option<u32>,
    /// Override water MLS-MPM fixed substep rate in Hz.
    pub water_substep_hz: Option<f32>,
    /// Override water-terrain collision keep-out distance in water grid cells.
    pub water_terrain_margin_cells: Option<f32>,
    /// Override water linear velocity damping per second.
    pub water_damping: Option<f32>,
    /// Override incompressible water pressure projection Jacobi iterations.
    pub water_pressure_iterations: Option<u32>,
    /// Override marker particle spacing relaxation iterations (0 disables anti-clump pass).
    pub water_spacing_iterations: Option<u32>,
    /// Run a deterministic terrain-edit soak around the pond for water validation.
    pub water_edit_soak: bool,
    /// Run the lightweight tree replacement benchmark and exit after completion.
    pub tree_bench: bool,
    /// Number of tree benchmark samples.
    pub tree_bench_samples: u32,
    /// Sweep Min Trunk Thickness during tree benchmark.
    pub tree_bench_min_thickness: bool,
    /// Do not wait for deferred rebuilds between tree benchmark samples.
    pub tree_bench_rapid: bool,
    /// Print the per-worktree run log directory and exit successfully.
    pub print_log_dir: bool,
    /// Print the latest run log path and exit successfully.
    pub latest_log: bool,
    /// Print the last N lines from the latest run log and exit successfully.
    pub tail_latest_log: Option<usize>,
    /// Print CLI help and exit successfully.
    pub help: bool,
}

impl AppOptions {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();

        let parse_f32_after = |flag: &str| -> Option<f32> {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse::<f32>().ok())
        };

        let parse_u32_after = |flag: &str| -> Option<u32> {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse::<u32>().ok())
        };

        let parse_string_after = |flag: &str| -> Option<String> {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };

        let present_mode = match parse_string_after("--present-mode") {
            Some(value) => {
                Some(PresentModePreference::from_cli_value(&value).unwrap_or_else(|| {
                    panic!(
                        "Unsupported --present-mode '{}'. Supported values: mailbox, immediate, fifo, fifo_relaxed",
                        value
                    )
                }))
            }
            None if args.iter().any(|a| a == "--present-mode") => {
                panic!(
                    "Missing value for --present-mode. Supported values: mailbox, immediate, fifo, fifo_relaxed"
                )
            }
            None => None,
        };

        let water_profile = match parse_string_after("--water-profile") {
            Some(value) => Some(
                WaterProfilePreference::from_cli_value(&value).unwrap_or_else(|| {
                    panic!(
                        "Unsupported --water-profile '{}'. Supported values: default, performance",
                        value
                    )
                }),
            ),
            None if args.iter().any(|a| a == "--water-profile") => {
                panic!("Missing value for --water-profile. Supported values: default, performance")
            }
            None => None,
        };

        let tail_latest_log = args
            .iter()
            .any(|a| a == "--tail-latest-log")
            .then(|| parse_u32_after("--tail-latest-log").unwrap_or(200) as usize);

        Self {
            windowed: args.iter().any(|a| a == "--windowed"),
            hidden: args.iter().any(|a| a == "--hidden"),
            no_shadows: args.iter().any(|a| a == "--no-shadows"),
            no_denoise: args.iter().any(|a| a == "--no-denoise"),
            no_god_rays: args.iter().any(|a| a == "--no-god-rays"),
            no_lens_flare: args.iter().any(|a| a == "--no-lens-flare"),
            no_tracer: args.iter().any(|a| a == "--no-tracer"),
            no_particles: args.iter().any(|a| a == "--no-particles"),
            no_flora: args.iter().any(|a| a == "--no-flora"),
            present_mode,
            swapchain_images: parse_f32_after("--swapchain-images").map(|v| v as u32),
            screenshot_path: parse_string_after("--screenshot"),
            screenshot_delay: parse_f32_after("--screenshot-delay").unwrap_or(5.0),
            auto_exit_delay: parse_f32_after("--auto-exit"),
            perf: args.iter().any(|a| a == "--perf"),
            water_profile,
            water_particles: parse_u32_after("--water-particles").map(|v| v as usize),
            water_grid: parse_u32_after("--water-grid").map(|v| v.max(4)),
            water_substep_hz: parse_f32_after("--water-substep-hz").map(|v| v.max(1.0)),
            water_terrain_margin_cells: parse_f32_after("--water-terrain-margin-cells")
                .map(|v| v.max(0.0)),
            water_damping: parse_f32_after("--water-damping").map(|v| v.max(0.0)),
            water_pressure_iterations: parse_u32_after("--water-pressure-iterations"),
            water_spacing_iterations: parse_u32_after("--water-spacing-iterations"),
            water_edit_soak: args.iter().any(|a| a == "--water-edit-soak"),
            tree_bench: args.iter().any(|a| a == "--tree-bench"),
            tree_bench_samples: parse_u32_after("--tree-bench-samples").unwrap_or(10),
            tree_bench_min_thickness: args.iter().any(|a| a == "--tree-bench-min-thickness"),
            tree_bench_rapid: args.iter().any(|a| a == "--tree-bench-rapid"),
            print_log_dir: args.iter().any(|a| a == "--print-log-dir"),
            latest_log: args.iter().any(|a| a == "--latest-log"),
            tail_latest_log,
            help: args.iter().any(|a| a == "--help" || a == "-h"),
        }
    }
}

fn print_help() {
    println!(
        r#"Usage:
  re-flora [options]

Options:
  --windowed                  Run in windowed mode (default: borderless fullscreen)
  --hidden                    Run with a hidden native window while preserving the render/swapchain path
  --no-shadows                Disable shadow rendering passes
  --no-denoise                Disable denoiser passes
  --no-god-rays               Disable god ray pass
  --no-lens-flare             Disable lens flare passes
  --no-tracer                 Disable main tracer pass
  --no-particles              Disable particle simulation and rendering
  --no-flora                  Disable flora and leaves rendering
  --present-mode <mode>       Override auto present mode selection: mailbox, immediate, fifo, fifo_relaxed
  --swapchain-images <N>      Override swapchain image count (default: auto)
  --screenshot <path>         Save one screenshot after rendering starts
  --screenshot-delay <sec>    Delay before screenshot capture (default: 5.0)
  --auto-exit <sec>           Exit automatically after rendering starts
  --perf                      Enable per-frame performance logging
  --water-profile <profile>   Select water profile: default, performance
  --water-particles <N>       Override initial water MLS-MPM particle count (0 = none)
  --water-grid <N>            Override cubic water MLS-MPM grid dimension
  --water-substep-hz <Hz>     Override water MLS-MPM fixed substep rate
  --water-terrain-margin-cells <C>
                              Override water-terrain keep-out distance in grid cells
  --water-damping <PerSec>    Override water linear velocity damping per second
  --water-pressure-iterations <N>
                              Override incompressible pressure projection iterations (0 = legacy EOS)
  --water-spacing-iterations <N>
                              Override marker particle spacing relaxation iterations (0 disables anti-clump pass)
  --water-edit-soak           Run deterministic pond terrain edits for water validation
  --tree-bench                Run tree replacement benchmark and exit
  --tree-bench-samples <N>    Tree benchmark samples (default: 10)
  --tree-bench-min-thickness  Sweep Min Trunk Thickness instead of Tree Height
  --tree-bench-rapid          Do not wait for deferred rebuilds between samples
  --print-log-dir             Print the per-worktree run log directory and exit
  --latest-log                Print the latest run log path and exit
  --tail-latest-log [N]       Print the last N lines of the latest run log and exit (default: 200)
  -h, --help                  Show this help and exit

Examples:
  re-flora --windowed
  re-flora --hidden --auto-exit 20 --perf
  re-flora --hidden --screenshot screenshots/check.png --screenshot-delay 5 --auto-exit 7
  re-flora --present-mode fifo
  re-flora --swapchain-images 2
  re-flora --no-shadows --no-denoise
  re-flora --screenshot out.png --screenshot-delay 3
  re-flora --auto-exit 10 --perf
  re-flora --hidden --auto-exit 4 --perf --water-profile performance
  re-flora --hidden --auto-exit 4 --perf --water-profile performance --water-damping 1.5 --water-terrain-margin-cells 0.0
  re-flora --hidden --auto-exit 14 --perf --water-profile performance --water-edit-soak
  re-flora --latest-log
  re-flora --tail-latest-log 120
  re-flora --windowed --tree-bench --tree-bench-samples 10"#
    );
}

#[derive(Clone, Debug)]
pub struct RenderFlags {
    pub enable_shadows: bool,
    pub enable_denoiser: bool,
    pub enable_god_rays: bool,
    pub enable_lens_flare: bool,
    pub enable_tracer: bool,
    pub enable_flora: bool,
    pub enable_particles: bool,
}

impl From<&AppOptions> for RenderFlags {
    fn from(options: &AppOptions) -> Self {
        Self {
            enable_shadows: !options.no_shadows,
            enable_denoiser: !options.no_denoise,
            enable_god_rays: !options.no_god_rays,
            enable_lens_flare: !options.no_lens_flare,
            enable_tracer: !options.no_tracer,
            enable_flora: !options.no_flora,
            enable_particles: !options.no_particles,
        }
    }
}

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
    PathBuf::from(env!("PROJECT_ROOT"))
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
        print_help();
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
