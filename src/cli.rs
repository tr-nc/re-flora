use re_flora_vkn::PresentMode;

use crate::ddgi::{
    supported_ddgi_spacings_label, validate_ddgi_spacing, DdgiBatchOrder, DdgiCaptureTarget,
    DdgiConsumerVisibility, DdgiDebugView, DdgiTerrainHardOrigin, DEFAULT_DDGI_SPACING_VOXELS,
};

pub const CAMERA_SNAPSHOT_LIST_HINT: &str =
    "Run `re-flora --list-camera-snapshots` to list available camera snapshots.";

const SCREENSHOT_USAGE: &str = "Expected `--screenshot <preset> <path> --screenshot-delay <sec>`.";
const DENOISER_BENCH_USAGE: &str = "Expected `--denoiser-bench <preset> <report.toml>`.";

pub const DEFAULT_DENOISER_BENCH_WARMUP_FRAMES: u32 = 90;
pub const DEFAULT_DENOISER_BENCH_CAPTURE_FRAMES: u32 = 64;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnvironmentLightingTestCase {
    #[default]
    Sealed,
    PattSeam,
    Portal,
    Walls,
    Donor,
    Dogleg,
    RadianceChanges,
    DensityChanges,
    TerrainEdits,
    TerrainEditsInflight,
    TerrainEditsInflightCapture,
    TerrainEditsClosed,
}

impl EnvironmentLightingTestCase {
    fn from_cli_value(value: &str) -> Option<Self> {
        match value {
            "sealed" => Some(Self::Sealed),
            "patt-seam" => Some(Self::PattSeam),
            "portal" => Some(Self::Portal),
            "walls" => Some(Self::Walls),
            "donor" => Some(Self::Donor),
            "dogleg" => Some(Self::Dogleg),
            "radiance-changes" => Some(Self::RadianceChanges),
            "density-changes" => Some(Self::DensityChanges),
            "terrain-edits" => Some(Self::TerrainEdits),
            "terrain-edits-inflight" => Some(Self::TerrainEditsInflight),
            "terrain-edits-inflight-capture" => Some(Self::TerrainEditsInflightCapture),
            "terrain-edits-closed" => Some(Self::TerrainEditsClosed),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sealed => "sealed",
            Self::PattSeam => "patt-seam",
            Self::Portal => "portal",
            Self::Walls => "walls",
            Self::Donor => "donor",
            Self::Dogleg => "dogleg",
            Self::RadianceChanges => "radiance-changes",
            Self::DensityChanges => "density-changes",
            Self::TerrainEdits => "terrain-edits",
            Self::TerrainEditsInflight => "terrain-edits-inflight",
            Self::TerrainEditsInflightCapture => "terrain-edits-inflight-capture",
            Self::TerrainEditsClosed => "terrain-edits-closed",
        }
    }
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
    /// Load a terrain-only authoritative voxel snapshot during startup.
    pub terrain_load_path: Option<String>,
    /// Save a terrain-only authoritative voxel snapshot after startup reaches readiness.
    pub terrain_save_path: Option<String>,
    /// Apply a named camera snapshot at startup. Screenshot runs set this from the requested preset.
    pub camera_snapshot: Option<String>,
    /// Run a fixed-camera temporal stability benchmark and write a TOML report.
    pub denoiser_bench: Option<DenoiserBenchOptions>,
    /// Print available camera snapshot names and exit successfully.
    pub list_camera_snapshots: bool,
    /// Auto-exit N seconds after rendering starts. None = don't auto-exit.
    pub auto_exit_delay: Option<f32>,
    /// Exercise egui full, partial, replacement, and free texture generations, then exit via
    /// the normal hidden-render path. Intended for lifecycle acceptance only.
    pub egui_texture_lifecycle_test: bool,
    /// Exercise coalesced programmatic window resizes through the normal swapchain/render path.
    /// Intended for hidden resize acceptance only.
    pub resize_lifecycle_test: bool,
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
    /// Build one deterministic static terrain case for environment-lighting validation.
    pub environment_lighting_test_scene: Option<EnvironmentLightingTestCase>,
    /// Save one pre-albedo linear environment-irradiance capture when the backend is ready.
    pub environment_irradiance_capture_path: Option<String>,
    /// Select the complete DDGI field recorded by the one-shot irradiance capture.
    pub environment_irradiance_capture_target: DdgiCaptureTarget,
    /// Select deterministic forward or reverse DDGI probe-batch traversal.
    pub ddgi_batch_order: DdgiBatchOrder,
    /// Select a permanent DDGI diagnostic view; exact modes are correctness-only and expensive.
    pub ddgi_debug_view: DdgiDebugView,
    /// Select visibility terms for steady-state DDGI consumer performance experiments.
    pub ddgi_consumer_visibility: DdgiConsumerVisibility,
    /// Select the terrain-only exact-visibility origin used for receiver diagnostics.
    pub ddgi_terrain_hard_origin: DdgiTerrainHardOrigin,
    /// Build a deterministic hybrid raster/terrain transparency regression scene.
    pub hybrid_transparency_test_scene: bool,
    /// Environment probe grid spacing in terrain voxels.
    pub environment_probe_spacing_voxels: u32,
    /// Rebuild the environment probe grid once at runtime with this spacing.
    pub environment_probe_rebuild_spacing_voxels: Option<u32>,
    /// Visualize the environment probe grid at startup.
    pub environment_probe_visualization: bool,
    /// Run the lightweight tree replacement benchmark and exit after completion.
    pub tree_bench: bool,
    /// Number of tree benchmark samples.
    pub tree_bench_samples: u32,
    /// Do not wait for deferred rebuilds between tree benchmark samples.
    pub tree_bench_rapid: bool,
    /// Run the authored special-flora paint benchmark and exit after completion.
    pub authored_flora_bench: bool,
    /// Number of authored flora benchmark paint samples.
    pub authored_flora_bench_samples: u32,
    /// Print the per-worktree run log directory and exit successfully.
    pub print_log_dir: bool,
    /// Print the latest run log path and exit successfully.
    pub latest_log: bool,
    /// Print the last N lines from the latest run log and exit successfully.
    pub tail_latest_log: Option<usize>,
    /// Print CLI help and exit successfully.
    pub help: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DenoiserBenchOptions {
    pub report_path: String,
    pub warmup_frames: u32,
    pub capture_frames: u32,
    pub camera_motion: bool,
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
                None => MonitorScorePreference::Highest,
            };
        let environment_probe_spacing_voxels = match parse_required_string_after(
            "--environment-probe-spacing-voxels",
            "one of: 64, 32, 16, 8",
        )? {
            Some(value) => {
                let parsed = value.parse::<u32>().map_err(|_| {
                    format!(
                        "Invalid --environment-probe-spacing-voxels '{value}'. Supported values: {}",
                        supported_ddgi_spacings_label()
                    )
                })?;
                validate_ddgi_spacing(parsed)?
            }
            None => DEFAULT_DDGI_SPACING_VOXELS,
        };
        let environment_probe_rebuild_spacing_voxels = match parse_required_string_after(
            "--environment-probe-rebuild-spacing-voxels",
            "one of: 64, 32, 16, 8",
        )? {
            Some(value) => {
                let parsed = value.parse::<u32>().map_err(|_| {
                    format!(
                        "Invalid --environment-probe-rebuild-spacing-voxels '{value}'. Supported values: {}",
                        supported_ddgi_spacings_label()
                    )
                })?;
                Some(validate_ddgi_spacing(parsed)?)
            }
            None => None,
        };
        let environment_lighting_test_scene = parse_environment_lighting_test_scene(&args)?;
        let environment_irradiance_capture_path = parse_required_string_after(
            "--environment-irradiance-capture",
            "an output .rfirr path",
        )?;
        let environment_irradiance_capture_target_value = parse_required_string_after(
            "--environment-irradiance-capture-target",
            "s0, s1, sN, converged, non-converged, or published",
        )?;
        if environment_irradiance_capture_target_value.is_some()
            && environment_irradiance_capture_path.is_none()
        {
            return Err(
                "--environment-irradiance-capture-target requires --environment-irradiance-capture"
                    .to_owned(),
            );
        }
        let environment_irradiance_capture_target =
            match environment_irradiance_capture_target_value {
                Some(value) => DdgiCaptureTarget::from_cli_value(&value).ok_or_else(|| {
                    format!(
                        "Invalid --environment-irradiance-capture-target '{value}'. Expected s0, s1, sN, converged, non-converged, or published."
                    )
                })?,
                None => DdgiCaptureTarget::default(),
            };
        let ddgi_batch_order =
            match parse_required_string_after("--ddgi-batch-order", "one of: forward, reverse")? {
                Some(value) => DdgiBatchOrder::from_cli_value(&value).ok_or_else(|| {
                    format!(
                        "Invalid --ddgi-batch-order '{value}'. Expected one of: forward, reverse."
                    )
                })?,
                None => DdgiBatchOrder::Forward,
            };
        let ddgi_debug_view = match parse_required_string_after(
            "--ddgi-debug-view",
            "one of: final, moment-visibility, exact-visibility, visibility-error, exact-irradiance, unoccluded-irradiance, equal-weight-irradiance, irradiance-error, weight-sum, dominant-probe, probe-state, relocation, irradiance-atlas, visibility-atlas",
        )? {
            Some(value) => DdgiDebugView::from_cli_value(&value).ok_or_else(|| {
                format!(
                    "Invalid --ddgi-debug-view '{value}'. Expected one of: final, moment-visibility, exact-visibility, visibility-error, exact-irradiance, unoccluded-irradiance, equal-weight-irradiance, irradiance-error, weight-sum, dominant-probe, probe-state, relocation, irradiance-atlas, visibility-atlas."
                )
            })?,
            None => DdgiDebugView::Final,
        };
        let ddgi_consumer_visibility = match parse_required_string_after(
            "--ddgi-consumer-visibility",
            "one of: full, moment-only, exact-only, none",
        )? {
            Some(value) => DdgiConsumerVisibility::from_cli_value(&value).ok_or_else(|| {
                format!(
                    "Invalid --ddgi-consumer-visibility '{value}'. Expected one of: full, moment-only, exact-only, none."
                )
            })?,
            None => DdgiConsumerVisibility::Full,
        };
        let ddgi_terrain_hard_origin = match parse_required_string_after(
            "--ddgi-terrain-hard-origin",
            "one of: surface-quarter, center-fixed, surface-fixed",
        )? {
            Some(value) => DdgiTerrainHardOrigin::from_cli_value(&value).ok_or_else(|| {
                format!(
                    "Invalid --ddgi-terrain-hard-origin '{value}'. Expected one of: surface-quarter, center-fixed, surface-fixed."
                )
            })?,
            None => DdgiTerrainHardOrigin::default(),
        };
        if environment_irradiance_capture_target.iteration() == Some(0)
            && ddgi_debug_view != DdgiDebugView::Final
        {
            return Err(
                "--environment-irradiance-capture-target s0 requires --ddgi-debug-view final"
                    .to_owned(),
            );
        }
        let screenshot = parse_screenshot_request(&args)?;
        let denoiser_bench = parse_denoiser_bench_request(&args, &parse_u32_after)?;
        if screenshot.is_some() && denoiser_bench.is_some() {
            return Err(format!(
                "Do not combine --screenshot with --denoiser-bench. {DENOISER_BENCH_USAGE}"
            ));
        }
        let camera_snapshot = if let Some(screenshot) = &screenshot {
            if args.iter().any(|a| a == "--camera-snapshot") {
                return Err(format!(
                    "Do not combine --camera-snapshot with --screenshot. {SCREENSHOT_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
                ));
            }
            Some(screenshot.preset_name.clone())
        } else if let Some((preset_name, _)) = &denoiser_bench {
            if args.iter().any(|a| a == "--camera-snapshot") {
                return Err(format!(
                    "Do not combine --camera-snapshot with --denoiser-bench. {DENOISER_BENCH_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
                ));
            }
            Some(preset_name.clone())
        } else {
            parse_required_string_after("--camera-snapshot", "a camera snapshot name")?
        };
        let screenshot_path = screenshot
            .as_ref()
            .map(|screenshot| screenshot.path.clone());
        let screenshot_delay = screenshot.as_ref().map(|screenshot| screenshot.delay);
        let terrain_load_path =
            parse_required_string_after("--terrain-load", "a terrain snapshot path")?;
        let terrain_save_path =
            parse_required_string_after("--terrain-save", "a terrain snapshot path")?;
        if terrain_load_path.is_some()
            && (environment_lighting_test_scene.is_some()
                || args.iter().any(|arg| {
                    arg == "--hybrid-transparency-test-scene" || arg == "--water-edit-soak"
                }))
        {
            return Err(
                "Do not combine --terrain-load with terrain-stamping test scenes or --water-edit-soak"
                    .to_owned(),
            );
        }

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
            terrain_load_path,
            terrain_save_path,
            camera_snapshot,
            denoiser_bench: denoiser_bench.map(|(_, options)| options),
            list_camera_snapshots: args.iter().any(|a| a == "--list-camera-snapshots"),
            auto_exit_delay: parse_f32_after("--auto-exit"),
            egui_texture_lifecycle_test: args.iter().any(|a| a == "--egui-texture-lifecycle-test"),
            resize_lifecycle_test: args.iter().any(|a| a == "--resize-lifecycle-test"),
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
            environment_lighting_test_scene,
            environment_irradiance_capture_path,
            environment_irradiance_capture_target,
            ddgi_batch_order,
            ddgi_debug_view,
            ddgi_consumer_visibility,
            ddgi_terrain_hard_origin,
            hybrid_transparency_test_scene: args
                .iter()
                .any(|a| a == "--hybrid-transparency-test-scene"),
            environment_probe_spacing_voxels,
            environment_probe_rebuild_spacing_voxels,
            environment_probe_visualization: args
                .iter()
                .any(|a| a == "--environment-probe-visualization"),
            tree_bench: args.iter().any(|a| a == "--tree-bench"),
            tree_bench_samples: parse_u32_after("--tree-bench-samples").unwrap_or(10),
            tree_bench_rapid: args.iter().any(|a| a == "--tree-bench-rapid"),
            authored_flora_bench: args.iter().any(|a| a == "--authored-flora-bench"),
            authored_flora_bench_samples: parse_u32_after("--authored-flora-bench-samples")
                .unwrap_or(25),
            print_log_dir: args.iter().any(|a| a == "--print-log-dir"),
            latest_log: args.iter().any(|a| a == "--latest-log"),
            tail_latest_log,
            help: args.iter().any(|a| a == "--help" || a == "-h"),
        })
    }
}

fn parse_denoiser_bench_request(
    args: &[String],
    parse_u32_after: &impl Fn(&str) -> Option<u32>,
) -> Result<Option<(String, DenoiserBenchOptions)>, String> {
    let Some(index) = args.iter().position(|arg| arg == "--denoiser-bench") else {
        if args.iter().any(|arg| {
            arg == "--denoiser-bench-warmup-frames"
                || arg == "--denoiser-bench-frames"
                || arg == "--denoiser-bench-camera-motion"
        }) {
            return Err(format!(
                "Denoiser benchmark frame options require --denoiser-bench. {DENOISER_BENCH_USAGE}"
            ));
        }
        return Ok(None);
    };

    let preset_name = required_denoiser_bench_arg(args, index + 1, "preset name")?;
    let report_path = required_denoiser_bench_arg(args, index + 2, "report path")?;
    let warmup_frames = parse_u32_after("--denoiser-bench-warmup-frames")
        .unwrap_or(DEFAULT_DENOISER_BENCH_WARMUP_FRAMES);
    let capture_frames =
        parse_u32_after("--denoiser-bench-frames").unwrap_or(DEFAULT_DENOISER_BENCH_CAPTURE_FRAMES);
    if capture_frames < 2 {
        return Err("--denoiser-bench-frames must be at least 2".to_owned());
    }

    Ok(Some((
        preset_name,
        DenoiserBenchOptions {
            report_path,
            warmup_frames,
            capture_frames,
            camera_motion: args
                .iter()
                .any(|arg| arg == "--denoiser-bench-camera-motion"),
        },
    )))
}

fn parse_environment_lighting_test_scene(
    args: &[String],
) -> Result<Option<EnvironmentLightingTestCase>, String> {
    let indices = args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| (arg == "--environment-lighting-test-scene").then_some(index))
        .collect::<Vec<_>>();
    if indices.is_empty() {
        return Ok(None);
    }
    if indices.len() > 1 {
        return Err("Only one --environment-lighting-test-scene is supported.".to_owned());
    }

    let value = args
        .get(indices[0] + 1)
        .filter(|value| !value.starts_with("--"));
    match value {
        None => Ok(Some(EnvironmentLightingTestCase::Sealed)),
        Some(value) => EnvironmentLightingTestCase::from_cli_value(value)
            .map(Some)
            .ok_or_else(|| {
                format!(
                    "Invalid --environment-lighting-test-scene '{value}'. Expected one of: sealed, patt-seam, portal, walls, donor, dogleg, radiance-changes, density-changes, terrain-edits, terrain-edits-inflight, terrain-edits-inflight-capture, terrain-edits-closed."
                )
            }),
    }
}

fn required_denoiser_bench_arg(
    args: &[String],
    index: usize,
    label: &str,
) -> Result<String, String> {
    let Some(value) = args.get(index) else {
        return Err(format!(
            "Missing denoiser benchmark {label}. {DENOISER_BENCH_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
        ));
    };
    if value.starts_with("--") {
        return Err(format!(
            "Missing denoiser benchmark {label}. {DENOISER_BENCH_USAGE}\n{CAMERA_SNAPSHOT_LIST_HINT}"
        ));
    }
    Ok(value.clone())
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
  re-flora [options]

Options:
  --windowed                  Run in windowed mode (default: borderless fullscreen)
  --hidden                    Run hidden while preserving render/swapchain path; audio output remains enabled unless --mute is set
  --mute                      Start with global audio output muted while keeping audio processing active
  --audio-output-device <text>
                              Select output device by case-insensitive substring/alias match
  --list-audio-output-devices Print output devices visible to PetalSonic/CPAL and exit
  --no-shadows                Disable shadow rendering passes
  --no-god-rays               Disable god ray pass
  --no-lens-flare             Disable lens flare passes
  --no-tracer                 Disable main tracer pass
  --no-particles              Disable particle simulation and rendering
  --no-flora                  Disable flora and leaves rendering
  --no-clouds                 Disable procedural cloud rendering
  --present-mode <mode>       Override auto present mode selection: mailbox, immediate, fifo, fifo_relaxed
  --monitor-score <mode>      Select borderless fullscreen monitor by resolution score: highest, lowest (default: highest)
  --swapchain-images <N>      Override swapchain image count (default: auto)
  --screenshot <preset> <path>
                              Save one screenshot from exactly one camera snapshot preset
  --screenshot-delay <sec>    Required delay before screenshot capture when --screenshot is used
  --terrain-load <path>      Load a terrain-only voxel snapshot during startup
  --terrain-save <path>      Save a terrain-only voxel snapshot once startup is ready
  --denoiser-bench <preset> <report.toml>
                              Capture a frame sequence and write temporal metrics
  --denoiser-bench-warmup-frames <N>
                              Frames discarded before capture (default: 90)
  --denoiser-bench-frames <N> Captured frames, at least 2 (default: 64)
  --denoiser-bench-camera-motion
                              Apply deterministic camera motion and retain up to four review keyframes
  --camera-snapshot <name>    Apply a saved camera snapshot at startup (do not combine with --screenshot)
  --list-camera-snapshots     Print available camera snapshot names and exit
  --auto-exit <sec>           Exit automatically after rendering starts
  --egui-texture-lifecycle-test
                              Exercise egui texture generations through full/partial/free updates
  --resize-lifecycle-test     Exercise coalesced programmatic resizes through the render path
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
  --environment-lighting-test-scene [case]
                              Build a lighting case: sealed (default), patt-seam, portal, walls, donor, dogleg,
                              radiance-changes, density-changes, terrain-edits,
                              terrain-edits-inflight, terrain-edits-inflight-capture, or
                              terrain-edits-closed
  --environment-irradiance-capture <path>
                              Save DDGI metadata, pre-albedo irradiance/hit mask, world hit, and exact sun visibility
  --environment-irradiance-capture-target <target>
                              Capture s0, s1, a specified sN, converged, or non-converged (default: s1)
  --ddgi-batch-order <order>  Traverse DDGI probe batches in forward or reverse order (default: forward)
  --ddgi-debug-view <view>    Select final, moment/exact visibility, error, weight, probe, relocation,
                              or atlas DDGI diagnostics (default: final)
  --ddgi-consumer-visibility <mode>
                              Select full, moment-only, exact-only, or none for consumer perf A/B
                              (default: full; probe transport remains full)
  --ddgi-terrain-hard-origin <mode>
                              Select surface-quarter, center-fixed, or surface-fixed exact visibility origin
                              for terrain receiver experiments (default: {})
  --hybrid-transparency-test-scene
                              Build the deterministic raster/terrain transparency regression scene
  --environment-probe-spacing-voxels <N>
                              Set environment probe spacing: 64, 32, 16, or 8 (default: 32)
  --environment-probe-rebuild-spacing-voxels <N>
                              Rebuild probes once after rendering starts, for runtime validation
  --environment-probe-visualization
                              Visualize the environment probe grid (debug; default: off)
  --tree-bench                Run tree replacement benchmark and exit
  --tree-bench-samples <N>    Tree benchmark samples (default: 10)
  --tree-bench-rapid          Do not wait for deferred rebuilds between samples
  --authored-flora-bench      Run authored special-flora paint benchmark and exit
  --authored-flora-bench-samples <N>
                              Authored flora benchmark paint samples (default: 25)
  --print-log-dir             Print the per-worktree run log directory and exit
  --latest-log                Print the latest run log path and exit
  --tail-latest-log [N]       Print the last N lines of the latest run log and exit (default: 200)
  -h, --help                  Show this help and exit

Examples:
  re-flora --windowed
  re-flora --hidden --mute --auto-exit 20 --perf
  re-flora --audio-output-device KA3
  re-flora --list-audio-output-devices
  re-flora --hidden --mute --screenshot player-default screenshots/check.png --screenshot-delay 2 --auto-exit 4
  re-flora --present-mode fifo
  re-flora --monitor-score lowest
  re-flora --swapchain-images 2
  re-flora --no-shadows
  re-flora --hidden --mute --screenshot tree-closeup out.png --screenshot-delay 2 --auto-exit 4
  re-flora --hidden --mute --windowed --denoiser-bench player-default target/denoiser.toml
  re-flora --list-camera-snapshots
  re-flora --auto-exit 10 --perf
  re-flora --hidden --mute --auto-exit 4 --perf --water-profile performance
  re-flora --hidden --mute --auto-exit 4 --perf --water-particles 35000 --water-particle-edge-len 0.05
  re-flora --hidden --mute --auto-exit 4 --perf --water-profile performance --water-damping 1.5 --water-terrain-margin-cells 0.0
  re-flora --hidden --mute --auto-exit 14 --perf --water-profile performance --water-edit-soak
  re-flora --hidden --mute --environment-lighting-test-scene sealed --environment-irradiance-capture target/sealed.rfirr --auto-exit 8
  re-flora --hidden --mute --windowed --hybrid-transparency-test-scene --screenshot player-default target/hybrid-transparency-test.png --screenshot-delay 2 --auto-exit 6
  re-flora --latest-log
  re-flora --tail-latest-log 120
  re-flora --windowed --tree-bench --tree-bench-samples 10"#,
        DdgiTerrainHardOrigin::default().label()
    );
}

#[derive(Clone, Debug)]
pub struct RenderFlags {
    pub enable_shadows: bool,
    pub enable_god_rays: bool,
    pub enable_lens_flare: bool,
    pub enable_tracer: bool,
    pub enable_flora: bool,
    pub enable_leaves: bool,
    pub enable_particles: bool,
    pub enable_clouds: bool,
}

impl From<&AppOptions> for RenderFlags {
    fn from(options: &AppOptions) -> Self {
        Self {
            enable_shadows: !options.no_shadows,
            enable_god_rays: !options.no_god_rays,
            enable_lens_flare: !options.no_lens_flare,
            enable_tracer: !options.no_tracer,
            enable_flora: !options.no_flora,
            enable_leaves: !options.no_flora,
            enable_particles: !options.no_particles,
            // Disabled for now; infrastructure kept for easy re-enable.
            enable_clouds: false,
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
        let options = parse(&["re-flora"]);

        assert!(!options.windowed);
        assert!(!options.hidden);
        assert!(!options.mute);
        assert!(options.audio_output_device.is_none());
        assert!(!options.list_audio_output_devices);
        assert!(!options.perf);
        assert!(options.present_mode.is_none());
        assert!(matches!(
            options.monitor_score,
            MonitorScorePreference::Highest
        ));
        assert!(options.screenshot_path.is_none());
        assert!(options.screenshot_delay.is_none());
        assert!(options.terrain_load_path.is_none());
        assert!(options.terrain_save_path.is_none());
        assert!(options.camera_snapshot.is_none());
        assert!(!options.list_camera_snapshots);
        assert!(!options.egui_texture_lifecycle_test);
        assert!(!options.resize_lifecycle_test);
        assert!(options.environment_lighting_test_scene.is_none());
        assert!(options.environment_irradiance_capture_path.is_none());
        assert_eq!(
            options.environment_irradiance_capture_target,
            DdgiCaptureTarget::Iteration(1)
        );
        assert_eq!(options.ddgi_batch_order, DdgiBatchOrder::Forward);
        assert_eq!(options.ddgi_debug_view, DdgiDebugView::Final);
        assert_eq!(
            options.ddgi_consumer_visibility,
            DdgiConsumerVisibility::Full
        );
        assert_eq!(
            options.ddgi_terrain_hard_origin,
            DdgiTerrainHardOrigin::SurfaceFixedWorld
        );
        assert_eq!(
            options.environment_probe_spacing_voxels,
            DEFAULT_DDGI_SPACING_VOXELS
        );
        assert!(options.environment_probe_rebuild_spacing_voxels.is_none());
        assert!(!options.environment_probe_visualization);
        assert_eq!(options.tree_bench_samples, 10);
        assert!(!options.authored_flora_bench);
        assert_eq!(options.authored_flora_bench_samples, 25);
        assert!(options.tail_latest_log.is_none());
    }

    #[test]
    fn parses_authored_flora_bench_options() {
        let options = parse(&[
            "re-flora",
            "--authored-flora-bench",
            "--authored-flora-bench-samples",
            "7",
        ]);

        assert!(options.authored_flora_bench);
        assert_eq!(options.authored_flora_bench_samples, 7);
    }

    #[test]
    fn parses_lifecycle_acceptance_options() {
        let options = parse(&[
            "re-flora",
            "--egui-texture-lifecycle-test",
            "--resize-lifecycle-test",
        ]);

        assert!(options.egui_texture_lifecycle_test);
        assert!(options.resize_lifecycle_test);
    }

    #[test]
    fn parses_environment_lighting_test_scene() {
        let options = parse(&["re-flora", "--environment-lighting-test-scene"]);

        assert_eq!(
            options.environment_lighting_test_scene,
            Some(EnvironmentLightingTestCase::Sealed)
        );
    }

    #[test]
    fn parses_named_environment_lighting_test_scenes() {
        for (name, expected) in [
            ("sealed", EnvironmentLightingTestCase::Sealed),
            ("patt-seam", EnvironmentLightingTestCase::PattSeam),
            ("portal", EnvironmentLightingTestCase::Portal),
            ("walls", EnvironmentLightingTestCase::Walls),
            ("donor", EnvironmentLightingTestCase::Donor),
            ("dogleg", EnvironmentLightingTestCase::Dogleg),
            (
                "radiance-changes",
                EnvironmentLightingTestCase::RadianceChanges,
            ),
            (
                "density-changes",
                EnvironmentLightingTestCase::DensityChanges,
            ),
            ("terrain-edits", EnvironmentLightingTestCase::TerrainEdits),
            (
                "terrain-edits-inflight",
                EnvironmentLightingTestCase::TerrainEditsInflight,
            ),
            (
                "terrain-edits-inflight-capture",
                EnvironmentLightingTestCase::TerrainEditsInflightCapture,
            ),
            (
                "terrain-edits-closed",
                EnvironmentLightingTestCase::TerrainEditsClosed,
            ),
        ] {
            let options = parse(&["re-flora", "--environment-lighting-test-scene", name]);
            assert_eq!(options.environment_lighting_test_scene, Some(expected));
        }
    }

    #[test]
    fn rejects_unknown_environment_lighting_test_scene() {
        let result = AppOptions::try_from_arg_strings(
            ["re-flora", "--environment-lighting-test-scene", "dynamic"]
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect(),
        );

        assert!(result.unwrap_err().contains(
            "sealed, patt-seam, portal, walls, donor, dogleg, radiance-changes, density-changes, terrain-edits, terrain-edits-inflight, terrain-edits-inflight-capture, terrain-edits-closed"
        ));
    }

    #[test]
    fn parses_environment_irradiance_capture_path() {
        let options = parse(&[
            "re-flora",
            "--environment-irradiance-capture",
            "target/sealed.rfirr",
        ]);

        assert_eq!(
            options.environment_irradiance_capture_path.as_deref(),
            Some("target/sealed.rfirr")
        );
        assert_eq!(
            options.environment_irradiance_capture_target,
            DdgiCaptureTarget::Iteration(1)
        );
    }

    #[test]
    fn parses_environment_irradiance_capture_target() {
        let options = parse(&[
            "re-flora",
            "--environment-irradiance-capture",
            "target/sealed-s4.rfirr",
            "--environment-irradiance-capture-target",
            "s4",
        ]);

        assert_eq!(
            options.environment_irradiance_capture_target,
            DdgiCaptureTarget::Iteration(4)
        );
    }

    #[test]
    fn parses_ddgi_batch_order() {
        let options = parse(&["re-flora", "--ddgi-batch-order", "reverse"]);
        assert_eq!(options.ddgi_batch_order, DdgiBatchOrder::Reverse);

        let result = AppOptions::try_from_arg_strings(
            ["re-flora", "--ddgi-batch-order", "inside-out"]
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect(),
        );
        assert!(result
            .unwrap_err()
            .contains("Expected one of: forward, reverse"));
    }

    #[test]
    fn rejects_capture_target_without_capture_path() {
        let result = AppOptions::try_from_arg_strings(
            ["re-flora", "--environment-irradiance-capture-target", "s2"]
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect(),
        );

        assert!(result.unwrap_err().contains(
            "--environment-irradiance-capture-target requires --environment-irradiance-capture"
        ));
    }

    #[test]
    fn parses_ddgi_debug_view() {
        let options = parse(&["re-flora", "--ddgi-debug-view", "exact-visibility"]);
        assert_eq!(options.ddgi_debug_view, DdgiDebugView::ExactVisibility);

        let options = parse(&["re-flora", "--ddgi-debug-view", "unoccluded-irradiance"]);
        assert_eq!(options.ddgi_debug_view, DdgiDebugView::UnoccludedIrradiance);

        let options = parse(&["re-flora", "--ddgi-debug-view", "equal-weight-irradiance"]);
        assert_eq!(
            options.ddgi_debug_view,
            DdgiDebugView::EqualWeightIrradiance
        );
    }

    #[test]
    fn parses_ddgi_consumer_visibility() {
        for (value, expected) in [
            ("full", DdgiConsumerVisibility::Full),
            ("moment-only", DdgiConsumerVisibility::MomentOnly),
            ("exact-only", DdgiConsumerVisibility::ExactOnly),
            ("none", DdgiConsumerVisibility::None),
        ] {
            let options = parse(&["re-flora", "--ddgi-consumer-visibility", value]);
            assert_eq!(options.ddgi_consumer_visibility, expected);
        }
    }

    #[test]
    fn parses_ddgi_terrain_hard_origin() {
        for (value, expected) in [
            (
                "surface-quarter",
                DdgiTerrainHardOrigin::SurfaceQuarterVoxel,
            ),
            ("center-fixed", DdgiTerrainHardOrigin::CenterFixedWorld),
            ("surface-fixed", DdgiTerrainHardOrigin::SurfaceFixedWorld),
        ] {
            let options = parse(&["re-flora", "--ddgi-terrain-hard-origin", value]);
            assert_eq!(options.ddgi_terrain_hard_origin, expected);
        }
    }

    #[test]
    fn defaults_ddgi_terrain_hard_origin_to_surface_fixed() {
        let options = parse(&["re-flora"]);
        assert_eq!(
            options.ddgi_terrain_hard_origin,
            DdgiTerrainHardOrigin::SurfaceFixedWorld
        );
        assert_eq!(DdgiTerrainHardOrigin::default().label(), "surface-fixed");
    }

    #[test]
    fn rejects_invalid_ddgi_terrain_hard_origin() {
        let result = AppOptions::try_from_arg_strings(
            ["re-flora", "--ddgi-terrain-hard-origin", "first-empty"]
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect(),
        );

        assert!(result
            .unwrap_err()
            .contains("Expected one of: surface-quarter, center-fixed, surface-fixed"));
    }

    #[test]
    fn rejects_unpublished_s0_capture_with_non_final_debug_view() {
        let result = AppOptions::try_from_arg_strings(
            [
                "re-flora",
                "--environment-irradiance-capture",
                "target/sealed-s0.rfirr",
                "--environment-irradiance-capture-target",
                "s0",
                "--ddgi-debug-view",
                "exact-irradiance",
            ]
            .iter()
            .map(|arg| (*arg).to_owned())
            .collect(),
        );

        assert!(result
            .unwrap_err()
            .contains("s0 requires --ddgi-debug-view final"));
    }

    #[test]
    fn parses_hybrid_transparency_test_scene() {
        let options = parse(&["re-flora", "--hybrid-transparency-test-scene"]);

        assert!(options.hybrid_transparency_test_scene);
    }

    #[test]
    fn parses_terrain_snapshot_load_save_paths() {
        let options = parse(&[
            "re-flora",
            "--terrain-load",
            "target/input.rflterrain",
            "--terrain-save",
            "target/output.rflterrain",
        ]);

        assert_eq!(
            options.terrain_load_path.as_deref(),
            Some("target/input.rflterrain")
        );
        assert_eq!(
            options.terrain_save_path.as_deref(),
            Some("target/output.rflterrain")
        );
    }

    #[test]
    fn terrain_load_requires_a_path_and_rejects_stamping_scenes() {
        let missing = AppOptions::try_from_arg_strings(
            ["re-flora", "--terrain-load"]
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect(),
        )
        .unwrap_err();
        assert!(missing.contains("Missing value for --terrain-load"));

        let incompatible = AppOptions::try_from_arg_strings(
            [
                "re-flora",
                "--terrain-load",
                "target/input.rflterrain",
                "--hybrid-transparency-test-scene",
            ]
            .iter()
            .map(|arg| (*arg).to_owned())
            .collect(),
        )
        .unwrap_err();
        assert!(incompatible.contains("terrain-stamping test scenes"));
    }

    #[test]
    fn parses_environment_probe_spacing() {
        let options = parse(&[
            "re-flora",
            "--environment-probe-spacing-voxels",
            "16",
            "--environment-probe-rebuild-spacing-voxels",
            "32",
            "--environment-probe-visualization",
        ]);

        assert_eq!(options.environment_probe_spacing_voxels, 16);
        assert_eq!(options.environment_probe_rebuild_spacing_voxels, Some(32));
        assert!(options.environment_probe_visualization);
    }

    #[test]
    fn rejects_unsupported_environment_probe_spacing() {
        let result = AppOptions::try_from_arg_strings(
            ["re-flora", "--environment-probe-spacing-voxels", "24"]
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect(),
        );

        assert!(result
            .unwrap_err()
            .contains("Supported values: 64, 32, 16, 8"));
    }

    #[test]
    fn rejects_unsupported_environment_probe_rebuild_spacing() {
        let result = AppOptions::try_from_arg_strings(
            [
                "re-flora",
                "--environment-probe-rebuild-spacing-voxels",
                "24",
            ]
            .iter()
            .map(|arg| (*arg).to_owned())
            .collect(),
        );

        assert!(result
            .unwrap_err()
            .contains("Supported values: 64, 32, 16, 8"));
    }

    #[test]
    fn parses_common_perf_and_water_options() {
        let options = parse(&[
            "re-flora",
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
            "re-flora",
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
            "re-flora",
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
        let options = parse(&["re-flora", "--list-audio-output-devices"]);
        assert!(options.list_audio_output_devices);
    }

    #[test]
    fn parses_camera_snapshot_options() {
        let options = parse(&[
            "re-flora",
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
            "re-flora",
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
    fn parses_denoiser_benchmark_with_frame_overrides() {
        let options = parse(&[
            "re-flora",
            "--hidden",
            "--denoiser-bench",
            "player-default",
            "target/report.toml",
            "--denoiser-bench-warmup-frames",
            "12",
            "--denoiser-bench-frames",
            "8",
            "--denoiser-bench-camera-motion",
        ]);

        assert_eq!(options.camera_snapshot.as_deref(), Some("player-default"));
        let benchmark = options.denoiser_bench.unwrap();
        assert_eq!(benchmark.report_path, "target/report.toml");
        assert_eq!(benchmark.warmup_frames, 12);
        assert_eq!(benchmark.capture_frames, 8);
        assert!(benchmark.camera_motion);
    }

    #[test]
    fn denoiser_benchmark_camera_motion_requires_benchmark() {
        let panic =
            std::panic::catch_unwind(|| parse(&["re-flora", "--denoiser-bench-camera-motion"]))
                .expect_err("camera motion without a benchmark should panic");
        assert!(panic_message(panic).contains("require --denoiser-bench"));
    }

    #[test]
    fn denoiser_benchmark_requires_multiple_capture_frames() {
        let panic = std::panic::catch_unwind(|| {
            parse(&[
                "re-flora",
                "--denoiser-bench",
                "player-default",
                "target/report.toml",
                "--denoiser-bench-frames",
                "1",
            ])
        })
        .expect_err("single-frame temporal benchmark should panic");
        assert!(panic_message(panic).contains("must be at least 2"));
    }

    #[test]
    fn screenshot_requires_delay() {
        let panic = std::panic::catch_unwind(|| {
            parse(&["re-flora", "--screenshot", "tree-closeup", "out.png"])
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
                "re-flora",
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
        let options = parse(&["re-flora", "--tail-latest-log"]);
        assert_eq!(options.tail_latest_log, Some(200));
    }

    #[test]
    fn unsupported_present_mode_panics_with_helpful_message() {
        let panic = std::panic::catch_unwind(|| parse(&["re-flora", "--present-mode", "bad"]))
            .expect_err("unsupported present mode should panic");
        let message = panic_message(panic);
        assert!(message.contains("Unsupported --present-mode"));
        assert!(message.contains("mailbox"));
    }

    #[test]
    fn missing_water_profile_panics_with_helpful_message() {
        let panic = std::panic::catch_unwind(|| parse(&["re-flora", "--water-profile"]))
            .expect_err("missing water profile should panic");
        assert!(panic_message(panic).contains("Missing value for --water-profile"));
    }

    #[test]
    fn missing_audio_output_device_panics_with_helpful_message() {
        let panic = std::panic::catch_unwind(|| parse(&["re-flora", "--audio-output-device"]))
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
