#[allow(unused)]
use crate::util::Timer;

mod authored_flora_bench;
mod boot;
mod camera_control;
mod camera_snapshot_ui;
mod ddgi_spatial_weight_readback;
mod denoiser_bench;
mod environment_irradiance_capture;
mod environment_lighting_test_scene;
mod frame_timing;
mod house_scene;
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
mod terrain_connectivity;
mod terrain_persistence;
mod tree_bench;
mod ui_style;
mod vegetation;
mod visible_terrain;
mod voxel_backpack;
mod water;
mod water_experience_scene;

use self::authored_flora_bench::AuthoredFloraBench;
use self::camera_control::{CameraControlRuntime, ORBIT_CAMERA_DEFAULT_FOCUS};
use self::camera_snapshot_ui::draw_camera_snapshots_ui;
use self::ddgi_spatial_weight_readback::DdgiSpatialWeightReadbackRuntime;
use self::denoiser_bench::{
    DenoiserBench, CAMERA_FORWARD_PER_FRAME_WORLD, CAMERA_STRAFE_PER_FRAME_WORLD,
    CAMERA_YAW_PER_FRAME_RADIANS,
};
use self::environment_irradiance_capture::EnvironmentIrradianceCaptureRuntime;
use self::frame_timing::{
    draw_frame_timing_panel, FrameCpuScope, FrameCpuTimings, FrameTimingSnapshot,
};
use self::loading::{LoadingPhase, LoadingState};
use self::moisture::TerrainMoistureRuntime;
use self::physics::TerrainPhysics;
use self::placeables::{IrrigationNetwork, SprinklerRuntime};
use self::player_tools::{PlayerTool, PlayerToolPointerAction, PlayerToolRuntime};
use self::screenshot::{PendingDenoiserFrame, ScreenshotFrameReadiness, ScreenshotRuntime};
use self::terrain_connectivity::TerrainConnectivityRuntime;
use self::terrain_persistence::TerrainPersistenceRuntime;
use self::tree_bench::TreeBench;
use self::vegetation::{TreeRuntime, TreeVariationConfig};
use self::visible_terrain::VisibleTerrainChange;
use self::voxel_backpack::VoxelBackpack;
use crate::app::camera_snapshots::CameraSnapshotLibrary;
use crate::app::environment;
use crate::app::physical_visible_terrain;
use crate::app::terrain_edit_bounds::INITIAL_EDITABLE_TERRAIN_BOUNDS;
use crate::app::world_edits::{BuildEdit, WorldEditPlan};
use crate::app::world_ops;
use crate::app::{DebugSettings, GuiAdjustables, WindSourceGuiValues};
use crate::audio::{
    canopy_audio_diagnostic_pose, CanopyAudioDiagnosticPose, CanopyAudioTrajectoryPhase,
    SpatialSoundManager, TreeAudioManager, TreeRustleParams,
};
use crate::builder::{
    ContreeBuilder, PlainBuilder, SceneAccelBuilder, SurfaceBuilder, VOXEL_FERTILITY_MAX,
    VOXEL_MOISTURE_MAX,
};
use crate::ddgi::{DdgiResourceBytes, DdgiVolumeGrid, SUPPORTED_DDGI_SPACINGS_VOXELS};
use crate::environment_probes::{
    EnvironmentProbeVisualizationFilter, EnvironmentProbeVisualizationMode,
};
use crate::flora::species;
use crate::game_time::WorldClock;
use crate::geom::UAabb3;
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
use crate::util::{ChunkPopMode, GrowingFloraChunk, GrowingFloraQueue, BENCH};
use crate::wind::{WindResponseCurve, WindSource};
use crate::RenderFlags;
use crate::{egui_renderer::EguiRenderer, window::WindowState, WaterProfilePreference};
use anyhow::{Context, Result};
use egui::{Color32, ColorImage, FontData, FontDefinitions, FontFamily, RichText, TextureHandle};
use glam::{UVec3, Vec2, Vec3, Vec4};
use rand::RngExt;
use re_flora_vkn::{
    Allocator, Extent2D, GpuProfiler, GpuProfilerFrameResults, PipelineStage, SwapchainDesc,
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
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::WindowId,
};

const LEAF_CLUSTER_DISTANCE: f32 = 0.08;
const TERRAIN_EDIT_PREVIEW_ALPHA: f32 = 0.9;
// Muted runs should exercise audio setup, source updates, ray tracing, and pump paths
// without producing audible output for the user.
const MUTED_AUDIO_OUTPUT_GAIN_DB: f32 = -120.0;
const CANOPY_AUDIO_DIAGNOSTIC_TREE_SEED: u64 = 122;
const CANOPY_AUDIO_DIAGNOSTIC_WIND_SOURCES: [WindSource; 1] =
    [WindSource::new(35.0, 1.0, 1.0, 3, 2.0, 0.5, 0.75)];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GlobalKeyboardCommand {
    Terminate,
    ToggleConfigPanel,
}

struct EguiTextureLifecycleTest {
    handle: Option<TextureHandle>,
    step: u32,
    completed: bool,
}

impl EguiTextureLifecycleTest {
    fn new(context: &egui::Context) -> Self {
        let image = ColorImage::new([8, 8], vec![Color32::from_rgb(40, 120, 200); 64]);
        let handle = context.load_texture(
            "architecture-egui-texture-lifecycle",
            image,
            egui::TextureOptions::NEAREST,
        );
        Self {
            handle: Some(handle),
            step: 0,
            completed: false,
        }
    }

    fn advance(&mut self) {
        let Some(handle) = self.handle.as_mut() else {
            return;
        };
        match self.step {
            0 => {
                log::info!("[EGUI_TEXTURE_LIFECYCLE] phase=initial_full queued size=8x8");
                self.step = 1;
            }
            1 | 2 => {
                let color = if self.step == 1 {
                    Color32::from_rgb(220, 80, 80)
                } else {
                    Color32::from_rgb(80, 220, 120)
                };
                handle.set_partial(
                    [self.step as usize, self.step as usize],
                    ColorImage::new([2, 2], vec![color; 4]),
                    egui::TextureOptions::NEAREST,
                );
                log::info!(
                    "[EGUI_TEXTURE_LIFECYCLE] phase=partial_update step={} pos={}x{} size=2x2",
                    self.step,
                    self.step,
                    self.step,
                );
                self.step += 1;
            }
            3 => {
                handle.set(
                    ColorImage::new([8, 8], vec![Color32::from_rgb(210, 180, 60); 64]),
                    egui::TextureOptions::NEAREST,
                );
                log::info!("[EGUI_TEXTURE_LIFECYCLE] phase=full_replacement queued size=8x8");
                self.step = 4;
            }
            4 => {
                let id = handle.id();
                self.handle.take();
                self.completed = true;
                log::info!("[EGUI_TEXTURE_LIFECYCLE] phase=free queued texture_id={id:?}");
                self.step = 5;
            }
            _ => {}
        }
    }
}

struct CanopyAudioDiagnosticRuntime {
    start_time_seconds: Option<f32>,
    previous_phase: Option<CanopyAudioTrajectoryPhase>,
}

impl CanopyAudioDiagnosticRuntime {
    fn new() -> Self {
        Self {
            start_time_seconds: None,
            previous_phase: None,
        }
    }

    fn sample(
        &mut self,
        tree_origin_world: Vec3,
        time_seconds: f32,
    ) -> (CanopyAudioDiagnosticPose, f32, bool) {
        let start_time_seconds = *self.start_time_seconds.get_or_insert(time_seconds);
        let elapsed_seconds = (time_seconds - start_time_seconds).max(0.0);
        let pose = canopy_audio_diagnostic_pose(tree_origin_world, elapsed_seconds);
        let phase_changed = self.previous_phase != Some(pose.phase);
        self.previous_phase = Some(pose.phase);
        (pose, elapsed_seconds, phase_changed)
    }
}

struct ResizeLifecycleTest {
    requested: usize,
    next_request_frame: u64,
    observed: Vec<re_flora_vkn::Extent2D>,
    complete: bool,
    publication_count_at_requests_complete: Option<usize>,
}

impl ResizeLifecycleTest {
    const SIZES: [PhysicalSize<u32>; 5] = [
        PhysicalSize::new(1152, 648),
        PhysicalSize::new(960, 600),
        PhysicalSize::new(1408, 792),
        PhysicalSize::new(1024, 768),
        PhysicalSize::new(1280, 720),
    ];

    fn request_next(&mut self, window: &winit::window::Window, frame: u64) -> Option<Extent2D> {
        if self.complete || frame < self.next_request_frame {
            return None;
        }
        let burst = usize::from(self.requested == 0) * 2 + 1;
        let mut latest_accepted_extent = None;
        for _ in 0..burst {
            let Some(size) = Self::SIZES.get(self.requested).copied() else {
                break;
            };
            self.requested += 1;
            let accepted = window.request_inner_size(size);
            if let Some(accepted) = accepted {
                latest_accepted_extent = Some(Extent2D::new(accepted.width, accepted.height));
            }
            log::info!(
                "[RESIZE_LIFECYCLE] phase=request index={} requested={}x{} accepted={:?} burst={}",
                self.requested - 1,
                size.width,
                size.height,
                accepted,
                burst > 1,
            );
        }
        if self.requested >= Self::SIZES.len() {
            self.complete = true;
            self.publication_count_at_requests_complete = Some(self.observed.len());
            log::info!(
                "[RESIZE_LIFECYCLE] phase=requests_complete count={} observed={}",
                self.requested,
                self.observed.len(),
            );
            return latest_accepted_extent;
        }
        self.next_request_frame = frame + 2;
        latest_accepted_extent
    }

    fn observe(&mut self, size: re_flora_vkn::Extent2D, generation: u64) {
        self.observed.push(size);
        log::info!(
            "[RESIZE_LIFECYCLE] phase=published index={} extent={}x{} generation={}",
            self.observed.len() - 1,
            size.width,
            size.height,
            generation,
        );
    }

    fn published_after_latest_request(&self) -> bool {
        self.complete
            && self
                .publication_count_at_requests_complete
                .is_some_and(|count| self.observed.len() > count)
    }
}

pub struct App {
    egui_renderer: EguiRenderer,
    loading_state: Option<LoadingState>,
    pending_frame_extent: Option<Extent2D>,
    resize_lifecycle_test: Option<ResizeLifecycleTest>,
    egui_texture_lifecycle_test: Option<EguiTextureLifecycleTest>,
    swapchain: Swapchain,
    window_state: WindowState,
    frame_manager: SwapchainFrameManager,
    gpu_profiler: Option<GpuProfiler>,
    gpu_profiler_latest_results: Option<GpuProfilerFrameResults>,
    time_info: TimeInfo,
    world_clock: WorldClock,
    render_flags: RenderFlags,
    cursor_position_physical: Option<Vec2>,
    camera_control: CameraControlRuntime,
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
    player_tools: PlayerToolRuntime,
    voxel_backpack: VoxelBackpack,
    water_particle_handoff_main_thread_ms: Option<f32>,

    terrain_moisture: TerrainMoistureRuntime,
    growing_flora_chunks: GrowingFloraQueue,
    terrain_connectivity: TerrainConnectivityRuntime,

    #[allow(dead_code)]
    tree_variation_config: TreeVariationConfig,
    #[allow(dead_code)]
    regenerate_trees_requested: bool,
    trees: TreeRuntime,

    particle_system: ParticleSystem,
    butterfly_emitters: Vec<ButterflyEmitter>,
    butterfly_emitter_desc: ButterflyEmitterDesc,
    butterfly_spawn_source_refresh_elapsed: f32,
    sprinklers: SprinklerRuntime,
    irrigation_network: IrrigationNetwork,
    particle_animation_time_sec: f32,
    water_sim: water::AsyncWaterSim,
    water_runtime_overrides: water::WaterRuntimeOverrides,
    water_terrain: water::WaterTerrainRuntime,
    particle_snapshots: Vec<ParticleSnapshot>,
    #[allow(dead_code)]
    terrain_harvest_particle_handles: Vec<ParticleHandle>,
    particle_forces: ParticleForces,

    render_start_time: Option<Instant>,
    terrain_persistence: TerrainPersistenceRuntime,
    screenshot_capture: ScreenshotRuntime,
    environment_irradiance_capture: EnvironmentIrradianceCaptureRuntime,
    ddgi_spatial_weight_readback: DdgiSpatialWeightReadbackRuntime,
    denoiser_bench: Option<DenoiserBench>,
    auto_exit_delay: Option<f32>,
    canopy_audio_telemetry_next_log_seconds: Option<f32>,
    canopy_audio_diagnostic: Option<CanopyAudioDiagnosticRuntime>,
    tree_bench: Option<TreeBench>,
    authored_flora_bench: Option<AuthoredFloraBench>,
    water_edit_soak: Option<water::WaterEditSoak>,
    water_experience_scene: Option<water_experience_scene::WaterExperienceScene>,
    environment_lighting_test_scene:
        Option<environment_lighting_test_scene::EnvironmentLightingTestScene>,
    hybrid_transparency_test_scene:
        Option<hybrid_transparency_test_scene::HybridTransparencyTestScene>,
    house_scene_requested: bool,
    visible_terrain_revision: u32,
    shutdown_started: bool,

    // note: always keep the context to end, as it has to be destroyed last
    vulkan_ctx: VulkanContext,

    // Keep ownership so the shared PetalSonic engine outlives every subsystem.
    #[allow(dead_code)]
    spatial_sound_manager: SpatialSoundManager,
    tree_audio_manager: TreeAudioManager,
}

impl Drop for App {
    fn drop(&mut self) {
        if let Err(err) = self.shutdown_for_termination() {
            panic!("[SHUTDOWN] failed during App drop: {err:#}");
        }
        if let Err(err) = self.spatial_sound_manager.stop() {
            log::warn!("Failed to stop audio engine during shutdown: {}", err);
        }

        // Ensure GPU work is done before resources begin destructing
        self.vulkan_ctx.device().wait_idle();
    }
}

impl App {
    fn apply_canopy_audio_diagnostic_trajectory(&mut self, time_seconds: f32) {
        let Some(diagnostic) = self.canopy_audio_diagnostic.as_mut() else {
            return;
        };
        let (pose, elapsed_seconds, phase_changed) =
            diagnostic.sample(self.debug_tree_pos, time_seconds);
        self.tracer
            .set_camera_pose_looking_at(pose.position_world, pose.target_world);
        if phase_changed {
            log::info!(
                "[AUDIO][CANOPY][TRAJECTORY] elapsed_seconds={:.6} phase={:?} position_world={:?} target_world={:?}",
                elapsed_seconds,
                pose.phase,
                pose.position_world,
                pose.target_world,
            );
        }
    }

    fn log_canopy_audio_telemetry(&mut self, time_seconds: f32) {
        let Some(next_log_seconds) = self.canopy_audio_telemetry_next_log_seconds else {
            return;
        };
        if time_seconds < next_log_seconds {
            return;
        }
        self.canopy_audio_telemetry_next_log_seconds = Some(time_seconds + 0.1);

        let Some(snapshot) = self.tree_audio_manager.canopy_telemetry_snapshot() else {
            return;
        };
        let emitter_count = snapshot
            .samples
            .iter()
            .map(|sample| sample.emitter_uuid)
            .collect::<std::collections::HashSet<_>>()
            .len();
        let observed_voice_count = snapshot
            .samples
            .iter()
            .filter_map(|sample| sample.direct_path.as_ref().map(|direct| direct.voice_id))
            .collect::<std::collections::HashSet<_>>()
            .len();
        log::info!(
            "[AUDIO][CANOPY][SUMMARY] time_seconds={:.6} trees={} emitters={} observed_voices={} samples={} extent_responses={} solve_discards={} last_discard_spatial_revision={} last_discard_geometry_version={} voice_identity_violations={} revision_rollbacks={} sample_contract_violations={} aggregate_mismatches={} petal_superseded_solves={} telemetry_queue_depth={} telemetry_queue_high_water={} telemetry_drops={} direct_rays={} sample_cache_hits={} processed_extents={} lobes={} retained={} deferred={} render_rejected_rollbacks={}",
            time_seconds,
            snapshot.trees.len(),
            emitter_count,
            observed_voice_count,
            snapshot.samples.len(),
            snapshot.telemetry.extent_response_count,
            snapshot.telemetry.solve_discard_count,
            snapshot.telemetry.last_discard_spatial_revision,
            snapshot.telemetry.last_discard_geometry_version,
            snapshot.telemetry.voice_identity_violation_count,
            snapshot.telemetry.revision_rollback_count,
            snapshot.telemetry.sample_contract_violation_count,
            snapshot.telemetry.aggregate_mismatch_count,
            snapshot.petal_superseded_solve_count,
            snapshot.petal_telemetry_queue_depth,
            snapshot.petal_telemetry_queue_high_water,
            snapshot.petal_telemetry_dropped_events,
            snapshot.petal_direct_ray_count,
            snapshot.petal_sample_cache_hit_count,
            snapshot.petal_processed_extent_count,
            snapshot.petal_lobe_count,
            snapshot.petal_retained_response_count,
            snapshot.petal_deferred_response_count,
            snapshot.petal_render_rejected_response_count,
        );
        for tree in snapshot.trees {
            log::info!(
                "[AUDIO][CANOPY][TREE] time_seconds={:.6} tree={} published={} replacements={} removals={} superseded_transitions={} retired_generations={}",
                time_seconds,
                tree.tree_id,
                tree.lifecycle.published_generation_count,
                tree.lifecycle.replacement_transition_count,
                tree.lifecycle.removal_transition_count,
                tree.lifecycle.superseded_transition_count,
                tree.lifecycle.retired_generation_count,
            );
        }
        for sample in snapshot.samples {
            let direct = sample.direct_path.as_ref();
            log::info!(
                "[AUDIO][CANOPY][SAMPLE] time_seconds={:.6} tree={} generation={} sample={} emitter={} voice={:?} position_tree_voxels={:?} position_world={:?} observed_world={:?} clearance_voxels={:.6} weight={:.9} observed_weight={:?} lifecycle_power={:.6} content_seed={} phase={:.9} provenance={:?} wind_target={:.6} wind_filtered={:.6} volume_db={:.6} candidate_membership={:?} solve_status={:?} hit={:?} hit_material={:?} transmission={:?} visible_fraction={:?} raw_gain={:?} filtered_gain={:?} classification={:?} dwell_seconds={:?} rays={:?} cache_hits={:?} hit_count={:?} cache_age_seconds={:?} spatial_revision={:?} geometry_version={:?} response_spatial_revision={:?} response_geometry_version={:?} lobes={:?} direct_transitions={:?} direct_superseded={:?}",
                time_seconds,
                sample.key.tree_id(),
                sample.key.generation(),
                sample.key.sample_id().value(),
                sample.emitter_uuid,
                direct.map(|value| value.voice_id),
                sample.position_tree_voxels,
                sample.position_world,
                direct.map(|value| value.observed_world_position),
                sample.clearance_voxels,
                sample.weight,
                direct.map(|value| value.normalized_power_weight),
                sample.lifecycle_power,
                sample.content_seed,
                sample.phase,
                sample.provenance,
                sample.target_wind_response,
                sample.current_wind_response,
                sample.current_volume_db,
                direct.map(|value| value.candidate_membership),
                direct.map(|value| value.solve_status),
                direct.map(|value| value.hit),
                direct.and_then(|value| value.hit_material.as_deref()),
                direct.map(|value| value.transmission),
                direct.map(|value| value.visible_fraction),
                direct.map(|value| value.raw_direct_gain),
                direct.map(|value| value.filtered_direct_gain),
                direct.map(|value| value.classification),
                direct.map(|value| value.dwell_seconds),
                direct.map(|value| value.ray_count),
                direct.map(|value| value.cache_hit_count),
                direct.map(|value| value.hit_count),
                direct.map(|value| value.cache_age_seconds),
                direct.map(|value| value.spatial_revision),
                direct.map(|value| value.geometry_version),
                direct.map(|value| value.response_spatial_revision),
                direct.map(|value| value.response_geometry_version),
                direct.map(|value| value.lobe_count),
                direct.map(|value| value.transition_count),
                direct.map(|value| value.superseded_response_count),
            );
        }
    }

    pub(super) fn set_manual_time_of_day(&mut self, time_of_day: f32) {
        self.debug_settings.adjustables.time_of_day.value = time_of_day;
        self.world_clock.set_live_time_of_day(time_of_day);
    }

    pub(super) fn track_growing_flora_chunk(&mut self, chunk_id: UVec3) {
        self.growing_flora_chunks
            .push(chunk_id, self.world_clock.flora_tick());
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
                        "environment_probes.rederive"
                            | "environment_probes.trace_priority"
                            | "ddgi.probe_relocate"
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

        let tick_delta = self.world_clock.flora_tick().wrapping_sub(last_flora_tick);
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

const VOXEL_DIM_PER_CHUNK: UVec3 = UVec3::new(256, 256, 256);
pub(super) const CHUNK_DIM: UVec3 = UVec3::new(2, 2, 2);
const FREE_ATLAS_DIM: UVec3 = UVec3::new(512, 512, 512);
const MAX_FRAMES_IN_FLIGHT: usize = 1;
const GPU_PROFILER_MAX_SCOPES_PER_FRAME: usize = 64;
const TERRAIN_EDIT_DEFAULT_RADIUS: f32 = 0.08;
const TERRAIN_EDIT_RADIUS_MIN: f32 = 0.03;
const TERRAIN_EDIT_RADIUS_MAX: f32 = 0.36;
const TERRAIN_EDIT_RADIUS_SCROLL_STEP: f32 = 0.01;
const CENTER_CROSS_MARK_ARM_LENGTH: f32 = 8.0;
const CENTER_CROSS_MARK_GAP: f32 = 3.0;
const CENTER_CROSS_MARK_STROKE_WIDTH: f32 = 1.5;
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

    fn update_environmental_acoustics(&mut self) {
        let quality = Self::environmental_acoustics_quality(
            self.debug_settings
                .adjustables
                .audio_ray_tracing_quality_percent
                .value,
        );
        if let Err(err) = self.spatial_sound_manager.set_environmental_acoustics(
            self.debug_settings
                .adjustables
                .audio_ray_tracing_enabled
                .value,
            quality,
        ) {
            log::warn!("Failed to update environmental acoustics: {err}");
        }
    }

    fn environmental_acoustics_quality(quality_percent: u32) -> f32 {
        quality_percent.min(100) as f32 / 100.0
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

        let mut terrain_persistence = TerrainPersistenceRuntime::from_options(options)?;
        let terrain_snapshot_reader = terrain_persistence.take_startup_reader();

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
        let frame_extent_generation = swapchain.frame_extent_generation();
        let frame_retirement_sink = frame_manager.retirement_sink();
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
            frame_retirement_sink.clone(),
        );
        let egui_texture_lifecycle_test = options
            .egui_texture_lifecycle_test
            .then(|| EguiTextureLifecycleTest::new(renderer.context()));

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
            contree_builder.acoustic_scene_snapshot(),
            options.audio_output_device.clone(),
        )?;

        let tracer = Tracer::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            frame_retirement_sink,
            chunk_bound,
            frame_extent_generation,
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
                environment_irradiance_capture_target: options
                    .environment_irradiance_capture_target,
                ddgi_batch_order: options.ddgi_batch_order,
                ddgi_debug_view: options.ddgi_debug_view,
                ddgi_terrain_hard_origin: options.ddgi_terrain_hard_origin,
            },
            spatial_sound_manager.clone(),
        )?;
        {
            let shadow = tracer.direct_sun_shadow_resources();
            let contree_resources = contree_builder.get_resources();
            plain_builder.initialize_terrain_moisture_dry_resources(
                shadow.gui_input,
                &plain_builder.get_resources().chunk_atlas,
                shadow.shadow_camera_info,
                shadow.shadow_map_tex_for_vsm_ping,
                shadow.leaf_shadow_opacity_blended_tex,
                shadow.leaf_shadow_mask_tex,
                shadow.cloud_shadow_tex,
                &contree_resources.contree_leaf_data,
                &contree_resources.surface_leaf_coords,
                &contree_resources.surface_leaf_chunk_info,
            )?;
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
        let mut debug_settings = DebugSettings::load();
        if options.canopy_audio_diagnostic {
            let mut fixed_tree_desc = TreeDesc::default();
            fixed_tree_desc.branching.seed = CANOPY_AUDIO_DIAGNOSTIC_TREE_SEED;
            debug_settings.tree.desc = fixed_tree_desc;
            debug_settings.adjustables.tree_age.value = 1.0;
            log::info!(
                "[AUDIO][CANOPY][DIAGNOSTIC] fixed_tree_seed={} fixed_tree_age=1.0 fixed_wind={:?}",
                CANOPY_AUDIO_DIAGNOSTIC_TREE_SEED,
                CANOPY_AUDIO_DIAGNOSTIC_WIND_SOURCES,
            );
        }
        spatial_sound_manager.set_environmental_acoustics(
            debug_settings.adjustables.audio_ray_tracing_enabled.value,
            Self::environmental_acoustics_quality(
                debug_settings
                    .adjustables
                    .audio_ray_tracing_quality_percent
                    .value,
            ),
        )?;
        let mut tree_placement_preview_desc = debug_settings
            .tree
            .desc
            .at_age(debug_settings.adjustables.tree_age.value);
        tree_placement_preview_desc.branching.seed = rand::rng().random::<u64>();
        let mut render_flags = RenderFlags::from(options);
        if render_flags.enable_flora {
            render_flags.enable_leaves = debug_settings.tree.render_leaves;
        }
        let world_clock = WorldClock::new(
            FLORA_FULL_GROWTH_TICKS,
            debug_settings.adjustables.time_of_day.value,
        );
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
        let leaf_emitter_desc = LeafEmitterDesc {
            color_low: color_to_vec4(debug_settings.adjustables.leaves_bottom_color.value),
            color_high: color_to_vec4(debug_settings.adjustables.leaves_tip_color.value),
            ..LeafEmitterDesc::default()
        };
        let trees = TreeRuntime::new(leaf_emitter_desc);
        let mut tree_audio_manager = TreeAudioManager::new(
            spatial_sound_manager.clone(),
            Self::tree_audio_wind_response_curve(&debug_settings.adjustables),
            debug_settings.adjustables.tree_wind_volume_db.value,
            Self::tree_rustle_params(&debug_settings.adjustables),
        )?;
        tree_audio_manager.set_canopy_telemetry_enabled(
            options.canopy_audio_telemetry || options.canopy_audio_diagnostic,
        );
        let butterfly_emitters = Vec::new();
        let butterfly_emitter_desc =
            Self::butterfly_desc_from_gui_adjustables(&debug_settings.adjustables);
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
        let water_gui_config_applied = options.water_profile.is_none() && !options.water_experience;
        if water_gui_config_applied {
            water::apply_water_gui_adjustables_to_config(
                &mut water_config,
                &debug_settings.adjustables,
            );
        }
        if options.water_experience {
            water_experience_scene::WaterExperienceScene::configure_water(&mut water_config);
        }
        let water_profile_config = (options.water_profile.is_some() || options.water_experience)
            .then(|| water_config.clone());
        let water_runtime_overrides =
            water::WaterRuntimeOverrides::from_options(options, water_profile_config);
        water_runtime_overrides.apply(&mut water_config);

        log::info!(
            "[WATER] config profile={:?} experience={} gui_config_applied={} particles={} grid={:?} substep_dt={:.6}s terrain_margin_cells={:.2} boundary_density_min_fluid_fraction={:.2} boundary_density_max_correction={:.2} boundary_density_transition_cells={:.2} damping={:.2}/s quiet_settling={:.2}/{:.2}/s terrain_tangent_damping={:.2}/s debug_spawn_height_offset={:.2} gravity={:?} stiffness={:.1} gamma={:.2} j_min={:.3} viscosity={:.3} pressure_floor={:.3} wall_damping={:.2} collider_bounds {:?}..{:?} initial_fluid={:?} cells_per_unit={}",
            options.water_profile,
            options.water_experience,
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
            water_config.initial_fluid_bounds,
            cells_per_unit,
        );
        let water_experience_scene = options.water_experience.then(|| {
            water_experience_scene::WaterExperienceScene::new(water_config.particle_count)
        });
        let water_sim = water::AsyncWaterSim::new(water_config);
        let water_terrain = water::WaterTerrainRuntime::new();
        let terrain_harvest_particle_handles = Vec::with_capacity(256);
        let particle_forces = ParticleForces {
            linear_damping: 0.08,
            ..ParticleForces::default()
        };
        let physical_terrain_publication =
            physical_visible_terrain::PhysicalTerrainPublication::loading(
                chunk_indices.clone(),
                VOXEL_DIM_PER_CHUNK,
            )?;

        let mut app = Self {
            vulkan_ctx,
            egui_renderer: renderer,
            window_state,
            loading_state: Some(LoadingState {
                chunk_indices,
                terrain_snapshot_reader,
                physical_terrain_publication,
                current: 0,
                step_label: "Initializing...".to_owned(),
                phase: LoadingPhase::Terrain,
                collider_total: 0,
            }),

            cursor_position_physical: None,
            camera_control: CameraControlRuntime::default(),
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

            pending_frame_extent: None,
            resize_lifecycle_test: options.resize_lifecycle_test.then(|| ResizeLifecycleTest {
                requested: 0,
                next_request_frame: 2,
                observed: Vec::new(),
                complete: false,
                publication_count_at_requests_complete: None,
            }),
            egui_texture_lifecycle_test,
            time_info: TimeInfo::default(),
            world_clock,
            render_flags,

            debug_settings,
            debug_tree_pos,
            tree_placement_preview_desc,
            tree_variation_config: TreeVariationConfig::default(),
            regenerate_trees_requested: false,
            trees,
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
            player_tools: PlayerToolRuntime::default(),
            voxel_backpack: VoxelBackpack::default(),
            water_particle_handoff_main_thread_ms: None,
            terrain_moisture: TerrainMoistureRuntime::default(),
            growing_flora_chunks: GrowingFloraQueue::default(),
            terrain_connectivity: TerrainConnectivityRuntime::default(),

            particle_system,
            butterfly_emitters,
            butterfly_emitter_desc,
            butterfly_spawn_source_refresh_elapsed: f32::INFINITY,
            sprinklers: SprinklerRuntime::new(),
            irrigation_network: IrrigationNetwork::default(),
            particle_animation_time_sec: 0.0,
            water_sim,
            water_runtime_overrides,
            water_terrain,
            particle_snapshots,
            terrain_harvest_particle_handles,
            particle_forces,

            render_start_time: None,
            terrain_persistence,
            screenshot_capture: ScreenshotRuntime::new(options.screenshot.clone()),
            environment_irradiance_capture: EnvironmentIrradianceCaptureRuntime::new(
                options.environment_irradiance_capture_path.clone(),
            ),
            ddgi_spatial_weight_readback: DdgiSpatialWeightReadbackRuntime::new(
                options.ddgi_spatial_weight_readback_path.clone(),
            ),
            denoiser_bench: options.denoiser_bench.clone().map(DenoiserBench::new),
            auto_exit_delay: options.auto_exit_delay,
            canopy_audio_telemetry_next_log_seconds: (options.canopy_audio_telemetry
                || options.canopy_audio_diagnostic)
                .then_some(0.0),
            canopy_audio_diagnostic: options
                .canopy_audio_diagnostic
                .then(CanopyAudioDiagnosticRuntime::new),
            tree_bench: options
                .tree_bench
                .then(|| TreeBench::new(options.tree_bench_samples)),
            authored_flora_bench: options
                .authored_flora_bench
                .then(|| AuthoredFloraBench::new(options.authored_flora_bench_samples)),
            water_edit_soak: options.water_edit_soak.then(water::WaterEditSoak::default),
            water_experience_scene,
            environment_lighting_test_scene: options
                .environment_lighting_test_scene
                .map(environment_lighting_test_scene::EnvironmentLightingTestScene::new),
            hybrid_transparency_test_scene: options
                .hybrid_transparency_test_scene
                .then(hybrid_transparency_test_scene::HybridTransparencyTestScene::new),
            house_scene_requested: options.house_scene,
            visible_terrain_revision: 0,
            shutdown_started: false,

            spatial_sound_manager,
            tree_audio_manager,
        };

        if app.mute_audio_output {
            log::info!(
                "--mute: forcing master audio output volume to 0 while keeping audio engine processing active"
            );
        }
        app.apply_effective_master_volume_gain("Failed to apply initial master volume");

        if options.environment_lighting_test_scene.is_some() {
            app.configure_environment_lighting_test_scene_camera();
        }
        if options.water_experience {
            app.configure_water_experience_camera()?;
        }
        app.sync_cursor_with_panels();

        app.configure_gui_font()?;
        app.load_item_panel_icons()?;
        app.rebuild_tree_placement_preview()?;
        if options.hybrid_transparency_test_scene {
            app.configure_hybrid_transparency_test_scene()?;
        }
        // Test scenes provide a useful default pose, but an explicit snapshot
        // is the caller's final camera choice for screenshots and repro runs.
        app.apply_startup_camera_snapshot(options.camera_snapshot.as_deref())?;
        app.sync_cursor_with_panels();

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

    fn execute_edit_plan(&mut self, plan: WorldEditPlan) -> Result<()> {
        anyhow::ensure!(
            self.terrain_persistence.allows_world_updates(),
            "terrain persistence is in fatal Error; restart is required"
        );
        for edit in plan.voxel_edits {
            world_ops::apply_voxel_edit(&mut self.plain_builder, edit)?;
        }
        if let Some(change) = VisibleTerrainChange::from_build_edits(plan.build_edits)? {
            self.publish_visible_terrain(change)?;
        }
        Ok(())
    }

    fn observe_initial_published_terrain_for_ddgi(&mut self) -> Result<u32> {
        let revision = self.visible_terrain_revision;
        self.tracer.observe_published_environment_probe_terrain(
            revision,
            UAabb3::new(UVec3::ZERO, CHUNK_DIM * VOXEL_DIM_PER_CHUNK),
        )?;
        Ok(revision)
    }

    fn gui_wants_keyboard_input(&self) -> bool {
        self.window_state.is_cursor_visible()
            && self.egui_renderer.context().egui_wants_keyboard_input()
    }

    fn global_keyboard_command(
        physical_key: PhysicalKey,
        state: ElementState,
        gui_wants_keyboard_input: bool,
    ) -> Option<GlobalKeyboardCommand> {
        if state != ElementState::Pressed || gui_wants_keyboard_input {
            return None;
        }

        match physical_key {
            PhysicalKey::Code(KeyCode::Escape) => Some(GlobalKeyboardCommand::Terminate),
            PhysicalKey::Code(KeyCode::KeyR) => Some(GlobalKeyboardCommand::ToggleConfigPanel),
            _ => None,
        }
    }

    pub fn on_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        if self.shutdown_started {
            return;
        }
        let is_keyboard_event = matches!(&event, WindowEvent::KeyboardInput { .. });
        let gui_wanted_keyboard_before_event = self.gui_wants_keyboard_input();

        if let WindowEvent::KeyboardInput { event, .. } = &event {
            match Self::global_keyboard_command(
                event.physical_key,
                event.state,
                gui_wanted_keyboard_before_event,
            ) {
                Some(GlobalKeyboardCommand::Terminate) => {
                    self.on_terminate(event_loop);
                    return;
                }
                Some(GlobalKeyboardCommand::ToggleConfigPanel) => {
                    self.config_panel_visible = !self.config_panel_visible;
                    self.sync_cursor_with_panels();
                    return;
                }
                None => {}
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
                self.screenshot_capture.request_clipboard();
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
                self.queue_current_frame_extent();
            }

            // resize the window
            WindowEvent::Resized(size) => {
                self.queue_frame_extent(Extent2D::new(size.width, size.height));
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyF {
                    self.window_state.toggle_fullscreen();
                }

                if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyG {
                    self.toggle_camera_control_mode();
                    return;
                }

                // Keep both movement systems synchronized with physical key state so a held key
                // remains active after G switches camera modes. Full input resets still happen
                // when a panel editor captures the keyboard.
                if self.keyboard_tool_shortcuts_available() {
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

                if self.is_free_look_camera_mode() || self.keyboard_tool_shortcuts_available() {
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
                            let now = Instant::now();
                            match self.player_tools.begin_pointer_action(button) {
                                Some(PlayerToolPointerAction::Continuous(action)) => {
                                    self.execute_continuous_terrain_tool_action(action, now);
                                }
                                Some(PlayerToolPointerAction::PlaceablePlacement) => {
                                    self.stop_terrain_edit_loop_sound();
                                    self.try_placeable_placement();
                                }
                                Some(PlayerToolPointerAction::CancelPlaceable) => {
                                    self.cancel_pipe_drag();
                                }
                                None => {}
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
                if self.shutdown_started {
                    return;
                }
                // when the windiw is resized, redraw is called afterwards, so when the window is minimized, return
                if self.window_state.is_minimized() {
                    return;
                }

                let frame_start = Instant::now();
                let frame_perf_enabled = self.perf_logging;
                let frame_timing_enabled = frame_perf_enabled || self.frame_timing_panel_visible;
                let mut cpu_timings = FrameCpuTimings::new(frame_timing_enabled);

                // resize the window if needed
                if self.pending_frame_extent.is_some() {
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
                if let Err(err) = self
                    .spatial_sound_manager
                    .publish_acoustic_scene(self.contree_builder.acoustic_scene_snapshot())
                {
                    log::warn!("Failed to publish acoustic terrain snapshot: {err}");
                }
                if self.loading_state.is_none() {
                    self.terrain_physics
                        .process_terrain_collider_updates(&self.contree_builder);
                }
                if self.loading_state.is_some() {
                    let water_terrain = self.advance_loading_water_terrain(frame_timing_enabled);
                    cpu_timings.add_ms(FrameCpuScope::TerrainSource, water_terrain.source_ms);
                    cpu_timings.add_ms(FrameCpuScope::WaterCache, water_terrain.cache_ms);
                    cpu_timings.add_ms(FrameCpuScope::ColliderQueue, water_terrain.collider_ms);
                    self.process_loading_step();
                    self.render_loading_frame();
                    return;
                }

                if self.terrain_persistence.allows_world_updates()
                    && self.player_tools.continuous_hold_active()
                {
                    let now = Instant::now();
                    if let Some(action) = self.player_tools.active_continuous_action() {
                        self.execute_continuous_terrain_tool_action(action, now);
                    } else {
                        self.stop_terrain_edit_loop_sound();
                    }
                }
                let frame_delta_time = self.time_info.delta_time();
                if self.terrain_persistence.allows_world_updates() {
                    if let Err(err) = self
                        .terrain_physics
                        .advance_dynamic_bodies(frame_delta_time, &mut self.tracer)
                    {
                        log::error!("Failed to advance dynamic bodies: {err:#}");
                    }
                }
                let fruit_refresh_tree_ids =
                    self.terrain_physics.take_attached_fruit_refresh_trees();
                if let Err(err) = self.refresh_attached_tree_fruits(&fruit_refresh_tree_ids) {
                    log::error!("Failed to refresh attached fruits after detachment: {err:#}");
                }
                let time_since_start = self.time_info.time_since_start();
                self.apply_canopy_audio_diagnostic_trajectory(time_since_start);
                let world_tick_seconds = crate::game_time::clamp_world_tick_seconds(
                    self.debug_settings.adjustables.world_tick_seconds.value,
                );
                let world_updates_running = self.terrain_persistence.allows_world_updates();
                let world_tick_steps = self.world_clock.advance_simulation(
                    frame_delta_time,
                    world_tick_seconds,
                    world_updates_running,
                );
                if world_updates_running && world_tick_steps > 0 {
                    self.update_growing_flora_chunk();
                }
                let configured_wind_sources =
                    GuiAdjustables::active_wind_sources(&self.debug_settings.wind_sources);
                let active_wind_sources = if self.canopy_audio_diagnostic.is_some() {
                    &CANOPY_AUDIO_DIAGNOSTIC_WIND_SOURCES[..]
                } else {
                    &configured_wind_sources
                };
                if let Err(err) = self.tree_audio_manager.update(
                    time_since_start,
                    active_wind_sources,
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
                if let Err(err) = self
                    .spatial_sound_manager
                    .publish_spatial_frame(f64::from(time_since_start))
                {
                    log::warn!("Failed to publish spatial audio frame: {}", err);
                }
                self.tree_audio_manager.collect_canopy_acoustic_telemetry();
                self.update_environmental_acoustics();
                self.log_canopy_audio_telemetry(time_since_start);

                if self.is_free_look_camera_mode() && !self.window_state.is_cursor_visible() {
                    let mouse_delta = self.camera_control.take_smoothed_free_look_mouse_delta();
                    self.tracer.handle_mouse(mouse_delta);
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
                let selected_item_panel_display_slot =
                    self.player_tools.selected_item_panel_display_slot();
                let voxel_palette_entries: Vec<VoxelPaletteEntry> = self
                    .voxel_backpack
                    .snapshot()
                    .into_iter()
                    .map(|entry| {
                        let voxel = entry.voxel;
                        let [red, green, blue] = voxel.color_rgb();
                        VoxelPaletteEntry {
                            voxel,
                            label: voxel.label(),
                            count: entry.count,
                            color: Color32::from_rgb(red, green, blue),
                            selected: false,
                        }
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
                let mut terrain_save_requested = false;
                let mut terrain_load_requested = false;
                let ddgi_runtime_status = self.tracer.ddgi_runtime_status();
                let environment_probe_status = ddgi_runtime_status.active();
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
                    self.sprinklers.len()
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
                let hide_terrain_edit_preview = self
                    .environment_lighting_test_scene
                    .as_ref()
                    .is_some_and(
                        environment_lighting_test_scene::EnvironmentLightingTestScene::hides_terrain_edit_preview,
                    );
                let terrain_edit_preview_center = (!hide_terrain_edit_preview)
                    .then(|| terrain_edit_hover.map(|hover| hover.center))
                    .flatten();
                let terrain_edit_preview_shape = self.terrain_edit_preview_shape();
                let terrain_edit_preview_color = self.terrain_edit_preview_color(
                    terrain_edit_hover
                        .map(|hover| hover.is_editable)
                        .unwrap_or(true),
                );
                let current_camera_is_free_fly = self.is_free_fly_camera_mode();
                let hide_ui_for_environment_test_capture = (self
                    .environment_lighting_test_scene
                    .is_some()
                    || self.hybrid_transparency_test_scene.is_some())
                    && (self.screenshot_capture.is_scheduled() || self.denoiser_bench.is_some());
                if self.loading_state.is_none() {
                    if let Some(test) = self.egui_texture_lifecycle_test.as_mut() {
                        test.advance();
                    }
                }
                let lifecycle_texture = self
                    .egui_texture_lifecycle_test
                    .as_ref()
                    .and_then(|test| test.handle.as_ref())
                    .map(|handle| (handle.id(), handle.size_vec2()));
                let egui_start = Instant::now();
                self.egui_renderer
                    .update(&self.window_state.window(), |ctx| {
                        let mut style = (*ctx.global_style()).clone();
                        apply_gui_style(&mut style);
                        ctx.set_global_style(style);

                        if hide_ui_for_environment_test_capture {
                            return;
                        }

                        if let Some((texture_id, texture_size)) = lifecycle_texture {
                            egui::Window::new("Texture lifecycle acceptance")
                                .default_pos(egui::pos2(8.0, 8.0))
                                .default_size(egui::vec2(180.0, 180.0))
                                .show(ctx, |ui| {
                                    ui.image((texture_id, texture_size));
                                });
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

                                    ui.add_space(8.0);
                                    ui.separator();
                                    ui.add_space(8.0);
                                    ui.heading(
                                        RichText::new("Terrain Snapshot")
                                            .size(16.0)
                                            .color(GOLD_ACCENT),
                                    );
                                    ui.label("Terrain-only; trees, entities, water, and time are retained.");
                                    ui.horizontal(|ui| {
                                        ui.label("Path");
                                        ui.text_edit_singleline(
                                            self.terrain_persistence.snapshot_path_mut(),
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        let ready =
                                            self.terrain_persistence.can_start_operation();
                                        if ui
                                            .add_enabled(ready, egui::Button::new("Save terrain"))
                                            .clicked()
                                        {
                                            terrain_save_requested = true;
                                        }
                                        if ui
                                            .add_enabled(ready, egui::Button::new("Load terrain"))
                                            .clicked()
                                        {
                                            terrain_load_requested = true;
                                        }
                                        ui.label(self.terrain_persistence.status_label());
                                    });

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
                                            ui.monospace(ddgi_runtime_status.active_line());
                                            ui.monospace(ddgi_runtime_status.builder_line());
                                            ui.monospace(ddgi_runtime_status.coordinator_line());
                                            ui.monospace(ddgi_runtime_status.availability_line());
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
                        let item_panel_response = draw_item_panel(
                            ctx,
                            &item_panel_slots,
                            Some(selected_item_panel_display_slot),
                            self.window_state.is_cursor_visible(),
                        );
                        clicked_item_panel_slot = item_panel_response.clicked_slot;

                        if self.player_tools.selected_tool() == PlayerTool::Staff {
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
                if terrain_load_requested || terrain_save_requested {
                    if terrain_load_requested {
                        self.perform_runtime_terrain_load();
                    } else {
                        self.perform_runtime_terrain_save();
                    }
                    self.sync_cursor_with_panels();
                    return;
                }
                if environment_probe_rebuild_requested {
                    self.tracer
                        .rebuild_environment_probes(self.environment_probe_spacing_draft);
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
                if let Err(err) = self
                    .tree_audio_manager
                    .set_rustle_params(Self::tree_rustle_params(&self.debug_settings.adjustables))
                {
                    log::error!("Failed to rebuild resident tree rustle clip: {err:#}");
                }

                if TreeBench::run_next(self) {
                    self.on_terminate(event_loop);
                    return;
                }
                if AuthoredFloraBench::run_next(self) {
                    self.on_terminate(event_loop);
                    return;
                }

                let water_terrain = self.advance_water_terrain(frame_timing_enabled);
                cpu_timings.add_ms(FrameCpuScope::TerrainSource, water_terrain.source_ms);
                cpu_timings.add_ms(FrameCpuScope::WaterCache, water_terrain.cache_ms);
                cpu_timings.add_ms(FrameCpuScope::ColliderQueue, water_terrain.collider_ms);
                self.maybe_resume_terrain_persistence_water();
                cpu_timings.time(FrameCpuScope::WaterEditSoak, || {
                    self.process_water_edit_soak();
                });
                self.process_environment_lighting_test_scene();
                self.process_hybrid_transparency_test_scene();
                self.tracer.drive_pending_ddgi_rebuild();

                if self.render_start_time.is_some() {
                    if let Some(spacing_voxels) =
                        self.environment_probe_rebuild_spacing_voxels.take()
                    {
                        log::info!(
                            "[DDGI][RUNTIME_REBUILD] requested spacing_voxels={spacing_voxels}"
                        );
                        self.tracer.rebuild_environment_probes(spacing_voxels);
                        log::info!(
                            "[DDGI][RUNTIME_REBUILD] complete spacing_voxels={spacing_voxels}"
                        );
                    }
                }

                let time_of_day_changed_by_gui =
                    self.debug_settings.adjustables.time_of_day.value != time_of_day_before_gui;
                let vsm_blur_radius_changed_by_gui =
                    self.debug_settings.adjustables.vsm_blur_radius.value
                        != vsm_blur_radius_before_gui;
                if time_of_day_changed_by_gui {
                    self.world_clock
                        .set_live_time_of_day(self.debug_settings.adjustables.time_of_day.value);
                }
                if time_of_day_changed_by_gui || vsm_blur_radius_changed_by_gui {
                    self.tracer.invalidate_local_direct_sun_shadow_histories();
                }

                self.world_clock.advance_daynight(
                    world_tick_steps,
                    world_tick_seconds,
                    self.debug_settings.adjustables.day_cycle_minutes.value,
                    self.debug_settings.adjustables.auto_daynight_cycle.value,
                );

                if self.render_flags.enable_particles {
                    if self.water_terrain_status().is_initialized()
                        && self.terrain_persistence.allows_water_simulation()
                    {
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
                self.process_water_experience_scene();

                self.apply_denoiser_benchmark_camera_motion();

                let gpu_record_start = Instant::now();
                let frame = match cpu_timings.time_if(
                    frame_perf_enabled,
                    FrameCpuScope::RenderAcquire,
                    || self.frame_manager.begin_frame(&mut self.swapchain),
                ) {
                    Ok(frame) => frame,
                    Err(SwapchainFrameError::OutOfDate) => {
                        self.queue_current_frame_extent();
                        return;
                    }
                    Err(error) => panic!("Error while acquiring next image. Cause: {}", error),
                };
                let frame_slot = frame.frame_slot();
                self.collect_gpu_profiler_frame(frame_slot);
                let cmdbuf = frame.command_buffer();
                let frame_extent_generation = frame.frame_extent_generation();
                assert_eq!(
                    frame_extent_generation,
                    self.swapchain.frame_extent_generation(),
                    "acquired frame extent generation is not the active swapchain generation"
                );
                self.tracer
                    .assert_frame_extent_generation(frame_extent_generation);
                let render_area = frame.extent();

                let render_record_start = frame_perf_enabled.then(Instant::now);
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
                    self.world_clock.live_time_of_day(),
                    self.debug_settings.adjustables.latitude.value,
                    self.debug_settings.adjustables.season.value,
                );
                let sun_dir = get_sun_dir(sun_altitude.asin().to_degrees(), sun_azimuth * 360.0);

                if !self.sprinklers.is_empty() {
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

                if self.terrain_moisture.has_chunks() {
                    let moisture_spread_gpu_scope =
                        self.gpu_profiler.as_mut().and_then(|profiler| {
                            profiler.begin_scope(
                                frame_slot,
                                cmdbuf,
                                "moisture_spread.pass",
                                PipelineStage::COMPUTE_SHADER,
                            )
                        });
                    self.terrain_moisture.record_spread(
                        &mut self.plain_builder,
                        cmdbuf,
                        self.perf_logging,
                    );
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
                        self.debug_settings.adjustables.dither_strength_lsb.value,
                        self.debug_settings
                            .adjustables
                            .raster_flora_ddgi_lighting
                            .value,
                        self.debug_settings.adjustables.path_tracing_reference.value,
                        self.debug_settings
                            .adjustables
                            .path_tracing_max_bounces
                            .value,
                        self.debug_settings
                            .adjustables
                            .path_tracing_ambient_light
                            .get_vec3(),
                        self.debug_settings
                            .adjustables
                            .terrain_ray_origin_offset_world
                            .value,
                        self.debug_settings
                            .adjustables
                            .ddgi_receiver_visibility_bias_world
                            .value,
                        self.debug_settings.adjustables.ddgi_history_retention.value,
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
                        self.world_clock.flora_tick(),
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
                        self.debug_settings.adjustables.god_ray_temporal_blend.value,
                        self.debug_settings.adjustables.god_ray_temporal_alpha.value,
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
                self.tracer.record_host_buffer_writes(cmdbuf);

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
                cpu_timings
                    .time_if(
                        frame_perf_enabled,
                        FrameCpuScope::RenderShadowPrepassRecord,
                        || {
                            self.tracer.record_shadow_prepass(
                                cmdbuf,
                                self.surface_builder.get_resources(),
                                self.time_info.time_since_start(),
                                leaf_color_tables,
                                &self.render_flags,
                                update_shadow_map,
                                self.debug_settings.adjustables.vsm_blur_radius.value,
                                vsm_temporal_alpha,
                                leaf_shadow_temporal_alpha,
                                gpu_profiler_for_shadow.as_mut(),
                                frame_slot,
                            )
                        },
                    )
                    .unwrap();
                if let Some(scope) = shadow_prepass_gpu_scope {
                    if let Some(profiler) = gpu_profiler_for_shadow.as_mut() {
                        profiler.end_scope(frame_slot, cmdbuf, scope, PipelineStage::ALL_COMMANDS);
                    }
                }
                self.gpu_profiler = gpu_profiler_for_shadow;

                if self.terrain_moisture.has_chunks() {
                    let moisture_dry_gpu_scope = self.gpu_profiler.as_mut().and_then(|profiler| {
                        profiler.begin_scope(
                            frame_slot,
                            cmdbuf,
                            "moisture_dry.pass",
                            PipelineStage::COMPUTE_SHADER,
                        )
                    });
                    let direct_shadow_available_mask =
                        self.tracer.direct_sun_shadow_available_mask();
                    self.terrain_moisture.record_dry(
                        &mut self.plain_builder,
                        &self.contree_builder,
                        cmdbuf,
                        sun_dir,
                        DIRECT_SUN_SHADOW_SOURCE_ALL,
                        direct_shadow_available_mask,
                        self.perf_logging,
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
                cpu_timings
                    .time_if(frame_perf_enabled, FrameCpuScope::RenderTraceRecord, || {
                        self.tracer.record_trace_after_shadow_prepass(
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
                    })
                    .unwrap();
                if let Some(scope) = tracer_gpu_scope {
                    if let Some(profiler) = gpu_profiler_for_trace.as_mut() {
                        profiler.end_scope(frame_slot, cmdbuf, scope, PipelineStage::ALL_COMMANDS);
                    }
                }
                self.gpu_profiler = gpu_profiler_for_trace;

                let mut environment_irradiance_readback =
                    match self.environment_irradiance_capture.record_if_ready(
                        &self.tracer,
                        &self.vulkan_ctx,
                        cmdbuf,
                        self.environment_lighting_test_scene.as_ref(),
                        self.time_info.total_frame_count(),
                    ) {
                        Ok(readback) => readback,
                        Err(err) => {
                            log::error!("[ENV_IRRADIANCE_CAPTURE] failed to prepare: {err:#}");
                            None
                        }
                    };

                let mut ddgi_spatial_weight_readback = match self
                    .ddgi_spatial_weight_readback
                    .record_if_ready(&self.tracer, &self.vulkan_ctx, cmdbuf)
                {
                    Ok(readback) => readback,
                    Err(err) => {
                        log::error!("[DDGI_SPATIAL_WEIGHT_READBACK] failed to prepare: {err:#}");
                        None
                    }
                };

                if self.resize_lifecycle_test.is_some() {
                    log::info!(
                        "[RESIZE_LIFECYCLE] phase=frame frame_generation={} swapchain_generation={} tracer_generation={} extent={}x{}",
                        frame_extent_generation.serial(),
                        self.swapchain.frame_extent_generation().serial(),
                        self.tracer.frame_extent_generation().serial(),
                        render_area.width,
                        render_area.height,
                    );
                }

                self.swapchain.record_blit(
                    self.tracer.get_screen_output_tex().get_image(),
                    cmdbuf,
                    &frame,
                );
                let device = self.vulkan_ctx.device();
                self.egui_renderer.prepare_command_buffer(device, cmdbuf);
                self.swapchain
                    .record_begin_render_pass_cmdbuf(cmdbuf, &frame);

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

                let screenshot_readiness = ScreenshotFrameReadiness::new(
                    self.render_start_time
                        .map(|started| started.elapsed().as_secs_f32()),
                    self.environment_lighting_test_scene.as_ref().is_none_or(
                        environment_lighting_test_scene::EnvironmentLightingTestScene::is_ready,
                    ) && self.hybrid_transparency_test_scene.as_ref().is_none_or(
                        hybrid_transparency_test_scene::HybridTransparencyTestScene::is_ready,
                    ),
                    self.tracer.ddgi_ready(),
                );
                let mut screenshot_readback = self.screenshot_capture.record_if_ready(
                    &self.tracer,
                    &self.vulkan_ctx,
                    &self.swapchain,
                    &frame,
                    screenshot_readiness,
                );
                let mut denoiser_frame_readback = if screenshot_readback.is_none()
                    && self
                        .denoiser_bench
                        .as_ref()
                        .is_some_and(DenoiserBench::should_capture)
                {
                    Some(
                        PendingDenoiserFrame::record(
                            &self.tracer,
                            &self.vulkan_ctx,
                            &self.swapchain,
                            &frame,
                        )
                        .unwrap_or_else(|err| {
                            panic!("[DENOISER_BENCH] Failed to prepare readback: {err:#}")
                        }),
                    )
                } else {
                    None
                };

                if let Some(scope) = frame_gpu_scope {
                    if let Some(profiler) = self.gpu_profiler.as_mut() {
                        profiler.end_scope(frame_slot, cmdbuf, scope, PipelineStage::ALL_COMMANDS);
                    }
                }

                cmdbuf.end();
                if let Some(start) = render_record_start {
                    cpu_timings.add_ms(
                        FrameCpuScope::RenderRecord,
                        start.elapsed().as_secs_f32() * 1000.0,
                    );
                }

                let present_result = cpu_timings.time_if(
                    frame_perf_enabled,
                    FrameCpuScope::RenderSubmitPresent,
                    || {
                        self.frame_manager.submit_and_present(
                            &self.vulkan_ctx,
                            &mut self.swapchain,
                            &frame,
                        )
                    },
                );
                let gpu_ms = gpu_record_start.elapsed().as_secs_f32() * 1000.0;

                match present_result {
                    Ok(is_suboptimal) if is_suboptimal => {
                        self.queue_current_frame_extent();
                    }
                    Err(SwapchainFrameError::OutOfDate) => {
                        self.queue_current_frame_extent();
                    }
                    Err(error) => panic!("Failed to present queue. Cause: {}", error),
                    _ => {}
                }

                let mut denoiser_bench_complete = false;
                let mut environment_irradiance_capture_complete = false;
                let mut ddgi_spatial_weight_readback_complete = false;
                if screenshot_readback.is_some()
                    || denoiser_frame_readback.is_some()
                    || environment_irradiance_readback.is_some()
                    || ddgi_spatial_weight_readback.is_some()
                {
                    // Finish the GPU copy before handing CPU processing to a worker. Waiting on
                    // this frame's fence from the worker raced the frame manager resetting the
                    // same fence when its slot was reused.
                    match frame.wait_until_complete() {
                        Ok(()) => {
                            if let Some(readback) = environment_irradiance_readback.take() {
                                match self.environment_irradiance_capture.complete(
                                    readback,
                                    self.environment_lighting_test_scene.as_mut(),
                                ) {
                                    Ok(sequence_complete) => {
                                        environment_irradiance_capture_complete = sequence_complete;
                                    }
                                    Err(err) => {
                                        log::error!(
                                            "[ENV_IRRADIANCE_CAPTURE] failed to write capture: {err:#}"
                                        );
                                    }
                                }
                            }
                            if let Some(readback) = ddgi_spatial_weight_readback.take() {
                                match self.ddgi_spatial_weight_readback.complete(readback) {
                                    Ok(()) => {
                                        ddgi_spatial_weight_readback_complete = true;
                                    }
                                    Err(err) => log::error!(
                                        "[DDGI_SPATIAL_WEIGHT_READBACK] failed to write: {err:#}"
                                    ),
                                }
                            }
                            if let Some(readback) = screenshot_readback.take() {
                                self.screenshot_capture.complete(readback);
                            }
                            if let Some(readback) = denoiser_frame_readback.take() {
                                denoiser_bench_complete = readback
                                    .complete(
                                        self.denoiser_bench
                                            .as_mut()
                                            .expect("benchmark readback requires benchmark state"),
                                    )
                                    .unwrap_or_else(|err| {
                                        panic!("[DENOISER_BENCH] Failed to record frame: {err:#}")
                                    });
                            }
                        }
                        Err(err) => {
                            log::error!("[READBACK] Failed while waiting for GPU readback: {}", err)
                        }
                    }
                }
                self.process_radiance_test_mutation_after_render();
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
                    log::info!(
                        "[PERF][CPU_FRAME_SCOPE] frame {} frame.cpu_total={:.0}us frame.egui={:.0}us render.path={:.0}us render.acquire={:.0}us render.record={:.0}us render.shadow_prepass_record={:.0}us render.trace_record={:.0}us render.submit_present={:.0}us frame.tracked_cpu={:.0}us frame.untracked_cpu={:.0}us",
                        frame_count,
                        total_ms * 1000.0,
                        egui_ms * 1000.0,
                        gpu_ms * 1000.0,
                        frame_timing_snapshot.render_acquire_ms * 1000.0,
                        frame_timing_snapshot.render_record_ms * 1000.0,
                        frame_timing_snapshot.render_shadow_prepass_record_ms * 1000.0,
                        frame_timing_snapshot.render_trace_record_ms * 1000.0,
                        frame_timing_snapshot.render_submit_present_ms * 1000.0,
                        frame_timing_snapshot.tracked_cpu_ms * 1000.0,
                        frame_timing_snapshot.untracked_cpu_ms * 1000.0,
                    );
                }
                if frame_perf_enabled {
                    let queue_work_ms = cpu_timings.queue_work_ms();
                    if frame_count.is_multiple_of(30) || total_ms >= 16.0 || queue_work_ms >= 2.0 {
                        let water_terrain = self.water_terrain_status().diagnostics();
                        log::info!(
                            "[PERF][FRAME] frame {} total {:.2}ms egui {:.2}ms gpu_present {:.2}ms contree_poll {:.2}ms terrain_source {:.2}ms cache_queue {:.2}ms collider_queue {:.2}ms water_edit_soak {:.2}ms water_handoff {:.2}ms particles {:.2}ms tracked_cpu {:.2}ms untracked_cpu {:.2}ms queues source_pending={} source_active={} collider_pending={} collider_active={} collider_inflight={} cache_pending={} cache_active={} cache_inflight={}",
                            frame_count,
                            total_ms,
                            egui_ms,
                            gpu_ms,
                            frame_timing_snapshot.contree_poll_ms,
                            frame_timing_snapshot.terrain_source_ms,
                            frame_timing_snapshot.water_cache_ms,
                            frame_timing_snapshot.collider_queue_ms,
                            frame_timing_snapshot.water_edit_soak_ms,
                            frame_timing_snapshot.water_handoff_ms,
                            frame_timing_snapshot.particles_ms,
                            frame_timing_snapshot.tracked_cpu_ms,
                            frame_timing_snapshot.untracked_cpu_ms,
                            water_terrain.source_pending,
                            water_terrain.source_active,
                            water_terrain.collider_pending,
                            water_terrain.collider_active,
                            water_terrain.collider_inflight,
                            water_terrain.cache_pending,
                            water_terrain.cache_active,
                            water_terrain.cache_inflight,
                        );
                    }
                }
                if let Some(render_start_time) = self.render_start_time {
                    let elapsed = render_start_time.elapsed().as_secs_f32();

                    if let Some(auto_exit_delay) = self.auto_exit_delay {
                        if elapsed >= auto_exit_delay {
                            if let Some(test) = self.egui_texture_lifecycle_test.as_ref() {
                                if !test.completed {
                                    panic!(
                                        "[EGUI_TEXTURE_LIFECYCLE] timed out before full/partial/replacement/free sequence completed step={}",
                                        test.step,
                                    );
                                }
                            }
                            if let Some(test) = self.resize_lifecycle_test.as_ref() {
                                if !test.published_after_latest_request() {
                                    panic!(
                                        "[RESIZE_LIFECYCLE] timed out before a coherent publication followed the latest request requested={} observed={:?}",
                                        test.requested,
                                        test.observed,
                                    );
                                }
                            }
                            if let Some(scene) = self
                                .environment_lighting_test_scene
                                .as_ref()
                                .filter(|scene| scene.edit_cycle_target_revision().is_some())
                            {
                                let status = self.tracer.ddgi_runtime_status().active();
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
                if environment_irradiance_capture_complete {
                    log::info!(target: "re_flora::app::core::environment_irradiance_capture", "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run");
                    self.on_terminate(event_loop);
                }
                if ddgi_spatial_weight_readback_complete {
                    log::info!(
                        target: "re_flora::app::core::ddgi_spatial_weight_readback",
                        "[DDGI_SPATIAL_WEIGHT_READBACK] complete; exiting one-shot readback run"
                    );
                    self.on_terminate(event_loop);
                }
            }
            _ => (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{App, GlobalKeyboardCommand};
    use winit::{
        event::ElementState,
        keyboard::{KeyCode, PhysicalKey},
    };

    #[test]
    fn focused_gui_text_input_reserves_global_keyboard_shortcuts() {
        for key in [KeyCode::KeyR, KeyCode::Escape] {
            assert_eq!(
                App::global_keyboard_command(PhysicalKey::Code(key), ElementState::Pressed, true,),
                None,
                "{key:?} must stay with the focused GUI editor",
            );
        }

        assert_eq!(
            App::global_keyboard_command(
                PhysicalKey::Code(KeyCode::KeyR),
                ElementState::Pressed,
                false,
            ),
            Some(GlobalKeyboardCommand::ToggleConfigPanel),
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
    fn environmental_acoustics_quality_normalizes_and_clamps_gui_percent() {
        assert_eq!(App::environmental_acoustics_quality(0), 0.0);
        assert_eq!(App::environmental_acoustics_quality(50), 0.5);
        assert_eq!(App::environmental_acoustics_quality(100), 1.0);
        assert_eq!(App::environmental_acoustics_quality(101), 1.0);
    }
}
