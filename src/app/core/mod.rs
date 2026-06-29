#[allow(unused)]
use crate::util::Timer;

mod authored_flora_bench;
mod boot;
mod camera_snapshot_ui;
mod frame_timing;
mod input;
mod lifecycle;
mod loading;
mod moisture;
mod particles;
mod placeables;
mod player_tools;
mod screenshot;
mod terrain_rebuild;
mod tree_bench;
mod ui_style;
mod vegetation;
mod water;

use self::authored_flora_bench::AuthoredFloraBench;
use self::camera_snapshot_ui::draw_camera_snapshots_ui;
use self::frame_timing::{
    draw_frame_timing_panel, FrameCpuScope, FrameCpuTimings, FrameTimingSnapshot,
};
use self::loading::{LoadingPhase, LoadingState};
use self::moisture::TerrainMoistureSystem;
use self::particles::TreeLeafEmitter;
use self::placeables::{SprinklerEmitter, SprinklerRecord};
use self::player_tools::PlayerToolState;
use self::terrain_rebuild::{ChunkRebuildRequest, TerrainChunkRebuildInFlight};
use self::tree_bench::{TreeBench, TreeBenchMode};
use self::vegetation::{TreeRecord, TreeVariationConfig};
use crate::app::camera_snapshots::CameraSnapshotLibrary;
use crate::app::cpu_solid_voxels::CpuSolidVoxelStore;
use crate::app::environment;
use crate::app::gui_config_loader::GuiConfigLoader;
use crate::app::gui_config_model::GuiConfigFile;
use crate::app::world_edits::{BuildEdit, VoxelEdit, WorldBuildBackend, WorldEditPlan};
use crate::app::world_ops;
use crate::app::{GuiAdjustables, WindSourceGuiValues};
use crate::audio::{SpatialSoundManager, TreeAudioManager, TreeRustleParams};
use crate::builder::{
    ContreeBuildJob, ContreeBuilder, PlainBuilder, SceneAccelBuilder, SceneTexUpdateJob,
    SurfaceBuildJob, SurfaceBuilder, VOXEL_TYPE_CHERRY_WOOD,
};
use crate::flora::species;
use crate::geom::{build_bvh, Aabb3, Cuboid, UAabb3};
use crate::particles::{
    ButterflyEmitter, ButterflyEmitterDesc, LeafEmitterDesc, ParticleForces, ParticleHandle,
    ParticleSnapshot, ParticleSystem, PARTICLE_CAPACITY,
};
use crate::tracer::{
    CloudGuiParams, GlassGuiParams, TerrainRayQuery, Tracer, TracerDesc, WindGuiParams,
};
use crate::tree_gen::TreeDesc;
use crate::util::get_sun_dir;
use crate::util::TimeInfo;
use crate::util::{GrowingFloraChunk, GrowingFloraQueue, LatestChunkQueue, ShaderCompiler, BENCH};
use crate::wind::WindResponseCurve;
use crate::RenderFlags;
use crate::{egui_renderer::EguiRenderer, window::WindowState, WaterProfilePreference};
use anyhow::{Context, Result};
use egui::{Color32, ColorImage, FontData, FontDefinitions, FontFamily, RichText, TextureHandle};
use glam::{UVec3, Vec2, Vec3, Vec4};
use petalsonic::config::{AmbisonicsBackend, HrtfBackend};
use std::collections::HashMap;

use std::time::{Duration, Instant};
use ui_style::{
    apply_gui_style, draw_item_panel, draw_placeable_panel, draw_voxel_palette, ItemPanelSlot,
    PlaceablePanelSlot, VoxelPaletteEntry, CUSTOM_GUI_FONT_NAME, CUSTOM_GUI_FONT_PATH,
    FLOWER_ACCENT, GOLD_ACCENT, HOE_SLOT_INDEX, HOE_TOOL_ACCENT, ITEM_PANEL_HOE_ICON_FALLBACK_PATH,
    ITEM_PANEL_HOE_ICON_PATH, ITEM_PANEL_SHOVEL_ICON_FALLBACK_PATH, ITEM_PANEL_SHOVEL_ICON_PATH,
    ITEM_PANEL_SMOOTH_ICON_FALLBACK_PATH, ITEM_PANEL_SMOOTH_ICON_PATH,
    ITEM_PANEL_STAFF_ICON_FALLBACK_PATH, ITEM_PANEL_STAFF_ICON_PATH,
    ITEM_PANEL_TREE_ICON_FALLBACK_PATH, ITEM_PANEL_TREE_ICON_PATH,
    ITEM_PANEL_WATER_ICON_FALLBACK_PATH, ITEM_PANEL_WATER_ICON_PATH, PANEL_BG, PANEL_DARK,
    SAGE_ACCENT, SHADOW_COLOR, SHOVEL_SLOT_INDEX, SHOVEL_TOOL_ACCENT, SMOOTH_SLOT_INDEX,
    SMOOTH_TOOL_ACCENT, SPRINKLER_PLACEABLE_SLOT_INDEX, STAFF_SLOT_INDEX, STAFF_TOOL_ACCENT,
    TREE_PLACEABLE_SLOT_INDEX, TREE_SLOT_INDEX, TREE_TOOL_ACCENT, WATER_TOOL_ACCENT,
};
use verdarium_vkn::{
    Allocator, GpuProfiler, GpuProfilerFrameResults, PipelineStage, SwapchainDesc,
    SwapchainFrameError, SwapchainFrameManager,
};
use verdarium_vkn::{Swapchain, VulkanContext};
use verdarium_water::PondWaterConfig;
use winit::{
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::WindowId,
};

const LEAF_CLUSTER_DISTANCE: f32 = 0.08;
// Muted runs should exercise audio setup, source updates, ray tracing, and pump paths
// without producing audible output for the user.
const MUTED_AUDIO_OUTPUT_GAIN_DB: f32 = -120.0;

#[derive(Clone, Copy, Debug, Default)]
struct TerrainSdfColliderRebuildRequest;

#[derive(Clone, Copy, Debug, Default)]
struct WaterTerrainCacheRebuildRequest;

pub struct App {
    egui_renderer: EguiRenderer,
    loading_state: Option<LoadingState>,
    is_resize_pending: bool,
    swapchain: Swapchain,
    window_state: WindowState,
    frame_manager: SwapchainFrameManager,
    gpu_profiler: Option<GpuProfiler>,
    gpu_profiler_latest_results: Option<GpuProfilerFrameResults>,
    time_info: TimeInfo,
    render_flags: RenderFlags,
    accumulated_mouse_delta: Vec2,
    smoothed_mouse_delta: Vec2,
    cursor_position_physical: Option<Vec2>,
    camera_control_mode: CameraControlMode,
    orbit_middle_mouse_drag_held: bool,
    orbit_middle_mouse_drag_last_position_physical: Option<Vec2>,
    mouse_wheel_dolly: MouseWheelDollySmoother,
    modifiers: ModifiersState,
    perf_logging: bool,
    mute_audio_output: bool,

    tracer: Tracer,

    // builders
    plain_builder: PlainBuilder,
    surface_builder: SurfaceBuilder,
    contree_builder: ContreeBuilder,
    scene_accel_builder: SceneAccelBuilder,

    // gui config and adjustables
    gui_config: GuiConfigFile,
    gui_adjustables: GuiAdjustables,
    wind_sources: Vec<WindSourceGuiValues>,
    debug_tree_pos: Vec3,
    config_panel_visible: bool,
    camera_snapshots: CameraSnapshotLibrary,
    camera_snapshot_draft_name: String,
    camera_snapshot_draft_description: String,
    camera_snapshot_status: Option<String>,
    frame_timing_panel_visible: bool,
    frame_timing_snapshot: FrameTimingSnapshot,
    is_fly_mode: bool,
    item_panel_shovel_icon: Option<TextureHandle>,
    item_panel_smooth_icon: Option<TextureHandle>,
    item_panel_staff_icon: Option<TextureHandle>,
    item_panel_hoe_icon: Option<TextureHandle>,
    item_panel_tree_icon: Option<TextureHandle>,
    item_panel_water_icon: Option<TextureHandle>,
    player_tools: PlayerToolState,
    water_particle_handoff_main_thread_ms: Option<f32>,

    flora_tick: u32,
    flora_tick_accumulator: f32,
    flora_paint_dab_serial: u32,
    growing_flora_chunks: GrowingFloraQueue,
    sun_position_update_tick_accumulator: u32,
    vsm_history_reset_pending: bool,

    #[allow(dead_code)]
    debug_tree_desc: TreeDesc,
    #[allow(dead_code)]
    tree_variation_config: TreeVariationConfig,
    #[allow(dead_code)]
    regenerate_trees_requested: bool,
    prev_bound: UAabb3,
    tree_records: HashMap<u32, TreeRecord>,

    // multi-tree management
    #[allow(dead_code)]
    next_tree_id: u32,
    #[allow(dead_code)]
    single_tree_id: u32, // ID for GUI single tree mode

    particle_system: ParticleSystem,
    leaf_emitters: Vec<TreeLeafEmitter>,
    tree_leaf_emitter_indices: HashMap<u32, Vec<usize>>,
    leaf_emitter_desc: LeafEmitterDesc,
    butterfly_emitters: Vec<ButterflyEmitter>,
    butterfly_emitter_desc: ButterflyEmitterDesc,
    sprinkler_records: Vec<SprinklerRecord>,
    sprinkler_emitters: Vec<SprinklerEmitter>,
    next_sprinkler_id: u32,
    terrain_moisture: TerrainMoistureSystem,
    particle_animation_time_sec: f32,
    water_sim: water::AsyncWaterSim,
    water_terrain_initialized: bool,
    water_terrain_collider_cache_rebuild_pending: bool,
    cpu_solid_voxels: CpuSolidVoxelStore,
    deferred_terrain_sdf_source_refreshes: LatestChunkQueue<water::TerrainSdfSourceRefreshRequest>,
    deferred_terrain_sdf_collider_rebuilds: LatestChunkQueue<TerrainSdfColliderRebuildRequest>,
    deferred_water_terrain_cache_rebuilds: LatestChunkQueue<WaterTerrainCacheRebuildRequest>,
    terrain_sdf_built_source_revisions: HashMap<UVec3, water::TerrainSdfSourceRevision>,
    terrain_sdf_source_refresh_inflight: Option<water::TerrainSdfSourceRefreshInFlight>,
    terrain_sdf_collider_build_inflight: bool,
    terrain_sdf_collider_job_tx: std::sync::mpsc::Sender<water::TerrainSdfColliderWorkerJob>,
    terrain_sdf_collider_result_rx:
        std::sync::mpsc::Receiver<water::TerrainSdfColliderWorkerResult>,
    water_terrain_cache_rebuild_inflight: bool,
    water_terrain_cache_job_tx: std::sync::mpsc::Sender<water::WaterTerrainCacheWorkerJob>,
    water_terrain_cache_result_rx: std::sync::mpsc::Receiver<water::WaterTerrainCacheWorkerResult>,
    particle_snapshots: Vec<ParticleSnapshot>,
    #[allow(dead_code)]
    terrain_harvest_particle_handles: Vec<ParticleHandle>,
    particle_forces: ParticleForces,

    render_start_time: Option<Instant>,
    screenshot_path: Option<String>,
    screenshot_delay: Option<f32>,
    screenshot_taken: bool,
    auto_exit_delay: Option<f32>,
    tree_bench: Option<TreeBench>,
    authored_flora_bench: Option<AuthoredFloraBench>,
    water_edit_soak: Option<water::WaterEditSoak>,
    deferred_chunk_rebuilds: LatestChunkQueue<ChunkRebuildRequest>,
    terrain_chunk_rebuild_inflight: Option<TerrainChunkRebuildInFlight>,

    // note: always keep the context to end, as it has to be destroyed last
    vulkan_ctx: VulkanContext,

    // Keep ownership so the shared PetalSonic engine outlives every subsystem.
    #[allow(dead_code)]
    spatial_sound_manager: SpatialSoundManager,
    tree_audio_manager: TreeAudioManager,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CameraControlMode {
    FreeLook,
    OrbitEdit,
}

impl Default for CameraControlMode {
    fn default() -> Self {
        Self::OrbitEdit
    }
}

impl CameraControlMode {
    fn is_free_look(self) -> bool {
        matches!(self, Self::FreeLook)
    }

    fn is_orbit_edit(self) -> bool {
        matches!(self, Self::OrbitEdit)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct MouseWheelDollySmoother {
    current_lines: f32,
    target_lines: f32,
}

impl MouseWheelDollySmoother {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn add_scroll_lines(&mut self, scroll_lines: f32) {
        if scroll_lines.abs() <= f32::EPSILON || !scroll_lines.is_finite() {
            return;
        }

        let pending_lines = (self.target_lines - self.current_lines + scroll_lines).clamp(
            -MOUSE_WHEEL_DOLLY_MAX_PENDING_LINES,
            MOUSE_WHEEL_DOLLY_MAX_PENDING_LINES,
        );
        self.target_lines = self.current_lines + pending_lines;
    }

    fn advance(&mut self, frame_delta_time: f32) -> f32 {
        if frame_delta_time <= f32::EPSILON || !frame_delta_time.is_finite() {
            return 0.0;
        }

        let remaining_lines = self.target_lines - self.current_lines;
        if remaining_lines.abs() <= MOUSE_WHEEL_DOLLY_SNAP_LINES {
            self.reset();
            return 0.0;
        }

        let alpha = (1.0 - (-MOUSE_WHEEL_DOLLY_INTERPOLATION_RATE * frame_delta_time).exp())
            .clamp(0.0, 1.0);
        let mut advanced_lines = remaining_lines * alpha;
        self.current_lines += advanced_lines;

        let remaining_after_advance = self.target_lines - self.current_lines;
        if remaining_after_advance.abs() <= MOUSE_WHEEL_DOLLY_SNAP_LINES {
            advanced_lines += remaining_after_advance;
            self.reset();
        }

        advanced_lines
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if let Err(err) = self.spatial_sound_manager.stop() {
            log::warn!("Failed to stop audio engine during shutdown: {}", err);
        }

        // Ensure GPU work is done before resources begin destructing
        self.vulkan_ctx.device().wait_idle();
    }
}

impl App {
    pub(super) fn track_growing_flora_chunk(&mut self, chunk_id: UVec3) {
        self.growing_flora_chunks.push(chunk_id, self.flora_tick);
    }

    fn collect_gpu_profiler_frame(&mut self, frame_slot: usize) {
        let Some(profiler) = &self.gpu_profiler else {
            return;
        };
        match profiler.try_collect_frame(frame_slot) {
            Ok(Some(results)) => {
                self.gpu_profiler_latest_results = Some(results);
            }
            Ok(None) => {}
            Err(err) => {
                log::warn!("[PERF][GPU_PROFILER] frame_slot={frame_slot} readback failed: {err}");
            }
        }
    }

    fn log_gpu_profiler_frame(&self, frame_count: u64) {
        let Some(results) = self.gpu_profiler_latest_results.as_ref() else {
            return;
        };
        if results.scopes.is_empty() && results.dropped_scope_count == 0 {
            return;
        }

        let scopes = results
            .scopes
            .iter()
            .map(|scope| format!("{}={:.0}us", scope.name, scope.duration_us()))
            .collect::<Vec<_>>()
            .join(" ");
        log::info!(
            "[PERF][GPU_FRAME_SCOPE] frame {} scopes={} dropped={} {}",
            frame_count,
            results.scopes.len(),
            results.dropped_scope_count,
            scopes,
        );
    }

    fn update_growing_flora_chunk(&mut self) {
        if self.deferred_surface_rebuild_inflight() {
            return;
        }

        let Some(GrowingFloraChunk {
            chunk_id,
            last_flora_tick,
        }) = self
            .growing_flora_chunks
            .pop_nearest_to(self.tracer.camera_position(), VOXEL_DIM_PER_CHUNK)
        else {
            return;
        };

        let tick_delta = self.flora_tick.wrapping_sub(last_flora_tick);
        let growth_tick_delta = tick_delta / FLORA_GROWTH_SPEED_DIVISOR;
        if growth_tick_delta == 0 {
            self.growing_flora_chunks.push(chunk_id, last_flora_tick);
            return;
        }
        match self
            .surface_builder
            .update_flora_growth_for_chunk(chunk_id, growth_tick_delta)
        {
            Ok(true) => {
                // still growing, requeue from the tick we successfully applied through
                let consumed_ticks = growth_tick_delta * FLORA_GROWTH_SPEED_DIVISOR;
                self.growing_flora_chunks
                    .push(chunk_id, last_flora_tick.wrapping_add(consumed_ticks));
            }
            Ok(false) => {}
            Err(err) => {
                log::warn!(
                    "Failed to update flora growth for chunk {}: {}",
                    chunk_id,
                    err
                );
                self.growing_flora_chunks.push(chunk_id, last_flora_tick);
            }
        }
    }
}

impl WorldBuildBackend for App {
    fn apply_voxel_edit(&mut self, edit: VoxelEdit) -> Result<()> {
        let result = world_ops::apply_voxel_edit(&mut self.plain_builder, edit);
        if result.is_ok() {
            self.request_vsm_history_reset();
        }
        result
    }

    fn apply_build_edit(&mut self, edit: BuildEdit) -> Result<()> {
        match edit {
            BuildEdit::RebuildMesh(bound) => {
                let chunk_ids =
                    world_ops::affected_chunk_indices_for_bound(bound, VOXEL_DIM_PER_CHUNK);
                self.enqueue_deferred_chunk_rebuilds(&chunk_ids);
            }
            BuildEdit::RebuildMeshWithoutFlora(bound) => {
                let chunk_ids =
                    world_ops::affected_chunk_indices_for_bound(bound, VOXEL_DIM_PER_CHUNK);
                self.enqueue_deferred_chunk_rebuilds_without_flora(&chunk_ids);
            }
            BuildEdit::RebuildChunks(chunk_ids) => {
                self.enqueue_deferred_chunk_rebuilds(&chunk_ids);
            }
            BuildEdit::RebuildChunksWithoutFlora(chunk_ids) => {
                self.enqueue_deferred_chunk_rebuilds_without_flora(&chunk_ids);
            }
        }
        Ok(())
    }
}

const VOXEL_DIM_PER_CHUNK: UVec3 = UVec3::new(256, 256, 256);
const CHUNK_DIM: UVec3 = UVec3::new(1, 1, 1);
const FREE_ATLAS_DIM: UVec3 = UVec3::new(512, 512, 512);
const MAX_FRAMES_IN_FLIGHT: usize = 1;
const GPU_PROFILER_MAX_SCOPES_PER_FRAME: usize = 64;
const TERRAIN_EDIT_DEFAULT_RADIUS: f32 = 0.08;
const TERRAIN_EDIT_RADIUS_MIN: f32 = 0.03;
const TERRAIN_EDIT_RADIUS_MAX: f32 = 0.36;
const TERRAIN_EDIT_RADIUS_SCROLL_STEP: f32 = 0.01;
const ORBIT_CAMERA_FOCUS: Vec3 = Vec3::new(0.5, 0.5, 0.5);
const ORBIT_CAMERA_MIN_DISTANCE: f32 = 0.2;
const ORBIT_CAMERA_MAX_DISTANCE: f32 = 5.0;
const ORBIT_CAMERA_DOLLY_SPEED: f32 = 0.75;
const ORBIT_CAMERA_MOUSE_DRAG_RADIANS_PER_PIXEL: f32 = 0.005;
const MOUSE_WHEEL_DOLLY_SECONDS_PER_LINE: f32 = 0.16;
const MOUSE_WHEEL_DOLLY_INTERPOLATION_RATE: f32 = 16.0;
const MOUSE_WHEEL_DOLLY_SNAP_LINES: f32 = 0.001;
const MOUSE_WHEEL_DOLLY_MAX_PENDING_LINES: f32 = 24.0;
const ORBIT_CAMERA_MAX_ELEVATION_RAD: f32 = std::f32::consts::FRAC_PI_2 - 0.04;
const SHOVEL_DIG_INTERVAL: Duration = Duration::from_millis(80);
const SHOVEL_RAY_QUERY_DISTANCE: f32 = 10.0;
const TERRAIN_SMOOTH_STRENGTH: f32 = 0.55;
const TERRAIN_SMOOTH_MAX_DELTA: f32 = 0.025;
const TERRAIN_SMOOTH_DEADBAND: f32 = 0.0035;
const TERRAIN_EDIT_LOOP_PATH: &str =
    "assets/sfx/ROCKMisc_Designed Rock Movement Loop A_SARM_RkBrck_Stereo-Loop.wav";
const TERRAIN_EDIT_LOOP_VOLUME_DB: f32 = 20.0;
const TERRAIN_EDIT_LOOP_MUTED_VOLUME_DB: f32 = -80.0;
const ITEM_PANEL_SCROLL_SFX_PATH: &str =
    "assets/sfx/MECHSwtch_Game Boy Advance SP, B Button, On 05_SARM_BTNS.wav";
const ITEM_PANEL_SCROLL_SFX_VOLUME_DB: f32 = -6.0;
const FLORA_SPROUT_DELAY_TICKS: u32 = 2;
const DEBUG_AUDIO_WALL_MIN: Vec3 = Vec3::new(300.0, 0.0, 512.0);
const DEBUG_AUDIO_WALL_MAX: Vec3 = Vec3::new(320.0, 256.0, 600.0);
const FLORA_FULL_GROWTH_TICKS: u32 = 30;
const FLORA_GROWTH_SPEED_DIVISOR: u32 = 10;
// Trimmed grasses should read as clipped, not newly sprouted: the shader's floor-based
// height trim makes low growth values collapse short grass to one voxel.
const FLORA_TRIM_MAX_GROWTH_PROGRESS: u32 = 160;
const SUN_POSITION_UPDATE_INTERVAL_TICKS: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActiveVoxelType {
    All,
    Dirt,
    Sand,
    CherryWood,
    OakWood,
    Rock,
}

const BACKPACK_VOXEL_TYPES: [ActiveVoxelType; 5] = [
    ActiveVoxelType::Dirt,
    ActiveVoxelType::Sand,
    ActiveVoxelType::CherryWood,
    ActiveVoxelType::OakWood,
    ActiveVoxelType::Rock,
];

impl ActiveVoxelType {
    pub(super) fn voxel_type(self) -> Option<u32> {
        match self {
            ActiveVoxelType::All => None,
            ActiveVoxelType::Dirt => Some(crate::builder::VOXEL_TYPE_DIRT),
            ActiveVoxelType::Sand => Some(crate::builder::VOXEL_TYPE_SAND),
            ActiveVoxelType::CherryWood => Some(crate::builder::VOXEL_TYPE_CHERRY_WOOD),
            ActiveVoxelType::OakWood => Some(crate::builder::VOXEL_TYPE_OAK_WOOD),
            ActiveVoxelType::Rock => Some(crate::builder::VOXEL_TYPE_ROCK),
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            ActiveVoxelType::All => "All",
            ActiveVoxelType::Dirt => "Dirt",
            ActiveVoxelType::Sand => "Sand",
            ActiveVoxelType::CherryWood => "Cherry wood",
            ActiveVoxelType::OakWood => "Oak wood",
            ActiveVoxelType::Rock => "Rock",
        }
    }

    pub(super) fn color(self) -> Color32 {
        match self {
            ActiveVoxelType::All => Color32::from_rgb(235, 230, 215),
            ActiveVoxelType::Dirt => Color32::from_rgb(178, 124, 80),
            ActiveVoxelType::Sand => Color32::from_rgb(229, 204, 126),
            ActiveVoxelType::CherryWood => Color32::from_rgb(219, 128, 152),
            ActiveVoxelType::OakWood => Color32::from_rgb(159, 110, 70),
            ActiveVoxelType::Rock => Color32::from_rgb(168, 176, 190),
        }
    }
}

impl App {
    fn apply_debug_audio_wall(&mut self) -> Result<()> {
        let wall = Cuboid::from_min_max(DEBUG_AUDIO_WALL_MIN, DEBUG_AUDIO_WALL_MAX);
        let wall_aabb = Aabb3::new(DEBUG_AUDIO_WALL_MIN, DEBUG_AUDIO_WALL_MAX);
        let bvh_nodes = build_bvh(&[wall_aabb], &[0]).map_err(anyhow::Error::msg)?;

        let result = self.plain_builder.chunk_modify_cuboids_with_voxel_type(
            &bvh_nodes,
            &[wall],
            VOXEL_TYPE_CHERRY_WOOD,
        );
        if result.is_ok() {
            self.request_vsm_history_reset();
        }
        result
    }

    fn master_volume_gain_db(master_volume_db: f32, mute_audio_output: bool) -> f32 {
        if mute_audio_output {
            MUTED_AUDIO_OUTPUT_GAIN_DB
        } else {
            master_volume_db.clamp(-20.0, 20.0)
        }
    }

    fn effective_master_volume_gain_db(&self) -> f32 {
        Self::master_volume_gain_db(
            self.gui_adjustables.master_volume.value,
            self.mute_audio_output,
        )
    }

    fn apply_effective_master_volume_gain(&self, error_context: &str) {
        if let Err(err) = self
            .spatial_sound_manager
            .set_global_volume_gain_db(self.effective_master_volume_gain_db())
        {
            log::error!("{}: {}", error_context, err);
        }
    }

    fn set_audio_output_muted(&mut self, muted: bool, reason: &str) {
        if self.mute_audio_output == muted {
            return;
        }

        self.mute_audio_output = muted;
        self.apply_effective_master_volume_gain("Failed to apply audio mute state");
        log::info!(
            "[AUDIO] Global output {} ({})",
            if muted { "muted" } else { "unmuted" },
            reason
        );
    }

    fn toggle_audio_output_mute(&mut self) {
        self.set_audio_output_muted(!self.mute_audio_output, "M key");
    }

    fn update_audio_ray_tracing(&mut self) {
        self.spatial_sound_manager
            .set_audio_ray_tracing_enabled(self.gui_adjustables.audio_ray_tracing_enabled.value);
    }

    fn update_spatial_audio_backends(&mut self) {
        let use_ambisonics = self.gui_adjustables.audio_use_ambisonics.value;
        let ambisonics_backend =
            Self::selected_ambisonics_backend(self.gui_adjustables.audio_ambisonics_backend.value);
        let hrtf_backend = Self::effective_hrtf_backend(
            Self::selected_hrtf_backend(self.gui_adjustables.audio_hrtf_backend.value),
            use_ambisonics,
            ambisonics_backend,
        );

        if let Err(err) = self.spatial_sound_manager.set_spatial_audio_rendering(
            hrtf_backend,
            use_ambisonics,
            ambisonics_backend,
        ) {
            log::error!("Failed to apply spatial audio rendering setting: {}", err);
        }
    }

    fn selected_hrtf_backend(value: u32) -> HrtfBackend {
        match value {
            1 => HrtfBackend::SteamAudio,
            _ => HrtfBackend::Native,
        }
    }

    fn selected_ambisonics_backend(value: u32) -> AmbisonicsBackend {
        match value {
            1 => AmbisonicsBackend::SteamAudio,
            _ => AmbisonicsBackend::Native,
        }
    }

    fn effective_hrtf_backend(
        direct_hrtf_backend: HrtfBackend,
        use_ambisonics: bool,
        ambisonics_backend: AmbisonicsBackend,
    ) -> HrtfBackend {
        if !use_ambisonics {
            return direct_hrtf_backend;
        }

        match ambisonics_backend {
            AmbisonicsBackend::Native => HrtfBackend::Native,
            AmbisonicsBackend::SteamAudio => HrtfBackend::SteamAudio,
        }
    }

    fn tree_audio_wind_response_curve(gui_adjustables: &GuiAdjustables) -> WindResponseCurve {
        WindResponseCurve {
            min_strength: gui_adjustables.tree_wind_response_min_strength.value,
            max_strength: gui_adjustables.tree_wind_response_max_strength.value,
            power: 1.0,
        }
    }

    fn tree_rustle_params(gui_adjustables: &GuiAdjustables) -> TreeRustleParams {
        TreeRustleParams {
            base_wind: gui_adjustables.tree_rustle_base_wind.value,
            gustiness: gui_adjustables.tree_rustle_gustiness.value,
            leaf_density: gui_adjustables.tree_rustle_leaf_density.value,
            dryness: gui_adjustables.tree_rustle_dryness.value,
            branch: gui_adjustables.tree_rustle_branch.value,
            air: gui_adjustables.tree_rustle_air.value,
            leaf_body: gui_adjustables.tree_rustle_leaf_body.value,
            crackle: gui_adjustables.tree_rustle_crackle.value,
            brightness: gui_adjustables.tree_rustle_brightness.value,
        }
    }

    fn wind_gui_params(wind_sources: &[WindSourceGuiValues]) -> WindGuiParams {
        WindGuiParams {
            sources: GuiAdjustables::active_wind_sources(wind_sources),
        }
    }

    pub fn new(_event_loop: &ActiveEventLoop, options: &crate::AppOptions) -> Result<Self> {
        let chunk_bound = UAabb3::new(UVec3::ZERO, CHUNK_DIM);
        let window_state = Self::create_window_state(_event_loop, options);
        let vulkan_ctx = Self::create_vulkan_context(&window_state);

        let shader_compiler = ShaderCompiler::new().unwrap();

        let device = vulkan_ctx.device();

        let allocator = Allocator::new_for_context(&vulkan_ctx);

        let swapchain = Swapchain::new(
            vulkan_ctx.clone(),
            window_state.window_extent(),
            SwapchainDesc {
                present_mode: options.present_mode.map(|mode| mode.as_present_mode()),
                image_count_override: options.swapchain_images,
                ..Default::default()
            },
        );

        let frame_manager = SwapchainFrameManager::new(
            device,
            vulkan_ctx.command_pool(),
            MAX_FRAMES_IN_FLIGHT,
            swapchain.image_count(),
        );
        let gpu_profiler = options
            .perf
            .then(|| {
                GpuProfiler::maybe_new(
                    &vulkan_ctx,
                    MAX_FRAMES_IN_FLIGHT,
                    GPU_PROFILER_MAX_SCOPES_PER_FRAME,
                    "PERF][GPU_PROFILER",
                )
            })
            .flatten();

        let renderer = EguiRenderer::new(
            vulkan_ctx.clone(),
            &window_state.window(),
            allocator.clone(),
            &shader_compiler,
            swapchain.get_render_pass(),
        );

        let plain_builder = PlainBuilder::new(
            vulkan_ctx.clone(),
            &shader_compiler,
            allocator.clone(),
            CHUNK_DIM * VOXEL_DIM_PER_CHUNK,
            FREE_ATLAS_DIM,
        );

        let mut surface_builder = SurfaceBuilder::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            plain_builder.get_resources(),
            VOXEL_DIM_PER_CHUNK,
            chunk_bound,
        );
        if options.perf {
            surface_builder.enable_gpu_job_profiling(32);
        }

        let contree_builder = ContreeBuilder::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            surface_builder.get_resources(),
            CHUNK_DIM,
            VOXEL_DIM_PER_CHUNK,
            512 * 1024 * 1024, // node buffer pool size
            512 * 1024 * 1024, // leaf buffer pool size
        );

        let scene_accel_builder = SceneAccelBuilder::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            chunk_bound,
        )?;

        let chunk_indices = {
            let mut indices = Vec::new();
            for x in 0..CHUNK_DIM.x {
                for y in 0..CHUNK_DIM.y {
                    for z in 0..CHUNK_DIM.z {
                        indices.push(UVec3::new(x, y, z));
                    }
                }
            }
            indices
        };

        // Shared spatial audio engine (PetalSonic) used by both the tracer (camera)
        // and the app-level tree ambience sources.
        let spatial_sound_manager = SpatialSoundManager::new(
            1024,
            contree_builder.audio_ray_tracer(),
            options.audio_output_device.clone(),
        )?;

        let tracer = Tracer::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            chunk_bound,
            window_state.window_extent(),
            contree_builder.get_resources(),
            scene_accel_builder.get_resources(),
            TracerDesc {
                scaling_factor: 0.5,
            },
            spatial_sound_manager.clone(),
        )?;

        let camera_snapshots = match CameraSnapshotLibrary::load_default() {
            Ok(library) => {
                log::info!(
                    "[CAMERA_SNAPSHOT] Loaded {} snapshots from {}",
                    library.snapshots().len(),
                    library.path().display()
                );
                library
            }
            Err(err) => {
                if options.camera_snapshot.is_some() {
                    return Err(err).context(
                        "camera snapshot was requested, but the snapshot file could not be loaded",
                    );
                }
                log::warn!(
                    "[CAMERA_SNAPSHOT] Failed to load snapshots; starting with an empty library: {}",
                    err
                );
                CameraSnapshotLibrary::empty_default()
            }
        };
        let camera_snapshot_draft_name = camera_snapshots.unique_name("snapshot");

        let debug_tree_pos = Vec3::new(2.0, 0.2, 2.0);
        let gui_config = GuiConfigLoader::load();
        let mut gui_adjustables = GuiAdjustables::from_config(&gui_config);
        let wind_sources = crate::app::wind_sources_from_config(&gui_config);

        let color_to_vec4 = |color: Color32| -> Vec4 {
            Vec4::new(
                color.r() as f32 / 255.0,
                color.g() as f32 / 255.0,
                color.b() as f32 / 255.0,
                1.0,
            )
        };

        let particle_system = ParticleSystem::new(PARTICLE_CAPACITY);
        let leaf_emitters = Vec::new();
        let tree_leaf_emitter_indices = HashMap::new();
        let leaf_emitter_desc = LeafEmitterDesc {
            color_low: color_to_vec4(gui_adjustables.leaves_bottom_color.value),
            color_high: color_to_vec4(gui_adjustables.leaves_tip_color.value),
            ..LeafEmitterDesc::default()
        };
        let tree_audio_manager = TreeAudioManager::new(
            spatial_sound_manager.clone(),
            Self::tree_audio_wind_response_curve(&gui_adjustables),
            gui_adjustables.tree_wind_volume_db.value,
            Self::tree_rustle_params(&gui_adjustables),
        );
        let butterfly_emitters = Vec::new();
        let butterfly_emitter_desc = Self::butterfly_desc_from_gui_adjustables(&gui_adjustables);
        let sprinkler_records = Vec::new();
        let sprinkler_emitters = Vec::new();
        let next_sprinkler_id = 1;
        let terrain_moisture = TerrainMoistureSystem::default();
        let particle_snapshots = Vec::with_capacity(PARTICLE_CAPACITY);
        // Start with the chosen profile and proportional world grid. For the implicit
        // default run, apply persisted GUI water sliders, then let explicit CLI
        // overrides win on top.
        let world_extent = CHUNK_DIM.as_vec3();
        let cells_per_unit = 32.0;
        let world_grid_dim = UVec3::new(
            (world_extent.x * cells_per_unit).ceil() as u32,
            (world_extent.y * cells_per_unit).ceil() as u32,
            (world_extent.z * cells_per_unit).ceil() as u32,
        );
        let mut water_config = match options.water_profile {
            Some(WaterProfilePreference::Default) | None => PondWaterConfig::default()
                .with_collider_bounds(Vec3::ZERO, world_extent)
                .with_grid_dim(world_grid_dim),
            Some(WaterProfilePreference::Performance) => PondWaterConfig::default()
                .with_substep_hz(60.0)
                .with_terrain_collision_margin_cells(0.0)
                .with_linear_damping_per_sec(1.5)
                .with_collider_bounds(Vec3::ZERO, world_extent)
                .with_grid_dim(world_grid_dim),
        };
        let water_gui_config_applied = options.water_profile.is_none();
        if water_gui_config_applied {
            water::apply_water_gui_adjustables_to_config(&mut water_config, &gui_adjustables);
        }
        if let Some(particle_count) = options.water_particles {
            water_config = water_config.with_particle_count(particle_count);
        }
        if let Some(edge_len) = options.water_particle_edge_len {
            water_config = water_config.with_particle_edge_len(edge_len);
        }
        if let Some(grid_dim) = options.water_grid {
            water_config = water_config.with_cubic_grid_dim(grid_dim);
        }
        if let Some(substep_hz) = options.water_substep_hz {
            water_config = water_config.with_substep_hz(substep_hz);
        }
        if let Some(margin_cells) = options.water_terrain_margin_cells {
            water_config = water_config.with_terrain_collision_margin_cells(margin_cells);
        }
        if let Some(damping_per_sec) = options.water_damping {
            water_config = water_config.with_linear_damping_per_sec(damping_per_sec);
        }
        if let Some(damping_per_sec) = options.water_terrain_tangent_damping {
            water_config = water_config.with_terrain_tangent_damping_per_sec(damping_per_sec);
        }
        if let Some(stiffness) = options.water_stiffness {
            water_config = water_config.with_stiffness(stiffness);
        }
        if let Some(gamma) = options.water_gamma {
            water_config = water_config.with_gamma(gamma);
        }
        if let Some(j_min) = options.water_j_min {
            water_config = water_config.with_j_min(j_min);
        }
        water::sync_water_gui_adjustables_from_config(&mut gui_adjustables, &water_config);

        log::info!(
            "[WATER] config profile={:?} gui_config_applied={} particles={} grid={:?} substep_dt={:.6}s terrain_margin_cells={:.2} boundary_density_min_fluid_fraction={:.2} boundary_density_max_correction={:.2} boundary_density_transition_cells={:.2} damping={:.2}/s quiet_settling={:.2}/{:.2}/s terrain_tangent_damping={:.2}/s debug_spawn_height_offset={:.2} gravity={:?} stiffness={:.1} gamma={:.2} j_min={:.3} viscosity={:.3} pressure_floor={:.3} wall_damping={:.2} collider_bounds {:?}..{:?} cells_per_unit={}",
            options.water_profile,
            water_gui_config_applied,
            water_config.particle_count,
            water_config.grid_dim,
            water_config.substep_dt,
            water_config.terrain_collision_margin_cells,
            water_config.terrain_density_min_fluid_fraction,
            water_config.terrain_density_max_correction_factor,
            water_config.terrain_density_occupancy_transition_cells,
            water_config.linear_damping_per_sec,
            water_config.quiet_settling_velocity_damping_per_sec,
            water_config.quiet_settling_affine_damping_per_sec,
            water_config.terrain_tangent_damping_per_sec,
            water_config.debug_spawn_height_offset,
            water_config.gravity,
            water_config.stiffness,
            water_config.gamma,
            water_config.j_min,
            water_config.dynamic_viscosity,
            water_config.pressure_floor,
            water_config.wall_damping,
            water_config.collider.min_ws,
            water_config.collider.max_ws,
            cells_per_unit,
        );
        let water_sim = water::AsyncWaterSim::new(water_config);
        let (terrain_sdf_collider_job_tx, terrain_sdf_collider_result_rx) =
            Self::spawn_terrain_sdf_collider_worker();
        let (water_terrain_cache_job_tx, water_terrain_cache_result_rx) =
            Self::spawn_water_terrain_cache_worker();
        let terrain_harvest_particle_handles = Vec::with_capacity(256);
        let particle_forces = ParticleForces {
            linear_damping: 0.08,
            ..ParticleForces::default()
        };

        let mut app = Self {
            vulkan_ctx,
            egui_renderer: renderer,
            window_state,
            loading_state: Some(LoadingState {
                chunk_indices,
                current: 0,
                step_label: "Initializing...".to_owned(),
                phase: LoadingPhase::Terrain,
            }),

            accumulated_mouse_delta: Vec2::ZERO,
            smoothed_mouse_delta: Vec2::ZERO,
            cursor_position_physical: None,
            camera_control_mode: CameraControlMode::default(),
            orbit_middle_mouse_drag_held: false,
            orbit_middle_mouse_drag_last_position_physical: None,
            mouse_wheel_dolly: MouseWheelDollySmoother::default(),
            modifiers: ModifiersState::default(),
            perf_logging: options.perf,
            mute_audio_output: options.mute,

            swapchain,
            frame_manager,
            gpu_profiler,
            gpu_profiler_latest_results: None,

            tracer,

            plain_builder,
            surface_builder,
            contree_builder,
            scene_accel_builder,

            is_resize_pending: false,
            time_info: TimeInfo::default(),
            render_flags: RenderFlags::from(options),

            gui_config,
            gui_adjustables,
            wind_sources,
            debug_tree_pos,
            debug_tree_desc: TreeDesc::default(),
            tree_variation_config: TreeVariationConfig::default(),
            regenerate_trees_requested: false,
            prev_bound: Default::default(),
            tree_records: HashMap::new(),
            config_panel_visible: false,
            camera_snapshots,
            camera_snapshot_draft_name,
            camera_snapshot_draft_description: String::new(),
            camera_snapshot_status: None,
            frame_timing_panel_visible: options.perf,
            frame_timing_snapshot: FrameTimingSnapshot::default(),
            is_fly_mode: true,
            item_panel_shovel_icon: None,
            item_panel_smooth_icon: None,
            item_panel_staff_icon: None,
            item_panel_hoe_icon: None,
            item_panel_tree_icon: None,
            item_panel_water_icon: None,
            player_tools: PlayerToolState::default(),
            water_particle_handoff_main_thread_ms: None,
            flora_tick: FLORA_FULL_GROWTH_TICKS,
            flora_tick_accumulator: 0.0,
            flora_paint_dab_serial: 0,
            growing_flora_chunks: GrowingFloraQueue::default(),
            sun_position_update_tick_accumulator: 0,
            vsm_history_reset_pending: true,

            // multi-tree management
            next_tree_id: 1, // Start from 1, use 0 for GUI single tree
            single_tree_id: 0,

            particle_system,
            leaf_emitters,
            tree_leaf_emitter_indices,
            leaf_emitter_desc,
            butterfly_emitters,
            butterfly_emitter_desc,
            sprinkler_records,
            sprinkler_emitters,
            next_sprinkler_id,
            terrain_moisture,
            particle_animation_time_sec: 0.0,
            water_sim,
            water_terrain_initialized: false,
            water_terrain_collider_cache_rebuild_pending: false,
            cpu_solid_voxels: CpuSolidVoxelStore::default(),
            deferred_terrain_sdf_source_refreshes: LatestChunkQueue::default(),
            deferred_terrain_sdf_collider_rebuilds: LatestChunkQueue::default(),
            deferred_water_terrain_cache_rebuilds: LatestChunkQueue::default(),
            terrain_sdf_built_source_revisions: HashMap::new(),
            terrain_sdf_source_refresh_inflight: None,
            terrain_sdf_collider_build_inflight: false,
            terrain_sdf_collider_job_tx,
            terrain_sdf_collider_result_rx,
            water_terrain_cache_rebuild_inflight: false,
            water_terrain_cache_job_tx,
            water_terrain_cache_result_rx,
            particle_snapshots,
            terrain_harvest_particle_handles,
            particle_forces,

            render_start_time: None,
            screenshot_path: options.screenshot_path.clone(),
            screenshot_delay: options.screenshot_delay,
            screenshot_taken: false,
            auto_exit_delay: options.auto_exit_delay,
            tree_bench: options.tree_bench.then(|| {
                let mode = if options.tree_bench_min_thickness {
                    TreeBenchMode::MinTrunkThickness
                } else {
                    TreeBenchMode::TreeHeight
                };
                TreeBench::new(options.tree_bench_samples, mode, options.tree_bench_rapid)
            }),
            authored_flora_bench: options
                .authored_flora_bench
                .then(|| AuthoredFloraBench::new(options.authored_flora_bench_samples)),
            water_edit_soak: options.water_edit_soak.then(water::WaterEditSoak::default),
            deferred_chunk_rebuilds: LatestChunkQueue::default(),
            terrain_chunk_rebuild_inflight: None,

            spatial_sound_manager,
            tree_audio_manager,
        };

        if app.mute_audio_output {
            log::info!(
                "--mute: forcing master audio output volume to 0 while keeping audio engine processing active"
            );
        }
        app.apply_effective_master_volume_gain("Failed to apply initial master volume");

        app.apply_startup_camera_snapshot(options.camera_snapshot.as_deref())?;

        app.configure_gui_font()?;
        app.load_item_panel_icons()?;

        Ok(app)
    }

    fn configure_gui_font(&mut self) -> Result<()> {
        if let Some(font_path) = CUSTOM_GUI_FONT_PATH {
            let font_bytes = std::fs::read(font_path)
                .with_context(|| format!("Failed to read GUI font from {font_path}"))?;
            let ctx = self.egui_renderer.context();

            let mut fonts = FontDefinitions::default();
            fonts.font_data.insert(
                CUSTOM_GUI_FONT_NAME.to_owned(),
                FontData::from_owned(font_bytes).into(),
            );

            if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
                family.insert(0, CUSTOM_GUI_FONT_NAME.to_owned());
            }

            if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
                family.insert(0, CUSTOM_GUI_FONT_NAME.to_owned());
            }

            ctx.set_fonts(fonts);
            log::info!("Loaded custom GUI font from {}", font_path);
        }

        Ok(())
    }

    fn load_item_panel_icons(&mut self) -> Result<()> {
        let shovel_path = if std::path::Path::new(ITEM_PANEL_SHOVEL_ICON_PATH).exists() {
            ITEM_PANEL_SHOVEL_ICON_PATH
        } else {
            log::warn!(
                "Item panel icon not found at {}. Falling back to {}",
                ITEM_PANEL_SHOVEL_ICON_PATH,
                ITEM_PANEL_SHOVEL_ICON_FALLBACK_PATH
            );
            ITEM_PANEL_SHOVEL_ICON_FALLBACK_PATH
        };

        let staff_path = if std::path::Path::new(ITEM_PANEL_STAFF_ICON_PATH).exists() {
            ITEM_PANEL_STAFF_ICON_PATH
        } else {
            log::warn!(
                "Item panel icon not found at {}. Falling back to {}",
                ITEM_PANEL_STAFF_ICON_PATH,
                ITEM_PANEL_STAFF_ICON_FALLBACK_PATH
            );
            ITEM_PANEL_STAFF_ICON_FALLBACK_PATH
        };

        let shovel_bytes = std::fs::read(shovel_path)
            .with_context(|| format!("Failed to read item panel icon from {shovel_path}"))?;
        let shovel_rgba = image::load_from_memory(&shovel_bytes)
            .with_context(|| format!("Failed to decode item panel icon from {shovel_path}"))?
            .to_rgba8();
        let shovel_size = [shovel_rgba.width() as usize, shovel_rgba.height() as usize];
        let shovel_pixels = shovel_rgba.into_raw();
        let shovel_image = ColorImage::from_rgba_unmultiplied(shovel_size, &shovel_pixels);

        let shovel_texture = self.egui_renderer.context().load_texture(
            "item_panel_wooden_shovel",
            shovel_image,
            egui::TextureOptions::NEAREST,
        );
        self.item_panel_shovel_icon = Some(shovel_texture);

        let staff_bytes = std::fs::read(staff_path)
            .with_context(|| format!("Failed to read item panel icon from {staff_path}"))?;
        let staff_rgba = image::load_from_memory(&staff_bytes)
            .with_context(|| format!("Failed to decode item panel icon from {staff_path}"))?
            .to_rgba8();
        let staff_size = [staff_rgba.width() as usize, staff_rgba.height() as usize];
        let staff_pixels = staff_rgba.into_raw();
        let staff_image = ColorImage::from_rgba_unmultiplied(staff_size, &staff_pixels);

        let staff_texture = self.egui_renderer.context().load_texture(
            "item_panel_wooden_staff",
            staff_image,
            egui::TextureOptions::NEAREST,
        );
        self.item_panel_staff_icon = Some(staff_texture);

        let smooth_path = if std::path::Path::new(ITEM_PANEL_SMOOTH_ICON_PATH).exists() {
            ITEM_PANEL_SMOOTH_ICON_PATH
        } else {
            log::warn!(
                "Item panel icon not found at {}. Falling back to {}",
                ITEM_PANEL_SMOOTH_ICON_PATH,
                ITEM_PANEL_SMOOTH_ICON_FALLBACK_PATH
            );
            ITEM_PANEL_SMOOTH_ICON_FALLBACK_PATH
        };

        let smooth_bytes = std::fs::read(smooth_path)
            .with_context(|| format!("Failed to read item panel icon from {smooth_path}"))?;
        let smooth_rgba = image::load_from_memory(&smooth_bytes)
            .with_context(|| format!("Failed to decode item panel icon from {smooth_path}"))?
            .to_rgba8();
        let smooth_size = [smooth_rgba.width() as usize, smooth_rgba.height() as usize];
        let smooth_pixels = smooth_rgba.into_raw();
        let smooth_image = ColorImage::from_rgba_unmultiplied(smooth_size, &smooth_pixels);

        let smooth_texture = self.egui_renderer.context().load_texture(
            "item_panel_smooth",
            smooth_image,
            egui::TextureOptions::NEAREST,
        );
        self.item_panel_smooth_icon = Some(smooth_texture);

        let hoe_path = if std::path::Path::new(ITEM_PANEL_HOE_ICON_PATH).exists() {
            ITEM_PANEL_HOE_ICON_PATH
        } else {
            log::warn!(
                "Item panel icon not found at {}. Falling back to {}",
                ITEM_PANEL_HOE_ICON_PATH,
                ITEM_PANEL_HOE_ICON_FALLBACK_PATH
            );
            ITEM_PANEL_HOE_ICON_FALLBACK_PATH
        };

        let hoe_bytes = std::fs::read(hoe_path)
            .with_context(|| format!("Failed to read item panel icon from {hoe_path}"))?;
        let hoe_rgba = image::load_from_memory(&hoe_bytes)
            .with_context(|| format!("Failed to decode item panel icon from {hoe_path}"))?
            .to_rgba8();
        let hoe_size = [hoe_rgba.width() as usize, hoe_rgba.height() as usize];
        let hoe_pixels = hoe_rgba.into_raw();
        let hoe_image = ColorImage::from_rgba_unmultiplied(hoe_size, &hoe_pixels);

        let hoe_texture = self.egui_renderer.context().load_texture(
            "item_panel_wooden_hoe",
            hoe_image,
            egui::TextureOptions::NEAREST,
        );
        self.item_panel_hoe_icon = Some(hoe_texture);

        let tree_path = if std::path::Path::new(ITEM_PANEL_TREE_ICON_PATH).exists() {
            ITEM_PANEL_TREE_ICON_PATH
        } else {
            log::warn!(
                "Item panel icon not found at {}. Falling back to {}",
                ITEM_PANEL_TREE_ICON_PATH,
                ITEM_PANEL_TREE_ICON_FALLBACK_PATH
            );
            ITEM_PANEL_TREE_ICON_FALLBACK_PATH
        };

        let tree_bytes = std::fs::read(tree_path)
            .with_context(|| format!("Failed to read item panel icon from {tree_path}"))?;
        let tree_rgba = image::load_from_memory(&tree_bytes)
            .with_context(|| format!("Failed to decode item panel icon from {tree_path}"))?
            .to_rgba8();
        let tree_size = [tree_rgba.width() as usize, tree_rgba.height() as usize];
        let tree_pixels = tree_rgba.into_raw();
        let tree_image = ColorImage::from_rgba_unmultiplied(tree_size, &tree_pixels);

        let tree_texture = self.egui_renderer.context().load_texture(
            "item_panel_tree_plant",
            tree_image,
            egui::TextureOptions::NEAREST,
        );
        self.item_panel_tree_icon = Some(tree_texture);

        let water_path = if std::path::Path::new(ITEM_PANEL_WATER_ICON_PATH).exists() {
            ITEM_PANEL_WATER_ICON_PATH
        } else {
            log::warn!(
                "Item panel icon not found at {}. Falling back to {}",
                ITEM_PANEL_WATER_ICON_PATH,
                ITEM_PANEL_WATER_ICON_FALLBACK_PATH
            );
            ITEM_PANEL_WATER_ICON_FALLBACK_PATH
        };

        let water_bytes = std::fs::read(water_path)
            .with_context(|| format!("Failed to read item panel icon from {water_path}"))?;
        let water_rgba = image::load_from_memory(&water_bytes)
            .with_context(|| format!("Failed to decode item panel icon from {water_path}"))?
            .to_rgba8();
        let water_size = [water_rgba.width() as usize, water_rgba.height() as usize];
        let water_pixels = water_rgba.into_raw();
        let water_image = ColorImage::from_rgba_unmultiplied(water_size, &water_pixels);

        let water_texture = self.egui_renderer.context().load_texture(
            "item_panel_water_debug",
            water_image,
            egui::TextureOptions::NEAREST,
        );
        self.item_panel_water_icon = Some(water_texture);
        Ok(())
    }

    fn calculate_sun_position(time_of_day: f32, latitude: f32, season: f32) -> (f32, f32) {
        environment::calculate_sun_position(time_of_day, latitude, season)
    }

    fn frame_rate_adjusted_vsm_temporal_alpha(alpha_60fps: f32, delta_seconds: f32) -> f32 {
        let alpha_60fps = alpha_60fps.clamp(0.0, 1.0);
        if alpha_60fps <= 0.0 || alpha_60fps >= 1.0 {
            return alpha_60fps;
        }

        let frame_scale = delta_seconds.max(0.0) * 60.0;
        1.0 - (1.0 - alpha_60fps).powf(frame_scale)
    }

    fn request_vsm_history_reset(&mut self) {
        self.vsm_history_reset_pending = true;
    }

    fn execute_edit_plan(&mut self, plan: WorldEditPlan) -> Result<()> {
        let affects_shadow_history = !plan.voxel_edits.is_empty() || !plan.build_edits.is_empty();
        world_ops::execute_edit_plan_on_backend(self, plan)?;
        if affects_shadow_history {
            self.request_vsm_history_reset();
        }
        Ok(())
    }

    fn gui_wants_keyboard_input(&self) -> bool {
        self.window_state.is_cursor_visible()
            && self.egui_renderer.context().egui_wants_keyboard_input()
    }

    pub fn on_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        let is_keyboard_event = matches!(&event, WindowEvent::KeyboardInput { .. });
        let gui_wanted_keyboard_before_event = self.gui_wants_keyboard_input();

        if let WindowEvent::KeyboardInput { event, .. } = &event {
            if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyE {
                self.config_panel_visible = !self.config_panel_visible;
                self.sync_cursor_with_panels();
                return;
            }
        }
        if let WindowEvent::CursorMoved { position, .. } = &event {
            self.cursor_position_physical = Some(Vec2::new(position.x as f32, position.y as f32));
        }
        if let WindowEvent::ModifiersChanged(modifiers) = &event {
            self.modifiers = modifiers.state();
        }

        // Tab is a game shortcut in normal play, but egui also uses it for focus traversal.
        // Handle it before forwarding to egui so pressing Tab does not leave a focused UI widget
        // that captures later keyboard shortcuts until the player clicks the world again.
        // Ignore keyboard-repeat presses so holding Tab changes the brush only once.
        if let WindowEvent::KeyboardInput { event, .. } = &event {
            if self.keyboard_tool_shortcuts_available()
                && self.is_staff_selected()
                && event.physical_key == KeyCode::Tab
            {
                if event.state == ElementState::Pressed && !event.repeat {
                    self.cycle_flora_paint_selection();
                }
                return;
            }
        }

        // Feed GUI-visible events to egui first. Keep keyboard movement available while panels are
        // merely open, but reserve keyboard input for egui while a text/numeric edit has focus.
        if self.window_state.is_cursor_visible() {
            let consumed = self
                .egui_renderer
                .on_window_event(&self.window_state.window(), &event)
                .consumed;
            let gui_wants_keyboard =
                gui_wanted_keyboard_before_event || self.gui_wants_keyboard_input();

            if is_keyboard_event && gui_wants_keyboard {
                self.reset_camera_movement_input();
                return;
            }

            if consumed && !is_keyboard_event {
                if let WindowEvent::CursorMoved { position, .. } = &event {
                    self.sync_orbit_middle_mouse_drag_position(Vec2::new(
                        position.x as f32,
                        position.y as f32,
                    ));
                }
                if let WindowEvent::MouseInput { state, button, .. } = &event {
                    if *state == ElementState::Released {
                        self.set_tool_mouse_button_state(*button, *state);
                        self.set_orbit_middle_mouse_drag_state(*button, *state);
                        self.refresh_terrain_edit_hold_from_mouse_buttons();
                    }
                }
                return;
            }
        }

        if let WindowEvent::KeyboardInput { event, .. } = &event {
            if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyQ {
                self.on_terminate(event_loop);
                return;
            }

            if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyP {
                self.frame_timing_panel_visible = !self.frame_timing_panel_visible;
                return;
            }

            if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyM {
                self.toggle_audio_output_mute();
                return;
            }
        }

        match event {
            // close the loop, therefore the window, when close button is clicked
            WindowEvent::CloseRequested => {
                self.on_terminate(event_loop);
            }

            // never happened and never tested, take caution
            WindowEvent::ScaleFactorChanged {
                scale_factor: _scale_factor,
                inner_size_writer: _inner_size_writer,
            } => {
                self.is_resize_pending = true;
            }

            // resize the window
            WindowEvent::Resized(_) => {
                self.is_resize_pending = true;
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyF {
                    self.window_state.toggle_fullscreen();
                }

                if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyG {
                    self.toggle_camera_control_mode();
                    return;
                }

                if self.keyboard_tool_shortcuts_available() && event.state == ElementState::Pressed
                {
                    let target_slot = match event.physical_key {
                        PhysicalKey::Code(KeyCode::Digit1) => Some(0),
                        PhysicalKey::Code(KeyCode::Digit2) => Some(1),
                        PhysicalKey::Code(KeyCode::Digit3) => Some(2),
                        PhysicalKey::Code(KeyCode::Digit4) => Some(3),
                        PhysicalKey::Code(KeyCode::Digit5) => Some(4),
                        _ => None,
                    };

                    if let Some(slot_idx) = target_slot {
                        self.select_item_panel_slot(slot_idx);
                    }

                    let target_placeable_slot = match event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyZ) => Some(TREE_PLACEABLE_SLOT_INDEX),
                        PhysicalKey::Code(KeyCode::KeyX) => Some(SPRINKLER_PLACEABLE_SLOT_INDEX),
                        _ => None,
                    };
                    if let Some(slot_idx) = target_placeable_slot {
                        self.select_placeable_panel_slot(slot_idx);
                        self.select_item_panel_slot(TREE_SLOT_INDEX);
                    }
                }

                if self.is_free_look_camera_mode() {
                    self.tracer.handle_keyboard(&event);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_orbit_middle_mouse_drag(Vec2::new(
                    position.x as f32,
                    position.y as f32,
                ));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.set_tool_mouse_button_state(button, state);
                self.set_orbit_middle_mouse_drag_state(button, state);

                if self.terrain_edit_pointer_available()
                    && (button == MouseButton::Left || button == MouseButton::Right)
                {
                    match state {
                        ElementState::Pressed => {
                            self.player_tools.shovel_dig_held = false;
                            let now = Instant::now();
                            if self.is_shovel_selected() && button == MouseButton::Left {
                                self.player_tools.shovel_dig_held = true;
                                self.try_shovel_dig(now);
                            } else if self.is_shovel_selected() && button == MouseButton::Right {
                                self.player_tools.shovel_dig_held = true;
                                self.try_shovel_place(now);
                            } else if self.is_smooth_selected() && button == MouseButton::Left {
                                self.player_tools.shovel_dig_held = true;
                                self.try_terrain_smooth(now);
                            } else if self.is_staff_selected() && button == MouseButton::Left {
                                self.player_tools.shovel_dig_held = true;
                                self.try_staff_regenerate(now);
                            } else if self.is_staff_selected() && button == MouseButton::Right {
                                self.player_tools.shovel_dig_held = true;
                                self.try_staff_remove_flora(now);
                            } else if self.is_hoe_selected() && button == MouseButton::Left {
                                self.player_tools.shovel_dig_held = true;
                                self.try_hoe_trim(now);
                            } else if self.is_place_tool_selected() && button == MouseButton::Left {
                                self.stop_terrain_edit_loop_sound();
                                self.try_placeable_placement();
                            }
                        }
                        ElementState::Released => {
                            self.refresh_terrain_edit_hold_from_mouse_buttons();
                        }
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(delta);
            }

            // redraw the window
            WindowEvent::RedrawRequested => {
                // when the windiw is resized, redraw is called afterwards, so when the window is minimized, return
                if self.window_state.is_minimized() {
                    return;
                }

                let frame_start = Instant::now();
                let frame_perf_enabled = self.perf_logging;
                let frame_timing_enabled = frame_perf_enabled || self.frame_timing_panel_visible;
                let mut cpu_timings = FrameCpuTimings::new(frame_timing_enabled);

                // resize the window if needed
                if self.is_resize_pending {
                    self.on_resize();
                }

                self.window_state.maintain_cursor_grab();

                self.time_info.update(self.perf_logging);
                cpu_timings.time(FrameCpuScope::ContreePoll, || {
                    self.contree_builder.poll_cpu_chunk_cache_jobs(
                        self.tracer.camera_position(),
                        VOXEL_DIM_PER_CHUNK,
                    );
                });
                cpu_timings.time(FrameCpuScope::TerrainSource, || {
                    self.process_terrain_sdf_source_updates();
                });

                if self.loading_state.is_some() {
                    self.process_loading_step();
                    self.render_loading_frame();
                    return;
                }

                if self.player_tools.shovel_dig_held {
                    let now = Instant::now();
                    if self.is_shovel_selected() && self.player_tools.left_mouse_held {
                        self.try_shovel_dig(now);
                    } else if self.is_shovel_selected() && self.player_tools.right_mouse_held {
                        self.try_shovel_place(now);
                    } else if self.is_smooth_selected() && self.player_tools.left_mouse_held {
                        self.try_terrain_smooth(now);
                    } else if self.is_staff_selected() && self.player_tools.left_mouse_held {
                        self.try_staff_regenerate(now);
                    } else if self.is_staff_selected() && self.player_tools.right_mouse_held {
                        self.try_staff_remove_flora(now);
                    } else if self.is_hoe_selected() && self.player_tools.left_mouse_held {
                        self.try_hoe_trim(now);
                    } else {
                        self.stop_terrain_edit_loop_sound();
                    }
                }
                let frame_delta_time = self.time_info.delta_time();
                self.terrain_moisture.update(frame_delta_time);
                self.update_sprinkler_moisture(frame_delta_time);
                let time_since_start = self.time_info.time_since_start();
                let world_tick_seconds = crate::game_time::clamp_world_tick_seconds(
                    self.gui_adjustables.world_tick_seconds.value,
                );
                self.flora_tick_accumulator += frame_delta_time / world_tick_seconds;
                let mut world_tick_steps = 0u32;
                while self.flora_tick_accumulator >= 1.0 {
                    self.flora_tick = self.flora_tick.wrapping_add(1);
                    self.flora_tick_accumulator -= 1.0;
                    world_tick_steps += 1;
                }
                if world_tick_steps > 0 {
                    self.update_growing_flora_chunk();
                }
                let active_wind_sources = GuiAdjustables::active_wind_sources(&self.wind_sources);
                if let Err(err) = self.tree_audio_manager.update(
                    time_since_start,
                    &active_wind_sources,
                    self.gui_adjustables.wind_audio_attack_decay.value,
                    self.gui_adjustables.wind_audio_release_decay.value,
                ) {
                    log::warn!("Failed to update tree audio sources: {}", err);
                }
                if let Err(err) = self.spatial_sound_manager.pump_audio() {
                    log::warn!("Failed to pump audio: {}", err);
                }
                self.update_audio_ray_tracing();
                self.update_spatial_audio_backends();

                if self.is_free_look_camera_mode() && !self.window_state.is_cursor_visible() {
                    // grab the value and immediately reset the accumulator
                    let mouse_delta = self.accumulated_mouse_delta;
                    self.accumulated_mouse_delta = Vec2::ZERO;

                    let alpha = 0.4; // mouse smoothing factor: 0 = no smoothing, 1 = infinite smoothing
                    self.smoothed_mouse_delta =
                        self.smoothed_mouse_delta * alpha + mouse_delta * (1.0 - alpha);

                    self.tracer.handle_mouse(self.smoothed_mouse_delta);
                }

                let mut tree_desc_changed = false;
                let time_of_day_before_gui = self.gui_adjustables.time_of_day.value;
                let vsm_blur_radius_before_gui = self.gui_adjustables.vsm_blur_radius.value;
                let item_panel_shovel_icon = self.item_panel_shovel_icon.clone();
                let item_panel_smooth_icon = self.item_panel_smooth_icon.clone();
                let item_panel_staff_icon = self.item_panel_staff_icon.clone();
                let item_panel_hoe_icon = self.item_panel_hoe_icon.clone();
                let item_panel_tree_icon = self.item_panel_tree_icon.clone();
                let item_panel_water_icon = self.item_panel_water_icon.clone();
                let selected_item_panel_slot = self.player_tools.selected_item_panel_slot;
                let selected_placeable_panel_slot = self.player_tools.selected_placeable_panel_slot;
                let voxel_palette_entries: Vec<VoxelPaletteEntry> = BACKPACK_VOXEL_TYPES
                    .iter()
                    .copied()
                    .map(|voxel_type| VoxelPaletteEntry {
                        voxel_type,
                        label: voxel_type.label(),
                        count: self.voxel_count(voxel_type),
                        color: voxel_type.color(),
                        selected: false,
                    })
                    .collect();
                let water_status_text = self
                    .water_sim
                    .status_text(self.water_particle_handoff_main_thread_ms);
                let grow_brush_hint = if self.is_staff_selected() {
                    format!(
                        "Grow brush: {} (Tab to cycle)",
                        self.current_flora_paint_selection_label()
                    )
                } else {
                    format!("Grow brush: {}", self.current_flora_paint_selection_label())
                };
                let placeable_hint = format!(
                    "Place: {} (Z/X to choose) · sprinklers {} · wet patches {}",
                    self.current_placeable_label(),
                    self.sprinkler_records.len(),
                    self.terrain_moisture.patch_count()
                );
                let status_bar_text = format!(
                    "{}\n{}\n{}",
                    water_status_text, grow_brush_hint, placeable_hint
                );
                let growing_flora_chunk_count = self.growing_flora_chunks.len();
                let mut camera_snapshot_to_apply = None;
                let mut clicked_item_panel_slot = None;
                let mut clicked_placeable_panel_slot = None;

                let current_camera_pose = self.tracer.camera_pose();
                let terrain_edit_preview_center = self.terrain_edit_hover_center();
                let terrain_edit_preview_shape = self.terrain_edit_preview_shape();
                let terrain_edit_preview_color = self.terrain_edit_preview_color();
                let egui_start = Instant::now();
                self.egui_renderer
                    .update(&self.window_state.window(), |ctx| {
                        let mut style = (*ctx.global_style()).clone();
                        apply_gui_style(&mut style);
                        ctx.set_global_style(style);

                        let mut config_panel_open = self.config_panel_visible;
                        if config_panel_open {
                            let config_frame = egui::containers::Frame {
                                fill: PANEL_BG,
                                inner_margin: egui::Margin::symmetric(20, 16),
                                corner_radius: egui::CornerRadius::same(0),
                                shadow: egui::epaint::Shadow {
                                    offset: [6, 6],
                                    blur: 0,
                                    spread: 0,
                                    color: SHADOW_COLOR,
                                },
                                stroke: egui::Stroke::new(3.0, SAGE_ACCENT),
                                ..Default::default()
                            };

                            let content_rect = ctx.content_rect();
                            let panel_pos = egui::pos2(content_rect.left(), content_rect.top());
                            let panel_size = egui::Vec2::new(
                                content_rect.width() * 0.24,
                                content_rect.height() * 0.6,
                            );

                            egui::Window::new("Debug Panel")
                                .id(egui::Id::new("config_panel"))
                                .open(&mut config_panel_open)
                                .frame(config_frame)
                                .resizable(true)
                                .movable(true)
                                .default_pos(panel_pos)
                                .default_size(panel_size)
                                .show(ctx, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.heading(
                                            RichText::new("Debug Panel")
                                                .size(18.0)
                                                .color(GOLD_ACCENT),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .add(egui::Button::new("Save").small())
                                                    .clicked()
                                                {
                                                    match self
                                                        .gui_adjustables
                                                        .save_to_config_with_wind_sources(
                                                            &self.wind_sources,
                                                        ) {
                                                        Ok(_) => {
                                                            log::info!("Config saved successfully");
                                                        }
                                                        Err(e) => {
                                                            log::error!(
                                                                "Failed to save config: {}",
                                                                e
                                                            );
                                                        }
                                                    }
                                                }
                                            },
                                        );
                                    });

                                    ui.add_space(4.0);
                                    ui.separator();
                                    ui.add_space(4.0);

                                    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(
                                        ui,
                                        |ui| {
                                            crate::app::render_gui_from_config(
                                                ui,
                                                &self.gui_config,
                                                &mut self.gui_adjustables,
                                                &mut self.wind_sources,
                                            );

                                            ui.add_space(8.0);
                                            ui.separator();
                                            ui.add_space(8.0);
                                            camera_snapshot_to_apply = draw_camera_snapshots_ui(
                                                ui,
                                                &mut self.camera_snapshots,
                                                &mut self.camera_snapshot_draft_name,
                                                &mut self.camera_snapshot_draft_description,
                                                &mut self.camera_snapshot_status,
                                                current_camera_pose,
                                                self.is_fly_mode,
                                            );

                                            ui.add_space(8.0);
                                            ui.separator();
                                            ui.add_space(8.0);
                                            ui.collapsing("Test Tree", |ui| {
                                                tree_desc_changed |=
                                                    self.debug_tree_desc.edit_by_gui(ui);
                                            });

                                            ui.add_space(8.0);
                                            ui.separator();
                                            ui.add_space(8.0);
                                            ui.heading(
                                                RichText::new("Flora Growth")
                                                    .size(16.0)
                                                    .color(GOLD_ACCENT),
                                            );
                                            ui.label(format!(
                                                "Updating chunks: {}",
                                                growing_flora_chunk_count
                                            ));

                                            ui.add_space(8.0);
                                            ui.separator();
                                            ui.add_space(8.0);
                                            ui.heading(
                                                RichText::new("Audio Ray Tracing")
                                                    .size(16.0)
                                                    .color(GOLD_ACCENT),
                                            );
                                            ui.checkbox(
                                                &mut self
                                                    .gui_adjustables
                                                    .audio_ray_tracing_enabled
                                                    .value,
                                                "Enable Audio Ray Tracing",
                                            );
                                        },
                                    );
                                });
                        }
                        self.config_panel_visible = config_panel_open;

                        let item_panel_slots = [
                            ItemPanelSlot {
                                index: STAFF_SLOT_INDEX,
                                label: "Grow",
                                key_hint: "1",
                                icon: item_panel_staff_icon.as_ref(),
                                accent: STAFF_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: SHOVEL_SLOT_INDEX,
                                label: "Dig",
                                key_hint: "2",
                                icon: item_panel_shovel_icon.as_ref(),
                                accent: SHOVEL_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: SMOOTH_SLOT_INDEX,
                                label: "Smooth",
                                key_hint: "3",
                                icon: item_panel_smooth_icon.as_ref(),
                                accent: SMOOTH_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: HOE_SLOT_INDEX,
                                label: "Trim",
                                key_hint: "4",
                                icon: item_panel_hoe_icon.as_ref(),
                                accent: HOE_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: TREE_SLOT_INDEX,
                                label: "Place",
                                key_hint: "5",
                                icon: item_panel_tree_icon.as_ref(),
                                accent: TREE_TOOL_ACCENT,
                                enabled: true,
                            },
                        ];
                        let item_panel_response = draw_item_panel(
                            ctx,
                            &item_panel_slots,
                            selected_item_panel_slot,
                            self.window_state.is_cursor_visible(),
                        );
                        clicked_item_panel_slot = item_panel_response.clicked_slot;

                        let placeable_panel_slots = [
                            PlaceablePanelSlot {
                                index: TREE_PLACEABLE_SLOT_INDEX,
                                label: "Tree",
                                key_hint: "Z",
                                icon: item_panel_tree_icon.as_ref(),
                                accent: TREE_TOOL_ACCENT,
                                enabled: true,
                            },
                            PlaceablePanelSlot {
                                index: SPRINKLER_PLACEABLE_SLOT_INDEX,
                                label: "Spray",
                                key_hint: "X",
                                icon: item_panel_water_icon.as_ref(),
                                accent: WATER_TOOL_ACCENT,
                                enabled: true,
                            },
                        ];
                        let placeable_panel_response = draw_placeable_panel(
                            ctx,
                            &placeable_panel_slots,
                            selected_placeable_panel_slot,
                            selected_item_panel_slot == TREE_SLOT_INDEX,
                            self.window_state.is_cursor_visible(),
                        );
                        clicked_placeable_panel_slot = placeable_panel_response.clicked_slot;

                        let voxel_palette_response =
                            draw_voxel_palette(ctx, &voxel_palette_entries, false);
                        self.player_tools.backpack_summary_panel_screen_pos =
                            voxel_palette_response
                                .panel_center
                                .map(|center| Vec2::new(center.x, center.y));

                        egui::Area::new("status_bar_panel".into())
                            .anchor(egui::Align2::LEFT_BOTTOM, egui::Vec2::new(16.0, -16.0))
                            .show(ctx, |ui| {
                                let status_bar_frame = egui::containers::Frame {
                                    fill: PANEL_DARK,
                                    inner_margin: egui::Margin::symmetric(10, 8),
                                    corner_radius: egui::CornerRadius::same(0),
                                    shadow: egui::epaint::Shadow {
                                        offset: [4, 4],
                                        blur: 0,
                                        spread: 0,
                                        color: SHADOW_COLOR,
                                    },
                                    stroke: egui::Stroke::new(2.0, FLOWER_ACCENT),
                                    ..Default::default()
                                };

                                status_bar_frame.show(ui, |ui| {
                                    ui.set_max_width(420.0);
                                    ui.label(
                                        RichText::new("Status Bar")
                                            .color(GOLD_ACCENT)
                                            .monospace()
                                            .size(12.0),
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new(status_bar_text.as_str())
                                            .color(SAGE_ACCENT)
                                            .monospace()
                                            .size(11.0),
                                    );
                                });
                            });

                        if self.frame_timing_panel_visible {
                            draw_frame_timing_panel(
                                ctx,
                                self.frame_timing_snapshot,
                                self.gpu_profiler_latest_results.as_ref(),
                                self.perf_logging,
                            );
                        }

                        // FPS counter in bottom right
                        egui::Area::new("fps_counter".into())
                            .anchor(egui::Align2::RIGHT_BOTTOM, egui::Vec2::new(-16.0, -16.0))
                            .show(ctx, |ui| {
                                let fps_frame = egui::containers::Frame {
                                    fill: PANEL_DARK,
                                    inner_margin: egui::Margin::symmetric(10, 6),
                                    corner_radius: egui::CornerRadius::same(0),
                                    shadow: egui::epaint::Shadow {
                                        offset: [4, 4],
                                        blur: 0,
                                        spread: 0,
                                        color: SHADOW_COLOR,
                                    },
                                    stroke: egui::Stroke::new(2.0, FLOWER_ACCENT),
                                    ..Default::default()
                                };

                                fps_frame.show(ui, |ui| {
                                    ui.allocate_ui_with_layout(
                                        egui::Vec2::new(110.0, 24.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new("FPS")
                                                    .color(GOLD_ACCENT)
                                                    .monospace()
                                                    .size(12.0),
                                            );
                                            ui.add_space(6.0);
                                            ui.label(
                                                RichText::new(format!(
                                                    "{:.1}",
                                                    self.time_info.display_fps()
                                                ))
                                                .color(SAGE_ACCENT)
                                                .monospace()
                                                .strong(),
                                            );
                                        },
                                    );
                                });
                            });
                    });
                let egui_ms = egui_start.elapsed().as_secs_f32() * 1000.0;
                self.sync_cursor_with_panels();
                if let Some(slot_idx) = clicked_item_panel_slot {
                    self.select_item_panel_slot(slot_idx);
                }
                if let Some(slot_idx) = clicked_placeable_panel_slot {
                    self.select_placeable_panel_slot(slot_idx);
                    self.select_item_panel_slot(TREE_SLOT_INDEX);
                }
                if self.gui_wants_keyboard_input() {
                    self.reset_camera_movement_input();
                }
                if let Some(snapshot) = camera_snapshot_to_apply {
                    self.apply_camera_snapshot(&snapshot);
                }

                self.apply_effective_master_volume_gain("Failed to apply master volume");
                if let Err(err) = self
                    .tree_audio_manager
                    .set_wind_volume_db(self.gui_adjustables.tree_wind_volume_db.value)
                {
                    log::error!("Failed to apply tree wind volume: {}", err);
                }
                self.tree_audio_manager.set_wind_response_curve(
                    Self::tree_audio_wind_response_curve(&self.gui_adjustables),
                );
                self.tree_audio_manager
                    .set_rustle_params(Self::tree_rustle_params(&self.gui_adjustables));

                if TreeBench::run_next(self) {
                    self.on_terminate(event_loop);
                    return;
                }
                if AuthoredFloraBench::run_next(self) {
                    self.on_terminate(event_loop);
                    return;
                }

                cpu_timings.time(FrameCpuScope::TerrainSource, || {
                    self.process_terrain_sdf_source_updates();
                });
                cpu_timings.time(FrameCpuScope::DeferredRebuild, || {
                    self.process_deferred_chunk_rebuild();
                });
                cpu_timings.time(FrameCpuScope::WaterCache, || {
                    self.process_deferred_water_terrain_cache_rebuild();
                });
                cpu_timings.time(FrameCpuScope::ColliderQueue, || {
                    self.process_deferred_terrain_sdf_collider_rebuild();
                });
                cpu_timings.time(FrameCpuScope::WaterEditSoak, || {
                    self.process_water_edit_soak();
                });

                let mut sun_update_ticks = 0;
                if self.gui_adjustables.auto_daynight_cycle.value && world_tick_steps > 0 {
                    self.sun_position_update_tick_accumulator += world_tick_steps;
                    while self.sun_position_update_tick_accumulator
                        >= SUN_POSITION_UPDATE_INTERVAL_TICKS
                    {
                        self.sun_position_update_tick_accumulator -=
                            SUN_POSITION_UPDATE_INTERVAL_TICKS;
                        sun_update_ticks += SUN_POSITION_UPDATE_INTERVAL_TICKS;
                    }
                }

                let time_of_day_changed_by_gui =
                    self.gui_adjustables.time_of_day.value != time_of_day_before_gui;
                let vsm_blur_radius_changed_by_gui =
                    self.gui_adjustables.vsm_blur_radius.value != vsm_blur_radius_before_gui;
                if time_of_day_changed_by_gui || vsm_blur_radius_changed_by_gui {
                    self.request_vsm_history_reset();
                }

                // update sun position if auto day/night cycle is enabled
                let sun_position_updated = sun_update_ticks > 0;
                if sun_position_updated {
                    // update time of day based on delta time and day cycle speed
                    // day_cycle_minutes is the real-world minutes for a full day cycle
                    // convert to time progression per second: 1.0 / (day_cycle_minutes * 60.0)
                    let time_speed = 1.0 / (self.gui_adjustables.day_cycle_minutes.value * 60.0);
                    self.gui_adjustables.time_of_day.value +=
                        sun_update_ticks as f32 * world_tick_seconds * time_speed;

                    // keep time_of_day in 0.0 to 1.0 range (wrap around)
                    self.gui_adjustables.time_of_day.value %= 1.0;
                }

                if self.render_flags.enable_particles {
                    if self.water_terrain_initialized {
                        let water_handoff_start = Instant::now();
                        self.update_water_sim(frame_delta_time, world_tick_seconds);
                        let elapsed_ms = water_handoff_start.elapsed().as_secs_f32() * 1000.0;
                        self.water_particle_handoff_main_thread_ms = Some(elapsed_ms);
                        cpu_timings.add_ms(FrameCpuScope::WaterHandoff, elapsed_ms);
                    } else {
                        self.water_particle_handoff_main_thread_ms = None;
                    }
                    cpu_timings.time(FrameCpuScope::Particles, || {
                        self.update_particle_simulation(frame_delta_time);
                    });
                }

                let gpu_record_start = Instant::now();
                let frame = match self.frame_manager.begin_frame(&mut self.swapchain) {
                    Ok(frame) => frame,
                    Err(SwapchainFrameError::OutOfDate) => {
                        self.is_resize_pending = true;
                        return;
                    }
                    Err(error) => panic!("Error while acquiring next image. Cause: {}", error),
                };
                let frame_slot = frame.frame_slot();
                self.collect_gpu_profiler_frame(frame_slot);
                let device = self.vulkan_ctx.device();
                let cmdbuf = frame.command_buffer();
                let image_idx = frame.image_index();

                cmdbuf.begin(false);
                if let Some(profiler) = self.gpu_profiler.as_mut() {
                    profiler.begin_frame(frame_slot, cmdbuf);
                }
                let frame_gpu_scope = self.gpu_profiler.as_mut().and_then(|profiler| {
                    profiler.begin_scope(
                        frame_slot,
                        cmdbuf,
                        "frame.render",
                        PipelineStage::ALL_COMMANDS,
                    )
                });

                let (sun_altitude, sun_azimuth) = Self::calculate_sun_position(
                    self.gui_adjustables.time_of_day.value,
                    self.gui_adjustables.latitude.value,
                    self.gui_adjustables.season.value,
                );
                let update_shadow_map = self.render_flags.enable_shadows;
                let wind_gui_params = Self::wind_gui_params(&self.wind_sources);
                let cloud_gui_params = CloudGuiParams {
                    // Disabled for now; infrastructure kept for easy re-enable.
                    enabled: false,
                    coverage: self.gui_adjustables.cloud_coverage.value,
                    density: self.gui_adjustables.cloud_density.value,
                    bottom_height: self.gui_adjustables.cloud_bottom_height.value,
                    top_height: self.gui_adjustables.cloud_top_height.value,
                    shape_scale: self.gui_adjustables.cloud_shape_scale.value,
                    detail_scale: self.gui_adjustables.cloud_detail_scale.value,
                    detail_strength: self.gui_adjustables.cloud_detail_strength.value,
                    wind_speed: self.gui_adjustables.cloud_wind_speed.value,
                    primary_steps: self.gui_adjustables.cloud_primary_steps.value,
                    light_steps: self.gui_adjustables.cloud_light_steps.value,
                    temporal_alpha: self.gui_adjustables.cloud_temporal_alpha.value,
                    absorption: self.gui_adjustables.cloud_absorption.value,
                    phase_eccentricity: self.gui_adjustables.cloud_phase_eccentricity.value,
                    silver_intensity: self.gui_adjustables.cloud_silver_intensity.value,
                    max_distance: self.gui_adjustables.cloud_max_distance.value,
                    // Disabled for now; restore original expression to re-enable.
                    shadows_enabled: false,
                    shadow_debug_overlay: self.gui_adjustables.cloud_shadow_debug_overlay.value,
                    shadow_strength: self.gui_adjustables.cloud_shadow_strength.value,
                    shadow_min_transmittance: self
                        .gui_adjustables
                        .cloud_shadow_min_transmittance
                        .value,
                    shadow_steps: self.gui_adjustables.cloud_shadow_steps.value,
                };

                let (terrain_moisture_patch_count, terrain_moisture_patches) =
                    self.terrain_moisture.shader_patches();

                self.tracer
                    .update_buffers(
                        &self.time_info,
                        self.gui_adjustables.debug_float.value,
                        self.gui_adjustables.debug_bool.value,
                        self.gui_adjustables.debug_uint.value,
                        Vec3::new(
                            self.gui_adjustables.flora_instance_hue_offset.value,
                            self.gui_adjustables.flora_instance_saturation_offset.value,
                            self.gui_adjustables.flora_instance_value_offset.value,
                        ),
                        Vec3::new(
                            self.gui_adjustables.flora_voxel_hue_offset.value,
                            self.gui_adjustables.flora_voxel_saturation_offset.value,
                            self.gui_adjustables.flora_voxel_value_offset.value,
                        ),
                        Vec3::new(
                            self.gui_adjustables.grass_bottom_dark_color.value.r() as f32 / 255.0,
                            self.gui_adjustables.grass_bottom_dark_color.value.g() as f32 / 255.0,
                            self.gui_adjustables.grass_bottom_dark_color.value.b() as f32 / 255.0,
                        ),
                        Vec3::new(
                            self.gui_adjustables.grass_bottom_light_color.value.r() as f32 / 255.0,
                            self.gui_adjustables.grass_bottom_light_color.value.g() as f32 / 255.0,
                            self.gui_adjustables.grass_bottom_light_color.value.b() as f32 / 255.0,
                        ),
                        Vec3::new(
                            self.gui_adjustables.grass_tip_dark_color.value.r() as f32 / 255.0,
                            self.gui_adjustables.grass_tip_dark_color.value.g() as f32 / 255.0,
                            self.gui_adjustables.grass_tip_dark_color.value.b() as f32 / 255.0,
                        ),
                        Vec3::new(
                            self.gui_adjustables.grass_tip_light_color.value.r() as f32 / 255.0,
                            self.gui_adjustables.grass_tip_light_color.value.g() as f32 / 255.0,
                            self.gui_adjustables.grass_tip_light_color.value.b() as f32 / 255.0,
                        ),
                        world_tick_seconds,
                        update_shadow_map,
                        self.gui_adjustables.lens_flare_intensity.value,
                        self.gui_adjustables.lens_flare_sun_pixel_scale.value,
                        GlassGuiParams {
                            tint: Vec3::new(
                                self.gui_adjustables.glass_tint.value.r() as f32 / 255.0,
                                self.gui_adjustables.glass_tint.value.g() as f32 / 255.0,
                                self.gui_adjustables.glass_tint.value.b() as f32 / 255.0,
                            ),
                            reflection_strength: self
                                .gui_adjustables
                                .glass_reflection_strength
                                .value,
                            ssr_strength: self.gui_adjustables.glass_ssr_strength.value,
                            ssr_steps: self.gui_adjustables.glass_ssr_steps.value,
                            per_voxel_reflection: self
                                .gui_adjustables
                                .glass_per_voxel_reflection
                                .value,
                            ssr_min_hit_thickness_voxels: self
                                .gui_adjustables
                                .glass_ssr_min_hit_thickness_voxels
                                .value,
                            ssr_footprint_pixels: self
                                .gui_adjustables
                                .glass_ssr_footprint_pixels
                                .value,
                            refraction_strength: self
                                .gui_adjustables
                                .glass_refraction_strength
                                .value,
                            alpha: self.gui_adjustables.glass_alpha.value,
                            glint_strength: self.gui_adjustables.glass_glint_strength.value,
                        },
                        self.gui_adjustables.wind_directional_bias_fraction.value,
                        self.gui_adjustables.wind_turbulence_fraction.value,
                        self.gui_adjustables.grass_vibration_amplitude_voxels.value,
                        self.gui_adjustables.grass_vibration_primary_speed.value,
                        self.gui_adjustables.grass_vibration_secondary_speed.value,
                        self.gui_adjustables.grass_natural_bend_min_voxels.value,
                        self.gui_adjustables.grass_natural_bend_max_voxels.value,
                        self.gui_adjustables.flora_bend_height_power.value,
                        self.gui_adjustables.leaf_paddle_amplitude_voxels.value,
                        self.gui_adjustables.leaf_paddle_primary_speed.value,
                        self.gui_adjustables.leaf_paddle_secondary_speed.value,
                        self.gui_adjustables
                            .leaf_paddle_amplitude_wind_start_strength
                            .value,
                        self.gui_adjustables
                            .leaf_paddle_amplitude_wind_full_strength
                            .value,
                        self.gui_adjustables
                            .leaf_paddle_amplitude_wind_knee_bias
                            .value,
                        self.gui_adjustables
                            .leaf_paddle_frequency_wind_start_strength
                            .value,
                        self.gui_adjustables
                            .leaf_paddle_frequency_wind_full_strength
                            .value,
                        self.gui_adjustables
                            .leaf_paddle_frequency_wind_knee_bias
                            .value,
                        self.gui_adjustables
                            .leaf_paddle_frequency_min_multiplier
                            .value,
                        self.gui_adjustables
                            .leaf_paddle_frequency_max_multiplier
                            .value,
                        self.gui_adjustables.leaf_shadow_fragment_opacity.value,
                        self.gui_adjustables.leaf_shadow_strength.value,
                        self.gui_adjustables.leaf_shadow_min_transmittance.value,
                        self.gui_adjustables.leaf_shadow_filter_radius_texels.value,
                        wind_gui_params,
                        cloud_gui_params,
                        self.flora_tick,
                        FLORA_SPROUT_DELAY_TICKS,
                        FLORA_FULL_GROWTH_TICKS,
                        get_sun_dir(sun_altitude.asin().to_degrees(), sun_azimuth * 360.0),
                        self.gui_adjustables.sun_size.value,
                        Vec3::new(
                            self.gui_adjustables.sun_color.value.r() as f32 / 255.0,
                            self.gui_adjustables.sun_color.value.g() as f32 / 255.0,
                            self.gui_adjustables.sun_color.value.b() as f32 / 255.0,
                        ),
                        self.gui_adjustables.sun_luminance.value,
                        self.gui_adjustables.sun_display_luminance.value,
                        sun_altitude,
                        sun_azimuth,
                        Vec3::new(
                            self.gui_adjustables.ambient_light.value.r() as f32 / 255.0,
                            self.gui_adjustables.ambient_light.value.g() as f32 / 255.0,
                            self.gui_adjustables.ambient_light.value.b() as f32 / 255.0,
                        ),
                        self.gui_adjustables.temporal_position_phi.value,
                        self.gui_adjustables.temporal_alpha.value,
                        self.gui_adjustables.phi_c.value,
                        self.gui_adjustables.phi_n.value,
                        self.gui_adjustables.phi_p.value,
                        self.gui_adjustables.min_phi_z.value,
                        self.gui_adjustables.max_phi_z.value,
                        self.gui_adjustables.phi_z_stable_sample_count.value,
                        self.gui_adjustables.is_changing_lum_phi.value,
                        self.gui_adjustables.is_spatial_denoising_enabled.value,
                        self.gui_adjustables.a_trous_iteration_count.value,
                        self.gui_adjustables.god_ray_max_depth.value,
                        self.gui_adjustables.god_ray_max_checks.value,
                        self.gui_adjustables.god_ray_weight.value,
                        Vec3::new(
                            self.gui_adjustables.sun_color.value.r() as f32 / 255.0,
                            self.gui_adjustables.sun_color.value.g() as f32 / 255.0,
                            self.gui_adjustables.sun_color.value.b() as f32 / 255.0,
                        ),
                        self.gui_adjustables.starlight_iterations.value,
                        self.gui_adjustables.starlight_formuparam.value,
                        self.gui_adjustables.starlight_volsteps.value,
                        self.gui_adjustables.starlight_stepsize.value,
                        self.gui_adjustables.starlight_zoom.value,
                        self.gui_adjustables.starlight_tile.value,
                        self.gui_adjustables.starlight_speed.value,
                        self.gui_adjustables.starlight_brightness.value,
                        self.gui_adjustables.starlight_darkmatter.value,
                        self.gui_adjustables.starlight_distfading.value,
                        self.gui_adjustables.starlight_saturation.value,
                        Vec3::new(
                            self.gui_adjustables.voxel_dirt_color.value.r() as f32 / 255.0,
                            self.gui_adjustables.voxel_dirt_color.value.g() as f32 / 255.0,
                            self.gui_adjustables.voxel_dirt_color.value.b() as f32 / 255.0,
                        ),
                        Vec3::new(
                            self.gui_adjustables.voxel_sand_color.value.r() as f32 / 255.0,
                            self.gui_adjustables.voxel_sand_color.value.g() as f32 / 255.0,
                            self.gui_adjustables.voxel_sand_color.value.b() as f32 / 255.0,
                        ),
                        Vec3::new(
                            self.gui_adjustables.voxel_cherry_wood_color.value.r() as f32 / 255.0,
                            self.gui_adjustables.voxel_cherry_wood_color.value.g() as f32 / 255.0,
                            self.gui_adjustables.voxel_cherry_wood_color.value.b() as f32 / 255.0,
                        ),
                        Vec3::new(
                            self.gui_adjustables.voxel_oak_wood_color.value.r() as f32 / 255.0,
                            self.gui_adjustables.voxel_oak_wood_color.value.g() as f32 / 255.0,
                            self.gui_adjustables.voxel_oak_wood_color.value.b() as f32 / 255.0,
                        ),
                        Vec3::new(
                            self.gui_adjustables.voxel_rock_color.value.r() as f32 / 255.0,
                            self.gui_adjustables.voxel_rock_color.value.g() as f32 / 255.0,
                            self.gui_adjustables.voxel_rock_color.value.b() as f32 / 255.0,
                        ),
                        self.gui_adjustables.voxel_color_variance.value,
                        &terrain_moisture_patches,
                        terrain_moisture_patch_count,
                        terrain_edit_preview_center,
                        self.player_tools.terrain_edit_radius,
                        terrain_edit_preview_shape,
                        terrain_edit_preview_color,
                        self.gui_adjustables.terrain_edit_preview_alpha.value,
                    )
                    .unwrap();

                let color_to_vec3 = |color: Color32| -> Vec3 {
                    Vec3::new(
                        color.r() as f32 / 255.0,
                        color.g() as f32 / 255.0,
                        color.b() as f32 / 255.0,
                    )
                };

                let mut flora_colors = [(Vec3::ZERO, Vec3::ZERO); species::MAX_FLORA_SPECIES];
                for (slot, desc) in flora_colors.iter_mut().zip(species::species()) {
                    *slot = match desc.key {
                        "tall_grass" | "short_grass" => (
                            color_to_vec3(self.gui_adjustables.grass_bottom_dark_color.value),
                            color_to_vec3(self.gui_adjustables.grass_tip_light_color.value),
                        ),
                        "ember_bloom" => (
                            color_to_vec3(self.gui_adjustables.ember_bloom_bottom_color.value),
                            color_to_vec3(self.gui_adjustables.ember_bloom_tip_color.value),
                        ),
                        _ => {
                            let bottom = Color32::from_rgb(
                                desc.default_bottom_color[0],
                                desc.default_bottom_color[1],
                                desc.default_bottom_color[2],
                            );
                            let tip = Color32::from_rgb(
                                desc.default_tip_color[0],
                                desc.default_tip_color[1],
                                desc.default_tip_color[2],
                            );
                            (color_to_vec3(bottom), color_to_vec3(tip))
                        }
                    };
                }
                let flora_colors = &flora_colors[..species::species_count()];

                let leaf_bottom = color_to_vec3(self.gui_adjustables.leaves_bottom_color.value);
                let leaf_tip = color_to_vec3(self.gui_adjustables.leaves_tip_color.value);
                let reset_vsm_history = self.vsm_history_reset_pending;
                let vsm_temporal_alpha = Self::frame_rate_adjusted_vsm_temporal_alpha(
                    self.gui_adjustables.vsm_temporal_alpha.value,
                    frame_delta_time,
                );
                let leaf_shadow_temporal_alpha = Self::frame_rate_adjusted_vsm_temporal_alpha(
                    self.gui_adjustables.leaf_shadow_temporal_alpha.value,
                    frame_delta_time,
                );
                let tracer_gpu_scope = self.gpu_profiler.as_mut().and_then(|profiler| {
                    profiler.begin_scope(
                        frame_slot,
                        cmdbuf,
                        "tracer.render",
                        PipelineStage::ALL_COMMANDS,
                    )
                });
                let mut gpu_profiler_for_trace = self.gpu_profiler.take();
                self.tracer
                    .record_trace(
                        cmdbuf,
                        self.surface_builder.get_resources(),
                        self.gui_adjustables.lod_distance.value,
                        self.gui_adjustables.flora_draw_distance.value,
                        self.gui_adjustables.grass_render_mode.value,
                        self.time_info.time_since_start(),
                        &flora_colors,
                        leaf_bottom,
                        leaf_tip,
                        &self.render_flags,
                        update_shadow_map,
                        self.gui_adjustables.vsm_blur_radius.value,
                        vsm_temporal_alpha,
                        leaf_shadow_temporal_alpha,
                        reset_vsm_history,
                        gpu_profiler_for_trace.as_mut(),
                        frame_slot,
                    )
                    .unwrap();
                if let Some(scope) = tracer_gpu_scope {
                    if let Some(profiler) = gpu_profiler_for_trace.as_mut() {
                        profiler.end_scope(frame_slot, cmdbuf, scope, PipelineStage::ALL_COMMANDS);
                    }
                }
                self.gpu_profiler = gpu_profiler_for_trace;
                if update_shadow_map {
                    self.vsm_history_reset_pending = false;
                }

                self.swapchain.record_blit(
                    self.tracer.get_screen_output_tex().get_image(),
                    cmdbuf,
                    image_idx,
                );

                let render_area = self.window_state.window_extent();
                let mut screenshot_readback = None;
                if !self.screenshot_taken {
                    if let (Some(render_start_time), Some(path), Some(delay)) = (
                        self.render_start_time,
                        self.screenshot_path.clone(),
                        self.screenshot_delay,
                    ) {
                        let elapsed = render_start_time.elapsed().as_secs_f32();
                        if elapsed >= delay {
                            self.screenshot_taken = true;
                            log::info!("[SCREENSHOT] Capturing after {:.2}s to {}", elapsed, path);
                            match self.prepare_screenshot_readback(path, render_area) {
                                Ok(readback) => screenshot_readback = Some(readback),
                                Err(err) => log::error!("[SCREENSHOT] Failed to prepare: {}", err),
                            }
                        }
                    }
                }

                self.swapchain
                    .record_begin_render_pass_cmdbuf(cmdbuf, image_idx, render_area);

                let egui_gpu_scope = self.gpu_profiler.as_mut().and_then(|profiler| {
                    profiler.begin_scope(
                        frame_slot,
                        cmdbuf,
                        "egui.render",
                        PipelineStage::ALL_COMMANDS,
                    )
                });
                self.egui_renderer
                    .record_command_buffer(device, cmdbuf, render_area);
                if let Some(scope) = egui_gpu_scope {
                    if let Some(profiler) = self.gpu_profiler.as_mut() {
                        profiler.end_scope(frame_slot, cmdbuf, scope, PipelineStage::ALL_COMMANDS);
                    }
                }

                cmdbuf.end_render_pass();

                if let Some(readback) = &screenshot_readback {
                    self.record_screenshot_readback(cmdbuf, image_idx, readback);
                }

                if let Some(scope) = frame_gpu_scope {
                    if let Some(profiler) = self.gpu_profiler.as_mut() {
                        profiler.end_scope(frame_slot, cmdbuf, scope, PipelineStage::ALL_COMMANDS);
                    }
                }

                cmdbuf.end();

                let present_result = self.frame_manager.submit_and_present(
                    &self.vulkan_ctx,
                    &mut self.swapchain,
                    &frame,
                );
                let gpu_ms = gpu_record_start.elapsed().as_secs_f32() * 1000.0;

                match present_result {
                    Ok(is_suboptimal) if is_suboptimal => {
                        self.is_resize_pending = true;
                    }
                    Err(SwapchainFrameError::OutOfDate) => {
                        self.is_resize_pending = true;
                    }
                    Err(error) => panic!("Failed to present queue. Cause: {}", error),
                    _ => {}
                }

                if let Some(readback) = screenshot_readback {
                    frame.wait_until_complete().unwrap();
                    Self::write_screenshot_readback(readback);
                }

                self.tracer.set_footstep_volume_gain(
                    -40.0 + self.gui_adjustables.footstep_volume_db.value,
                );
                self.update_camera_for_current_mode(frame_delta_time);

                let total_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
                let frame_count = self.time_info.total_frame_count();
                let frame_timing_snapshot =
                    cpu_timings.snapshot(frame_count, total_ms, egui_ms, gpu_ms);
                self.frame_timing_snapshot = frame_timing_snapshot;
                if frame_perf_enabled && frame_count.is_multiple_of(30) {
                    log::info!(
                        "[PERF] frame {} total {:.2}ms egui {:.2}ms gpu+present {:.2}ms",
                        frame_count,
                        total_ms,
                        egui_ms,
                        gpu_ms
                    );
                    self.log_gpu_profiler_frame(frame_count);
                }
                if frame_perf_enabled {
                    let queue_work_ms = cpu_timings.queue_work_ms();
                    if frame_count.is_multiple_of(30) || total_ms >= 16.0 || queue_work_ms >= 2.0 {
                        log::info!(
                            "[PERF][FRAME] frame {} total {:.2}ms egui {:.2}ms gpu_present {:.2}ms contree_poll {:.2}ms terrain_source {:.2}ms deferred_rebuild {:.2}ms cache_queue {:.2}ms collider_queue {:.2}ms water_edit_soak {:.2}ms water_handoff {:.2}ms particles {:.2}ms tracked_cpu {:.2}ms untracked_cpu {:.2}ms queues deferred_pending={} deferred_active={} deferred_inflight={} source_pending={} source_active={} collider_pending={} collider_active={} collider_inflight={} cache_pending={} cache_active={} cache_inflight={}",
                            frame_count,
                            total_ms,
                            egui_ms,
                            gpu_ms,
                            frame_timing_snapshot.contree_poll_ms,
                            frame_timing_snapshot.terrain_source_ms,
                            frame_timing_snapshot.deferred_rebuild_ms,
                            frame_timing_snapshot.water_cache_ms,
                            frame_timing_snapshot.collider_queue_ms,
                            frame_timing_snapshot.water_edit_soak_ms,
                            frame_timing_snapshot.water_handoff_ms,
                            frame_timing_snapshot.particles_ms,
                            frame_timing_snapshot.tracked_cpu_ms,
                            frame_timing_snapshot.untracked_cpu_ms,
                            self.deferred_chunk_rebuilds.len(),
                            self.deferred_chunk_rebuilds.active_len(),
                            self.terrain_chunk_rebuild_inflight.is_some(),
                            self.deferred_terrain_sdf_source_refreshes.len(),
                            self.deferred_terrain_sdf_source_refreshes.active_len(),
                            self.deferred_terrain_sdf_collider_rebuilds.len(),
                            self.deferred_terrain_sdf_collider_rebuilds.active_len(),
                            self.terrain_sdf_collider_build_inflight,
                            self.deferred_water_terrain_cache_rebuilds.len(),
                            self.deferred_water_terrain_cache_rebuilds.active_len(),
                            self.water_terrain_cache_rebuild_inflight,
                        );
                    }
                }
                if let Some(render_start_time) = self.render_start_time {
                    let elapsed = render_start_time.elapsed().as_secs_f32();

                    if let Some(auto_exit_delay) = self.auto_exit_delay {
                        if elapsed >= auto_exit_delay {
                            log::info!("[AUTO-EXIT] Exiting after {:.2}s", elapsed);
                            self.on_terminate(event_loop);
                        }
                    }
                }
            }
            _ => (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{App, MouseWheelDollySmoother};
    use petalsonic::config::{AmbisonicsBackend, HrtfBackend};

    #[test]
    fn ambisonics_backend_selects_matching_decoder_backend() {
        assert_eq!(
            App::effective_hrtf_backend(HrtfBackend::Native, false, AmbisonicsBackend::SteamAudio),
            HrtfBackend::Native
        );
        assert_eq!(
            App::effective_hrtf_backend(HrtfBackend::Native, true, AmbisonicsBackend::SteamAudio),
            HrtfBackend::SteamAudio
        );
        assert_eq!(
            App::effective_hrtf_backend(HrtfBackend::SteamAudio, true, AmbisonicsBackend::Native),
            HrtfBackend::Native
        );
    }

    #[test]
    fn mute_state_forces_effective_master_volume_to_silence() {
        let muted_gain_db = App::master_volume_gain_db(0.0, true);
        let normal_min_gain_db = App::master_volume_gain_db(-20.0, false);
        let normal_default_gain_db = App::master_volume_gain_db(0.0, false);
        let normal_max_gain_db = App::master_volume_gain_db(20.0, false);

        assert_eq!(muted_gain_db, super::MUTED_AUDIO_OUTPUT_GAIN_DB);
        assert_eq!(normal_default_gain_db, 0.0);
        assert!(muted_gain_db <= normal_min_gain_db);
        assert!(normal_default_gain_db < normal_max_gain_db);
    }

    #[test]
    fn mouse_wheel_dolly_smoother_interpolates_toward_target() {
        let mut smoother = MouseWheelDollySmoother::default();
        smoother.add_scroll_lines(1.0);

        let first_step = smoother.advance(1.0 / 60.0);
        assert!(first_step > 0.0);
        assert!(first_step < 1.0);

        let mut total_advanced = first_step;
        for _ in 0..120 {
            total_advanced += smoother.advance(1.0 / 60.0);
        }

        assert!((total_advanced - 1.0).abs() <= 0.0001);
        assert_eq!(smoother.current_lines, 0.0);
        assert_eq!(smoother.target_lines, 0.0);
    }

    #[test]
    fn mouse_wheel_dolly_smoother_clamps_pending_scroll_lines() {
        let mut smoother = MouseWheelDollySmoother::default();
        smoother.add_scroll_lines(100.0);
        assert_eq!(
            smoother.target_lines - smoother.current_lines,
            super::MOUSE_WHEEL_DOLLY_MAX_PENDING_LINES
        );

        smoother.advance(1.0 / 60.0);
        smoother.add_scroll_lines(-100.0);
        assert_eq!(
            smoother.target_lines - smoother.current_lines,
            -super::MOUSE_WHEEL_DOLLY_MAX_PENDING_LINES
        );
    }
}
