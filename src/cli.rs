use re_flora_vkn::PresentMode;

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

#[derive(Clone, Copy, Debug)]
pub enum MonitorScorePreference {
    Highest,
    Lowest,
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

    pub fn as_present_mode(self) -> PresentMode {
        match self {
            Self::Mailbox => PresentMode::Mailbox,
            Self::Immediate => PresentMode::Immediate,
            Self::Fifo => PresentMode::Fifo,
            Self::FifoRelaxed => PresentMode::FifoRelaxed,
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

impl MonitorScorePreference {
    fn from_cli_value(value: &str) -> Option<Self> {
        match value {
            "highest" => Some(Self::Highest),
            "lowest" => Some(Self::Lowest),
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
    /// Audio processing still runs, but master output volume is forced to 0.
    /// On Wayland, fall back to requesting minimization because hidden windows are unsupported.
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
    /// Select borderless fullscreen monitor by physical-pixel score.
    pub monitor_score: MonitorScorePreference,
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
    /// Override water MLS-MPM per-particle rest-volume cube edge length.
    pub water_particle_edge_len: Option<f32>,
    /// Override water MLS-MPM cubic grid dimension.
    pub water_grid: Option<u32>,
    /// Override water MLS-MPM fixed substep rate in Hz.
    pub water_substep_hz: Option<f32>,
    /// Override water-terrain collision keep-out distance in water grid cells.
    pub water_terrain_margin_cells: Option<f32>,
    /// Override water linear velocity damping per second.
    pub water_damping: Option<f32>,
    /// Override water-terrain tangential damping per second.
    pub water_terrain_tangent_damping: Option<f32>,
    /// Override weakly-compressible equation-of-state stiffness.
    pub water_stiffness: Option<f32>,
    /// Override weakly-compressible equation-of-state gamma.
    pub water_gamma: Option<f32>,
    /// Override minimum weakly-compressible volume ratio J.
    pub water_j_min: Option<f32>,
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
    pub fn from_args() -> Self {
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

        let monitor_score = match parse_string_after("--monitor-score") {
            Some(value) => MonitorScorePreference::from_cli_value(&value).unwrap_or_else(|| {
                panic!(
                    "Unsupported --monitor-score '{}'. Supported values: highest, lowest",
                    value
                )
            }),
            None if args.iter().any(|a| a == "--monitor-score") => {
                panic!("Missing value for --monitor-score. Supported values: highest, lowest")
            }
            None => MonitorScorePreference::Lowest,
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
            monitor_score,
            swapchain_images: parse_f32_after("--swapchain-images").map(|v| v as u32),
            screenshot_path: parse_string_after("--screenshot"),
            screenshot_delay: parse_f32_after("--screenshot-delay").unwrap_or(5.0),
            auto_exit_delay: parse_f32_after("--auto-exit"),
            perf: args.iter().any(|a| a == "--perf"),
            water_profile,
            water_particles: parse_u32_after("--water-particles").map(|v| v as usize),
            water_particle_edge_len: parse_f32_after("--water-particle-edge-len")
                .map(|v| v.max(1.0e-6)),
            water_grid: parse_u32_after("--water-grid").map(|v| v.max(4)),
            water_substep_hz: parse_f32_after("--water-substep-hz").map(|v| v.max(1.0)),
            water_terrain_margin_cells: parse_f32_after("--water-terrain-margin-cells")
                .map(|v| v.max(0.0)),
            water_damping: parse_f32_after("--water-damping").map(|v| v.max(0.0)),
            water_terrain_tangent_damping: parse_f32_after("--water-terrain-tangent-damping")
                .map(|v| v.max(0.0)),
            water_stiffness: parse_f32_after("--water-stiffness").map(|v| v.max(0.0)),
            water_gamma: parse_f32_after("--water-gamma").map(|v| v.max(1.0e-4)),
            water_j_min: parse_f32_after("--water-j-min").map(|v| v.clamp(1.0e-4, 1.0)),
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

pub fn print_help() {
    println!(
        r#"Usage:
  re-flora [options]

Options:
  --windowed                  Run in windowed mode (default: borderless fullscreen)
  --hidden                    Run hidden; mute audio output while preserving render/swapchain and audio processing paths
  --no-shadows                Disable shadow rendering passes
  --no-denoise                Disable denoiser passes
  --no-god-rays               Disable god ray pass
  --no-lens-flare             Disable lens flare passes
  --no-tracer                 Disable main tracer pass
  --no-particles              Disable particle simulation and rendering
  --no-flora                  Disable flora and leaves rendering
  --present-mode <mode>       Override auto present mode selection: mailbox, immediate, fifo, fifo_relaxed
  --monitor-score <mode>      Select borderless fullscreen monitor by resolution score: highest, lowest (default: lowest)
  --swapchain-images <N>      Override swapchain image count (default: auto)
  --screenshot <path>         Save one screenshot after rendering starts
  --screenshot-delay <sec>    Delay before screenshot capture (default: 5.0)
  --auto-exit <sec>           Exit automatically after rendering starts
  --perf                      Enable per-frame performance logging
  --water-profile <profile>   Select water profile: default, performance
  --water-particles <N>       Seed N initial water MLS-MPM particles in the startup pool (0 = none)
  --water-particle-edge-len <L>
                              Override per-particle rest-volume cube edge length
  --water-grid <N>            Override cubic water MLS-MPM grid dimension
  --water-substep-hz <Hz>     Override water MLS-MPM fixed substep rate
  --water-terrain-margin-cells <C>
                              Override water-terrain keep-out distance in grid cells
  --water-damping <PerSec>    Override water linear velocity damping per second
  --water-terrain-tangent-damping <PerSec>
                              Override terrain-contact tangential damping per second
  --water-stiffness <K>       Override weakly-compressible EOS stiffness
  --water-gamma <G>           Override weakly-compressible EOS gamma
  --water-j-min <J>           Override minimum weakly-compressible volume ratio J
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
  re-flora --monitor-score lowest
  re-flora --swapchain-images 2
  re-flora --no-shadows --no-denoise
  re-flora --screenshot out.png --screenshot-delay 3
  re-flora --auto-exit 10 --perf
  re-flora --hidden --auto-exit 4 --perf --water-profile performance
  re-flora --hidden --auto-exit 4 --perf --water-particles 35000 --water-particle-edge-len 0.05
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
