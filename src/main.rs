mod app;
mod audio;
mod builder;
mod egui_renderer;
mod flora;
mod gameplay;
mod generated;
mod geom;
#[macro_use]
mod gui_adjustables;
mod particles;
mod procedual_placer;
mod resource;
mod tracer;
mod tree_gen;
mod util;
mod vkn;
mod wind;
mod window;

use ash::vk;
use app::AppController;
use env_logger::Env;
use winit::event_loop::EventLoop;

#[derive(Clone, Copy, Debug)]
pub enum PresentModePreference {
    Mailbox,
    Immediate,
    Fifo,
    FifoRelaxed,
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

/// Application launch options parsed from CLI arguments.
#[derive(Clone, Debug)]
pub struct AppOptions {
    /// Run in windowed mode instead of borderless fullscreen.
    pub windowed: bool,
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
    /// Enable swapchain optimizations (unclamped image count).
    /// May not work on all platforms.
    pub swapchain_optimize: bool,
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

        Self {
            windowed: args.iter().any(|a| a == "--windowed"),
            no_shadows: args.iter().any(|a| a == "--no-shadows"),
            no_denoise: args.iter().any(|a| a == "--no-denoise"),
            no_god_rays: args.iter().any(|a| a == "--no-god-rays"),
            no_lens_flare: args.iter().any(|a| a == "--no-lens-flare"),
            no_tracer: args.iter().any(|a| a == "--no-tracer"),
            no_particles: args.iter().any(|a| a == "--no-particles"),
            no_flora: args.iter().any(|a| a == "--no-flora"),
            present_mode,
            swapchain_optimize: args.iter().any(|a| a == "--swapchain-optimize"),
            swapchain_images: parse_f32_after("--swapchain-images").map(|v| v as u32),
            screenshot_path: parse_string_after("--screenshot"),
            screenshot_delay: parse_f32_after("--screenshot-delay").unwrap_or(5.0),
            auto_exit_delay: parse_f32_after("--auto-exit"),
            perf: args.iter().any(|a| a == "--perf"),
            help: args.iter().any(|a| a == "--help"),
        }
    }
}

fn print_help() {
    println!(
        "Usage:\n  re-flora [options]\n\nOptions:\n  --windowed                  Run in windowed mode (default: borderless fullscreen)\n  --no-shadows                Disable shadow rendering passes\n  --no-denoise                Disable denoiser passes\n  --no-god-rays               Disable god ray pass\n  --no-lens-flare             Disable lens flare passes\n  --no-tracer                 Disable main tracer pass\n  --no-particles              Disable particle simulation and rendering\n  --no-flora                  Disable flora and leaves rendering\n  --present-mode <mode>       Override auto present mode selection: mailbox, immediate, fifo, fifo_relaxed\n  --swapchain-optimize        Enable swapchain optimizations (may not work on all platforms)\n  --swapchain-images <N>      Override swapchain image count (default: auto)\n  --screenshot <path>         Save one screenshot after rendering starts\n  --screenshot-delay <sec>    Delay before screenshot capture (default: 5.0)\n  --auto-exit <sec>           Exit automatically after rendering starts\n  --perf                      Enable per-frame performance logging\n  --help                      Show this help and exit\n\nExamples:\n  re-flora --windowed\n  re-flora --present-mode fifo\n  re-flora --no-shadows --no-denoise\n  re-flora --screenshot out.png --screenshot-delay 3\n  re-flora --auto-exit 10 --perf"
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

fn init_env_logger() {
    env_logger::Builder::from_env(Env::default().default_filter_or(
        "info,winit=warn,sctk=warn,wayland_client=warn,x11rb=warn,calloop=error,symphonia_format_riff=warn",
    ))
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

    init_env_logger();

    let options = AppOptions::from_args();
    if options.help {
        print_help();
        return;
    }

    let mut app = AppController::new(options);
    let event_loop = EventLoop::builder().build().unwrap();
    let result = event_loop.run_app(&mut app);

    match result {
        Ok(_) => log::info!("Application exited successfully"),
        Err(e) => log::error!("Application exited with error: {:?}", e),
    }
}
