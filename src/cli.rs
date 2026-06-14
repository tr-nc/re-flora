use verdarium_vkn::PresentMode;

pub const CAMERA_SNAPSHOT_LIST_HINT: &str =
    "Run `verdarium --list-camera-snapshots` to list available camera snapshots.";

const SCREENSHOT_USAGE: &str = "Expected `--screenshot <preset> <path> --screenshot-delay <sec>`.";

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
    /// On Wayland, fall back to requesting minimization because hidden windows are unsupported.
    pub hidden: bool,
    /// Start with global audio output muted while keeping audio processing active.
    pub mute: bool,
    /// Select an audio output device by case-insensitive substring match.
    pub audio_output_device: Option<String>,
    /// Print audio output devices visible to PetalSonic/CPAL and exit successfully.
    pub list_audio_output_devices: bool,
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
    /// Disable procedural cloud rendering.
    pub no_clouds: bool,
    /// Preferred swapchain present mode override.
    pub present_mode: Option<PresentModePreference>,
    /// Select borderless fullscreen monitor by physical-pixel score.
    pub monitor_score: MonitorScorePreference,
    /// Override swapchain image count. None = auto (max(min_image_count, 3)).
    pub swapchain_images: Option<u32>,
    /// Path to save a screenshot after rendering starts. None = no screenshot.
    pub screenshot_path: Option<String>,
    /// Delay in seconds after rendering starts before taking the screenshot. Required with --screenshot.
    pub screenshot_delay: Option<f32>,
    /// Apply a named camera snapshot at startup. Screenshot runs set this from the requested preset.
    pub camera_snapshot: Option<String>,
    /// Print available camera snapshot names and exit successfully.
    pub list_camera_snapshots: bool,
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

#[derive(Clone, Debug)]
struct ParsedScreenshot {
    preset_name: String,
    path: String,
    delay: f32,
}

impl AppOptions {
    pub fn from_args() -> Self {
        Self::from_arg_strings(std::env::args().collect())
    }

    pub fn try_from_args() -> Result<Self, String> {
        Self::try_from_arg_strings(std::env::args().collect())
    }

    fn from_arg_strings(args: Vec<String>) -> Self {
        Self::try_from_arg_strings(args).unwrap_or_else(|err| panic!("{err}"))
    }

    fn try_from_arg_strings(args: Vec<String>) -> Result<Self, String> {
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

        let parse_required_string_after =
            |flag: &str, label: &str| -> Result<Option<String>, String> {
                let Some(index) = args.iter().position(|a| a == flag) else {
                    return Ok(None);
                };
                let Some(value) = args.get(index + 1) else {
                    return Err(format!("Missing value for {flag}. Expected {label}."));
                };
                if value.starts_with("--") {
                    return Err(format!("Missing value for {flag}. Expected {label}."));
                }
                Ok(Some(value.clone()))
            };

        let present_mode = match parse_required_string_after(
            "--present-mode",
            "one of: mailbox, immediate, fifo, fifo_relaxed",
        )? {
            Some(value) => Some(parse_present_mode_preference(&value)?),
            None => None,
        };

        let water_profile =
            match parse_required_string_after("--water-profile", "one of: default, performance")? {
                Some(value) => Some(parse_water_profile_preference(&value)?),
                None => None,
            };

        let monitor_score =
            match parse_required_string_after("--monitor-score", "one of: highest, lowest")? {
                Some(value) => parse_monitor_score_preference(&value)?,
                None => MonitorScorePreference::Lowest,
            };

        let screenshot = parse_screenshot_request(&args)?;
        let camera_snapshot = if let Some(screenshot) = &screenshot {
            if args.iter().any(|a| a == "--camera-snapshot") {
                return Err(format!(
                    "Do not combine --camera-snapshot with --screenshot. {SCREENSHOT_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
                ));
            }
            Some(screenshot.preset_name.clone())
        } else {
            parse_required_string_after("--camera-snapshot", "a camera snapshot name")?
        };
        let screenshot_path = screenshot
            .as_ref()
            .map(|screenshot| screenshot.path.clone());
        let screenshot_delay = screenshot.as_ref().map(|screenshot| screenshot.delay);

        let tail_latest_log = args
            .iter()
            .any(|a| a == "--tail-latest-log")
            .then(|| parse_u32_after("--tail-latest-log").unwrap_or(200) as usize);

        Ok(Self {
            windowed: args.iter().any(|a| a == "--windowed"),
            hidden: args.iter().any(|a| a == "--hidden"),
            mute: args.iter().any(|a| a == "--mute"),
            audio_output_device: parse_required_string_after(
                "--audio-output-device",
                "an output device name substring",
            )?,
            list_audio_output_devices: args.iter().any(|a| a == "--list-audio-output-devices"),
            no_shadows: args.iter().any(|a| a == "--no-shadows"),
            no_denoise: args.iter().any(|a| a == "--no-denoise"),
            no_god_rays: args.iter().any(|a| a == "--no-god-rays"),
            no_lens_flare: args.iter().any(|a| a == "--no-lens-flare"),
            no_tracer: args.iter().any(|a| a == "--no-tracer"),
            no_particles: args.iter().any(|a| a == "--no-particles"),
            no_flora: args.iter().any(|a| a == "--no-flora"),
            no_clouds: args.iter().any(|a| a == "--no-clouds"),
            present_mode,
            monitor_score,
            swapchain_images: parse_f32_after("--swapchain-images").map(|v| v as u32),
            screenshot_path,
            screenshot_delay,
            camera_snapshot,
            list_camera_snapshots: args.iter().any(|a| a == "--list-camera-snapshots"),
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
        })
    }
}

fn parse_present_mode_preference(value: &str) -> Result<PresentModePreference, String> {
    PresentModePreference::from_cli_value(value).ok_or_else(|| {
        format!(
            "Unsupported --present-mode '{value}'. Supported values: mailbox, immediate, fifo, fifo_relaxed"
        )
    })
}

fn parse_water_profile_preference(value: &str) -> Result<WaterProfilePreference, String> {
    WaterProfilePreference::from_cli_value(value).ok_or_else(|| {
        format!("Unsupported --water-profile '{value}'. Supported values: default, performance")
    })
}

fn parse_monitor_score_preference(value: &str) -> Result<MonitorScorePreference, String> {
    MonitorScorePreference::from_cli_value(value).ok_or_else(|| {
        format!("Unsupported --monitor-score '{value}'. Supported values: highest, lowest")
    })
}

fn parse_screenshot_request(args: &[String]) -> Result<Option<ParsedScreenshot>, String> {
    let screenshot_indices: Vec<usize> = args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| (arg == "--screenshot").then_some(index))
        .collect();

    if screenshot_indices.is_empty() {
        if args.iter().any(|arg| arg == "--screenshot-delay") {
            return Err(format!(
                "--screenshot-delay requires --screenshot. {SCREENSHOT_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
            ));
        }
        return Ok(None);
    }

    if screenshot_indices.len() > 1 {
        return Err(format!(
            "Only one --screenshot is supported per run. {SCREENSHOT_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
        ));
    }

    let screenshot_index = screenshot_indices[0];
    let preset_name = required_screenshot_arg(args, screenshot_index + 1, "preset name")?;
    let path = required_screenshot_arg(args, screenshot_index + 2, "output path")?;
    let delay = parse_required_screenshot_delay(args)?;

    Ok(Some(ParsedScreenshot {
        preset_name,
        path,
        delay,
    }))
}

fn required_screenshot_arg(args: &[String], index: usize, label: &str) -> Result<String, String> {
    let Some(value) = args.get(index) else {
        return Err(format!(
            "Missing screenshot {label}. {SCREENSHOT_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
        ));
    };
    if value.starts_with("--") {
        return Err(format!(
            "Missing screenshot {label}. {SCREENSHOT_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
        ));
    }
    Ok(value.clone())
}

fn parse_required_screenshot_delay(args: &[String]) -> Result<f32, String> {
    let delay_indices: Vec<usize> = args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| (arg == "--screenshot-delay").then_some(index))
        .collect();

    if delay_indices.is_empty() {
        return Err(format!(
            "Missing --screenshot-delay <sec> for --screenshot. {SCREENSHOT_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
        ));
    }
    if delay_indices.len() > 1 {
        return Err(format!(
            "Only one --screenshot-delay is supported per run. {SCREENSHOT_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
        ));
    }

    let delay_index = delay_indices[0];
    let Some(value) = args.get(delay_index + 1) else {
        return Err(format!(
            "Missing value for --screenshot-delay. {SCREENSHOT_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
        ));
    };
    if value.starts_with("--") {
        return Err(format!(
            "Missing value for --screenshot-delay. {SCREENSHOT_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
        ));
    }
    let delay = value.parse::<f32>().map_err(|_| {
        format!(
            "Invalid --screenshot-delay '{}'. Expected a non-negative number of seconds.\n{CAMERA_SNAPSHOT_LIST_HINT}",
            value
        )
    })?;
    if !delay.is_finite() || delay < 0.0 {
        return Err(format!(
            "Invalid --screenshot-delay '{}'. Expected a non-negative number of seconds.\n{CAMERA_SNAPSHOT_LIST_HINT}",
            value
        ));
    }
    Ok(delay)
}

pub fn print_help() {
    println!(
        r#"Usage:
  verdarium [options]

Options:
  --windowed                  Run in windowed mode (default: borderless fullscreen)
  --hidden                    Run hidden while preserving render/swapchain path; audio output remains enabled unless --mute is set
  --mute                      Start with global audio output muted while keeping audio processing active
  --audio-output-device <text>
                              Select output device by case-insensitive substring/alias match
  --list-audio-output-devices Print output devices visible to PetalSonic/CPAL and exit
  --no-shadows                Disable shadow rendering passes
  --no-denoise                Disable denoiser passes
  --no-god-rays               Disable god ray pass
  --no-lens-flare             Disable lens flare passes
  --no-tracer                 Disable main tracer pass
  --no-particles              Disable particle simulation and rendering
  --no-flora                  Disable flora and leaves rendering
  --no-clouds                 Disable procedural cloud rendering
  --present-mode <mode>       Override auto present mode selection: mailbox, immediate, fifo, fifo_relaxed
  --monitor-score <mode>      Select borderless fullscreen monitor by resolution score: highest, lowest (default: lowest)
  --swapchain-images <N>      Override swapchain image count (default: auto)
  --screenshot <preset> <path>
                              Save one screenshot from exactly one camera snapshot preset
  --screenshot-delay <sec>    Required delay before screenshot capture when --screenshot is used
  --camera-snapshot <name>    Apply a saved camera snapshot at startup (do not combine with --screenshot)
  --list-camera-snapshots     Print available camera snapshot names and exit
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
  verdarium --windowed
  verdarium --hidden --mute --auto-exit 20 --perf
  verdarium --audio-output-device KA3
  verdarium --list-audio-output-devices
  verdarium --hidden --mute --screenshot player-default screenshots/check.png --screenshot-delay 2 --auto-exit 4
  verdarium --present-mode fifo
  verdarium --monitor-score lowest
  verdarium --swapchain-images 2
  verdarium --no-shadows --no-denoise
  verdarium --hidden --mute --screenshot tree-closeup out.png --screenshot-delay 2 --auto-exit 4
  verdarium --list-camera-snapshots
  verdarium --auto-exit 10 --perf
  verdarium --hidden --mute --auto-exit 4 --perf --water-profile performance
  verdarium --hidden --mute --auto-exit 4 --perf --water-particles 35000 --water-particle-edge-len 0.05
  verdarium --hidden --mute --auto-exit 4 --perf --water-profile performance --water-damping 1.5 --water-terrain-margin-cells 0.0
  verdarium --hidden --mute --auto-exit 14 --perf --water-profile performance --water-edit-soak
  verdarium --latest-log
  verdarium --tail-latest-log 120
  verdarium --windowed --tree-bench --tree-bench-samples 10"#
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
    pub enable_clouds: bool,
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
            enable_clouds: !options.no_clouds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> AppOptions {
        AppOptions::from_arg_strings(args.iter().map(|arg| (*arg).to_owned()).collect())
    }

    #[test]
    fn defaults_match_runtime_expectations() {
        let options = parse(&["verdarium"]);

        assert!(!options.windowed);
        assert!(!options.hidden);
        assert!(!options.mute);
        assert!(options.audio_output_device.is_none());
        assert!(!options.list_audio_output_devices);
        assert!(!options.perf);
        assert!(options.present_mode.is_none());
        assert!(matches!(
            options.monitor_score,
            MonitorScorePreference::Lowest
        ));
        assert!(options.screenshot_path.is_none());
        assert!(options.screenshot_delay.is_none());
        assert!(options.camera_snapshot.is_none());
        assert!(!options.list_camera_snapshots);
        assert_eq!(options.tree_bench_samples, 10);
        assert!(options.tail_latest_log.is_none());
    }

    #[test]
    fn parses_common_perf_and_water_options() {
        let options = parse(&[
            "verdarium",
            "--hidden",
            "--mute",
            "--auto-exit",
            "4",
            "--audio-output-device",
            "KA3",
            "--perf",
            "--water-profile",
            "performance",
            "--water-particles",
            "35000",
            "--water-particle-edge-len",
            "0.05",
            "--water-grid",
            "128",
            "--water-substep-hz",
            "60",
            "--water-terrain-margin-cells",
            "0.0",
            "--water-damping",
            "1.5",
            "--water-terrain-tangent-damping",
            "2.0",
            "--water-stiffness",
            "12",
            "--water-gamma",
            "4",
            "--water-j-min",
            "0.25",
            "--water-edit-soak",
        ]);

        assert!(options.hidden);
        assert!(options.mute);
        assert_eq!(options.audio_output_device.as_deref(), Some("KA3"));
        assert!(options.perf);
        assert_eq!(options.auto_exit_delay, Some(4.0));
        assert!(matches!(
            options.water_profile,
            Some(WaterProfilePreference::Performance)
        ));
        assert_eq!(options.water_particles, Some(35000));
        assert_eq!(options.water_particle_edge_len, Some(0.05));
        assert_eq!(options.water_grid, Some(128));
        assert_eq!(options.water_substep_hz, Some(60.0));
        assert_eq!(options.water_terrain_margin_cells, Some(0.0));
        assert_eq!(options.water_damping, Some(1.5));
        assert_eq!(options.water_terrain_tangent_damping, Some(2.0));
        assert_eq!(options.water_stiffness, Some(12.0));
        assert_eq!(options.water_gamma, Some(4.0));
        assert_eq!(options.water_j_min, Some(0.25));
        assert!(options.water_edit_soak);
    }

    #[test]
    fn clamps_numeric_options_like_runtime_parser() {
        let options = parse(&[
            "verdarium",
            "--water-particle-edge-len",
            "0",
            "--water-grid",
            "1",
            "--water-substep-hz",
            "0",
            "--water-terrain-margin-cells",
            "-5",
            "--water-damping",
            "-1",
            "--water-terrain-tangent-damping",
            "-1",
            "--water-gamma",
            "0",
            "--water-j-min",
            "5",
        ]);

        assert_eq!(options.water_particle_edge_len, Some(1.0e-6));
        assert_eq!(options.water_grid, Some(4));
        assert_eq!(options.water_substep_hz, Some(1.0));
        assert_eq!(options.water_terrain_margin_cells, Some(0.0));
        assert_eq!(options.water_damping, Some(0.0));
        assert_eq!(options.water_terrain_tangent_damping, Some(0.0));
        assert_eq!(options.water_gamma, Some(1.0e-4));
        assert_eq!(options.water_j_min, Some(1.0));
    }

    #[test]
    fn parses_log_query_options() {
        let options = parse(&[
            "verdarium",
            "--print-log-dir",
            "--latest-log",
            "--tail-latest-log",
            "120",
        ]);

        assert!(options.print_log_dir);
        assert!(options.latest_log);
        assert_eq!(options.tail_latest_log, Some(120));
    }

    #[test]
    fn parses_audio_output_device_query_option() {
        let options = parse(&["verdarium", "--list-audio-output-devices"]);
        assert!(options.list_audio_output_devices);
    }

    #[test]
    fn parses_camera_snapshot_options() {
        let options = parse(&[
            "verdarium",
            "--camera-snapshot",
            "tree-closeup",
            "--list-camera-snapshots",
        ]);

        assert_eq!(options.camera_snapshot.as_deref(), Some("tree-closeup"));
        assert!(options.list_camera_snapshots);
        assert!(options.screenshot_path.is_none());
        assert!(options.screenshot_delay.is_none());
    }

    #[test]
    fn parses_screenshot_preset_path_and_required_delay() {
        let options = parse(&[
            "verdarium",
            "--hidden",
            "--screenshot",
            "tree-closeup",
            "out.png",
            "--screenshot-delay",
            "2.5",
        ]);

        assert!(options.hidden);
        assert_eq!(options.camera_snapshot.as_deref(), Some("tree-closeup"));
        assert_eq!(options.screenshot_path.as_deref(), Some("out.png"));
        assert_eq!(options.screenshot_delay, Some(2.5));
    }

    #[test]
    fn screenshot_requires_delay() {
        let panic = std::panic::catch_unwind(|| {
            parse(&["verdarium", "--screenshot", "tree-closeup", "out.png"])
        })
        .expect_err("missing screenshot delay should panic");
        let message = panic_message(panic);
        assert!(message.contains("Missing --screenshot-delay"));
        assert!(message.contains("--list-camera-snapshots"));
    }

    #[test]
    fn screenshot_rejects_separate_camera_snapshot_flag() {
        let panic = std::panic::catch_unwind(|| {
            parse(&[
                "verdarium",
                "--screenshot",
                "tree-closeup",
                "out.png",
                "--screenshot-delay",
                "2",
                "--camera-snapshot",
                "other",
            ])
        })
        .expect_err("screenshot should own the camera snapshot preset");
        let message = panic_message(panic);
        assert!(message.contains("Do not combine --camera-snapshot with --screenshot"));
        assert!(message.contains("--list-camera-snapshots"));
    }

    #[test]
    fn tail_latest_log_defaults_to_200_without_value() {
        let options = parse(&["verdarium", "--tail-latest-log"]);
        assert_eq!(options.tail_latest_log, Some(200));
    }

    #[test]
    fn unsupported_present_mode_panics_with_helpful_message() {
        let panic = std::panic::catch_unwind(|| parse(&["verdarium", "--present-mode", "bad"]))
            .expect_err("unsupported present mode should panic");
        let message = panic_message(panic);
        assert!(message.contains("Unsupported --present-mode"));
        assert!(message.contains("mailbox"));
    }

    #[test]
    fn missing_water_profile_panics_with_helpful_message() {
        let panic = std::panic::catch_unwind(|| parse(&["verdarium", "--water-profile"]))
            .expect_err("missing water profile should panic");
        assert!(panic_message(panic).contains("Missing value for --water-profile"));
    }

    #[test]
    fn missing_audio_output_device_panics_with_helpful_message() {
        let panic = std::panic::catch_unwind(|| parse(&["verdarium", "--audio-output-device"]))
            .expect_err("missing audio output device should panic");
        assert!(panic_message(panic).contains("Missing value for --audio-output-device"));
    }

    fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
        if let Some(message) = panic.downcast_ref::<&str>() {
            (*message).to_owned()
        } else if let Some(message) = panic.downcast_ref::<String>() {
            message.clone()
        } else {
            "<non-string panic>".to_owned()
        }
    }
}
