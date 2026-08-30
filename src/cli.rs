use re_flora_vkn::PresentMode;
use std::collections::HashSet;

use crate::ddgi::{
    supported_ddgi_spacings_label, validate_ddgi_spacing, DdgiBatchOrder, DdgiCaptureTarget,
    DdgiDebugView, DdgiTerrainHardOrigin, DEFAULT_DDGI_SPACING_VOXELS,
};

pub const CAMERA_SNAPSHOT_LIST_HINT: &str =
    "Run `re-flora --list-camera-snapshots` to list available camera snapshots.";

const SCREENSHOT_USAGE: &str = "Expected `--screenshot <preset> <path> --screenshot-delay <sec>`.";
const DENOISER_BENCH_USAGE: &str = "Expected `--denoiser-bench <preset> <report.toml>`.";
const FOLIAGE_SHADOW_BENCH_USAGE: &str = "Expected `--foliage-shadow-bench <report.toml>`.";

pub const DEFAULT_DENOISER_BENCH_WARMUP_FRAMES: u32 = 90;
pub const DEFAULT_DENOISER_BENCH_CAPTURE_FRAMES: u32 = 64;
pub const DEFAULT_TERRAIN_CONNECTIVITY_BENCH_WARMUP_FRAMES: u32 = 600;
pub const DEFAULT_TERRAIN_CONNECTIVITY_BENCH_OBSERVE_FRAMES: u32 = 180;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMotion {
    Fixed,
    Scripted,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainConnectivityBenchMode {
    Existing,
    Correct,
    Bounded,
    Manual,
}

impl TerrainConnectivityBenchMode {
    fn from_cli_value(value: &str) -> Option<Self> {
        match value {
            "existing" => Some(Self::Existing),
            "correct" => Some(Self::Correct),
            "bounded" => Some(Self::Bounded),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Existing => "existing",
            Self::Correct => "correct",
            Self::Bounded => "bounded",
            Self::Manual => "manual",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainConnectivityBenchOptions {
    pub mode: TerrainConnectivityBenchMode,
    pub available_particles: usize,
    pub warmup_frames: u32,
    pub observe_frames: u32,
    pub voxel_budget: usize,
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
    PointLightChanges,
    VoxelEmissiveChanges,
    RasterEmitterChanges,
    MultiSourceStress,
    LocalLightScaling,
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
            "point-light-changes" => Some(Self::PointLightChanges),
            "voxel-emissive-changes" => Some(Self::VoxelEmissiveChanges),
            "raster-emitter-changes" => Some(Self::RasterEmitterChanges),
            "multi-source-stress" => Some(Self::MultiSourceStress),
            "local-light-scaling" => Some(Self::LocalLightScaling),
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
            Self::PointLightChanges => "point-light-changes",
            Self::VoxelEmissiveChanges => "voxel-emissive-changes",
            Self::RasterEmitterChanges => "raster-emitter-changes",
            Self::MultiSourceStress => "multi-source-stress",
            Self::LocalLightScaling => "local-light-scaling",
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

#[derive(Clone, Debug)]
pub enum LaunchCommand {
    Help,
    InspectLogs(LogInspection),
    ListCameraSnapshots,
    Run(RunPlan),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogInspection {
    pub print_directory: bool,
    pub print_latest_path: bool,
    pub tail_latest_lines: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct RunPlan {
    pub platform: PlatformPlan,
    pub audio: AudioPlan,
    pub world: WorldPlan,
    pub automation: AutomationPlan,
    pub scenario: Scenario,
}

#[derive(Clone, Debug)]
pub struct PlatformPlan {
    pub display: DisplayPlan,
    pub render: RenderPlan,
    pub lifecycle: LifecyclePlan,
}

#[derive(Clone, Debug)]
pub struct WorldPlan {
    pub terrain: TerrainPersistencePlan,
    pub water: WaterPlan,
    pub lighting: EnvironmentLightingPlan,
}

#[derive(Clone, Debug, Default)]
pub struct AutomationPlan {
    pub camera: CameraAutomation,
    pub benchmarks: BenchmarkPlan,
}

#[derive(Clone, Copy, Debug)]
pub struct DisplayPlan {
    pub windowed: bool,
    pub hidden: bool,
    pub present_mode: Option<PresentModePreference>,
    pub monitor_score: MonitorScorePreference,
    pub swapchain_images: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct AudioPlan {
    pub muted: bool,
    pub canopy_telemetry: bool,
    pub output_device: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RenderPlan {
    pub flags: RenderFlags,
    pub perf_logging: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerrainPersistencePlan {
    pub load_path: Option<String>,
    pub save_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum CameraAutomation {
    #[default]
    None,
    Snapshot(String),
    Screenshot {
        snapshot: String,
        capture: ScreenshotOptions,
    },
    DenoiserBenchmark {
        snapshot: String,
        benchmark: CameraDenoiserOptions,
    },
}

impl CameraAutomation {
    pub fn snapshot_name(&self) -> Option<&str> {
        match self {
            Self::Snapshot(name)
            | Self::Screenshot { snapshot: name, .. }
            | Self::DenoiserBenchmark { snapshot: name, .. } => Some(name),
            Self::None => None,
        }
    }

    pub fn screenshot(&self) -> Option<&ScreenshotOptions> {
        match self {
            Self::Screenshot { capture, .. } => Some(capture),
            _ => None,
        }
    }

    pub fn denoiser_benchmark(&self) -> Option<&CameraDenoiserOptions> {
        match self {
            Self::DenoiserBenchmark { benchmark, .. } => Some(benchmark),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LifecyclePlan {
    pub auto_exit_delay: Option<f32>,
    pub egui_texture_test: bool,
    pub resize_test: bool,
}

#[derive(Clone, Debug, Default)]
pub struct WaterPlan {
    pub profile: Option<WaterProfilePreference>,
    pub particles: Option<usize>,
    pub particle_edge_len: Option<f32>,
    pub grid: Option<u32>,
    pub substep_hz: Option<f32>,
    pub terrain_margin_cells: Option<f32>,
    pub damping: Option<f32>,
    pub terrain_tangent_damping: Option<f32>,
    pub stiffness: Option<f32>,
    pub gamma: Option<f32>,
    pub j_min: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct EnvironmentLightingPlan {
    pub irradiance_capture_path: Option<String>,
    pub spatial_weight_readback_path: Option<String>,
    pub capture_target: DdgiCaptureTarget,
    pub batch_order: DdgiBatchOrder,
    pub debug_view: DdgiDebugView,
    pub terrain_hard_origin: DdgiTerrainHardOrigin,
    pub probe_spacing_voxels: u32,
    pub rebuild_probe_spacing_voxels: Option<u32>,
    pub visualize_probes: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BenchmarkPlan {
    pub tree_samples: Option<u32>,
    pub authored_flora_samples: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Scenario {
    #[default]
    Garden,
    CanopyAudioDiagnostic {
        constrained_budget: bool,
    },
    WaterExperience,
    WaterEditSoak,
    EnvironmentLighting(EnvironmentLightingTestCase),
    HybridTransparency,
    House,
    TerrainConnectivityBenchmark(TerrainConnectivityBenchOptions),
    FoliageShadowBenchmark(FoliageDenoiserOptions),
}

impl Scenario {
    pub fn environment_lighting(&self) -> Option<EnvironmentLightingTestCase> {
        match self {
            Self::EnvironmentLighting(test_case) => Some(*test_case),
            _ => None,
        }
    }

    pub fn canopy_audio_diagnostic(&self) -> Option<bool> {
        match self {
            Self::CanopyAudioDiagnostic { constrained_budget } => Some(*constrained_budget),
            _ => None,
        }
    }

    pub fn terrain_connectivity_benchmark(&self) -> Option<TerrainConnectivityBenchOptions> {
        match self {
            Self::TerrainConnectivityBenchmark(options) => Some(*options),
            _ => None,
        }
    }

    pub fn foliage_shadow_benchmark(&self) -> Option<&FoliageDenoiserOptions> {
        match self {
            Self::FoliageShadowBenchmark(options) => Some(options),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DenoiserCaptureOptions {
    pub report_path: String,
    pub warmup_frames: u32,
    pub capture_frames: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraDenoiserOptions {
    pub capture: DenoiserCaptureOptions,
    pub camera_motion: CameraMotion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoliageDenoiserOptions {
    pub capture: DenoiserCaptureOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScreenshotOptions {
    pub path: String,
    pub delay: f32,
}

#[derive(Clone, Debug)]
struct ParsedScreenshot {
    preset_name: String,
    options: ScreenshotOptions,
}

impl LaunchCommand {
    pub fn try_from_args() -> Result<Self, String> {
        Self::try_from_arg_strings(std::env::args().collect())
    }

    fn try_from_arg_strings(args: Vec<String>) -> Result<Self, String> {
        reject_duplicate_flags(&args)?;
        if let Some(query) = parse_query_command(&args)? {
            return Ok(query);
        }
        parse_run_plan(args).map(Self::Run)
    }
}

fn reject_duplicate_flags(args: &[String]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for flag in args
        .iter()
        .skip(1)
        .filter(|argument| argument.starts_with("--") || argument.as_str() == "-h")
    {
        if !seen.insert(flag.as_str()) {
            return Err(format!("Duplicate CLI flag '{flag}' is not supported."));
        }
    }
    Ok(())
}

fn parse_query_command(args: &[String]) -> Result<Option<LaunchCommand>, String> {
    let values = args.get(1..).unwrap_or_default();
    let help = values
        .iter()
        .any(|value| value == "--help" || value == "-h");
    let snapshots = values
        .iter()
        .any(|value| value == "--list-camera-snapshots");
    let logs = values.iter().any(|value| {
        matches!(
            value.as_str(),
            "--print-log-dir" | "--latest-log" | "--tail-latest-log"
        )
    });
    let query_count = usize::from(help) + usize::from(snapshots) + usize::from(logs);
    if query_count == 0 {
        return Ok(None);
    }
    if query_count > 1 {
        return Err("Do not combine help, log inspection, and camera snapshot queries.".to_owned());
    }
    if help {
        if values.len() != 1 {
            return Err("Do not combine --help with run or query arguments.".to_owned());
        }
        return Ok(Some(LaunchCommand::Help));
    }
    if snapshots {
        if values != ["--list-camera-snapshots"] {
            return Err(
                "Do not combine --list-camera-snapshots with run or log arguments.".to_owned(),
            );
        }
        return Ok(Some(LaunchCommand::ListCameraSnapshots));
    }

    let mut inspection = LogInspection {
        print_directory: false,
        print_latest_path: false,
        tail_latest_lines: None,
    };
    let mut index = 0;
    while index < values.len() {
        match values[index].as_str() {
            "--print-log-dir" if !inspection.print_directory => {
                inspection.print_directory = true;
                index += 1;
            }
            "--latest-log" if !inspection.print_latest_path => {
                inspection.print_latest_path = true;
                index += 1;
            }
            "--tail-latest-log" if inspection.tail_latest_lines.is_none() => {
                let line_count = match values.get(index + 1) {
                    Some(value) if !value.starts_with("--") => {
                        value.parse::<u32>().map_err(|_| {
                            format!(
                            "Invalid --tail-latest-log '{value}'. Expected a nonnegative integer."
                        )
                        })? as usize
                    }
                    _ => 200,
                };
                inspection.tail_latest_lines = Some(line_count);
                index +=
                    usize::from(values.get(index + 1).is_some_and(|v| !v.starts_with("--"))) + 1;
            }
            argument => {
                return Err(format!(
                    "Do not combine log inspection with run arguments or duplicate query flag '{argument}'."
                ));
            }
        }
    }
    Ok(Some(LaunchCommand::InspectLogs(inspection)))
}

fn parse_run_plan(args: Vec<String>) -> Result<RunPlan, String> {
    let parse_required_string_after = |flag: &str, label: &str| -> Result<Option<String>, String> {
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
    let environment_irradiance_capture_path =
        parse_required_string_after("--environment-irradiance-capture", "an output .rfirr path")?;
    let ddgi_spatial_weight_readback_path =
        parse_required_string_after("--ddgi-spatial-weight-readback", "an output text path")?;
    let environment_irradiance_capture_target_value = parse_required_string_after(
        "--environment-irradiance-capture-target",
        "e0, e1, eN, converged, or published",
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
                        "Invalid --environment-irradiance-capture-target '{value}'. Expected e0, e1, eN, converged, or published."
                    )
                })?,
                None => DdgiCaptureTarget::default(),
            };
    let ddgi_batch_order =
        match parse_required_string_after("--ddgi-batch-order", "one of: forward, reverse")? {
            Some(value) => DdgiBatchOrder::from_cli_value(&value).ok_or_else(|| {
                format!("Invalid --ddgi-batch-order '{value}'. Expected one of: forward, reverse.")
            })?,
            None => DdgiBatchOrder::Forward,
        };
    let ddgi_debug_view = match parse_required_string_after(
            "--ddgi-debug-view",
            "one of: final, moment-visibility, exact-visibility, visibility-error, exact-irradiance, unoccluded-irradiance, equal-weight-irradiance, raw-cage-irradiance, spatial-weight-current, spatial-weight-nominal, spatial-weight-wrap, spatial-weight-nominal-wrap, spatial-weight-readback, spatial-weight-current-no-surface, spatial-weight-nominal-no-surface, irradiance-error, weight-sum, moment-support, dominant-probe, probe-state, relocation, irradiance-atlas, visibility-atlas",
        )? {
            Some(value) => DdgiDebugView::from_cli_value(&value).ok_or_else(|| {
                format!(
                    "Invalid --ddgi-debug-view '{value}'. Expected one of: final, moment-visibility, exact-visibility, visibility-error, exact-irradiance, unoccluded-irradiance, equal-weight-irradiance, raw-cage-irradiance, spatial-weight-current, spatial-weight-nominal, spatial-weight-wrap, spatial-weight-nominal-wrap, spatial-weight-readback, spatial-weight-current-no-surface, spatial-weight-nominal-no-surface, irradiance-error, weight-sum, moment-support, dominant-probe, probe-state, relocation, irradiance-atlas, visibility-atlas."
                )
            })?,
            None => DdgiDebugView::Final,
        };
    if ddgi_spatial_weight_readback_path.is_some()
        && ddgi_debug_view != DdgiDebugView::SpatialWeightReadback
    {
        return Err(
            "--ddgi-spatial-weight-readback requires --ddgi-debug-view spatial-weight-readback"
                .to_owned(),
        );
    }
    if ddgi_debug_view == DdgiDebugView::SpatialWeightReadback
        && ddgi_spatial_weight_readback_path.is_none()
    {
        return Err(
            "--ddgi-debug-view spatial-weight-readback requires --ddgi-spatial-weight-readback"
                .to_owned(),
        );
    }
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
    let screenshot = parse_screenshot_request(&args)?;
    let denoiser_bench = parse_denoiser_bench_request(&args)?;
    let foliage_shadow_bench = parse_foliage_shadow_bench_request(&args)?;
    if denoiser_bench.is_some() && foliage_shadow_bench.is_some() {
        return Err(format!(
                "Do not combine --denoiser-bench with --foliage-shadow-bench. {DENOISER_BENCH_USAGE} {FOLIAGE_SHADOW_BENCH_USAGE}"
            ));
    }
    if screenshot.is_some() && denoiser_bench.is_some() {
        return Err(format!(
            "Do not combine --screenshot with --denoiser-bench. {DENOISER_BENCH_USAGE}"
        ));
    }
    if screenshot.is_some() && foliage_shadow_bench.is_some() {
        return Err(format!(
            "Do not combine --screenshot with --foliage-shadow-bench. {FOLIAGE_SHADOW_BENCH_USAGE}"
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
    let screenshot_options = screenshot
        .as_ref()
        .map(|screenshot| screenshot.options.clone());
    let terrain_load_path =
        parse_required_string_after("--terrain-load", "a terrain snapshot path")?;
    let terrain_save_path =
        parse_required_string_after("--terrain-save", "a terrain snapshot path")?;
    let water_experience = args.iter().any(|arg| arg == "--water-experience");
    let hybrid_transparency_test_scene = args
        .iter()
        .any(|arg| arg == "--hybrid-transparency-test-scene");
    let house_scene = args.iter().any(|arg| arg == "--house-scene");
    let water_edit_soak = args.iter().any(|arg| arg == "--water-edit-soak");
    let foliage_shadow_bench_requested = foliage_shadow_bench.is_some();
    let canopy_audio_budget_diagnostic = args
        .iter()
        .any(|arg| arg == "--canopy-audio-budget-diagnostic");
    let canopy_audio_diagnostic =
        canopy_audio_budget_diagnostic || args.iter().any(|arg| arg == "--canopy-audio-diagnostic");
    if canopy_audio_diagnostic
        && (terrain_load_path.is_some()
            || water_experience
            || environment_lighting_test_scene.is_some()
            || hybrid_transparency_test_scene
            || house_scene
            || screenshot_options.is_some()
            || denoiser_bench.is_some()
            || camera_snapshot.is_some())
    {
        return Err("Do not combine --canopy-audio-diagnostic with another fixed scene, terrain load, screenshot, denoiser benchmark, or camera snapshot".to_owned());
    }
    if water_experience
        && (environment_lighting_test_scene.is_some()
            || hybrid_transparency_test_scene
            || house_scene
            || water_edit_soak)
    {
        return Err(
                "Do not combine --water-experience with terrain-stamping test scenes or --water-edit-soak"
                    .to_owned(),
            );
    }
    if terrain_load_path.is_some()
        && (environment_lighting_test_scene.is_some()
            || hybrid_transparency_test_scene
            || house_scene
            || water_edit_soak
            || water_experience)
    {
        return Err(
            "Do not combine --terrain-load with terrain-stamping test scenes or --water-edit-soak"
                .to_owned(),
        );
    }
    if house_scene
        && (environment_lighting_test_scene.is_some()
            || hybrid_transparency_test_scene
            || water_edit_soak)
    {
        return Err(
            "Do not combine --house-scene with terrain-stamping test scenes or --water-edit-soak"
                .to_owned(),
        );
    }
    if foliage_shadow_bench_requested
        && (terrain_load_path.is_some()
            || water_experience
            || environment_lighting_test_scene.is_some()
            || hybrid_transparency_test_scene
            || house_scene
            || water_edit_soak
            || camera_snapshot.is_some()
            || args.iter().any(|arg| arg == "--no-flora"))
    {
        return Err("Do not combine --foliage-shadow-bench with another fixed scene, terrain load, camera snapshot, water edit soak, or --no-flora".to_owned());
    }

    let terrain_connectivity_bench_mode = parse_required_string_after(
        "--terrain-connectivity-bench",
        "existing, correct, bounded, or manual",
    )?;
    let terrain_connectivity_bench = terrain_connectivity_bench_mode
            .map(|value| {
                TerrainConnectivityBenchMode::from_cli_value(&value).ok_or_else(|| {
                    format!(
                        "Invalid --terrain-connectivity-bench '{value}'. Expected existing, correct, bounded, or manual."
                    )
                })
            })
            .transpose()?
            .map(|mode| -> Result<_, String> { Ok(TerrainConnectivityBenchOptions {
                mode,
                available_particles: parse_optional_u32_after(
                    &args,
                    "--terrain-connectivity-bench-available-particles",
                )?
                .unwrap_or(16_384)
                .min(16_384) as usize,
                warmup_frames: parse_optional_u32_after(&args, "--terrain-connectivity-bench-warmup-frames")?
                    .unwrap_or(DEFAULT_TERRAIN_CONNECTIVITY_BENCH_WARMUP_FRAMES),
                observe_frames: parse_optional_u32_after(&args, "--terrain-connectivity-bench-observe-frames")?
                    .unwrap_or(DEFAULT_TERRAIN_CONNECTIVITY_BENCH_OBSERVE_FRAMES)
                    .max(1),
                voxel_budget: parse_optional_u32_after(&args, "--terrain-connectivity-bench-voxel-budget")?
                    .unwrap_or(16_384)
                    .max(1) as usize,
            }) })
            .transpose()?;
    if terrain_connectivity_bench.is_none()
        && args.iter().any(|arg| {
            matches!(
                arg.as_str(),
                "--terrain-connectivity-bench-available-particles"
                    | "--terrain-connectivity-bench-warmup-frames"
                    | "--terrain-connectivity-bench-observe-frames"
                    | "--terrain-connectivity-bench-voxel-budget"
            )
        })
    {
        return Err(
            "Terrain connectivity benchmark options require --terrain-connectivity-bench"
                .to_owned(),
        );
    }

    let tree_bench = args.iter().any(|argument| argument == "--tree-bench");
    let tree_bench_samples = parse_optional_u32_after(&args, "--tree-bench-samples")?;
    if tree_bench_samples.is_some() && !tree_bench {
        return Err("--tree-bench-samples requires --tree-bench".to_owned());
    }
    let authored_flora_bench = args
        .iter()
        .any(|argument| argument == "--authored-flora-bench");
    let authored_flora_bench_samples =
        parse_optional_u32_after(&args, "--authored-flora-bench-samples")?;
    if authored_flora_bench_samples.is_some() && !authored_flora_bench {
        return Err("--authored-flora-bench-samples requires --authored-flora-bench".to_owned());
    }

    let mut scenarios = Vec::new();
    if canopy_audio_diagnostic {
        scenarios.push(Scenario::CanopyAudioDiagnostic {
            constrained_budget: canopy_audio_budget_diagnostic,
        });
    }
    if water_experience {
        scenarios.push(Scenario::WaterExperience);
    }
    if water_edit_soak {
        scenarios.push(Scenario::WaterEditSoak);
    }
    if let Some(test_case) = environment_lighting_test_scene {
        scenarios.push(Scenario::EnvironmentLighting(test_case));
    }
    if hybrid_transparency_test_scene {
        scenarios.push(Scenario::HybridTransparency);
    }
    if house_scene {
        scenarios.push(Scenario::House);
    }
    if let Some(benchmark) = terrain_connectivity_bench {
        scenarios.push(Scenario::TerrainConnectivityBenchmark(benchmark));
    }
    if let Some(benchmark) = foliage_shadow_bench {
        scenarios.push(Scenario::FoliageShadowBenchmark(benchmark));
    }
    let scenario = match scenarios.as_slice() {
        [] => Scenario::Garden,
        [scenario] => scenario.clone(),
        _ => {
            return Err(
                "Choose exactly one fixed scenario; scenario flags cannot be combined.".to_owned(),
            )
        }
    };

    let camera = match (screenshot_options, denoiser_bench, camera_snapshot) {
        (Some(capture), None, Some(snapshot)) => CameraAutomation::Screenshot { snapshot, capture },
        (None, Some((snapshot, benchmark)), _) => CameraAutomation::DenoiserBenchmark {
            snapshot,
            benchmark,
        },
        (None, None, Some(snapshot)) => CameraAutomation::Snapshot(snapshot),
        (None, None, None) => CameraAutomation::None,
        _ => return Err("Choose exactly one camera automation mode.".to_owned()),
    };

    let no_shadows = args.iter().any(|a| a == "--no-shadows");
    let no_flora = args.iter().any(|a| a == "--no-flora");
    Ok(RunPlan {
        platform: PlatformPlan {
            display: DisplayPlan {
                windowed: args.iter().any(|a| a == "--windowed"),
                hidden: args.iter().any(|a| a == "--hidden"),
                present_mode,
                monitor_score,
                swapchain_images: parse_optional_u32_after(&args, "--swapchain-images")?,
            },
            render: RenderPlan {
                flags: RenderFlags {
                    enable_shadows: !no_shadows,
                    enable_leaf_shadows: !no_shadows
                        && !args.iter().any(|a| a == "--no-leaf-shadows"),
                    enable_god_rays: !args.iter().any(|a| a == "--no-god-rays"),
                    enable_lens_flare: !args.iter().any(|a| a == "--no-lens-flare"),
                    enable_tracer: !args.iter().any(|a| a == "--no-tracer"),
                    enable_flora: !no_flora,
                    enable_leaves: !no_flora,
                    enable_particles: !args.iter().any(|a| a == "--no-particles"),
                    enable_clouds: false,
                },
                perf_logging: args.iter().any(|a| a == "--perf"),
            },
            lifecycle: LifecyclePlan {
                auto_exit_delay: parse_optional_f32_after(&args, "--auto-exit")?,
                egui_texture_test: args.iter().any(|a| a == "--egui-texture-lifecycle-test"),
                resize_test: args.iter().any(|a| a == "--resize-lifecycle-test"),
            },
        },
        audio: AudioPlan {
            muted: args.iter().any(|a| a == "--mute"),
            canopy_telemetry: args.iter().any(|a| a == "--canopy-audio-telemetry"),
            output_device: parse_required_string_after(
                "--audio-output-device",
                "an output device name substring",
            )?,
        },
        world: WorldPlan {
            terrain: TerrainPersistencePlan {
                load_path: terrain_load_path,
                save_path: terrain_save_path,
            },
            water: WaterPlan {
                profile: water_profile,
                particles: parse_optional_u32_after(&args, "--water-particles")?
                    .map(|value| value as usize),
                particle_edge_len: parse_optional_f32_after(&args, "--water-particle-edge-len")?
                    .map(|value| value.max(1.0e-6)),
                grid: parse_optional_u32_after(&args, "--water-grid")?.map(|value| value.max(4)),
                substep_hz: parse_optional_f32_after(&args, "--water-substep-hz")?
                    .map(|value| value.max(1.0)),
                terrain_margin_cells: parse_optional_f32_after(
                    &args,
                    "--water-terrain-margin-cells",
                )?
                .map(|value| value.max(0.0)),
                damping: parse_optional_f32_after(&args, "--water-damping")?
                    .map(|value| value.max(0.0)),
                terrain_tangent_damping: parse_optional_f32_after(
                    &args,
                    "--water-terrain-tangent-damping",
                )?
                .map(|value| value.max(0.0)),
                stiffness: parse_optional_f32_after(&args, "--water-stiffness")?
                    .map(|value| value.max(0.0)),
                gamma: parse_optional_f32_after(&args, "--water-gamma")?
                    .map(|value| value.max(1.0e-4)),
                j_min: parse_optional_f32_after(&args, "--water-j-min")?
                    .map(|value| value.clamp(1.0e-4, 1.0)),
            },
            lighting: EnvironmentLightingPlan {
                irradiance_capture_path: environment_irradiance_capture_path,
                spatial_weight_readback_path: ddgi_spatial_weight_readback_path,
                capture_target: environment_irradiance_capture_target,
                batch_order: ddgi_batch_order,
                debug_view: ddgi_debug_view,
                terrain_hard_origin: ddgi_terrain_hard_origin,
                probe_spacing_voxels: environment_probe_spacing_voxels,
                rebuild_probe_spacing_voxels: environment_probe_rebuild_spacing_voxels,
                visualize_probes: args
                    .iter()
                    .any(|a| a == "--environment-probe-visualization"),
            },
        },
        automation: AutomationPlan {
            camera,
            benchmarks: BenchmarkPlan {
                tree_samples: tree_bench.then_some(tree_bench_samples.unwrap_or(10)),
                authored_flora_samples: authored_flora_bench
                    .then_some(authored_flora_bench_samples.unwrap_or(25)),
            },
        },
        scenario,
    })
}

fn parse_denoiser_bench_request(
    args: &[String],
) -> Result<Option<(String, CameraDenoiserOptions)>, String> {
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
    let warmup_frames = parse_optional_u32_after(args, "--denoiser-bench-warmup-frames")?
        .unwrap_or(DEFAULT_DENOISER_BENCH_WARMUP_FRAMES);
    let capture_frames = parse_optional_u32_after(args, "--denoiser-bench-frames")?
        .unwrap_or(DEFAULT_DENOISER_BENCH_CAPTURE_FRAMES);
    if capture_frames < 2 {
        return Err("--denoiser-bench-frames must be at least 2".to_owned());
    }

    Ok(Some((
        preset_name,
        CameraDenoiserOptions {
            capture: DenoiserCaptureOptions {
                report_path,
                warmup_frames,
                capture_frames,
            },
            camera_motion: if args
                .iter()
                .any(|arg| arg == "--denoiser-bench-camera-motion")
            {
                CameraMotion::Scripted
            } else {
                CameraMotion::Fixed
            },
        },
    )))
}

fn parse_foliage_shadow_bench_request(
    args: &[String],
) -> Result<Option<FoliageDenoiserOptions>, String> {
    let Some(index) = args.iter().position(|arg| arg == "--foliage-shadow-bench") else {
        if args.iter().any(|arg| {
            arg == "--foliage-shadow-bench-warmup-frames" || arg == "--foliage-shadow-bench-frames"
        }) {
            return Err(format!(
                "Foliage shadow benchmark frame options require --foliage-shadow-bench. {FOLIAGE_SHADOW_BENCH_USAGE}"
            ));
        }
        return Ok(None);
    };

    let report_path = required_denoiser_bench_arg(args, index + 1, "report path")?;
    let warmup_frames = parse_optional_u32_after(args, "--foliage-shadow-bench-warmup-frames")?
        .unwrap_or(DEFAULT_DENOISER_BENCH_WARMUP_FRAMES);
    let capture_frames = parse_optional_u32_after(args, "--foliage-shadow-bench-frames")?
        .unwrap_or(DEFAULT_DENOISER_BENCH_CAPTURE_FRAMES);
    if capture_frames < 2 {
        return Err("--foliage-shadow-bench-frames must be at least 2".to_owned());
    }

    Ok(Some(FoliageDenoiserOptions {
        capture: DenoiserCaptureOptions {
            report_path,
            warmup_frames,
            capture_frames,
        },
    }))
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
                    "Invalid --environment-lighting-test-scene '{value}'. Expected one of: sealed, patt-seam, portal, walls, donor, dogleg, radiance-changes, point-light-changes, voxel-emissive-changes, raster-emitter-changes, multi-source-stress, local-light-scaling, density-changes, terrain-edits, terrain-edits-inflight, terrain-edits-inflight-capture, terrain-edits-closed."
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

fn parse_optional_u32_after(args: &[String], flag: &str) -> Result<Option<u32>, String> {
    let Some(index) = args.iter().position(|argument| argument == flag) else {
        return Ok(None);
    };
    let Some(value) = args.get(index + 1).filter(|value| !value.starts_with("--")) else {
        return Err(format!(
            "Missing value for {flag}. Expected a nonnegative integer."
        ));
    };
    value
        .parse::<u32>()
        .map(Some)
        .map_err(|_| format!("Invalid {flag} '{value}'. Expected a nonnegative integer."))
}

fn parse_optional_f32_after(args: &[String], flag: &str) -> Result<Option<f32>, String> {
    let Some(index) = args.iter().position(|argument| argument == flag) else {
        return Ok(None);
    };
    let Some(value) = args.get(index + 1).filter(|value| !value.starts_with("--")) else {
        return Err(format!(
            "Missing value for {flag}. Expected a finite number."
        ));
    };
    let parsed = value
        .parse::<f32>()
        .map_err(|_| format!("Invalid {flag} '{value}'. Expected a finite number."))?;
    if !parsed.is_finite() {
        return Err(format!(
            "Invalid {flag} '{value}'. Expected a finite number."
        ));
    }
    Ok(Some(parsed))
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
        options: ScreenshotOptions { path, delay },
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
  --canopy-audio-telemetry    Log opt-in per-tree and per-canopy-sample acoustic telemetry at 10 Hz
  --canopy-audio-diagnostic   Run the fixed tree, wind, forward/hold/reverse listener trajectory and enable canopy telemetry
  --canopy-audio-budget-diagnostic
                              Run the same trajectory with five fixed trees and a two-extent acoustic budget
  --audio-output-device <text>
                              Select output device by case-insensitive substring/alias match
  --no-shadows                Disable shadow rendering passes
  --no-leaf-shadows           Disable leaf-opacity shadows while retaining terrain/VSM shadows
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
  --foliage-shadow-bench <report.toml>
                              Run the fixed-tree, fixed-camera receiver stability benchmark
  --foliage-shadow-bench-warmup-frames <N>
                              Frames discarded before foliage-shadow capture (default: 90)
  --foliage-shadow-bench-frames <N>
                              Foliage-shadow frames captured, at least 2 (default: 64)
  --camera-snapshot <name>    Apply a saved camera snapshot at startup (do not combine with --screenshot)
  --list-camera-snapshots     Print available camera snapshot names and exit
  --auto-exit <sec>           Exit automatically after rendering starts
  --egui-texture-lifecycle-test
                              Exercise egui texture generations through full/partial/free updates
  --resize-lifecycle-test     Exercise coalesced programmatic resizes through the render path
  --perf                      Enable per-frame performance logging
  --water-experience          Launch the stable basin, water fill, lighting, and camera experience
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
                              radiance-changes, point-light-changes, voxel-emissive-changes,
                              raster-emitter-changes, multi-source-stress, local-light-scaling,
                              density-changes, terrain-edits,
                              terrain-edits-inflight, terrain-edits-inflight-capture, or
                              terrain-edits-closed
  --environment-irradiance-capture <path>
                              Save DDGI metadata, pre-albedo irradiance/hit mask, world hit, and exact sun visibility
  --ddgi-spatial-weight-readback <path>
                              Save the fixed saved-terrain eight-probe contribution readback (requires spatial-weight-readback)
  --environment-irradiance-capture-target <target>
                              Capture e0, e1, a specified eN, converged, or published (default: e0)
  --ddgi-batch-order <order>  Traverse DDGI probe batches in forward or reverse order (default: forward)
  --ddgi-debug-view <view>    Select final, moment/exact visibility, error, weight/support, probe, relocation,
                              spatial-weight, readback, or atlas DDGI diagnostics (default: final)
  --ddgi-terrain-hard-origin <mode>
                              Select surface-quarter, center-fixed, or surface-fixed exact visibility origin
                              for terrain receiver experiments (default: {})
  --hybrid-transparency-test-scene
                              Build the deterministic raster/terrain transparency regression scene
  --house-scene               Build the terrain-integrated Hobbit hill house
  --environment-probe-spacing-voxels <N>
                              Set environment probe spacing: 64, 32, 16, or 8 (default: 32)
  --environment-probe-rebuild-spacing-voxels <N>
                              Rebuild probes once after rendering starts, for runtime validation
  --environment-probe-visualization
                              Visualize the environment probe grid (debug; default: off)
  --tree-bench                Run tree replacement benchmark and exit
  --tree-bench-samples <N>    Tree benchmark samples (default: 10)
  --authored-flora-bench      Run authored special-flora paint benchmark and exit
  --authored-flora-bench-samples <N>
                              Authored flora benchmark paint samples (default: 25)
  --terrain-connectivity-bench <existing|correct|bounded|manual>
                              Run the 437,205-voxel release benchmark or manual scene
  --terrain-connectivity-bench-available-particles <N>
                              Set actual free particle slots at release (default/max: 16384)
  --terrain-connectivity-bench-warmup-frames <N>
                              Settled frames before release (default: 600)
  --terrain-connectivity-bench-observe-frames <N>
                              Frames retained after release (default: 180)
  --terrain-connectivity-bench-voxel-budget <N>
                              Per-frame topology voxel budget in bounded mode (default: 16384)
  --print-log-dir             Print the per-worktree run log directory and exit
  --latest-log                Print the latest run log path and exit
  --tail-latest-log [N]       Print the last N lines of the latest run log and exit (default: 200)
  -h, --help                  Show this help and exit

Examples:
  re-flora --windowed
  re-flora --hidden --mute --auto-exit 20 --perf
  re-flora --audio-output-device KA3
  re-flora --hidden --mute --screenshot player-default screenshots/check.png --screenshot-delay 2 --auto-exit 4
  re-flora --present-mode fifo
  re-flora --monitor-score lowest
  re-flora --swapchain-images 2
  re-flora --no-shadows
  re-flora --hidden --mute --screenshot tree-closeup out.png --screenshot-delay 2 --auto-exit 4
  re-flora --hidden --mute --windowed --denoiser-bench player-default target/denoiser.toml
  re-flora --hidden --mute --windowed --foliage-shadow-bench target/foliage-shadow.toml
  re-flora --list-camera-snapshots
  re-flora --auto-exit 10 --perf
  re-flora --hidden --mute --auto-exit 4 --perf --water-profile performance
  re-flora --water-experience
  re-flora --hidden --mute --auto-exit 4 --perf --water-particles 35000 --water-particle-edge-len 0.05
  re-flora --hidden --mute --auto-exit 4 --perf --water-profile performance --water-damping 1.5 --water-terrain-margin-cells 0.0
  re-flora --hidden --mute --auto-exit 14 --perf --water-profile performance --water-edit-soak
  re-flora --hidden --mute --environment-lighting-test-scene sealed --environment-irradiance-capture target/sealed.rfirr --auto-exit 8
  re-flora --hidden --mute --windowed --hybrid-transparency-test-scene --screenshot player-default target/hybrid-transparency-test.png --screenshot-delay 2 --auto-exit 6
  re-flora --house-scene --camera-snapshot house-overlook
  re-flora --latest-log
  re-flora --tail-latest-log 120
  re-flora --windowed --tree-bench --tree-bench-samples 10"#,
        DdgiTerrainHardOrigin::default().label()
    );
}

#[derive(Clone, Debug)]
pub struct RenderFlags {
    pub enable_shadows: bool,
    pub enable_leaf_shadows: bool,
    pub enable_god_rays: bool,
    pub enable_lens_flare: bool,
    pub enable_tracer: bool,
    pub enable_flora: bool,
    pub enable_leaves: bool,
    pub enable_particles: bool,
    pub enable_clouds: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn try_launch(args: &[&str]) -> Result<LaunchCommand, String> {
        LaunchCommand::try_from_arg_strings(
            args.iter().map(|argument| (*argument).to_owned()).collect(),
        )
    }

    fn try_parse_owned(args: Vec<String>) -> Result<RunPlan, String> {
        match LaunchCommand::try_from_arg_strings(args)? {
            LaunchCommand::Run(plan) => Ok(plan),
            _ => Err("expected run arguments".to_owned()),
        }
    }

    fn parse(args: &[&str]) -> RunPlan {
        try_parse_owned(args.iter().map(|arg| (*arg).to_owned()).collect())
            .unwrap_or_else(|err| panic!("{err}"))
    }

    #[test]
    fn defaults_match_runtime_expectations() {
        let options = parse(&["re-flora"]);

        assert!(!options.platform.display.windowed);
        assert!(!options.platform.display.hidden);
        assert!(!options.audio.muted);
        assert!(!options.audio.canopy_telemetry);
        assert_eq!(options.scenario, Scenario::Garden);
        assert!(options.audio.output_device.is_none());
        assert!(options.platform.render.flags.enable_leaf_shadows);
        assert!(!options.platform.render.perf_logging);
        assert!(options.platform.display.present_mode.is_none());
        assert!(matches!(
            options.platform.display.monitor_score,
            MonitorScorePreference::Highest
        ));
        assert_eq!(options.automation.camera, CameraAutomation::None);
        assert!(options.world.terrain.load_path.is_none());
        assert!(options.world.terrain.save_path.is_none());
        assert!(!options.platform.lifecycle.egui_texture_test);
        assert!(!options.platform.lifecycle.resize_test);
        assert!(options.world.lighting.irradiance_capture_path.is_none());
        assert!(options
            .world
            .lighting
            .spatial_weight_readback_path
            .is_none());
        assert_eq!(
            options.world.lighting.capture_target,
            DdgiCaptureTarget::Epoch(0)
        );
        assert_eq!(options.world.lighting.batch_order, DdgiBatchOrder::Forward);
        assert_eq!(options.world.lighting.debug_view, DdgiDebugView::Final);
        assert_eq!(
            options.world.lighting.terrain_hard_origin,
            DdgiTerrainHardOrigin::SurfaceFixedWorld
        );
        assert_eq!(
            options.world.lighting.probe_spacing_voxels,
            DEFAULT_DDGI_SPACING_VOXELS
        );
        assert!(options
            .world
            .lighting
            .rebuild_probe_spacing_voxels
            .is_none());
        assert!(!options.world.lighting.visualize_probes);
        assert_eq!(options.automation.benchmarks, BenchmarkPlan::default());
    }

    #[test]
    fn run_plan_has_one_primary_owner_for_each_launch_domain() {
        let plan = parse(&["re-flora"]);

        assert!(!plan.platform.display.hidden);
        assert!(plan.world.terrain.load_path.is_none());
        assert_eq!(plan.automation.camera, CameraAutomation::None);
    }

    #[test]
    fn parses_terrain_connectivity_bench_options() {
        let options = parse(&[
            "re-flora",
            "--terrain-connectivity-bench",
            "correct",
            "--terrain-connectivity-bench-available-particles",
            "8192",
            "--terrain-connectivity-bench-warmup-frames",
            "120",
            "--terrain-connectivity-bench-observe-frames",
            "45",
        ]);

        assert_eq!(
            options.scenario.terrain_connectivity_benchmark(),
            Some(TerrainConnectivityBenchOptions {
                mode: TerrainConnectivityBenchMode::Correct,
                available_particles: 8192,
                warmup_frames: 120,
                observe_frames: 45,
                voxel_budget: 16_384,
            })
        );
    }

    #[test]
    fn parses_manual_terrain_connectivity_scene() {
        let options = parse(&[
            "re-flora",
            "--terrain-connectivity-bench",
            "manual",
            "--terrain-connectivity-bench-warmup-frames",
            "0",
        ]);

        assert_eq!(
            options.scenario.terrain_connectivity_benchmark(),
            Some(TerrainConnectivityBenchOptions {
                mode: TerrainConnectivityBenchMode::Manual,
                available_particles: 16_384,
                warmup_frames: 0,
                observe_frames: DEFAULT_TERRAIN_CONNECTIVITY_BENCH_OBSERVE_FRAMES,
                voxel_budget: 16_384,
            })
        );
    }

    #[test]
    fn parses_authored_flora_bench_options() {
        let options = parse(&[
            "re-flora",
            "--authored-flora-bench",
            "--authored-flora-bench-samples",
            "7",
        ]);

        assert_eq!(
            options.automation.benchmarks.authored_flora_samples,
            Some(7)
        );
    }

    #[test]
    fn parses_lifecycle_acceptance_options() {
        let options = parse(&[
            "re-flora",
            "--egui-texture-lifecycle-test",
            "--resize-lifecycle-test",
        ]);

        assert!(options.platform.lifecycle.egui_texture_test);
        assert!(options.platform.lifecycle.resize_test);
    }

    #[test]
    fn parses_fixed_canopy_audio_diagnostic() {
        let options = parse(&[
            "re-flora",
            "--hidden",
            "--mute",
            "--canopy-audio-diagnostic",
        ]);

        assert_eq!(options.scenario.canopy_audio_diagnostic(), Some(false));
        assert!(!options.audio.canopy_telemetry);
    }

    #[test]
    fn parses_fixed_canopy_audio_budget_diagnostic() {
        let options = parse(&[
            "re-flora",
            "--hidden",
            "--mute",
            "--canopy-audio-budget-diagnostic",
        ]);

        assert_eq!(options.scenario.canopy_audio_diagnostic(), Some(true));
    }

    #[test]
    fn fixed_canopy_audio_diagnostic_rejects_competing_scene() {
        let error = try_parse_owned(
            ["re-flora", "--canopy-audio-diagnostic", "--house-scene"]
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect(),
        )
        .unwrap_err();

        assert!(error.contains("Do not combine --canopy-audio-diagnostic"));
    }

    #[test]
    fn parses_environment_lighting_test_scene() {
        let options = parse(&["re-flora", "--environment-lighting-test-scene"]);

        assert_eq!(
            options.scenario.environment_lighting(),
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
                "point-light-changes",
                EnvironmentLightingTestCase::PointLightChanges,
            ),
            (
                "voxel-emissive-changes",
                EnvironmentLightingTestCase::VoxelEmissiveChanges,
            ),
            (
                "raster-emitter-changes",
                EnvironmentLightingTestCase::RasterEmitterChanges,
            ),
            (
                "multi-source-stress",
                EnvironmentLightingTestCase::MultiSourceStress,
            ),
            (
                "local-light-scaling",
                EnvironmentLightingTestCase::LocalLightScaling,
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
            assert_eq!(options.scenario.environment_lighting(), Some(expected));
        }
    }

    #[test]
    fn rejects_unknown_environment_lighting_test_scene() {
        let result = try_parse_owned(
            ["re-flora", "--environment-lighting-test-scene", "dynamic"]
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect(),
        );

        assert!(result.unwrap_err().contains(
            "sealed, patt-seam, portal, walls, donor, dogleg, radiance-changes, point-light-changes, voxel-emissive-changes, raster-emitter-changes, multi-source-stress, local-light-scaling, density-changes, terrain-edits, terrain-edits-inflight, terrain-edits-inflight-capture, terrain-edits-closed"
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
            options.world.lighting.irradiance_capture_path.as_deref(),
            Some("target/sealed.rfirr")
        );
        assert_eq!(
            options.world.lighting.capture_target,
            DdgiCaptureTarget::Epoch(0)
        );
    }

    #[test]
    fn parses_environment_irradiance_capture_target() {
        let options = parse(&[
            "re-flora",
            "--environment-irradiance-capture",
            "target/sealed-e4.rfirr",
            "--environment-irradiance-capture-target",
            "e4",
        ]);

        assert_eq!(
            options.world.lighting.capture_target,
            DdgiCaptureTarget::Epoch(4)
        );
    }

    #[test]
    fn parses_ddgi_batch_order() {
        let options = parse(&["re-flora", "--ddgi-batch-order", "reverse"]);
        assert_eq!(options.world.lighting.batch_order, DdgiBatchOrder::Reverse);

        let result = try_parse_owned(
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
        let result = try_parse_owned(
            ["re-flora", "--environment-irradiance-capture-target", "e2"]
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
        assert_eq!(
            options.world.lighting.debug_view,
            DdgiDebugView::ExactVisibility
        );

        let options = parse(&["re-flora", "--ddgi-debug-view", "unoccluded-irradiance"]);
        assert_eq!(
            options.world.lighting.debug_view,
            DdgiDebugView::UnoccludedIrradiance
        );

        let options = parse(&["re-flora", "--ddgi-debug-view", "equal-weight-irradiance"]);
        assert_eq!(
            options.world.lighting.debug_view,
            DdgiDebugView::EqualWeightIrradiance
        );

        let options = parse(&["re-flora", "--ddgi-debug-view", "raw-cage-irradiance"]);
        assert_eq!(
            options.world.lighting.debug_view,
            DdgiDebugView::RawCageIrradiance
        );

        let options = parse(&["re-flora", "--ddgi-debug-view", "moment-support"]);
        assert_eq!(
            options.world.lighting.debug_view,
            DdgiDebugView::MomentSupport
        );

        for (value, expected) in [
            (
                "spatial-weight-current",
                DdgiDebugView::SpatialWeightCurrent,
            ),
            (
                "spatial-weight-nominal",
                DdgiDebugView::SpatialWeightNominal,
            ),
            ("spatial-weight-wrap", DdgiDebugView::SpatialWeightWrap),
            (
                "spatial-weight-nominal-wrap",
                DdgiDebugView::SpatialWeightNominalWrap,
            ),
            (
                "spatial-weight-readback",
                DdgiDebugView::SpatialWeightReadback,
            ),
            (
                "spatial-weight-current-no-surface",
                DdgiDebugView::SpatialWeightCurrentNoSurface,
            ),
            (
                "spatial-weight-nominal-no-surface",
                DdgiDebugView::SpatialWeightNominalNoSurface,
            ),
        ] {
            let options = if expected == DdgiDebugView::SpatialWeightReadback {
                parse(&[
                    "re-flora",
                    "--ddgi-debug-view",
                    value,
                    "--ddgi-spatial-weight-readback",
                    "target/readback.txt",
                ])
            } else {
                parse(&["re-flora", "--ddgi-debug-view", value])
            };
            assert_eq!(options.world.lighting.debug_view, expected);
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
            assert_eq!(options.world.lighting.terrain_hard_origin, expected);
        }
    }

    #[test]
    fn defaults_ddgi_terrain_hard_origin_to_surface_fixed() {
        let options = parse(&["re-flora"]);
        assert_eq!(
            options.world.lighting.terrain_hard_origin,
            DdgiTerrainHardOrigin::SurfaceFixedWorld
        );
        assert_eq!(DdgiTerrainHardOrigin::default().label(), "surface-fixed");
    }

    #[test]
    fn rejects_invalid_ddgi_terrain_hard_origin() {
        let result = try_parse_owned(
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
    fn parses_hybrid_transparency_test_scene() {
        let options = parse(&["re-flora", "--hybrid-transparency-test-scene"]);

        assert_eq!(options.scenario, Scenario::HybridTransparency);
    }

    #[test]
    fn parses_house_scene_and_rejects_snapshot_input() {
        let options = parse(&["re-flora", "--house-scene"]);
        assert_eq!(options.scenario, Scenario::House);

        let incompatible = try_parse_owned(
            [
                "re-flora",
                "--terrain-load",
                "target/input.rflterrain",
                "--house-scene",
            ]
            .iter()
            .map(|arg| (*arg).to_owned())
            .collect(),
        )
        .unwrap_err();
        assert!(incompatible.contains("Do not combine --terrain-load"));
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
            options.world.terrain.load_path.as_deref(),
            Some("target/input.rflterrain")
        );
        assert_eq!(
            options.world.terrain.save_path.as_deref(),
            Some("target/output.rflterrain")
        );
    }

    #[test]
    fn terrain_load_requires_a_path_and_rejects_stamping_scenes() {
        let missing = try_parse_owned(
            ["re-flora", "--terrain-load"]
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect(),
        )
        .unwrap_err();
        assert!(missing.contains("Missing value for --terrain-load"));

        let incompatible = try_parse_owned(
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

        assert_eq!(options.world.lighting.probe_spacing_voxels, 16);
        assert_eq!(
            options.world.lighting.rebuild_probe_spacing_voxels,
            Some(32)
        );
        assert!(options.world.lighting.visualize_probes);
    }

    #[test]
    fn rejects_unsupported_environment_probe_spacing() {
        let result = try_parse_owned(
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
        let result = try_parse_owned(
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
            "--canopy-audio-telemetry",
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

        assert!(options.platform.display.hidden);
        assert!(options.audio.muted);
        assert!(options.audio.canopy_telemetry);
        assert_eq!(options.audio.output_device.as_deref(), Some("KA3"));
        assert!(options.platform.render.perf_logging);
        assert_eq!(options.platform.lifecycle.auto_exit_delay, Some(4.0));
        assert!(matches!(
            options.world.water.profile,
            Some(WaterProfilePreference::Performance)
        ));
        assert_eq!(options.world.water.particles, Some(35000));
        assert_eq!(options.world.water.particle_edge_len, Some(0.05));
        assert_eq!(options.world.water.grid, Some(128));
        assert_eq!(options.world.water.substep_hz, Some(60.0));
        assert_eq!(options.world.water.terrain_margin_cells, Some(0.0));
        assert_eq!(options.world.water.damping, Some(1.5));
        assert_eq!(options.world.water.terrain_tangent_damping, Some(2.0));
        assert_eq!(options.world.water.stiffness, Some(12.0));
        assert_eq!(options.world.water.gamma, Some(4.0));
        assert_eq!(options.world.water.j_min, Some(0.25));
        assert_eq!(options.scenario, Scenario::WaterEditSoak);
    }

    #[test]
    fn parses_stable_water_experience_entry() {
        let options = parse(&[
            "re-flora",
            "--water-experience",
            "--hidden",
            "--mute",
            "--auto-exit",
            "8",
        ]);

        assert_eq!(options.scenario, Scenario::WaterExperience);
        assert!(options.platform.display.hidden);
        assert!(options.audio.muted);
        assert_eq!(options.platform.lifecycle.auto_exit_delay, Some(8.0));
    }

    #[test]
    fn water_experience_rejects_competing_terrain_scenes() {
        let error = try_parse_owned(
            [
                "re-flora",
                "--water-experience",
                "--hybrid-transparency-test-scene",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        )
        .unwrap_err();

        assert!(error.contains("Do not combine --water-experience"));
    }

    #[test]
    fn water_experience_rejects_loaded_terrain() {
        let error = try_parse_owned(
            [
                "re-flora",
                "--water-experience",
                "--terrain-load",
                "saves/custom.rflterrain",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        )
        .unwrap_err();

        assert!(error.contains("Do not combine --terrain-load"));
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

        assert_eq!(options.world.water.particle_edge_len, Some(1.0e-6));
        assert_eq!(options.world.water.grid, Some(4));
        assert_eq!(options.world.water.substep_hz, Some(1.0));
        assert_eq!(options.world.water.terrain_margin_cells, Some(0.0));
        assert_eq!(options.world.water.damping, Some(0.0));
        assert_eq!(options.world.water.terrain_tangent_damping, Some(0.0));
        assert_eq!(options.world.water.gamma, Some(1.0e-4));
        assert_eq!(options.world.water.j_min, Some(1.0));
    }

    #[test]
    fn parses_log_query_options() {
        let LaunchCommand::InspectLogs(inspection) = try_launch(&[
            "re-flora",
            "--print-log-dir",
            "--latest-log",
            "--tail-latest-log",
            "120",
        ])
        .unwrap() else {
            panic!("log flags must produce a query command");
        };

        assert!(inspection.print_directory);
        assert!(inspection.print_latest_path);
        assert_eq!(inspection.tail_latest_lines, Some(120));
    }

    #[test]
    fn parses_camera_snapshot_options() {
        let options = parse(&["re-flora", "--camera-snapshot", "tree-closeup"]);

        assert_eq!(
            options.automation.camera.snapshot_name(),
            Some("tree-closeup")
        );
        assert!(options.automation.camera.screenshot().is_none());
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

        assert!(options.platform.display.hidden);
        assert_eq!(
            options.automation.camera.snapshot_name(),
            Some("tree-closeup")
        );
        assert_eq!(
            options.automation.camera.screenshot(),
            Some(&ScreenshotOptions {
                path: "out.png".to_owned(),
                delay: 2.5,
            })
        );
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

        assert_eq!(
            options.automation.camera.snapshot_name(),
            Some("player-default")
        );
        let benchmark = options.automation.camera.denoiser_benchmark().unwrap();
        assert_eq!(benchmark.capture.report_path, "target/report.toml");
        assert_eq!(benchmark.capture.warmup_frames, 12);
        assert_eq!(benchmark.capture.capture_frames, 8);
        assert_eq!(benchmark.camera_motion, CameraMotion::Scripted);
    }

    #[test]
    fn parses_foliage_shadow_benchmark_without_camera_snapshot() {
        let options = parse(&[
            "re-flora",
            "--hidden",
            "--foliage-shadow-bench",
            "target/foliage-shadow.toml",
            "--foliage-shadow-bench-warmup-frames",
            "12",
            "--foliage-shadow-bench-frames",
            "8",
        ]);

        assert!(options.automation.camera.snapshot_name().is_none());
        let benchmark = options.scenario.foliage_shadow_benchmark().unwrap();
        assert_eq!(benchmark.capture.report_path, "target/foliage-shadow.toml");
        assert_eq!(benchmark.capture.warmup_frames, 12);
        assert_eq!(benchmark.capture.capture_frames, 8);
    }

    #[test]
    fn leaf_shadow_control_preserves_other_shadow_passes() {
        let options = parse(&["re-flora", "--no-leaf-shadows"]);
        let flags = &options.platform.render.flags;
        assert!(flags.enable_shadows);
        assert!(!flags.enable_leaf_shadows);
        assert!(flags.enable_leaves);
    }

    #[test]
    fn foliage_shadow_benchmark_rejects_no_flora() {
        let panic = std::panic::catch_unwind(|| {
            parse(&[
                "re-flora",
                "--foliage-shadow-bench",
                "target/foliage-shadow.toml",
                "--no-flora",
            ])
        })
        .expect_err("foliage benchmark without flora should panic");
        assert!(panic_message(panic).contains("--no-flora"));
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
        let LaunchCommand::InspectLogs(inspection) =
            try_launch(&["re-flora", "--tail-latest-log"]).unwrap()
        else {
            panic!("tail flag must produce a query command");
        };
        assert_eq!(inspection.tail_latest_lines, Some(200));
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

    #[test]
    fn launch_command_keeps_queries_disjoint_from_run_state() {
        assert!(matches!(
            try_launch(&["re-flora", "--help"]).unwrap(),
            LaunchCommand::Help
        ));
        assert!(matches!(
            try_launch(&["re-flora", "--list-camera-snapshots"]).unwrap(),
            LaunchCommand::ListCameraSnapshots
        ));
        let LaunchCommand::InspectLogs(inspection) = try_launch(&[
            "re-flora",
            "--print-log-dir",
            "--latest-log",
            "--tail-latest-log",
            "120",
        ])
        .unwrap() else {
            panic!("log flags must produce an InspectLogs query");
        };
        assert!(inspection.print_directory);
        assert!(inspection.print_latest_path);
        assert_eq!(inspection.tail_latest_lines, Some(120));

        for arguments in [
            ["re-flora", "--help", "--hidden"],
            ["re-flora", "--list-camera-snapshots", "--camera-snapshot"],
            ["re-flora", "--latest-log", "--hidden"],
        ] {
            assert!(try_launch(&arguments).is_err(), "accepted {arguments:?}");
        }
    }

    #[test]
    fn run_plan_uses_closed_scenario_and_unique_camera_automation() {
        let LaunchCommand::Run(plan) = try_launch(&[
            "re-flora",
            "--house-scene",
            "--camera-snapshot",
            "house-overlook",
        ])
        .unwrap() else {
            panic!("run arguments must produce a Run plan");
        };
        assert!(matches!(plan.scenario, Scenario::House));
        assert!(matches!(
            plan.automation.camera,
            CameraAutomation::Snapshot(ref name) if name == "house-overlook"
        ));

        let competing = try_launch(&[
            "re-flora",
            "--environment-lighting-test-scene",
            "portal",
            "--hybrid-transparency-test-scene",
        ])
        .unwrap_err();
        assert!(competing.contains("scenario"), "{competing}");
    }

    #[test]
    fn every_numeric_argument_rejects_invalid_text_instead_of_defaulting() {
        let cases: &[&[&str]] = &[
            &["re-flora", "--swapchain-images", "invalid"],
            &["re-flora", "--auto-exit", "invalid"],
            &["re-flora", "--water-particles", "invalid"],
            &["re-flora", "--water-particle-edge-len", "invalid"],
            &["re-flora", "--water-grid", "invalid"],
            &["re-flora", "--water-substep-hz", "invalid"],
            &["re-flora", "--water-terrain-margin-cells", "invalid"],
            &["re-flora", "--water-damping", "invalid"],
            &["re-flora", "--water-terrain-tangent-damping", "invalid"],
            &["re-flora", "--water-stiffness", "invalid"],
            &["re-flora", "--water-gamma", "invalid"],
            &["re-flora", "--water-j-min", "invalid"],
            &[
                "re-flora",
                "--denoiser-bench",
                "player-default",
                "report.toml",
                "--denoiser-bench-warmup-frames",
                "invalid",
            ],
            &[
                "re-flora",
                "--denoiser-bench",
                "player-default",
                "report.toml",
                "--denoiser-bench-frames",
                "invalid",
            ],
            &[
                "re-flora",
                "--foliage-shadow-bench",
                "report.toml",
                "--foliage-shadow-bench-warmup-frames",
                "invalid",
            ],
            &[
                "re-flora",
                "--foliage-shadow-bench",
                "report.toml",
                "--foliage-shadow-bench-frames",
                "invalid",
            ],
            &[
                "re-flora",
                "--tree-bench",
                "--tree-bench-samples",
                "invalid",
            ],
            &[
                "re-flora",
                "--authored-flora-bench",
                "--authored-flora-bench-samples",
                "invalid",
            ],
            &[
                "re-flora",
                "--terrain-connectivity-bench",
                "bounded",
                "--terrain-connectivity-bench-available-particles",
                "invalid",
            ],
            &[
                "re-flora",
                "--terrain-connectivity-bench",
                "bounded",
                "--terrain-connectivity-bench-warmup-frames",
                "invalid",
            ],
            &[
                "re-flora",
                "--terrain-connectivity-bench",
                "bounded",
                "--terrain-connectivity-bench-observe-frames",
                "invalid",
            ],
            &[
                "re-flora",
                "--terrain-connectivity-bench",
                "bounded",
                "--terrain-connectivity-bench-voxel-budget",
                "invalid",
            ],
            &["re-flora", "--tail-latest-log", "invalid"],
        ];

        for arguments in cases {
            let error = try_launch(arguments).unwrap_err();
            assert!(error.contains("Invalid"), "{arguments:?}: {error}");
        }
    }

    #[test]
    fn benchmark_sample_counts_require_their_benchmark() {
        for arguments in [
            ["re-flora", "--tree-bench-samples", "10"],
            ["re-flora", "--authored-flora-bench-samples", "25"],
        ] {
            let error = try_launch(&arguments).unwrap_err();
            assert!(error.contains("requires"), "{arguments:?}: {error}");
        }
    }

    #[test]
    fn duplicate_flags_fail_closed_even_when_the_first_value_is_valid() {
        for arguments in [
            vec!["re-flora", "--water-grid", "128", "--water-grid", "invalid"],
            vec!["re-flora", "--water-grid", "128", "--water-grid"],
            vec!["re-flora", "--hidden", "--hidden"],
        ] {
            let error = try_launch(&arguments).unwrap_err();
            assert!(error.contains("Duplicate"), "{arguments:?}: {error}");
        }
    }

    #[test]
    fn every_pair_of_fixed_scenarios_is_rejected() {
        let fixed_scenarios: &[(&str, &[&str])] = &[
            ("canopy", &["--canopy-audio-diagnostic"]),
            ("water", &["--water-experience"]),
            ("water-edit", &["--water-edit-soak"]),
            ("lighting", &["--environment-lighting-test-scene"]),
            ("hybrid", &["--hybrid-transparency-test-scene"]),
            ("house", &["--house-scene"]),
            ("connectivity", &["--terrain-connectivity-bench", "bounded"]),
            ("foliage-shadow", &["--foliage-shadow-bench", "report.toml"]),
        ];

        for (left_index, (left_name, left_args)) in fixed_scenarios.iter().enumerate() {
            for (right_name, right_args) in &fixed_scenarios[left_index + 1..] {
                let arguments = std::iter::once("re-flora")
                    .chain(left_args.iter().copied())
                    .chain(right_args.iter().copied())
                    .collect::<Vec<_>>();
                let error = try_launch(&arguments).unwrap_err();
                assert!(
                    error.contains("scenario") || error.contains("Do not combine"),
                    "accepted/unclear {left_name}+{right_name}: {error}"
                );
            }
        }
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
