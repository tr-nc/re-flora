#[allow(unused)]
use crate::util::Timer;

mod authored_flora_bench;
mod boot;
mod camera_snapshot_ui;
mod denoiser_bench;
mod environment_irradiance_capture;
mod environment_lighting_test_scene;
mod frame_timing;
mod hybrid_transparency_test_scene;
mod input;
mod lifecycle;
mod loading;
mod moisture;
mod particles;
mod physics;
mod placeables;
mod planting;
mod player_tools;
mod screenshot;
mod terrain_rebuild;
mod tree_bench;
mod ui_style;
mod vegetation;
mod water;

use self::authored_flora_bench::AuthoredFloraBench;
use self::camera_snapshot_ui::draw_camera_snapshots_ui;
use self::denoiser_bench::{
    DenoiserBench, CAMERA_FORWARD_PER_FRAME_WORLD, CAMERA_STRAFE_PER_FRAME_WORLD,
    CAMERA_YAW_PER_FRAME_RADIANS,
};
use self::frame_timing::{
    draw_frame_timing_panel, FrameCpuScope, FrameCpuTimings, FrameTimingSnapshot,
};
use self::loading::{LoadingPhase, LoadingState};
use self::particles::TreeLeafEmitter;
use self::physics::TerrainPhysics;
use self::placeables::{IrrigationNetwork, PipeDrag, SprinklerEmitter, SprinklerRecord};
use self::player_tools::PlayerToolState;
use self::terrain_rebuild::{ChunkRebuildRequest, TerrainChunkRebuildInFlight};
use self::tree_bench::TreeBench;
use self::vegetation::{TreeRecord, TreeVariationConfig};
use crate::app::camera_snapshots::CameraSnapshotLibrary;
use crate::app::environment;
use crate::app::terrain_edit_bounds::INITIAL_EDITABLE_TERRAIN_BOUNDS;
use crate::app::world_edits::{BuildEdit, VoxelEdit, WorldBuildBackend, WorldEditPlan};
use crate::app::world_ops;
use crate::app::{DebugSettings, GuiAdjustables, WindSourceGuiValues};
use crate::audio::{SpatialSoundManager, TreeAudioManager, TreeRustleParams};
use crate::builder::{
    ContreeBuildJob, ContreeBuilder, PlainBuilder, SceneAccelBuilder, SceneTexUpdateJob,
    SurfaceBuildJob, SurfaceBuilder, VOXEL_FERTILITY_MAX, VOXEL_MOISTURE_MAX, VOXEL_TYPE_DIRT,
};
use crate::ddgi::{DdgiResourceBytes, DdgiVolumeGrid, SUPPORTED_DDGI_SPACINGS_VOXELS};
use crate::environment_probes::{
    EnvironmentProbeVisualizationFilter, EnvironmentProbeVisualizationMode,
};
use crate::flora::species;
use crate::geom::{build_bvh, Aabb3, Cuboid, UAabb3};
use crate::particles::{
    ButterflyEmitter, ButterflyEmitterDesc, LeafEmitterDesc, ParticleForces, ParticleHandle,
    ParticleSnapshot, ParticleSystem, PARTICLE_CAPACITY,
};
use crate::tracer::tree_preview_mesh::build_tree_preview_mesh;
use crate::tracer::{
    allium_height_color_tables, grass_flora_height_color_tables, kochia_color_tables,
    solid_flora_height_color_tables, CloudGuiParams, FruitMotionParams, GlassGuiParams,
    KochiaMotionParams, KochiaVisualParams, TerrainRayQuery, Tracer, TracerDesc, WindGuiParams,
    DIRECT_SUN_SHADOW_SOURCE_ALL,
};
use crate::tree_gen::TreeDesc;
use crate::util::get_sun_dir;
use crate::util::TimeInfo;
use crate::util::{ChunkPopMode, GrowingFloraChunk, GrowingFloraQueue, LatestChunkQueue, BENCH};
use crate::wind::WindResponseCurve;
use crate::RenderFlags;
use crate::{egui_renderer::EguiRenderer, window::WindowState, WaterProfilePreference};
use anyhow::{Context, Result};
use egui::{Color32, ColorImage, FontData, FontDefinitions, FontFamily, RichText, TextureHandle};
use glam::{UVec3, Vec2, Vec3, Vec4};
use petalsonic::config::{AmbisonicsBackend, HrtfBackend};
use rand::RngExt;
use std::collections::HashMap;

use re_flora_vkn::{
    Allocator, GpuProfiler, GpuProfilerFrameResults, PipelineStage, SwapchainDesc,
    SwapchainFrameError, SwapchainFrameManager,
};
use re_flora_vkn::{Swapchain, VulkanContext};
use re_flora_water::PondWaterConfig;
use std::time::{Duration, Instant};
use ui_style::{
    apply_gui_style, draw_center_card, draw_flora_paint_panel, draw_item_panel, draw_voxel_palette,
    FloraPaintPanelEntry, ItemPanelSlot, VoxelPaletteEntry, CUSTOM_GUI_FONT_NAME,
    CUSTOM_GUI_FONT_PATH, FERTILIZER_SLOT_INDEX, FERTILIZER_TOOL_ACCENT, FLOWER_ACCENT,
    GOLD_ACCENT, HAND_SLOT_INDEX, HOE_SLOT_INDEX, HOE_TOOL_ACCENT,
    ITEM_PANEL_FERTILIZER_ICON_FALLBACK_PATH, ITEM_PANEL_FERTILIZER_ICON_PATH,
    ITEM_PANEL_HOE_ICON_FALLBACK_PATH, ITEM_PANEL_HOE_ICON_PATH, ITEM_PANEL_PIPE_ICON_PATH,
    ITEM_PANEL_SHOVEL_ICON_FALLBACK_PATH, ITEM_PANEL_SHOVEL_ICON_PATH,
    ITEM_PANEL_SMOOTH_ICON_FALLBACK_PATH, ITEM_PANEL_SMOOTH_ICON_PATH,
    ITEM_PANEL_SOIL_INSPECTOR_ICON_FALLBACK_PATH, ITEM_PANEL_SOIL_INSPECTOR_ICON_PATH,
    ITEM_PANEL_SPRINKLER_ICON_PATH, ITEM_PANEL_STAFF_ICON_FALLBACK_PATH,
    ITEM_PANEL_STAFF_ICON_PATH, ITEM_PANEL_TILLER_ICON_FALLBACK_PATH, ITEM_PANEL_TILLER_ICON_PATH,
    ITEM_PANEL_TREE_ICON_FALLBACK_PATH, ITEM_PANEL_TREE_ICON_PATH,
    ITEM_PANEL_WATER_ICON_FALLBACK_PATH, ITEM_PANEL_WATER_ICON_PATH, PANEL_BG, PANEL_DARK,
    PIPE_PLACEABLE_SLOT_INDEX, PIPE_SLOT_INDEX, SAGE_ACCENT, SHADOW_COLOR, SHOVEL_SLOT_INDEX,
    SHOVEL_TOOL_ACCENT, SMOOTH_SLOT_INDEX, SMOOTH_TOOL_ACCENT, SOIL_INSPECTOR_SLOT_INDEX,
    SOIL_INSPECTOR_TOOL_ACCENT, SPRINKLER_PLACEABLE_SLOT_INDEX, SPRINKLER_SLOT_INDEX,
    STAFF_SLOT_INDEX, STAFF_TOOL_ACCENT, TILLER_SLOT_INDEX, TILLER_TOOL_ACCENT,
    TREE_PLACEABLE_SLOT_INDEX, TREE_SLOT_INDEX, TREE_TOOL_ACCENT, WATERING_SLOT_INDEX,
    WATER_TOOL_ACCENT,
};
use winit::{
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::WindowId,
};

const LEAF_CLUSTER_DISTANCE: f32 = 0.08;
const TERRAIN_EDIT_PREVIEW_ALPHA: f32 = 0.2;
// Muted runs should exercise audio setup, source updates, ray tracing, and pump paths
// without producing audible output for the user.
const MUTED_AUDIO_OUTPUT_GAIN_DB: f32 = -120.0;

fn advance_time_of_day(
    current_time_of_day: f32,
    elapsed_ticks: u32,
    world_tick_seconds: f32,
    day_cycle_minutes: f32,
) -> f32 {
    let time_speed = 1.0 / (day_cycle_minutes * 60.0);
    (current_time_of_day + elapsed_ticks as f32 * world_tick_seconds * time_speed) % 1.0
}

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
    current_time_of_day: f32,
    render_flags: RenderFlags,
    accumulated_mouse_delta: Vec2,
    smoothed_mouse_delta: Vec2,
    cursor_position_physical: Option<Vec2>,
    camera_control_mode: CameraControlMode,
    orbit_camera_focus: Vec3,
    orbit_keyboard_pan_input: OrbitKeyboardPanInput,
    orbit_mouse_drag_held: bool,
    orbit_mouse_drag_button: Option<MouseButton>,
    orbit_mouse_drag_pan_active: bool,
    orbit_mouse_drag_last_position_physical: Option<Vec2>,
    orbit_pan_smoother: OrbitDeltaSmoother,
    orbit_rotation_smoother: OrbitDeltaSmoother,
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
    terrain_physics: TerrainPhysics,

    debug_settings: DebugSettings,
    debug_tree_pos: Vec3,
    tree_placement_preview_desc: TreeDesc,
    config_panel_visible: bool,
    environment_probe_spacing_draft: u32,
    environment_probe_rebuild_spacing_voxels: Option<u32>,
    camera_snapshots: CameraSnapshotLibrary,
    camera_snapshot_draft_name: String,
    camera_snapshot_draft_description: String,
    camera_snapshot_status: Option<String>,
    frame_timing_panel_visible: bool,
    frame_timing_snapshot: FrameTimingSnapshot,
    card_display_visible: bool,
    item_panel_shovel_icon: Option<TextureHandle>,
    item_panel_smooth_icon: Option<TextureHandle>,
    item_panel_staff_icon: Option<TextureHandle>,
    item_panel_hoe_icon: Option<TextureHandle>,
    item_panel_tree_icon: Option<TextureHandle>,
    item_panel_water_icon: Option<TextureHandle>,
    item_panel_sprinkler_icon: Option<TextureHandle>,
    item_panel_pipe_icon: Option<TextureHandle>,
    item_panel_soil_inspector_icon: Option<TextureHandle>,
    item_panel_fertilizer_icon: Option<TextureHandle>,
    item_panel_tiller_icon: Option<TextureHandle>,
    player_tools: PlayerToolState,
    water_particle_handoff_main_thread_ms: Option<f32>,

    flora_tick: u32,
    flora_tick_accumulator: f32,
    moisture_dry_chunk_cursor: u32,
    moisture_spread_chunk_cursor: u32,
    flora_paint_dab_serial: u32,
    growing_flora_chunks: GrowingFloraQueue,
    sun_position_update_tick_accumulator: u32,
    vsm_history_reset_pending: bool,

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
    butterfly_spawn_source_refresh_elapsed: f32,
    sprinkler_records: Vec<SprinklerRecord>,
    sprinkler_emitters: Vec<SprinklerEmitter>,
    next_sprinkler_id: u32,
    irrigation_network: IrrigationNetwork,
    active_pipe_drag: Option<PipeDrag>,
    particle_animation_time_sec: f32,
    water_sim: water::AsyncWaterSim,
    water_runtime_overrides: water::WaterRuntimeOverrides,
    water_terrain_initialized: bool,
    water_terrain_collider_cache_rebuild_pending: bool,
    deferred_terrain_sdf_source_refreshes: LatestChunkQueue<water::TerrainSdfSourceRefreshRequest>,
    deferred_terrain_sdf_collider_rebuilds:
        LatestChunkQueue<water::TerrainSdfColliderRebuildRequest>,
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
    screenshot_to_clipboard_requested: bool,
    environment_irradiance_capture_path: Option<String>,
    environment_irradiance_capture_taken: bool,
    denoiser_bench: Option<DenoiserBench>,
    auto_exit_delay: Option<f32>,
    tree_bench: Option<TreeBench>,
    authored_flora_bench: Option<AuthoredFloraBench>,
    water_edit_soak: Option<water::WaterEditSoak>,
    environment_lighting_test_scene:
        Option<environment_lighting_test_scene::EnvironmentLightingTestScene>,
    hybrid_transparency_test_scene:
        Option<hybrid_transparency_test_scene::HybridTransparencyTestScene>,
    deferred_chunk_rebuilds: LatestChunkQueue<ChunkRebuildRequest>,
    terrain_chunk_rebuild_inflight: Option<TerrainChunkRebuildInFlight>,

    // note: always keep the context to end, as it has to be destroyed last
    vulkan_ctx: VulkanContext,

    // Keep ownership so the shared PetalSonic engine outlives every subsystem.
    #[allow(dead_code)]
    spatial_sound_manager: SpatialSoundManager,
    tree_audio_manager: TreeAudioManager,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum CameraControlMode {
    FreeFly,
    Walk,
    #[default]
    OrbitEdit,
}

impl CameraControlMode {
    fn is_free_look(self) -> bool {
        matches!(self, Self::FreeFly | Self::Walk)
    }

    fn is_free_fly(self) -> bool {
        matches!(self, Self::FreeFly)
    }

    fn is_walk(self) -> bool {
        matches!(self, Self::Walk)
    }

    fn is_orbit_edit(self) -> bool {
        matches!(self, Self::OrbitEdit)
    }

    fn next(self) -> Self {
        match self {
            Self::OrbitEdit => Self::FreeFly,
            Self::FreeFly => Self::Walk,
            Self::Walk => Self::OrbitEdit,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct OrbitKeyboardPanInput {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

impl OrbitKeyboardPanInput {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn handle_key(&mut self, code: KeyCode, pressed: bool) {
        match code {
            KeyCode::KeyW | KeyCode::ArrowUp => self.forward = pressed,
            KeyCode::KeyS | KeyCode::ArrowDown => self.backward = pressed,
            KeyCode::KeyA | KeyCode::ArrowLeft => self.left = pressed,
            KeyCode::KeyD | KeyCode::ArrowRight => self.right = pressed,
            KeyCode::KeyE => self.up = pressed,
            KeyCode::KeyQ => self.down = pressed,
            _ => {}
        }
    }

    fn input_vector(self) -> Vec3 {
        let mut input = Vec3::ZERO;
        if self.forward {
            input.z += 1.0;
        }
        if self.backward {
            input.z -= 1.0;
        }
        if self.left {
            input.x -= 1.0;
        }
        if self.right {
            input.x += 1.0;
        }
        if self.up {
            input.y += 1.0;
        }
        if self.down {
            input.y -= 1.0;
        }
        input.normalize_or_zero()
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct OrbitDeltaSmoother {
    current_delta: Vec3,
    target_delta: Vec3,
}

impl OrbitDeltaSmoother {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn add_delta(&mut self, delta: Vec3) {
        if delta.is_finite() {
            self.target_delta += delta;
        }
    }

    fn pending_delta(&self) -> Vec3 {
        self.target_delta - self.current_delta
    }

    fn advance(&mut self, frame_delta_time: f32) -> Vec3 {
        if frame_delta_time <= f32::EPSILON || !frame_delta_time.is_finite() {
            return Vec3::ZERO;
        }

        let remaining_delta = self.pending_delta();
        if remaining_delta.length_squared() <= ORBIT_CAMERA_DELTA_SNAP_DISTANCE.powi(2) {
            self.reset();
            return remaining_delta;
        }

        let alpha = (1.0 - (-ORBIT_CAMERA_DELTA_INTERPOLATION_RATE * frame_delta_time).exp())
            .clamp(0.0, 1.0);
        let mut advanced_delta = remaining_delta * alpha;
        self.current_delta += advanced_delta;

        let remaining_after_advance = self.target_delta - self.current_delta;
        if remaining_after_advance.length_squared() <= ORBIT_CAMERA_DELTA_SNAP_DISTANCE.powi(2) {
            advanced_delta += remaining_after_advance;
            self.reset();
        }

        advanced_delta
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
                for scope in results.scopes.iter().filter(|scope| {
                    matches!(
                        scope.name,
                        "environment_probes.rederive" | "environment_probes.trace_priority"
                    )
                }) {
                    log::info!(
                        "[PERF][GPU_EVENT_SCOPE] {}={:.0}us",
                        scope.name,
                        scope.duration_us(),
                    );
                }
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
        }) = self.growing_flora_chunks.pop(ChunkPopMode::Nearest {
            focus: self.tracer.camera_position(),
            chunk_extent: VOXEL_DIM_PER_CHUNK,
        })
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
                if self.enqueue_deferred_chunk_rebuilds(&chunk_ids) {
                    self.terrain_physics.mark_terrain_voxel_bound_dirty(bound);
                }
            }
            BuildEdit::RebuildMeshWithoutFlora(bound) => {
                let chunk_ids =
                    world_ops::affected_chunk_indices_for_bound(bound, VOXEL_DIM_PER_CHUNK);
                if self.enqueue_deferred_chunk_rebuilds_without_flora(&chunk_ids) {
                    self.terrain_physics.mark_terrain_voxel_bound_dirty(bound);
                }
            }
            BuildEdit::RebuildChunks(chunk_ids) => {
                if self.enqueue_deferred_chunk_rebuilds(&chunk_ids) {
                    self.terrain_physics
                        .mark_terrain_chunks_dirty(&chunk_ids, VOXEL_DIM_PER_CHUNK);
                }
            }
            BuildEdit::RebuildChunksWithoutFlora(chunk_ids) => {
                if self.enqueue_deferred_chunk_rebuilds_without_flora(&chunk_ids) {
                    self.terrain_physics
                        .mark_terrain_chunks_dirty(&chunk_ids, VOXEL_DIM_PER_CHUNK);
                }
            }
        }
        Ok(())
    }
}

const VOXEL_DIM_PER_CHUNK: UVec3 = UVec3::new(256, 256, 256);
pub(super) const CHUNK_DIM: UVec3 = UVec3::new(2, 2, 2);
const FREE_ATLAS_DIM: UVec3 = UVec3::new(512, 512, 512);
const MAX_FRAMES_IN_FLIGHT: usize = 1;
const GPU_PROFILER_MAX_SCOPES_PER_FRAME: usize = 64;
const TERRAIN_EDIT_DEFAULT_RADIUS: f32 = 0.08;
const TERRAIN_EDIT_RADIUS_MIN: f32 = 0.03;
const TERRAIN_EDIT_RADIUS_MAX: f32 = 0.36;
const TERRAIN_EDIT_RADIUS_SCROLL_STEP: f32 = 0.01;
const ORBIT_CAMERA_DEFAULT_FOCUS_HEIGHT: f32 = 0.5;
const ORBIT_CAMERA_DEFAULT_FOCUS: Vec3 =
    INITIAL_EDITABLE_TERRAIN_BOUNDS.center_at_height(ORBIT_CAMERA_DEFAULT_FOCUS_HEIGHT);
const ORBIT_CAMERA_MIN_DISTANCE: f32 = 0.2;
const ORBIT_CAMERA_MAX_DISTANCE: f32 = 5.0;
const ORBIT_CAMERA_DOLLY_SPEED: f32 = 0.75;
const ORBIT_CAMERA_FOCUS_RAY_QUERY_DISTANCE: f32 = 10.0;
const ORBIT_CAMERA_MOUSE_DRAG_RADIANS_PER_PIXEL: f32 = 0.005;
const ORBIT_CAMERA_MOUSE_PAN_UNITS_PER_PHYSICAL_PIXEL: f32 = 0.001;
const ORBIT_CAMERA_DELTA_INTERPOLATION_RATE: f32 = 14.0;
const ORBIT_CAMERA_DELTA_SNAP_DISTANCE: f32 = 0.00001;
const ORBIT_CAMERA_KEYBOARD_PAN_UNITS_PER_SECOND_AT_UNIT_DISTANCE: f32 = 0.9;
const ORBIT_CAMERA_KEYBOARD_PAN_FAR_ZOOM_BOOST_START: f32 = 0.95;
const ORBIT_CAMERA_KEYBOARD_PAN_FAR_ZOOM_MAX_MULTIPLIER: f32 = 1.6;
const CENTER_CROSS_MARK_ARM_LENGTH: f32 = 8.0;
const CENTER_CROSS_MARK_GAP: f32 = 3.0;
const CENTER_CROSS_MARK_STROKE_WIDTH: f32 = 1.5;
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

fn draw_center_cross_mark(ctx: &egui::Context) {
    let center = ctx.content_rect().center();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("center_cross_mark"),
    ));
    let shadow_stroke = egui::Stroke::new(
        CENTER_CROSS_MARK_STROKE_WIDTH + 1.5,
        Color32::from_black_alpha(150),
    );
    let foreground_stroke = egui::Stroke::new(
        CENTER_CROSS_MARK_STROKE_WIDTH,
        Color32::from_white_alpha(230),
    );
    let segments = [
        (
            egui::pos2(center.x - CENTER_CROSS_MARK_ARM_LENGTH, center.y),
            egui::pos2(center.x - CENTER_CROSS_MARK_GAP, center.y),
        ),
        (
            egui::pos2(center.x + CENTER_CROSS_MARK_GAP, center.y),
            egui::pos2(center.x + CENTER_CROSS_MARK_ARM_LENGTH, center.y),
        ),
        (
            egui::pos2(center.x, center.y - CENTER_CROSS_MARK_ARM_LENGTH),
            egui::pos2(center.x, center.y - CENTER_CROSS_MARK_GAP),
        ),
        (
            egui::pos2(center.x, center.y + CENTER_CROSS_MARK_GAP),
            egui::pos2(center.x, center.y + CENTER_CROSS_MARK_ARM_LENGTH),
        ),
    ];

    for (start, end) in segments {
        painter.line_segment([start, end], shadow_stroke);
    }
    for (start, end) in segments {
        painter.line_segment([start, end], foreground_stroke);
    }
}

impl App {
    fn debug_startup_block_bounds() -> (Vec3, Vec3) {
        // Temporary synthetic obstacle. Bounds are derived from the atlas dimensions so changing
        // CHUNK_DIM does not require hand-updating debug geometry.
        let atlas_dim = (CHUNK_DIM * VOXEL_DIM_PER_CHUNK).as_vec3();
        let min = Vec3::new(atlas_dim.x * 0.58, 0.0, atlas_dim.z * 0.75);
        let max = (min + Vec3::new(20.0, atlas_dim.y * 0.5, 88.0)).min(atlas_dim);
        (min, max)
    }

    fn apply_debug_cuboid(&mut self, min: Vec3, max: Vec3, voxel_type: u32) -> Result<()> {
        let cuboid = Cuboid::from_min_max(min, max);
        let aabb = Aabb3::new(min, max);
        let bvh_nodes = build_bvh(&[aabb], &[0]).map_err(anyhow::Error::msg)?;
        self.plain_builder
            .chunk_modify_cuboids_with_voxel_type(&bvh_nodes, &[cuboid], voxel_type)
    }

    fn apply_debug_startup_materials(&mut self) -> Result<()> {
        let (block_min, block_max) = Self::debug_startup_block_bounds();
        self.apply_debug_cuboid(block_min, block_max, VOXEL_TYPE_DIRT)?;

        self.request_vsm_history_reset();
        Ok(())
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
            self.debug_settings.adjustables.master_volume.value,
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
        self.spatial_sound_manager.set_audio_ray_tracing_enabled(
            self.debug_settings
                .adjustables
                .audio_ray_tracing_enabled
                .value,
        );
    }

    fn update_spatial_audio_backends(&mut self) {
        let use_ambisonics = self.debug_settings.adjustables.audio_use_ambisonics.value;
        let ambisonics_backend = Self::selected_ambisonics_backend(
            self.debug_settings
                .adjustables
                .audio_ambisonics_backend
                .value,
        );
        let hrtf_backend = Self::effective_hrtf_backend(
            Self::selected_hrtf_backend(self.debug_settings.adjustables.audio_hrtf_backend.value),
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
            swapchain.get_render_pass(),
        );

        let plain_builder = PlainBuilder::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            CHUNK_DIM * VOXEL_DIM_PER_CHUNK,
            FREE_ATLAS_DIM,
        );

        let mut surface_builder = SurfaceBuilder::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            plain_builder.get_resources(),
            VOXEL_DIM_PER_CHUNK,
            chunk_bound,
        );
        if options.perf {
            surface_builder.enable_gpu_job_profiling(32);
        }

        let contree_pool_sizes =
            ContreeBuilder::pool_sizes_for_chunk_dim(CHUNK_DIM, VOXEL_DIM_PER_CHUNK);
        log::info!(
            "Contree pool sizes: node={:.2} MiB leaf={:.2} MiB chunk_dim={:?} per_chunk_node={} bytes per_chunk_leaf={} MiB",
            contree_pool_sizes.node_pool_size_in_bytes as f64 / (1024.0 * 1024.0),
            contree_pool_sizes.leaf_pool_size_in_bytes as f64 / (1024.0 * 1024.0),
            CHUNK_DIM,
            contree_pool_sizes.node_chunk_size_in_bytes,
            contree_pool_sizes.leaf_chunk_size_in_bytes / (1024 * 1024),
        );
        let contree_builder = ContreeBuilder::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            surface_builder.get_resources(),
            CHUNK_DIM,
            VOXEL_DIM_PER_CHUNK,
            contree_pool_sizes.node_pool_size_in_bytes,
            contree_pool_sizes.leaf_pool_size_in_bytes,
        );

        let scene_accel_builder =
            SceneAccelBuilder::new(vulkan_ctx.clone(), allocator.clone(), chunk_bound)?;

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
            chunk_bound,
            window_state.window_extent(),
            contree_builder.get_resources(),
            scene_accel_builder.get_resources(),
            plain_builder.get_resources(),
            TracerDesc {
                scaling_factor: 0.5,
                default_camera_look_at: ORBIT_CAMERA_DEFAULT_FOCUS,
                voxel_dim_per_chunk: VOXEL_DIM_PER_CHUNK,
                environment_probe_spacing_voxels: options.environment_probe_spacing_voxels,
                environment_probe_visualization_enabled: options.environment_probe_visualization,
                environment_irradiance_capture_enabled: options
                    .environment_irradiance_capture_path
                    .is_some(),
                ddgi_debug_view: options.ddgi_debug_view,
            },
            spatial_sound_manager.clone(),
        )?;
        {
            let shadow = tracer.direct_sun_shadow_resources();
            let contree_resources = contree_builder.get_resources();
            plain_builder.bind_terrain_moisture_dry_resources(
                shadow.gui_input,
                shadow.shadow_camera_info,
                shadow.shadow_map_tex_for_vsm_ping,
                shadow.leaf_shadow_opacity_blended_tex,
                shadow.leaf_shadow_mask_tex,
                shadow.cloud_shadow_tex,
                &contree_resources.contree_leaf_data,
                &contree_resources.surface_leaf_coords,
                &contree_resources.surface_leaf_chunk_info,
            );
        }

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

        let editable_center = INITIAL_EDITABLE_TERRAIN_BOUNDS.center();
        let debug_tree_pos = if options.environment_lighting_test_scene.is_some() {
            environment_lighting_test_scene::STARTUP_TREE_POSITION
        } else if options.hybrid_transparency_test_scene {
            hybrid_transparency_test_scene::STARTUP_TREE_POSITION
        } else {
            Vec3::new(editable_center.x, 0.2, editable_center.z)
        };
        let debug_settings = DebugSettings::load();
        let mut tree_placement_preview_desc = debug_settings
            .tree
            .desc
            .at_age(debug_settings.adjustables.tree_age.value);
        tree_placement_preview_desc.branching.seed = rand::rng().random::<u64>();
        let mut render_flags = RenderFlags::from(options);
        if render_flags.enable_flora {
            render_flags.enable_leaves = debug_settings.tree.render_leaves;
        }
        let current_time_of_day = debug_settings.adjustables.time_of_day.value;
        let terrain_physics = TerrainPhysics::new(debug_settings.adjustables.fruit_cycle.value);

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
            color_low: color_to_vec4(debug_settings.adjustables.leaves_bottom_color.value),
            color_high: color_to_vec4(debug_settings.adjustables.leaves_tip_color.value),
            ..LeafEmitterDesc::default()
        };
        let tree_audio_manager = TreeAudioManager::new(
            spatial_sound_manager.clone(),
            Self::tree_audio_wind_response_curve(&debug_settings.adjustables),
            debug_settings.adjustables.tree_wind_volume_db.value,
            Self::tree_rustle_params(&debug_settings.adjustables),
        );
        let butterfly_emitters = Vec::new();
        let butterfly_emitter_desc =
            Self::butterfly_desc_from_gui_adjustables(&debug_settings.adjustables);
        let sprinkler_records = Vec::new();
        let sprinkler_emitters = Vec::new();
        let next_sprinkler_id = 1;
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
        let water_profile_config = options.water_profile.map(|_| water_config.clone());
        water::apply_water_gui_adjustables_to_config(
            &mut water_config,
            &debug_settings.adjustables,
        );
        let water_runtime_overrides =
            water::WaterRuntimeOverrides::from_options(options, water_profile_config);
        water_runtime_overrides.apply(&mut water_config);

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
                collider_total: 0,
            }),

            accumulated_mouse_delta: Vec2::ZERO,
            smoothed_mouse_delta: Vec2::ZERO,
            cursor_position_physical: None,
            camera_control_mode: CameraControlMode::default(),
            orbit_camera_focus: ORBIT_CAMERA_DEFAULT_FOCUS,
            orbit_keyboard_pan_input: OrbitKeyboardPanInput::default(),
            orbit_mouse_drag_held: false,
            orbit_mouse_drag_button: None,
            orbit_mouse_drag_pan_active: false,
            orbit_mouse_drag_last_position_physical: None,
            orbit_pan_smoother: OrbitDeltaSmoother::default(),
            orbit_rotation_smoother: OrbitDeltaSmoother::default(),
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
            terrain_physics,

            is_resize_pending: false,
            time_info: TimeInfo::default(),
            current_time_of_day,
            render_flags,

            debug_settings,
            debug_tree_pos,
            tree_placement_preview_desc,
            tree_variation_config: TreeVariationConfig::default(),
            regenerate_trees_requested: false,
            prev_bound: Default::default(),
            tree_records: HashMap::new(),
            config_panel_visible: false,
            environment_probe_spacing_draft: options.environment_probe_spacing_voxels,
            environment_probe_rebuild_spacing_voxels: options
                .environment_probe_rebuild_spacing_voxels,
            camera_snapshots,
            camera_snapshot_draft_name,
            camera_snapshot_draft_description: String::new(),
            camera_snapshot_status: None,
            frame_timing_panel_visible: options.perf,
            frame_timing_snapshot: FrameTimingSnapshot::default(),
            card_display_visible: false,
            item_panel_shovel_icon: None,
            item_panel_smooth_icon: None,
            item_panel_staff_icon: None,
            item_panel_hoe_icon: None,
            item_panel_tree_icon: None,
            item_panel_water_icon: None,
            item_panel_sprinkler_icon: None,
            item_panel_pipe_icon: None,
            item_panel_soil_inspector_icon: None,
            item_panel_fertilizer_icon: None,
            item_panel_tiller_icon: None,
            player_tools: PlayerToolState::default(),
            water_particle_handoff_main_thread_ms: None,
            flora_tick: FLORA_FULL_GROWTH_TICKS,
            flora_tick_accumulator: 0.0,
            moisture_dry_chunk_cursor: 0,
            moisture_spread_chunk_cursor: 0,
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
            butterfly_spawn_source_refresh_elapsed: f32::INFINITY,
            sprinkler_records,
            sprinkler_emitters,
            next_sprinkler_id,
            irrigation_network: IrrigationNetwork::default(),
            active_pipe_drag: None,
            particle_animation_time_sec: 0.0,
            water_sim,
            water_runtime_overrides,
            water_terrain_initialized: false,
            water_terrain_collider_cache_rebuild_pending: false,
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
            screenshot_to_clipboard_requested: false,
            environment_irradiance_capture_path: options
                .environment_irradiance_capture_path
                .clone(),
            environment_irradiance_capture_taken: false,
            denoiser_bench: options.denoiser_bench.clone().map(DenoiserBench::new),
            auto_exit_delay: options.auto_exit_delay,
            tree_bench: options
                .tree_bench
                .then(|| TreeBench::new(options.tree_bench_samples, options.tree_bench_rapid)),
            authored_flora_bench: options
                .authored_flora_bench
                .then(|| AuthoredFloraBench::new(options.authored_flora_bench_samples)),
            water_edit_soak: options.water_edit_soak.then(water::WaterEditSoak::default),
            environment_lighting_test_scene: options
                .environment_lighting_test_scene
                .map(environment_lighting_test_scene::EnvironmentLightingTestScene::new),
            hybrid_transparency_test_scene: options
                .hybrid_transparency_test_scene
                .then(hybrid_transparency_test_scene::HybridTransparencyTestScene::new),
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
        if options.environment_lighting_test_scene.is_some() {
            app.configure_environment_lighting_test_scene_camera();
        }
        app.sync_cursor_with_panels();

        app.configure_gui_font()?;
        app.load_item_panel_icons()?;
        app.rebuild_tree_placement_preview()?;
        if options.hybrid_transparency_test_scene {
            app.configure_hybrid_transparency_test_scene()?;
        }

        Ok(app)
    }

    fn rebuild_tree_placement_preview(&mut self) -> Result<()> {
        let mesh = build_tree_preview_mesh(&self.tree_placement_preview_desc);
        self.tracer.upload_tree_geometry_preview(&mesh)
    }

    fn sync_tree_placement_preview_from_gui(&mut self) -> Result<()> {
        let seed = self.tree_placement_preview_desc.branching.seed;
        self.tree_placement_preview_desc = self
            .debug_settings
            .tree
            .desc
            .at_age(self.debug_settings.adjustables.tree_age.value);
        self.tree_placement_preview_desc.branching.seed = seed;
        self.rebuild_tree_placement_preview()
    }

    pub(super) fn advance_tree_placement_preview(&mut self) -> Result<()> {
        self.tree_placement_preview_desc = self
            .debug_settings
            .tree
            .desc
            .at_age(self.debug_settings.adjustables.tree_age.value);
        self.tree_placement_preview_desc.branching.seed = rand::rng().random::<u64>();
        self.rebuild_tree_placement_preview()
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

        let sprinkler_path = if std::path::Path::new(ITEM_PANEL_SPRINKLER_ICON_PATH).exists() {
            ITEM_PANEL_SPRINKLER_ICON_PATH
        } else {
            log::warn!(
                "Item panel icon not found at {}. Falling back to {}",
                ITEM_PANEL_SPRINKLER_ICON_PATH,
                ITEM_PANEL_WATER_ICON_FALLBACK_PATH
            );
            ITEM_PANEL_WATER_ICON_FALLBACK_PATH
        };
        let sprinkler_bytes = std::fs::read(sprinkler_path)
            .with_context(|| format!("Failed to read item panel icon from {sprinkler_path}"))?;
        let sprinkler_rgba = image::load_from_memory(&sprinkler_bytes)
            .with_context(|| format!("Failed to decode item panel icon from {sprinkler_path}"))?
            .to_rgba8();
        let sprinkler_size = [
            sprinkler_rgba.width() as usize,
            sprinkler_rgba.height() as usize,
        ];
        let sprinkler_pixels = sprinkler_rgba.into_raw();
        let sprinkler_image = ColorImage::from_rgba_unmultiplied(sprinkler_size, &sprinkler_pixels);
        let sprinkler_texture = self.egui_renderer.context().load_texture(
            "item_panel_sprinkler",
            sprinkler_image,
            egui::TextureOptions::NEAREST,
        );
        self.item_panel_sprinkler_icon = Some(sprinkler_texture);

        let pipe_bytes = std::fs::read(ITEM_PANEL_PIPE_ICON_PATH).with_context(|| {
            format!("Failed to read item panel icon from {ITEM_PANEL_PIPE_ICON_PATH}")
        })?;
        let pipe_rgba = image::load_from_memory(&pipe_bytes)
            .with_context(|| {
                format!("Failed to decode item panel icon from {ITEM_PANEL_PIPE_ICON_PATH}")
            })?
            .to_rgba8();
        let pipe_size = [pipe_rgba.width() as usize, pipe_rgba.height() as usize];
        let pipe_pixels = pipe_rgba.into_raw();
        let pipe_image = ColorImage::from_rgba_unmultiplied(pipe_size, &pipe_pixels);
        let pipe_texture = self.egui_renderer.context().load_texture(
            "item_panel_pipe",
            pipe_image,
            egui::TextureOptions::NEAREST,
        );
        self.item_panel_pipe_icon = Some(pipe_texture);

        let soil_inspector_path =
            if std::path::Path::new(ITEM_PANEL_SOIL_INSPECTOR_ICON_PATH).exists() {
                ITEM_PANEL_SOIL_INSPECTOR_ICON_PATH
            } else {
                log::warn!(
                    "Item panel icon not found at {}. Falling back to {}",
                    ITEM_PANEL_SOIL_INSPECTOR_ICON_PATH,
                    ITEM_PANEL_SOIL_INSPECTOR_ICON_FALLBACK_PATH
                );
                ITEM_PANEL_SOIL_INSPECTOR_ICON_FALLBACK_PATH
            };

        let soil_inspector_bytes = std::fs::read(soil_inspector_path).with_context(|| {
            format!("Failed to read item panel icon from {soil_inspector_path}")
        })?;
        let soil_inspector_rgba = image::load_from_memory(&soil_inspector_bytes)
            .with_context(|| {
                format!("Failed to decode item panel icon from {soil_inspector_path}")
            })?
            .to_rgba8();
        let soil_inspector_size = [
            soil_inspector_rgba.width() as usize,
            soil_inspector_rgba.height() as usize,
        ];
        let soil_inspector_pixels = soil_inspector_rgba.into_raw();
        let soil_inspector_image =
            ColorImage::from_rgba_unmultiplied(soil_inspector_size, &soil_inspector_pixels);

        let soil_inspector_texture = self.egui_renderer.context().load_texture(
            "item_panel_soil_inspector",
            soil_inspector_image,
            egui::TextureOptions::NEAREST,
        );
        self.item_panel_soil_inspector_icon = Some(soil_inspector_texture);

        let fertilizer_path = if std::path::Path::new(ITEM_PANEL_FERTILIZER_ICON_PATH).exists() {
            ITEM_PANEL_FERTILIZER_ICON_PATH
        } else {
            log::warn!(
                "Item panel icon not found at {}. Falling back to {}",
                ITEM_PANEL_FERTILIZER_ICON_PATH,
                ITEM_PANEL_FERTILIZER_ICON_FALLBACK_PATH
            );
            ITEM_PANEL_FERTILIZER_ICON_FALLBACK_PATH
        };

        let fertilizer_bytes = std::fs::read(fertilizer_path)
            .with_context(|| format!("Failed to read item panel icon from {fertilizer_path}"))?;
        let fertilizer_rgba = image::load_from_memory(&fertilizer_bytes)
            .with_context(|| format!("Failed to decode item panel icon from {fertilizer_path}"))?
            .to_rgba8();
        let fertilizer_size = [
            fertilizer_rgba.width() as usize,
            fertilizer_rgba.height() as usize,
        ];
        let fertilizer_pixels = fertilizer_rgba.into_raw();
        let fertilizer_image =
            ColorImage::from_rgba_unmultiplied(fertilizer_size, &fertilizer_pixels);

        let fertilizer_texture = self.egui_renderer.context().load_texture(
            "item_panel_fertilizer",
            fertilizer_image,
            egui::TextureOptions::NEAREST,
        );
        self.item_panel_fertilizer_icon = Some(fertilizer_texture);

        let tiller_path = if std::path::Path::new(ITEM_PANEL_TILLER_ICON_PATH).exists() {
            ITEM_PANEL_TILLER_ICON_PATH
        } else {
            log::warn!(
                "Item panel icon not found at {}. Falling back to {}",
                ITEM_PANEL_TILLER_ICON_PATH,
                ITEM_PANEL_TILLER_ICON_FALLBACK_PATH
            );
            ITEM_PANEL_TILLER_ICON_FALLBACK_PATH
        };

        let tiller_bytes = std::fs::read(tiller_path)
            .with_context(|| format!("Failed to read item panel icon from {tiller_path}"))?;
        let tiller_rgba = image::load_from_memory(&tiller_bytes)
            .with_context(|| format!("Failed to decode item panel icon from {tiller_path}"))?
            .to_rgba8();
        let tiller_size = [tiller_rgba.width() as usize, tiller_rgba.height() as usize];
        let tiller_pixels = tiller_rgba.into_raw();
        let tiller_image = ColorImage::from_rgba_unmultiplied(tiller_size, &tiller_pixels);

        let tiller_texture = self.egui_renderer.context().load_texture(
            "item_panel_tiller",
            tiller_image,
            egui::TextureOptions::NEAREST,
        );
        self.item_panel_tiller_icon = Some(tiller_texture);
        Ok(())
    }

    fn calculate_sun_position(time_of_day: f32, latitude: f32, season: f32) -> (f32, f32) {
        environment::calculate_sun_position(time_of_day, latitude, season)
    }

    fn apply_denoiser_benchmark_camera_motion(&mut self) {
        let Some((capture_frame, is_last_frame)) = self
            .denoiser_bench
            .as_ref()
            .and_then(DenoiserBench::camera_motion_frame)
        else {
            return;
        };

        let position = self.tracer.camera_position();
        let front = self.tracer.camera_front().normalize_or_zero();
        let mut right = front.cross(Vec3::Y).normalize_or_zero();
        if right.length_squared() <= f32::EPSILON {
            right = Vec3::X;
        }
        let translation =
            right * CAMERA_STRAFE_PER_FRAME_WORLD + front * CAMERA_FORWARD_PER_FRAME_WORLD;
        let (sin_yaw, cos_yaw) = CAMERA_YAW_PER_FRAME_RADIANS.sin_cos();
        let rotated_front = Vec3::new(
            cos_yaw * front.x + sin_yaw * front.z,
            front.y,
            -sin_yaw * front.x + cos_yaw * front.z,
        )
        .normalize_or_zero();
        let new_position = position + translation;
        self.tracer
            .set_camera_pose_looking_at(new_position, new_position + rotated_front);

        if capture_frame == 0 || is_last_frame {
            log::info!(
                "[DENOISER_BENCH] camera motion frame={} position=({:.4},{:.4},{:.4})",
                capture_frame,
                new_position.x,
                new_position.y,
                new_position.z,
            );
        }
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
        let environment_probe_edit_bound =
            Self::environment_probe_edit_bound(&plan, VOXEL_DIM_PER_CHUNK);
        world_ops::execute_edit_plan_on_backend(self, plan)?;
        if affects_shadow_history {
            self.request_vsm_history_reset();
        }
        if let Some(edit_bound) = environment_probe_edit_bound {
            self.tracer
                .request_environment_probe_refresh_near_voxel_bound(edit_bound);
        }
        Ok(())
    }

    fn environment_probe_edit_bound(
        plan: &WorldEditPlan,
        voxel_dim_per_chunk: UVec3,
    ) -> Option<UAabb3> {
        plan.build_edits
            .iter()
            .filter_map(|edit| match edit {
                BuildEdit::RebuildMesh(bound) | BuildEdit::RebuildMeshWithoutFlora(bound) => {
                    Some(*bound)
                }
                BuildEdit::RebuildChunks(chunk_ids)
                | BuildEdit::RebuildChunksWithoutFlora(chunk_ids) => {
                    let min_chunk = chunk_ids.iter().copied().reduce(UVec3::min)?;
                    let max_chunk = chunk_ids.iter().copied().reduce(UVec3::max)?;
                    Some(UAabb3::new(
                        min_chunk * voxel_dim_per_chunk,
                        (max_chunk + UVec3::ONE) * voxel_dim_per_chunk,
                    ))
                }
            })
            .reduce(|combined, bound| combined.union_with(&bound))
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
            if event.state == ElementState::Pressed && event.physical_key == KeyCode::Escape {
                self.on_terminate(event_loop);
                return;
            }

            if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyR {
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
                    self.sync_orbit_mouse_drag_position(Vec2::new(
                        position.x as f32,
                        position.y as f32,
                    ));
                }
                if let WindowEvent::MouseInput { state, button, .. } = &event {
                    if *state == ElementState::Released {
                        let captured = self.set_orbit_mouse_drag_state(*button, *state);
                        if !captured {
                            self.set_tool_mouse_button_state(*button, *state);
                            self.refresh_terrain_edit_hold_from_mouse_buttons();
                        }
                    }
                }
                return;
            }
        }

        if let WindowEvent::KeyboardInput { event, .. } = &event {
            if event.state == ElementState::Pressed
                && !event.repeat
                && event.physical_key == KeyCode::KeyP
            {
                self.screenshot_to_clipboard_requested = true;
                log::info!("[SCREENSHOT] P pressed; capturing next frame to clipboard");
                return;
            }

            if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyM {
                self.toggle_audio_output_mute();
                return;
            }

            if event.state == ElementState::Pressed
                && !event.repeat
                && event.physical_key == KeyCode::KeyC
            {
                self.card_display_visible = !self.card_display_visible;
                self.sync_cursor_with_panels();
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

                if self.is_orbit_edit_camera_mode() && self.keyboard_tool_shortcuts_available() {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        self.handle_orbit_keyboard_camera_input(code, event.state);
                    }
                }

                if self.keyboard_tool_shortcuts_available() && event.state == ElementState::Pressed
                {
                    let target_slot = match event.physical_key {
                        PhysicalKey::Code(KeyCode::Digit1) => Some(0),
                        PhysicalKey::Code(KeyCode::Digit2) => Some(1),
                        PhysicalKey::Code(KeyCode::Digit3) => Some(2),
                        PhysicalKey::Code(KeyCode::Digit4) => Some(3),
                        PhysicalKey::Code(KeyCode::Digit5) => Some(4),
                        PhysicalKey::Code(KeyCode::Digit6) => Some(5),
                        PhysicalKey::Code(KeyCode::Digit7) => Some(6),
                        PhysicalKey::Code(KeyCode::Digit8) => Some(7),
                        PhysicalKey::Code(KeyCode::Digit9) => Some(8),
                        _ => None,
                    };

                    if let Some(slot_idx) = target_slot {
                        self.select_item_panel_slot(slot_idx);
                    }

                    let target_placeable_slot = match event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyZ) => Some(TREE_PLACEABLE_SLOT_INDEX),
                        PhysicalKey::Code(KeyCode::KeyX) => Some(SPRINKLER_PLACEABLE_SLOT_INDEX),
                        PhysicalKey::Code(KeyCode::KeyV) => Some(PIPE_PLACEABLE_SLOT_INDEX),
                        _ => None,
                    };
                    if let Some(slot_idx) = target_placeable_slot {
                        self.select_placeable_tool(slot_idx);
                    }
                }

                if self.is_free_look_camera_mode() {
                    self.tracer.handle_keyboard(&event);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_orbit_mouse_drag(Vec2::new(position.x as f32, position.y as f32));
                self.try_update_pipe_drag_preview();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let captured = self.set_orbit_mouse_drag_state(button, state);
                if captured {
                    return;
                }

                self.set_tool_mouse_button_state(button, state);

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
                            } else if self.is_watering_selected() && button == MouseButton::Left {
                                self.player_tools.shovel_dig_held = true;
                                self.player_tools.last_watering_time = None;
                                self.try_watering_brush(now);
                            } else if self.is_fertilizer_selected() && button == MouseButton::Left {
                                self.player_tools.shovel_dig_held = true;
                                self.player_tools.last_fertilizing_time = None;
                                self.try_fertilizer_brush(now);
                            } else if self.is_tiller_selected() && button == MouseButton::Left {
                                self.player_tools.shovel_dig_held = true;
                                self.player_tools.last_tilling_time = None;
                                self.try_tiller_brush(now);
                            } else if self.is_place_tool_selected() && button == MouseButton::Left {
                                self.stop_terrain_edit_loop_sound();
                                self.try_placeable_placement();
                            } else if self.is_place_tool_selected() && button == MouseButton::Right
                            {
                                self.cancel_pipe_drag();
                            }
                        }
                        ElementState::Released => {
                            if button == MouseButton::Left {
                                self.try_finish_pipe_drag();
                            }
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
                if self.loading_state.is_none() {
                    self.terrain_physics
                        .process_terrain_collider_updates(&self.contree_builder);
                }
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
                    } else if self.is_watering_selected() && self.player_tools.left_mouse_held {
                        self.try_watering_brush(now);
                    } else if self.is_fertilizer_selected() && self.player_tools.left_mouse_held {
                        self.try_fertilizer_brush(now);
                    } else if self.is_tiller_selected() && self.player_tools.left_mouse_held {
                        self.try_tiller_brush(now);
                    } else {
                        self.stop_terrain_edit_loop_sound();
                    }
                }
                let frame_delta_time = self.time_info.delta_time();
                if let Err(err) = self
                    .terrain_physics
                    .advance_dynamic_bodies(frame_delta_time, &mut self.tracer)
                {
                    log::error!("Failed to advance dynamic bodies: {err:#}");
                }
                let fruit_refresh_tree_ids =
                    self.terrain_physics.take_attached_fruit_refresh_trees();
                if let Err(err) = self.refresh_attached_tree_fruits(&fruit_refresh_tree_ids) {
                    log::error!("Failed to refresh attached fruits after detachment: {err:#}");
                }
                let time_since_start = self.time_info.time_since_start();
                let world_tick_seconds = crate::game_time::clamp_world_tick_seconds(
                    self.debug_settings.adjustables.world_tick_seconds.value,
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
                let active_wind_sources =
                    GuiAdjustables::active_wind_sources(&self.debug_settings.wind_sources);
                if let Err(err) = self.tree_audio_manager.update(
                    time_since_start,
                    &active_wind_sources,
                    self.debug_settings
                        .adjustables
                        .wind_audio_attack_decay
                        .value,
                    self.debug_settings
                        .adjustables
                        .wind_audio_release_decay
                        .value,
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
                let time_of_day_before_gui = self.debug_settings.adjustables.time_of_day.value;
                let tree_age_before_gui = self.debug_settings.adjustables.tree_age.value;
                let fruit_cycle_before_gui = self.debug_settings.adjustables.fruit_cycle.value;
                let vsm_blur_radius_before_gui =
                    self.debug_settings.adjustables.vsm_blur_radius.value;
                let item_panel_shovel_icon = self.item_panel_shovel_icon.clone();
                let item_panel_smooth_icon = self.item_panel_smooth_icon.clone();
                let item_panel_staff_icon = self.item_panel_staff_icon.clone();
                let item_panel_hoe_icon = self.item_panel_hoe_icon.clone();
                let item_panel_tree_icon = self.item_panel_tree_icon.clone();
                let item_panel_water_icon = self.item_panel_water_icon.clone();
                let item_panel_sprinkler_icon = self.item_panel_sprinkler_icon.clone();
                let item_panel_pipe_icon = self.item_panel_pipe_icon.clone();
                let item_panel_soil_inspector_icon = self.item_panel_soil_inspector_icon.clone();
                let item_panel_fertilizer_icon = self.item_panel_fertilizer_icon.clone();
                let item_panel_tiller_icon = self.item_panel_tiller_icon.clone();
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
                let flora_paint_selection_count = species::PLAYER_FLORA_PAINT_SELECTIONS.len();
                let current_flora_paint_selection_index = if flora_paint_selection_count == 0 {
                    0
                } else {
                    self.player_tools.flora_paint_selection_index % flora_paint_selection_count
                };
                let flora_paint_panel_entries: Vec<FloraPaintPanelEntry> =
                    species::PLAYER_FLORA_PAINT_SELECTIONS
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(index, selection)| FloraPaintPanelEntry {
                            index,
                            label: species::flora_paint_selection_label(selection),
                            selected: index == current_flora_paint_selection_index,
                        })
                        .collect();
                let growing_flora_chunk_count = self.growing_flora_chunks.len();
                let mut camera_snapshot_to_apply = None;
                let mut clicked_item_panel_slot = None;
                let mut clicked_flora_paint_selection_index = None;
                let collision_probe_ready = self.terrain_physics.collision_probe_ready();
                let collision_probe_active = self.terrain_physics.collision_probe_active();
                let collision_probe_status = self.terrain_physics.collision_probe_status();
                let mut drop_collision_probe_requested = false;
                let mut clear_collision_probe_requested = false;
                let environment_probe_status = self.tracer.environment_probe_status();
                let environment_probe_draft_grid = DdgiVolumeGrid::new(
                    CHUNK_DIM * VOXEL_DIM_PER_CHUNK,
                    self.environment_probe_spacing_draft,
                )
                .expect("environment probe UI only exposes supported spacings");
                let environment_probe_draft_bytes =
                    DdgiResourceBytes::for_grid(environment_probe_draft_grid)
                        .expect("environment probe UI grid must produce valid DDGI atlases");
                let mut environment_probe_visualization =
                    self.tracer.environment_probe_visualization_settings();
                let mut environment_probe_rebuild_requested = false;

                let current_camera_pose = self.tracer.camera_pose();
                let terrain_edit_hover = self.terrain_edit_hover();
                let show_tree_preview = self.is_place_tool_selected()
                    && self.current_placeable_kind() == placeables::PlaceableKind::Tree;
                if show_tree_preview {
                    if let Some(hover) = terrain_edit_hover {
                        let tint = if hover.is_editable {
                            Vec4::ONE
                        } else {
                            Vec4::new(5.0, 0.08, 0.08, 1.0)
                        };
                        if let Err(err) = self.tracer.show_tree_geometry_preview(hover.center, tint)
                        {
                            self.tracer.clear_tree_geometry_preview();
                            log::error!("Failed to position tree geometry preview: {err}");
                        }
                    } else {
                        self.tracer.clear_tree_geometry_preview();
                    }
                } else {
                    self.tracer.clear_tree_geometry_preview();
                }
                let water_status_text = self
                    .water_sim
                    .status_text(self.water_particle_handoff_main_thread_ms);
                let placeable_hint = format!(
                    "Place: {} (Z/X or bottom bar) · Water: 6 + LMB · Inspector: 7 · Fert: 8 + LMB · Till: 9 + LMB · sprinklers {}",
                    self.current_placeable_label(),
                    self.sprinkler_records.len()
                );
                let soil_inspector_panel_text = if self.is_soil_inspector_selected() {
                    Some(match terrain_edit_hover {
                        Some(hover) if hover.is_editable => {
                            let radius = self.player_tools.terrain_edit_radius;
                            match (
                                self.plain_builder
                                    .sample_soil_moisture_sphere(hover.center, radius),
                                self.plain_builder
                                    .sample_soil_fertility_sphere(hover.center, radius),
                            ) {
                                (Ok(moisture), Ok(fertility)) if moisture.count > 0 => format!(
                                    "avg humidity {:.2}/{}\navg fertility {:.2}/{}",
                                    moisture.average().unwrap_or(0.0),
                                    VOXEL_MOISTURE_MAX,
                                    fertility.average().unwrap_or(0.0),
                                    VOXEL_FERTILITY_MAX
                                ),
                                (Ok(_), Ok(_)) => "nothing to inspect".to_string(),
                                (Err(err), _) | (_, Err(err)) => {
                                    log::error!("Inspector sample failed: {}", err);
                                    "sample failed".to_string()
                                }
                            }
                        }
                        Some(_) => "outside editable area".to_string(),
                        None => "point at terrain".to_string(),
                    })
                } else {
                    None
                };
                let soil_inspector_panel_pos = soil_inspector_panel_text.as_ref().map(|_| {
                    let extent = self.window_state.window_extent();
                    let screen_center =
                        Vec2::new(extent.width as f32 * 0.5, extent.height as f32 * 0.5);
                    let cursor = if self.window_state.is_cursor_visible() {
                        self.cursor_position_physical.unwrap_or(screen_center)
                    } else {
                        screen_center
                    };
                    let scale_factor = self.window_state.window().scale_factor() as f32;
                    egui::pos2(cursor.x / scale_factor + 18.0, cursor.y / scale_factor)
                });
                let status_bar_text = format!("{}\n{}", water_status_text, placeable_hint);
                let terrain_edit_preview_center = terrain_edit_hover.map(|hover| hover.center);
                let terrain_edit_preview_shape = self.terrain_edit_preview_shape();
                let terrain_edit_preview_color = self.terrain_edit_preview_color(
                    terrain_edit_hover
                        .map(|hover| hover.is_editable)
                        .unwrap_or(true),
                );
                let current_camera_is_free_fly = self.is_free_fly_camera_mode();
                let hide_ui_for_environment_test_capture =
                    (self.environment_lighting_test_scene.is_some()
                        || self.hybrid_transparency_test_scene.is_some())
                        && (self.screenshot_path.is_some() || self.denoiser_bench.is_some());
                let egui_start = Instant::now();
                self.egui_renderer
                    .update(&self.window_state.window(), |ctx| {
                        let mut style = (*ctx.global_style()).clone();
                        apply_gui_style(&mut style);
                        ctx.set_global_style(style);

                        if hide_ui_for_environment_test_capture {
                            return;
                        }

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
                                                    match self.debug_settings.save() {
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
                                    if let Some(status) = self.debug_settings.save_status() {
                                        ui.small(status);
                                    }

                                    ui.add_space(4.0);
                                    ui.separator();
                                    ui.add_space(4.0);

                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false; 2])
                                        .scroll_source(
                                            egui::containers::scroll_area::ScrollSource::MOUSE_WHEEL,
                                        )
                                        .show(ui, |ui| {
                                            tree_desc_changed |= self.debug_settings.draw(ui);

                                            ui.add_space(8.0);
                                            ui.separator();
                                            ui.add_space(8.0);
                                            ui.heading(
                                                RichText::new("Environment Probes")
                                                    .size(16.0)
                                                    .color(GOLD_ACCENT),
                                            );
                                            egui::ComboBox::from_label("Spacing (voxels)")
                                                .selected_text(
                                                    self.environment_probe_spacing_draft.to_string(),
                                                )
                                                .show_ui(ui, |ui| {
                                                    for spacing in
                                                        SUPPORTED_DDGI_SPACINGS_VOXELS
                                                    {
                                                        ui.selectable_value(
                                                            &mut self
                                                                .environment_probe_spacing_draft,
                                                            spacing,
                                                            spacing.to_string(),
                                                        );
                                                    }
                                                });
                                            let grid = environment_probe_status.grid;
                                            let bytes =
                                                environment_probe_status.resource_bytes;
                                            ui.monospace(format!(
                                                "Current {} vox · {} x {} x {} · {}/{} filtered · {:?}",
                                                grid.spacing_voxels(),
                                                grid.dimensions().x,
                                                grid.dimensions().y,
                                                grid.dimensions().z,
                                                environment_probe_status.filtered_probe_count,
                                                grid.probe_count(),
                                                environment_probe_status.stage,
                                            ));
                                            ui.monospace(format!(
                                                "Revisions: sky {} · terrain {}",
                                                environment_probe_status.global_sky_revision,
                                                environment_probe_status
                                                    .relocated_terrain_revision
                                                    .map_or_else(|| "pending".to_owned(), |value| value.to_string()),
                                            ));
                                            ui.monospace(format!(
                                                "Allocated {:.2} MiB (irradiance {:.2} + visibility {:.2} + metadata {:.2} + rays {:.2} + sky {:.4} + stats {:.4})",
                                                bytes.total() as f64 / (1024.0 * 1024.0),
                                                bytes.irradiance_atlas as f64 / (1024.0 * 1024.0),
                                                bytes.visibility_atlas as f64 / (1024.0 * 1024.0),
                                                bytes.probe_metadata as f64 / (1024.0 * 1024.0),
                                                bytes.transient_ray_data as f64 / (1024.0 * 1024.0),
                                                bytes.global_sky_irradiance as f64 / (1024.0 * 1024.0),
                                                bytes.trace_stats as f64 / (1024.0 * 1024.0),
                                            ));
                                            if environment_probe_draft_grid != grid {
                                                ui.monospace(format!(
                                                    "Selected {} x {} x {} · {} probes · {:.2} MiB",
                                                    environment_probe_draft_grid.dimensions().x,
                                                    environment_probe_draft_grid.dimensions().y,
                                                    environment_probe_draft_grid.dimensions().z,
                                                    environment_probe_draft_grid.probe_count(),
                                                    environment_probe_draft_bytes.total() as f64
                                                        / (1024.0 * 1024.0),
                                                ));
                                            }
                                            environment_probe_rebuild_requested = ui
                                                .add_enabled(
                                                    self.environment_probe_spacing_draft
                                                        != grid.spacing_voxels(),
                                                    egui::Button::new("Apply / Rebuild"),
                                                )
                                                .clicked();
                                            ui.checkbox(
                                                &mut environment_probe_visualization.enabled,
                                                "Visualize probes",
                                            );
                                            ui.add_enabled_ui(
                                                environment_probe_visualization.enabled,
                                                |ui| {
                                                    egui::ComboBox::from_label("Display")
                                                        .selected_text(
                                                            environment_probe_visualization
                                                                .mode
                                                                .label(),
                                                        )
                                                        .show_ui(ui, |ui| {
                                                            for mode in
                                                                EnvironmentProbeVisualizationMode::ALL
                                                            {
                                                                ui.selectable_value(
                                                                    &mut
                                                                        environment_probe_visualization
                                                                            .mode,
                                                                    mode,
                                                                    mode.label(),
                                                                );
                                                            }
                                                        });
                                                    egui::ComboBox::from_label("Filter")
                                                        .selected_text(
                                                            environment_probe_visualization
                                                                .filter
                                                                .label(),
                                                        )
                                                        .show_ui(ui, |ui| {
                                                            for filter in
                                                                EnvironmentProbeVisualizationFilter::ALL
                                                            {
                                                                ui.selectable_value(
                                                                    &mut
                                                                        environment_probe_visualization
                                                                            .filter,
                                                                    filter,
                                                                    filter.label(),
                                                                );
                                                            }
                                                        });
                                                    ui.add(
                                                        egui::Slider::new(
                                                            &mut
                                                                environment_probe_visualization
                                                                    .camera_radius_voxels,
                                                            0.0..=512.0,
                                                        )
                                                        .text("Camera radius (vox; 0 = all)"),
                                                    );
                                                    ui.add(
                                                        egui::Slider::new(
                                                            &mut
                                                                environment_probe_visualization
                                                                    .instance_stride,
                                                            1..=64,
                                                        )
                                                        .text("Instance stride"),
                                                    );
                                                    ui.add(
                                                        egui::Slider::new(
                                                            &mut
                                                                environment_probe_visualization
                                                                    .marker_size_voxels,
                                                            0.5..=12.0,
                                                        )
                                                        .text("Marker size (voxels)"),
                                                    );
                                                    ui.checkbox(
                                                        &mut environment_probe_visualization
                                                            .depth_tested,
                                                        "Depth tested",
                                                    );
                                                    ui.monospace(format!(
                                                        "Submitted instances: {}",
                                                        environment_probe_visualization
                                                            .submitted_instance_count(
                                                                grid.probe_count()
                                                            ),
                                                    ));
                                                },
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
                                                current_camera_is_free_fly,
                                            );

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

                                        });
                                });
                        }
                        self.config_panel_visible = config_panel_open;

                        egui::Area::new("collision_probe_panel".into())
                            .order(egui::Order::Foreground)
                            .anchor(
                                egui::Align2::CENTER_TOP,
                                egui::Vec2::new(0.0, 16.0),
                            )
                            .show(ctx, |ui| {
                                let probe_frame = egui::containers::Frame {
                                    fill: PANEL_DARK,
                                    inner_margin: egui::Margin::symmetric(12, 10),
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

                                probe_frame.show(ui, |ui| {
                                    ui.set_min_width(220.0);
                                    ui.label(
                                        RichText::new("Collision Probe")
                                            .color(GOLD_ACCENT)
                                            .monospace()
                                            .size(12.0),
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new(collision_probe_status.as_str())
                                            .color(SAGE_ACCENT)
                                            .monospace()
                                            .size(11.0),
                                    );
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add_enabled(
                                                collision_probe_ready,
                                                egui::Button::new("Drop Apple Probe"),
                                            )
                                            .clicked()
                                        {
                                            drop_collision_probe_requested = true;
                                        }
                                        if ui
                                            .add_enabled(
                                                collision_probe_active,
                                                egui::Button::new("Clear"),
                                            )
                                            .clicked()
                                        {
                                            clear_collision_probe_requested = true;
                                        }
                                    });
                                });
                            });

                        let item_panel_slots = [
                            ItemPanelSlot {
                                index: HAND_SLOT_INDEX,
                                label: "Hand",
                                key_hint: "1",
                                category: Some("FREE"),
                                icon: None,
                                accent: SAGE_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: STAFF_SLOT_INDEX,
                                label: "Grow",
                                key_hint: "2",
                                category: Some("TOOLS"),
                                icon: item_panel_staff_icon.as_ref(),
                                accent: STAFF_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: SHOVEL_SLOT_INDEX,
                                label: "Dig",
                                key_hint: "3",
                                category: Some("TOOLS"),
                                icon: item_panel_shovel_icon.as_ref(),
                                accent: SHOVEL_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: SMOOTH_SLOT_INDEX,
                                label: "Smooth",
                                key_hint: "4",
                                category: Some("TOOLS"),
                                icon: item_panel_smooth_icon.as_ref(),
                                accent: SMOOTH_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: HOE_SLOT_INDEX,
                                label: "Trim",
                                key_hint: "5",
                                category: Some("TOOLS"),
                                icon: item_panel_hoe_icon.as_ref(),
                                accent: HOE_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: WATERING_SLOT_INDEX,
                                label: "Water",
                                key_hint: "6",
                                category: Some("CARE"),
                                icon: item_panel_water_icon.as_ref(),
                                accent: WATER_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: FERTILIZER_SLOT_INDEX,
                                label: "Fert",
                                key_hint: "8",
                                category: Some("CARE"),
                                icon: item_panel_fertilizer_icon.as_ref(),
                                accent: FERTILIZER_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: TILLER_SLOT_INDEX,
                                label: "Till",
                                key_hint: "9",
                                category: Some("CARE"),
                                icon: item_panel_tiller_icon.as_ref(),
                                accent: TILLER_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: SOIL_INSPECTOR_SLOT_INDEX,
                                label: "Inspector",
                                key_hint: "7",
                                category: Some("SCAN"),
                                icon: item_panel_soil_inspector_icon.as_ref(),
                                accent: SOIL_INSPECTOR_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: TREE_SLOT_INDEX,
                                label: "Tree",
                                key_hint: "Z",
                                category: Some("ITEMS"),
                                icon: item_panel_tree_icon.as_ref(),
                                accent: TREE_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: SPRINKLER_SLOT_INDEX,
                                label: "Spray",
                                key_hint: "X",
                                category: Some("ITEMS"),
                                icon: item_panel_sprinkler_icon.as_ref(),
                                accent: WATER_TOOL_ACCENT,
                                enabled: true,
                            },
                            ItemPanelSlot {
                                index: PIPE_SLOT_INDEX,
                                label: "Pipe",
                                key_hint: "V",
                                category: Some("ITEMS"),
                                icon: item_panel_pipe_icon.as_ref(),
                                accent: WATER_TOOL_ACCENT,
                                enabled: true,
                            },
                        ];
                        let selected_item_panel_display_slot = if selected_item_panel_slot
                            == Some(TREE_SLOT_INDEX)
                            && selected_placeable_panel_slot == SPRINKLER_PLACEABLE_SLOT_INDEX
                        {
                            Some(SPRINKLER_SLOT_INDEX)
                        } else if selected_item_panel_slot == Some(TREE_SLOT_INDEX)
                            && selected_placeable_panel_slot == PIPE_PLACEABLE_SLOT_INDEX
                        {
                            Some(PIPE_SLOT_INDEX)
                        } else {
                            selected_item_panel_slot.or(Some(HAND_SLOT_INDEX))
                        };
                        let item_panel_response = draw_item_panel(
                            ctx,
                            &item_panel_slots,
                            selected_item_panel_display_slot,
                            self.window_state.is_cursor_visible(),
                        );
                        clicked_item_panel_slot = item_panel_response.clicked_slot;

                        if selected_item_panel_slot == Some(STAFF_SLOT_INDEX) {
                            let flora_paint_panel_response = draw_flora_paint_panel(
                                ctx,
                                &flora_paint_panel_entries,
                                self.window_state.is_cursor_visible(),
                            );
                            clicked_flora_paint_selection_index =
                                flora_paint_panel_response.clicked_selection_index;
                        }

                        let voxel_palette_response =
                            draw_voxel_palette(ctx, &voxel_palette_entries, false);
                        self.player_tools.backpack_summary_panel_screen_pos =
                            voxel_palette_response
                                .panel_center
                                .map(|center| Vec2::new(center.x, center.y));

                        if let (Some(panel_pos), Some(panel_text)) = (
                            soil_inspector_panel_pos,
                            soil_inspector_panel_text.as_deref(),
                        ) {
                            egui::Area::new("soil_inspector_panel".into())
                                .order(egui::Order::Foreground)
                                .fixed_pos(panel_pos)
                                .interactable(false)
                                .show(ctx, |ui| {
                                    let inspector_frame = egui::containers::Frame {
                                        fill: PANEL_DARK,
                                        inner_margin: egui::Margin::symmetric(10, 8),
                                        corner_radius: egui::CornerRadius::same(0),
                                        shadow: egui::epaint::Shadow {
                                            offset: [4, 4],
                                            blur: 0,
                                            spread: 0,
                                            color: SHADOW_COLOR,
                                        },
                                        stroke: egui::Stroke::new(2.0, SOIL_INSPECTOR_TOOL_ACCENT),
                                        ..Default::default()
                                    };

                                    inspector_frame.show(ui, |ui| {
                                        ui.set_min_width(150.0);
                                        ui.label(
                                            RichText::new("Inspector")
                                                .color(GOLD_ACCENT)
                                                .monospace()
                                                .size(12.0),
                                        );
                                        ui.add_space(4.0);
                                        ui.label(
                                            RichText::new(panel_text)
                                                .color(SAGE_ACCENT)
                                                .monospace()
                                                .size(11.0),
                                        );
                                    });
                                });
                        }

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

                        if self.card_display_visible {
                            draw_center_card(ctx);
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

                        draw_center_cross_mark(ctx);
                    });
                let egui_ms = egui_start.elapsed().as_secs_f32() * 1000.0;
                if clear_collision_probe_requested {
                    self.terrain_physics.clear_collision_probe(&mut self.tracer);
                }
                if drop_collision_probe_requested {
                    if let Err(err) = self.terrain_physics.drop_collision_probe(&mut self.tracer) {
                        log::error!("Failed to drop collision probe: {err:#}");
                    }
                }
                if environment_probe_rebuild_requested {
                    if let Err(err) = self
                        .tracer
                        .rebuild_environment_probes(self.environment_probe_spacing_draft)
                    {
                        log::error!("Failed to rebuild environment probes: {err:#}");
                    }
                }
                self.tracer
                    .set_environment_probe_visualization_settings(environment_probe_visualization);
                self.sync_cursor_with_panels();
                if let Some(slot_idx) = clicked_item_panel_slot {
                    self.select_item_panel_slot(slot_idx);
                }
                if let Some(selection_idx) = clicked_flora_paint_selection_index {
                    self.select_flora_paint_selection_index(selection_idx);
                }
                if self.gui_wants_keyboard_input() {
                    self.reset_camera_movement_input();
                }
                if let Some(snapshot) = camera_snapshot_to_apply {
                    self.apply_camera_snapshot(&snapshot);
                }
                let tree_age_changed =
                    self.debug_settings.adjustables.tree_age.value != tree_age_before_gui;
                let fruit_cycle_changed =
                    self.debug_settings.adjustables.fruit_cycle.value != fruit_cycle_before_gui;
                if tree_desc_changed && tree_age_changed {
                    if !self.stage_tuned_tree_desc_from_gui() {
                        if let Err(err) = self.update_tuned_tree_from_gui() {
                            log::error!("Failed to update tuning tree from GUI sliders: {err}");
                        }
                    }
                } else if tree_desc_changed {
                    match self.update_tuned_tree_from_gui() {
                        Ok(()) => log::info!(
                            "Updated tuning tree from GUI sliders at age {:.3}",
                            self.debug_settings.adjustables.tree_age.value,
                        ),
                        Err(err) => {
                            log::error!("Failed to update tuning tree from GUI sliders: {}", err)
                        }
                    }
                }
                if tree_age_changed {
                    if let Err(err) = self.update_all_tree_ages_from_gui() {
                        log::error!("Failed to rebuild trees for global age: {err:#}");
                    }
                }
                if fruit_cycle_changed {
                    if let Err(err) = self.terrain_physics.set_fruit_cycle(
                        self.debug_settings.adjustables.fruit_cycle.value,
                        &mut self.tracer,
                    ) {
                        log::error!("Failed to update fruit lifecycle cycle: {err:#}");
                    }
                    let fruit_refresh_tree_ids =
                        self.terrain_physics.take_attached_fruit_refresh_trees();
                    if let Err(err) = self.refresh_attached_tree_fruits(&fruit_refresh_tree_ids) {
                        log::error!("Failed to refresh attached fruits after cycle scrub: {err:#}");
                    }
                }
                if tree_desc_changed || tree_age_changed {
                    if let Err(err) = self.sync_tree_placement_preview_from_gui() {
                        log::error!("Failed to rebuild tree geometry preview: {err}");
                    }
                }

                self.apply_effective_master_volume_gain("Failed to apply master volume");
                if let Err(err) = self
                    .tree_audio_manager
                    .set_wind_volume_db(self.debug_settings.adjustables.tree_wind_volume_db.value)
                {
                    log::error!("Failed to apply tree wind volume: {}", err);
                }
                self.tree_audio_manager.set_wind_response_curve(
                    Self::tree_audio_wind_response_curve(&self.debug_settings.adjustables),
                );
                self.tree_audio_manager
                    .set_rustle_params(Self::tree_rustle_params(&self.debug_settings.adjustables));

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
                self.process_environment_lighting_test_scene();
                self.process_hybrid_transparency_test_scene();
                if self.deferred_chunk_rebuilds_idle() {
                    if let Err(err) = self.tracer.start_pending_environment_probe_refresh() {
                        log::error!(
                            "Failed to start DDGI refresh after terrain publication: {err:#}"
                        );
                    }
                }

                if self.render_start_time.is_some() {
                    if let Some(spacing_voxels) =
                        self.environment_probe_rebuild_spacing_voxels.take()
                    {
                        log::info!(
                            "[DDGI][RUNTIME_REBUILD] requested spacing_voxels={spacing_voxels}"
                        );
                        match self.tracer.rebuild_environment_probes(spacing_voxels) {
                            Ok(()) => log::info!(
                                "[DDGI][RUNTIME_REBUILD] complete spacing_voxels={spacing_voxels}"
                            ),
                            Err(err) => log::error!(
                                "[DDGI][RUNTIME_REBUILD] failed spacing_voxels={spacing_voxels}: {err:#}"
                            ),
                        }
                    }
                }

                let mut sun_update_ticks = 0;
                if self.debug_settings.adjustables.auto_daynight_cycle.value && world_tick_steps > 0
                {
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
                    self.debug_settings.adjustables.time_of_day.value != time_of_day_before_gui;
                let vsm_blur_radius_changed_by_gui =
                    self.debug_settings.adjustables.vsm_blur_radius.value
                        != vsm_blur_radius_before_gui;
                if time_of_day_changed_by_gui {
                    self.current_time_of_day = self.debug_settings.adjustables.time_of_day.value;
                }
                if time_of_day_changed_by_gui || vsm_blur_radius_changed_by_gui {
                    self.request_vsm_history_reset();
                }

                // update sun position if auto day/night cycle is enabled
                let sun_position_updated = sun_update_ticks > 0;
                if sun_position_updated {
                    self.current_time_of_day = advance_time_of_day(
                        self.current_time_of_day,
                        sun_update_ticks,
                        world_tick_seconds,
                        self.debug_settings.adjustables.day_cycle_minutes.value,
                    );
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

                self.apply_denoiser_benchmark_camera_motion();

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
                    self.current_time_of_day,
                    self.debug_settings.adjustables.latitude.value,
                    self.debug_settings.adjustables.season.value,
                );
                let sun_dir = get_sun_dir(sun_altitude.asin().to_degrees(), sun_azimuth * 360.0);

                if !self.sprinkler_records.is_empty() {
                    let sprinkler_moisture_gpu_scope =
                        self.gpu_profiler.as_mut().and_then(|profiler| {
                            profiler.begin_scope(
                                frame_slot,
                                cmdbuf,
                                "sprinkler_moisture.pass",
                                PipelineStage::COMPUTE_SHADER,
                            )
                        });
                    self.record_sprinkler_moisture(cmdbuf, frame_delta_time);
                    if let Some(scope) = sprinkler_moisture_gpu_scope {
                        if let Some(profiler) = self.gpu_profiler.as_mut() {
                            profiler.end_scope(
                                frame_slot,
                                cmdbuf,
                                scope,
                                PipelineStage::COMPUTE_SHADER,
                            );
                        }
                    }
                }

                if self.has_terrain_moisture_spread_chunks() {
                    let moisture_spread_gpu_scope =
                        self.gpu_profiler.as_mut().and_then(|profiler| {
                            profiler.begin_scope(
                                frame_slot,
                                cmdbuf,
                                "moisture_spread.pass",
                                PipelineStage::COMPUTE_SHADER,
                            )
                        });
                    self.record_terrain_moisture_spread_chunks(cmdbuf);
                    if let Some(scope) = moisture_spread_gpu_scope {
                        if let Some(profiler) = self.gpu_profiler.as_mut() {
                            profiler.end_scope(
                                frame_slot,
                                cmdbuf,
                                scope,
                                PipelineStage::COMPUTE_SHADER,
                            );
                        }
                    }
                }

                self.render_flags.enable_leaves =
                    self.render_flags.enable_flora && self.debug_settings.tree.render_leaves;
                let update_shadow_map = self.render_flags.enable_shadows;
                let wind_gui_params = Self::wind_gui_params(&self.debug_settings.wind_sources);
                let cloud_gui_params = CloudGuiParams {
                    // Disabled for now; infrastructure kept for easy re-enable.
                    enabled: false,
                    coverage: self.debug_settings.adjustables.cloud_coverage.value,
                    density: self.debug_settings.adjustables.cloud_density.value,
                    bottom_height: self.debug_settings.adjustables.cloud_bottom_height.value,
                    top_height: self.debug_settings.adjustables.cloud_top_height.value,
                    shape_scale: self.debug_settings.adjustables.cloud_shape_scale.value,
                    detail_scale: self.debug_settings.adjustables.cloud_detail_scale.value,
                    detail_strength: self.debug_settings.adjustables.cloud_detail_strength.value,
                    wind_speed: self.debug_settings.adjustables.cloud_wind_speed.value,
                    primary_steps: self.debug_settings.adjustables.cloud_primary_steps.value,
                    light_steps: self.debug_settings.adjustables.cloud_light_steps.value,
                    temporal_alpha: self.debug_settings.adjustables.cloud_temporal_alpha.value,
                    absorption: self.debug_settings.adjustables.cloud_absorption.value,
                    phase_eccentricity: self
                        .debug_settings
                        .adjustables
                        .cloud_phase_eccentricity
                        .value,
                    silver_intensity: self.debug_settings.adjustables.cloud_silver_intensity.value,
                    max_distance: self.debug_settings.adjustables.cloud_max_distance.value,
                    // Disabled for now; restore original expression to re-enable.
                    shadows_enabled: false,
                    shadow_strength: self.debug_settings.adjustables.cloud_shadow_strength.value,
                    shadow_min_transmittance: self
                        .debug_settings
                        .adjustables
                        .cloud_shadow_min_transmittance
                        .value,
                    shadow_steps: self.debug_settings.adjustables.cloud_shadow_steps.value,
                };

                self.tracer
                    .update_buffers(
                        &self.time_info,
                        self.debug_settings
                            .adjustables
                            .flora_growth_override_enabled
                            .value,
                        self.debug_settings.adjustables.flora_growth_override.value,
                        self.debug_settings
                            .adjustables
                            .terrain_self_shadow_tolerance_voxels
                            .value,
                        Vec3::new(
                            self.debug_settings
                                .adjustables
                                .flora_instance_hue_offset
                                .value,
                            self.debug_settings
                                .adjustables
                                .flora_instance_saturation_offset
                                .value,
                            self.debug_settings
                                .adjustables
                                .flora_instance_value_offset
                                .value,
                        ),
                        Vec3::new(
                            self.debug_settings.adjustables.flora_voxel_hue_offset.value,
                            self.debug_settings
                                .adjustables
                                .flora_voxel_saturation_offset
                                .value,
                            self.debug_settings
                                .adjustables
                                .flora_voxel_value_offset
                                .value,
                        ),
                        Vec3::new(
                            self.debug_settings
                                .adjustables
                                .grass_bottom_dark_color
                                .value
                                .r() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .grass_bottom_dark_color
                                .value
                                .g() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .grass_bottom_dark_color
                                .value
                                .b() as f32
                                / 255.0,
                        ),
                        Vec3::new(
                            self.debug_settings
                                .adjustables
                                .grass_bottom_light_color
                                .value
                                .r() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .grass_bottom_light_color
                                .value
                                .g() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .grass_bottom_light_color
                                .value
                                .b() as f32
                                / 255.0,
                        ),
                        Vec3::new(
                            self.debug_settings
                                .adjustables
                                .grass_tip_dark_color
                                .value
                                .r() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .grass_tip_dark_color
                                .value
                                .g() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .grass_tip_dark_color
                                .value
                                .b() as f32
                                / 255.0,
                        ),
                        Vec3::new(
                            self.debug_settings
                                .adjustables
                                .grass_tip_light_color
                                .value
                                .r() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .grass_tip_light_color
                                .value
                                .g() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .grass_tip_light_color
                                .value
                                .b() as f32
                                / 255.0,
                        ),
                        world_tick_seconds,
                        update_shadow_map,
                        self.debug_settings.adjustables.lens_flare_intensity.value,
                        self.debug_settings
                            .adjustables
                            .lens_flare_sun_pixel_scale
                            .value,
                        GlassGuiParams {
                            tint: Vec3::new(
                                self.debug_settings.adjustables.glass_tint.value.r() as f32 / 255.0,
                                self.debug_settings.adjustables.glass_tint.value.g() as f32 / 255.0,
                                self.debug_settings.adjustables.glass_tint.value.b() as f32 / 255.0,
                            ),
                            reflection_strength: self
                                .debug_settings
                                .adjustables
                                .glass_reflection_strength
                                .value,
                            ssr_strength: self.debug_settings.adjustables.glass_ssr_strength.value,
                            ssr_steps: self.debug_settings.adjustables.glass_ssr_steps.value,
                            per_voxel_reflection: self
                                .debug_settings
                                .adjustables
                                .glass_per_voxel_reflection
                                .value,
                            ssr_min_hit_thickness_voxels: self
                                .debug_settings
                                .adjustables
                                .glass_ssr_min_hit_thickness_voxels
                                .value,
                            ssr_footprint_pixels: self
                                .debug_settings
                                .adjustables
                                .glass_ssr_footprint_pixels
                                .value,
                            refraction_strength: self
                                .debug_settings
                                .adjustables
                                .glass_refraction_strength
                                .value,
                            alpha: self.debug_settings.adjustables.glass_alpha.value,
                            glint_strength: self
                                .debug_settings
                                .adjustables
                                .glass_glint_strength
                                .value,
                        },
                        self.debug_settings
                            .adjustables
                            .wind_directional_bias_fraction
                            .value,
                        self.debug_settings
                            .adjustables
                            .wind_turbulence_fraction
                            .value,
                        self.debug_settings
                            .adjustables
                            .grass_vibration_amplitude_voxels
                            .value,
                        self.debug_settings
                            .adjustables
                            .grass_vibration_primary_speed
                            .value,
                        self.debug_settings
                            .adjustables
                            .grass_vibration_secondary_speed
                            .value,
                        self.debug_settings
                            .adjustables
                            .grass_natural_bend_min_voxels
                            .value,
                        self.debug_settings
                            .adjustables
                            .grass_natural_bend_max_voxels
                            .value,
                        self.debug_settings
                            .adjustables
                            .flora_bend_height_power
                            .value,
                        KochiaMotionParams {
                            body_wind_response: self
                                .debug_settings
                                .adjustables
                                .kochia_body_wind_response
                                .value,
                            branch_jelly_amplitude_voxels: self
                                .debug_settings
                                .adjustables
                                .kochia_branch_jelly_amplitude_voxels
                                .value,
                            branch_jelly_speed: self
                                .debug_settings
                                .adjustables
                                .kochia_branch_jelly_speed
                                .value,
                            branch_phase_spread: self
                                .debug_settings
                                .adjustables
                                .kochia_branch_phase_spread
                                .value,
                            tip_flutter_amplitude_voxels: self
                                .debug_settings
                                .adjustables
                                .kochia_tip_flutter_amplitude_voxels
                                .value,
                            tip_flutter_speed: self
                                .debug_settings
                                .adjustables
                                .kochia_tip_flutter_speed
                                .value,
                        },
                        KochiaVisualParams {
                            bottom_darkening: self
                                .debug_settings
                                .adjustables
                                .kochia_bottom_darkening
                                .value,
                            branch_value_variation: self
                                .debug_settings
                                .adjustables
                                .kochia_branch_value_variation
                                .value,
                            voxel_value_variation: self
                                .debug_settings
                                .adjustables
                                .kochia_voxel_value_variation
                                .value,
                            branch_count: self.debug_settings.adjustables.kochia_branch_count.value,
                            bottom_diameter_voxels: self
                                .debug_settings
                                .adjustables
                                .kochia_bottom_diameter_voxels
                                .value,
                            waist_diameter_voxels: self
                                .debug_settings
                                .adjustables
                                .kochia_waist_diameter_voxels
                                .value,
                            top_diameter_voxels: self
                                .debug_settings
                                .adjustables
                                .kochia_top_diameter_voxels
                                .value,
                            waist_height: self.debug_settings.adjustables.kochia_waist_height.value,
                        },
                        self.debug_settings
                            .adjustables
                            .leaf_paddle_amplitude_voxels
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_paddle_primary_speed
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_paddle_secondary_speed
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_paddle_amplitude_wind_start_strength
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_paddle_amplitude_wind_full_strength
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_paddle_amplitude_wind_knee_bias
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_paddle_frequency_wind_start_strength
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_paddle_frequency_wind_full_strength
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_paddle_frequency_wind_knee_bias
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_paddle_frequency_min_multiplier
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_paddle_frequency_max_multiplier
                            .value,
                        FruitMotionParams {
                            swing_length_voxels: self
                                .debug_settings
                                .tree
                                .desc
                                .fruit_swing_length_voxels,
                            max_angle_radians: self
                                .debug_settings
                                .tree
                                .desc
                                .fruit_swing_max_angle_degrees
                                .to_radians(),
                            swing_speed: self.debug_settings.tree.desc.fruit_swing_speed,
                            speed_variation: self
                                .debug_settings
                                .tree
                                .desc
                                .fruit_swing_speed_variation,
                            min_response: self.debug_settings.tree.desc.fruit_swing_min_response,
                        },
                        self.debug_settings
                            .adjustables
                            .leaf_shadow_fragment_opacity
                            .value,
                        self.debug_settings.adjustables.leaf_shadow_strength.value,
                        self.debug_settings
                            .adjustables
                            .leaf_shadow_min_transmittance
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_shadow_filter_radius_texels
                            .value,
                        self.debug_settings
                            .adjustables
                            .leaf_transmission_strength
                            .value,
                        wind_gui_params,
                        cloud_gui_params,
                        self.flora_tick,
                        FLORA_SPROUT_DELAY_TICKS,
                        FLORA_FULL_GROWTH_TICKS,
                        self.time_info.time_since_start_duration().as_millis() as u32,
                        self.debug_settings
                            .adjustables
                            .flora_spawn_duration_seconds
                            .value,
                        self.debug_settings
                            .adjustables
                            .flora_spawn_rise_fraction
                            .value,
                        self.debug_settings
                            .adjustables
                            .flora_spawn_overshoot_min_voxels
                            .value,
                        self.debug_settings
                            .adjustables
                            .flora_spawn_overshoot_max_voxels
                            .value,
                        self.debug_settings
                            .adjustables
                            .flora_spawn_stagger_seconds
                            .value,
                        sun_dir,
                        self.debug_settings.adjustables.sun_size.value,
                        Vec3::new(
                            self.debug_settings.adjustables.sun_color.value.r() as f32 / 255.0,
                            self.debug_settings.adjustables.sun_color.value.g() as f32 / 255.0,
                            self.debug_settings.adjustables.sun_color.value.b() as f32 / 255.0,
                        ),
                        self.debug_settings.adjustables.sun_luminance.value,
                        self.debug_settings.adjustables.sun_display_luminance.value,
                        sun_altitude,
                        sun_azimuth,
                        self.debug_settings.adjustables.god_ray_max_depth.value,
                        self.debug_settings.adjustables.god_ray_max_checks.value,
                        self.debug_settings.adjustables.god_ray_weight.value,
                        Vec3::new(
                            self.debug_settings.adjustables.sun_color.value.r() as f32 / 255.0,
                            self.debug_settings.adjustables.sun_color.value.g() as f32 / 255.0,
                            self.debug_settings.adjustables.sun_color.value.b() as f32 / 255.0,
                        ),
                        self.debug_settings.adjustables.starlight_iterations.value,
                        self.debug_settings.adjustables.starlight_formuparam.value,
                        self.debug_settings.adjustables.starlight_volsteps.value,
                        self.debug_settings.adjustables.starlight_stepsize.value,
                        self.debug_settings.adjustables.starlight_zoom.value,
                        self.debug_settings.adjustables.starlight_tile.value,
                        self.debug_settings.adjustables.starlight_speed.value,
                        self.debug_settings.adjustables.starlight_brightness.value,
                        self.debug_settings.adjustables.starlight_darkmatter.value,
                        self.debug_settings.adjustables.starlight_distfading.value,
                        self.debug_settings.adjustables.starlight_saturation.value,
                        Vec3::new(
                            self.debug_settings.adjustables.voxel_dirt_color.value.r() as f32
                                / 255.0,
                            self.debug_settings.adjustables.voxel_dirt_color.value.g() as f32
                                / 255.0,
                            self.debug_settings.adjustables.voxel_dirt_color.value.b() as f32
                                / 255.0,
                        ),
                        Vec3::new(
                            self.debug_settings.adjustables.voxel_sand_color.value.r() as f32
                                / 255.0,
                            self.debug_settings.adjustables.voxel_sand_color.value.g() as f32
                                / 255.0,
                            self.debug_settings.adjustables.voxel_sand_color.value.b() as f32
                                / 255.0,
                        ),
                        Vec3::new(
                            self.debug_settings
                                .adjustables
                                .voxel_cherry_wood_color
                                .value
                                .r() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .voxel_cherry_wood_color
                                .value
                                .g() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .voxel_cherry_wood_color
                                .value
                                .b() as f32
                                / 255.0,
                        ),
                        Vec3::new(
                            self.debug_settings
                                .adjustables
                                .voxel_oak_wood_color
                                .value
                                .r() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .voxel_oak_wood_color
                                .value
                                .g() as f32
                                / 255.0,
                            self.debug_settings
                                .adjustables
                                .voxel_oak_wood_color
                                .value
                                .b() as f32
                                / 255.0,
                        ),
                        Vec3::new(
                            self.debug_settings.adjustables.voxel_rock_color.value.r() as f32
                                / 255.0,
                            self.debug_settings.adjustables.voxel_rock_color.value.g() as f32
                                / 255.0,
                            self.debug_settings.adjustables.voxel_rock_color.value.b() as f32
                                / 255.0,
                        ),
                        self.debug_settings.adjustables.voxel_color_variance.value,
                        terrain_edit_preview_center,
                        self.player_tools.terrain_edit_radius,
                        terrain_edit_preview_shape,
                        terrain_edit_preview_color,
                        TERRAIN_EDIT_PREVIEW_ALPHA,
                    )
                    .unwrap();

                let color_to_vec3 = |color: Color32| -> Vec3 {
                    Vec3::new(
                        color.r() as f32 / 255.0,
                        color.g() as f32 / 255.0,
                        color.b() as f32 / 255.0,
                    )
                };

                let mut flora_color_tables =
                    [solid_flora_height_color_tables(Vec3::ZERO, Vec3::ZERO);
                        species::MAX_FLORA_SPECIES];
                for (slot, desc) in flora_color_tables.iter_mut().zip(species::species()) {
                    *slot = match desc.key {
                        "tall_grass" | "short_grass" => grass_flora_height_color_tables(
                            color_to_vec3(
                                self.debug_settings
                                    .adjustables
                                    .grass_bottom_dark_color
                                    .value,
                            ),
                            color_to_vec3(
                                self.debug_settings
                                    .adjustables
                                    .grass_bottom_light_color
                                    .value,
                            ),
                            color_to_vec3(
                                self.debug_settings.adjustables.grass_tip_dark_color.value,
                            ),
                            color_to_vec3(
                                self.debug_settings.adjustables.grass_tip_light_color.value,
                            ),
                        ),
                        "ember_bloom" => allium_height_color_tables(
                            color_to_vec3(
                                self.debug_settings
                                    .adjustables
                                    .ember_bloom_bottom_color
                                    .value,
                            ),
                            color_to_vec3(
                                self.debug_settings
                                    .adjustables
                                    .ember_bloom_stem_tip_color
                                    .value,
                            ),
                            color_to_vec3(
                                self.debug_settings
                                    .adjustables
                                    .ember_bloom_flower_purple_color
                                    .value,
                            ),
                            color_to_vec3(
                                self.debug_settings
                                    .adjustables
                                    .ember_bloom_flower_secondary_color
                                    .value,
                            ),
                        ),
                        "kochia" => kochia_color_tables(
                            color_to_vec3(self.debug_settings.adjustables.kochia_color_a.value),
                            color_to_vec3(self.debug_settings.adjustables.kochia_color_b.value),
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
                            solid_flora_height_color_tables(
                                color_to_vec3(bottom),
                                color_to_vec3(tip),
                            )
                        }
                    };
                }
                let flora_color_tables = &flora_color_tables[..species::species_count()];

                let leaf_color_tables = solid_flora_height_color_tables(
                    color_to_vec3(self.debug_settings.adjustables.leaves_bottom_color.value),
                    color_to_vec3(self.debug_settings.adjustables.leaves_tip_color.value),
                );
                let reset_vsm_history = self.vsm_history_reset_pending;
                let vsm_temporal_alpha = Self::frame_rate_adjusted_vsm_temporal_alpha(
                    self.debug_settings.adjustables.vsm_temporal_alpha.value,
                    frame_delta_time,
                );
                let leaf_shadow_temporal_alpha = Self::frame_rate_adjusted_vsm_temporal_alpha(
                    self.debug_settings
                        .adjustables
                        .leaf_shadow_temporal_alpha
                        .value,
                    frame_delta_time,
                );
                let mut gpu_profiler_for_shadow = self.gpu_profiler.take();
                let shadow_prepass_gpu_scope =
                    gpu_profiler_for_shadow.as_mut().and_then(|profiler| {
                        profiler.begin_scope(
                            frame_slot,
                            cmdbuf,
                            "tracer.shadow_prepass",
                            PipelineStage::ALL_COMMANDS,
                        )
                    });
                self.tracer
                    .record_shadow_prepass(
                        cmdbuf,
                        self.surface_builder.get_resources(),
                        self.time_info.time_since_start(),
                        leaf_color_tables,
                        &self.render_flags,
                        update_shadow_map,
                        self.debug_settings.adjustables.vsm_blur_radius.value,
                        vsm_temporal_alpha,
                        leaf_shadow_temporal_alpha,
                        reset_vsm_history,
                        gpu_profiler_for_shadow.as_mut(),
                        frame_slot,
                    )
                    .unwrap();
                if let Some(scope) = shadow_prepass_gpu_scope {
                    if let Some(profiler) = gpu_profiler_for_shadow.as_mut() {
                        profiler.end_scope(frame_slot, cmdbuf, scope, PipelineStage::ALL_COMMANDS);
                    }
                }
                self.gpu_profiler = gpu_profiler_for_shadow;
                if update_shadow_map {
                    self.vsm_history_reset_pending = false;
                }

                if self.has_terrain_moisture_dry_chunks() {
                    let moisture_dry_gpu_scope = self.gpu_profiler.as_mut().and_then(|profiler| {
                        profiler.begin_scope(
                            frame_slot,
                            cmdbuf,
                            "moisture_dry.pass",
                            PipelineStage::COMPUTE_SHADER,
                        )
                    });
                    self.record_terrain_moisture_dry_chunks(
                        cmdbuf,
                        sun_dir,
                        DIRECT_SUN_SHADOW_SOURCE_ALL,
                        self.tracer.direct_sun_shadow_available_mask(),
                    );
                    if let Some(scope) = moisture_dry_gpu_scope {
                        if let Some(profiler) = self.gpu_profiler.as_mut() {
                            profiler.end_scope(
                                frame_slot,
                                cmdbuf,
                                scope,
                                PipelineStage::COMPUTE_SHADER,
                            );
                        }
                    }
                }

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
                    .record_trace_after_shadow_prepass(
                        cmdbuf,
                        self.surface_builder.get_resources(),
                        self.debug_settings.adjustables.lod_distance.value,
                        self.debug_settings.adjustables.flora_draw_distance.value,
                        self.debug_settings.adjustables.grass_render_mode.value,
                        self.time_info.time_since_start(),
                        flora_color_tables,
                        leaf_color_tables,
                        &self.render_flags,
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

                let mut environment_irradiance_readback = None;
                if !self.environment_irradiance_capture_taken {
                    if let Some(path) = self.environment_irradiance_capture_path.clone() {
                        let test_scene_ready = self
                            .environment_lighting_test_scene
                            .as_ref()
                            .is_none_or(
                            environment_lighting_test_scene::EnvironmentLightingTestScene::is_ready,
                        );
                        if test_scene_ready && self.tracer.ddgi_ready() {
                            match self.prepare_environment_irradiance_capture_readback(path.clone())
                            {
                                Ok(readback) => {
                                    self.record_environment_irradiance_capture_readback(
                                        cmdbuf, &readback,
                                    );
                                    self.environment_irradiance_capture_taken = true;
                                    environment_irradiance_readback = Some(readback);
                                    log::info!(
                                        "[ENV_IRRADIANCE_CAPTURE] recording backend={} path={}",
                                        "ddgi",
                                        path,
                                    );
                                }
                                Err(err) => log::error!(
                                    "[ENV_IRRADIANCE_CAPTURE] failed to prepare {}: {err:#}",
                                    path,
                                ),
                            }
                        }
                    }
                }

                let render_area = self.window_state.window_extent();
                let mut screenshot_readback = if self.screenshot_to_clipboard_requested {
                    self.screenshot_to_clipboard_requested = false;
                    match self.prepare_clipboard_screenshot_readback(render_area) {
                        Ok(readback) => Some(readback),
                        Err(err) => {
                            log::error!(
                                "[SCREENSHOT] Failed to prepare clipboard capture: {}",
                                err
                            );
                            None
                        }
                    }
                } else {
                    None
                };
                if screenshot_readback.is_none() && !self.screenshot_taken {
                    if let (Some(render_start_time), Some(path), Some(delay)) = (
                        self.render_start_time,
                        self.screenshot_path.clone(),
                        self.screenshot_delay,
                    ) {
                        let elapsed = render_start_time.elapsed().as_secs_f32();
                        let test_scene_ready = self
                            .environment_lighting_test_scene
                            .as_ref()
                            .is_none_or(
                            environment_lighting_test_scene::EnvironmentLightingTestScene::is_ready,
                        ) && self
                            .hybrid_transparency_test_scene
                            .as_ref()
                            .is_none_or(
                            hybrid_transparency_test_scene::HybridTransparencyTestScene::is_ready,
                        );
                        let environment_lighting_ready = self.tracer.ddgi_ready();
                        if elapsed >= delay && test_scene_ready && environment_lighting_ready {
                            self.screenshot_taken = true;
                            log::info!("[SCREENSHOT] Capturing after {:.2}s to {}", elapsed, path);
                            match self.prepare_screenshot_readback(path, render_area) {
                                Ok(readback) => screenshot_readback = Some(readback),
                                Err(err) => log::error!("[SCREENSHOT] Failed to prepare: {}", err),
                            }
                        }
                    }
                }
                if screenshot_readback.is_none()
                    && self
                        .denoiser_bench
                        .as_ref()
                        .is_some_and(DenoiserBench::should_capture)
                {
                    match self.prepare_denoiser_benchmark_readback(render_area) {
                        Ok(readback) => screenshot_readback = Some(readback),
                        Err(err) => panic!("[DENOISER_BENCH] Failed to prepare readback: {err:#}"),
                    }
                }

                self.swapchain.record_blit(
                    self.tracer.get_screen_output_tex().get_image(),
                    cmdbuf,
                    image_idx,
                    render_area,
                );
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
                let device = self.vulkan_ctx.device();
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

                let mut denoiser_bench_complete = false;
                if screenshot_readback.is_some() || environment_irradiance_readback.is_some() {
                    // Finish the GPU copy before handing CPU processing to a worker. Waiting on
                    // this frame's fence from the worker raced the frame manager resetting the
                    // same fence when its slot was reused.
                    match frame.wait_until_complete() {
                        Ok(()) => {
                            if let Some(readback) = environment_irradiance_readback.take() {
                                if let Err(err) =
                                    Self::write_environment_irradiance_capture_readback(readback)
                                {
                                    log::error!(
                                        "[ENV_IRRADIANCE_CAPTURE] failed to write capture: {err:#}"
                                    );
                                }
                            }
                            if let Some(readback) = screenshot_readback.take() {
                                if matches!(
                                    readback.destination,
                                    screenshot::ScreenshotDestination::DenoiserBenchmark
                                ) {
                                    let width = readback.width;
                                    let height = readback.height;
                                    let rgba = Self::read_screenshot_rgba(&readback)
                                        .unwrap_or_else(|err| {
                                            panic!("[DENOISER_BENCH] Failed to read frame: {err:#}")
                                        });
                                    denoiser_bench_complete = self
                                        .denoiser_bench
                                        .as_mut()
                                        .expect("benchmark readback requires benchmark state")
                                        .record_frame(width, height, &rgba)
                                        .unwrap_or_else(|err| {
                                            panic!(
                                                "[DENOISER_BENCH] Failed to record frame: {err:#}"
                                            )
                                        });
                                } else {
                                    std::thread::Builder::new()
                                        .name("screenshot-readback".to_owned())
                                        .spawn(move || Self::write_screenshot_readback(readback))
                                        .unwrap_or_else(|err| {
                                            log::error!(
                                                "[SCREENSHOT] Failed to start readback thread: {}",
                                                err
                                            );
                                            panic!(
                                                "failed to start screenshot readback thread: {err}"
                                            );
                                        });
                                }
                            }
                        }
                        Err(err) => {
                            log::error!("[READBACK] Failed while waiting for GPU readback: {}", err)
                        }
                    }
                }
                if let Some(benchmark) = self.denoiser_bench.as_mut() {
                    benchmark.mark_frame_presented();
                }

                self.tracer.set_footstep_volume_gain(
                    -40.0 + self.debug_settings.adjustables.footstep_volume_db.value,
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
                            if let Some(scene) = self
                                .environment_lighting_test_scene
                                .as_ref()
                                .filter(|scene| scene.edit_cycle_target_revision().is_some())
                            {
                                let status = self.tracer.environment_probe_status();
                                panic!(
                                    "[ENV_LIGHT_EDIT_CYCLE] timed out before completion phase={} target_revision={} ddgi_stage={:?} ddgi_relocated_terrain_revision={:?}",
                                    scene.phase_label(),
                                    scene.edit_cycle_target_revision().unwrap_or(0),
                                    status.stage,
                                    status.relocated_terrain_revision,
                                );
                            }
                            log::info!("[AUTO-EXIT] Exiting after {:.2}s", elapsed);
                            self.on_terminate(event_loop);
                        }
                    }
                }
                if denoiser_bench_complete {
                    self.on_terminate(event_loop);
                }
            }
            _ => (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        advance_time_of_day, App, CameraControlMode, MouseWheelDollySmoother, OrbitDeltaSmoother,
        OrbitKeyboardPanInput,
    };
    use glam::Vec3;
    use petalsonic::config::{AmbisonicsBackend, HrtfBackend};
    use winit::keyboard::KeyCode;

    #[test]
    fn day_night_clock_advances_without_mutating_its_persisted_start_value() {
        let persisted_start = 0.25;
        let current = advance_time_of_day(persisted_start, 600, 0.05, 1.0);

        assert_eq!(persisted_start, 0.25);
        assert!((current - 0.75).abs() < 1.0e-6);
    }

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
    fn camera_control_mode_defaults_to_orbit_edit() {
        assert_eq!(CameraControlMode::default(), CameraControlMode::OrbitEdit);
    }

    #[test]
    fn camera_control_mode_cycles_through_orbit_fly_and_walk() {
        assert_eq!(
            CameraControlMode::OrbitEdit.next(),
            CameraControlMode::FreeFly
        );
        assert_eq!(CameraControlMode::FreeFly.next(), CameraControlMode::Walk);
        assert_eq!(CameraControlMode::Walk.next(), CameraControlMode::OrbitEdit);
    }

    #[test]
    fn debug_startup_block_top_reaches_chunk_seam() {
        let (min, max) = App::debug_startup_block_bounds();
        assert!(max.y > min.y);
        assert_eq!(max.y, super::VOXEL_DIM_PER_CHUNK.y as f32);
    }

    #[test]
    fn orbit_keyboard_pan_input_normalizes_diagonal_motion() {
        let mut input = OrbitKeyboardPanInput::default();
        input.handle_key(KeyCode::KeyW, true);
        input.handle_key(KeyCode::KeyD, true);

        let direction = input.input_vector();
        assert!((direction.length() - 1.0).abs() <= 0.0001);
        assert!(direction.x > 0.0);
        assert!(direction.z > 0.0);
    }

    #[test]
    fn orbit_delta_smoother_eases_and_preserves_full_delta() {
        let mut smoother = OrbitDeltaSmoother::default();
        let target_delta = Vec3::new(0.25, 0.0, -0.1);
        smoother.add_delta(target_delta);

        let first_step = smoother.advance(1.0 / 60.0);
        assert!(first_step.length() > 0.0);
        assert!(first_step.length() < target_delta.length());

        let mut total_advanced = first_step;
        for _ in 0..120 {
            total_advanced += smoother.advance(1.0 / 60.0);
        }

        assert!((total_advanced - target_delta).length() <= 0.0001);
        assert_eq!(smoother.current_delta, Vec3::ZERO);
        assert_eq!(smoother.target_delta, Vec3::ZERO);
    }

    #[test]
    fn orbit_delta_smoother_preserves_continuous_keyboard_distance() {
        let mut smoother = OrbitDeltaSmoother::default();
        let frame_delta_time = 1.0 / 60.0;
        let velocity = Vec3::new(0.9, 0.0, -0.4);
        let mut total_advanced = Vec3::ZERO;

        for _ in 0..60 {
            smoother.add_delta(velocity * frame_delta_time);
            total_advanced += smoother.advance(frame_delta_time);
        }
        assert!(total_advanced.length() < velocity.length());

        for _ in 0..120 {
            total_advanced += smoother.advance(frame_delta_time);
        }

        assert!((total_advanced - velocity).length() <= 0.0001);
        assert_eq!(smoother.current_delta, Vec3::ZERO);
        assert_eq!(smoother.target_delta, Vec3::ZERO);
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
