mod resources;
pub use resources::*;

mod butterfly_palette;
pub use butterfly_palette::*;

mod palette_remap;

mod particle_texture_layout;
pub use particle_texture_layout::*;

mod sprinkler_resources;
pub use sprinkler_resources::*;

mod irrigation_pipe_resources;
pub use irrigation_pipe_resources::*;

mod geometry_preview_resources;
pub use geometry_preview_resources::*;

mod dynamic_fruit_resources;
pub use dynamic_fruit_resources::*;

pub mod tree_preview_mesh;

mod extent_dependent_resources;
pub use extent_dependent_resources::*;

mod vertex;
pub use vertex::*;

pub mod voxel_encoding;

mod voxel_geometry;

mod leaves_construct;
pub use leaves_construct::{
    collision_probe_apple_offsets, voxel_apple_offsets, TREE_FRUIT_MAX_RADIUS_VOXELS,
};

mod pipeline_builder;
use pipeline_builder::*;

mod buffer_updater;
use buffer_updater::*;

use glam::{IVec3, Mat4, UVec3, Vec2, Vec3, Vec4};
use winit::event::KeyEvent;

const LEAF_INSTANCE_TYPE: u32 = crate::flora::species::TREE_LEAF_RENDER_SPECIES_INDEX;
const APPLE_INSTANCE_TYPE: u32 = crate::flora::species::APPLE_RENDER_SPECIES_INDEX;
const APPLE_BOTTOM_COLOR: Vec3 = Vec3::new(0.48, 0.025, 0.018);
const APPLE_TIP_COLOR: Vec3 = Vec3::new(0.95, 0.06, 0.035);
pub const FLORA_HEIGHT_COLOR_TABLE_LEN: usize = 12;
pub type FloraHeightColorTables = [[u32; FLORA_HEIGHT_COLOR_TABLE_LEN]; 2];

use crate::audio::SpatialSoundManager;

use crate::builder::{
    ContreeBuilderResources, FloraInstanceResources, PlainBuilderResources,
    SceneAccelBuilderResources, SurfaceResources, TreeLeavesInstance,
};
use crate::ddgi::{
    DdgiDebugView, DdgiRayBatch, DdgiVolume, DDGI_GUTTER_WORKGROUP_SIZE,
    DDGI_IRRADIANCE_INTERIOR_SIDE, DDGI_IRRADIANCE_STORED_SIDE, DDGI_RELOCATION_WORKGROUP_SIZE,
    DDGI_TRACE_WORKGROUP_SIZE, DDGI_VISIBILITY_INTERIOR_SIDE,
};
use crate::environment_lighting::{EnvironmentLightingBackend, EnvironmentLightingCache};
use crate::environment_probes::{
    EnvironmentProbeVisualizationPushConstants, EnvironmentProbeVisualizationResources,
    EnvironmentProbeVisualizationSettings, EnvironmentProbeVolume, EnvironmentProbeVolumeStatus,
};
use crate::gameplay::{
    calculate_directional_light_matrices, Camera, CameraDesc, CameraPose, CameraVectors,
};
use crate::generated::gpu_structs::{PushConstantFlora, PushConstantLeafShadowTemporal};
use crate::geom::UAabb3;
use crate::particles::{ParticleSnapshot, PARTICLE_CAPACITY};
use crate::resource::ResourceContainer;
use crate::util::TimeInfo;
use crate::wind::WindSource;
use anyhow::Result;
use re_flora_vkn::vk;
use re_flora_vkn::{
    execute_one_time_gpu_job, Allocator, AttachmentDescOuter, AttachmentType, Buffer, ClearValue,
    ColorClearValue, CommandBuffer, ComputePipeline, DepthOrStencilClearValue, DescriptorPool,
    Extent2D, Extent3D, Framebuffer, GpuProfiler, GraphicsPipeline, MemoryAccess, MemoryBarrier,
    PipelineBarrier, PipelineStage, PushConstantInfo, RenderPass, RenderTarget, Texture,
    TextureLayout, Viewport, VulkanContext, WriteDescriptorSet,
};
use std::collections::HashMap;
use std::time::Instant;

const MAX_TERRAIN_QUERIES: usize = 1_000;
const SHADOW_MAP_RESOLUTION: u32 = 1024;
const CLOUD_SHADOW_MAP_RESOLUTION: u32 = 256;
const LEAF_SHADOW_OPACITY_RESOLUTION: u32 = 2048;
const ENVIRONMENT_PROBE_COEFFICIENT_BINDING: u32 = 18;
const ENVIRONMENT_PROBE_SUMMARY_BINDING: u32 = 19;
const ENVIRONMENT_PROBE_VISIBILITY_BINDING: u32 = 20;
const ENVIRONMENT_PROBE_DIRECTION_BINDING: u32 = 21;
const ENVIRONMENT_PROBE_STATS_BINDING: u32 = 22;
const DDGI_GLOBAL_SKY_BINDING: u32 = 23;
const DDGI_PROBE_METADATA_BINDING: u32 = 24;
const DDGI_IRRADIANCE_ATLAS_BINDING: u32 = 27;
const DDGI_VISIBILITY_ATLAS_BINDING: u32 = 28;
const ENVIRONMENT_PROBE_TRACE_BATCH_SIZE: u32 = 128;
const ENVIRONMENT_PROBE_TRACE_RAYS_PER_PROBE: u32 = 70;
const ENVIRONMENT_PROBE_PRIORITY_REGION_RADIUS: u32 = 1;
pub(super) const WIND_VOLUME_BUCKET_COUNT: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WindVolumePushConstants {
    time: f32,
    bucket_index: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EnvironmentProbeUpdatePushConstants {
    first_probe_index: u32,
    probe_count: u32,
    environment_revision: u32,
    trace_visibility: u32,
    priority_grid_min: [u32; 3],
    priority_side: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EnvironmentProbeClassificationPushConstants {
    terrain_revision: u32,
    _padding: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DdgiProbeRelocationPushConstants {
    grid_dimensions: [u32; 3],
    spacing_voxels: u32,
    voxels_per_world_unit: [f32; 3],
    terrain_revision: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DdgiProbeTracePushConstants {
    first_probe_index: u32,
    probe_count: u32,
    terrain_revision: u32,
    _padding: u32,
    far_distance_world: f32,
    _padding_float: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DdgiAtlasFilterPushConstants {
    first_probe_index: u32,
    probe_count: u32,
    tile_columns: u32,
    terrain_revision: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DdgiVisibilityFilterPushConstants {
    first_probe_index: u32,
    probe_count: u32,
    tile_columns: u32,
    terrain_revision: u32,
    spacing_world: [f32; 3],
    far_distance_world: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DdgiAtlasGutterPushConstants {
    first_probe_index: u32,
    probe_count: u32,
    tile_columns: u32,
    _padding: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EnvironmentProbeStatsPushConstants {
    probe_count: u32,
    _padding: [u32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EnvironmentProbeTraceRegion {
    grid_min: UVec3,
    side: u32,
}

impl EnvironmentProbeTraceRegion {
    fn dispatch_probe_count(self) -> u32 {
        self.side * self.side * self.side
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlassPushConstants {
    box_min_near_alpha: [f32; 4],
    box_max_far_alpha: [f32; 4],
}

const TERRARIUM_GLASS_NEAR_ALPHA: f32 = 0.025;
const TERRARIUM_GLASS_FAR_ALPHA: f32 = 0.070;
const DEFAULT_CAMERA_DISTANCE_SCALE: f32 = 0.7;
const DEFAULT_CAMERA_DISTANCE_PADDING: f32 = 0.65;
const DEFAULT_CAMERA_HEIGHT: f32 = 1.0;

#[derive(Debug, Clone)]
pub struct WindGuiParams {
    pub sources: Vec<WindSource>,
}

#[derive(Debug, Clone, Copy)]
pub enum TerrainEditPreviewShape {
    Sphere,
    SurfaceCircle,
}

impl TerrainEditPreviewShape {
    fn as_u32(self) -> u32 {
        match self {
            Self::Sphere => 0,
            Self::SurfaceCircle => 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GlassGuiParams {
    pub tint: Vec3,
    pub reflection_strength: f32,
    pub ssr_strength: f32,
    pub ssr_steps: u32,
    pub per_voxel_reflection: bool,
    pub ssr_min_hit_thickness_voxels: f32,
    pub ssr_footprint_pixels: f32,
    pub refraction_strength: f32,
    pub alpha: f32,
    pub glint_strength: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct CloudGuiParams {
    pub enabled: bool,
    pub coverage: f32,
    pub density: f32,
    pub bottom_height: f32,
    pub top_height: f32,
    pub shape_scale: f32,
    pub detail_scale: f32,
    pub detail_strength: f32,
    pub wind_speed: f32,
    pub primary_steps: u32,
    pub light_steps: u32,
    pub temporal_alpha: f32,
    pub absorption: f32,
    pub phase_eccentricity: f32,
    pub silver_intensity: f32,
    pub max_distance: f32,
    pub shadows_enabled: bool,
    pub shadow_strength: f32,
    pub shadow_min_transmittance: f32,
    pub shadow_steps: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WindSourceGpu {
    pub params: [f32; 4],
    pub noise: [f32; 4],
}

impl From<WindSource> for WindSourceGpu {
    fn from(source: WindSource) -> Self {
        Self {
            params: [source.direction_degrees, source.speed, source.gain, 0.0],
            noise: [
                source.pattern_scale,
                source.octaves as f32,
                source.lacunarity,
                source.persistence,
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VsmFilterPushConstants {
    blur_radius: u32,
    temporal_alpha: f32,
    reset_history: u32,
    _pad0: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CloudTemporalPushConstants {
    reset_history: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CloudShadowTemporalPushConstants {
    reset_history: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct TerrainRayQuery {
    pub origin: Vec3,
    pub direction: Vec3,
}

fn should_render_grass_species(species_index: usize, grass_render_mode: u32) -> bool {
    match species_index {
        0 => grass_render_mode != 2,
        1 => grass_render_mode != 1,
        _ => true,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TerrainRayHitSample {
    pub position: Vec3,
    pub is_valid: bool,
}

fn srgb_to_linear_channel(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_to_linear_color(color: Vec3) -> Vec3 {
    Vec3::new(
        srgb_to_linear_channel(color.x),
        srgb_to_linear_channel(color.y),
        srgb_to_linear_channel(color.z),
    )
}

fn pack_linear_rgb10(color: Vec3) -> u32 {
    let quantize = |channel: f32| -> u32 { (channel.clamp(0.0, 1.0) * 1023.0).round() as u32 };
    quantize(color.x) | (quantize(color.y) << 10) | (quantize(color.z) << 20)
}

fn flora_height_color_table(
    bottom_srgb: Vec3,
    tip_srgb: Vec3,
) -> [u32; FLORA_HEIGHT_COLOR_TABLE_LEN] {
    let bottom_linear = srgb_to_linear_color(bottom_srgb);
    let tip_linear = srgb_to_linear_color(tip_srgb);
    let mut table = [0; FLORA_HEIGHT_COLOR_TABLE_LEN];
    for (row, color) in table.iter_mut().enumerate() {
        let height_t = row as f32 / (FLORA_HEIGHT_COLOR_TABLE_LEN - 1) as f32;
        *color = pack_linear_rgb10(bottom_linear.lerp(tip_linear, height_t));
    }
    table
}

pub fn solid_flora_height_color_tables(
    bottom_srgb: Vec3,
    tip_srgb: Vec3,
) -> FloraHeightColorTables {
    let table = flora_height_color_table(bottom_srgb, tip_srgb);
    [table, table]
}

pub fn kochia_color_tables(color_a_srgb: Vec3, color_b_srgb: Vec3) -> FloraHeightColorTables {
    let color_a = pack_linear_rgb10(srgb_to_linear_color(color_a_srgb));
    let color_b = pack_linear_rgb10(srgb_to_linear_color(color_b_srgb));
    [
        [color_a; FLORA_HEIGHT_COLOR_TABLE_LEN],
        [color_b; FLORA_HEIGHT_COLOR_TABLE_LEN],
    ]
}

pub fn allium_height_color_tables(
    stem_bottom_srgb: Vec3,
    stem_top_srgb: Vec3,
    flower_a_srgb: Vec3,
    flower_b_srgb: Vec3,
) -> FloraHeightColorTables {
    let stem_bottom = srgb_to_linear_color(stem_bottom_srgb);
    let stem_top = srgb_to_linear_color(stem_top_srgb);
    let flower_a = srgb_to_linear_color(flower_a_srgb);
    let flower_b = srgb_to_linear_color(flower_b_srgb);
    let mut table_a = [0; FLORA_HEIGHT_COLOR_TABLE_LEN];
    let mut table_b = [0; FLORA_HEIGHT_COLOR_TABLE_LEN];

    for row in 0..FLORA_HEIGHT_COLOR_TABLE_LEN {
        let height_t = row as f32 / (FLORA_HEIGHT_COLOR_TABLE_LEN - 1) as f32;
        if height_t < 0.64 {
            let stem_color = stem_bottom.lerp(stem_top, height_t / 0.64);
            table_a[row] = pack_linear_rgb10(stem_color);
            table_b[row] = pack_linear_rgb10(stem_color);
        } else {
            table_a[row] = pack_linear_rgb10(flower_a);
            table_b[row] = pack_linear_rgb10(flower_b);
        }
    }

    [table_a, table_b]
}

#[cfg(test)]
mod flora_color_tests {
    use super::*;

    #[test]
    fn kochia_palette_contains_two_height_independent_colors() {
        let tables = kochia_color_tables(Vec3::new(0.84, 0.72, 0.30), Vec3::new(0.85, 0.32, 0.37));

        assert_ne!(tables[0][0], tables[1][0]);
        assert!(tables[0].iter().all(|color| *color == tables[0][0]));
        assert!(tables[1].iter().all(|color| *color == tables[1][0]));
    }
}

#[cfg(test)]
mod default_camera_tests {
    use super::*;

    #[test]
    fn default_camera_pose_frames_requested_focus_from_above() {
        let bound = UAabb3::new(UVec3::ZERO, UVec3::splat(2));
        let focus = Vec3::new(1.0, 0.5, 1.0);
        let (position, yaw_deg, pitch_deg) = Tracer::default_camera_pose_for_bound(bound, focus);

        assert_eq!(position.x, focus.x);
        assert!(position.y > focus.y);
        assert!(position.z > focus.z);

        let yaw = yaw_deg.to_radians();
        let pitch = pitch_deg.to_radians();
        let camera_front = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        )
        .normalize();
        assert!(camera_front.distance((focus - position).normalize()) < 1.0e-6);
    }
}

pub fn grass_flora_height_color_tables(
    bottom_dark_srgb: Vec3,
    bottom_light_srgb: Vec3,
    tip_dark_srgb: Vec3,
    tip_light_srgb: Vec3,
) -> FloraHeightColorTables {
    [
        flora_height_color_table(bottom_dark_srgb, tip_dark_srgb),
        flora_height_color_table(bottom_light_srgb, tip_light_srgb),
    ]
}

fn flora_push_constant(
    time: f32,
    instance_ty: u32,
    chunk_world_offset: UVec3,
    height_color_tables: FloraHeightColorTables,
) -> PushConstantFlora {
    PushConstantFlora {
        time,
        instance_ty,
        chunk_world_offset: chunk_world_offset.to_array(),
        height_dark_color_rgb10: height_color_tables[0],
        height_light_color_rgb10: height_color_tables[1],
        ..bytemuck::Zeroable::zeroed()
    }
}

#[derive(Debug, Clone, Copy)]
struct TreeRenderInstanceData {
    world_pos: UVec3,
    leaf_local_pos: IVec3,
}

#[derive(Clone, Copy, Debug)]
pub struct TracerDesc {
    pub scaling_factor: f32,
    pub default_camera_look_at: Vec3,
    pub voxel_dim_per_chunk: UVec3,
    pub environment_probe_spacing_voxels: u32,
    pub environment_probe_visualization_enabled: bool,
    pub environment_lighting_backend: EnvironmentLightingBackend,
    pub environment_irradiance_capture_enabled: bool,
    pub ddgi_debug_view: DdgiDebugView,
}

#[derive(Debug, Clone, Copy)]
pub struct FruitMotionParams {
    pub swing_length_voxels: f32,
    pub max_angle_radians: f32,
    pub swing_speed: f32,
    pub speed_variation: f32,
    pub min_response: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct KochiaMotionParams {
    pub body_wind_response: f32,
    pub branch_jelly_amplitude_voxels: f32,
    pub branch_jelly_speed: f32,
    pub branch_phase_spread: f32,
    pub tip_flutter_amplitude_voxels: f32,
    pub tip_flutter_speed: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct KochiaVisualParams {
    pub bottom_darkening: f32,
    pub branch_value_variation: f32,
    pub voxel_value_variation: f32,
    pub branch_count: u32,
    pub bottom_diameter_voxels: f32,
    pub waist_diameter_voxels: f32,
    pub top_diameter_voxels: f32,
    pub waist_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LodState {
    Lod0,
    Lod1,
}

pub const DIRECT_SUN_SHADOW_SOURCE_TERRAIN: u32 = 1 << 0;
pub const DIRECT_SUN_SHADOW_SOURCE_LEAF: u32 = 1 << 1;
pub const DIRECT_SUN_SHADOW_SOURCE_CLOUD: u32 = 1 << 2;
pub const DIRECT_SUN_SHADOW_SOURCE_ALL: u32 = DIRECT_SUN_SHADOW_SOURCE_TERRAIN
    | DIRECT_SUN_SHADOW_SOURCE_LEAF
    | DIRECT_SUN_SHADOW_SOURCE_CLOUD;

pub struct DirectSunShadowResources<'a> {
    pub gui_input: &'a Buffer,
    pub shadow_camera_info: &'a Buffer,
    pub shadow_map_tex_for_vsm_ping: &'a Texture,
    pub leaf_shadow_opacity_blended_tex: &'a Texture,
    pub leaf_shadow_mask_tex: &'a Texture,
    pub cloud_shadow_tex: &'a Texture,
}

pub struct Tracer {
    vulkan_ctx: VulkanContext,

    desc: TracerDesc,
    chunk_bound: UAabb3,

    allocator: Allocator,
    resources: TracerResources,
    particle_resources: ParticleRendererResources,
    sprinkler_resources: SprinklerRendererResources,
    irrigation_pipe_resources: IrrigationPipeRendererResources,
    geometry_preview_resources: GeometryPreviewRendererResources,
    dynamic_fruit_resources: DynamicFruitRendererResources,
    environment_probe_visualization_resources: EnvironmentProbeVisualizationResources,

    camera: Camera,
    camera_view_mat_prev_frame: Mat4,
    camera_proj_mat_prev_frame: Mat4,
    current_view_proj_mat: Mat4,
    shadow_camera_initialized: bool,
    shadow_map_history_valid: bool,
    leaf_shadow_history_valid: bool,
    cloud_history_valid: bool,
    cloud_shadow_history_valid: bool,
    environment_lighting: EnvironmentLightingCache,
    environment_lighting_backend: EnvironmentLightingBackend,
    environment_probes: EnvironmentProbeVolume,
    ddgi_volume: Option<DdgiVolume>,
    ddgi_initial_terrain_ready_revision: Option<u32>,
    ddgi_trace_stats_readback_pending: Option<DdgiRayBatch>,
    environment_probe_environment_revision: u32,
    environment_probe_seeded_revision: u32,
    environment_probe_classification_pending: bool,
    environment_probe_placement_initialized: bool,
    environment_probe_trace_cursor: u32,
    environment_probe_local_field_ready: bool,
    environment_probe_priority_regions: [Option<EnvironmentProbeTraceRegion>; 2],
    environment_probe_terrain_revision: u32,
    environment_probe_converged_terrain_revision: u32,
    environment_probe_refresh_started_at: Option<Instant>,
    environment_probe_stats_readback_pending: bool,
    environment_probe_visualization: EnvironmentProbeVisualizationSettings,

    compute_pipelines: ComputePipelines,
    graphics_pipelines: GraphicsPipelines,

    render_target_color_and_depth: RenderTarget,
    render_target_depth_only: RenderTarget,
    render_target_leaf_shadow_opacity: RenderTarget,
    render_target_gui: RenderTarget,

    #[allow(dead_code)]
    pool: DescriptorPool,

    world_tick_seconds: f32,
    last_wind_volume_step: Option<u32>,
    initialized_wind_volume_bucket_count: u32,
    wind_source_buffer_capacity: usize,
    spatial_sound_manager: SpatialSoundManager,
    particle_instance_scratch: Vec<ParticleInstanceGpu>,
    translucent_particle_instance_scratch: Vec<ParticleInstanceGpu>,
}

impl Drop for Tracer {
    fn drop(&mut self) {}
}

impl Tracer {
    pub fn direct_sun_shadow_resources(&self) -> DirectSunShadowResources<'_> {
        DirectSunShadowResources {
            gui_input: &self.resources.uniforms.gui_input,
            shadow_camera_info: &self.resources.shadow.shadow_camera_info,
            shadow_map_tex_for_vsm_ping: &self.resources.shadow.shadow_map_tex_for_vsm_ping,
            leaf_shadow_opacity_blended_tex: &self.resources.shadow.leaf_shadow_opacity_blended_tex,
            leaf_shadow_mask_tex: &self.resources.shadow.leaf_shadow_mask_tex,
            cloud_shadow_tex: &self.resources.shadow.cloud_shadow_tex,
        }
    }

    pub fn direct_sun_shadow_available_mask(&self) -> u32 {
        let mut mask = 0;
        if self.terrain_shadow_vsm_ready() {
            mask |= DIRECT_SUN_SHADOW_SOURCE_TERRAIN;
        }
        if self.leaf_shadow_history_valid {
            mask |= DIRECT_SUN_SHADOW_SOURCE_LEAF;
        }
        if self.cloud_shadow_history_valid {
            mask |= DIRECT_SUN_SHADOW_SOURCE_CLOUD;
        }
        mask
    }

    pub fn terrain_shadow_vsm_ready(&self) -> bool {
        self.shadow_camera_initialized && self.shadow_map_history_valid
    }

    fn default_camera_pose_for_bound(
        chunk_bound: UAabb3,
        default_camera_look_at: Vec3,
    ) -> (Vec3, f32, f32) {
        let min = chunk_bound.min().as_vec3();
        let max = chunk_bound.max().as_vec3();
        let extent = max - min;
        let horizontal_extent = extent.x.max(extent.z).max(1.0);
        let camera_distance =
            horizontal_extent * DEFAULT_CAMERA_DISTANCE_SCALE + DEFAULT_CAMERA_DISTANCE_PADDING;
        let camera_position = Vec3::new(
            default_camera_look_at.x,
            DEFAULT_CAMERA_HEIGHT,
            default_camera_look_at.z + camera_distance,
        );
        let look_direction = (default_camera_look_at - camera_position).normalize();
        let yaw_deg = look_direction.x.atan2(-look_direction.z).to_degrees();
        let horizontal_len = Vec2::new(look_direction.x, look_direction.z).length();
        let pitch_deg = look_direction.y.atan2(horizontal_len).to_degrees();
        (camera_position, yaw_deg, pitch_deg)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        chunk_bound: UAabb3,
        screen_extent: Extent2D,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
        plain_builder_resources: &PlainBuilderResources,
        desc: TracerDesc,
        spatial_sound_manager: SpatialSoundManager,
    ) -> Result<Self> {
        let render_extent = Self::get_render_extent(screen_extent, desc.scaling_factor);
        let (camera_position, camera_yaw_deg, camera_pitch_deg) =
            Self::default_camera_pose_for_bound(chunk_bound, desc.default_camera_look_at);

        let camera = Camera::new(
            camera_position,
            camera_yaw_deg,
            camera_pitch_deg,
            CameraDesc {
                aspect_ratio: render_extent.get_aspect_ratio(),
                ..Default::default()
            },
            spatial_sound_manager.clone(),
        )?;

        let pool = DescriptorPool::new(vulkan_ctx.device()).unwrap();

        let shader_modules = PipelineBuilder::create_shader_modules(&vulkan_ctx)?;

        let resources = TracerResources::new(
            &vulkan_ctx,
            allocator.clone(),
            &shader_modules.tracer_sm,
            &shader_modules.tracer_shadow_sm,
            &shader_modules.composition_sm,
            &shader_modules.cloud_temporal_sm,
            &shader_modules.god_ray_sm,
            &shader_modules.post_processing_sm,
            &shader_modules.player_collider_sm,
            &shader_modules.terrain_query_sm,
            &shader_modules.flora_vert_sm,
            chunk_bound,
            render_extent,
            screen_extent,
            Extent2D::new(SHADOW_MAP_RESOLUTION, SHADOW_MAP_RESOLUTION),
            Extent2D::new(CLOUD_SHADOW_MAP_RESOLUTION, CLOUD_SHADOW_MAP_RESOLUTION),
            Extent2D::new(
                LEAF_SHADOW_OPACITY_RESOLUTION,
                LEAF_SHADOW_OPACITY_RESOLUTION,
            ),
            MAX_TERRAIN_QUERIES as u32,
        );
        let particle_resources =
            ParticleRendererResources::new(vulkan_ctx.device().clone(), allocator.clone());
        let sprinkler_resources =
            SprinklerRendererResources::new(vulkan_ctx.device().clone(), allocator.clone());
        let irrigation_pipe_resources =
            IrrigationPipeRendererResources::new(vulkan_ctx.device().clone(), allocator.clone());
        let geometry_preview_resources =
            GeometryPreviewRendererResources::new(vulkan_ctx.device().clone(), allocator.clone());
        let dynamic_fruit_resources =
            DynamicFruitRendererResources::new(vulkan_ctx.device().clone(), allocator.clone());
        let environment_probes = EnvironmentProbeVolume::new(
            vulkan_ctx.device().clone(),
            allocator.clone(),
            chunk_bound.dimensions() * desc.voxel_dim_per_chunk,
            desc.voxel_dim_per_chunk,
            desc.environment_probe_spacing_voxels,
        )?;
        // The temporary A/B selector shares shader modules, so both variants need valid DDGI
        // descriptors. Only the selected DDGI backend schedules initialization or update work.
        let ddgi_volume = Some(DdgiVolume::new(
            &vulkan_ctx,
            allocator.clone(),
            chunk_bound.dimensions() * desc.voxel_dim_per_chunk,
            desc.environment_probe_spacing_voxels,
        )?);
        let environment_probe_visualization_resources = EnvironmentProbeVisualizationResources::new(
            vulkan_ctx.device().clone(),
            allocator.clone(),
        );

        let compute_pipelines = PipelineBuilder::create_compute_pipelines(
            &vulkan_ctx,
            &shader_modules,
            &pool,
            &resources,
            contree_builder_resources,
            scene_accel_resources,
            plain_builder_resources,
            &environment_probes,
            ddgi_volume.as_ref(),
        );
        let render_passes = PipelineBuilder::create_render_passes(
            &vulkan_ctx,
            resources.extent_dependent_resources.gfx_output_tex.clone(),
            resources.extent_dependent_resources.gfx_depth_tex.clone(),
            resources.shadow.shadow_map_depth_tex.clone(),
            resources.shadow.leaf_shadow_opacity_tex.clone(),
        );

        let graphics_pipelines = PipelineBuilder::create_graphics_pipelines(
            &vulkan_ctx,
            &shader_modules,
            &render_passes,
            &pool,
            &resources,
            plain_builder_resources,
            &environment_probes,
        );

        let framebuffer_color_and_depth = Self::create_framebuffer_color_and_depth(
            &vulkan_ctx,
            &render_passes.render_pass_color_and_depth,
            &resources.extent_dependent_resources.gfx_output_tex,
            &resources.extent_dependent_resources.gfx_depth_tex,
        );
        let framebuffer_depth_only = Self::create_framebuffer_depth(
            &vulkan_ctx,
            &render_passes.render_pass_depth,
            &resources.shadow.shadow_map_depth_tex,
        );
        let framebuffer_leaf_shadow_opacity = Self::create_framebuffer_color(
            &vulkan_ctx,
            &render_passes.render_pass_leaf_shadow_opacity,
            &resources.shadow.leaf_shadow_opacity_tex,
        );

        let render_target_color_and_depth = RenderTarget::new(
            render_passes.render_pass_color_and_depth,
            vec![framebuffer_color_and_depth],
        );
        let render_target_depth_only = RenderTarget::new(
            render_passes.render_pass_depth,
            vec![framebuffer_depth_only],
        );
        let render_target_leaf_shadow_opacity = RenderTarget::new(
            render_passes.render_pass_leaf_shadow_opacity,
            vec![framebuffer_leaf_shadow_opacity],
        );
        let gui_render_pass = RenderPass::with_attachments(
            vulkan_ctx.device().clone(),
            &[AttachmentDescOuter {
                texture: resources
                    .extent_dependent_resources
                    .screenshot_output_tex
                    .clone(),
                load_op: vk::AttachmentLoadOp::LOAD,
                store_op: vk::AttachmentStoreOp::STORE,
                initial_layout: TextureLayout::GENERAL,
                final_layout: TextureLayout::GENERAL,
                ty: AttachmentType::Color,
            }],
        );
        let framebuffer_gui = Self::create_framebuffer_color(
            &vulkan_ctx,
            &gui_render_pass,
            &resources.extent_dependent_resources.screenshot_output_tex,
        );
        let render_target_gui = RenderTarget::new(gui_render_pass, vec![framebuffer_gui]);

        let particle_capacity = PARTICLE_CAPACITY;
        log::info!(
            "[ENV_LIGHTING] backend={} ready=false state=initializing",
            desc.environment_lighting_backend.label(),
        );

        Ok(Self {
            vulkan_ctx,
            desc,
            chunk_bound,
            allocator,
            resources,
            particle_resources,
            sprinkler_resources,
            irrigation_pipe_resources,
            geometry_preview_resources,
            dynamic_fruit_resources,
            environment_probe_visualization_resources,
            camera,
            camera_view_mat_prev_frame: Mat4::IDENTITY,
            camera_proj_mat_prev_frame: Mat4::IDENTITY,
            current_view_proj_mat: Mat4::IDENTITY,
            shadow_camera_initialized: false,
            shadow_map_history_valid: false,
            leaf_shadow_history_valid: false,
            cloud_history_valid: false,
            cloud_shadow_history_valid: false,
            environment_lighting: EnvironmentLightingCache::default(),
            environment_lighting_backend: desc.environment_lighting_backend,
            environment_probes,
            ddgi_volume,
            ddgi_initial_terrain_ready_revision: None,
            ddgi_trace_stats_readback_pending: None,
            environment_probe_environment_revision: 0,
            environment_probe_seeded_revision: 0,
            environment_probe_classification_pending: true,
            environment_probe_placement_initialized: false,
            environment_probe_trace_cursor: 0,
            environment_probe_local_field_ready: false,
            environment_probe_priority_regions: [None, None],
            environment_probe_terrain_revision: 0,
            environment_probe_converged_terrain_revision: 0,
            environment_probe_refresh_started_at: None,
            environment_probe_stats_readback_pending: false,
            environment_probe_visualization: EnvironmentProbeVisualizationSettings {
                enabled: desc.environment_probe_visualization_enabled,
                ..Default::default()
            },
            compute_pipelines,
            graphics_pipelines,
            render_target_color_and_depth,
            render_target_depth_only,
            render_target_leaf_shadow_opacity,
            render_target_gui,
            pool,
            world_tick_seconds: crate::game_time::WORLD_TICK_SECONDS_DEFAULT,
            last_wind_volume_step: None,
            initialized_wind_volume_bucket_count: 0,
            wind_source_buffer_capacity: 1,
            spatial_sound_manager,
            particle_instance_scratch: Vec::with_capacity(particle_capacity),
            translucent_particle_instance_scratch: Vec::with_capacity(particle_capacity),
        })
    }

    pub fn environment_probe_status(&self) -> EnvironmentProbeVolumeStatus {
        self.environment_probes.status()
    }

    pub fn rebuild_environment_probes(&mut self, spacing_voxels: u32) -> Result<()> {
        let replacement = EnvironmentProbeVolume::new(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            self.chunk_bound.dimensions() * self.desc.voxel_dim_per_chunk,
            self.desc.voxel_dim_per_chunk,
            spacing_voxels,
        )?;
        self.vulkan_ctx.device().wait_idle();
        self.environment_probes = replacement;
        if self.environment_lighting_backend == EnvironmentLightingBackend::Ddgi {
            let mut replacement = DdgiVolume::new(
                &self.vulkan_ctx,
                self.allocator.clone(),
                self.chunk_bound.dimensions() * self.desc.voxel_dim_per_chunk,
                spacing_voxels,
            )?;
            if let Some(terrain_revision) = self.ddgi_initial_terrain_ready_revision {
                replacement.request_initialization(terrain_revision);
            }
            self.ddgi_volume = Some(replacement);
            self.update_ddgi_descriptors()?;
        }
        self.environment_probe_seeded_revision = 0;
        self.environment_probe_classification_pending = true;
        self.environment_probe_placement_initialized = false;
        self.environment_probe_trace_cursor = 0;
        self.environment_probe_local_field_ready = false;
        self.environment_probe_priority_regions = [None, None];
        self.environment_probe_terrain_revision = 0;
        self.environment_probe_converged_terrain_revision = 0;
        self.environment_probe_refresh_started_at = None;
        self.environment_probe_stats_readback_pending = false;
        self.ddgi_trace_stats_readback_pending = None;
        self.update_environment_probe_consumer_descriptors();
        let resources: [&dyn ResourceContainer; 2] = [&self.resources, &self.environment_probes];
        self.graphics_pipelines
            .environment_probe_visualization_depth_ppl
            .auto_update_descriptor_sets(&resources)?;
        self.graphics_pipelines
            .environment_probe_visualization_overlay_ppl
            .auto_update_descriptor_sets(&resources)?;
        Ok(())
    }

    pub fn request_environment_probe_refresh_near_voxel_bound(&mut self, edit_bound: UAabb3) {
        self.environment_probe_terrain_revision = self
            .environment_probe_terrain_revision
            .wrapping_add(1)
            .max(1);
        self.environment_probe_classification_pending = true;
        self.environment_probe_refresh_started_at = Some(Instant::now());

        let grid = self.environment_probes.status().grid;
        let edit_center = edit_bound.center();
        let camera_voxel_position =
            self.camera.position() * self.desc.voxel_dim_per_chunk.as_vec3();
        let edit_region = Self::environment_probe_priority_region(grid, edit_center);
        let camera_region = Self::environment_probe_priority_region(grid, camera_voxel_position);
        self.environment_probe_priority_regions = [
            Some(edit_region),
            (camera_region != edit_region).then_some(camera_region),
        ];

        log::info!(
            "[ENV_PROBES] terrain refresh requested revision={} edit_voxel_bound={:?}..{:?} edit_priority_min={:?} camera_priority_min={:?} region_side={}",
            self.environment_probe_terrain_revision,
            edit_bound.min(),
            edit_bound.max(),
            edit_region.grid_min,
            camera_region.grid_min,
            edit_region.side,
        );
    }

    /// Starts the static DDGI build after the caller has finished all initial terrain work.
    /// Runtime terrain edits intentionally do not call this M2 seam yet.
    pub fn notify_ddgi_initial_terrain_ready(&mut self, terrain_revision: u32) {
        if self.environment_lighting_backend != EnvironmentLightingBackend::Ddgi {
            return;
        }
        self.ddgi_initial_terrain_ready_revision = Some(terrain_revision);
        let volume = self
            .ddgi_volume
            .as_mut()
            .expect("DDGI backend requires an allocated volume");
        if volume.request_initialization(terrain_revision) {
            let status = volume.status();
            log::info!(
                "[DDGI] initialization requested terrain_revision={} spacing_voxels={} probes={} stage={:?}",
                terrain_revision,
                status.grid.spacing_voxels(),
                status.grid.probe_count(),
                status.stage,
            );
        }
    }

    fn environment_probe_priority_region(
        grid: crate::environment_probes::EnvironmentProbeGrid,
        voxel_position: Vec3,
    ) -> EnvironmentProbeTraceRegion {
        let maximum_coordinate = grid.dimensions() - UVec3::ONE;
        let center = (voxel_position / grid.spacing_voxels() as f32)
            .round()
            .max(Vec3::ZERO)
            .as_uvec3()
            .min(maximum_coordinate);
        let grid_min =
            center.saturating_sub(UVec3::splat(ENVIRONMENT_PROBE_PRIORITY_REGION_RADIUS));
        EnvironmentProbeTraceRegion {
            grid_min,
            side: ENVIRONMENT_PROBE_PRIORITY_REGION_RADIUS * 2 + 1,
        }
    }

    pub fn environment_lighting_backend(&self) -> EnvironmentLightingBackend {
        self.environment_lighting_backend
    }

    pub fn ddgi_debug_view(&self) -> DdgiDebugView {
        self.desc.ddgi_debug_view
    }

    pub fn environment_lighting_backend_ready(&self) -> bool {
        match self.environment_lighting_backend {
            EnvironmentLightingBackend::LocalSh => self.environment_probe_local_field_ready,
            EnvironmentLightingBackend::Ddgi => self
                .ddgi_volume
                .as_ref()
                .is_some_and(|volume| volume.status().is_ready()),
        }
    }

    pub fn environment_lighting_backend_ready_for_terrain_revision(&self, revision: u32) -> bool {
        match self.environment_lighting_backend {
            EnvironmentLightingBackend::LocalSh => {
                self.environment_probe_terrain_revision_ready(revision)
            }
            EnvironmentLightingBackend::Ddgi => self.ddgi_volume.as_ref().is_some_and(|volume| {
                let status = volume.status();
                status.is_ready() && status.relocated_terrain_revision == Some(revision)
            }),
        }
    }

    pub fn environment_irradiance_capture_extent(&self) -> Extent2D {
        self.resources
            .extent_dependent_resources
            .compute_output_tex
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .expect("terrain output must be two-dimensional")
    }

    pub fn record_environment_irradiance_capture_readback(
        &self,
        cmdbuf: &CommandBuffer,
        readback: &Buffer,
    ) {
        let source = &self
            .resources
            .extent_dependent_resources
            .environment_irradiance_capture;
        assert_eq!(source.get_size_bytes(), readback.get_size_bytes());
        PipelineBarrier::new(
            PipelineStage::COMPUTE_SHADER,
            PipelineStage::TRANSFER,
            [MemoryBarrier::new(
                MemoryAccess::SHADER_WRITE,
                MemoryAccess::TRANSFER_READ,
            )],
        )
        .record_insert(self.vulkan_ctx.device(), cmdbuf);
        source.record_copy_to_buffer(cmdbuf, readback, source.get_size_bytes(), 0, 0);
        PipelineBarrier::new(
            PipelineStage::TRANSFER,
            PipelineStage::HOST,
            [MemoryBarrier::new(
                MemoryAccess::TRANSFER_WRITE,
                MemoryAccess::HOST_READ,
            )],
        )
        .record_insert(self.vulkan_ctx.device(), cmdbuf);
    }

    pub fn environment_probe_terrain_revision(&self) -> u32 {
        self.environment_probe_terrain_revision
    }

    pub fn environment_probe_terrain_revision_ready(&self, revision: u32) -> bool {
        revision != 0 && self.environment_probe_converged_terrain_revision == revision
    }

    pub fn environment_probe_visualization_settings(
        &self,
    ) -> EnvironmentProbeVisualizationSettings {
        self.environment_probe_visualization
    }

    pub fn set_environment_probe_visualization_settings(
        &mut self,
        settings: EnvironmentProbeVisualizationSettings,
    ) {
        self.environment_probe_visualization = settings.sanitized();
    }

    /// A framebuffer that contains the color and depth textures for the main render pass
    fn create_framebuffer_color_and_depth(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        target_texture: &Texture,
        depth_texture: &Texture,
    ) -> Framebuffer {
        let target_image_extent = target_texture
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .unwrap();

        Framebuffer::from_textures(
            vulkan_ctx.clone(),
            render_pass,
            &[target_texture, depth_texture],
            target_image_extent,
        )
        .unwrap()
    }

    /// A framebuffer that contains a color texture.
    fn create_framebuffer_color(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        color_tex: &Texture,
    ) -> Framebuffer {
        let color_extent = color_tex
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .unwrap();
        Framebuffer::from_textures(vulkan_ctx.clone(), render_pass, &[color_tex], color_extent)
            .unwrap()
    }

    /// A framebuffer that contains the shadow map texture
    fn create_framebuffer_depth(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        shadow_map_tex: &Texture,
    ) -> Framebuffer {
        let shadow_image_extent = shadow_map_tex
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .unwrap();
        Framebuffer::from_textures(
            vulkan_ctx.clone(),
            render_pass,
            &[shadow_map_tex],
            shadow_image_extent,
        )
        .unwrap()
    }

    pub fn on_resize(
        &mut self,
        screen_extent: Extent2D,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
        plain_builder_resources: &PlainBuilderResources,
    ) {
        let render_extent = Self::get_render_extent(screen_extent, self.desc.scaling_factor);

        self.camera.on_resize(render_extent);

        // this must be done first
        self.resources.on_resize(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            render_extent,
            screen_extent,
        );

        let framebuffer_color_and_depth = Self::create_framebuffer_color_and_depth(
            &self.vulkan_ctx,
            self.render_target_color_and_depth.get_render_pass(),
            &self.resources.extent_dependent_resources.gfx_output_tex,
            &self.resources.extent_dependent_resources.gfx_depth_tex,
        );
        let framebuffer_depth_only = Self::create_framebuffer_depth(
            &self.vulkan_ctx,
            self.render_target_depth_only.get_render_pass(),
            &self.resources.shadow.shadow_map_depth_tex,
        );
        let framebuffer_leaf_shadow_opacity = Self::create_framebuffer_color(
            &self.vulkan_ctx,
            self.render_target_leaf_shadow_opacity.get_render_pass(),
            &self.resources.shadow.leaf_shadow_opacity_tex,
        );
        let framebuffer_gui = Self::create_framebuffer_color(
            &self.vulkan_ctx,
            self.render_target_gui.get_render_pass(),
            &self
                .resources
                .extent_dependent_resources
                .screenshot_output_tex,
        );

        self.render_target_color_and_depth = RenderTarget::new(
            self.render_target_color_and_depth.get_render_pass().clone(),
            vec![framebuffer_color_and_depth],
        );
        self.render_target_depth_only = RenderTarget::new(
            self.render_target_depth_only.get_render_pass().clone(),
            vec![framebuffer_depth_only],
        );
        self.render_target_leaf_shadow_opacity = RenderTarget::new(
            self.render_target_leaf_shadow_opacity
                .get_render_pass()
                .clone(),
            vec![framebuffer_leaf_shadow_opacity],
        );
        self.render_target_gui = RenderTarget::new(
            self.render_target_gui.get_render_pass().clone(),
            vec![framebuffer_gui],
        );

        self.cloud_history_valid = false;
        self.cloud_shadow_history_valid = false;
        self.update_sets(
            contree_builder_resources,
            scene_accel_resources,
            plain_builder_resources,
        );
    }

    fn update_sets(
        &mut self,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
        plain_builder_resources: &PlainBuilderResources,
    ) {
        let update_compute_fn = |ppl: &ComputePipeline, resources: &[&dyn ResourceContainer]| {
            ppl.auto_update_descriptor_sets(resources).unwrap()
        };

        let update_graphics_fn = |ppl: &GraphicsPipeline, resources: &[&dyn ResourceContainer]| {
            ppl.auto_update_descriptor_sets(resources).unwrap()
        };

        let all_resources = self.all_descriptor_resources(
            contree_builder_resources,
            scene_accel_resources,
            plain_builder_resources,
        );
        update_compute_fn(&self.compute_pipelines.tracer_ppl, &all_resources);
        update_compute_fn(
            &self.compute_pipelines.environment_probe_classify_ppl,
            &all_resources,
        );
        update_compute_fn(
            &self.compute_pipelines.environment_probe_update_ppl,
            &all_resources,
        );
        update_compute_fn(&self.compute_pipelines.tracer_shadow_ppl, &all_resources);
        update_compute_fn(&self.compute_pipelines.player_collider_ppl, &all_resources);
        update_compute_fn(&self.compute_pipelines.terrain_query_ppl, &all_resources);

        let tracer_resources = self.tracer_descriptor_resources();
        update_compute_fn(&self.compute_pipelines.wind_volume_ppl, &tracer_resources);
        update_compute_fn(
            &self.compute_pipelines.shadow_depth_copy_ppl,
            &tracer_resources,
        );
        update_compute_fn(
            &self.compute_pipelines.leaf_shadow_mask_ppl,
            &tracer_resources,
        );
        update_compute_fn(&self.compute_pipelines.vsm_creation_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.vsm_blur_h_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.vsm_blur_v_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.god_ray_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.cloud_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.cloud_shadow_ppl, &tracer_resources);
        update_compute_fn(
            &self.compute_pipelines.cloud_shadow_temporal_ppl,
            &tracer_resources,
        );
        update_compute_fn(
            &self.compute_pipelines.cloud_temporal_ppl,
            &tracer_resources,
        );
        update_compute_fn(&self.compute_pipelines.lens_flare_ppl, &tracer_resources);
        update_compute_fn(
            &self.compute_pipelines.lens_flare_sun_visible_ppl,
            &tracer_resources,
        );
        update_compute_fn(
            &self.compute_pipelines.lens_flare_downsample_ppl,
            &tracer_resources,
        );
        update_compute_fn(&self.compute_pipelines.composition_ppl, &tracer_resources);
        update_compute_fn(
            &self.compute_pipelines.post_processing_ppl,
            &tracer_resources,
        );

        // update graphics pipelines descriptor sets
        update_graphics_fn(
            &self.graphics_pipelines.terrain_depth_prefill_ppl,
            &tracer_resources,
        );
        update_graphics_fn(&self.graphics_pipelines.flora_ppl, &all_resources);
        update_graphics_fn(&self.graphics_pipelines.flora_lod_ppl, &all_resources);
        update_graphics_fn(&self.graphics_pipelines.leaves_ppl, &tracer_resources);
        update_graphics_fn(&self.graphics_pipelines.leaves_lod_ppl, &tracer_resources);
        update_graphics_fn(
            &self.graphics_pipelines.leaves_shadow_lod_ppl,
            &tracer_resources,
        );
        update_graphics_fn(&self.graphics_pipelines.sprinkler_ppl, &tracer_resources);
        update_graphics_fn(
            &self.graphics_pipelines.geometry_preview_ppl,
            &tracer_resources,
        );
        let environment_probe_resources: [&dyn ResourceContainer; 2] =
            [&self.resources, &self.environment_probes];
        update_compute_fn(
            &self.compute_pipelines.environment_probe_global_copy_ppl,
            &environment_probe_resources,
        );
        update_graphics_fn(
            &self
                .graphics_pipelines
                .environment_probe_visualization_depth_ppl,
            &environment_probe_resources,
        );
        update_graphics_fn(
            &self
                .graphics_pipelines
                .environment_probe_visualization_overlay_ppl,
            &environment_probe_resources,
        );
        update_graphics_fn(&self.graphics_pipelines.particle_ppl, &tracer_resources);
        update_graphics_fn(
            &self.graphics_pipelines.water_droplet_ppl,
            &tracer_resources,
        );
        if let Some(ddgi_volume) = self.ddgi_volume.as_ref() {
            let ddgi_resources: [&dyn ResourceContainer; 2] = [&self.resources, ddgi_volume];
            if let Some(pipeline) = self.compute_pipelines.ddgi_global_sky_filter_ppl.as_ref() {
                update_compute_fn(pipeline, &ddgi_resources);
            }
            if let Some(pipeline) = self.compute_pipelines.ddgi_octahedral_gutter_ppl.as_ref() {
                update_compute_fn(pipeline, &[ddgi_volume]);
            }
            if let Some(pipeline) = self.compute_pipelines.ddgi_probe_relocate_ppl.as_ref() {
                update_compute_fn(pipeline, &[plain_builder_resources, ddgi_volume]);
            }
            if let Some(pipeline) = self.compute_pipelines.ddgi_probe_trace_ppl.as_ref() {
                update_compute_fn(
                    pipeline,
                    &[
                        &self.resources,
                        contree_builder_resources,
                        scene_accel_resources,
                        ddgi_volume,
                    ],
                );
            }
            for pipeline in [
                self.compute_pipelines.ddgi_irradiance_filter_ppl.as_ref(),
                self.compute_pipelines.ddgi_visibility_filter_ppl.as_ref(),
                self.compute_pipelines.ddgi_irradiance_gutter_ppl.as_ref(),
                self.compute_pipelines.ddgi_visibility_gutter_ppl.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                update_compute_fn(pipeline, &[ddgi_volume]);
            }
        }
    }

    fn tracer_descriptor_resources(&self) -> [&dyn ResourceContainer; 2] {
        [
            &self.resources as &dyn ResourceContainer,
            &self.environment_probes as &dyn ResourceContainer,
        ]
    }

    fn all_descriptor_resources<'a>(
        &'a self,
        contree_builder_resources: &'a ContreeBuilderResources,
        scene_accel_resources: &'a SceneAccelBuilderResources,
        plain_builder_resources: &'a PlainBuilderResources,
    ) -> [&'a dyn ResourceContainer; 6] {
        [
            &self.resources as &dyn ResourceContainer,
            contree_builder_resources as &dyn ResourceContainer,
            scene_accel_resources as &dyn ResourceContainer,
            plain_builder_resources as &dyn ResourceContainer,
            &self.environment_probes as &dyn ResourceContainer,
            self.ddgi_volume
                .as_ref()
                .expect("tracer descriptors require DDGI resources")
                as &dyn ResourceContainer,
        ]
    }

    fn update_environment_probe_consumer_descriptors(&self) {
        let coefficients = &self.environment_probes.environment_probe_coefficients;
        let summaries = &self.environment_probes.environment_probe_summaries;
        let visibility = &self.environment_probes.environment_probe_visibility;
        let directions = &self.environment_probes.environment_probe_directions;
        let stats = &self.environment_probes.environment_probe_stats;
        for pipeline in [
            &self.compute_pipelines.environment_probe_global_copy_ppl,
            &self.compute_pipelines.tracer_ppl,
        ] {
            pipeline.write_descriptor_set(
                0,
                WriteDescriptorSet::new_buffer_write(
                    ENVIRONMENT_PROBE_COEFFICIENT_BINDING,
                    coefficients,
                ),
            );
            pipeline.write_descriptor_set(
                0,
                WriteDescriptorSet::new_buffer_write(ENVIRONMENT_PROBE_SUMMARY_BINDING, summaries),
            );
        }
        self.compute_pipelines.tracer_ppl.write_descriptor_set(
            0,
            WriteDescriptorSet::new_buffer_write(ENVIRONMENT_PROBE_VISIBILITY_BINDING, visibility),
        );
        self.compute_pipelines
            .environment_probe_classify_ppl
            .write_descriptor_set(
                0,
                WriteDescriptorSet::new_buffer_write(ENVIRONMENT_PROBE_SUMMARY_BINDING, summaries),
            );
        self.compute_pipelines
            .environment_probe_stats_ppl
            .write_descriptor_set(
                0,
                WriteDescriptorSet::new_buffer_write(ENVIRONMENT_PROBE_SUMMARY_BINDING, summaries),
            );
        self.compute_pipelines
            .environment_probe_stats_ppl
            .write_descriptor_set(
                0,
                WriteDescriptorSet::new_buffer_write(ENVIRONMENT_PROBE_STATS_BINDING, stats),
            );
        for (binding, buffer) in [
            (ENVIRONMENT_PROBE_COEFFICIENT_BINDING, coefficients),
            (ENVIRONMENT_PROBE_SUMMARY_BINDING, summaries),
            (ENVIRONMENT_PROBE_VISIBILITY_BINDING, visibility),
            (ENVIRONMENT_PROBE_DIRECTION_BINDING, directions),
        ] {
            self.compute_pipelines
                .environment_probe_update_ppl
                .write_descriptor_set(0, WriteDescriptorSet::new_buffer_write(binding, buffer));
        }
        for pipeline in [
            &self.graphics_pipelines.flora_ppl,
            &self.graphics_pipelines.flora_lod_ppl,
            &self.graphics_pipelines.leaves_ppl,
            &self.graphics_pipelines.leaves_lod_ppl,
            &self.graphics_pipelines.sprinkler_ppl,
            &self.graphics_pipelines.dynamic_fruit_ppl,
            &self.graphics_pipelines.particle_ppl,
            &self.graphics_pipelines.water_droplet_ppl,
        ] {
            pipeline.write_descriptor_set(
                0,
                WriteDescriptorSet::new_buffer_write(
                    ENVIRONMENT_PROBE_COEFFICIENT_BINDING,
                    coefficients,
                ),
            );
            pipeline.write_descriptor_set(
                0,
                WriteDescriptorSet::new_buffer_write(ENVIRONMENT_PROBE_SUMMARY_BINDING, summaries),
            );
            pipeline.write_descriptor_set(
                0,
                WriteDescriptorSet::new_buffer_write(
                    ENVIRONMENT_PROBE_VISIBILITY_BINDING,
                    visibility,
                ),
            );
        }
    }

    fn update_ddgi_descriptors(&self) -> Result<()> {
        let Some(ddgi_volume) = self.ddgi_volume.as_ref() else {
            return Ok(());
        };
        let resources: [&dyn ResourceContainer; 2] = [&self.resources, ddgi_volume];
        if let Some(pipeline) = self.compute_pipelines.ddgi_global_sky_filter_ppl.as_ref() {
            pipeline.auto_update_descriptor_sets(&resources)?;
        }
        if let Some(pipeline) = self.compute_pipelines.ddgi_octahedral_gutter_ppl.as_ref() {
            pipeline.auto_update_descriptor_sets(&[ddgi_volume])?;
        }
        if let Some(pipeline) = self.compute_pipelines.ddgi_probe_relocate_ppl.as_ref() {
            pipeline.auto_update_descriptor_sets(&[ddgi_volume])?;
        }
        if let Some(pipeline) = self.compute_pipelines.ddgi_probe_trace_ppl.as_ref() {
            pipeline.auto_update_descriptor_sets(&[ddgi_volume])?;
        }
        for pipeline in [
            self.compute_pipelines.ddgi_irradiance_filter_ppl.as_ref(),
            self.compute_pipelines.ddgi_visibility_filter_ppl.as_ref(),
            self.compute_pipelines.ddgi_irradiance_gutter_ppl.as_ref(),
            self.compute_pipelines.ddgi_visibility_gutter_ppl.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            pipeline.auto_update_descriptor_sets(&[ddgi_volume])?;
        }
        self.compute_pipelines.tracer_ppl.write_descriptor_set(
            0,
            WriteDescriptorSet::new_buffer_write(
                DDGI_PROBE_METADATA_BINDING,
                &ddgi_volume.ddgi_probe_metadata,
            ),
        );
        for (binding, texture) in [
            (
                DDGI_GLOBAL_SKY_BINDING,
                &ddgi_volume.ddgi_global_sky_irradiance,
            ),
            (
                DDGI_IRRADIANCE_ATLAS_BINDING,
                &ddgi_volume.ddgi_irradiance_atlas,
            ),
            (
                DDGI_VISIBILITY_ATLAS_BINDING,
                &ddgi_volume.ddgi_visibility_atlas,
            ),
        ] {
            self.compute_pipelines.tracer_ppl.write_descriptor_set(
                0,
                WriteDescriptorSet::new_texture_write(
                    binding,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    texture,
                    TextureLayout::SHADER_READ_ONLY,
                ),
            );
        }
        Ok(())
    }

    // create a lower resolution texture for rendering, for better performance,
    // less memory usage, and stylized rendering
    fn get_render_extent(screen_extent: Extent2D, scaling_factor: f32) -> Extent2D {
        Extent2D::new(
            (screen_extent.width as f32 * scaling_factor) as u32,
            (screen_extent.height as f32 * scaling_factor) as u32,
        )
    }

    pub fn get_screen_output_tex(&self) -> &Texture {
        &self.resources.extent_dependent_resources.screen_output_tex
    }

    fn ensure_wind_source_buffer_capacity(&mut self, source_count: usize) -> Result<()> {
        let required_capacity = source_count.max(1);
        if required_capacity <= self.wind_source_buffer_capacity {
            return Ok(());
        }

        let new_capacity = required_capacity.next_power_of_two();
        *self.resources.wind.wind_sources = TracerResources::create_wind_sources_buffer(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            new_capacity,
        );
        self.wind_source_buffer_capacity = new_capacity;
        let tracer_resources = self.tracer_descriptor_resources();
        self.compute_pipelines
            .wind_volume_ppl
            .auto_update_descriptor_sets(&tracer_resources)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_buffers(
        &mut self,
        time_info: &TimeInfo,
        flora_growth_override_enabled: bool,
        flora_growth_override: f32,
        terrain_self_shadow_tolerance_voxels: f32,
        flora_instance_hsv_offset_max: Vec3,
        flora_voxel_hsv_offset_max: Vec3,
        grass_bottom_dark: Vec3,
        grass_bottom_light: Vec3,
        grass_tip_dark: Vec3,
        grass_tip_light: Vec3,
        world_tick_seconds: f32,
        update_shadow_map: bool,
        lens_flare_intensity: f32,
        lens_flare_sun_pixel_scale: f32,
        glass_gui_params: GlassGuiParams,
        wind_directional_bias_fraction: f32,
        wind_turbulence_fraction: f32,
        grass_vibration_amplitude_voxels: f32,
        grass_vibration_primary_speed: f32,
        grass_vibration_secondary_speed: f32,
        grass_natural_bend_min_voxels: f32,
        grass_natural_bend_max_voxels: f32,
        flora_bend_height_power: f32,
        kochia_motion: KochiaMotionParams,
        kochia_visual: KochiaVisualParams,
        leaf_paddle_amplitude_voxels: f32,
        leaf_paddle_primary_speed: f32,
        leaf_paddle_secondary_speed: f32,
        leaf_paddle_amplitude_wind_start_strength: f32,
        leaf_paddle_amplitude_wind_full_strength: f32,
        leaf_paddle_amplitude_wind_knee_bias: f32,
        leaf_paddle_frequency_wind_start_strength: f32,
        leaf_paddle_frequency_wind_full_strength: f32,
        leaf_paddle_frequency_wind_knee_bias: f32,
        leaf_paddle_frequency_min_multiplier: f32,
        leaf_paddle_frequency_max_multiplier: f32,
        fruit_motion: FruitMotionParams,
        leaf_shadow_fragment_opacity: f32,
        leaf_shadow_strength: f32,
        leaf_shadow_min_transmittance: f32,
        leaf_shadow_filter_radius_texels: f32,
        leaf_transmission_strength: f32,
        wind_gui_params: WindGuiParams,
        cloud_gui_params: CloudGuiParams,
        flora_tick: u32,
        sprout_delay_ticks: u32,
        full_growth_ticks: u32,
        spawn_time_ms: u32,
        spawn_duration_seconds: f32,
        spawn_rise_fraction: f32,
        spawn_overshoot_min_voxels: f32,
        spawn_overshoot_max_voxels: f32,
        spawn_stagger_seconds: f32,
        sun_dir: Vec3,
        sun_size: f32,
        sun_color: Vec3,
        sun_luminance: f32,
        sun_display_luminance: f32,
        sun_altitude: f32,
        sun_azimuth: f32,
        god_ray_max_depth: f32,
        god_ray_max_checks: u32,
        god_ray_weight: f32,
        god_ray_color: Vec3,
        starlight_iterations: i32,
        starlight_formuparam: f32,
        starlight_volsteps: i32,
        starlight_stepsize: f32,
        starlight_zoom: f32,
        starlight_tile: f32,
        starlight_speed: f32,
        starlight_brightness: f32,
        starlight_darkmatter: f32,
        starlight_distfading: f32,
        starlight_saturation: f32,
        voxel_dirt_color: Vec3,
        voxel_sand_color: Vec3,
        voxel_cherry_wood_color: Vec3,
        voxel_oak_wood_color: Vec3,
        voxel_rock_color: Vec3,
        voxel_color_variance: f32,
        terrain_edit_preview_center: Option<Vec3>,
        terrain_edit_preview_radius: f32,
        terrain_edit_preview_shape: TerrainEditPreviewShape,
        terrain_edit_preview_color: Vec3,
        terrain_edit_preview_alpha: f32,
    ) -> Result<()> {
        let view_mat = self.camera.get_view_mat();
        let proj_mat = self.camera.get_proj_mat();
        self.current_view_proj_mat = proj_mat * view_mat;
        BufferUpdater::update_camera_info(
            &mut self.resources.uniforms.camera_info,
            view_mat,
            proj_mat,
        )?;

        // Shadow camera info. Shadow maps are rendered every frame while shadows
        // are enabled, so PCSS and VSM both use the latest light-space matrix.
        if update_shadow_map || !self.shadow_camera_initialized {
            let world_bound = self.chunk_bound.into();
            let shadow_map_extent = self
                .resources
                .shadow
                .shadow_map_tex
                .get_image()
                .get_desc()
                .extent;
            let shadow_map_resolution = shadow_map_extent.width.min(shadow_map_extent.height);
            let (shadow_view_mat, shadow_proj_mat) =
                calculate_directional_light_matrices(world_bound, sun_dir, shadow_map_resolution);
            self.shadow_camera_initialized = true;
            BufferUpdater::update_camera_info(
                &mut self.resources.shadow.shadow_camera_info,
                shadow_view_mat,
                shadow_proj_mat,
            )?;
        }

        // camera info prev frame
        BufferUpdater::update_camera_info(
            &mut self.resources.uniforms.camera_info_prev_frame,
            self.camera_view_mat_prev_frame,
            self.camera_proj_mat_prev_frame,
        )?;

        BufferUpdater::update_god_ray_info(
            &self.resources,
            god_ray_max_depth,
            god_ray_max_checks,
            god_ray_weight,
            god_ray_color,
        )?;

        BufferUpdater::update_post_processing_info(&self.resources, self.desc.scaling_factor)?;

        BufferUpdater::update_voxel_colors(
            &self.resources,
            voxel_dirt_color,
            voxel_sand_color,
            voxel_cherry_wood_color,
            voxel_oak_wood_color,
            voxel_rock_color,
            voxel_color_variance,
        )?;
        BufferUpdater::update_terrain_edit_preview(
            &self.resources,
            terrain_edit_preview_center,
            terrain_edit_preview_radius,
            terrain_edit_preview_shape,
            terrain_edit_preview_color,
            terrain_edit_preview_alpha,
        )?;

        self.world_tick_seconds = crate::game_time::clamp_world_tick_seconds(world_tick_seconds);

        self.ensure_wind_source_buffer_capacity(wind_gui_params.sources.len())?;
        BufferUpdater::update_gui_input(
            &self.resources,
            flora_growth_override_enabled,
            flora_growth_override,
            terrain_self_shadow_tolerance_voxels,
            flora_instance_hsv_offset_max,
            flora_voxel_hsv_offset_max,
            grass_bottom_dark,
            grass_bottom_light,
            grass_tip_dark,
            grass_tip_light,
            lens_flare_intensity,
            lens_flare_sun_pixel_scale,
            glass_gui_params,
            wind_directional_bias_fraction,
            wind_turbulence_fraction,
            self.world_tick_seconds,
            grass_vibration_amplitude_voxels,
            grass_vibration_primary_speed,
            grass_vibration_secondary_speed,
            grass_natural_bend_min_voxels,
            grass_natural_bend_max_voxels,
            flora_bend_height_power,
            kochia_motion,
            kochia_visual,
            leaf_paddle_amplitude_voxels,
            leaf_paddle_primary_speed,
            leaf_paddle_secondary_speed,
            leaf_paddle_amplitude_wind_start_strength,
            leaf_paddle_amplitude_wind_full_strength,
            leaf_paddle_amplitude_wind_knee_bias,
            leaf_paddle_frequency_wind_start_strength,
            leaf_paddle_frequency_wind_full_strength,
            leaf_paddle_frequency_wind_knee_bias,
            leaf_paddle_frequency_min_multiplier,
            leaf_paddle_frequency_max_multiplier,
            fruit_motion,
            leaf_shadow_fragment_opacity,
            leaf_shadow_strength,
            leaf_shadow_min_transmittance,
            leaf_shadow_filter_radius_texels,
            leaf_transmission_strength,
            wind_gui_params,
            cloud_gui_params,
        )?;

        BufferUpdater::update_flora_growth_info(
            &self.resources,
            flora_tick,
            sprout_delay_ticks,
            full_growth_ticks,
            spawn_time_ms,
            spawn_duration_seconds,
            spawn_rise_fraction,
            spawn_overshoot_min_voxels,
            spawn_overshoot_max_voxels,
            spawn_stagger_seconds,
        )?;

        BufferUpdater::update_sun_info(
            &self.resources,
            sun_dir,
            sun_size,
            sun_color,
            sun_luminance,
            sun_display_luminance,
            sun_altitude,
            sun_azimuth,
        )?;

        let environment_lighting = self.environment_lighting.update(sun_dir);
        let ddgi_status = self
            .ddgi_volume
            .as_ref()
            .expect("shading uniforms require DDGI resources")
            .status();
        BufferUpdater::update_shading_info(
            &self.resources,
            environment_lighting,
            self.environment_probes.status().grid,
            self.desc.voxel_dim_per_chunk,
            self.environment_probe_local_field_ready,
            self.environment_lighting_backend,
            self.environment_lighting_backend_ready(),
            self.desc.environment_irradiance_capture_enabled,
            ddgi_status.irradiance_layout.tile_grid().x,
            ddgi_status.visibility_layout.tile_grid().x,
            self.desc.ddgi_debug_view.as_u32(),
        )?;
        self.environment_probe_environment_revision = environment_lighting.revision;

        BufferUpdater::update_starlight_info(
            &self.resources,
            starlight_iterations,
            starlight_formuparam,
            starlight_volsteps,
            starlight_stepsize,
            starlight_zoom,
            starlight_tile,
            starlight_speed,
            starlight_brightness,
            starlight_darkmatter,
            starlight_distfading,
            starlight_saturation,
        )?;

        BufferUpdater::update_env_info(&self.resources, time_info.total_frame_count() as u32)?;

        self.camera_view_mat_prev_frame = self.camera.get_view_mat();
        self.camera_proj_mat_prev_frame = self.camera.get_proj_mat();

        Ok(())
    }

    /// Returns a list of chunks that need to be drawn this frame.
    fn chunks_needs_to_draw_this_frame<'a>(
        &self,
        surface_resources: &'a SurfaceResources,
        lod_distance: f32,
        flora_draw_distance: f32,
    ) -> HashMap<LodState, Vec<&'a FloraInstanceResources>> {
        let mut lod0_instances = Vec::new();
        let mut lod1_instances = Vec::new();
        let camera_pos = self.camera.position();

        for (aabb, instances) in &surface_resources.instances.chunk_flora_instances {
            // perform frustum culling
            if !aabb.is_inside_frustum(self.current_view_proj_mat) {
                continue;
            }

            // calculate distance from camera to chunk center
            let chunk_center = aabb.center();
            let distance = (camera_pos - chunk_center).length();

            // skip chunks beyond max flora draw distance
            if distance > flora_draw_distance {
                continue;
            }

            if distance <= lod_distance {
                lod0_instances.push(instances);
            } else {
                lod1_instances.push(instances);
            }
        }

        let mut result = HashMap::new();
        result.insert(LodState::Lod0, lod0_instances);
        result.insert(LodState::Lod1, lod1_instances);
        result
    }

    fn trees_needs_to_draw_this_frame<'a>(
        &self,
        tree_instances: &'a HashMap<u32, TreeLeavesInstance>,
        lod_distance: f32,
        flora_draw_distance: f32,
    ) -> HashMap<LodState, Vec<&'a TreeLeavesInstance>> {
        let mut lod0_instances = Vec::new();
        let mut lod1_instances = Vec::new();
        let camera_pos = self.camera.position();

        for tree_instance in tree_instances.values() {
            // perform frustum culling
            if !tree_instance
                .aabb
                .is_inside_frustum(self.current_view_proj_mat)
            {
                continue;
            }

            // calculate distance from camera to tree center
            let tree_center = tree_instance.aabb.center();
            let distance = (camera_pos - tree_center).length();

            if distance > flora_draw_distance {
                continue;
            }

            if distance <= lod_distance {
                lod0_instances.push(tree_instance);
            } else {
                lod1_instances.push(tree_instance);
            }
        }

        let mut result = HashMap::new();
        result.insert(LodState::Lod0, lod0_instances);
        result.insert(LodState::Lod1, lod1_instances);
        result
    }

    fn with_gpu_scope<T>(
        gpu_profiler: Option<&mut GpuProfiler>,
        gpu_profiler_frame_slot: usize,
        cmdbuf: &CommandBuffer,
        name: &'static str,
        work: impl FnOnce() -> T,
    ) -> T {
        let Some(profiler) = gpu_profiler else {
            return work();
        };
        let scope = profiler.begin_scope(
            gpu_profiler_frame_slot,
            cmdbuf,
            name,
            PipelineStage::ALL_COMMANDS,
        );
        let result = work();
        if let Some(scope) = scope {
            profiler.end_scope(
                gpu_profiler_frame_slot,
                cmdbuf,
                scope,
                PipelineStage::ALL_COMMANDS,
            );
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::needless_option_as_deref)]
    pub fn record_shadow_prepass(
        &mut self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
        time: f32,
        leaf_color_tables: FloraHeightColorTables,
        render_flags: &crate::RenderFlags,
        update_shadow_map: bool,
        vsm_blur_radius: u32,
        vsm_temporal_alpha: f32,
        leaf_shadow_temporal_alpha: f32,
        reset_vsm_history: bool,
        mut gpu_profiler: Option<&mut GpuProfiler>,
        gpu_profiler_frame_slot: usize,
    ) -> Result<()> {
        if self.environment_probe_stats_readback_pending {
            self.environment_probe_stats_readback_pending = false;
            let counts = self
                .environment_probes
                .update_state_counts_from_readback()?;
            log::info!(
                "[ENV_PROBES] state counts inactive={} inside_solid={} relocation_pending={} valid={} dirty={} updating={} relocation_failed={} total={}",
                counts.inactive,
                counts.inside_solid,
                counts.relocation_pending,
                counts.valid,
                counts.dirty,
                counts.updating,
                counts.relocation_failed,
                counts.total(),
            );
        }
        if let Some(batch) = self.ddgi_trace_stats_readback_pending.take() {
            let volume = self
                .ddgi_volume
                .as_ref()
                .expect("DDGI trace stats require an allocated volume");
            let stats = volume.update_trace_stats_from_readback()?;
            anyhow::ensure!(
                stats.ray_records == batch.probe_count * crate::ddgi::DDGI_RAYS_PER_PROBE,
                "DDGI trace produced {} records for a {}x{} batch",
                stats.ray_records,
                batch.probe_count,
                crate::ddgi::DDGI_RAYS_PER_PROBE,
            );
            anyhow::ensure!(
                stats.non_finite_records == 0,
                "DDGI trace produced non-finite records: {stats:?}",
            );
            let filtered_probe_count = batch.first_probe_index + batch.probe_count;
            let probe_count = volume.status().grid.probe_count();
            if batch.first_probe_index == 0
                || filtered_probe_count == probe_count
                || filtered_probe_count % 1_024 == 0
            {
                log::info!(
                    "[DDGI] ray batch verified first_probe={} probes={} rays_per_probe={} records={} valid_probe_rays={} invalid_probe_rays={} misses={} frontface_hits={} backface_hits={} non_finite={} terrain_revision={}",
                    batch.first_probe_index,
                    batch.probe_count,
                    crate::ddgi::DDGI_RAYS_PER_PROBE,
                    stats.ray_records,
                    stats.valid_probe_rays,
                    stats.invalid_probe_rays,
                    stats.misses,
                    stats.frontface_hits,
                    stats.backface_hits,
                    stats.non_finite_records,
                    batch.terrain_revision,
                );
            }
        }

        self.graphics_pipelines
            .begin_manual_buffer_frame(gpu_profiler_frame_slot);

        let compute_to_compute_barrier = PipelineBarrier::compute_shader_access();
        // VSM filtering writes shadow_map_tex_for_vsm_ping in compute, then the
        // flora vertex shader samples it in the same command buffer. MoltenVK/Metal
        // needs the write made visible to graphics explicitly; a compute->compute
        // barrier is not enough and causes close grass shadow flicker on macOS.
        let compute_to_graphics_barrier = PipelineBarrier::shader_access(
            PipelineStage::COMPUTE_SHADER,
            PipelineStage::VERTEX_SHADER,
        );
        let color_to_compute_barrier = PipelineBarrier::new(
            PipelineStage::COLOR_ATTACHMENT_OUTPUT,
            PipelineStage::COMPUTE_SHADER,
            [MemoryBarrier::new(
                MemoryAccess::COLOR_ATTACHMENT_WRITE,
                MemoryAccess::SHADER_READ | MemoryAccess::SHADER_WRITE,
            )],
        );
        let compute_to_transfer_barrier = PipelineBarrier::new(
            PipelineStage::COMPUTE_SHADER,
            PipelineStage::TRANSFER,
            [MemoryBarrier::new(
                MemoryAccess::SHADER_WRITE,
                MemoryAccess::TRANSFER_READ | MemoryAccess::TRANSFER_WRITE,
            )],
        );
        let transfer_to_compute_barrier = PipelineBarrier::new(
            PipelineStage::TRANSFER,
            PipelineStage::COMPUTE_SHADER,
            [MemoryBarrier::new(
                MemoryAccess::TRANSFER_READ | MemoryAccess::TRANSFER_WRITE,
                MemoryAccess::SHADER_READ | MemoryAccess::SHADER_WRITE,
            )],
        );
        let transfer_to_host_barrier = PipelineBarrier::new(
            PipelineStage::TRANSFER,
            PipelineStage::HOST,
            [MemoryBarrier::new(
                MemoryAccess::TRANSFER_WRITE,
                MemoryAccess::HOST_READ,
            )],
        );
        let depth_to_compute_barrier = PipelineBarrier::new(
            PipelineStage::EARLY_FRAGMENT_TESTS | PipelineStage::LATE_FRAGMENT_TESTS,
            PipelineStage::COMPUTE_SHADER,
            [MemoryBarrier::new(
                MemoryAccess::DEPTH_STENCIL_ATTACHMENT_WRITE,
                MemoryAccess::SHADER_READ,
            )],
        );

        Self::with_gpu_scope(
            gpu_profiler.as_deref_mut(),
            gpu_profiler_frame_slot,
            cmdbuf,
            "clear.targets",
            || self.record_clear_render_targets(cmdbuf, render_flags, update_shadow_map),
        );

        let ddgi_global_sky_needs_update = self.environment_lighting_backend
            == EnvironmentLightingBackend::Ddgi
            && self.ddgi_volume.as_ref().is_some_and(|volume| {
                volume.global_sky_needs_update(self.environment_probe_environment_revision)
            });
        if ddgi_global_sky_needs_update {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.global_sky_filter",
                || self.record_ddgi_global_sky_filter_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.global_sky_gutter",
                || self.record_ddgi_global_sky_gutter_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

            let environment_revision = self.environment_probe_environment_revision;
            let volume = self
                .ddgi_volume
                .as_mut()
                .expect("DDGI sky update requires an allocated volume");
            volume.mark_global_sky_ready(environment_revision);
            log::info!(
                "[DDGI] global sky ready revision={} interior={}x{} stored={}x{} samples_per_texel=2048 stage={:?}",
                environment_revision,
                DDGI_IRRADIANCE_INTERIOR_SIDE,
                DDGI_IRRADIANCE_INTERIOR_SIDE,
                DDGI_IRRADIANCE_STORED_SIDE,
                DDGI_IRRADIANCE_STORED_SIDE,
                volume.status().stage,
            );
        }

        let ddgi_relocation_revision = self
            .ddgi_volume
            .as_ref()
            .and_then(DdgiVolume::pending_relocation_terrain_revision);
        if let Some(terrain_revision) = ddgi_relocation_revision {
            let terrain_to_relocation_barrier = PipelineBarrier::shader_access(
                PipelineStage::COMPUTE_SHADER,
                PipelineStage::COMPUTE_SHADER,
            );
            terrain_to_relocation_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.probe_relocate",
                || self.record_ddgi_probe_relocation_pass(cmdbuf, terrain_revision),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

            let volume = self
                .ddgi_volume
                .as_mut()
                .expect("DDGI relocation requires an allocated volume");
            volume.mark_relocated(terrain_revision);
            let status = volume.status();
            log::info!(
                "[DDGI] relocation complete terrain_revision={} probes={} spacing_voxels={} max_displacement_voxels={} min_clearance_voxels=1 stage={:?}",
                terrain_revision,
                status.grid.probe_count(),
                status.grid.spacing_voxels(),
                status.grid.spacing_voxels() / 2,
                status.stage,
            );
        }

        let ddgi_ray_batch = self
            .ddgi_volume
            .as_ref()
            .and_then(DdgiVolume::next_ray_batch_to_trace);
        if let Some(batch) = ddgi_ray_batch {
            {
                let volume = self
                    .ddgi_volume
                    .as_ref()
                    .expect("DDGI trace requires an allocated volume");
                volume.ddgi_trace_stats.record_fill(
                    cmdbuf,
                    0,
                    volume.status().resource_bytes.trace_stats,
                    0,
                );
            }
            transfer_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.probe_trace",
                || self.record_ddgi_probe_trace_pass(cmdbuf, batch),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

            self.ddgi_volume
                .as_mut()
                .expect("DDGI trace requires an allocated volume")
                .mark_ray_batch_ready(batch);

            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.irradiance_filter",
                || self.record_ddgi_irradiance_filter_pass(cmdbuf, batch),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.visibility_filter",
                || self.record_ddgi_visibility_filter_pass(cmdbuf, batch),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.atlas_gutters",
                || self.record_ddgi_atlas_gutter_passes(cmdbuf, batch),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

            let volume = self
                .ddgi_volume
                .as_ref()
                .expect("DDGI trace requires an allocated volume");
            compute_to_transfer_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            volume.record_trace_stats_readback(cmdbuf);
            transfer_to_host_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            self.ddgi_trace_stats_readback_pending = Some(batch);

            let volume = self
                .ddgi_volume
                .as_mut()
                .expect("DDGI filtering requires an allocated volume");
            volume.mark_ray_batch_filtered(batch);
            let mut status = volume.status();
            if status.filtered_probe_count == status.grid.probe_count() {
                volume.mark_ready();
                status = volume.status();
                log::info!(
                    "[DDGI] volume ready probes={} terrain_revision={} environment_revision={} stage={:?}",
                    status.grid.probe_count(),
                    status.relocated_terrain_revision.unwrap_or_default(),
                    status.global_sky_revision,
                    status.stage,
                );
                log::info!(
                    "[ENV_LIGHTING] backend={} ready=true terrain_revision={}",
                    self.environment_lighting_backend.label(),
                    status.relocated_terrain_revision.unwrap_or_default(),
                );
            }
            if batch.first_probe_index == 0
                || status.filtered_probe_count == status.grid.probe_count()
                || status.filtered_probe_count % 1_024 == 0
            {
                log::info!(
                    "[DDGI] atlas batch complete first_probe={} probes={} rays_per_probe={} filtered={}/{} terrain_revision={} stage={:?}",
                    batch.first_probe_index,
                    batch.probe_count,
                    crate::ddgi::DDGI_RAYS_PER_PROBE,
                    status.filtered_probe_count,
                    status.grid.probe_count(),
                    batch.terrain_revision,
                    status.stage,
                );
            }
        }

        if self.environment_probe_environment_revision != self.environment_probe_seeded_revision {
            let previous_revision = self.environment_probe_seeded_revision;
            let probe_read_to_write_barrier = PipelineBarrier::new(
                PipelineStage::VERTEX_SHADER | PipelineStage::COMPUTE_SHADER,
                PipelineStage::COMPUTE_SHADER,
                [MemoryBarrier::new(
                    MemoryAccess::SHADER_READ,
                    MemoryAccess::SHADER_WRITE,
                )],
            );
            probe_read_to_write_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            if self.environment_probe_placement_initialized {
                let probe_count = self.environment_probes.status().grid.probe_count();
                Self::with_gpu_scope(
                    gpu_profiler.as_deref_mut(),
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "environment_probes.rederive",
                    || {
                        self.record_environment_probe_update_pass(
                            cmdbuf,
                            0,
                            probe_count,
                            false,
                            None,
                        )
                    },
                );
                log::info!(
                    "[ENV_PROBES] rederived local SH previous_revision={} revision={} probes={} terrain_rays=0",
                    previous_revision,
                    self.environment_probe_environment_revision,
                    probe_count,
                );
            } else {
                Self::with_gpu_scope(
                    gpu_profiler.as_deref_mut(),
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "environment_probes.global_copy",
                    || self.record_environment_probe_global_copy_pass(cmdbuf),
                );
            }
            let probe_write_barrier = PipelineBarrier::shader_access(
                PipelineStage::COMPUTE_SHADER,
                PipelineStage::COMPUTE_SHADER | PipelineStage::VERTEX_SHADER,
            );
            probe_write_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            self.environment_probe_seeded_revision = self.environment_probe_environment_revision;
            if !self.environment_probe_placement_initialized {
                self.environment_probes.mark_all_valid_global_copy();
            }
            if previous_revision == 0 {
                let status = self.environment_probes.status();
                log::info!(
                    "[ENV_PROBES] seeded global SH revision={} probes={} valid={}",
                    self.environment_probe_seeded_revision,
                    status.grid.probe_count(),
                    status.valid_probe_count,
                );
            }
        }

        if self.environment_probe_classification_pending {
            let retain_partial_local_field = self.environment_probe_local_field_ready;
            let probe_read_to_classification_barrier = PipelineBarrier::new(
                PipelineStage::VERTEX_SHADER | PipelineStage::COMPUTE_SHADER,
                PipelineStage::COMPUTE_SHADER,
                [MemoryBarrier::new(
                    MemoryAccess::SHADER_READ | MemoryAccess::SHADER_WRITE,
                    MemoryAccess::SHADER_WRITE,
                )],
            );
            probe_read_to_classification_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "environment_probes.classify",
                || self.record_environment_probe_classification_pass(cmdbuf),
            );
            let probe_classification_write_barrier = PipelineBarrier::shader_access(
                PipelineStage::COMPUTE_SHADER,
                PipelineStage::COMPUTE_SHADER | PipelineStage::VERTEX_SHADER,
            );
            probe_classification_write_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            self.environment_probe_classification_pending = false;
            self.environment_probe_placement_initialized = true;
            self.environment_probe_trace_cursor = 0;
            self.environment_probe_local_field_ready = retain_partial_local_field;
            self.environment_probes.mark_all_classified_dirty();
            let status = self.environment_probes.status();
            log::info!(
                "[ENV_PROBES] classified placement probes={} spacing_voxels={} terrain_revision={} retained_partial_field={} usable=0 state=dirty_or_invalid",
                status.grid.probe_count(),
                status.grid.spacing_voxels(),
                self.environment_probe_terrain_revision,
                retain_partial_local_field,
            );

            let mut priority_dispatch_count = 0;
            for region in self
                .environment_probe_priority_regions
                .into_iter()
                .flatten()
            {
                let region_probe_count = region.dispatch_probe_count();
                Self::with_gpu_scope(
                    gpu_profiler.as_deref_mut(),
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "environment_probes.trace_priority",
                    || {
                        self.record_environment_probe_update_pass(
                            cmdbuf,
                            0,
                            region_probe_count,
                            true,
                            Some(region),
                        )
                    },
                );
                let priority_write_barrier = PipelineBarrier::shader_access(
                    PipelineStage::COMPUTE_SHADER,
                    PipelineStage::COMPUTE_SHADER | PipelineStage::VERTEX_SHADER,
                );
                priority_write_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
                priority_dispatch_count += region_probe_count;
            }
            self.environment_probe_priority_regions = [None, None];
            if priority_dispatch_count > 0 {
                let response_ms = self
                    .environment_probe_refresh_started_at
                    .map(|started| started.elapsed().as_secs_f64() * 1000.0)
                    .unwrap_or(0.0);
                log::info!(
                    "[ENV_PROBES] terrain local response scheduled revision={} priority_dispatch_probes={} rays_per_probe={} schedule_ms={:.2}",
                    self.environment_probe_terrain_revision,
                    priority_dispatch_count,
                    ENVIRONMENT_PROBE_TRACE_RAYS_PER_PROBE,
                    response_ms,
                );
            }
        }

        let probe_count = self.environment_probes.status().grid.probe_count();
        if self.environment_probe_placement_initialized
            && self.environment_probe_trace_cursor < probe_count
        {
            let first_probe_index = self.environment_probe_trace_cursor;
            let batch_probe_count =
                (probe_count - first_probe_index).min(ENVIRONMENT_PROBE_TRACE_BATCH_SIZE);
            let probe_read_to_trace_barrier = PipelineBarrier::new(
                PipelineStage::VERTEX_SHADER | PipelineStage::COMPUTE_SHADER,
                PipelineStage::COMPUTE_SHADER,
                [MemoryBarrier::new(
                    MemoryAccess::SHADER_READ | MemoryAccess::SHADER_WRITE,
                    MemoryAccess::SHADER_READ | MemoryAccess::SHADER_WRITE,
                )],
            );
            probe_read_to_trace_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "environment_probes.trace",
                || {
                    self.record_environment_probe_update_pass(
                        cmdbuf,
                        first_probe_index,
                        batch_probe_count,
                        true,
                        None,
                    )
                },
            );
            let probe_trace_write_barrier = PipelineBarrier::shader_access(
                PipelineStage::COMPUTE_SHADER,
                PipelineStage::COMPUTE_SHADER | PipelineStage::VERTEX_SHADER,
            );
            probe_trace_write_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            self.environment_probe_trace_cursor += batch_probe_count;
            if first_probe_index == 0 {
                log::info!(
                    "[ENV_PROBES] visibility trace started probes={} batch_size={} rays_per_probe={} environment_revision={} terrain_revision={}",
                    probe_count,
                    ENVIRONMENT_PROBE_TRACE_BATCH_SIZE,
                    ENVIRONMENT_PROBE_TRACE_RAYS_PER_PROBE,
                    self.environment_probe_environment_revision,
                    self.environment_probe_terrain_revision,
                );
            }
            if self.environment_probe_trace_cursor == probe_count {
                self.environment_probe_local_field_ready = true;
                self.environment_probe_converged_terrain_revision =
                    self.environment_probe_terrain_revision;
                self.environment_probes.environment_probe_stats.record_fill(
                    cmdbuf,
                    0,
                    self.environment_probes
                        .environment_probe_stats
                        .get_size_bytes(),
                    0,
                );
                transfer_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
                Self::with_gpu_scope(
                    gpu_profiler.as_deref_mut(),
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "environment_probes.stats",
                    || self.record_environment_probe_stats_pass(cmdbuf),
                );
                compute_to_transfer_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
                self.environment_probes.record_state_counts_readback(cmdbuf);
                transfer_to_host_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
                self.environment_probe_stats_readback_pending = true;
                let convergence_ms = self
                    .environment_probe_refresh_started_at
                    .take()
                    .map(|started| started.elapsed().as_secs_f64() * 1000.0);
                log::info!(
                    "[ENV_PROBES] visibility trace complete processed={} environment_revision={} terrain_revision={} convergence_ms={} local_field_ready=true",
                    probe_count,
                    self.environment_probe_environment_revision,
                    self.environment_probe_terrain_revision,
                    convergence_ms
                        .map(|value| format!("{value:.2}"))
                        .unwrap_or_else(|| "initial".to_string()),
                );
                log::info!(
                    "[ENV_LIGHTING] backend={} ready={} terrain_revision={}",
                    self.environment_lighting_backend.label(),
                    self.environment_lighting_backend_ready(),
                    self.environment_probe_converged_terrain_revision,
                );
            }
        }

        let has_graphics_pass = render_flags.enable_flora
            || render_flags.enable_particles
            || self.sprinkler_resources.instance_count > 0
            || self.irrigation_pipe_resources.instance_count > 0
            || self.geometry_preview_resources.has_visible_mesh()
            || self.environment_probe_visualization.enabled
            || self.dynamic_fruit_resources.instance_count > 0;

        if render_flags.enable_flora {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "wind_volume.pass",
                || self.record_wind_volume_pass(cmdbuf, time),
            );

            let b1 = PipelineBarrier::shader_access(
                PipelineStage::COMPUTE_SHADER,
                PipelineStage::VERTEX_SHADER | PipelineStage::COMPUTE_SHADER,
            );
            b1.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        if render_flags.enable_flora
            && render_flags.enable_leaves
            && render_flags.enable_shadows
            && update_shadow_map
        {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "leaf_shadow_opacity.pass",
                || {
                    self.record_leaves_shadow_lod_pass(
                        cmdbuf,
                        surface_resources,
                        leaf_color_tables,
                        time,
                    )
                },
            );
            color_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "leaf_shadow_temporal.pass",
                || {
                    self.record_leaf_shadow_temporal_pass(
                        cmdbuf,
                        leaf_shadow_temporal_alpha,
                        reset_vsm_history,
                    )
                },
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "leaf_shadow_mask.pass",
                || self.record_leaf_shadow_mask_pass(cmdbuf),
            );
            compute_to_transfer_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            self.record_store_leaf_shadow_history(cmdbuf);
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }
        if has_graphics_pass || (render_flags.enable_shadows && update_shadow_map) {
            let frag_to_compute_barrier = PipelineBarrier::shader_access(
                PipelineStage::FRAGMENT_SHADER,
                PipelineStage::COMPUTE_SHADER,
            );
            frag_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        if render_flags.enable_shadows && update_shadow_map {
            let dynamic_fruit_shadow_changed = self.dynamic_fruit_resources.take_shadow_changed();
            if self.dynamic_fruit_resources.instance_count > 0 {
                Self::with_gpu_scope(
                    gpu_profiler.as_deref_mut(),
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "dynamic_fruit_shadow.pass",
                    || self.record_dynamic_fruit_shadow_pass(cmdbuf),
                );
                depth_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            }
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "shadow_depth_copy.pass",
                || self.record_shadow_depth_copy_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "tracer_shadow.pass",
                || self.record_tracer_shadow_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "vsm_filtering.pass",
                || {
                    self.record_vsm_filtering_pass(
                        cmdbuf,
                        vsm_blur_radius,
                        vsm_temporal_alpha,
                        reset_vsm_history || dynamic_fruit_shadow_changed,
                    )
                },
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            if render_flags.enable_clouds {
                Self::with_gpu_scope(
                    gpu_profiler.as_deref_mut(),
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "cloud_shadow.pass",
                    || self.record_cloud_shadow_pass(cmdbuf),
                );
                compute_to_transfer_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
                self.record_store_cloud_shadow_history(cmdbuf);
                transfer_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            } else {
                self.cloud_shadow_history_valid = false;
            }
            compute_to_graphics_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        if has_graphics_pass && !render_flags.enable_flora {
            let b1 = PipelineBarrier::shader_access(
                PipelineStage::COMPUTE_SHADER,
                PipelineStage::VERTEX_SHADER | PipelineStage::COMPUTE_SHADER,
            );
            b1.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_trace_after_shadow_prepass(
        &mut self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
        lod_distance: f32,
        flora_draw_distance: f32,
        grass_render_mode: u32,
        time: f32,
        flora_color_tables: &[FloraHeightColorTables],
        leaf_color_tables: FloraHeightColorTables,
        render_flags: &crate::RenderFlags,
        mut gpu_profiler: Option<&mut GpuProfiler>,
        gpu_profiler_frame_slot: usize,
    ) -> Result<()> {
        let compute_to_compute_barrier = PipelineBarrier::compute_shader_access();
        let tracer_to_raster_barrier = PipelineBarrier::new(
            PipelineStage::COMPUTE_SHADER,
            PipelineStage::FRAGMENT_SHADER | PipelineStage::COMPUTE_SHADER,
            [MemoryBarrier::new(
                MemoryAccess::SHADER_WRITE,
                MemoryAccess::SHADER_READ,
            )],
        );
        let tracer_clear_to_raster_barrier = PipelineBarrier::new(
            PipelineStage::TRANSFER,
            PipelineStage::FRAGMENT_SHADER | PipelineStage::COMPUTE_SHADER,
            [MemoryBarrier::new(
                MemoryAccess::TRANSFER_WRITE,
                MemoryAccess::SHADER_READ,
            )],
        );
        let raster_to_compute_barrier = PipelineBarrier::new(
            PipelineStage::COLOR_ATTACHMENT_OUTPUT
                | PipelineStage::EARLY_FRAGMENT_TESTS
                | PipelineStage::LATE_FRAGMENT_TESTS,
            PipelineStage::COMPUTE_SHADER,
            [MemoryBarrier::new(
                MemoryAccess::COLOR_ATTACHMENT_WRITE | MemoryAccess::DEPTH_STENCIL_ATTACHMENT_WRITE,
                MemoryAccess::SHADER_READ,
            )],
        );
        let compute_to_transfer_barrier = PipelineBarrier::new(
            PipelineStage::COMPUTE_SHADER,
            PipelineStage::TRANSFER,
            [MemoryBarrier::new(
                MemoryAccess::SHADER_WRITE,
                MemoryAccess::TRANSFER_READ | MemoryAccess::TRANSFER_WRITE,
            )],
        );
        let transfer_to_compute_barrier = PipelineBarrier::new(
            PipelineStage::TRANSFER,
            PipelineStage::COMPUTE_SHADER,
            [MemoryBarrier::new(
                MemoryAccess::TRANSFER_READ | MemoryAccess::TRANSFER_WRITE,
                MemoryAccess::SHADER_READ | MemoryAccess::SHADER_WRITE,
            )],
        );

        // Terrarium glass is composited analytically in composition.comp so it can refract the
        // already-combined scene and depth-test against ray-traced terrain. Keep it out of the
        // raster graphics pass to avoid transparent-layer accumulation and coplanar edge shimmer.
        let enable_glass = false;
        let has_graphics_pass = render_flags.enable_flora
            || render_flags.enable_particles
            || self.sprinkler_resources.instance_count > 0
            || self.irrigation_pipe_resources.instance_count > 0
            || self.geometry_preview_resources.has_visible_mesh()
            || self.environment_probe_visualization.enabled
            || self.dynamic_fruit_resources.instance_count > 0;

        if render_flags.enable_flora {
            assert_eq!(
                flora_color_tables.len(),
                self.resources.meshes.flora_meshes.len(),
                "Flora color-table count ({}) must match flora mesh count ({})",
                flora_color_tables.len(),
                self.resources.meshes.flora_meshes.len()
            );
        }

        // Terrain depth must exist before the raster pass. The raster pass seeds
        // its hardware depth attachment from this output so every raster
        // fragment is tested against terrain before transparent blending can
        // discard the individual depths of layers behind it.
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        if render_flags.enable_tracer {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "tracer.pass",
                || self.record_tracer_pass(cmdbuf),
            );
            tracer_to_raster_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        } else {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "tracer_clear.pass",
                || self.clear_tracer_outputs(cmdbuf),
            );
            tracer_clear_to_raster_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        if has_graphics_pass {
            if let Some(profiler) = gpu_profiler.as_deref_mut() {
                let graphics_scope = profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.pass",
                    PipelineStage::ALL_COMMANDS,
                );
                self.record_all_graphics_passes(
                    cmdbuf,
                    surface_resources,
                    lod_distance,
                    flora_draw_distance,
                    grass_render_mode,
                    flora_color_tables,
                    leaf_color_tables,
                    time,
                    render_flags.enable_flora,
                    render_flags.enable_leaves,
                    render_flags.enable_particles,
                    enable_glass,
                    Some(profiler),
                    gpu_profiler_frame_slot,
                );
                if let Some(scope) = graphics_scope {
                    profiler.end_scope(
                        gpu_profiler_frame_slot,
                        cmdbuf,
                        scope,
                        PipelineStage::ALL_COMMANDS,
                    );
                }
            } else {
                self.record_all_graphics_passes(
                    cmdbuf,
                    surface_resources,
                    lod_distance,
                    flora_draw_distance,
                    grass_render_mode,
                    flora_color_tables,
                    leaf_color_tables,
                    time,
                    render_flags.enable_flora,
                    render_flags.enable_leaves,
                    render_flags.enable_particles,
                    enable_glass,
                    None,
                    gpu_profiler_frame_slot,
                );
            }
            raster_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        if render_flags.enable_god_rays {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "god_ray.pass",
                || self.record_god_ray_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        if render_flags.enable_clouds {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "cloud.pass",
                || self.record_cloud_pass(cmdbuf),
            );
            compute_to_transfer_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            self.record_store_cloud_history(cmdbuf);
            transfer_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        } else {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "cloud_clear.pass",
                || self.clear_cloud_output(cmdbuf),
            );
        }
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        if render_flags.enable_lens_flare {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "lens_flare_sun_visible.pass",
                || self.record_lens_flare_sun_visible_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "lens_flare.pass",
                || self.record_lens_flare_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "lens_flare_downsample.pass",
                || self.record_lens_flare_downsample_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }
        Self::with_gpu_scope(
            gpu_profiler.as_deref_mut(),
            gpu_profiler_frame_slot,
            cmdbuf,
            "composition.pass",
            || self.record_composition_pass(cmdbuf),
        );
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        if let Some(profiler) = gpu_profiler {
            let postprocessing_scope = profiler.begin_scope(
                gpu_profiler_frame_slot,
                cmdbuf,
                "post_processing.pass",
                PipelineStage::ALL_COMMANDS,
            );
            self.record_post_processing_pass(cmdbuf);
            if let Some(scope) = postprocessing_scope {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        } else {
            self.record_post_processing_pass(cmdbuf);
        }
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        Ok(())
    }

    fn record_store_vsm_history(&self, cmdbuf: &CommandBuffer) {
        let history = self.resources.shadow.vsm_history();
        history.current().get_image().record_copy_to(
            cmdbuf,
            history.previous().get_image(),
            TextureLayout::GENERAL,
            TextureLayout::GENERAL,
        );
    }

    fn record_store_cloud_history(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .cloud_output_tex
            .get_image()
            .record_copy_to(
                cmdbuf,
                self.resources
                    .extent_dependent_resources
                    .cloud_history_tex
                    .get_image(),
                TextureLayout::GENERAL,
                TextureLayout::GENERAL,
            );
    }

    fn record_store_cloud_shadow_history(&self, cmdbuf: &CommandBuffer) {
        let history = self.resources.shadow.cloud_shadow_history();
        history.current().get_image().record_copy_to(
            cmdbuf,
            history.previous().get_image(),
            TextureLayout::GENERAL,
            TextureLayout::GENERAL,
        );
    }

    fn record_clear_render_targets(
        &self,
        cmdbuf: &CommandBuffer,
        render_flags: &crate::RenderFlags,
        update_shadow_map: bool,
    ) {
        let has_graphics_pass = render_flags.enable_flora
            || render_flags.enable_particles
            || self.sprinkler_resources.instance_count > 0
            || self.irrigation_pipe_resources.instance_count > 0
            || self.geometry_preview_resources.has_visible_mesh()
            || self.dynamic_fruit_resources.instance_count > 0;
        if !has_graphics_pass {
            self.resources
                .extent_dependent_resources
                .gfx_output_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
                );
            self.resources
                .extent_dependent_resources
                .gfx_depth_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::DepthStencil(DepthOrStencilClearValue::Depth(1.0)),
                );
        }

        if update_shadow_map {
            self.resources
                .shadow
                .shadow_map_depth_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::DepthStencil(DepthOrStencilClearValue::Depth(1.0)),
                );

            self.resources
                .shadow
                .shadow_map_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::Float([1.0, 0.0, 0.0, 0.0])),
                );

            for tex in [
                &self.resources.shadow.cloud_shadow_raw_tex,
                &self.resources.shadow.cloud_shadow_tex,
            ] {
                tex.get_image().record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::Float([1.0, 0.0, 0.0, 0.0])),
                );
            }

            self.resources
                .shadow
                .leaf_shadow_opacity_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
                );

            self.resources
                .shadow
                .leaf_shadow_mask_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
                );
        }

        self.resources
            .extent_dependent_resources
            .god_ray_output_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
            );

        self.resources
            .extent_dependent_resources
            .lens_flare_full_output_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
            );

        self.resources
            .extent_dependent_resources
            .lens_flare_output_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
            );

        if render_flags.enable_lens_flare {
            self.resources
                .extent_dependent_resources
                .lens_flare_visible_count_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
                );
        }
    }

    /// Draw all flora species (LOD0+LOD1), leaves (LOD0+LOD1), and particles
    /// inside a single Vulkan render pass to avoid tile-memory load/store
    /// overhead on tile-based GPUs (Apple Silicon via MoltenVK) and prevent
    /// the particle pass from clearing the flora output.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::needless_option_as_deref)]
    fn record_all_graphics_passes(
        &self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
        lod_distance: f32,
        flora_draw_distance: f32,
        grass_render_mode: u32,
        flora_color_tables: &[FloraHeightColorTables],
        leaf_color_tables: FloraHeightColorTables,
        time: f32,
        enable_flora: bool,
        enable_leaves: bool,
        enable_particles: bool,
        enable_glass: bool,
        mut gpu_profiler: Option<&mut GpuProfiler>,
        gpu_profiler_frame_slot: usize,
    ) {
        let render_target = &self.render_target_color_and_depth;

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        self.graphics_pipelines
            .terrain_depth_prefill_ppl
            .record_texture_transitions(cmdbuf);
        if enable_flora {
            for pipeline in [
                &self.graphics_pipelines.flora_ppl,
                &self.graphics_pipelines.flora_lod_ppl,
                &self.graphics_pipelines.leaves_ppl,
                &self.graphics_pipelines.leaves_lod_ppl,
            ] {
                pipeline.record_texture_transitions(cmdbuf);
            }
        }
        if self.sprinkler_resources.instance_count > 0
            || self.irrigation_pipe_resources.instance_count > 0
        {
            self.graphics_pipelines
                .sprinkler_ppl
                .record_texture_transitions(cmdbuf);
        }
        if self.geometry_preview_resources.has_visible_mesh() {
            self.graphics_pipelines
                .geometry_preview_ppl
                .record_texture_transitions(cmdbuf);
        }
        if self.dynamic_fruit_resources.instance_count > 0 {
            self.graphics_pipelines
                .dynamic_fruit_ppl
                .record_texture_transitions(cmdbuf);
        }
        if enable_particles {
            self.graphics_pipelines
                .particle_ppl
                .record_texture_transitions(cmdbuf);
            self.graphics_pipelines
                .water_droplet_ppl
                .record_texture_transitions(cmdbuf);
        }
        if enable_glass {
            self.graphics_pipelines
                .glass_ppl
                .record_texture_transitions(cmdbuf);
        }

        Self::with_gpu_scope(
            gpu_profiler.as_deref_mut(),
            gpu_profiler_frame_slot,
            cmdbuf,
            "graphics.renderpass.begin",
            || render_target.record_begin(cmdbuf, &clear_values),
        );

        let render_extent = self
            .resources
            .extent_dependent_resources
            .gfx_output_tex
            .get_image()
            .get_desc()
            .extent;
        let viewport = Viewport::from_extent(render_extent.as_extent_2d().unwrap());
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: render_extent.width,
                height: render_extent.height,
            },
        };

        // Seed the hardware depth attachment with ray-traced terrain before any
        // raster geometry is blended. This preserves per-fragment terrain
        // occlusion even when a nearer translucent raster object later owns the
        // final raster depth pixel.
        let terrain_depth_prefill = &self.graphics_pipelines.terrain_depth_prefill_ppl;
        terrain_depth_prefill.record_bind(cmdbuf);
        terrain_depth_prefill.record_viewport_scissor(cmdbuf, viewport, scissor);
        cmdbuf.bind_vertex_buffers(0, &[&self.resources.meshes.terrain_depth_prefill_vertices]);
        terrain_depth_prefill.record(cmdbuf, 3, 1, 0, 0, None);

        // Draw all flora species, both LOD levels
        if enable_flora {
            let flora_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.flora",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let chunks_by_lod = self.chunks_needs_to_draw_this_frame(
                surface_resources,
                lod_distance,
                flora_draw_distance,
            );
            for (species_index, height_color_tables) in flora_color_tables.iter().enumerate() {
                if !should_render_grass_species(species_index, grass_render_mode) {
                    continue;
                }

                for &lod_state in &[LodState::Lod0, LodState::Lod1] {
                    let pipeline = match lod_state {
                        LodState::Lod0 => &self.graphics_pipelines.flora_ppl,
                        LodState::Lod1 => &self.graphics_pipelines.flora_lod_ppl,
                    };
                    let mesh_collection = match lod_state {
                        LodState::Lod0 => &self.resources.meshes.flora_meshes,
                        LodState::Lod1 => &self.resources.meshes.flora_meshes_lod,
                    };
                    let mesh = mesh_collection.get(species_index).unwrap_or_else(|| {
                        panic!("Missing flora mesh for species index {}", species_index)
                    });

                    pipeline.record_bind(cmdbuf);
                    pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);

                    cmdbuf.bind_index_buffer_u32(&mesh.indices);

                    let flora_instances = &chunks_by_lod[&lod_state];
                    for instances in flora_instances.iter() {
                        let instance_count = instances.species_len(species_index);
                        if instance_count == 0 {
                            continue;
                        }
                        let instance_offset = FloraInstanceResources::species_offset(species_index);
                        let push_constant = flora_push_constant(
                            time,
                            species_index as u32,
                            instances.chunk_world_offset,
                            *height_color_tables,
                        );

                        cmdbuf.bind_vertex_buffers(0, &[&mesh.vertices]);
                        pipeline.record_indexed_with_manual_buffers(
                            cmdbuf,
                            1,
                            &[
                                (0, &instances.resource.instances_buf),
                                (1, &instances.grass_growth_potential_levels),
                            ],
                            mesh.indices_len,
                            instance_count,
                            0,
                            0,
                            instance_offset,
                            Some(&PushConstantInfo {
                                shader_stage: vk::ShaderStageFlags::VERTEX,
                                push_constants: bytemuck::bytes_of(&push_constant).to_vec(),
                            }),
                        );
                    }
                }
            }
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), flora_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }

            // Draw leaves, both LOD levels.
            if enable_leaves {
                let leaves_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                    profiler.begin_scope(
                        gpu_profiler_frame_slot,
                        cmdbuf,
                        "graphics.leaves",
                        PipelineStage::ALL_COMMANDS,
                    )
                });
                let trees_by_lod = self.trees_needs_to_draw_this_frame(
                    &surface_resources.instances.leaves_instances,
                    lod_distance,
                    flora_draw_distance,
                );
                for &lod_state in &[LodState::Lod0, LodState::Lod1] {
                    let pipeline = match lod_state {
                        LodState::Lod0 => &self.graphics_pipelines.leaves_ppl,
                        LodState::Lod1 => &self.graphics_pipelines.leaves_lod_ppl,
                    };
                    let (indices_buf, vertices_buf, indices_len) = match lod_state {
                        LodState::Lod0 => (
                            &self.resources.meshes.leaves_resources.indices,
                            &self.resources.meshes.leaves_resources.vertices,
                            self.resources.meshes.leaves_resources.indices_len,
                        ),
                        LodState::Lod1 => (
                            &self.resources.meshes.leaves_resources_lod.indices,
                            &self.resources.meshes.leaves_resources_lod.vertices,
                            self.resources.meshes.leaves_resources_lod.indices_len,
                        ),
                    };

                    let leaves_instances = &trees_by_lod[&lod_state];
                    if leaves_instances.is_empty() {
                        continue;
                    }

                    pipeline.record_bind(cmdbuf);
                    pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);

                    cmdbuf.bind_index_buffer_u32(indices_buf);

                    for tree_instance in leaves_instances.iter() {
                        if tree_instance.resources.instances_len == 0 {
                            continue;
                        }
                        let leaf_push = flora_push_constant(
                            time,
                            LEAF_INSTANCE_TYPE,
                            tree_instance.chunk_world_offset,
                            leaf_color_tables,
                        );
                        cmdbuf.bind_vertex_buffers(0, &[vertices_buf]);
                        pipeline.record_indexed_with_manual_buffer(
                            cmdbuf,
                            1,
                            0,
                            &tree_instance.resources.instances_buf,
                            indices_len,
                            tree_instance.resources.instances_len,
                            0,
                            0,
                            0,
                            Some(&PushConstantInfo {
                                shader_stage: vk::ShaderStageFlags::VERTEX,
                                push_constants: bytemuck::bytes_of(&leaf_push).to_vec(),
                            }),
                        );
                    }
                }
                if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), leaves_scope) {
                    profiler.end_scope(
                        gpu_profiler_frame_slot,
                        cmdbuf,
                        scope,
                        PipelineStage::ALL_COMMANDS,
                    );
                }
            }

            // Draw apples as render-only tree fruit instances using the same
            // foliage shaders as leaves, but with a smaller apple mesh and a
            // distinct instance type for color/motion.
            let apples_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.apples",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let apples_by_lod = self.trees_needs_to_draw_this_frame(
                &surface_resources.instances.apple_instances,
                lod_distance,
                flora_draw_distance,
            );
            for &lod_state in &[LodState::Lod0, LodState::Lod1] {
                let pipeline = match lod_state {
                    LodState::Lod0 => &self.graphics_pipelines.leaves_ppl,
                    LodState::Lod1 => &self.graphics_pipelines.leaves_lod_ppl,
                };
                let (indices_buf, vertices_buf, indices_len) = match lod_state {
                    LodState::Lod0 => (
                        &self.resources.meshes.apple_resources.indices,
                        &self.resources.meshes.apple_resources.vertices,
                        self.resources.meshes.apple_resources.indices_len,
                    ),
                    LodState::Lod1 => (
                        &self.resources.meshes.apple_resources_lod.indices,
                        &self.resources.meshes.apple_resources_lod.vertices,
                        self.resources.meshes.apple_resources_lod.indices_len,
                    ),
                };

                let apple_instances = &apples_by_lod[&lod_state];
                if apple_instances.is_empty() {
                    continue;
                }

                pipeline.record_bind(cmdbuf);
                pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
                cmdbuf.bind_index_buffer_u32(indices_buf);

                for tree_instance in apple_instances.iter() {
                    if tree_instance.resources.instances_len == 0 {
                        continue;
                    }
                    let apple_push = flora_push_constant(
                        time,
                        APPLE_INSTANCE_TYPE,
                        tree_instance.chunk_world_offset,
                        solid_flora_height_color_tables(APPLE_BOTTOM_COLOR, APPLE_TIP_COLOR),
                    );
                    cmdbuf.bind_vertex_buffers(0, &[vertices_buf]);
                    pipeline.record_indexed_with_manual_buffer(
                        cmdbuf,
                        1,
                        0,
                        &tree_instance.resources.instances_buf,
                        indices_len,
                        tree_instance.resources.instances_len,
                        0,
                        0,
                        0,
                        Some(&PushConstantInfo {
                            shader_stage: vk::ShaderStageFlags::VERTEX,
                            push_constants: bytemuck::bytes_of(&apple_push).to_vec(),
                        }),
                    );
                }
            }
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), apples_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        } // end enable_flora

        if self.irrigation_pipe_resources.instance_count > 0 {
            let pipes_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.irrigation_pipes",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let resources = &self.irrigation_pipe_resources;
            let pipeline = &self.graphics_pipelines.sprinkler_ppl;
            pipeline.record_bind(cmdbuf);
            pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
            cmdbuf.bind_index_buffer_u32(&resources.indices);
            cmdbuf.bind_vertex_buffers(0, &[&resources.vertices, &resources.instances]);
            pipeline.record_indexed(
                cmdbuf,
                resources.indices_len,
                resources.instance_count,
                0,
                0,
                0,
                None,
            );
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), pipes_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        }

        if self.sprinkler_resources.instance_count > 0 {
            let sprinklers_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.sprinklers",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let resources = &self.sprinkler_resources;
            let pipeline = &self.graphics_pipelines.sprinkler_ppl;
            pipeline.record_bind(cmdbuf);
            pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
            cmdbuf.bind_index_buffer_u32(&resources.indices);
            cmdbuf.bind_vertex_buffers(0, &[&resources.vertices, &resources.instances]);
            pipeline.record_indexed(
                cmdbuf,
                resources.indices_len,
                resources.instance_count,
                0,
                0,
                0,
                None,
            );
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), sprinklers_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        }

        if self.geometry_preview_resources.has_visible_mesh() {
            let preview_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.geometry_preview",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let pipeline = &self.graphics_pipelines.geometry_preview_ppl;
            pipeline.record_bind(cmdbuf);
            pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
            for resources in [
                &self.geometry_preview_resources.pipe,
                &self.geometry_preview_resources.tree,
            ] {
                if resources.instance_count == 0 {
                    continue;
                }
                cmdbuf.bind_index_buffer_u32(&resources.indices);
                cmdbuf.bind_vertex_buffers(0, &[&resources.vertices, &resources.instances]);
                pipeline.record_indexed(
                    cmdbuf,
                    resources.indices_len,
                    resources.instance_count,
                    0,
                    0,
                    0,
                    None,
                );
            }
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), preview_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        }

        let environment_probe_instance_count = self
            .environment_probe_visualization
            .submitted_instance_count(self.environment_probes.status().grid.probe_count());
        if environment_probe_instance_count > 0 {
            let probe_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.environment_probes",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let pipeline = if self.environment_probe_visualization.depth_tested {
                &self
                    .graphics_pipelines
                    .environment_probe_visualization_depth_ppl
            } else {
                &self
                    .graphics_pipelines
                    .environment_probe_visualization_overlay_ppl
            };
            let push_constants = EnvironmentProbeVisualizationPushConstants::new(
                self.environment_probe_visualization,
                self.desc.voxel_dim_per_chunk,
            );
            pipeline.record_bind(cmdbuf);
            pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
            cmdbuf.bind_index_buffer_u32(
                &self
                    .environment_probe_visualization_resources
                    .marker_indices,
            );
            cmdbuf.bind_vertex_buffers(
                0,
                &[&self
                    .environment_probe_visualization_resources
                    .marker_vertices],
            );
            pipeline.record_indexed(
                cmdbuf,
                self.environment_probe_visualization_resources.index_count(),
                environment_probe_instance_count,
                0,
                0,
                0,
                Some(&PushConstantInfo {
                    shader_stage: vk::ShaderStageFlags::VERTEX,
                    push_constants: bytemuck::bytes_of(&push_constants).to_vec(),
                }),
            );
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), probe_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        }

        if self.dynamic_fruit_resources.instance_count > 0 {
            let fruit_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.dynamic_fruit",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let resources = &self.dynamic_fruit_resources;
            let pipeline = &self.graphics_pipelines.dynamic_fruit_ppl;
            pipeline.record_bind(cmdbuf);
            pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
            cmdbuf.bind_index_buffer_u32(&resources.indices);
            cmdbuf.bind_vertex_buffers(0, &[&resources.vertices, &resources.instances]);
            pipeline.record_indexed(
                cmdbuf,
                resources.indices_len,
                resources.instance_count,
                0,
                0,
                0,
                None,
            );
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), fruit_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        }

        // Draw particles in the same render pass (no second CLEAR)
        if enable_particles {
            let particles_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.particles",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let particle_resources = &self.particle_resources;
            let draw_particles =
                |pipeline: &GraphicsPipeline, instance_buffer: &Buffer, instance_count: u32| {
                    if instance_count == 0 {
                        return;
                    }
                    pipeline.record_bind(cmdbuf);
                    pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
                    cmdbuf.bind_index_buffer_u32(&particle_resources.indices);
                    cmdbuf.bind_vertex_buffers(0, &[&particle_resources.vertices, instance_buffer]);
                    pipeline.record_indexed(
                        cmdbuf,
                        particle_resources.indices_len,
                        instance_count,
                        0,
                        0,
                        0,
                        None,
                    );
                };
            draw_particles(
                &self.graphics_pipelines.particle_ppl,
                &particle_resources.instance_buffer,
                particle_resources.instance_count,
            );
            // Translucent droplets are sorted back-to-front and rendered after ordinary
            // particles. Their nearest depth lets the hybrid compositor place them over the
            // ray-traced terrain while preserving correct blending between sorted droplets.
            draw_particles(
                &self.graphics_pipelines.water_droplet_ppl,
                &particle_resources.translucent_instance_buffer,
                particle_resources.translucent_instance_count,
            );
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), particles_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        }

        if enable_glass {
            let glass_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.terrarium_glass",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let glass = &self.resources.meshes.glass;
            if glass.indices_len > 0 {
                let glass_ppl = &self.graphics_pipelines.glass_ppl;
                glass_ppl.record_bind(cmdbuf);
                glass_ppl.record_viewport_scissor(cmdbuf, viewport, scissor);
                cmdbuf.bind_index_buffer_u32(&glass.indices);
                cmdbuf.bind_vertex_buffers(0, &[&glass.vertices]);

                let box_min = glass.box_min;
                let box_max = glass.box_max;
                let push = GlassPushConstants {
                    box_min_near_alpha: [
                        box_min.x,
                        box_min.y,
                        box_min.z,
                        TERRARIUM_GLASS_NEAR_ALPHA,
                    ],
                    box_max_far_alpha: [box_max.x, box_max.y, box_max.z, TERRARIUM_GLASS_FAR_ALPHA],
                };
                let box_center = (box_min + box_max) * 0.5;
                let face_centers = [
                    Vec3::new(box_min.x, box_center.y, box_center.z),
                    Vec3::new(box_max.x, box_center.y, box_center.z),
                    Vec3::new(box_center.x, box_center.y, box_min.z),
                    Vec3::new(box_center.x, box_center.y, box_max.z),
                ];
                let camera_position = self.camera.position();
                let mut face_order = [0usize, 1, 2, 3];
                face_order.sort_by(|left, right| {
                    let left_dist = face_centers[*left].distance_squared(camera_position);
                    let right_dist = face_centers[*right].distance_squared(camera_position);
                    left_dist
                        .partial_cmp(&right_dist)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                if glass.edge_indices_len > 0 {
                    glass_ppl.record_indexed(
                        cmdbuf,
                        glass.edge_indices_len,
                        1,
                        glass.edge_index_start,
                        0,
                        0,
                        Some(&PushConstantInfo {
                            shader_stage: vk::ShaderStageFlags::VERTEX,
                            push_constants: bytemuck::bytes_of(&push).to_vec(),
                        }),
                    );
                }

                // Draw the nearest glass panes first while depth writing is enabled. This keeps the
                // front pane as the single composited transparent layer over ray-traced terrain,
                // while the separate bevel/rim geometry above provides the visible optical edges.
                for face_id in face_order {
                    glass_ppl.record_indexed(
                        cmdbuf,
                        glass.pane_index_count,
                        1,
                        glass.pane_index_starts[face_id],
                        0,
                        0,
                        Some(&PushConstantInfo {
                            shader_stage: vk::ShaderStageFlags::VERTEX,
                            push_constants: bytemuck::bytes_of(&push).to_vec(),
                        }),
                    );
                }
            }
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), glass_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        }

        Self::with_gpu_scope(
            gpu_profiler.as_deref_mut(),
            gpu_profiler_frame_slot,
            cmdbuf,
            "graphics.renderpass.end",
            || render_target.record_end(cmdbuf),
        );
    }

    fn record_leaves_shadow_lod_pass(
        &self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
        leaf_color_tables: FloraHeightColorTables,
        time: f32,
    ) {
        self.graphics_pipelines
            .leaves_shadow_lod_ppl
            .record_texture_transitions(cmdbuf);
        self.graphics_pipelines
            .leaves_shadow_lod_ppl
            .record_bind(cmdbuf);

        let clear_values: [vk::ClearValue; 0] = [];

        self.render_target_leaf_shadow_opacity
            .record_begin(cmdbuf, &clear_values);

        let shadow_extent = self
            .resources
            .shadow
            .leaf_shadow_opacity_tex
            .get_image()
            .get_desc()
            .extent;
        let viewport = Viewport::from_extent(shadow_extent.as_extent_2d().unwrap());
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: shadow_extent.width,
                height: shadow_extent.height,
            },
        };

        self.graphics_pipelines
            .leaves_shadow_lod_ppl
            .record_viewport_scissor(cmdbuf, viewport, scissor);

        cmdbuf.bind_index_buffer_u32(&self.resources.meshes.leaves_resources_lod.indices);

        // loop through all tree leaves instances
        for tree_instance in surface_resources.instances.leaves_instances.values() {
            if tree_instance.resources.instances_len == 0 {
                continue;
            }
            let push_constant = flora_push_constant(
                time,
                LEAF_INSTANCE_TYPE,
                tree_instance.chunk_world_offset,
                leaf_color_tables,
            );

            cmdbuf.bind_vertex_buffers(0, &[&self.resources.meshes.leaves_resources_lod.vertices]);
            // render this instance for shadow map
            self.graphics_pipelines
                .leaves_shadow_lod_ppl
                .record_indexed_with_manual_buffer(
                    cmdbuf,
                    1,
                    0,
                    &tree_instance.resources.instances_buf,
                    self.resources.meshes.leaves_resources_lod.indices_len,
                    tree_instance.resources.instances_len,
                    0,
                    0,
                    0,
                    Some(&PushConstantInfo {
                        shader_stage: vk::ShaderStageFlags::VERTEX,
                        push_constants: bytemuck::bytes_of(&push_constant).to_vec(),
                    }),
                );
        }

        cmdbuf.bind_index_buffer_u32(&self.resources.meshes.apple_resources_lod.indices);
        for tree_instance in surface_resources.instances.apple_instances.values() {
            if tree_instance.resources.instances_len == 0 {
                continue;
            }
            let push_constant = flora_push_constant(
                time,
                APPLE_INSTANCE_TYPE,
                tree_instance.chunk_world_offset,
                solid_flora_height_color_tables(APPLE_BOTTOM_COLOR, APPLE_TIP_COLOR),
            );

            cmdbuf.bind_vertex_buffers(0, &[&self.resources.meshes.apple_resources_lod.vertices]);
            self.graphics_pipelines
                .leaves_shadow_lod_ppl
                .record_indexed_with_manual_buffer(
                    cmdbuf,
                    1,
                    0,
                    &tree_instance.resources.instances_buf,
                    self.resources.meshes.apple_resources_lod.indices_len,
                    tree_instance.resources.instances_len,
                    0,
                    0,
                    0,
                    Some(&PushConstantInfo {
                        shader_stage: vk::ShaderStageFlags::VERTEX,
                        push_constants: bytemuck::bytes_of(&push_constant).to_vec(),
                    }),
                );
        }

        self.render_target_leaf_shadow_opacity.record_end(cmdbuf);
    }

    fn record_dynamic_fruit_shadow_pass(&self, cmdbuf: &CommandBuffer) {
        let resources = &self.dynamic_fruit_resources;
        if resources.instance_count == 0 {
            return;
        }

        let pipeline = &self.graphics_pipelines.dynamic_fruit_shadow_ppl;
        pipeline.record_texture_transitions(cmdbuf);

        let clear_values: [vk::ClearValue; 0] = [];
        self.render_target_depth_only
            .record_begin(cmdbuf, &clear_values);

        let shadow_extent = self
            .resources
            .shadow
            .shadow_map_depth_tex
            .get_image()
            .get_desc()
            .extent;
        let viewport = Viewport::from_extent(shadow_extent.as_extent_2d().unwrap());
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: shadow_extent.width,
                height: shadow_extent.height,
            },
        };

        pipeline.record_bind(cmdbuf);
        pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
        cmdbuf.bind_index_buffer_u32(&resources.indices);
        cmdbuf.bind_vertex_buffers(0, &[&resources.vertices, &resources.instances]);
        pipeline.record_indexed(
            cmdbuf,
            resources.indices_len,
            resources.instance_count,
            0,
            0,
            0,
            None,
        );

        self.render_target_depth_only.record_end(cmdbuf);
    }

    fn record_tracer_shadow_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.tracer_shadow_ppl.record(
            cmdbuf,
            self.resources
                .shadow
                .shadow_map_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_environment_probe_global_copy_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines
            .environment_probe_global_copy_ppl
            .record(
                cmdbuf,
                Extent3D::new(self.environment_probes.status().grid.probe_count(), 1, 1),
                None,
            );
    }

    fn record_ddgi_global_sky_filter_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines
            .ddgi_global_sky_filter_ppl
            .as_ref()
            .expect("DDGI sky filter pipeline requires the DDGI backend")
            .record(
                cmdbuf,
                Extent3D::new(
                    DDGI_IRRADIANCE_INTERIOR_SIDE,
                    DDGI_IRRADIANCE_INTERIOR_SIDE,
                    1,
                ),
                None,
            );
    }

    fn record_ddgi_global_sky_gutter_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines
            .ddgi_octahedral_gutter_ppl
            .as_ref()
            .expect("DDGI gutter pipeline requires the DDGI backend")
            .record(
                cmdbuf,
                Extent3D::new(DDGI_IRRADIANCE_STORED_SIDE, DDGI_IRRADIANCE_STORED_SIDE, 1),
                None,
            );
    }

    fn record_ddgi_probe_relocation_pass(&self, cmdbuf: &CommandBuffer, terrain_revision: u32) {
        let volume = self
            .ddgi_volume
            .as_ref()
            .expect("DDGI relocation requires an allocated volume");
        let grid = volume.status().grid;
        let push_constants = DdgiProbeRelocationPushConstants {
            grid_dimensions: grid.dimensions().to_array(),
            spacing_voxels: grid.spacing_voxels(),
            voxels_per_world_unit: self.desc.voxel_dim_per_chunk.as_vec3().to_array(),
            terrain_revision,
        };
        self.compute_pipelines
            .ddgi_probe_relocate_ppl
            .as_ref()
            .expect("DDGI relocation pipeline requires the DDGI backend")
            .record(
                cmdbuf,
                Extent3D::new(grid.probe_count() * DDGI_RELOCATION_WORKGROUP_SIZE, 1, 1),
                Some(bytemuck::bytes_of(&push_constants)),
            );
    }

    fn record_ddgi_probe_trace_pass(&self, cmdbuf: &CommandBuffer, batch: DdgiRayBatch) {
        let far_distance_world = self.chunk_bound.dimensions().as_vec3().length() * 2.0;
        let push_constants = DdgiProbeTracePushConstants {
            first_probe_index: batch.first_probe_index,
            probe_count: batch.probe_count,
            terrain_revision: batch.terrain_revision,
            _padding: 0,
            far_distance_world,
            _padding_float: [0.0; 3],
        };
        self.compute_pipelines
            .ddgi_probe_trace_ppl
            .as_ref()
            .expect("DDGI trace pipeline requires the DDGI backend")
            .record(
                cmdbuf,
                Extent3D::new(batch.probe_count * DDGI_TRACE_WORKGROUP_SIZE, 1, 1),
                Some(bytemuck::bytes_of(&push_constants)),
            );
    }

    fn record_ddgi_irradiance_filter_pass(&self, cmdbuf: &CommandBuffer, batch: DdgiRayBatch) {
        let volume = self
            .ddgi_volume
            .as_ref()
            .expect("DDGI irradiance filtering requires an allocated volume");
        let push_constants = DdgiAtlasFilterPushConstants {
            first_probe_index: batch.first_probe_index,
            probe_count: batch.probe_count,
            tile_columns: volume.status().irradiance_layout.tile_grid().x,
            terrain_revision: batch.terrain_revision,
        };
        self.compute_pipelines
            .ddgi_irradiance_filter_ppl
            .as_ref()
            .expect("DDGI irradiance filter pipeline requires the DDGI backend")
            .record(
                cmdbuf,
                Extent3D::new(
                    batch.probe_count * DDGI_IRRADIANCE_INTERIOR_SIDE,
                    DDGI_IRRADIANCE_INTERIOR_SIDE,
                    1,
                ),
                Some(bytemuck::bytes_of(&push_constants)),
            );
    }

    fn record_ddgi_visibility_filter_pass(&self, cmdbuf: &CommandBuffer, batch: DdgiRayBatch) {
        let volume = self
            .ddgi_volume
            .as_ref()
            .expect("DDGI visibility filtering requires an allocated volume");
        let grid = volume.status().grid;
        let spacing_world =
            Vec3::splat(grid.spacing_voxels() as f32) / self.desc.voxel_dim_per_chunk.as_vec3();
        let far_distance_world = self.chunk_bound.dimensions().as_vec3().length() * 2.0;
        let push_constants = DdgiVisibilityFilterPushConstants {
            first_probe_index: batch.first_probe_index,
            probe_count: batch.probe_count,
            tile_columns: volume.status().visibility_layout.tile_grid().x,
            terrain_revision: batch.terrain_revision,
            spacing_world: spacing_world.to_array(),
            far_distance_world,
        };
        self.compute_pipelines
            .ddgi_visibility_filter_ppl
            .as_ref()
            .expect("DDGI visibility filter pipeline requires the DDGI backend")
            .record(
                cmdbuf,
                Extent3D::new(
                    batch.probe_count * DDGI_VISIBILITY_INTERIOR_SIDE,
                    DDGI_VISIBILITY_INTERIOR_SIDE,
                    1,
                ),
                Some(bytemuck::bytes_of(&push_constants)),
            );
    }

    fn record_ddgi_atlas_gutter_passes(&self, cmdbuf: &CommandBuffer, batch: DdgiRayBatch) {
        let volume = self
            .ddgi_volume
            .as_ref()
            .expect("DDGI gutter filtering requires an allocated volume");
        let irradiance_push = DdgiAtlasGutterPushConstants {
            first_probe_index: batch.first_probe_index,
            probe_count: batch.probe_count,
            tile_columns: volume.status().irradiance_layout.tile_grid().x,
            _padding: 0,
        };
        self.compute_pipelines
            .ddgi_irradiance_gutter_ppl
            .as_ref()
            .expect("DDGI irradiance gutter pipeline requires the DDGI backend")
            .record(
                cmdbuf,
                Extent3D::new(batch.probe_count * DDGI_GUTTER_WORKGROUP_SIZE, 1, 1),
                Some(bytemuck::bytes_of(&irradiance_push)),
            );

        let visibility_push = DdgiAtlasGutterPushConstants {
            tile_columns: volume.status().visibility_layout.tile_grid().x,
            ..irradiance_push
        };
        self.compute_pipelines
            .ddgi_visibility_gutter_ppl
            .as_ref()
            .expect("DDGI visibility gutter pipeline requires the DDGI backend")
            .record(
                cmdbuf,
                Extent3D::new(batch.probe_count * DDGI_GUTTER_WORKGROUP_SIZE, 1, 1),
                Some(bytemuck::bytes_of(&visibility_push)),
            );
    }

    fn record_environment_probe_classification_pass(&self, cmdbuf: &CommandBuffer) {
        let push_constants = EnvironmentProbeClassificationPushConstants {
            terrain_revision: self.environment_probe_terrain_revision,
            _padding: [0; 3],
        };
        self.compute_pipelines
            .environment_probe_classify_ppl
            .record(
                cmdbuf,
                Extent3D::new(self.environment_probes.status().grid.probe_count(), 1, 1),
                Some(bytemuck::bytes_of(&push_constants)),
            );
    }

    fn record_environment_probe_update_pass(
        &self,
        cmdbuf: &CommandBuffer,
        first_probe_index: u32,
        probe_count: u32,
        trace_visibility: bool,
        priority_region: Option<EnvironmentProbeTraceRegion>,
    ) {
        let (priority_grid_min, priority_side) = priority_region
            .map(|region| (region.grid_min.to_array(), region.side))
            .unwrap_or(([0; 3], 0));
        let push_constants = EnvironmentProbeUpdatePushConstants {
            first_probe_index,
            probe_count,
            environment_revision: self.environment_probe_environment_revision,
            trace_visibility: u32::from(trace_visibility),
            priority_grid_min,
            priority_side,
        };
        self.compute_pipelines.environment_probe_update_ppl.record(
            cmdbuf,
            Extent3D::new(probe_count, 1, 1),
            Some(bytemuck::bytes_of(&push_constants)),
        );
    }

    fn record_environment_probe_stats_pass(&self, cmdbuf: &CommandBuffer) {
        let probe_count = self.environment_probes.status().grid.probe_count();
        let push_constants = EnvironmentProbeStatsPushConstants {
            probe_count,
            _padding: [0; 3],
        };
        self.compute_pipelines.environment_probe_stats_ppl.record(
            cmdbuf,
            Extent3D::new(probe_count, 1, 1),
            Some(bytemuck::bytes_of(&push_constants)),
        );
    }

    fn record_shadow_depth_copy_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.shadow_depth_copy_ppl.record(
            cmdbuf,
            self.resources
                .shadow
                .shadow_map_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_leaf_shadow_temporal_pass(
        &mut self,
        cmdbuf: &CommandBuffer,
        temporal_alpha: f32,
        reset_leaf_shadow_history: bool,
    ) {
        let reset_history = reset_leaf_shadow_history || !self.leaf_shadow_history_valid;
        let push_constants = PushConstantLeafShadowTemporal {
            temporal_alpha: temporal_alpha.clamp(0.0, 1.0),
            reset_history: u32::from(reset_history),
            ..bytemuck::Zeroable::zeroed()
        };
        let push_constants_bytes = bytemuck::bytes_of(&push_constants);
        self.compute_pipelines.leaf_shadow_temporal_ppl.record(
            cmdbuf,
            self.resources
                .shadow
                .leaf_shadow_opacity_blended_tex
                .get_image()
                .get_desc()
                .extent,
            Some(push_constants_bytes),
        );
        self.leaf_shadow_history_valid = true;
    }

    fn record_leaf_shadow_mask_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.leaf_shadow_mask_ppl.record(
            cmdbuf,
            self.resources
                .shadow
                .leaf_shadow_mask_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_store_leaf_shadow_history(&self, cmdbuf: &CommandBuffer) {
        let history = self.resources.shadow.leaf_shadow_history();
        history.current().get_image().record_copy_to(
            cmdbuf,
            history.previous().get_image(),
            TextureLayout::GENERAL,
            TextureLayout::GENERAL,
        );
    }

    fn wind_volume_bucket_step_seconds(&self) -> f32 {
        self.world_tick_seconds
    }

    fn wind_volume_bucket_time(
        step_index: u32,
        bucket_index: u32,
        bucket_count: u32,
        step_seconds: f32,
    ) -> f32 {
        if step_index < bucket_index {
            return 0.0;
        }

        let last_bucket_step =
            bucket_index + ((step_index - bucket_index) / bucket_count) * bucket_count;
        last_bucket_step as f32 * step_seconds
    }

    fn record_wind_volume_bucket_pass(
        &self,
        cmdbuf: &CommandBuffer,
        dispatch_extent: Extent3D,
        bucket_index: u32,
        time: f32,
    ) {
        let push_constants = WindVolumePushConstants { time, bucket_index };

        self.compute_pipelines.wind_volume_ppl.record(
            cmdbuf,
            dispatch_extent,
            Some(bytemuck::bytes_of(&push_constants)),
        );
    }

    fn record_wind_volume_pass(&mut self, cmdbuf: &CommandBuffer, time: f32) {
        let bucket_count = WIND_VOLUME_BUCKET_COUNT;
        let step_seconds = self.wind_volume_bucket_step_seconds();
        let step_index = (time / step_seconds).floor().max(0.0) as u32;
        let mut dispatch_extent = self
            .resources
            .wind
            .wind_volume_tex
            .get_image()
            .get_desc()
            .extent;
        dispatch_extent.width /= WIND_VOLUME_BUCKET_COUNT;

        if self.initialized_wind_volume_bucket_count != bucket_count {
            for bucket_index in 0..bucket_count {
                let bucket_time = Self::wind_volume_bucket_time(
                    step_index,
                    bucket_index,
                    bucket_count,
                    step_seconds,
                );
                self.record_wind_volume_bucket_pass(
                    cmdbuf,
                    dispatch_extent,
                    bucket_index,
                    bucket_time,
                );
            }

            self.initialized_wind_volume_bucket_count = bucket_count;
            self.last_wind_volume_step = Some(step_index);
            return;
        }

        if self.last_wind_volume_step == Some(step_index) {
            return;
        }

        let bucket_index = step_index % bucket_count;
        let bucket_time = step_index as f32 * step_seconds;
        self.record_wind_volume_bucket_pass(cmdbuf, dispatch_extent, bucket_index, bucket_time);
        self.last_wind_volume_step = Some(step_index);
    }

    fn record_vsm_filtering_pass(
        &mut self,
        cmdbuf: &CommandBuffer,
        vsm_blur_radius: u32,
        vsm_temporal_alpha: f32,
        reset_vsm_history: bool,
    ) {
        let compute_to_compute_barrier = PipelineBarrier::compute_shader_access();

        let extent = self
            .resources
            .shadow
            .shadow_map_tex
            .get_image()
            .get_desc()
            .extent;
        self.compute_pipelines
            .vsm_creation_ppl
            .record(cmdbuf, extent, None);

        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        let reset_history = reset_vsm_history || !self.shadow_map_history_valid;
        let push_constants = VsmFilterPushConstants {
            blur_radius: vsm_blur_radius,
            temporal_alpha: vsm_temporal_alpha.clamp(0.0, 1.0),
            reset_history: u32::from(reset_history),
            _pad0: 0,
        };
        let push_constants_bytes = bytemuck::bytes_of(&push_constants);
        self.compute_pipelines
            .vsm_blur_h_ppl
            .record(cmdbuf, extent, Some(push_constants_bytes));

        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        self.compute_pipelines
            .vsm_blur_v_ppl
            .record(cmdbuf, extent, Some(push_constants_bytes));

        self.record_store_vsm_history(cmdbuf);
        self.shadow_map_history_valid = true;
    }

    fn record_tracer_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.tracer_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .compute_output_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn clear_tracer_outputs(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .compute_output_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
            );
        self.resources
            .extent_dependent_resources
            .compute_depth_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
            );
    }

    fn record_god_ray_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.god_ray_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .compute_depth_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_cloud_shadow_pass(&mut self, cmdbuf: &CommandBuffer) {
        let extent = self
            .resources
            .shadow
            .cloud_shadow_raw_tex
            .get_image()
            .get_desc()
            .extent;

        self.compute_pipelines
            .cloud_shadow_ppl
            .record(cmdbuf, extent, None);

        PipelineBarrier::compute_shader_access().record_insert(self.vulkan_ctx.device(), cmdbuf);

        let push_constants = CloudShadowTemporalPushConstants {
            reset_history: u32::from(!self.cloud_shadow_history_valid),
        };
        self.compute_pipelines.cloud_shadow_temporal_ppl.record(
            cmdbuf,
            extent,
            Some(bytemuck::bytes_of(&push_constants)),
        );
        self.cloud_shadow_history_valid = true;
    }

    fn record_cloud_pass(&mut self, cmdbuf: &CommandBuffer) {
        let extent = self
            .resources
            .extent_dependent_resources
            .cloud_raw_tex
            .get_image()
            .get_desc()
            .extent;

        self.compute_pipelines
            .cloud_ppl
            .record(cmdbuf, extent, None);

        PipelineBarrier::compute_shader_access().record_insert(self.vulkan_ctx.device(), cmdbuf);

        let push_constants = CloudTemporalPushConstants {
            reset_history: u32::from(!self.cloud_history_valid),
        };
        self.compute_pipelines.cloud_temporal_ppl.record(
            cmdbuf,
            extent,
            Some(bytemuck::bytes_of(&push_constants)),
        );
        self.cloud_history_valid = true;
    }

    fn clear_cloud_output(&mut self, cmdbuf: &CommandBuffer) {
        self.cloud_history_valid = false;
        self.cloud_shadow_history_valid = false;
        for tex in [
            &self.resources.extent_dependent_resources.cloud_raw_tex,
            &self.resources.extent_dependent_resources.cloud_output_tex,
        ] {
            tex.get_image().record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
            );
        }
    }

    fn record_composition_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.composition_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .composited_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_lens_flare_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.lens_flare_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .lens_flare_full_output_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_lens_flare_sun_visible_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.lens_flare_sun_visible_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .lens_flare_full_output_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_lens_flare_downsample_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.lens_flare_downsample_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .lens_flare_output_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_post_processing_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.post_processing_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .screen_output_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    #[allow(dead_code)]
    fn record_player_collider_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines
            .player_collider_ppl
            .record(cmdbuf, Extent3D::new(1, 1, 1), None);
    }

    pub fn handle_keyboard(&mut self, key_event: &KeyEvent) {
        self.camera.handle_keyboard(key_event);
    }

    pub fn reset_camera_input(&mut self) {
        self.camera.reset_input();
    }

    pub fn handle_mouse(&mut self, delta: Vec2) {
        self.camera.handle_mouse(delta);
    }

    #[allow(dead_code)]
    pub fn reset_camera_velocity(&mut self) {
        self.camera.reset_velocity();
    }

    pub fn camera_position(&self) -> Vec3 {
        self.camera.position()
    }

    pub fn camera_pose(&self) -> CameraPose {
        self.camera.pose()
    }

    pub fn apply_camera_pose(&mut self, pose: CameraPose) {
        self.camera.apply_pose(pose);
        let view_mat = self.camera.get_view_mat();
        let proj_mat = self.camera.get_proj_mat();
        self.camera_view_mat_prev_frame = view_mat;
        self.camera_proj_mat_prev_frame = proj_mat;
        self.current_view_proj_mat = proj_mat * view_mat;
        self.shadow_map_history_valid = false;
        self.cloud_history_valid = false;
        if let Err(err) = self
            .spatial_sound_manager
            .update_player_pos(self.camera.position(), self.camera.vectors())
        {
            log::warn!(
                "Failed to update listener after applying camera pose: {}",
                err
            );
        }
    }

    pub fn camera_front(&self) -> Vec3 {
        self.camera.front()
    }

    pub fn camera_ray_from_screen_position(
        &self,
        screen_pos_physical: Vec2,
        screen_extent: Extent2D,
    ) -> Option<(Vec3, Vec3)> {
        self.camera
            .ray_from_screen_position(screen_pos_physical, screen_extent)
    }

    pub fn set_camera_pose_looking_at(&mut self, position: Vec3, target: Vec3) -> bool {
        let changed = self.camera.set_pose_looking_at(position, target);
        if changed {
            if let Err(err) = self
                .spatial_sound_manager
                .update_player_pos(self.camera.position(), self.camera.vectors())
            {
                log::warn!(
                    "Failed to update listener after applying look-at camera pose: {}",
                    err
                );
            }
        }
        changed
    }

    pub fn project_screen_point_to_world(
        &self,
        screen_pos: Vec2,
        screen_extent: Vec2,
        distance_from_camera: f32,
    ) -> Option<Vec3> {
        if screen_extent.x <= 0.0 || screen_extent.y <= 0.0 {
            return None;
        }

        let ndc = Vec3::new(
            (screen_pos.x / screen_extent.x) * 2.0 - 1.0,
            (screen_pos.y / screen_extent.y) * 2.0 - 1.0,
            0.0,
        );
        let clip = ndc.extend(1.0);
        let mut world = self.current_view_proj_mat.inverse() * clip;
        if world.w.abs() <= 1e-6 {
            return None;
        }
        world /= world.w;

        let camera_pos = self.camera.position();
        let world_pos = Vec3::new(world.x, world.y, world.z);
        let direction = (world_pos - camera_pos).normalize_or_zero();
        if direction.length_squared() <= 1e-6 {
            return None;
        }

        Some(camera_pos + direction * distance_from_camera)
    }

    #[allow(dead_code)]
    pub fn camera_vectors(&self) -> &CameraVectors {
        self.camera.vectors()
    }

    pub fn set_footstep_volume_gain(&mut self, volume_gain: f32) {
        self.camera.set_footstep_volume_gain(volume_gain);
    }

    pub fn update_fly_camera(&mut self, frame_delta_time: f32) {
        self.camera.update_transform_fly_mode(frame_delta_time);
        self.spatial_sound_manager
            .update_player_pos(self.camera.position(), self.camera.vectors())
            .unwrap();
    }

    pub fn prepare_walk_camera_movement(
        &mut self,
        frame_delta_time: f32,
    ) -> crate::gameplay::camera::PlayerWalkMovementRequest {
        self.camera.prepare_walk_movement(frame_delta_time)
    }

    pub fn apply_walk_camera_movement(
        &mut self,
        frame_delta_time: f32,
        request: crate::gameplay::camera::PlayerWalkMovementRequest,
        result: crate::gameplay::camera::PlayerWalkMovementResult,
    ) {
        self.camera
            .apply_walk_movement(frame_delta_time, request, result);
        self.spatial_sound_manager
            .update_player_pos(self.camera.position(), self.camera.vectors())
            .unwrap();
    }

    pub fn upload_sprinklers(&mut self, instances: &[SprinklerRenderInstance]) -> Result<()> {
        self.sprinkler_resources.upload(instances)
    }

    pub fn upload_irrigation_pipes(&mut self, data: &IrrigationPipeRenderData) -> Result<()> {
        self.irrigation_pipe_resources.upload(data)
    }

    pub fn upload_irrigation_pipe_preview(
        &mut self,
        data: &IrrigationPipeRenderData,
    ) -> Result<()> {
        let mesh = build_pipe_preview_mesh(data)?;
        self.geometry_preview_resources.pipe.upload(&mesh)?;
        self.geometry_preview_resources
            .pipe
            .show(Vec3::ZERO, Vec4::ONE)
    }

    pub fn clear_irrigation_pipe_preview(&mut self) {
        self.geometry_preview_resources.pipe.clear();
    }

    pub fn upload_debug_geometry_preview(
        &mut self,
        mesh: &GeometryPreviewMesh,
        base_position: Vec3,
        tint: Vec4,
    ) -> Result<()> {
        self.geometry_preview_resources.pipe.upload(mesh)?;
        self.geometry_preview_resources
            .pipe
            .show(base_position, tint)
    }

    pub fn upload_tree_geometry_preview(&mut self, mesh: &GeometryPreviewMesh) -> Result<()> {
        self.geometry_preview_resources.tree.upload(mesh)
    }

    pub fn show_tree_geometry_preview(&mut self, base_position: Vec3, tint: Vec4) -> Result<()> {
        self.geometry_preview_resources
            .tree
            .show(base_position, tint)
    }

    pub fn clear_tree_geometry_preview(&mut self) {
        self.geometry_preview_resources.tree.clear();
    }

    pub fn upload_collision_probe_geometry(&mut self) -> Result<()> {
        // The formal dynamic-fruit mesh is immutable and uploaded once with Tracer resources.
        debug_assert!(self.dynamic_fruit_resources.indices_len > 0);
        Ok(())
    }

    pub fn show_dynamic_fruit_geometry(
        &mut self,
        instances: &[DynamicFruitRenderInstance],
    ) -> Result<()> {
        self.dynamic_fruit_resources.show(instances)
    }

    pub fn clear_collision_probe_geometry(&mut self) {
        self.dynamic_fruit_resources.clear();
    }

    pub fn upload_particles(&mut self, snapshots: &[ParticleSnapshot]) -> Result<()> {
        let capacity = PARTICLE_CAPACITY;
        let count = snapshots.len().min(capacity);
        let texture_layout = ParticleTextureLayout::new();
        texture_layout.assert_valid();
        let texture_layer_count = self
            .resources
            .textures
            .particle_lod_tex_lut
            .get_image()
            .get_desc()
            .array_len;
        debug_assert_eq!(
            texture_layer_count,
            texture_layout.total_layer_count(),
            "Particle texture LUT layer count mismatch (runtime {}, expected {})",
            texture_layer_count,
            texture_layout.total_layer_count()
        );
        let butterfly_frame_count = texture_layout.butterfly_frames_per_view();
        let camera_right_xz =
            Vec2::new(self.camera.vectors().right.x, self.camera.vectors().right.z)
                .normalize_or_zero();
        let camera_forward_xz =
            Vec2::new(self.camera.vectors().front.x, self.camera.vectors().front.z)
                .normalize_or_zero();
        const SPRITE_FLIP_BIT: u32 = 1 << 31;
        const MIN_SPEED_SQ: f32 = 0.01 * 0.01;

        let is_moving_right_relative_to_player = |velocity: Vec3| -> bool {
            let velocity_xz = Vec2::new(velocity.x, velocity.z);
            if velocity_xz.length_squared() <= MIN_SPEED_SQ {
                return false;
            }
            if camera_right_xz.length_squared() <= f32::EPSILON {
                return false;
            }

            velocity_xz.normalize().dot(camera_right_xz) > 0.0
        };
        let pack_particle_tex_index = |tex_index: u32, flip_sprite_x: bool| -> u32 {
            let base = tex_index & !SPRITE_FLIP_BIT;
            if flip_sprite_x {
                base | SPRITE_FLIP_BIT
            } else {
                base
            }
        };

        self.particle_instance_scratch.clear();
        self.particle_instance_scratch.reserve(count);
        self.translucent_particle_instance_scratch.clear();
        self.translucent_particle_instance_scratch.reserve(count);
        for snap in snapshots.iter().take(capacity) {
            let butterfly_tex_index = {
                let vel_xz = Vec2::new(snap.velocity.x, snap.velocity.z);
                let vel_dir_xz = if vel_xz.length_squared() > MIN_SPEED_SQ {
                    vel_xz.normalize()
                } else {
                    Vec2::ZERO
                };
                let view_index = if vel_dir_xz == Vec2::ZERO {
                    0
                } else {
                    let z = vel_dir_xz.dot(camera_forward_xz);

                    if z >= 0.0 {
                        0
                    } else {
                        1
                    }
                };

                let palette_preset = ButterflyPalettePreset::from_index(snap.texture_variant);
                let preset_base_layer =
                    texture_layout.butterfly_preset_base_layer(palette_preset as u32);
                let frame_offset = snap.animation_frame_offset % butterfly_frame_count.max(1);
                let tex_index =
                    preset_base_layer + view_index * butterfly_frame_count + frame_offset;
                debug_assert!(
                    texture_layout.contains_layer(tex_index),
                    "Butterfly texture index {} out of LUT bounds {}",
                    tex_index,
                    texture_layout.total_layer_count()
                );
                tex_index
            };
            let instance = ParticleInstanceGpu {
                position: snap.position_ws.to_array(),
                size: snap.size,
                color: snap.color.to_array(),
                tex_index: match snap.kind {
                    crate::particles::ParticleRenderKind::Leaf => texture_layout.leaf_layer(),
                    crate::particles::ParticleRenderKind::Butterfly => pack_particle_tex_index(
                        butterfly_tex_index,
                        is_moving_right_relative_to_player(snap.velocity),
                    ),
                    crate::particles::ParticleRenderKind::WaterDroplet => {
                        texture_layout.leaf_layer()
                    }
                },
            };
            match snap.kind {
                crate::particles::ParticleRenderKind::WaterDroplet => {
                    self.translucent_particle_instance_scratch.push(instance)
                }
                crate::particles::ParticleRenderKind::Leaf
                | crate::particles::ParticleRenderKind::Butterfly => {
                    self.particle_instance_scratch.push(instance)
                }
            }
        }

        let camera_position = self.camera.position();
        self.translucent_particle_instance_scratch
            .sort_unstable_by(|a, b| {
                let distance_sq = |instance: &ParticleInstanceGpu| {
                    Vec3::from_array(instance.position).distance_squared(camera_position)
                };
                distance_sq(b).total_cmp(&distance_sq(a))
            });

        if !self.particle_instance_scratch.is_empty() {
            self.particle_resources
                .instance_buffer
                .fill(&self.particle_instance_scratch)?;
        }
        if !self.translucent_particle_instance_scratch.is_empty() {
            self.particle_resources
                .translucent_instance_buffer
                .fill(&self.translucent_particle_instance_scratch)?;
        }
        self.particle_resources.instance_count = self.particle_instance_scratch.len() as u32;
        self.particle_resources.translucent_instance_count =
            self.translucent_particle_instance_scratch.len() as u32;
        Ok(())
    }

    fn pack_tree_leaf_voxel_local_pos(local_pos: IVec3) -> Result<u32> {
        const PACKED_MIN: i32 = -512;
        const PACKED_MAX: i32 = 511;
        anyhow::ensure!(
            (PACKED_MIN..=PACKED_MAX).contains(&local_pos.x)
                && (PACKED_MIN..=PACKED_MAX).contains(&local_pos.y)
                && (PACKED_MIN..=PACKED_MAX).contains(&local_pos.z),
            "tree leaf voxel local position {:?} exceeds packed signed 10-bit range",
            local_pos,
        );
        let encode = |value: i32| -> u32 { ((value - PACKED_MIN) as u32) & 0x3ff };
        Ok(encode(local_pos.x) | (encode(local_pos.y) << 10) | (encode(local_pos.z) << 20))
    }

    fn build_tree_render_instances(
        &self,
        tree_id: u32,
        instances: &[TreeRenderInstanceData],
        aabb_margin: f32,
        span_label: &str,
    ) -> Result<TreeLeavesInstance> {
        use crate::builder::TreeLeafInstance;

        let mut instances_data = Vec::with_capacity(instances.len());
        let chunk_world_offset = instances
            .iter()
            .map(|instance| instance.world_pos)
            .reduce(UVec3::min)
            .unwrap_or(UVec3::ZERO);
        if let Some(max_pos) = instances
            .iter()
            .map(|instance| instance.world_pos)
            .reduce(UVec3::max)
        {
            anyhow::ensure!(
                (max_pos - chunk_world_offset)
                    .cmplt(UVec3::splat(1024))
                    .all(),
                "{} instance span exceeds packed 10-bit local-position range",
                span_label,
            );
        }
        let pack_local_pos = |world_pos: UVec3| -> u32 {
            let local_pos = world_pos - chunk_world_offset;
            (local_pos.x & 0x3ff) | ((local_pos.y & 0x3ff) << 10) | ((local_pos.z & 0x3ff) << 20)
        };
        for instance in instances.iter() {
            instances_data.push(TreeLeafInstance {
                packed_local_pos: pack_local_pos(instance.world_pos),
                packed_leaf_local_pos: Self::pack_tree_leaf_voxel_local_pos(
                    instance.leaf_local_pos,
                )?,
            });
        }

        let scaled_positions = instances
            .iter()
            .map(|instance| {
                let pos = instance.world_pos;
                Vec3::new(
                    pos.x as f32 / 256.0,
                    pos.y as f32 / 256.0,
                    pos.z as f32 / 256.0,
                )
            })
            .collect::<Vec<_>>();
        let aabb =
            crate::builder::InstanceResources::compute_leaves_aabb(&scaled_positions, aabb_margin);

        let mut tree_instance = TreeLeavesInstance::new_with_capacity(
            tree_id,
            aabb,
            chunk_world_offset,
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            instances.len() as u64,
        );

        if !instances_data.is_empty() {
            tree_instance
                .resources
                .instances_buf
                .fill(&instances_data)?;
            tree_instance.resources.instances_len = instances_data.len() as u32;
        } else {
            tree_instance.resources.instances_len = 0;
        }

        Ok(tree_instance)
    }

    pub fn add_tree_leaves(
        &mut self,
        surface_resources: &mut SurfaceResources,
        tree_id: u32,
        leaf_positions: &[UVec3],
        leaf_local_positions: &[IVec3],
    ) -> Result<()> {
        anyhow::ensure!(
            leaf_positions.len() == leaf_local_positions.len(),
            "tree leaf position and anchor metadata lengths differ"
        );
        let leaf_voxel_instances = leaf_positions
            .iter()
            .copied()
            .zip(leaf_local_positions.iter().copied())
            .map(|(world_pos, leaf_local_pos)| TreeRenderInstanceData {
                world_pos,
                leaf_local_pos,
            })
            .collect::<Vec<_>>();
        let tree_leaves_instance =
            self.build_tree_render_instances(tree_id, &leaf_voxel_instances, 0.2, "tree leaf")?;
        surface_resources
            .instances
            .leaves_instances
            .insert(tree_id, tree_leaves_instance);

        Ok(())
    }

    pub fn add_tree_apples(
        &mut self,
        surface_resources: &mut SurfaceResources,
        tree_id: u32,
        apples: &[(UVec3, u32)],
    ) -> Result<()> {
        // This path is also used when a pending fruit atomically transitions to the dynamic
        // renderer. Wait before replacing the per-tree instance buffer still referenced by an
        // earlier frame.
        self.vulkan_ctx.device().wait_idle();
        let apple_instances = apples
            .iter()
            .map(|&(world_pos, radius_voxels)| TreeRenderInstanceData {
                world_pos,
                // Apples do not use the leaf-local offset. Reuse its signed 10-bit x field for
                // the discrete voxel radius consumed only by the apple shader path.
                leaf_local_pos: IVec3::new(
                    radius_voxels.clamp(1, TREE_FRUIT_MAX_RADIUS_VOXELS) as i32,
                    0,
                    0,
                ),
            })
            .collect::<Vec<_>>();
        let tree_apple_instance =
            self.build_tree_render_instances(tree_id, &apple_instances, 0.08, "tree apple")?;
        surface_resources
            .instances
            .apple_instances
            .insert(tree_id, tree_apple_instance);

        Ok(())
    }

    pub fn remove_tree_leaves(
        &mut self,
        surface_resources: &mut SurfaceResources,
        tree_id: u32,
    ) -> Result<()> {
        self.vulkan_ctx.device().wait_idle();
        let removed_leaves = surface_resources
            .instances
            .leaves_instances
            .remove(&tree_id);
        let removed_apples = surface_resources.instances.apple_instances.remove(&tree_id);

        match (removed_leaves, removed_apples) {
            (Some(leaves), Some(apples)) => log::info!(
                "Removed tree {} with {} leaves and {} apples",
                tree_id,
                leaves.resources.instances_len,
                apples.resources.instances_len
            ),
            (Some(leaves), None) => log::info!(
                "Removed tree {} with {} leaves",
                tree_id,
                leaves.resources.instances_len
            ),
            (None, Some(apples)) => log::info!(
                "Removed tree {} with {} apples",
                tree_id,
                apples.resources.instances_len
            ),
            (None, None) => log::warn!("Attempted to remove non-existent tree {}", tree_id),
        }
        Ok(())
    }

    pub fn query_terrain_rays_batch_with_validity(
        &mut self,
        rays: &[TerrainRayQuery],
    ) -> Result<Vec<TerrainRayHitSample>> {
        if rays.is_empty() {
            return Ok(vec![]);
        }

        let mut all_hits = Vec::with_capacity(rays.len());
        for chunk in rays.chunks(MAX_TERRAIN_QUERIES) {
            all_hits.extend(self.query_terrain_rays_chunk_with_validity(chunk)?);
        }
        Ok(all_hits)
    }

    fn query_terrain_rays_chunk_with_validity(
        &mut self,
        rays: &[TerrainRayQuery],
    ) -> Result<Vec<TerrainRayHitSample>> {
        debug_assert!(!rays.is_empty());
        debug_assert!(rays.len() <= MAX_TERRAIN_QUERIES);

        let query_count = rays.len() as u32;

        self.resources
            .terrain_query
            .terrain_query_count
            .fill_uniform(&crate::generated::gpu_structs::TerrainQueryCount {
                valid_query_count: query_count,
                ..bytemuck::Zeroable::zeroed()
            })?;

        let mut ray_data = Vec::with_capacity(rays.len() * 8);
        for ray in rays {
            ray_data.push(ray.origin.x);
            ray_data.push(ray.origin.y);
            ray_data.push(ray.origin.z);
            ray_data.push(0.0);
            ray_data.push(ray.direction.x);
            ray_data.push(ray.direction.y);
            ray_data.push(ray.direction.z);
            ray_data.push(0.0);
        }
        self.resources
            .terrain_query
            .terrain_query_info
            .fill(&ray_data)?;

        execute_one_time_gpu_job(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.compute_pipelines.terrain_query_ppl.record(
                    cmdbuf,
                    Extent3D::new(query_count, 1, 1),
                    None,
                );
            },
        );

        let raw_data = self
            .resources
            .terrain_query
            .terrain_query_result
            .read_back()
            .unwrap();
        let hit_data: &[f32] = unsafe {
            std::slice::from_raw_parts(raw_data.as_ptr() as *const f32, (query_count as usize) * 4)
        };

        let mut hits = Vec::with_capacity(query_count as usize);
        for item in hit_data.chunks_exact(4) {
            hits.push(TerrainRayHitSample {
                position: Vec3::new(item[0], item[1], item[2]),
                is_valid: item[3] >= 0.5,
            });
        }
        Ok(hits)
    }

    pub fn query_terrain_ray_with_validity(
        &mut self,
        ray: TerrainRayQuery,
    ) -> Result<TerrainRayHitSample> {
        let samples = self.query_terrain_rays_batch_with_validity(&[ray])?;
        Ok(samples[0])
    }
}
