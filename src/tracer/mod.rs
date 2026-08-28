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

mod flora_lighting_cache;
use flora_lighting_cache::FloraLightingCache;

mod flora_frame_plan;
use flora_frame_plan::{
    FloraFramePlan, FloraFramePlanConfig, TreeFoliageBatch, TreeFoliageFramePlan, TreeFoliageInput,
    TreeFoliageKind, TreeFoliageMainConfig,
};

mod leaf_shadow_proxy;
use leaf_shadow_proxy::build_leaf_shadow_proxies;

mod direct_sun_shadow_runtime;
pub use direct_sun_shadow_runtime::DIRECT_SUN_SHADOW_SOURCE_ALL;
use direct_sun_shadow_runtime::{DirectSunShadowLightSpaceChange, DirectSunShadowRuntime};

mod local_light_visibility_diagnostic;
use local_light_visibility_diagnostic::{
    LocalLightVisibilityDiagnostic, LocalLightVisibilityDiagnosticEvidence,
};

pub mod tree_preview_mesh;

mod extent_dependent_resources;
pub use extent_dependent_resources::*;

mod vertex;
pub use vertex::*;

pub mod voxel_encoding;

mod voxel_geometry;

mod leaves_construct;
pub use leaves_construct::{voxel_apple_offsets, TREE_FRUIT_MAX_RADIUS_VOXELS};

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
pub(crate) const ENVIRONMENT_IRRADIANCE_CAPTURE_PLANE_COUNT: u32 = 5;
pub const FLORA_HEIGHT_COLOR_TABLE_LEN: usize = 12;
pub type FloraHeightColorTables = [[u32; FLORA_HEIGHT_COLOR_TABLE_LEN]; 2];
const FLORA_LIGHTING_CACHE_OFFSET_BITS: u32 = 22;
const FLORA_LIGHTING_CACHE_OFFSET_MASK: u32 = (1 << FLORA_LIGHTING_CACHE_OFFSET_BITS) - 1;
const FLORA_LIGHTING_CACHE_VOXEL_COUNT_MAX: u32 = 0x1ff;
const FLORA_LIGHTING_CACHE_LOD_BIT: u32 = 1 << 31;
const FLORA_INSTANCE_TYPE_MASK: u32 = 0xff;
const FLORA_LIGHTING_CACHE_INSTANCE_COUNT_SHIFT: u32 = 8;

use crate::audio::SpatialSoundManager;

use crate::builder::{
    ContreeBuilderResources, FloraInstanceResources, PlainBuilderResources,
    SceneAccelBuilderResources, SurfaceResources, TreeLeavesInstance,
};
use crate::ddgi::{
    DdgiBatchOrder, DdgiBuildKind, DdgiBuildToken, DdgiCaptureCheckpoint, DdgiCapturePublication,
    DdgiCaptureTarget, DdgiDebugView, DdgiFieldIdentity, DdgiLocalLightTraceTotals, DdgiRayBatch,
    DdgiRuntime, DdgiRuntimeStatus, DdgiRuntimeVolumeTarget, DdgiScheduledWorkKind, DdgiTraceStats,
    DdgiValidatedIterationOutcome, DdgiVerifiedBatchOutcome, DdgiVolume, DdgiVolumes,
    DdgiVoxelVisibility, DDGI_CONVERGENCE_POLICY, DDGI_GUTTER_WORKGROUP_SIZE,
    DDGI_IRRADIANCE_INTERIOR_SIDE, DDGI_IRRADIANCE_STORED_SIDE, DDGI_RELOCATION_WORKGROUP_SIZE,
    DDGI_TRACE_WORKGROUP_SIZE, DDGI_VISIBILITY_INTERIOR_SIDE,
};
use crate::environment_lighting::{
    DdgiRadianceSnapshot, DdgiVoxelPaletteSnapshot, EnvironmentLightingCache,
};
use crate::environment_probes::{
    EnvironmentProbeVisualizationPushConstants, EnvironmentProbeVisualizationResources,
    EnvironmentProbeVisualizationSettings,
};
use crate::gameplay::{
    calculate_directional_light_matrices, Camera, CameraDesc, CameraPose, CameraVectors,
};
use crate::generated::gpu_structs::{
    PushConstantFlora, PushConstantGlassResolve, PushConstantLeafShadowTemporal,
};
use crate::geom::UAabb3;
use crate::lighting::{
    LightId, LocalLightBudget, LocalLightGpuSnapshot, LocalLightInfluenceBound, LocalLightOverflow,
    LocalLightOverflowReason, LocalLightSnapshot, ProviderId, EMISSIVE_VOXEL_COLOR_SRGB,
    EMISSIVE_VOXEL_SURFACE_RADIANCE, LOCAL_LIGHT_FLAG_DDGI_TRACE_DIAGNOSTICS,
    LOCAL_LIGHT_GPU_CAPACITY,
};
use crate::particles::{ParticleSnapshot, PARTICLE_CAPACITY};
use crate::resource::ResourceContainer;
use crate::util::TimeInfo;
use crate::wind::WindSource;
use anyhow::{Context, Result};
use re_flora_vkn::vk;
use re_flora_vkn::{
    execute_one_time_gpu_job, Allocator, AttachmentDescOuter, AttachmentType, Buffer, BufferUse,
    ClearValue, ColorClearValue, CommandBuffer, ComputePipeline, DepthOrStencilClearValue,
    DescriptorPool, DescriptorResource, DescriptorUpdate, DescriptorWrite, Extent2D, Extent3D,
    FrameExtentGeneration, FrameRetirement, FrameRetirementSink, Framebuffer, GpuProfiler,
    GraphicsPipeline, PipelineBarrier, PipelineStage, PreparedDescriptorGeneration,
    PreparedDrawDescriptors, PushConstantInfo, RenderPass, RenderTarget, Texture, TextureLayout,
    TextureRegion, Viewport, VulkanContext,
};
use std::time::Instant;

fn require_ddgi_staging_preparation(build_token: DdgiBuildToken, result: Result<()>) {
    result.unwrap_or_else(|err| {
        panic!(
            "DDGI staging preparation failed token_serial={} kind={:?} terrain_revision={} spacing_voxels={}: {err:#}",
            build_token.serial(),
            build_token.kind(),
            build_token.terrain_revision(),
            build_token.spacing_voxels(),
        )
    });
}

fn local_light_impact_voxel_bound(
    bound: LocalLightInfluenceBound,
    voxels_per_world_unit: UVec3,
    world_extent_voxels: UVec3,
) -> UAabb3 {
    let scale = voxels_per_world_unit.as_vec3();
    let extent = world_extent_voxels.as_vec3();
    let min = (bound.min() * scale).floor().clamp(Vec3::ZERO, extent);
    let max = (bound.max() * scale).ceil().clamp(Vec3::ZERO, extent);
    UAabb3::new(min.as_uvec3(), max.as_uvec3())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DdgiLocalLightGpuEvidence {
    pub field: DdgiFieldIdentity,
    pub local_source_revision: u64,
    pub local_light_count: u32,
    pub sampled_probe_count: u32,
    pub expected_probe_count: u32,
    pub totals: DdgiLocalLightTraceTotals,
    pub emissive_surface_hits: u64,
    pub emissive_surface_radiance_luma_q8: u64,
}

impl DdgiLocalLightGpuEvidence {
    fn new(
        field: DdgiFieldIdentity,
        local_source_revision: u64,
        local_light_count: u32,
        expected_probe_count: u32,
    ) -> Self {
        Self {
            field,
            local_source_revision,
            local_light_count,
            sampled_probe_count: 0,
            expected_probe_count,
            totals: DdgiLocalLightTraceTotals::default(),
            emissive_surface_hits: 0,
            emissive_surface_radiance_luma_q8: 0,
        }
    }

    fn accumulate(&mut self, batch: DdgiRayBatch, stats: DdgiTraceStats) {
        assert_eq!(self.field, batch.logical());
        self.sampled_probe_count += batch.probe_count;
        assert!(self.sampled_probe_count <= self.expected_probe_count);
        self.totals.accumulate(stats);
        self.emissive_surface_hits += u64::from(stats.emissive_surface_hits);
        self.emissive_surface_radiance_luma_q8 +=
            u64::from(stats.emissive_surface_radiance_luma_q8);
    }

    pub fn is_complete(self) -> bool {
        self.sampled_probe_count == self.expected_probe_count
    }

    pub fn matches_classified_field(self, classified: DdgiFieldIdentity) -> bool {
        let sampled = self.field.field();
        let classified_source = classified.source();
        let classified_key = classified.field();
        sampled.serial() == classified_key.serial()
            && sampled.geometry_revision() == classified_key.geometry_revision()
            && sampled.radiance_revision() == classified_key.radiance_revision()
            && sampled.spacing_voxels() == classified_key.spacing_voxels()
            && sampled.update_epoch() == classified_key.update_epoch()
            && self.field.source() == classified_source
    }
}

const MAX_TERRAIN_QUERIES: usize = 1_000;
const SHADOW_MAP_RESOLUTION: u32 = 1024;
const CLOUD_SHADOW_MAP_RESOLUTION: u32 = 256;
const LEAF_SHADOW_OPACITY_RESOLUTION: u32 = 2048;
const DDGI_ATLAS_REDUCTION_WORKGROUP_SIZE: u32 = 64;
pub(super) const WIND_VOLUME_BUCKET_COUNT: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WindVolumePushConstants {
    time: f32,
    bucket_index: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DdgiProbeRelocationPushConstants {
    grid_dimensions: [u32; 3],
    spacing_voxels: u32,
    voxels_per_world_unit: [f32; 3],
    terrain_revision: u32,
    glass_experiment_enabled: u32,
    _padding: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DdgiProbeTracePushConstants {
    first_probe_index: u32,
    probe_count: u32,
    terrain_revision: u32,
    has_history: u32,
    source_slot: u32,
    far_distance_world: f32,
    _padding: [u32; 2],
    epoch_rotation: [f32; 4],
    local_refresh_enabled: [u32; 4],
    local_refresh_world_min: [f32; 4],
    local_refresh_world_max: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DdgiAtlasFilterPushConstants {
    first_probe_index: u32,
    probe_count: u32,
    tile_columns: u32,
    terrain_revision: u32,
    destination_is_transport_source: u32,
    source_slot: u32,
    has_history: u32,
    history_retention: f32,
    epoch_rotation: [f32; 4],
    local_refresh_enabled: [u32; 4],
    local_refresh_world_min: [f32; 4],
    local_refresh_world_max: [f32; 4],
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
    destination_slot: u32,
    source_slot: u32,
    has_history: u32,
    history_retention: f32,
    epoch_rotation: [f32; 4],
    local_refresh_enabled: [u32; 4],
    local_refresh_world_min: [f32; 4],
    local_refresh_world_max: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DdgiAtlasGutterPushConstants {
    first_probe_index: u32,
    probe_count: u32,
    tile_columns: u32,
    destination_is_transport_source: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DdgiAtlasReductionPushConstants {
    terrain_revision: u32,
    probe_count: u32,
    tile_columns: u32,
    destination_slot: u32,
    source_slot: u32,
    has_source: u32,
    relative_floor: f32,
    _padding: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlassPushConstants {
    box_min_near_alpha: [f32; 4],
    box_max_far_alpha: [f32; 4],
}

const GLASS_VOXEL_CACHE_BUILD_PASS: u32 = 0;
const GLASS_VOXEL_CACHE_SHADE_PASS: u32 = 1;
const GLASS_VOXEL_CACHE_RESOLVE_PASS: u32 = 2;

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

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EffectTemporalPushConstants {
    reset_history: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GodRayTemporalPushConstants {
    reset_history: u32,
    temporal_blend_enabled: u32,
    temporal_alpha: f32,
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum GlassSceneQueryEventKind {
    Interface,
    Opaque,
    Miss,
    StepBudget,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GlassSceneQueryCaptureEvent {
    pub(crate) kind: GlassSceneQueryEventKind,
    pub(crate) position: Vec3,
    pub(crate) from_voxel_type: u32,
    pub(crate) to_voxel_type: u32,
    pub(crate) tied_axes: u8,
    pub(crate) dda_steps: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GlassStraightTransportCapture {
    pub(crate) terminal: GlassSceneQueryEventKind,
    pub(crate) transmittance: Vec3,
    pub(crate) interfaces: u32,
    pub(crate) scene_queries: u32,
    pub(crate) dda_steps: u32,
    pub(crate) exhausted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DdgiSemanticOccupancyCapture {
    pub(crate) occupied: bool,
    pub(crate) geometry_revision: u32,
    pub(crate) glass_material_revision: u32,
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

#[cfg(test)]
mod ddgi_density_rebuild_tests {
    use super::require_ddgi_staging_preparation;
    use crate::ddgi::{DdgiBuildKind, DdgiBuildToken};

    #[test]
    #[should_panic(
        expected = "DDGI staging preparation failed token_serial=9 kind=Terrain terrain_revision=8 spacing_voxels=16: injected allocation failure"
    )]
    fn staging_preparation_error_fails_fast_with_claimed_identity() {
        let token = DdgiBuildToken::for_test(9, 8, 16, DdgiBuildKind::Terrain);
        require_ddgi_staging_preparation(
            token,
            Err(anyhow::anyhow!("injected allocation failure")),
        );
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

fn flora_lighting_cache_location(cache_offset: u32, voxel_count: u32, is_lod: bool) -> u32 {
    assert!(
        cache_offset <= FLORA_LIGHTING_CACHE_OFFSET_MASK,
        "flora lighting cache offset {cache_offset} exceeds {} entries",
        FLORA_LIGHTING_CACHE_OFFSET_MASK + 1,
    );
    assert!(
        (1..=FLORA_LIGHTING_CACHE_VOXEL_COUNT_MAX).contains(&voxel_count),
        "flora mesh voxel count {voxel_count} is outside the cache encoding range",
    );
    cache_offset
        | (voxel_count << FLORA_LIGHTING_CACHE_OFFSET_BITS)
        | if is_lod {
            FLORA_LIGHTING_CACHE_LOD_BIT
        } else {
            0
        }
}

fn flora_lighting_cache_instance_ty(instance_ty: u32, instance_count: u32) -> u32 {
    assert!(
        instance_ty <= FLORA_INSTANCE_TYPE_MASK,
        "flora instance type {instance_ty} exceeds the packed cache encoding",
    );
    assert!(
        instance_count <= u32::MAX >> FLORA_LIGHTING_CACHE_INSTANCE_COUNT_SHIFT,
        "flora instance count {instance_count} exceeds the packed cache encoding",
    );
    instance_ty | (instance_count << FLORA_LIGHTING_CACHE_INSTANCE_COUNT_SHIFT)
}

fn flora_lighting_cache_dispatch_enabled(
    raster_flora_ddgi_lighting: bool,
    local_lights_active: bool,
    required_entries: u32,
) -> bool {
    (raster_flora_ddgi_lighting || local_lights_active) && required_entries > 0
}

#[cfg(test)]
mod flora_lighting_cache_location_tests {
    use super::*;
    use crate::ddgi::{DdgiFieldKey, DdgiFieldState};
    use crate::lighting::{LocalLight, LocalLightRegistry, PointLight};

    #[test]
    fn cache_location_packs_offset_voxel_count_and_lod_without_overlap() {
        let packed = flora_lighting_cache_location(123_456, 237, true);
        assert_eq!(packed & FLORA_LIGHTING_CACHE_OFFSET_MASK, 123_456);
        assert_eq!(
            (packed >> FLORA_LIGHTING_CACHE_OFFSET_BITS) & FLORA_LIGHTING_CACHE_VOXEL_COUNT_MAX,
            237
        );
        assert_ne!(packed & FLORA_LIGHTING_CACHE_LOD_BIT, 0);
    }

    #[test]
    fn compute_instance_type_packs_the_dispatch_bound_without_changing_species() {
        let packed = flora_lighting_cache_instance_ty(3, 40_000);
        assert_eq!(packed & FLORA_INSTANCE_TYPE_MASK, 3);
        assert_eq!(packed >> FLORA_LIGHTING_CACHE_INSTANCE_COUNT_SHIFT, 40_000);
    }

    #[test]
    fn cache_dispatch_requires_environment_or_local_lighting_and_visible_flora() {
        assert!(flora_lighting_cache_dispatch_enabled(true, false, 1));
        assert!(flora_lighting_cache_dispatch_enabled(false, true, 1));
        assert!(!flora_lighting_cache_dispatch_enabled(true, true, 0));
        assert!(!flora_lighting_cache_dispatch_enabled(false, false, 1));
    }

    #[test]
    fn point_light_gpu_values_stay_in_world_units_and_priority_scales_each_axis_once() {
        let mut domain = LocalLightRegistry::default();
        domain.add(LocalLight::Point(
            PointLight::new(Vec3::new(1.0, 2.0, 3.0), Vec3::ONE, 4.0, 0.05, 0.5).unwrap(),
        ));
        let gpu = LocalLightGpuSnapshot::from_authoritative(
            &domain.snapshot(),
            LocalLightBudget::point_lights(1),
            0,
        );
        assert_eq!(gpu.lights[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(gpu.lights[0].range, 0.5);
        assert_eq!(gpu.lights[0].source_radius, 0.05);

        let world_bound = gpu.payload().influence_bound().unwrap();
        let voxel_bound = local_light_impact_voxel_bound(
            world_bound,
            UVec3::new(256, 128, 64),
            UVec3::splat(1_024),
        );
        assert_eq!(voxel_bound.min(), UVec3::new(128, 192, 160));
        assert_eq!(voxel_bound.max(), UVec3::new(384, 320, 224));
    }

    #[test]
    fn gpu_sweep_identity_accepts_only_the_final_state_classification_change() {
        let source = DdgiFieldKey::new(8, 4, 2, 32, DdgiFieldState::Converging, 6).unwrap();
        let sampled = DdgiFieldIdentity::new(
            DdgiFieldKey::new(9, 4, 2, 32, DdgiFieldState::Converging, 7).unwrap(),
            Some(source),
        )
        .unwrap();
        let classified = DdgiFieldIdentity::new(
            DdgiFieldKey::new(9, 4, 2, 32, DdgiFieldState::Converged, 7).unwrap(),
            Some(source),
        )
        .unwrap();
        let evidence = DdgiLocalLightGpuEvidence::new(sampled, 1, 1, 4_913);

        assert!(evidence.matches_classified_field(classified));
        let different_epoch = DdgiFieldIdentity::new(
            DdgiFieldKey::new(10, 4, 2, 32, DdgiFieldState::Converged, 8).unwrap(),
            Some(sampled.field()),
        )
        .unwrap();
        assert!(!evidence.matches_classified_field(different_epoch));
    }

    #[test]
    fn every_surface_flora_mesh_fits_the_cache_voxel_count_encoding() {
        for desc in crate::flora::species::species() {
            for is_lod in [false, true] {
                let voxel_count = (desc.mesh_generator)(is_lod).unwrap().voxel_infos.len() as u32;
                assert!(
                    voxel_count <= FLORA_LIGHTING_CACHE_VOXEL_COUNT_MAX,
                    "{} has {voxel_count} voxels",
                    desc.key,
                );
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TreeRenderInstanceData {
    world_pos: UVec3,
    leaf_local_pos: IVec3,
}

#[derive(Debug, Clone, Copy)]
struct TreeShadowRenderInstanceData {
    world_pos: UVec3,
    leaf_local_pos: IVec3,
    billboard_size_voxels: f32,
    opacity_layer_count: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct TracerDesc {
    pub scaling_factor: f32,
    pub default_camera_look_at: Vec3,
    pub voxel_dim_per_chunk: UVec3,
    pub environment_probe_spacing_voxels: u32,
    pub environment_probe_visualization_enabled: bool,
    pub environment_irradiance_capture_enabled: bool,
    pub environment_irradiance_capture_target: DdgiCaptureTarget,
    pub ddgi_batch_order: DdgiBatchOrder,
    pub ddgi_debug_view: DdgiDebugView,
    pub ddgi_terrain_hard_origin: crate::ddgi::DdgiTerrainHardOrigin,
    pub ddgi_local_light_trace_diagnostics_enabled: bool,
    pub glass_experiment_enabled: bool,
    pub glass_debug_view: u32,
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

pub struct DirectSunShadowResources<'a> {
    pub gui_input: &'a Buffer,
    pub shadow_camera_info: &'a Buffer,
    pub shadow_map_tex_for_vsm_ping: &'a Texture,
    pub leaf_shadow_opacity_blended_tex: &'a Texture,
    pub leaf_shadow_mask_tex: &'a Texture,
    pub cloud_shadow_tex: &'a Texture,
}

/// Descriptor sets prepared for a private DDGI staging volume. The sets retain the staging
/// volume's resource owners but are not visible to any frame until promotion publishes them.
struct PreparedDdgiConsumerDescriptors {
    token_serial: u64,
    tracer: PreparedDescriptorGeneration,
    tracer_glass: PreparedDescriptorGeneration,
    flora_lighting_cache: PreparedDescriptorGeneration,
    tree_leaf_lighting_cache: PreparedDescriptorGeneration,
    graphics: Vec<PreparedDescriptorGeneration>,
}

struct PreparedTreeFoliageBatch<'a> {
    batch: TreeFoliageBatch,
    instance: &'a TreeLeavesInstance,
    descriptors: PreparedDrawDescriptors,
}

#[derive(Clone, Copy)]
enum TreeFoliageInstanceStream {
    Visible,
    Shadow,
}

fn tree_foliage_instance(
    surface_resources: &SurfaceResources,
    batch: TreeFoliageBatch,
    stream: TreeFoliageInstanceStream,
) -> &TreeLeavesInstance {
    let instances = match batch.kind() {
        TreeFoliageKind::Leaves => &surface_resources.instances.leaves_instances,
        TreeFoliageKind::Apples => &surface_resources.instances.apple_instances,
    };
    let instance = instances.get(&batch.tree_id()).unwrap_or_else(|| {
        panic!(
            "tree foliage frame batch {:?} tree {} must resolve to its snapshotted resource",
            batch.kind(),
            batch.tree_id(),
        )
    });
    let actual_instance_count = match stream {
        TreeFoliageInstanceStream::Visible => instance.resources.instances_len,
        TreeFoliageInstanceStream::Shadow => instance.resources.shadow_instances_len,
    };
    assert_eq!(
        actual_instance_count,
        batch.instance_count(),
        "tree foliage {:?} stream batch {:?} tree {} instance count changed after planning",
        match stream {
            TreeFoliageInstanceStream::Visible => "visible",
            TreeFoliageInstanceStream::Shadow => "shadow",
        },
        batch.kind(),
        batch.tree_id(),
    );
    instance
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
    direct_sun_shadows: DirectSunShadowRuntime,
    cloud_history_valid: bool,
    god_ray_temporal_blend_enabled: bool,
    god_ray_temporal_alpha: f32,
    god_ray_history_valid: bool,
    lens_flare_history_valid: bool,
    environment_lighting: EnvironmentLightingCache,
    flora_lighting_cache: FloraLightingCache,
    ddgi_voxel_visibility: DdgiVoxelVisibility,
    ddgi_runtime: DdgiRuntime,
    ddgi_history_retention: f32,
    prepared_ddgi_consumer_descriptors: Option<PreparedDdgiConsumerDescriptors>,
    ddgi_trace_stats_readback_pending: Option<DdgiRayBatch>,
    ddgi_local_light_gpu_evidence_accumulating: Option<DdgiLocalLightGpuEvidence>,
    ddgi_local_light_gpu_evidence_complete: Option<DdgiLocalLightGpuEvidence>,
    ddgi_relocation_stats_readback_pending: bool,
    ddgi_flora_consumer_logged_token_serial: Option<u64>,
    local_light_live_revision: Option<u64>,
    local_light_live_payload: Option<crate::lighting::LocalLightGpuPayload>,
    local_light_uploaded_source_revision: Option<u64>,
    local_light_uploaded_registry_revision: Option<u64>,
    local_light_live_count: u32,
    local_light_live_overflow: Vec<LocalLightOverflow>,
    local_light_visibility_diagnostic: LocalLightVisibilityDiagnostic,
    environment_probe_visualization: EnvironmentProbeVisualizationSettings,

    compute_pipelines: ComputePipelines,
    graphics_pipelines: GraphicsPipelines,

    render_target_color_and_depth: RenderTarget,
    render_target_depth_only: RenderTarget,
    render_target_leaf_shadow_opacity: RenderTarget,
    render_target_gui: RenderTarget,
    frame_retirement_sink: FrameRetirementSink,
    frame_extent_generation: FrameExtentGeneration,
    descriptor_generation: u64,
    tree_instance_generation: u64,

    #[allow(dead_code)]
    pool: DescriptorPool,

    world_tick_seconds: f32,
    raster_flora_ddgi_lighting: bool,
    last_wind_volume_step: Option<u32>,
    initialized_wind_volume_bucket_count: u32,
    wind_source_buffer_capacity: usize,
    spatial_sound_manager: SpatialSoundManager,
    particle_instance_scratch: Vec<ParticleInstanceGpu>,
    translucent_particle_instance_scratch: Vec<ParticleInstanceGpu>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GlassDebugSummary {
    pub extent: Extent2D,
    pub glass_pixels: usize,
    pub foreground_pixels: usize,
    pub screen_hit_pixels: usize,
    pub raster_screen_hit_pixels: usize,
    pub fallback_pixels: usize,
    pub query_budget_fallback_pixels: usize,
    pub exhaustion_pixels: usize,
    pub scene_queries_max: u32,
    pub interfaces_max: u32,
    pub dda_steps_median: u32,
    pub dda_steps_p95: u32,
    pub dda_steps_max: u32,
    pub peak_active_paths: u32,
    pub tir_events: usize,
    pub throughput_cutoffs: usize,
    pub top_k_pruned: usize,
    pub nonfinite_pixels: usize,
}

impl Drop for Tracer {
    fn drop(&mut self) {}
}

impl Tracer {
    /// Declares CPU writes before reflected pipeline resources consume these buffers.
    ///
    /// The declaration is intentionally made outside render passes: a HostWrite-to-shader
    /// dependency must be recorded before a subpass begins. The descriptor runtime derives each
    /// consumer stage and access from the pipeline's active reflection.
    pub fn record_host_buffer_writes(&self, cmdbuf: &CommandBuffer) {
        let updated_buffers = [
            &*self.resources.uniforms.gui_input,
            &*self.resources.uniforms.sun_info,
            &*self.resources.uniforms.shading_info,
            &*self.resources.uniforms.camera_info,
            &*self.resources.uniforms.camera_info_prev_frame,
            &*self.resources.uniforms.env_info,
            &*self.resources.uniforms.starlight_info,
            &*self.resources.uniforms.voxel_colors,
            &*self.resources.uniforms.terrain_edit_preview,
            &*self.resources.uniforms.flora_growth_info,
            &*self.resources.uniforms.god_ray_info,
            &*self.resources.uniforms.post_processing_info,
            &*self.resources.shadow.shadow_camera_info,
            &*self.resources.wind.wind_sources,
            &*self.resources.local_lighting.local_light_info,
            &*self.resources.local_lighting.local_lights,
            &*self
                .resources
                .local_lighting
                .local_light_visibility_diagnostic_info,
        ];
        for buffer in updated_buffers {
            cmdbuf.use_buffer(buffer, BufferUse::HostWrite);
        }
    }

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
        self.direct_sun_shadows.available_mask()
    }

    pub fn invalidate_local_direct_sun_shadow_histories(&mut self) {
        self.direct_sun_shadows.invalidate_local_histories();
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
        frame_retirement_sink: FrameRetirementSink,
        chunk_bound: UAabb3,
        frame_extent_generation: FrameExtentGeneration,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
        plain_builder_resources: &PlainBuilderResources,
        desc: TracerDesc,
        spatial_sound_manager: SpatialSoundManager,
    ) -> Result<Self> {
        let screen_extent = frame_extent_generation.extent();
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
        );

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
            desc.environment_irradiance_capture_enabled,
            desc.glass_experiment_enabled,
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
        let dynamic_fruit_resources = DynamicFruitRendererResources::new(
            vulkan_ctx.device().clone(),
            allocator.clone(),
            frame_retirement_sink.clone(),
        );
        let ddgi_volume = DdgiVolume::new(
            &vulkan_ctx,
            allocator.clone(),
            chunk_bound.dimensions() * desc.voxel_dim_per_chunk,
            desc.environment_probe_spacing_voxels,
            desc.voxel_dim_per_chunk,
            desc.ddgi_batch_order,
        )?;
        let ddgi_runtime = DdgiRuntime::new(ddgi_volume.status().grid);
        let mut ddgi_runtime = ddgi_runtime;
        ddgi_runtime.configure_capture(
            desc.environment_irradiance_capture_enabled,
            desc.environment_irradiance_capture_target,
            desc.ddgi_batch_order,
        );
        log::info!(
            "[DDGI][HARD_ORIGIN] terrain_mode={}",
            desc.ddgi_terrain_hard_origin.label()
        );
        log::info!(
            "[DDGI][VISIBILITY] runtime_consumers=moment transport_and_reference=moment+exact"
        );
        let ddgi_voxel_visibility = DdgiVoxelVisibility::new(
            &vulkan_ctx,
            allocator.clone(),
            chunk_bound.dimensions() * desc.voxel_dim_per_chunk,
            desc.voxel_dim_per_chunk,
            desc.glass_experiment_enabled,
            if desc.glass_experiment_enabled {
                crate::voxel_material::GLASS_EXPERIMENT_MATERIAL_REVISION
            } else {
                0
            },
            &shader_modules.ddgi_voxel_visibility_pack_sm,
        )?;
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
            &ddgi_volume,
            &ddgi_voxel_visibility,
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
            &ddgi_volume,
            &ddgi_voxel_visibility,
        );
        ddgi_runtime.install_volumes(DdgiVolumes::new(ddgi_volume));

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
        log::info!("[ENV_LIGHTING] backend=ddgi ready=false state=initializing");

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
            direct_sun_shadows: DirectSunShadowRuntime::default(),
            cloud_history_valid: false,
            god_ray_temporal_blend_enabled: true,
            god_ray_temporal_alpha: 0.10,
            god_ray_history_valid: false,
            lens_flare_history_valid: false,
            environment_lighting: EnvironmentLightingCache::default(),
            flora_lighting_cache: FloraLightingCache::default(),
            ddgi_voxel_visibility,
            ddgi_runtime,
            ddgi_history_retention: 0.99,
            prepared_ddgi_consumer_descriptors: None,
            ddgi_trace_stats_readback_pending: None,
            ddgi_local_light_gpu_evidence_accumulating: None,
            ddgi_local_light_gpu_evidence_complete: None,
            ddgi_relocation_stats_readback_pending: false,
            ddgi_flora_consumer_logged_token_serial: None,
            local_light_live_revision: None,
            local_light_live_payload: None,
            local_light_uploaded_source_revision: None,
            local_light_uploaded_registry_revision: None,
            local_light_live_count: 0,
            local_light_live_overflow: Vec::new(),
            local_light_visibility_diagnostic: LocalLightVisibilityDiagnostic::default(),
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
            frame_retirement_sink,
            frame_extent_generation,
            descriptor_generation: 1,
            tree_instance_generation: 1,
            pool,
            world_tick_seconds: crate::game_time::WORLD_TICK_SECONDS_DEFAULT,
            raster_flora_ddgi_lighting: true,
            last_wind_volume_step: None,
            initialized_wind_volume_bucket_count: 0,
            wind_source_buffer_capacity: 1,
            spatial_sound_manager,
            particle_instance_scratch: Vec::with_capacity(particle_capacity),
            translucent_particle_instance_scratch: Vec::with_capacity(particle_capacity),
        })
    }

    pub fn ddgi_runtime_status(&self) -> DdgiRuntimeStatus {
        self.ddgi_runtime
            .status(self.ddgi_runtime.volumes().status())
    }

    /// Latest radiance identity observed by the transport scheduler. Test scenes use this logical
    /// lifecycle value to sequence deterministic changes without inspecting GPU resource slots.
    pub fn ddgi_latest_radiance_revision(&self) -> Option<u32> {
        self.ddgi_runtime.latest_radiance_revision()
    }

    pub fn rebuild_environment_probes(&mut self, spacing_voxels: u32) {
        self.ddgi_runtime.request_density_rebuild(spacing_voxels);
        log::info!(
            "[DDGI] density rebuild queued spacing_voxels={} coordinator={:?}",
            spacing_voxels,
            self.ddgi_runtime.refresh_state(),
        );
        self.drive_pending_ddgi_rebuild();
    }

    fn prepare_ddgi_staging(&mut self, build_token: DdgiBuildToken) -> Result<()> {
        let mut staging = DdgiVolume::new(
            &self.vulkan_ctx,
            self.allocator.clone(),
            self.chunk_bound.dimensions() * self.desc.voxel_dim_per_chunk,
            build_token.spacing_voxels(),
            self.desc.voxel_dim_per_chunk,
            self.desc.ddgi_batch_order,
        )?;
        staging.assign_build_token(build_token);
        staging.request_initialization(build_token.terrain_revision());
        if let Some(retired_token) = self
            .ddgi_runtime
            .volumes()
            .status()
            .staging()
            .and_then(|staging| staging.build_token)
            .filter(|retired_token| *retired_token != build_token)
        {
            log::info!(
                "[DDGI] obsolete staging promotion skipped token_serial={} kind={:?} terrain_revision={} replacement_token_serial={} replacement_terrain_revision={}",
                retired_token.serial(),
                retired_token.kind(),
                retired_token.terrain_revision(),
                build_token.serial(),
                build_token.terrain_revision(),
            );
        }
        // Builder descriptors move first. Active consumers keep sampling the complete volume until
        // the replacement reaches Ready and is explicitly promoted on a later frame.
        let descriptor_generation = self.next_descriptor_generation();
        let descriptor_retirements =
            self.update_ddgi_builder_descriptors(&staging, None, descriptor_generation);
        for retirement in descriptor_retirements {
            self.frame_retirement_sink.retire(retirement);
        }
        // Prepare the consumer generation while this volume is still private. The descriptor
        // writes and owner copies are paid during staging setup; promotion only swaps the already
        // complete sets into the active pipelines and schedules the old generation for retirement.
        self.prepared_ddgi_consumer_descriptors =
            Some(self.stage_ddgi_consumer_descriptors(&staging));
        self.ddgi_trace_stats_readback_pending = None;
        self.ddgi_relocation_stats_readback_pending = false;
        let retired_staging = self.ddgi_runtime.volumes_mut().prepare_staging(staging);
        if let Some(retired_staging) = retired_staging {
            self.frame_retirement_sink.retire(FrameRetirement::new(
                "ddgi.staging.volume",
                descriptor_generation,
                retired_staging,
            ));
        }
        let status = self.ddgi_runtime.volumes().status();
        log::info!(
            "[DDGI] staging prepared token_serial={} kind={:?} spacing_voxels={} probes={} active_terrain_revision={} target_terrain_revision={}",
            build_token.serial(),
            build_token.kind(),
            status.builder().grid.spacing_voxels(),
            status.builder().grid.probe_count(),
            status
                .active()
                .relocated_terrain_revision
                .unwrap_or_default(),
            build_token.terrain_revision(),
        );
        Ok(())
    }

    fn start_next_ddgi_scheduled_work(&mut self) -> Result<bool> {
        let Some(runtime_work) = self.ddgi_runtime.claim_transport_work() else {
            return Ok(false);
        };
        let work = runtime_work.scheduled();
        let destination = work.destination().field();
        assert_eq!(
            runtime_work.authored_lighting().revision,
            destination.radiance_revision(),
            "DDGI transport work must retain the authored lighting revision it was scheduled with",
        );
        let status = self.ddgi_runtime.volumes().builder().status();
        anyhow::ensure!(
            status.grid.spacing_voxels() == destination.spacing_voxels(),
            "DDGI scheduler selected spacing {} for builder spacing {}",
            destination.spacing_voxels(),
            status.grid.spacing_voxels(),
        );
        self.ddgi_runtime
            .volumes_mut()
            .builder_mut()
            .begin_scheduled_work(
                work,
                runtime_work.local_refresh_voxel_bound(),
                runtime_work.radiance_history_policy(),
                runtime_work.probe_priority(),
            )?;
        if let Some(priority) = runtime_work.probe_priority() {
            log::debug!(
                "[DDGI][PRIORITY] latched reason={:?} voxel_min={:?} voxel_max={:?} field_serial={} revision={}",
                priority.reason(),
                priority.voxel_bound().min(),
                priority.voxel_bound().max(),
                destination.serial(),
                destination.radiance_revision(),
            );
        }
        if work.kind() == DdgiScheduledWorkKind::GeometryUpdate {
            if let Some((dirty_probes, preserved_probes)) = self
                .ddgi_runtime
                .volumes()
                .builder()
                .local_refresh_probe_partition()
            {
                log::info!(
                    "[DDGI][LOCAL_RECOVERY] prepared geometry_revision={} dirty_probes={} preserved_probes={} minimum_epoch={} stable_epochs={} max_absolute_delta={:.3}",
                    destination.geometry_revision(),
                    dirty_probes,
                    preserved_probes,
                    crate::ddgi::DDGI_LOCAL_RECOVERY_MIN_EPOCH,
                    crate::ddgi::DDGI_LOCAL_RECOVERY_STABLE_EPOCHS,
                    crate::ddgi::DDGI_LOCAL_RECOVERY_MAX_ABSOLUTE_DELTA,
                );
            }
        }
        let descriptor_generation = self.next_descriptor_generation();
        let descriptor_retirements = {
            let volumes = self.ddgi_runtime.volumes();
            let builder = volumes.builder();
            let inherited_source = (work.kind() == DdgiScheduledWorkKind::GeometryUpdate
                && work.transport_source().is_some()
                && !volumes.builder_is_active())
            .then(|| volumes.active());
            self.update_ddgi_builder_descriptors(builder, inherited_source, descriptor_generation)
        };
        for retirement in descriptor_retirements {
            self.frame_retirement_sink.retire(retirement);
        }
        let lighting = self.ddgi_runtime.lighting_diagnostics();
        log::debug!(
            "[DDGI][SCHEDULER] claimed kind={:?} serial={} geometry_revision={} radiance_revision={} spacing_voxels={} state={:?} update_epoch={} source={:?} latest_transport_revision={:?} source_live_revision={:?} scheduler_published_revision={:?} in_flight_revision={:?} revision_lag={} coalesced_revisions={} mixed_in_flight={}",
            work.kind(),
            destination.serial(),
            destination.geometry_revision(),
            destination.radiance_revision(),
            destination.spacing_voxels(),
            destination.state(),
            destination.update_epoch(),
            work.destination().source(),
            lighting.latest_transport_revision,
            lighting.latest_source_live_revision,
            lighting.scheduler_published_revision,
            lighting.in_flight_revision,
            lighting.scheduler_revision_lag(),
            lighting.coalesced_revisions,
            lighting.has_mixed_in_flight_revision,
        );
        Ok(true)
    }

    fn rebuild_ddgi_voxel_visibility(
        &mut self,
        geometry_revision: u32,
        edited_voxel_bound: UAabb3,
    ) -> Result<()> {
        let started = std::time::Instant::now();
        let Some(update) = self
            .ddgi_voxel_visibility
            .begin_pack(geometry_revision, edited_voxel_bound)?
        else {
            log::info!("[DDGI][VOXEL_VISIBILITY] reused geometry_revision={geometry_revision}");
            return Ok(());
        };
        let dispatch = update.word_dimensions;
        let block_dispatch = update.block_dimensions;
        let pack_to_blocks = PipelineBarrier::shader_access(
            PipelineStage::COMPUTE_SHADER,
            PipelineStage::COMPUTE_SHADER,
        );
        let pack_to_queries = PipelineBarrier::shader_access(
            PipelineStage::COMPUTE_SHADER,
            PipelineStage::COMPUTE_SHADER | PipelineStage::VERTEX_SHADER,
        );
        execute_one_time_gpu_job(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                cmdbuf.use_buffer(
                    &self.ddgi_voxel_visibility.ddgi_voxel_visibility_info,
                    BufferUse::HostWrite,
                );
                self.compute_pipelines
                    .ddgi_voxel_visibility_pack_ppl
                    .record(
                        cmdbuf,
                        Extent3D::new(dispatch.x, dispatch.y, dispatch.z),
                        None,
                    );
                pack_to_blocks.record_insert(self.vulkan_ctx.device(), cmdbuf);
                self.compute_pipelines
                    .ddgi_voxel_visibility_blocks_ppl
                    .record(
                        cmdbuf,
                        Extent3D::new(block_dispatch.x, block_dispatch.y, block_dispatch.z),
                        None,
                    );
                pack_to_queries.record_insert(self.vulkan_ctx.device(), cmdbuf);
            },
        );
        self.ddgi_voxel_visibility.publish_pack(geometry_revision)?;
        debug_assert_eq!(
            self.ddgi_voxel_visibility.published_revision(),
            Some(geometry_revision)
        );
        log::info!(
            "[DDGI][VOXEL_VISIBILITY] published geometry_revision={} packed_offset={} packed={}x{}x{} block_offset={} blocks={}x{}x{} elapsed_ms={:.3}",
            geometry_revision,
            update.word_offset,
            dispatch.x,
            dispatch.y,
            dispatch.z,
            update.block_offset,
            block_dispatch.x,
            block_dispatch.y,
            block_dispatch.z,
            started.elapsed().as_secs_f64() * 1000.0,
        );
        Ok(())
    }

    /// Observes one authoritative visible-terrain publication. Exact occupancy is synchronously
    /// published first, so the runtime cannot claim work for this revision before the matching
    /// Visibility Map exists.
    pub fn observe_published_environment_probe_terrain(
        &mut self,
        geometry_revision: u32,
        edited_voxel_bound: UAabb3,
    ) -> Result<()> {
        self.rebuild_ddgi_voxel_visibility(geometry_revision, edited_voxel_bound)?;
        let newly_observed = self
            .ddgi_runtime
            .observe_visible_terrain(geometry_revision, edited_voxel_bound);
        let combined_edit_bound = self.ddgi_runtime.edited_voxel_bound();
        let invalidation_bound = self.ddgi_runtime.invalidation_voxel_bound();
        log::info!(
            "[DDGI] runtime observed visible terrain revision={} newly_observed={} edit_voxel_bound={:?}..{:?} combined_edit_voxel_bound={:?} invalidation_voxel_bound={:?} coordinator={:?}",
            geometry_revision,
            newly_observed,
            edited_voxel_bound.min(),
            edited_voxel_bound.max(),
            combined_edit_bound.map(|bound| (bound.min(), bound.max())),
            invalidation_bound.map(|bound| (bound.min(), bound.max())),
            self.ddgi_runtime.refresh_state(),
        );
        Ok(())
    }

    /// Claims and installs the latest authoritative DDGI build at a GPU-safe replacement point.
    pub fn drive_pending_ddgi_rebuild(&mut self) {
        let Some(build) = self.ddgi_runtime.claim_volume_build() else {
            return;
        };
        let build_token = build.token();
        match build.target() {
            DdgiRuntimeVolumeTarget::Active => {
                let builder_status = self.ddgi_runtime.volumes().builder().status();
                assert!(
                    builder_status.build_token.is_none(),
                    "initial DDGI Volume must not already have a build token"
                );
                self.ddgi_runtime
                    .volumes_mut()
                    .builder_mut()
                    .assign_build_token(build_token);
                assert!(
                    self.ddgi_runtime
                        .volumes_mut()
                        .builder_mut()
                        .request_initialization(build_token.terrain_revision()),
                    "initial DDGI Volume must accept its authoritative terrain revision"
                );
                let status = self.ddgi_runtime.volumes().status().builder();
                log::info!(
                    "[DDGI] initialization requested terrain_revision={} spacing_voxels={} probes={} stage={:?}",
                    build_token.terrain_revision(),
                    status.grid.spacing_voxels(),
                    status.grid.probe_count(),
                    status.stage,
                );
            }
            DdgiRuntimeVolumeTarget::Staging => {
                let preparation = self.prepare_ddgi_staging(build_token);
                require_ddgi_staging_preparation(build_token, preparation);
                log::info!(
                    "[DDGI] rebuild started token_serial={} kind={:?} terrain_revision={} spacing_voxels={} edited_voxel_bound={:?} invalidation_voxel_bound={:?}",
                    build_token.serial(),
                    build_token.kind(),
                    build_token.terrain_revision(),
                    build_token.spacing_voxels(),
                    self.ddgi_runtime
                        .edited_voxel_bound()
                        .map(|bound| (bound.min(), bound.max())),
                    self.ddgi_runtime
                        .invalidation_voxel_bound()
                        .map(|bound| (bound.min(), bound.max())),
                );
            }
        }
    }

    pub fn ddgi_debug_view(&self) -> DdgiDebugView {
        self.desc.ddgi_debug_view
    }

    pub(crate) fn ddgi_live_radiance_revision(&self) -> u32 {
        self.ddgi_runtime
            .live_authored_lighting()
            .map_or(0, |lighting| lighting.revision)
    }

    pub(crate) fn ddgi_live_radiance_snapshot(&self) -> Option<DdgiRadianceSnapshot> {
        self.ddgi_runtime
            .live_authored_lighting()
            .map(|lighting| lighting.snapshot)
    }

    pub(crate) fn ddgi_builder_radiance_snapshot(&self) -> Option<DdgiRadianceSnapshot> {
        self.ddgi_runtime.volumes().builder().radiance_snapshot()
    }

    pub(crate) fn local_light_live_state(&self) -> (Option<u64>, u32) {
        (
            self.local_light_uploaded_source_revision,
            self.local_light_live_count,
        )
    }

    pub(crate) fn local_light_revision_observability(
        &self,
    ) -> (Option<u64>, Option<u64>, Option<u64>) {
        (
            self.local_light_uploaded_source_revision,
            self.local_light_uploaded_registry_revision,
            self.local_light_live_revision,
        )
    }

    pub(crate) fn local_light_transport_observability(&self) -> (u64, u64) {
        (
            self.environment_lighting.revision_lag(),
            self.environment_lighting.coalesced_live_revisions(),
        )
    }

    pub(crate) fn local_light_overflow_evidence(&self) -> &[LocalLightOverflow] {
        &self.local_light_live_overflow
    }

    pub(crate) fn ddgi_lighting_diagnostics(&self) -> crate::ddgi::DdgiLightingDiagnostics {
        self.ddgi_runtime.lighting_diagnostics()
    }

    pub(crate) fn ddgi_local_light_gpu_evidence(&self) -> Option<DdgiLocalLightGpuEvidence> {
        self.ddgi_local_light_gpu_evidence_complete
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn request_local_light_visibility_diagnostic(
        &mut self,
        geometry_revision: u32,
        source_revision: u64,
        light_id: LightId,
        receiver_position: Vec3,
        receiver_normal: Vec3,
        ray_origin_offset_world: f32,
    ) -> Result<u32> {
        self.local_light_visibility_diagnostic.request(
            &self.resources.local_lighting,
            geometry_revision,
            source_revision,
            light_id,
            receiver_position,
            receiver_normal,
            ray_origin_offset_world,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn request_local_light_aggregate_visibility_diagnostic(
        &mut self,
        geometry_revision: u32,
        source_revision: u64,
        receiver_position: Vec3,
        receiver_normal: Vec3,
        ray_origin_offset_world: f32,
    ) -> Result<u32> {
        self.local_light_visibility_diagnostic.request_aggregate(
            &self.resources.local_lighting,
            geometry_revision,
            source_revision,
            receiver_position,
            receiver_normal,
            ray_origin_offset_world,
        )
    }

    pub(crate) fn local_light_visibility_diagnostic_evidence(
        &self,
    ) -> Option<LocalLightVisibilityDiagnosticEvidence> {
        self.local_light_visibility_diagnostic.published()
    }

    fn observe_ddgi_local_light_gpu_evidence(
        &mut self,
        batch: DdgiRayBatch,
        stats: DdgiTraceStats,
        expected_probe_count: u32,
        snapshot: DdgiRadianceSnapshot,
    ) -> Result<()> {
        if snapshot.local_lights.info.flags & LOCAL_LIGHT_FLAG_DDGI_TRACE_DIAGNOSTICS == 0 {
            return Ok(());
        }
        anyhow::ensure!(
            snapshot.local_lights.info.transport_revision == batch.radiance_revision(),
            "DDGI local-light diagnostics observed transport revision {} for batch revision {}",
            snapshot.local_lights.info.transport_revision,
            batch.radiance_revision(),
        );
        if self
            .ddgi_local_light_gpu_evidence_accumulating
            .is_none_or(|evidence| evidence.field != batch.logical())
        {
            self.ddgi_local_light_gpu_evidence_accumulating = Some(DdgiLocalLightGpuEvidence::new(
                batch.logical(),
                snapshot.local_lights.source_revision(),
                snapshot.local_lights.count(),
                expected_probe_count,
            ));
        }
        let evidence = self
            .ddgi_local_light_gpu_evidence_accumulating
            .as_mut()
            .expect("DDGI local-light diagnostics accumulator must exist");
        anyhow::ensure!(
            evidence.local_source_revision == snapshot.local_lights.source_revision()
                && evidence.local_light_count == snapshot.local_lights.count(),
            "DDGI local-light diagnostic sweep mixed immutable snapshots",
        );
        evidence.accumulate(batch, stats);
        if evidence.is_complete() {
            let semantic_identity_changed =
                self.ddgi_local_light_gpu_evidence_complete
                    .is_none_or(|previous| {
                        previous.field.field().geometry_revision()
                            != evidence.field.field().geometry_revision()
                            || previous.field.field().radiance_revision()
                                != evidence.field.field().radiance_revision()
                            || previous.local_source_revision != evidence.local_source_revision
                            || previous.local_light_count != evidence.local_light_count
                    });
            if semantic_identity_changed {
                log::info!(
                    "[DDGI][LOCAL_LIGHT_GPU_EVIDENCE] complete field_serial={} geometry_revision={} radiance_revision={} update_epoch={} source_revision={} light_count={} probes={} candidates={} visible={} occluded={} irradiance_luma_q8={} irradiance_luma={:.6} emissive_surface_hits={} emissive_surface_radiance_luma_q8={}",
                    evidence.field.field().serial(),
                    evidence.field.field().geometry_revision(),
                    evidence.field.field().radiance_revision(),
                    evidence.field.field().update_epoch(),
                    evidence.local_source_revision,
                    evidence.local_light_count,
                    evidence.sampled_probe_count,
                    evidence.totals.candidates,
                    evidence.totals.visible,
                    evidence.totals.occluded,
                    evidence.totals.irradiance_luma_q8,
                    evidence.totals.irradiance_luma(),
                    evidence.emissive_surface_hits,
                    evidence.emissive_surface_radiance_luma_q8,
                );
            }
            self.ddgi_local_light_gpu_evidence_complete = Some(*evidence);
        }
        Ok(())
    }

    pub fn ddgi_capture_checkpoint(&self) -> Option<DdgiCaptureCheckpoint> {
        self.ddgi_runtime
            .status(self.ddgi_runtime.volumes().status())
            .capture_checkpoint()
    }

    pub fn ddgi_capture_target(&self) -> DdgiCaptureTarget {
        self.ddgi_runtime.capture_target()
    }

    fn observe_ddgi_capture_checkpoint(
        &mut self,
        build_token: DdgiBuildToken,
        field: DdgiFieldIdentity,
        validation: crate::ddgi::DdgiAtlasValidationStats,
        publication: DdgiCapturePublication,
    ) {
        if !self.ddgi_runtime.observe_capture_checkpoint(
            build_token,
            field,
            validation,
            publication,
        ) {
            return;
        }
        log::info!(
            "[ENV_IRRADIANCE_CAPTURE] checkpoint target={} build_token_serial={} field_serial={} state={:?} update_epoch={} publication={:?}",
            self.ddgi_runtime.capture_target().label(),
            build_token.serial(),
            field.field().serial(),
            field.field().state(),
            field.field().update_epoch(),
            publication,
        );
    }

    pub fn ddgi_ready(&self) -> bool {
        self.ddgi_runtime.volumes().status().active().is_ready()
    }

    pub fn ddgi_ready_for_terrain_revision(&self, revision: u32) -> bool {
        let status = self.ddgi_runtime.volumes().status().active();
        status.is_ready() && status.relocated_terrain_revision == Some(revision)
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

    pub fn frame_extent_generation(&self) -> FrameExtentGeneration {
        self.frame_extent_generation
    }

    pub fn assert_frame_extent_generation(&self, generation: FrameExtentGeneration) {
        assert_eq!(
            generation, self.frame_extent_generation,
            "tracer extent resources are not bound to the acquired frame generation"
        );
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
        source.record_copy_to_buffer(cmdbuf, readback, source.get_size_bytes(), 0, 0);
        cmdbuf.use_buffer(readback, BufferUse::HostRead);
    }

    pub fn record_ddgi_spatial_weight_readback(&self, cmdbuf: &CommandBuffer, readback: &Buffer) {
        let source = &self
            .resources
            .extent_dependent_resources
            .ddgi_spatial_weight_readback;
        assert_eq!(source.get_size_bytes(), readback.get_size_bytes());
        source.record_copy_to_buffer(cmdbuf, readback, source.get_size_bytes(), 0, 0);
        cmdbuf.use_buffer(readback, BufferUse::HostRead);
    }

    pub fn environment_probe_terrain_revision_ready(&self, revision: u32) -> bool {
        revision != 0 && self.ddgi_ready_for_terrain_revision(revision)
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
        frame_extent_generation: FrameExtentGeneration,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
        plain_builder_resources: &PlainBuilderResources,
    ) {
        let expected_serial = self
            .frame_extent_generation
            .serial()
            .checked_add(1)
            .expect("tracer frame extent generation overflow");
        assert_eq!(
            frame_extent_generation.serial(),
            expected_serial,
            "tracer frame extent generation must advance exactly once"
        );
        let screen_extent = frame_extent_generation.extent();
        let render_extent = Self::get_render_extent(screen_extent, self.desc.scaling_factor);

        self.camera.on_resize(render_extent);

        // Publish the replacement extent bundle before rebuilding framebuffers. The returned
        // bundle remains owned until the frame-completion retirement clock releases it.
        let retired_extent_resources = self.resources.replace_extent_dependent_resources(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            render_extent,
            screen_extent,
            self.desc.environment_irradiance_capture_enabled,
            self.desc.glass_experiment_enabled,
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

        let new_render_target_color_and_depth = RenderTarget::new(
            self.render_target_color_and_depth.get_render_pass().clone(),
            vec![framebuffer_color_and_depth],
        );
        let new_render_target_depth_only = RenderTarget::new(
            self.render_target_depth_only.get_render_pass().clone(),
            vec![framebuffer_depth_only],
        );
        let new_render_target_leaf_shadow_opacity = RenderTarget::new(
            self.render_target_leaf_shadow_opacity
                .get_render_pass()
                .clone(),
            vec![framebuffer_leaf_shadow_opacity],
        );
        let new_render_target_gui = RenderTarget::new(
            self.render_target_gui.get_render_pass().clone(),
            vec![framebuffer_gui],
        );

        let retired_render_target_color_and_depth = std::mem::replace(
            &mut self.render_target_color_and_depth,
            new_render_target_color_and_depth,
        );
        let retired_render_target_depth_only = std::mem::replace(
            &mut self.render_target_depth_only,
            new_render_target_depth_only,
        );
        let retired_render_target_leaf_shadow_opacity = std::mem::replace(
            &mut self.render_target_leaf_shadow_opacity,
            new_render_target_leaf_shadow_opacity,
        );
        let retired_render_target_gui =
            std::mem::replace(&mut self.render_target_gui, new_render_target_gui);

        let retired_generation = self.frame_extent_generation.serial();
        self.frame_extent_generation = frame_extent_generation;
        self.frame_retirement_sink.retire(FrameRetirement::new(
            "tracer.extent_dependent",
            retired_generation,
            (
                retired_extent_resources,
                retired_render_target_color_and_depth,
                retired_render_target_depth_only,
                retired_render_target_leaf_shadow_opacity,
                retired_render_target_gui,
            ),
        ));

        self.cloud_history_valid = false;
        self.god_ray_history_valid = false;
        self.lens_flare_history_valid = false;
        self.direct_sun_shadows.invalidate_cloud_history();
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
        let descriptor_generation = self.next_descriptor_generation();
        let frame_retirement_sink = self.frame_retirement_sink.clone();
        let update_compute_fn = |ppl: &ComputePipeline, resources: &[&dyn ResourceContainer]| {
            frame_retirement_sink.retire(
                ppl.publish_descriptors(
                    "tracer.resize.compute.descriptors",
                    descriptor_generation,
                    DescriptorUpdate::All(resources),
                )
                .expect("compute descriptor update failed during update_sets"),
            );
        };

        let update_graphics_fn = |ppl: &GraphicsPipeline, resources: &[&dyn ResourceContainer]| {
            frame_retirement_sink.retire(
                ppl.publish_descriptors(
                    "tracer.resize.graphics.descriptors",
                    descriptor_generation,
                    DescriptorUpdate::All(resources),
                )
                .expect("graphics descriptor update failed during update_sets"),
            );
        };

        let update_graphics_set_fn =
            |ppl: &GraphicsPipeline, binding_name: &str, resources: &[&dyn ResourceContainer]| {
                frame_retirement_sink.retire(
                    ppl.publish_descriptors(
                        "tracer.resize.graphics.descriptors",
                        descriptor_generation,
                        DescriptorUpdate::SetContaining {
                            anchor: binding_name,
                            providers: resources,
                        },
                    )
                    .expect("graphics descriptor set update failed during update_sets"),
                );
            };

        let all_resources = self.all_descriptor_resources(
            contree_builder_resources,
            scene_accel_resources,
            plain_builder_resources,
        );
        update_compute_fn(&self.compute_pipelines.tracer_ppl, &all_resources);
        update_compute_fn(&self.compute_pipelines.tracer_glass_ppl, &all_resources);
        update_compute_fn(&self.compute_pipelines.tracer_shadow_ppl, &all_resources);
        update_compute_fn(&self.compute_pipelines.player_collider_ppl, &all_resources);
        update_compute_fn(&self.compute_pipelines.terrain_query_ppl, &all_resources);
        // The flora lighting cache pipeline has only per-draw set 1 descriptors. Its
        // generation is created by the transient adapter at dispatch time.

        let tracer_resources = self.tracer_descriptor_resources();
        let environment_lighting_resources = self.environment_lighting_descriptor_resources();
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
        update_compute_fn(
            &self.compute_pipelines.god_ray_temporal_ppl,
            &tracer_resources,
        );
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
            &self.compute_pipelines.lens_flare_temporal_ppl,
            &tracer_resources,
        );
        update_compute_fn(
            &self.compute_pipelines.lens_flare_sun_visible_ppl,
            &tracer_resources,
        );
        update_compute_fn(&self.compute_pipelines.composition_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.glass_resolve_ppl, &all_resources);
        update_compute_fn(
            &self.compute_pipelines.post_processing_ppl,
            &tracer_resources,
        );

        // update graphics pipelines descriptor sets
        update_graphics_fn(
            &self.graphics_pipelines.terrain_depth_prefill_ppl,
            &tracer_resources,
        );
        update_graphics_set_fn(
            &self.graphics_pipelines.flora_ppl,
            "gui_input",
            &all_resources,
        );
        update_graphics_set_fn(
            &self.graphics_pipelines.flora_lod_ppl,
            "gui_input",
            &all_resources,
        );
        update_graphics_set_fn(
            &self.graphics_pipelines.leaves_ppl,
            "gui_input",
            &environment_lighting_resources,
        );
        update_graphics_set_fn(
            &self.graphics_pipelines.leaves_lod_ppl,
            "gui_input",
            &environment_lighting_resources,
        );
        update_graphics_set_fn(
            &self.graphics_pipelines.leaves_shadow_lod_ppl,
            "gui_input",
            &tracer_resources,
        );
        update_graphics_fn(
            &self.graphics_pipelines.sprinkler_ppl,
            &environment_lighting_resources,
        );
        update_graphics_fn(
            &self.graphics_pipelines.geometry_preview_ppl,
            &tracer_resources,
        );
        update_graphics_fn(
            &self
                .graphics_pipelines
                .environment_probe_visualization_depth_ppl,
            &environment_lighting_resources,
        );
        update_graphics_fn(
            &self
                .graphics_pipelines
                .environment_probe_visualization_overlay_ppl,
            &environment_lighting_resources,
        );
        update_graphics_fn(
            &self.graphics_pipelines.dynamic_fruit_ppl,
            &environment_lighting_resources,
        );
        update_graphics_fn(
            &self.graphics_pipelines.particle_ppl,
            &environment_lighting_resources,
        );
        update_graphics_fn(
            &self.graphics_pipelines.water_droplet_ppl,
            &environment_lighting_resources,
        );
        let ddgi_builder = self.ddgi_runtime.volumes().builder();
        let ddgi_resources: [&dyn ResourceContainer; 2] = [&self.resources, ddgi_builder];
        update_compute_fn(
            &self.compute_pipelines.ddgi_global_sky_filter_ppl,
            &ddgi_resources,
        );
        update_compute_fn(
            &self.compute_pipelines.ddgi_octahedral_gutter_ppl,
            &[ddgi_builder],
        );
        update_compute_fn(
            &self.compute_pipelines.ddgi_probe_relocate_ppl,
            &[plain_builder_resources, ddgi_builder],
        );
        update_compute_fn(
            &self.compute_pipelines.ddgi_probe_trace_ppl,
            &[
                &self.resources,
                contree_builder_resources,
                scene_accel_resources,
                plain_builder_resources,
                ddgi_builder,
                &self.ddgi_voxel_visibility,
            ],
        );
        update_compute_fn(
            &self.compute_pipelines.ddgi_voxel_visibility_pack_ppl,
            &[plain_builder_resources, &self.ddgi_voxel_visibility],
        );
        update_compute_fn(
            &self.compute_pipelines.ddgi_voxel_visibility_blocks_ppl,
            &[&self.ddgi_voxel_visibility],
        );
        for pipeline in [
            &self.compute_pipelines.ddgi_irradiance_filter_ppl,
            &self.compute_pipelines.ddgi_visibility_filter_ppl,
            &self.compute_pipelines.ddgi_irradiance_gutter_ppl,
            &self.compute_pipelines.ddgi_visibility_gutter_ppl,
            &self.compute_pipelines.ddgi_atlas_reduce_ppl,
        ] {
            update_compute_fn(pipeline, &[ddgi_builder]);
        }
    }

    fn next_descriptor_generation(&mut self) -> u64 {
        let generation = self.descriptor_generation;
        self.descriptor_generation = self
            .descriptor_generation
            .checked_add(1)
            .expect("tracer descriptor generation overflow");
        generation
    }

    fn retire_tree_instance<T: 'static>(&mut self, resident: T) {
        let generation = self.tree_instance_generation;
        self.tree_instance_generation = self
            .tree_instance_generation
            .checked_add(1)
            .expect("tracer tree instance generation overflow");
        self.frame_retirement_sink.retire(FrameRetirement::new(
            "tracer.tree_instances",
            generation,
            resident,
        ));
    }

    fn tracer_descriptor_resources(&self) -> [&dyn ResourceContainer; 3] {
        [
            &self.resources as &dyn ResourceContainer,
            self.ddgi_runtime.volumes().active() as &dyn ResourceContainer,
            &self.ddgi_voxel_visibility as &dyn ResourceContainer,
        ]
    }

    fn environment_lighting_descriptor_resources(&self) -> [&dyn ResourceContainer; 3] {
        [
            &self.resources as &dyn ResourceContainer,
            self.ddgi_runtime.volumes().active() as &dyn ResourceContainer,
            &self.ddgi_voxel_visibility as &dyn ResourceContainer,
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
            self.ddgi_runtime.volumes().active() as &dyn ResourceContainer,
            &self.ddgi_voxel_visibility as &dyn ResourceContainer,
        ]
    }

    fn update_ddgi_builder_descriptors(
        &self,
        ddgi_volume: &DdgiVolume,
        inherited_source: Option<&DdgiVolume>,
        generation: u64,
    ) -> Vec<FrameRetirement> {
        let mut relocate = Vec::new();
        let mut trace = Vec::new();
        let mut irradiance_filter = Vec::new();
        let mut visibility_filter = Vec::new();
        let mut atlas_reduce = Vec::new();
        let mut global_sky_filter = Vec::new();
        let mut octahedral_gutter = Vec::new();
        let mut irradiance_gutter = Vec::new();
        let mut visibility_gutter = Vec::new();

        macro_rules! write_buffer {
            ($writes:expr, $name:literal, $buffer:expr) => {
                $writes.push(DescriptorWrite {
                    name: $name,
                    resource: DescriptorResource::Buffer($buffer),
                });
            };
        }
        macro_rules! write_texture {
            ($writes:expr, $name:literal, $texture:expr) => {
                $writes.push(DescriptorWrite {
                    name: $name,
                    resource: DescriptorResource::Texture($texture),
                });
            };
        }

        write_buffer!(
            relocate,
            "ddgi_probe_metadata",
            &ddgi_volume.ddgi_probe_metadata
        );
        write_buffer!(
            relocate,
            "ddgi_relocation_stats",
            &ddgi_volume.ddgi_relocation_stats
        );

        for writes in [
            &mut trace,
            &mut irradiance_filter,
            &mut visibility_filter,
            &mut atlas_reduce,
        ] {
            write_buffer!(
                writes,
                "ddgi_probe_metadata",
                &ddgi_volume.ddgi_probe_metadata
            );
        }
        for writes in [&mut trace, &mut irradiance_filter, &mut visibility_filter] {
            write_buffer!(
                writes,
                "ddgi_transient_ray_data",
                &ddgi_volume.ddgi_transient_ray_data
            );
        }
        write_buffer!(trace, "ddgi_trace_stats", &ddgi_volume.ddgi_trace_stats);
        write_buffer!(
            atlas_reduce,
            "ddgi_atlas_reduction",
            &ddgi_volume.ddgi_atlas_reduction
        );
        write_buffer!(
            global_sky_filter,
            "ddgi_radiance_sun",
            &ddgi_volume.ddgi_radiance_sun
        );
        write_buffer!(trace, "ddgi_radiance_sun", &ddgi_volume.ddgi_radiance_sun);
        write_buffer!(
            trace,
            "ddgi_radiance_voxel_palette",
            &ddgi_volume.ddgi_radiance_voxel_palette
        );
        write_buffer!(
            trace,
            "ddgi_transport_query_info",
            &ddgi_volume.ddgi_transport_query_info
        );
        write_buffer!(
            trace,
            "ddgi_local_light_info",
            &ddgi_volume.ddgi_local_light_info
        );
        write_buffer!(trace, "ddgi_local_lights", &ddgi_volume.ddgi_local_lights);

        let source_irradiance = inherited_source
            .and_then(DdgiVolume::published_irradiance_atlas)
            .unwrap_or(&ddgi_volume.ddgi_transport_source_irradiance_atlas);
        let source_visibility = inherited_source
            .and_then(DdgiVolume::published_visibility_atlas)
            .unwrap_or(&ddgi_volume.ddgi_transport_source_visibility_atlas);

        write_texture!(
            trace,
            "ddgi_transport_source_irradiance_atlas",
            source_irradiance
        );
        write_texture!(
            trace,
            "ddgi_global_sky_irradiance",
            ddgi_volume.building_global_sky_irradiance()
        );
        write_texture!(
            trace,
            "ddgi_irradiance_atlas",
            &ddgi_volume.ddgi_irradiance_atlas
        );
        write_texture!(
            trace,
            "ddgi_visibility_atlas",
            &ddgi_volume.ddgi_visibility_atlas
        );
        write_texture!(
            trace,
            "ddgi_transport_source_visibility_atlas",
            source_visibility
        );
        write_texture!(
            global_sky_filter,
            "ddgi_global_sky_irradiance",
            ddgi_volume.building_global_sky_irradiance()
        );
        write_texture!(
            octahedral_gutter,
            "ddgi_global_sky_irradiance",
            ddgi_volume.building_global_sky_irradiance()
        );
        for writes in [
            &mut irradiance_filter,
            &mut irradiance_gutter,
            &mut atlas_reduce,
        ] {
            write_texture!(
                writes,
                "ddgi_irradiance_atlas",
                &ddgi_volume.ddgi_irradiance_atlas
            );
            write_texture!(
                writes,
                "ddgi_transport_source_irradiance_atlas",
                source_irradiance
            );
        }
        for writes in [&mut visibility_filter, &mut visibility_gutter] {
            write_texture!(
                writes,
                "ddgi_visibility_atlas",
                &ddgi_volume.ddgi_visibility_atlas
            );
            write_texture!(
                writes,
                "ddgi_transport_source_visibility_atlas",
                source_visibility
            );
        }

        vec![
            self.compute_pipelines
                .ddgi_probe_relocate_ppl
                .publish_descriptors(
                    "ddgi.builder.descriptors",
                    generation,
                    DescriptorUpdate::Named(&relocate),
                )
                .expect("DDGI relocation descriptor update failed"),
            self.compute_pipelines
                .ddgi_probe_trace_ppl
                .publish_descriptors(
                    "ddgi.builder.descriptors",
                    generation,
                    DescriptorUpdate::Named(&trace),
                )
                .expect("DDGI trace descriptor update failed"),
            self.compute_pipelines
                .ddgi_irradiance_filter_ppl
                .publish_descriptors(
                    "ddgi.builder.descriptors",
                    generation,
                    DescriptorUpdate::Named(&irradiance_filter),
                )
                .expect("DDGI irradiance filter descriptor update failed"),
            self.compute_pipelines
                .ddgi_visibility_filter_ppl
                .publish_descriptors(
                    "ddgi.builder.descriptors",
                    generation,
                    DescriptorUpdate::Named(&visibility_filter),
                )
                .expect("DDGI visibility filter descriptor update failed"),
            self.compute_pipelines
                .ddgi_atlas_reduce_ppl
                .publish_descriptors(
                    "ddgi.builder.descriptors",
                    generation,
                    DescriptorUpdate::Named(&atlas_reduce),
                )
                .expect("DDGI atlas reduction descriptor update failed"),
            self.compute_pipelines
                .ddgi_global_sky_filter_ppl
                .publish_descriptors(
                    "ddgi.builder.descriptors",
                    generation,
                    DescriptorUpdate::Named(&global_sky_filter),
                )
                .expect("DDGI global sky filter descriptor update failed"),
            self.compute_pipelines
                .ddgi_octahedral_gutter_ppl
                .publish_descriptors(
                    "ddgi.builder.descriptors",
                    generation,
                    DescriptorUpdate::Named(&octahedral_gutter),
                )
                .expect("DDGI octahedral gutter descriptor update failed"),
            self.compute_pipelines
                .ddgi_irradiance_gutter_ppl
                .publish_descriptors(
                    "ddgi.builder.descriptors",
                    generation,
                    DescriptorUpdate::Named(&irradiance_gutter),
                )
                .expect("DDGI irradiance gutter descriptor update failed"),
            self.compute_pipelines
                .ddgi_visibility_gutter_ppl
                .publish_descriptors(
                    "ddgi.builder.descriptors",
                    generation,
                    DescriptorUpdate::Named(&visibility_gutter),
                )
                .expect("DDGI visibility gutter descriptor update failed"),
        ]
    }

    fn stage_ddgi_consumer_descriptors(
        &self,
        ddgi_volume: &DdgiVolume,
    ) -> PreparedDdgiConsumerDescriptors {
        let graphics_pipelines = [
            &self.graphics_pipelines.flora_ppl,
            &self.graphics_pipelines.flora_lod_ppl,
            &self.graphics_pipelines.leaves_ppl,
            &self.graphics_pipelines.leaves_lod_ppl,
            &self.graphics_pipelines.sprinkler_ppl,
            &self.graphics_pipelines.dynamic_fruit_ppl,
            &self.graphics_pipelines.particle_ppl,
            &self.graphics_pipelines.water_droplet_ppl,
            &self
                .graphics_pipelines
                .environment_probe_visualization_depth_ppl,
            &self
                .graphics_pipelines
                .environment_probe_visualization_overlay_ppl,
        ];
        let irradiance_atlas = ddgi_volume
            .published_irradiance_atlas()
            .unwrap_or(&ddgi_volume.ddgi_irradiance_atlas);
        let visibility_atlas = ddgi_volume
            .published_visibility_atlas()
            .unwrap_or(&ddgi_volume.ddgi_visibility_atlas);
        let mut tracer_writes = vec![DescriptorWrite {
            name: "ddgi_probe_metadata",
            resource: DescriptorResource::Buffer(&ddgi_volume.ddgi_probe_metadata),
        }];
        let mut flora_lighting_cache_writes = vec![DescriptorWrite {
            name: "ddgi_probe_metadata",
            resource: DescriptorResource::Buffer(&ddgi_volume.ddgi_probe_metadata),
        }];
        let mut tree_leaf_lighting_cache_writes = vec![DescriptorWrite {
            name: "ddgi_probe_metadata",
            resource: DescriptorResource::Buffer(&ddgi_volume.ddgi_probe_metadata),
        }];
        let mut graphics_writes = vec![DescriptorWrite {
            name: "ddgi_probe_metadata",
            resource: DescriptorResource::Buffer(&ddgi_volume.ddgi_probe_metadata),
        }];
        for (binding, texture) in [
            (
                "ddgi_global_sky_irradiance",
                ddgi_volume.published_global_sky_irradiance(),
            ),
            ("ddgi_irradiance_atlas", irradiance_atlas),
            ("ddgi_visibility_atlas", visibility_atlas),
        ] {
            let write = DescriptorWrite {
                name: binding,
                resource: DescriptorResource::Texture(texture),
            };
            tracer_writes.push(write);
            flora_lighting_cache_writes.push(write);
            tree_leaf_lighting_cache_writes.push(write);
            graphics_writes.push(write);
        }

        PreparedDdgiConsumerDescriptors {
            token_serial: ddgi_volume
                .status()
                .build_token
                .expect("staged DDGI consumer descriptors require a build token")
                .serial(),
            tracer: self
                .compute_pipelines
                .tracer_ppl
                .prepare_descriptors(DescriptorUpdate::Named(&tracer_writes))
                .expect("DDGI consumer tracer descriptor preparation failed"),
            tracer_glass: self
                .compute_pipelines
                .tracer_glass_ppl
                .prepare_descriptors(DescriptorUpdate::Named(&tracer_writes))
                .expect("DDGI consumer Glass tracer descriptor preparation failed"),
            flora_lighting_cache: self
                .compute_pipelines
                .flora_lighting_cache_ppl
                .prepare_descriptors(DescriptorUpdate::Named(&flora_lighting_cache_writes))
                .expect("DDGI consumer flora cache descriptor preparation failed"),
            tree_leaf_lighting_cache: self
                .compute_pipelines
                .tree_leaf_lighting_cache_ppl
                .prepare_descriptors(DescriptorUpdate::Named(&tree_leaf_lighting_cache_writes))
                .expect("DDGI consumer tree-leaf cache descriptor preparation failed"),
            graphics: graphics_pipelines
                .iter()
                .map(|pipeline| {
                    pipeline
                        .prepare_descriptors(DescriptorUpdate::Named(&graphics_writes))
                        .expect("DDGI consumer graphics descriptor preparation failed")
                })
                .collect(),
        }
    }

    fn publish_ddgi_consumer_descriptors(
        &self,
        prepared: PreparedDdgiConsumerDescriptors,
        generation: u64,
    ) -> Vec<FrameRetirement> {
        let graphics_pipelines = [
            &self.graphics_pipelines.flora_ppl,
            &self.graphics_pipelines.flora_lod_ppl,
            &self.graphics_pipelines.leaves_ppl,
            &self.graphics_pipelines.leaves_lod_ppl,
            &self.graphics_pipelines.sprinkler_ppl,
            &self.graphics_pipelines.dynamic_fruit_ppl,
            &self.graphics_pipelines.particle_ppl,
            &self.graphics_pipelines.water_droplet_ppl,
            &self
                .graphics_pipelines
                .environment_probe_visualization_depth_ppl,
            &self
                .graphics_pipelines
                .environment_probe_visualization_overlay_ppl,
        ];
        assert_eq!(
            prepared.graphics.len(),
            graphics_pipelines.len(),
            "DDGI consumer descriptor preparation pipeline order changed"
        );
        let mut retirements = Vec::with_capacity(4 + graphics_pipelines.len());
        retirements.push(
            self.compute_pipelines
                .tracer_ppl
                .publish_prepared_descriptors(
                    "ddgi.consumer.descriptors",
                    generation,
                    prepared.tracer,
                ),
        );
        retirements.push(
            self.compute_pipelines
                .tracer_glass_ppl
                .publish_prepared_descriptors(
                    "ddgi.consumer.descriptors",
                    generation,
                    prepared.tracer_glass,
                ),
        );
        retirements.push(
            self.compute_pipelines
                .flora_lighting_cache_ppl
                .publish_prepared_descriptors(
                    "ddgi.consumer.descriptors",
                    generation,
                    prepared.flora_lighting_cache,
                ),
        );
        retirements.push(
            self.compute_pipelines
                .tree_leaf_lighting_cache_ppl
                .publish_prepared_descriptors(
                    "ddgi.consumer.descriptors",
                    generation,
                    prepared.tree_leaf_lighting_cache,
                ),
        );
        for (pipeline, descriptor_sets) in graphics_pipelines.into_iter().zip(prepared.graphics) {
            retirements.push(pipeline.publish_prepared_descriptors(
                "ddgi.consumer.descriptors",
                generation,
                descriptor_sets,
            ));
        }
        retirements
    }

    fn update_ddgi_consumer_descriptors(
        &self,
        ddgi_volume: &DdgiVolume,
        generation: u64,
    ) -> Vec<FrameRetirement> {
        let prepared = self.stage_ddgi_consumer_descriptors(ddgi_volume);
        self.publish_ddgi_consumer_descriptors(prepared, generation)
    }

    fn promote_ready_ddgi_staging(&mut self) {
        let status = self.ddgi_runtime.volumes().status();
        if !status.staging_is_ready() {
            return;
        }
        let build_token = status
            .staging()
            .and_then(|staging| staging.build_token)
            .expect("every DDGI staging volume must carry its build token");
        if !self.ddgi_runtime.token_can_promote(build_token) {
            log::info!(
                "[DDGI] obsolete staging promotion skipped token_serial={} kind={:?} terrain_revision={} spacing_voxels={} coordinator={:?}",
                build_token.serial(),
                build_token.kind(),
                build_token.terrain_revision(),
                build_token.spacing_voxels(),
                self.ddgi_runtime.refresh_state(),
            );
            assert!(
                self.ddgi_runtime.finish_obsolete_volume_build(build_token),
                "completed obsolete DDGI staging token must release the single update slot"
            );
            self.prepared_ddgi_consumer_descriptors = None;
            return;
        }

        // Previous frames may still sample the active volume. Publish a new descriptor generation
        // and keep the old generation/resource owners on the frame-completion retirement clock
        // while the frame's shading constants move to the complete staging volume.
        let publication_started = Instant::now();
        let descriptor_generation = self.next_descriptor_generation();
        let prepared = self
            .prepared_ddgi_consumer_descriptors
            .take()
            .filter(|prepared| prepared.token_serial == build_token.serial());
        let descriptor_retirements = if let Some(prepared) = prepared {
            self.publish_ddgi_consumer_descriptors(prepared, descriptor_generation)
        } else {
            let builder = self.ddgi_runtime.volumes().builder();
            self.update_ddgi_consumer_descriptors(builder, descriptor_generation)
        };
        let descriptor_rebind_ms = publication_started.elapsed().as_secs_f64() * 1_000.0;
        for retirement in descriptor_retirements {
            self.frame_retirement_sink.retire(retirement);
        }
        // DdgiRuntime::promote_ready_volume performs promote_staging(build_token) and the
        // coordinator token promotion as one fail-fast transaction.
        let retired_active = self
            .ddgi_runtime
            .promote_ready_volume(build_token)
            .expect("ready DDGI staging volume must be promotable");
        let resource_swap_ms =
            publication_started.elapsed().as_secs_f64() * 1_000.0 - descriptor_rebind_ms;
        let active = self.ddgi_runtime.volumes().status().active();
        let published = active
            .published_field
            .expect("promoted staging volume must have a finite published field");
        let published_key = published.field();
        let published_slot = self
            .ddgi_runtime
            .volumes()
            .active()
            .published_irradiance_label()
            .expect("promoted staging volume must have a resident published field");
        let promoted_terrain_revision = active.relocated_terrain_revision.unwrap_or_default();
        let cleared_terrain_invalidation = build_token.kind() == DdgiBuildKind::Terrain;
        log::info!(
            "[DDGI] staging promoted token_serial={} kind={:?} spacing_voxels={} probes={} geometry_revision={} radiance_revision={} environment_revision={} published_state={:?} published_update_epoch={} published_slot={} published_source={:?} building={:?} cleared_terrain_invalidation={} stage={:?}",
            build_token.serial(),
            build_token.kind(),
            active.grid.spacing_voxels(),
            active.grid.probe_count(),
            promoted_terrain_revision,
            published_key.radiance_revision(),
            active.global_sky_revision,
            published_key.state(),
            published_key.update_epoch(),
            published_slot,
            published.source(),
            active.building_field,
            cleared_terrain_invalidation,
            active.stage,
        );
        log::info!(
            "[DDGI][PUBLICATION_TIMING] token_serial={} descriptor_rebind_ms={:.3} resource_swap_ms={:.3} total_publication_ms={:.3} descriptor_generation={}",
            build_token.serial(),
            descriptor_rebind_ms,
            resource_swap_ms.max(0.0),
            publication_started.elapsed().as_secs_f64() * 1_000.0,
            descriptor_generation,
        );
        log::info!(
            "[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial={} geometry_revision={} radiance_revision={} spacing_voxels={} published_slot={} state={:?} update_epoch={} source={:?} sampler=sampleDiffuseEnvironment shading_info=shared descriptor_seam=update_ddgi_consumer_descriptors",
            build_token.serial(),
            promoted_terrain_revision,
            published_key.radiance_revision(),
            active.grid.spacing_voxels(),
            published_slot,
            published_key.state(),
            published_key.update_epoch(),
            published.source(),
        );
        drop(retired_active);
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
        let descriptor_generation = self.next_descriptor_generation();
        let tracer_resources = self.tracer_descriptor_resources();
        let descriptor_retirement = self.compute_pipelines.wind_volume_ppl.publish_descriptors(
            "tracer.wind.descriptors",
            descriptor_generation,
            DescriptorUpdate::All(&tracer_resources),
        )?;
        self.frame_retirement_sink.retire(descriptor_retirement);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_buffers(
        &mut self,
        time_info: &TimeInfo,
        local_lights: &LocalLightSnapshot,
        flora_growth_override_enabled: bool,
        flora_growth_override: f32,
        dither_strength_lsb: f32,
        raster_flora_ddgi_lighting: bool,
        path_tracing_reference: bool,
        path_tracing_max_bounces: u32,
        path_tracing_ambient_light: Vec3,
        terrain_ray_origin_offset_world: f32,
        ddgi_receiver_visibility_bias_world: f32,
        ddgi_history_retention: f32,
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
        god_ray_temporal_blend_enabled: bool,
        god_ray_temporal_alpha: f32,
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
        self.promote_ready_ddgi_staging();
        let provisional_local_light_gpu = LocalLightGpuSnapshot::from_authoritative(
            local_lights,
            LocalLightBudget::point_lights(LOCAL_LIGHT_GPU_CAPACITY),
            0,
        );
        let provisional_payload = provisional_local_light_gpu.payload();
        let selection_changed = self.local_light_live_payload.map_or_else(
            || provisional_payload.count() > 0,
            |previous| !previous.selection_eq(provisional_payload),
        );
        let live_revision = if selection_changed {
            self.local_light_live_revision
                .unwrap_or(0)
                .wrapping_add(1)
                .max(1)
        } else {
            self.local_light_live_revision.unwrap_or(0)
        };
        let local_light_gpu = provisional_local_light_gpu
            .with_live_revision(live_revision)
            .with_flags(if self.desc.ddgi_local_light_trace_diagnostics_enabled {
                LOCAL_LIGHT_FLAG_DDGI_TRACE_DIAGNOSTICS
            } else {
                0
            });
        let metadata_changed = self.local_light_uploaded_source_revision
            != Some(local_lights.source_revision())
            || self.local_light_uploaded_registry_revision
                != Some(local_lights.registry_revision());
        if selection_changed || metadata_changed {
            self.resources
                .local_lighting
                .local_light_info
                .fill_uniform(&local_light_gpu.info)?;
            if selection_changed {
                self.resources
                    .local_lighting
                    .local_lights
                    .fill(&local_light_gpu.lights)?;
            }
            self.local_light_live_revision = Some(live_revision);
            self.local_light_live_payload = Some(local_light_gpu.payload());
            self.local_light_uploaded_source_revision = Some(local_lights.source_revision());
            self.local_light_uploaded_registry_revision = Some(local_lights.registry_revision());
            self.local_light_live_count = local_light_gpu.info.count;
            log::info!(
                "[LOCAL_LIGHT][LIVE] source_revision={} registry_revision={} live_gpu_revision={} count={} capacity={} overflow_count={} selection_changed={} direct_upload=true",
                local_lights.source_revision(),
                local_lights.registry_revision(),
                live_revision,
                local_light_gpu.info.count,
                local_light_gpu.info.capacity,
                local_light_gpu.info.overflow_count,
                selection_changed,
            );
            let mut overflow_groups =
                std::collections::BTreeMap::<(ProviderId, LocalLightOverflowReason), usize>::new();
            for overflow in &local_light_gpu.overflow {
                *overflow_groups
                    .entry((overflow.source.provider(), overflow.reason))
                    .or_insert(0usize) += 1;
            }
            for ((provider, reason), count) in overflow_groups {
                log::info!(
                    "[LOCAL_LIGHT][OVERFLOW] source_revision={} provider={} reason={} count={} explicit=true",
                    local_lights.source_revision(),
                    provider.get(),
                    reason.label(),
                    count,
                );
            }
            self.local_light_live_overflow = local_light_gpu.overflow.clone();
        }
        let terrain_ray_origin_offset_world = terrain_ray_origin_offset_world.max(0.0);
        let ddgi_receiver_visibility_bias_world = ddgi_receiver_visibility_bias_world.max(0.0);
        let view_mat = self.camera.get_view_mat();
        let proj_mat = self.camera.get_proj_mat();
        self.current_view_proj_mat = proj_mat * view_mat;
        let ddgi_world_extent = self
            .ddgi_runtime
            .volumes()
            .status()
            .active()
            .grid
            .world_extent_voxels();
        let camera_voxel = (self.camera.position() * self.desc.voxel_dim_per_chunk.as_vec3())
            .clamp(Vec3::ZERO, ddgi_world_extent.as_vec3())
            .round()
            .as_uvec3();
        self.ddgi_runtime
            .observe_camera_probe_priority(UAabb3::new(camera_voxel, camera_voxel));
        BufferUpdater::update_camera_info(
            &mut self.resources.uniforms.camera_info,
            view_mat,
            proj_mat,
        )?;

        // Shadow camera info. Shadow maps are rendered every frame while shadows
        // are enabled, so PCSS and VSM both use the latest light-space matrix.
        let shadow_light_space_change = self.direct_sun_shadows.observe_sun_direction(sun_dir);
        if shadow_light_space_change != DirectSunShadowLightSpaceChange::Unchanged {
            log::debug!(
                "[SHADOW][LIGHT_SPACE] change={:?} revision={} sun_direction={:?} history_policy=reset_all_no_cross_space_blend",
                shadow_light_space_change,
                self.direct_sun_shadows.light_space_revision(),
                sun_dir.normalize(),
            );
        }
        if self
            .direct_sun_shadows
            .camera_update_required(update_shadow_map)
        {
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
            BufferUpdater::update_camera_info(
                &mut self.resources.shadow.shadow_camera_info,
                shadow_view_mat,
                shadow_proj_mat,
            )?;
            self.direct_sun_shadows.mark_camera_updated();
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
        self.god_ray_temporal_blend_enabled = god_ray_temporal_blend_enabled;
        self.god_ray_temporal_alpha = god_ray_temporal_alpha.clamp(0.01, 1.0);

        BufferUpdater::update_post_processing_info(
            &self.resources,
            self.desc.scaling_factor,
            dither_strength_lsb,
        )?;

        BufferUpdater::update_voxel_colors(
            &self.resources,
            voxel_dirt_color,
            voxel_sand_color,
            voxel_cherry_wood_color,
            voxel_oak_wood_color,
            voxel_rock_color,
            voxel_color_variance,
            EMISSIVE_VOXEL_COLOR_SRGB,
            EMISSIVE_VOXEL_SURFACE_RADIANCE,
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
        self.raster_flora_ddgi_lighting = raster_flora_ddgi_lighting;
        self.ddgi_history_retention = ddgi_history_retention.clamp(0.0, 0.99);

        self.ensure_wind_source_buffer_capacity(wind_gui_params.sources.len())?;
        BufferUpdater::update_gui_input(
            &self.resources,
            flora_growth_override_enabled,
            flora_growth_override,
            raster_flora_ddgi_lighting,
            path_tracing_reference,
            path_tracing_max_bounces,
            path_tracing_ambient_light,
            terrain_ray_origin_offset_world,
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

        let environment_lighting = self.environment_lighting.update(
            DdgiRadianceSnapshot {
                sun_direction: sun_dir,
                sun_color,
                sun_luminance,
                terrain_ray_origin_offset_world,
                ddgi_receiver_visibility_bias_world,
                glass_experiment_enabled: self.desc.glass_experiment_enabled,
                glass_material_revision: if self.desc.glass_experiment_enabled {
                    crate::voxel_material::GLASS_EXPERIMENT_MATERIAL_REVISION
                } else {
                    0
                },
                voxel_palette: DdgiVoxelPaletteSnapshot {
                    dirt_color: voxel_dirt_color,
                    sand_color: voxel_sand_color,
                    cherry_wood_color: voxel_cherry_wood_color,
                    oak_wood_color: voxel_oak_wood_color,
                    rock_color: voxel_rock_color,
                    hash_color_variance: voxel_color_variance,
                    emissive_color: EMISSIVE_VOXEL_COLOR_SRGB,
                    emissive_radiance: EMISSIVE_VOXEL_SURFACE_RADIANCE,
                },
                local_lights: local_light_gpu.payload(),
            },
            time_info.time_since_start_duration(),
        );
        if environment_lighting.transport_published {
            log::info!(
                "[DDGI][LIGHTING] transport_published=true live_revision={} transport_revision={} source_live_revision={} revision_lag={} coalesced_live_revisions={} published_at_ms={} transport_age_ms={} change_reason={:?} sun_angle_degrees={:.4} sun_color_relative={:.5} sun_luminance_relative={:.5} non_solar_changed={} local_lights_changed={} local_light_source_revision={} local_light_count={} sun_direction={:?} sun_color={:?} sun_luminance={:.4}",
                environment_lighting.live.revision,
                environment_lighting.transport.revision,
                environment_lighting.transport.source_live_revision,
                environment_lighting.revision_lag(),
                environment_lighting.coalesced_live_revisions,
                environment_lighting.transport.published_at.as_millis(),
                environment_lighting
                    .transport_age(time_info.time_since_start_duration())
                    .as_millis(),
                environment_lighting.transport.change.reason,
                environment_lighting
                    .transport
                    .change
                    .delta
                    .sun_angle_radians
                    .to_degrees(),
                environment_lighting
                    .transport
                    .change
                    .delta
                    .sun_color_relative,
                environment_lighting
                    .transport
                    .change
                    .delta
                    .sun_luminance_relative,
                environment_lighting
                    .transport
                    .change
                    .delta
                    .non_solar_changed,
                environment_lighting
                    .transport
                    .change
                    .delta
                    .local_lights_changed,
                environment_lighting
                    .transport
                    .snapshot
                    .local_lights
                    .source_revision(),
                environment_lighting.transport.snapshot.local_lights.count(),
                environment_lighting.transport.snapshot.sun_direction,
                environment_lighting.transport.snapshot.sun_color,
                environment_lighting.transport.snapshot.sun_luminance,
            );
            if environment_lighting
                .transport
                .change
                .delta
                .local_lights_changed
            {
                let impact = environment_lighting
                    .transport
                    .change
                    .delta
                    .local_light_impact_bound
                    .map(|bound| {
                        local_light_impact_voxel_bound(
                            bound,
                            self.desc.voxel_dim_per_chunk,
                            self.ddgi_runtime
                                .volumes()
                                .status()
                                .active()
                                .grid
                                .world_extent_voxels(),
                        )
                    });
                self.ddgi_runtime.observe_lighting_impact_probe_priority(
                    environment_lighting.transport.revision,
                    impact,
                );
            }
        }
        let ddgi_status = self.ddgi_runtime.volumes().status().active();
        let ddgi_geometry_revision = self
            .ddgi_voxel_visibility
            .published_revision()
            .unwrap_or_default();
        // Keep the last complete active field available while a terrain replacement builds. The
        // coordinator retains the conservative pending bound for scheduling and diagnostics, but
        // consumers intentionally use the resident field until its replacement promotes.
        let ddgi_consumer_invalidation_voxel_bound = None;
        BufferUpdater::update_shading_info(
            &self.resources,
            environment_lighting.transport,
            ddgi_status.grid,
            self.desc.voxel_dim_per_chunk,
            self.ddgi_ready(),
            ddgi_geometry_revision,
            self.desc.environment_irradiance_capture_enabled,
            ddgi_status.irradiance_layout.tile_grid().x,
            ddgi_status.visibility_layout.tile_grid().x,
            self.desc.ddgi_debug_view.as_u32(),
            self.desc.ddgi_terrain_hard_origin.as_u32(),
            self.desc.glass_experiment_enabled,
            self.desc.glass_debug_view,
            ddgi_receiver_visibility_bias_world,
            ddgi_consumer_invalidation_voxel_bound,
        )?;
        self.ddgi_runtime
            .observe_authored_lighting(environment_lighting.transport);

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
        mut gpu_profiler: Option<&mut GpuProfiler>,
        gpu_profiler_frame_slot: usize,
    ) -> Result<()> {
        self.record_graphics_buffer_uses(cmdbuf, surface_resources);
        self.local_light_visibility_diagnostic
            .resolve_readback(&self.resources.local_lighting)?;
        if std::mem::take(&mut self.ddgi_relocation_stats_readback_pending) {
            let stats = self
                .ddgi_runtime
                .volumes()
                .builder()
                .update_relocation_stats_from_readback()?;
            anyhow::ensure!(
                stats.probes == stats.valid.saturating_add(stats.failed),
                "DDGI relocation stats probe partition is inconsistent: {stats:?}",
            );
            anyhow::ensure!(
                stats.valid
                    == stats
                        .fast_target
                        .saturating_add(stats.local_target)
                        .saturating_add(stats.outer_target)
                        .saturating_add(stats.outer_best_effort)
                        .saturating_add(stats.full_escape),
                "DDGI relocation stats path partition is inconsistent: {stats:?}",
            );
            log::info!(
                "[DDGI] relocation stats probes={} valid={} failed={} fast_target={} local_target={} outer_target={} outer_best_effort={} full_escape={} moved={} clearance_below_half_target={} clearance_half_to_target={} clearance_target={} clearance_sum={} distance_squared_twice_sum={}",
                stats.probes,
                stats.valid,
                stats.failed,
                stats.fast_target,
                stats.local_target,
                stats.outer_target,
                stats.outer_best_effort,
                stats.full_escape,
                stats.moved,
                stats.clearance_below_half_target,
                stats.clearance_half_to_target,
                stats.clearance_target,
                stats.clearance_sum,
                stats.distance_squared_twice_sum,
            );
        }

        if let Some(batch) = self.ddgi_trace_stats_readback_pending.take() {
            let volume = self.ddgi_runtime.volumes().builder();
            if !volume.pending_trace_stats_batch_is(batch) {
                log::warn!(
                    "[DDGI] stale trace-stat readback ignored batch={batch:?} builder_token={:?} builder_stage={:?} builder_complete={:?} builder_building={:?} builder_radiance_revision={:?}",
                    volume.status().build_token,
                    volume.status().stage,
                    volume.status().complete_field,
                    volume.status().building_field,
                    volume.status().radiance_revision,
                );
            } else {
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
                let filtered_probe_count = volume.status().filtered_probe_count;
                let probe_count = volume.status().grid.probe_count();
                let build_token = volume.status().build_token;
                let radiance_snapshot = volume
                    .radiance_snapshot()
                    .context("DDGI trace-stat readback lost its immutable radiance snapshot")?;
                if filtered_probe_count == batch.probe_count
                    || filtered_probe_count == probe_count
                    || filtered_probe_count.is_multiple_of(1_024)
                {
                    log::debug!(
                        "[DDGI] ray batch verified first_probe={} probes={} rays_per_probe={} records={} valid_probe_rays={} invalid_probe_rays={} misses={} frontface_hits={} backface_hits={} non_finite={} local_light_candidates={} local_light_visible={} local_light_occluded={} local_light_irradiance_luma_q8={} emissive_surface_hits={} emissive_surface_radiance_luma_q8={} terrain_revision={} token_serial={:?} radiance_revision={} state={:?} update_epoch={} source={:?}",
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
                        stats.local_light_candidates,
                        stats.local_light_visible,
                        stats.local_light_occluded,
                        stats.local_light_irradiance_luma_q8,
                        stats.emissive_surface_hits,
                        stats.emissive_surface_radiance_luma_q8,
                        batch.geometry_revision(),
                        build_token.map(DdgiBuildToken::serial),
                        batch.radiance_revision(),
                        batch.state(),
                        batch.update_epoch(),
                        batch.source(),
                    );
                }
                self.observe_ddgi_local_light_gpu_evidence(
                    batch,
                    stats,
                    probe_count,
                    radiance_snapshot,
                )?;
                let outcome = self
                    .ddgi_runtime
                    .volumes_mut()
                    .builder_mut()
                    .mark_trace_stats_verified(batch)?;
                match outcome {
                    DdgiVerifiedBatchOutcome::Continue => {}
                    DdgiVerifiedBatchOutcome::AwaitingAtlasValidation(identity) => {
                        let atlas_stats = self
                            .ddgi_runtime
                            .volumes()
                            .builder()
                            .update_atlas_validation_from_readback()?;
                        let classified_field = self
                            .ddgi_runtime
                            .volumes()
                            .builder()
                            .preview_validated_field(
                                identity,
                                atlas_stats,
                                DDGI_CONVERGENCE_POLICY,
                            )?;
                        let work = self
                            .ddgi_runtime
                            .volumes()
                            .builder()
                            .status()
                            .scheduled_work
                            .expect("validated DDGI epoch must retain scheduled work");
                        self.ddgi_runtime
                            .validate_transport_completion(work, classified_field)
                            .map_err(|error| {
                                anyhow::anyhow!(
                                    "DDGI scheduler rejected completion before publication: {error:?}"
                                )
                            })?;
                        let validated = self
                            .ddgi_runtime
                            .volumes_mut()
                            .builder_mut()
                            .mark_atlas_validated(identity, atlas_stats, DDGI_CONVERGENCE_POLICY)?;
                        let status = self.ddgi_runtime.volumes().builder().status();
                        let key = identity.field();
                        log::debug!(
                            "[DDGI] full-atlas validated token_serial={:?} geometry_revision={} radiance_revision={} spacing_voxels={} state={:?} update_epoch={} source={:?} source_slot={} destination_slot={} max_abs_rgb_delta={:.8} max_rel_rgb_delta={:.8} max_rgb_value={:.8} non_finite={} negative_rgb_texels={} valid_texels={} scanned_stored_texels={} abs_threshold={:.8} rel_threshold={:.8} consecutive_below={}/{} published_slot={:?} stage={:?}",
                            build_token.map(DdgiBuildToken::serial),
                            key.geometry_revision(),
                            key.radiance_revision(),
                            key.spacing_voxels(),
                            key.state(),
                            key.update_epoch(),
                            identity.source(),
                            batch.source_label(),
                            batch.destination_label(),
                            atlas_stats.max_absolute_rgb_delta,
                            atlas_stats.max_relative_rgb_delta,
                            atlas_stats.max_rgb_value,
                            atlas_stats.non_finite_count,
                            atlas_stats.negative_rgb_texel_count,
                            atlas_stats.valid_texel_count,
                            atlas_stats.scanned_stored_texel_count,
                            DDGI_CONVERGENCE_POLICY.absolute_threshold,
                            DDGI_CONVERGENCE_POLICY.relative_threshold,
                            status.consecutive_below_threshold,
                            DDGI_CONVERGENCE_POLICY.consecutive_epochs,
                            self.ddgi_runtime.volumes().builder().published_irradiance_label(),
                            status.stage,
                        );

                        let completed = match validated {
                            DdgiValidatedIterationOutcome::Published {
                                work,
                                field,
                                consecutive_below_threshold,
                            } => {
                                let field_key = field.field();
                                log::debug!(
                                    "[DDGI] finite field published serial={} state={:?} update_epoch={} slot={} source={:?} consecutive_below={} ready=true partial_next_never_visible=true",
                                    field_key.serial(),
                                    field_key.state(),
                                    field_key.update_epoch(),
                                    batch.destination_label(),
                                    field.source(),
                                    consecutive_below_threshold,
                                );
                                Some((work, field))
                            }
                            DdgiValidatedIterationOutcome::Converged {
                                work,
                                field,
                                reason,
                            } => {
                                log::info!(
                                    "[DDGI] transport converged serial={} update_epoch={} slot={} source={:?} reason={:?} ready=true",
                                    field.field().serial(),
                                    field.field().update_epoch(),
                                    batch.destination_label(),
                                    field.source(),
                                    reason,
                                );
                                Some((work, field))
                            }
                        };

                        if let Some((_, field)) = completed {
                            anyhow::ensure!(
                                field == classified_field,
                                "DDGI atlas classification changed during publication"
                            );
                        }

                        if let Some((work, field)) = completed {
                            let completed_build_token = build_token
                                .context("validated DDGI field has no volume build token")?;
                            self.ddgi_runtime
                                .complete_transport_work(work, field, completed_build_token)
                                .map_err(|error| {
                                    anyhow::anyhow!(
                                        "DDGI scheduler rejected validated completion: {error:?}"
                                    )
                                })?;
                            let lighting = self.ddgi_runtime.lighting_diagnostics();
                            log::debug!(
                                "[DDGI][PUBLICATION] serial={} geometry_revision={} radiance_revision={} update_epoch={} kind={:?} latest_transport_revision={:?} source_live_revision={:?} scheduler_published_revision={:?} revision_lag={} coalesced_revisions={} max_abs_rgb_delta={:.8} max_rel_rgb_delta={:.8} mixed_in_flight={}",
                                field.field().serial(),
                                field.field().geometry_revision(),
                                field.field().radiance_revision(),
                                field.field().update_epoch(),
                                work.kind(),
                                lighting.latest_transport_revision,
                                lighting.latest_source_live_revision,
                                lighting.scheduler_published_revision,
                                lighting.scheduler_revision_lag(),
                                lighting.coalesced_revisions,
                                atlas_stats.max_absolute_rgb_delta,
                                atlas_stats.max_relative_rgb_delta,
                                lighting.has_mixed_in_flight_revision,
                            );
                            if self.ddgi_runtime.volumes().builder_is_active() {
                                let descriptor_generation = self.next_descriptor_generation();
                                let descriptor_retirements = {
                                    let builder = self.ddgi_runtime.volumes().builder();
                                    self.update_ddgi_consumer_descriptors(
                                        builder,
                                        descriptor_generation,
                                    )
                                };
                                for retirement in descriptor_retirements {
                                    self.frame_retirement_sink.retire(retirement);
                                }
                                let slot = self
                                    .ddgi_runtime
                                    .volumes()
                                    .builder()
                                    .published_irradiance_label()
                                    .expect("validated DDGI field must be resident");
                                let key = field.field();
                                log::debug!(
                                    "[DDGI][CONSUMERS] atomically rebound published_slot={} state={:?} update_epoch={} token_serial={:?} geometry_revision={} radiance_revision={} spacing_voxels={} source={:?}",
                                    slot,
                                    key.state(),
                                    key.update_epoch(),
                                    build_token.map(DdgiBuildToken::serial),
                                    key.geometry_revision(),
                                    key.radiance_revision(),
                                    key.spacing_voxels(),
                                    field.source(),
                                );
                                log::debug!(
                                    "[ENV_LIGHTING] backend=ddgi ready=true geometry_revision={} state={:?} update_epoch={} radiance_revision={} slot={}",
                                    key.geometry_revision(),
                                    key.state(),
                                    key.update_epoch(),
                                    key.radiance_revision(),
                                    slot,
                                );
                            }
                            self.observe_ddgi_capture_checkpoint(
                                completed_build_token,
                                field,
                                atlas_stats,
                                DdgiCapturePublication::Published,
                            );
                        }
                    }
                }
            }
        }

        self.start_next_ddgi_scheduled_work()?;

        self.graphics_pipelines
            .begin_transient_descriptor_frame(gpu_profiler_frame_slot);

        // VSM filtering writes shadow_map_tex_for_vsm_ping in compute, then the
        // flora vertex shader samples it in the same command buffer. MoltenVK/Metal
        // needs the write made visible to graphics explicitly; a compute->compute
        // barrier is not enough and causes close grass shadow flicker on macOS.
        let compute_to_graphics_barrier = PipelineBarrier::shader_access(
            PipelineStage::COMPUTE_SHADER,
            PipelineStage::VERTEX_SHADER,
        );

        Self::with_gpu_scope(
            gpu_profiler.as_deref_mut(),
            gpu_profiler_frame_slot,
            cmdbuf,
            "clear.targets",
            || self.record_clear_render_targets(cmdbuf, render_flags, update_shadow_map),
        );

        self.ddgi_runtime
            .volumes()
            .builder()
            .record_cpu_buffer_writes(cmdbuf);

        if self.local_light_visibility_diagnostic.has_queued() {
            let diagnostic = &mut self.local_light_visibility_diagnostic;
            let resources = &self.resources.local_lighting;
            let pipeline = &self.compute_pipelines.local_light_visibility_diagnostic_ppl;
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "local_light.fixed_visibility_diagnostic",
                || diagnostic.record(resources, pipeline, cmdbuf),
            );
        }

        if let Some(lighting) = self.ddgi_runtime.in_flight_authored_lighting() {
            let revision = lighting.revision;
            if self
                .ddgi_runtime
                .volumes()
                .builder()
                .should_latch_radiance_snapshot(revision)
            {
                self.ddgi_runtime
                    .volumes_mut()
                    .builder_mut()
                    .latch_radiance_snapshot(revision, lighting.snapshot)?;
                log::info!(
                    "[DDGI] radiance snapshot latched revision={} stage={:?}",
                    revision,
                    self.ddgi_runtime.volumes().status().builder().stage,
                );
            }
        }

        if self.ddgi_runtime.frame_plan().global_sky_needs_update {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.global_sky_filter",
                || self.record_ddgi_global_sky_filter_pass(cmdbuf),
            );
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.global_sky_gutter",
                || self.record_ddgi_global_sky_gutter_pass(cmdbuf),
            );
            let environment_revision = self
                .ddgi_runtime
                .volumes()
                .status()
                .builder()
                .radiance_revision
                .expect("a DDGI global-sky pass requires a latched radiance snapshot");
            self.ddgi_runtime
                .mark_global_sky_ready(environment_revision)?;
            let status = self.ddgi_runtime.volumes().status().builder();
            log::info!(
                "[DDGI] global sky ready revision={} interior={}x{} stored={}x{} samples_per_texel=2048 stage={:?}",
                environment_revision,
                DDGI_IRRADIANCE_INTERIOR_SIDE,
                DDGI_IRRADIANCE_INTERIOR_SIDE,
                DDGI_IRRADIANCE_STORED_SIDE,
                DDGI_IRRADIANCE_STORED_SIDE,
                status.stage,
            );
        }

        let ddgi_relocation_revision = self.ddgi_runtime.frame_plan().relocation_terrain_revision;
        if let Some(terrain_revision) = ddgi_relocation_revision {
            let volume = self.ddgi_runtime.volumes().builder();
            volume.ddgi_relocation_stats.record_fill(
                cmdbuf,
                0,
                volume.status().resource_bytes.relocation_stats,
                0,
            );
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.probe_relocate",
                || self.record_ddgi_probe_relocation_pass(cmdbuf, terrain_revision),
            );
            let volume = self.ddgi_runtime.volumes().builder();
            volume.record_relocation_stats_readback(cmdbuf);
            self.ddgi_relocation_stats_readback_pending = true;
            self.ddgi_runtime.mark_relocated(terrain_revision)?;
            let status = self.ddgi_runtime.volumes().status().builder();
            log::info!(
                "[DDGI] relocation complete terrain_revision={} probes={} spacing_voxels={} max_displacement_voxels={} min_clearance_voxels=1 preferred_clearance_voxels={} local_search_radius_voxels={} stage={:?}",
                terrain_revision,
                status.grid.probe_count(),
                status.grid.spacing_voxels(),
                status.grid.spacing_voxels() / 2,
                (status.grid.spacing_voxels() / 4).max(1),
                (status.grid.spacing_voxels() / 4).max(1),
                status.stage,
            );
        }

        if self
            .ddgi_runtime
            .frame_plan()
            .visibility_preservation_needed
        {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.visibility_preserve",
                || {
                    self.ddgi_runtime
                        .volumes()
                        .builder()
                        .record_visibility_preservation(cmdbuf)
                },
            );
            self.ddgi_runtime.mark_visibility_preserved();
            log::debug!(
                "[DDGI][VISIBILITY] preserved=true geometry_revision={} radiance_revision={:?} visibility_filter_skipped=true",
                self.ddgi_runtime
                    .volumes()
                    .status()
                    .builder()
                    .scheduled_work
                    .expect("visibility preservation requires scheduled work")
                    .destination()
                    .field()
                    .geometry_revision(),
                self.ddgi_runtime
                    .volumes()
                    .status()
                    .builder()
                    .radiance_revision,
            );
        }

        let ddgi_frame_plan = self.ddgi_runtime.frame_plan();
        let ddgi_ray_batch = ddgi_frame_plan.ray_batch;
        if let Some(batch) = ddgi_ray_batch {
            let iteration_will_complete = ddgi_frame_plan.iteration_will_complete;
            {
                let volume = self.ddgi_runtime.volumes().builder();
                volume.ddgi_trace_stats.record_fill(
                    cmdbuf,
                    0,
                    volume.status().resource_bytes.trace_stats,
                    0,
                );
                if iteration_will_complete {
                    volume.ddgi_atlas_reduction.record_fill(
                        cmdbuf,
                        0,
                        volume.status().resource_bytes.atlas_reduction,
                        0,
                    );
                }
            }
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.probe_trace",
                || self.record_ddgi_probe_trace_pass(cmdbuf, batch),
            );
            self.ddgi_runtime.mark_ray_batch_ready(batch);

            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.irradiance_filter",
                || self.record_ddgi_irradiance_filter_pass(cmdbuf, batch),
            );
            // Trace directions rotate once per full epoch, so irradiance and directional distance
            // moments update together into the same ping-pong slot.
            if batch.writes_visibility() {
                Self::with_gpu_scope(
                    gpu_profiler.as_deref_mut(),
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "ddgi.visibility_filter",
                    || self.record_ddgi_visibility_filter_pass(cmdbuf, batch),
                );
            }
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "ddgi.atlas_gutters",
                || self.record_ddgi_atlas_gutter_passes(cmdbuf, batch),
            );
            if iteration_will_complete {
                Self::with_gpu_scope(
                    gpu_profiler.as_deref_mut(),
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "ddgi.atlas_reduce",
                    || self.record_ddgi_atlas_reduction_pass(cmdbuf, batch),
                );
            }

            let volume = self.ddgi_runtime.volumes().builder();
            volume.record_trace_stats_readback(cmdbuf);
            if iteration_will_complete {
                volume.record_atlas_reduction_readback(cmdbuf);
            }
            self.ddgi_trace_stats_readback_pending = Some(batch);

            self.ddgi_runtime.mark_ray_batch_filtered(batch);
            let status = self.ddgi_runtime.volumes().status().builder();
            if status.filtered_probe_count == batch.probe_count
                || status.filtered_probe_count == status.grid.probe_count()
                || status.filtered_probe_count.is_multiple_of(1_024)
            {
                log::debug!(
                    "[DDGI] atlas batch complete first_probe={} probes={} rays_per_probe={} filtered={}/{} geometry_revision={} token_serial={:?} radiance_revision={} spacing_voxels={} state={:?} update_epoch={} source={:?} destination={} priority={:?} irradiance_history_retention={:.5} visibility_history_retention={:.5} visibility_written={} awaiting_trace_stats=true awaiting_atlas_validation={} stage={:?}",
                    batch.first_probe_index,
                    batch.probe_count,
                    crate::ddgi::DDGI_RAYS_PER_PROBE,
                    status.filtered_probe_count,
                    status.grid.probe_count(),
                    batch.geometry_revision(),
                    status.build_token.map(DdgiBuildToken::serial),
                    batch.radiance_revision(),
                    batch.spacing_voxels(),
                    batch.state(),
                    batch.update_epoch(),
                    batch.source(),
                    batch.destination_label(),
                    batch.probe_priority().map(|priority| priority.reason()),
                    batch.irradiance_history_retention(self.ddgi_history_retention),
                    batch.visibility_history_retention(self.ddgi_history_retention),
                    batch.writes_visibility(),
                    iteration_will_complete,
                    status.stage,
                );
            }
        }

        if render_flags.enable_flora {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "wind_volume.pass",
                || self.record_wind_volume_pass(cmdbuf, time),
            );
        }

        let direct_sun_update_plan =
            (render_flags.enable_shadows && update_shadow_map).then(|| {
                let dynamic_fruit_shadow_changed =
                    self.dynamic_fruit_resources.take_shadow_changed();
                self.direct_sun_shadows
                    .plan_update(dynamic_fruit_shadow_changed)
            });

        if render_flags.enable_flora
            && render_flags.enable_leaves
            && render_flags.enable_leaf_shadows
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
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "leaf_shadow_temporal.pass",
                || {
                    self.record_leaf_shadow_temporal_pass(
                        cmdbuf,
                        leaf_shadow_temporal_alpha,
                        direct_sun_update_plan
                            .expect("enabled shadow update must have a history plan")
                            .reset_leaf_history(),
                    )
                },
            );
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "leaf_shadow_mask.pass",
                || self.record_leaf_shadow_mask_pass(cmdbuf),
            );
            self.record_store_leaf_shadow_history(cmdbuf);
            self.direct_sun_shadows.mark_leaf_history_recorded();
        }
        if render_flags.enable_shadows && update_shadow_map {
            if self.dynamic_fruit_resources.instance_count > 0 {
                Self::with_gpu_scope(
                    gpu_profiler.as_deref_mut(),
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "dynamic_fruit_shadow.pass",
                    || self.record_dynamic_fruit_shadow_pass(cmdbuf),
                );
            }
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "shadow_depth_copy.pass",
                || self.record_shadow_depth_copy_pass(cmdbuf),
            );
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "tracer_shadow.pass",
                || self.record_tracer_shadow_pass(cmdbuf),
            );
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
                        direct_sun_update_plan
                            .expect("enabled shadow update must have a history plan")
                            .reset_terrain_history(),
                    )
                },
            );
            self.direct_sun_shadows.mark_terrain_history_recorded();
            if render_flags.enable_clouds {
                let reset_cloud_history = self.direct_sun_shadows.cloud_history_reset_required();
                Self::with_gpu_scope(
                    gpu_profiler.as_deref_mut(),
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "cloud_shadow.pass",
                    || self.record_cloud_shadow_pass(cmdbuf, reset_cloud_history),
                );
                self.record_store_cloud_shadow_history(cmdbuf);
                self.direct_sun_shadows.mark_cloud_history_recorded();
            } else {
                self.direct_sun_shadows.invalidate_cloud_history();
            }
            compute_to_graphics_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        Ok(())
    }

    /// Declares explicit vertex/index uses and CPU writes before graphics render passes.
    ///
    /// Reflected storage/uniform accesses belong to the descriptor runtime. This seam deliberately
    /// retains only operation-local vertex/index semantics and producer-side HostWrite state.
    fn record_graphics_buffer_uses(
        &self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
    ) {
        let record_index = |buffer: &Buffer| cmdbuf.use_buffer(buffer, BufferUse::IndexRead);
        let record_vertex = |buffer: &Buffer| cmdbuf.use_buffer(buffer, BufferUse::VertexRead);
        let record_mesh = |indices: &Buffer, vertices: &Buffer, indices_len: u32| {
            if indices_len > 0 {
                record_index(indices);
                record_vertex(vertices);
            }
        };
        let record_instance = |buffer: &Buffer| {
            cmdbuf.use_buffer(buffer, BufferUse::HostWrite);
            cmdbuf.use_buffer(buffer, BufferUse::VertexRead);
        };

        record_vertex(&self.resources.meshes.terrain_depth_prefill_vertices);
        for mesh in &self.resources.meshes.flora_meshes {
            record_mesh(&mesh.indices, &mesh.vertices, mesh.indices_len);
        }
        for mesh in &self.resources.meshes.flora_meshes_lod {
            record_mesh(&mesh.indices, &mesh.vertices, mesh.indices_len);
        }
        record_mesh(
            &self.resources.meshes.leaves_resources.indices,
            &self.resources.meshes.leaves_resources.vertices,
            self.resources.meshes.leaves_resources.indices_len,
        );
        record_mesh(
            &self.resources.meshes.leaves_resources_lod.indices,
            &self.resources.meshes.leaves_resources_lod.vertices,
            self.resources.meshes.leaves_resources_lod.indices_len,
        );
        record_mesh(
            &self.resources.meshes.apple_resources.indices,
            &self.resources.meshes.apple_resources.vertices,
            self.resources.meshes.apple_resources.indices_len,
        );
        record_mesh(
            &self.resources.meshes.apple_resources_lod.indices,
            &self.resources.meshes.apple_resources_lod.vertices,
            self.resources.meshes.apple_resources_lod.indices_len,
        );
        record_mesh(
            &self.sprinkler_resources.indices,
            &self.sprinkler_resources.vertices,
            self.sprinkler_resources.indices_len,
        );
        record_mesh(
            &self.irrigation_pipe_resources.indices,
            &self.irrigation_pipe_resources.vertices,
            self.irrigation_pipe_resources.indices_len,
        );
        record_mesh(
            &self.geometry_preview_resources.pipe.indices,
            &self.geometry_preview_resources.pipe.vertices,
            self.geometry_preview_resources.pipe.indices_len,
        );
        record_mesh(
            &self.geometry_preview_resources.tree.indices,
            &self.geometry_preview_resources.tree.vertices,
            self.geometry_preview_resources.tree.indices_len,
        );
        if self.environment_probe_visualization.enabled {
            record_mesh(
                &self
                    .environment_probe_visualization_resources
                    .marker_indices,
                &self
                    .environment_probe_visualization_resources
                    .marker_vertices,
                self.environment_probe_visualization_resources.index_count(),
            );
        }
        record_mesh(
            &self.dynamic_fruit_resources.indices,
            &self.dynamic_fruit_resources.vertices,
            self.dynamic_fruit_resources.indices_len,
        );
        record_mesh(
            &self.particle_resources.indices,
            &self.particle_resources.vertices,
            self.particle_resources.indices_len,
        );
        let glass = &self.resources.meshes.glass;
        record_mesh(&glass.indices, &glass.vertices, glass.indices_len);

        if self.dynamic_fruit_resources.instance_count > 0 {
            record_instance(&self.dynamic_fruit_resources.instances);
        }
        if self.particle_resources.instance_count > 0 {
            record_instance(&self.particle_resources.instance_buffer);
        }
        if self.particle_resources.translucent_instance_count > 0 {
            record_instance(&self.particle_resources.translucent_instance_buffer);
        }
        if self.sprinkler_resources.instance_count > 0 {
            record_instance(&self.sprinkler_resources.instances);
        }
        if self.irrigation_pipe_resources.instance_count > 0 {
            record_instance(&self.irrigation_pipe_resources.instances);
        }
        for preview in [
            &self.geometry_preview_resources.pipe,
            &self.geometry_preview_resources.tree,
        ] {
            if preview.instance_count > 0 {
                record_instance(&preview.instances);
            }
        }
        for (_, flora_resources) in &surface_resources.instances.chunk_flora_instances {
            if flora_resources.total_instance_len() > 0 {
                cmdbuf.use_buffer(
                    &flora_resources.resource.instances_buf,
                    BufferUse::HostWrite,
                );
            }
        }
        for tree_instances in surface_resources
            .instances
            .leaves_instances
            .values()
            .chain(surface_resources.instances.apple_instances.values())
        {
            if tree_instances.resources.instances_len > 0 {
                cmdbuf.use_buffer(
                    &tree_instances.resources.instances_buf,
                    BufferUse::HostWrite,
                );
            }
        }
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
        if render_flags.enable_tracer {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "tracer.pass",
                || self.record_tracer_pass(cmdbuf),
            );
            // Graphics pipelines declare their sampled Image uses before the render pass; those
            // transitions replace the former global compute-to-raster barrier.
        } else {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "tracer_clear.pass",
                || self.clear_tracer_outputs(cmdbuf),
            );
            // Clear transitions and the later composition pipeline own the transfer-to-shader
            // dependency; no global fallback barrier is needed here.
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
            // RenderTarget attachment state is committed by the recording transaction, and the
            // composition pipeline declares its shader reads after the render pass.
        }

        if render_flags.enable_god_rays {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "god_ray.pass",
                || self.record_god_ray_pass(cmdbuf),
            );
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "god_ray_temporal.pass",
                || self.record_god_ray_temporal_pass(cmdbuf),
            );
            if self.god_ray_temporal_blend_enabled {
                self.record_store_god_ray_history(cmdbuf);
                self.god_ray_history_valid = true;
            } else {
                self.god_ray_history_valid = false;
            }
        } else {
            self.god_ray_history_valid = false;
        }

        if render_flags.enable_clouds {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "cloud.pass",
                || self.record_cloud_pass(cmdbuf),
            );
            self.record_store_cloud_history(cmdbuf);
        } else {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "cloud_clear.pass",
                || self.clear_cloud_output(cmdbuf),
            );
        }
        if render_flags.enable_lens_flare {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "lens_flare_sun_visible.pass",
                || self.record_lens_flare_sun_visible_pass(cmdbuf),
            );
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "lens_flare.pass",
                || self.record_lens_flare_pass(cmdbuf),
            );
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "lens_flare_temporal.pass",
                || self.record_lens_flare_temporal_pass(cmdbuf),
            );
            self.record_store_lens_flare_history(cmdbuf);
            self.lens_flare_history_valid = true;
        } else {
            self.lens_flare_history_valid = false;
        }
        Self::with_gpu_scope(
            gpu_profiler.as_deref_mut(),
            gpu_profiler_frame_slot,
            cmdbuf,
            "composition.pass",
            || self.record_composition_pass(cmdbuf),
        );
        if self.desc.glass_experiment_enabled {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "glass.resolve",
                || self.record_glass_resolve_pass(cmdbuf),
            );
        }
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

    fn record_store_god_ray_history(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .god_ray_output_tex
            .get_image()
            .record_copy_to(
                cmdbuf,
                self.resources
                    .extent_dependent_resources
                    .god_ray_history_tex
                    .get_image(),
                TextureLayout::GENERAL,
                TextureLayout::GENERAL,
            );
    }

    fn record_store_lens_flare_history(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .lens_flare_output_tex
            .get_image()
            .record_copy_to(
                cmdbuf,
                self.resources
                    .extent_dependent_resources
                    .lens_flare_history_tex
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

        if !render_flags.enable_god_rays {
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
        }

        if !render_flags.enable_lens_flare {
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
        }

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
        &mut self,
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

        let flora_frame_plan = if enable_flora {
            FloraFramePlan::build(
                FloraFramePlanConfig {
                    camera_position: self.camera.position(),
                    view_projection: self.current_view_proj_mat,
                    lod_distance,
                    draw_distance: flora_draw_distance,
                    chunk_count: surface_resources.instances.chunk_flora_instances.len(),
                    species_count: flora_color_tables.len(),
                    max_cache_entries: FLORA_LIGHTING_CACHE_OFFSET_MASK + 1,
                },
                |chunk_index| {
                    surface_resources.instances.chunk_flora_instances[chunk_index]
                        .0
                        .clone()
                },
                |chunk_index, species_index| {
                    surface_resources.instances.chunk_flora_instances[chunk_index]
                        .1
                        .species_len(species_index)
                },
                |species_index| should_render_grass_species(species_index, grass_render_mode),
                |species_index, lod_state| match lod_state {
                    LodState::Lod0 => self.resources.meshes.flora_meshes[species_index].voxel_count,
                    LodState::Lod1 => {
                        self.resources.meshes.flora_meshes_lod[species_index].voxel_count
                    }
                },
            )
        } else {
            FloraFramePlan::default()
        };
        let required_flora_cache_entries = flora_frame_plan.required_cache_entries();
        let tree_foliage_frame_plan = TreeFoliageFramePlan::for_main(
            TreeFoliageMainConfig {
                camera_position: self.camera.position(),
                view_projection: self.current_view_proj_mat,
                lod_distance,
                draw_distance: flora_draw_distance,
                render_leaves: enable_flora && enable_leaves,
                render_apples: enable_flora,
                lighting_cache_start: required_flora_cache_entries,
                max_cache_entries: FLORA_LIGHTING_CACHE_OFFSET_MASK + 1,
            },
            surface_resources
                .instances
                .leaves_instances
                .iter()
                .map(|(&tree_id, instance)| TreeFoliageInput {
                    tree_id,
                    bounds: instance.aabb.clone(),
                    instance_count: instance.resources.instances_len,
                }),
            surface_resources
                .instances
                .apple_instances
                .iter()
                .map(|(&tree_id, instance)| TreeFoliageInput {
                    tree_id,
                    bounds: instance.aabb.clone(),
                    instance_count: instance.resources.instances_len,
                }),
        );
        let required_tree_leaf_cache_entries =
            tree_foliage_frame_plan.required_lighting_cache_entries();
        let required_lighting_cache_entries = required_flora_cache_entries
            .checked_add(required_tree_leaf_cache_entries)
            .expect("visible raster flora lighting cache entry count overflow");

        let flora_cache_buffer = if flora_lighting_cache_dispatch_enabled(
            self.raster_flora_ddgi_lighting,
            self.local_light_live_count > 0,
            required_lighting_cache_entries,
        ) {
            self.flora_lighting_cache.ensure_capacity(
                self.vulkan_ctx.device().clone(),
                self.allocator.clone(),
                gpu_profiler_frame_slot,
                required_lighting_cache_entries,
            );
            let cache_buffer = self.flora_lighting_cache.buffer(gpu_profiler_frame_slot);
            self.compute_pipelines
                .flora_lighting_cache_ppl
                .begin_transient_descriptor_frame(gpu_profiler_frame_slot);

            let cache_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.flora_lighting_cache",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            for &batch in flora_frame_plan.batches() {
                let species_index = batch.species_index();
                let lod_state = batch.lod_state();
                let instances =
                    &surface_resources.instances.chunk_flora_instances[batch.chunk_index()].1;
                let mesh = match lod_state {
                    LodState::Lod0 => &self.resources.meshes.flora_meshes[species_index],
                    LodState::Lod1 => &self.resources.meshes.flora_meshes_lod[species_index],
                };
                debug_assert_eq!(mesh.voxel_count, batch.mesh_voxel_count());
                let mut push_constant = flora_push_constant(
                    time,
                    species_index as u32,
                    instances.chunk_world_offset,
                    flora_color_tables[species_index],
                );
                push_constant.lighting_cache_location = flora_lighting_cache_location(
                    batch.lighting_cache_offset(),
                    batch.mesh_voxel_count(),
                    lod_state == LodState::Lod1,
                );
                push_constant.instance_ty =
                    flora_lighting_cache_instance_ty(species_index as u32, batch.instance_count());
                self.compute_pipelines
                    .flora_lighting_cache_ppl
                    .record_with_descriptors(
                        cmdbuf,
                        &[
                            (
                                "flora_instances",
                                DescriptorResource::Buffer(&instances.resource.instances_buf),
                            ),
                            (
                                "grass_growth_potential_levels",
                                DescriptorResource::Buffer(
                                    &instances.grass_growth_potential_levels,
                                ),
                            ),
                            (
                                "flora_lighting_cache",
                                DescriptorResource::Buffer(&cache_buffer),
                            ),
                            ("flora_vertices", DescriptorResource::Buffer(&mesh.vertices)),
                        ],
                        Extent3D::new(batch.mesh_voxel_count(), batch.instance_count(), 1),
                        Some(bytemuck::bytes_of(&push_constant)),
                    )
                    .expect("flora lighting transient descriptors must match reflection");
            }
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), cache_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
            if required_tree_leaf_cache_entries > 0 {
                self.compute_pipelines
                    .tree_leaf_lighting_cache_ppl
                    .begin_transient_descriptor_frame(gpu_profiler_frame_slot);
                let leaf_cache_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                    profiler.begin_scope(
                        gpu_profiler_frame_slot,
                        cmdbuf,
                        "graphics.leaf_lighting_cache",
                        PipelineStage::ALL_COMMANDS,
                    )
                });
                for &batch in tree_foliage_frame_plan
                    .batches()
                    .iter()
                    .filter(|batch| batch.kind() == TreeFoliageKind::Leaves)
                {
                    let tree_instance = tree_foliage_instance(
                        surface_resources,
                        batch,
                        TreeFoliageInstanceStream::Visible,
                    );
                    let instance_count = batch.instance_count();
                    let mut push_constant = flora_push_constant(
                        time,
                        LEAF_INSTANCE_TYPE,
                        tree_instance.chunk_world_offset,
                        leaf_color_tables,
                    );
                    push_constant.lighting_cache_location = flora_lighting_cache_location(
                        batch
                            .lighting_cache_offset()
                            .expect("every visible leaf batch must reserve cache entries"),
                        1,
                        false,
                    );
                    push_constant.instance_ty =
                        flora_lighting_cache_instance_ty(LEAF_INSTANCE_TYPE, instance_count);
                    self.compute_pipelines
                        .tree_leaf_lighting_cache_ppl
                        .record_with_descriptors(
                            cmdbuf,
                            &[
                                (
                                    "tree_leaf_instances",
                                    DescriptorResource::Buffer(
                                        &tree_instance.resources.instances_buf,
                                    ),
                                ),
                                (
                                    "flora_lighting_cache",
                                    DescriptorResource::Buffer(&cache_buffer),
                                ),
                            ],
                            Extent3D::new(instance_count, 1, 1),
                            Some(bytemuck::bytes_of(&push_constant)),
                        )
                        .expect("tree-leaf lighting transient descriptors must match reflection");
                }
                if let (Some(profiler), Some(scope)) =
                    (gpu_profiler.as_deref_mut(), leaf_cache_scope)
                {
                    profiler.end_scope(
                        gpu_profiler_frame_slot,
                        cmdbuf,
                        scope,
                        PipelineStage::ALL_COMMANDS,
                    );
                }
            }
            PipelineBarrier::shader_access(
                PipelineStage::COMPUTE_SHADER,
                PipelineStage::VERTEX_SHADER,
            )
            .record_insert(self.vulkan_ctx.device(), cmdbuf);
            Some(cache_buffer)
        } else {
            None
        };

        let prepared_flora_descriptors = flora_frame_plan
            .batches()
            .iter()
            .map(|batch| {
                let instances =
                    &surface_resources.instances.chunk_flora_instances[batch.chunk_index()].1;
                let pipeline = match batch.lod_state() {
                    LodState::Lod0 => &self.graphics_pipelines.flora_ppl,
                    LodState::Lod1 => &self.graphics_pipelines.flora_lod_ppl,
                };
                pipeline
                    .prepare_draw_descriptors(
                        cmdbuf,
                        &[
                            (
                                "flora_instances",
                                DescriptorResource::Buffer(&instances.resource.instances_buf),
                            ),
                            (
                                "grass_growth_potential_levels",
                                DescriptorResource::Buffer(
                                    &instances.grass_growth_potential_levels,
                                ),
                            ),
                            (
                                "flora_lighting_cache",
                                DescriptorResource::Buffer(match flora_cache_buffer.as_ref() {
                                    Some(buffer) => buffer.as_ref(),
                                    None => &instances.resource.instances_buf,
                                }),
                            ),
                        ],
                    )
                    .expect("flora draw descriptors must match reflection")
            })
            .collect::<Vec<_>>();

        let prepared_tree_foliage_batches = tree_foliage_frame_plan
            .batches()
            .iter()
            .copied()
            .map(|batch| {
                let instance = tree_foliage_instance(
                    surface_resources,
                    batch,
                    TreeFoliageInstanceStream::Visible,
                );
                let pipeline = match batch.lod_state() {
                    LodState::Lod0 => &self.graphics_pipelines.leaves_ppl,
                    LodState::Lod1 => &self.graphics_pipelines.leaves_lod_ppl,
                };
                let descriptors = pipeline
                    .prepare_draw_descriptors(
                        cmdbuf,
                        &[
                            (
                                "tree_leaf_instances",
                                DescriptorResource::Buffer(&instance.resources.instances_buf),
                            ),
                            (
                                "flora_lighting_cache",
                                DescriptorResource::Buffer(match flora_cache_buffer.as_ref() {
                                    Some(buffer) => buffer.as_ref(),
                                    None => &instance.resources.instances_buf,
                                }),
                            ),
                        ],
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "{:?} draw descriptors must match reflection: {error}",
                            batch.kind(),
                        )
                    });
                PreparedTreeFoliageBatch {
                    batch,
                    instance,
                    descriptors,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            prepared_tree_foliage_batches.len(),
            tree_foliage_frame_plan.batches().len(),
            "every tree foliage frame batch must have one paired prepared batch",
        );

        self.graphics_pipelines
            .terrain_depth_prefill_ppl
            .prepare_descriptor_resources(cmdbuf);
        if enable_flora {
            for pipeline in [
                &self.graphics_pipelines.flora_ppl,
                &self.graphics_pipelines.flora_lod_ppl,
                &self.graphics_pipelines.leaves_ppl,
                &self.graphics_pipelines.leaves_lod_ppl,
            ] {
                pipeline.prepare_descriptor_resources(cmdbuf);
            }
        }
        if self.sprinkler_resources.instance_count > 0
            || self.irrigation_pipe_resources.instance_count > 0
        {
            self.graphics_pipelines
                .sprinkler_ppl
                .prepare_descriptor_resources(cmdbuf);
        }
        if self.geometry_preview_resources.has_visible_mesh() {
            self.graphics_pipelines
                .geometry_preview_ppl
                .prepare_descriptor_resources(cmdbuf);
        }
        if self.dynamic_fruit_resources.instance_count > 0 {
            self.graphics_pipelines
                .dynamic_fruit_ppl
                .prepare_descriptor_resources(cmdbuf);
        }
        if enable_particles {
            self.graphics_pipelines
                .particle_ppl
                .prepare_descriptor_resources(cmdbuf);
            self.graphics_pipelines
                .water_droplet_ppl
                .prepare_descriptor_resources(cmdbuf);
        }
        if enable_glass {
            self.graphics_pipelines
                .glass_ppl
                .prepare_descriptor_resources(cmdbuf);
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
            let recorded_flora_instance_count = flora_frame_plan.instance_count();
            let flora_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.flora",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let mut prepared_flora_descriptors = prepared_flora_descriptors.iter();
            for group in flora_frame_plan.groups() {
                let species_index = group.species_index();
                let lod_state = group.lod_state();
                let pipeline = match lod_state {
                    LodState::Lod0 => &self.graphics_pipelines.flora_ppl,
                    LodState::Lod1 => &self.graphics_pipelines.flora_lod_ppl,
                };
                let mesh = match lod_state {
                    LodState::Lod0 => &self.resources.meshes.flora_meshes[species_index],
                    LodState::Lod1 => &self.resources.meshes.flora_meshes_lod[species_index],
                };
                pipeline.record_bind(cmdbuf);
                pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
                cmdbuf.bind_index_buffer_u32(&mesh.indices);
                cmdbuf.bind_vertex_buffers(0, &[&mesh.vertices]);

                for &batch in flora_frame_plan.group_batches(group) {
                    debug_assert_eq!(batch.species_index(), species_index);
                    debug_assert_eq!(batch.lod_state(), lod_state);
                    debug_assert_eq!(batch.mesh_voxel_count(), mesh.voxel_count);
                    let instances =
                        &surface_resources.instances.chunk_flora_instances[batch.chunk_index()].1;
                    let mut push_constant = flora_push_constant(
                        time,
                        species_index as u32,
                        instances.chunk_world_offset,
                        flora_color_tables[species_index],
                    );
                    push_constant.lighting_cache_location = flora_lighting_cache_location(
                        batch.lighting_cache_offset(),
                        batch.mesh_voxel_count(),
                        lod_state == LodState::Lod1,
                    );

                    pipeline.record_indexed_with_prepared_descriptors(
                        cmdbuf,
                        prepared_flora_descriptors
                            .next()
                            .expect("every flora frame batch must have prepared descriptors"),
                        mesh.indices_len,
                        batch.instance_count(),
                        0,
                        0,
                        FloraInstanceResources::species_offset(species_index),
                        Some(&PushConstantInfo {
                            shader_stage: vk::ShaderStageFlags::VERTEX,
                            push_constants: bytemuck::bytes_of(&push_constant).to_vec(),
                        }),
                    );
                }
            }
            debug_assert!(prepared_flora_descriptors.next().is_none());
            if self.raster_flora_ddgi_lighting && recorded_flora_instance_count > 0 {
                let active = self.ddgi_runtime.volumes().status().active();
                if let Some(token) = active.build_token.filter(|token| {
                    self.ddgi_flora_consumer_logged_token_serial != Some(token.serial())
                }) {
                    self.ddgi_flora_consumer_logged_token_serial = Some(token.serial());
                    log::info!(
                        "[DDGI][FLORA_CONSUMER] draw_recorded active_token_serial={} terrain_revision={} spacing_voxels={} instance_count={} cache_entries={} sampler=flora_lighting_cache shading_info=shared",
                        token.serial(),
                        active.relocated_terrain_revision.unwrap_or_default(),
                        active.grid.spacing_voxels(),
                        recorded_flora_instance_count,
                        required_flora_cache_entries,
                    );
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
                for group in tree_foliage_frame_plan
                    .groups()
                    .iter()
                    .filter(|group| group.kind() == TreeFoliageKind::Leaves)
                {
                    let lod_state = group.lod_state();
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
                    pipeline.record_bind(cmdbuf);
                    pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
                    cmdbuf.bind_index_buffer_u32(indices_buf);

                    let planned_batches = &tree_foliage_frame_plan.batches()[group.batch_range()];
                    let prepared_batches = &prepared_tree_foliage_batches[group.batch_range()];
                    debug_assert!(planned_batches
                        .iter()
                        .zip(prepared_batches)
                        .all(|(batch, prepared)| *batch == prepared.batch));
                    for prepared in prepared_batches {
                        let batch = prepared.batch;
                        assert_eq!(
                            prepared.instance.resources.instances_len,
                            batch.instance_count(),
                            "leaf frame batch tree {} instance count changed before draw",
                            batch.tree_id(),
                        );
                        let mut leaf_push = flora_push_constant(
                            time,
                            LEAF_INSTANCE_TYPE,
                            prepared.instance.chunk_world_offset,
                            leaf_color_tables,
                        );
                        leaf_push.lighting_cache_location = flora_lighting_cache_location(
                            batch
                                .lighting_cache_offset()
                                .expect("every visible leaf batch must reserve cache entries"),
                            1,
                            false,
                        );
                        cmdbuf.bind_vertex_buffers(0, &[vertices_buf]);
                        pipeline.record_indexed_with_prepared_descriptors(
                            cmdbuf,
                            &prepared.descriptors,
                            indices_len,
                            batch.instance_count(),
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
            for group in tree_foliage_frame_plan
                .groups()
                .iter()
                .filter(|group| group.kind() == TreeFoliageKind::Apples)
            {
                let lod_state = group.lod_state();
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
                pipeline.record_bind(cmdbuf);
                pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
                cmdbuf.bind_index_buffer_u32(indices_buf);

                let planned_batches = &tree_foliage_frame_plan.batches()[group.batch_range()];
                let prepared_batches = &prepared_tree_foliage_batches[group.batch_range()];
                debug_assert!(planned_batches
                    .iter()
                    .zip(prepared_batches)
                    .all(|(batch, prepared)| *batch == prepared.batch));
                for prepared in prepared_batches {
                    let batch = prepared.batch;
                    assert_eq!(
                        prepared.instance.resources.instances_len,
                        batch.instance_count(),
                        "apple frame batch tree {} instance count changed before draw",
                        batch.tree_id(),
                    );
                    let apple_push = flora_push_constant(
                        time,
                        APPLE_INSTANCE_TYPE,
                        prepared.instance.chunk_world_offset,
                        solid_flora_height_color_tables(APPLE_BOTTOM_COLOR, APPLE_TIP_COLOR),
                    );
                    cmdbuf.bind_vertex_buffers(0, &[vertices_buf]);
                    pipeline.record_indexed_with_prepared_descriptors(
                        cmdbuf,
                        &prepared.descriptors,
                        indices_len,
                        batch.instance_count(),
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
            .submitted_instance_count(
                self.ddgi_runtime
                    .volumes()
                    .status()
                    .active()
                    .grid
                    .probe_count(),
            );
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
            .prepare_descriptor_resources(cmdbuf);
        let pipeline = &self.graphics_pipelines.leaves_shadow_lod_ppl;
        let tree_foliage_frame_plan = TreeFoliageFramePlan::for_shadow(
            surface_resources
                .instances
                .leaves_instances
                .iter()
                .map(|(&tree_id, instance)| TreeFoliageInput {
                    tree_id,
                    bounds: instance.aabb.clone(),
                    instance_count: instance.resources.shadow_instances_len,
                }),
            surface_resources
                .instances
                .apple_instances
                .iter()
                .map(|(&tree_id, instance)| TreeFoliageInput {
                    tree_id,
                    bounds: instance.aabb.clone(),
                    instance_count: instance.resources.shadow_instances_len,
                }),
        );
        let prepared_tree_foliage_batches = tree_foliage_frame_plan
            .batches()
            .iter()
            .copied()
            .map(|batch| {
                let instance = tree_foliage_instance(
                    surface_resources,
                    batch,
                    TreeFoliageInstanceStream::Shadow,
                );
                let descriptors = pipeline
                    .prepare_draw_descriptors(
                        cmdbuf,
                        &[(
                            "tree_leaf_shadow_instances",
                            DescriptorResource::Buffer(&instance.resources.shadow_instances_buf),
                        )],
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "{:?} shadow draw descriptors must match reflection: {error}",
                            batch.kind(),
                        )
                    });
                PreparedTreeFoliageBatch {
                    batch,
                    instance,
                    descriptors,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            prepared_tree_foliage_batches.len(),
            tree_foliage_frame_plan.batches().len(),
            "every foliage shadow frame batch must have one paired prepared batch",
        );
        pipeline.record_bind(cmdbuf);

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

        for group in tree_foliage_frame_plan.groups() {
            debug_assert_eq!(group.lod_state(), LodState::Lod1);
            let (indices, vertices, indices_len, instance_type, color_tables) = match group.kind() {
                TreeFoliageKind::Leaves => (
                    &self.resources.meshes.leaves_resources_lod.indices,
                    &self.resources.meshes.leaves_resources_lod.vertices,
                    self.resources.meshes.leaves_resources_lod.indices_len,
                    LEAF_INSTANCE_TYPE,
                    leaf_color_tables,
                ),
                TreeFoliageKind::Apples => (
                    &self.resources.meshes.apple_resources_lod.indices,
                    &self.resources.meshes.apple_resources_lod.vertices,
                    self.resources.meshes.apple_resources_lod.indices_len,
                    APPLE_INSTANCE_TYPE,
                    solid_flora_height_color_tables(APPLE_BOTTOM_COLOR, APPLE_TIP_COLOR),
                ),
            };
            cmdbuf.bind_index_buffer_u32(indices);

            let planned_batches = &tree_foliage_frame_plan.batches()[group.batch_range()];
            let prepared_batches = &prepared_tree_foliage_batches[group.batch_range()];
            debug_assert!(planned_batches
                .iter()
                .zip(prepared_batches)
                .all(|(batch, prepared)| *batch == prepared.batch));
            for prepared in prepared_batches {
                let batch = prepared.batch;
                assert_eq!(
                    prepared.instance.resources.shadow_instances_len,
                    batch.instance_count(),
                    "foliage shadow batch {:?} tree {} instance count changed before draw",
                    batch.kind(),
                    batch.tree_id(),
                );
                let push_constant = flora_push_constant(
                    time,
                    instance_type,
                    prepared.instance.chunk_world_offset,
                    color_tables,
                );

                cmdbuf.bind_vertex_buffers(0, &[vertices]);
                pipeline.record_indexed_with_prepared_descriptors(
                    cmdbuf,
                    &prepared.descriptors,
                    indices_len,
                    batch.instance_count(),
                    0,
                    0,
                    0,
                    Some(&PushConstantInfo {
                        shader_stage: vk::ShaderStageFlags::VERTEX,
                        push_constants: bytemuck::bytes_of(&push_constant).to_vec(),
                    }),
                );
            }
        }

        self.render_target_leaf_shadow_opacity.record_end(cmdbuf);
    }

    fn record_dynamic_fruit_shadow_pass(&self, cmdbuf: &CommandBuffer) {
        let resources = &self.dynamic_fruit_resources;
        if resources.instance_count == 0 {
            return;
        }

        let pipeline = &self.graphics_pipelines.dynamic_fruit_shadow_ppl;
        pipeline.prepare_descriptor_resources(cmdbuf);

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

    fn record_ddgi_global_sky_filter_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.ddgi_global_sky_filter_ppl.record(
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
        self.compute_pipelines.ddgi_octahedral_gutter_ppl.record(
            cmdbuf,
            Extent3D::new(DDGI_IRRADIANCE_STORED_SIDE, DDGI_IRRADIANCE_STORED_SIDE, 1),
            None,
        );
    }

    fn record_ddgi_probe_relocation_pass(&self, cmdbuf: &CommandBuffer, terrain_revision: u32) {
        let volume = self.ddgi_runtime.volumes().builder();
        let grid = volume.status().grid;
        let push_constants = DdgiProbeRelocationPushConstants {
            grid_dimensions: grid.dimensions().to_array(),
            spacing_voxels: grid.spacing_voxels(),
            voxels_per_world_unit: self.desc.voxel_dim_per_chunk.as_vec3().to_array(),
            terrain_revision,
            glass_experiment_enabled: u32::from(self.desc.glass_experiment_enabled),
            _padding: [0; 3],
        };
        self.compute_pipelines.ddgi_probe_relocate_ppl.record(
            cmdbuf,
            Extent3D::new(grid.probe_count() * DDGI_RELOCATION_WORKGROUP_SIZE, 1, 1),
            Some(bytemuck::bytes_of(&push_constants)),
        );
    }

    fn record_ddgi_probe_trace_pass(&self, cmdbuf: &CommandBuffer, batch: DdgiRayBatch) {
        let far_distance_world = self.chunk_bound.dimensions().as_vec3().length() * 2.0;
        let (local_refresh_enabled, local_refresh_world_min, local_refresh_world_max) =
            self.ddgi_local_refresh_push_constants(batch);
        let push_constants = DdgiProbeTracePushConstants {
            first_probe_index: batch.first_probe_index,
            probe_count: batch.probe_count,
            terrain_revision: batch.geometry_revision(),
            has_history: u32::from(batch.source().is_some()),
            source_slot: batch.source_slot_index(),
            far_distance_world,
            _padding: [0; 2],
            epoch_rotation: batch.epoch_rotation(),
            local_refresh_enabled,
            local_refresh_world_min,
            local_refresh_world_max,
        };
        self.compute_pipelines.ddgi_probe_trace_ppl.record(
            cmdbuf,
            Extent3D::new(batch.probe_count * DDGI_TRACE_WORKGROUP_SIZE, 1, 1),
            Some(bytemuck::bytes_of(&push_constants)),
        );
    }

    fn ddgi_local_refresh_push_constants(
        &self,
        batch: DdgiRayBatch,
    ) -> ([u32; 4], [f32; 4], [f32; 4]) {
        let Some(bound) = batch.local_refresh_voxel_bound() else {
            return ([0; 4], [0.0; 4], [0.0; 4]);
        };
        let voxels_per_world_unit = self.desc.voxel_dim_per_chunk.as_vec3();
        let world_min = bound.min().as_vec3() / voxels_per_world_unit;
        let world_max = bound.max().as_vec3() / voxels_per_world_unit;
        (
            [1, batch.local_recovery_epoch(), 0, 0],
            [world_min.x, world_min.y, world_min.z, 0.0],
            [world_max.x, world_max.y, world_max.z, 0.0],
        )
    }

    fn record_ddgi_irradiance_filter_pass(&self, cmdbuf: &CommandBuffer, batch: DdgiRayBatch) {
        let volume = self.ddgi_runtime.volumes().builder();
        let (local_refresh_enabled, local_refresh_world_min, local_refresh_world_max) =
            self.ddgi_local_refresh_push_constants(batch);
        let push_constants = DdgiAtlasFilterPushConstants {
            first_probe_index: batch.first_probe_index,
            probe_count: batch.probe_count,
            tile_columns: volume.status().irradiance_layout.tile_grid().x,
            terrain_revision: batch.geometry_revision(),
            destination_is_transport_source: u32::from(batch.destination_is_transport_source()),
            source_slot: batch.source_slot_index(),
            has_history: u32::from(batch.irradiance_history_is_valid()),
            history_retention: batch.irradiance_history_retention(self.ddgi_history_retention),
            epoch_rotation: batch.epoch_rotation(),
            local_refresh_enabled,
            local_refresh_world_min,
            local_refresh_world_max,
        };
        self.compute_pipelines.ddgi_irradiance_filter_ppl.record(
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
        let volume = self.ddgi_runtime.volumes().builder();
        let grid = volume.status().grid;
        let spacing_world =
            Vec3::splat(grid.spacing_voxels() as f32) / self.desc.voxel_dim_per_chunk.as_vec3();
        let far_distance_world = self.chunk_bound.dimensions().as_vec3().length() * 2.0;
        let (local_refresh_enabled, local_refresh_world_min, local_refresh_world_max) =
            self.ddgi_local_refresh_push_constants(batch);
        let push_constants = DdgiVisibilityFilterPushConstants {
            first_probe_index: batch.first_probe_index,
            probe_count: batch.probe_count,
            tile_columns: volume.status().visibility_layout.tile_grid().x,
            terrain_revision: batch.geometry_revision(),
            spacing_world: spacing_world.to_array(),
            far_distance_world,
            destination_slot: batch.destination_slot_index(),
            source_slot: batch.source_slot_index(),
            has_history: u32::from(batch.visibility_history_is_valid()),
            history_retention: batch.visibility_history_retention(self.ddgi_history_retention),
            epoch_rotation: batch.epoch_rotation(),
            local_refresh_enabled,
            local_refresh_world_min,
            local_refresh_world_max,
        };
        self.compute_pipelines.ddgi_visibility_filter_ppl.record(
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
        let volume = self.ddgi_runtime.volumes().builder();
        let irradiance_push = DdgiAtlasGutterPushConstants {
            first_probe_index: batch.first_probe_index,
            probe_count: batch.probe_count,
            tile_columns: volume.status().irradiance_layout.tile_grid().x,
            destination_is_transport_source: u32::from(batch.destination_is_transport_source()),
        };
        self.compute_pipelines.ddgi_irradiance_gutter_ppl.record(
            cmdbuf,
            Extent3D::new(batch.probe_count * DDGI_GUTTER_WORKGROUP_SIZE, 1, 1),
            Some(bytemuck::bytes_of(&irradiance_push)),
        );

        if batch.writes_visibility() {
            let visibility_push = DdgiAtlasGutterPushConstants {
                tile_columns: volume.status().visibility_layout.tile_grid().x,
                destination_is_transport_source: batch.destination_slot_index(),
                ..irradiance_push
            };
            self.compute_pipelines.ddgi_visibility_gutter_ppl.record(
                cmdbuf,
                Extent3D::new(batch.probe_count * DDGI_GUTTER_WORKGROUP_SIZE, 1, 1),
                Some(bytemuck::bytes_of(&visibility_push)),
            );
        }
    }

    fn record_ddgi_atlas_reduction_pass(&self, cmdbuf: &CommandBuffer, batch: DdgiRayBatch) {
        let volume = self.ddgi_runtime.volumes().builder();
        let source = batch.source();
        let push_constants = DdgiAtlasReductionPushConstants {
            terrain_revision: batch.geometry_revision(),
            probe_count: volume.status().grid.probe_count(),
            tile_columns: volume.status().irradiance_layout.tile_grid().x,
            destination_slot: batch.destination_slot_index(),
            source_slot: batch.source_slot_index(),
            has_source: u32::from(source.is_some()),
            relative_floor: DDGI_CONVERGENCE_POLICY.relative_floor,
            _padding: 0,
        };
        let stored_texel_count =
            volume.status().grid.probe_count() * DDGI_IRRADIANCE_STORED_SIDE.pow(2);
        let workgroup_count = stored_texel_count.div_ceil(DDGI_ATLAS_REDUCTION_WORKGROUP_SIZE);
        self.compute_pipelines.ddgi_atlas_reduce_ppl.record(
            cmdbuf,
            Extent3D::new(workgroup_count * DDGI_ATLAS_REDUCTION_WORKGROUP_SIZE, 1, 1),
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
        &self,
        cmdbuf: &CommandBuffer,
        temporal_alpha: f32,
        reset_leaf_shadow_history: bool,
    ) {
        let push_constants = PushConstantLeafShadowTemporal {
            temporal_alpha: temporal_alpha.clamp(0.0, 1.0),
            reset_history: u32::from(reset_leaf_shadow_history),
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
        &self,
        cmdbuf: &CommandBuffer,
        vsm_blur_radius: u32,
        vsm_temporal_alpha: f32,
        reset_vsm_history: bool,
    ) {
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

        let push_constants = VsmFilterPushConstants {
            blur_radius: vsm_blur_radius,
            temporal_alpha: vsm_temporal_alpha.clamp(0.0, 1.0),
            reset_history: u32::from(reset_vsm_history),
            _pad0: 0,
        };
        let push_constants_bytes = bytemuck::bytes_of(&push_constants);
        self.compute_pipelines
            .vsm_blur_h_ppl
            .record(cmdbuf, extent, Some(push_constants_bytes));

        self.compute_pipelines
            .vsm_blur_v_ppl
            .record(cmdbuf, extent, Some(push_constants_bytes));

        self.record_store_vsm_history(cmdbuf);
    }

    fn record_tracer_pass(&self, cmdbuf: &CommandBuffer) {
        let pipeline = if self.desc.glass_experiment_enabled {
            &self.compute_pipelines.tracer_glass_ppl
        } else {
            &self.compute_pipelines.tracer_ppl
        };
        pipeline.record(
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
        self.resources
            .extent_dependent_resources
            .glass_front_depth_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([1.0, 0.0, 0.0, 0.0])),
            );
        self.resources
            .extent_dependent_resources
            .glass_front_data_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
            );
    }

    fn record_god_ray_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.god_ray_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .god_ray_raw_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_god_ray_temporal_pass(&self, cmdbuf: &CommandBuffer) {
        let push_constants = GodRayTemporalPushConstants {
            reset_history: u32::from(!self.god_ray_history_valid),
            temporal_blend_enabled: u32::from(self.god_ray_temporal_blend_enabled),
            temporal_alpha: self.god_ray_temporal_alpha,
        };
        self.compute_pipelines.god_ray_temporal_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .god_ray_output_tex
                .get_image()
                .get_desc()
                .extent,
            Some(bytemuck::bytes_of(&push_constants)),
        );
    }

    fn record_cloud_shadow_pass(&self, cmdbuf: &CommandBuffer, reset_cloud_history: bool) {
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

        let push_constants = CloudShadowTemporalPushConstants {
            reset_history: u32::from(reset_cloud_history),
        };
        self.compute_pipelines.cloud_shadow_temporal_ppl.record(
            cmdbuf,
            extent,
            Some(bytemuck::bytes_of(&push_constants)),
        );
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
        self.direct_sun_shadows.invalidate_cloud_history();
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

    fn record_glass_resolve_pass(&self, cmdbuf: &CommandBuffer) {
        let resources = &self.resources.extent_dependent_resources;
        resources.glass_voxel_cache_metadata.record_fill(
            cmdbuf,
            0,
            u64::from(GLASS_VOXEL_CACHE_CAPACITY) * GLASS_VOXEL_CACHE_METADATA_BYTES_PER_ENTRY,
            0,
        );
        resources.glass_voxel_cache_active_count.record_fill(
            cmdbuf,
            0,
            GLASS_VOXEL_CACHE_ACTIVE_COUNT_BYTES,
            0,
        );
        let extent = resources.composited_tex.get_image().get_desc().extent;
        // Build a cell-key table from visible Glass pixels, shade its dense unique-cell list,
        // then composite cached transport back to pixels. Reflected resource tracking inserts
        // the transfer/compute and compute/compute barriers between these ordered dispatches.
        for pass in [GLASS_VOXEL_CACHE_BUILD_PASS, GLASS_VOXEL_CACHE_RESOLVE_PASS] {
            let push = PushConstantGlassResolve {
                pass,
                ..bytemuck::Zeroable::zeroed()
            };
            self.compute_pipelines.glass_resolve_ppl.record(
                cmdbuf,
                extent,
                Some(bytemuck::bytes_of(&push)),
            );
            if pass == GLASS_VOXEL_CACHE_BUILD_PASS {
                let push = PushConstantGlassResolve {
                    pass: GLASS_VOXEL_CACHE_SHADE_PASS,
                    ..bytemuck::Zeroable::zeroed()
                };
                self.compute_pipelines.glass_resolve_ppl.record(
                    cmdbuf,
                    Extent3D::new(GLASS_VOXEL_CACHE_CAPACITY, 1, 1),
                    Some(bytemuck::bytes_of(&push)),
                );
            }
        }
    }

    fn record_lens_flare_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.lens_flare_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .lens_flare_raw_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_lens_flare_temporal_pass(&self, cmdbuf: &CommandBuffer) {
        let push_constants = EffectTemporalPushConstants {
            reset_history: u32::from(!self.lens_flare_history_valid),
        };
        self.compute_pipelines.lens_flare_temporal_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .lens_flare_output_tex
                .get_image()
                .get_desc()
                .extent,
            Some(bytemuck::bytes_of(&push_constants)),
        );
    }

    fn record_lens_flare_sun_visible_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.lens_flare_sun_visible_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .lens_flare_raw_tex
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
        self.direct_sun_shadows.invalidate_local_histories();
        self.cloud_history_valid = false;
        self.god_ray_history_valid = false;
        self.lens_flare_history_valid = false;
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
            self.direct_sun_shadows.invalidate_local_histories();
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

    pub fn update_fly_camera(&mut self, frame_delta_time: f32) {
        self.camera.update_transform_fly_mode(frame_delta_time);
        self.spatial_sound_manager
            .update_player_pos(self.camera.position(), self.camera.vectors())
            .unwrap();
    }

    pub fn prepare_walk_camera_movement(
        &mut self,
        frame_delta_time: f32,
        sim_time_seconds: f64,
    ) -> crate::gameplay::camera::PlayerWalkMovementRequest {
        self.camera
            .prepare_walk_movement(frame_delta_time, sim_time_seconds)
    }

    pub fn apply_walk_camera_movement(
        &mut self,
        frame_delta_time: f32,
        sim_time_seconds: f64,
        request: crate::gameplay::camera::PlayerWalkMovementRequest,
        result: crate::gameplay::camera::PlayerWalkMovementResult,
    ) {
        self.camera
            .apply_walk_movement(frame_delta_time, sim_time_seconds, request, result);
        self.spatial_sound_manager
            .update_player_pos(self.camera.position(), self.camera.vectors())
            .unwrap();
    }

    pub fn take_footstep_events(&mut self) -> Vec<crate::gameplay::camera::FootstepEvent> {
        self.camera.take_footstep_events()
    }

    pub fn upload_sprinklers(&mut self, instances: &[SprinklerRenderInstance]) -> Result<()> {
        self.sprinkler_resources.upload(instances)
    }

    pub(crate) fn sprinkler_instance_count(&self) -> u32 {
        self.sprinkler_resources.instance_count
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

    pub fn show_dynamic_fruit_geometry(
        &mut self,
        instances: &[DynamicFruitRenderInstance],
    ) -> Result<()> {
        self.dynamic_fruit_resources.show(instances)
    }

    pub fn clear_dynamic_fruit_geometry(&mut self) {
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
                    crate::particles::ParticleRenderKind::TerrainVoxel => {
                        texture_layout.leaf_layer()
                    }
                },
            };
            match snap.kind {
                crate::particles::ParticleRenderKind::WaterDroplet => {
                    self.translucent_particle_instance_scratch.push(instance)
                }
                crate::particles::ParticleRenderKind::Leaf
                | crate::particles::ParticleRenderKind::Butterfly
                | crate::particles::ParticleRenderKind::TerrainVoxel => {
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
        shadow_instances: &[TreeShadowRenderInstanceData],
        aabb_margin: f32,
        span_label: &str,
    ) -> Result<TreeLeavesInstance> {
        use crate::builder::{TreeLeafInstance, TreeLeafShadowInstance};

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
        if let Some(max_shadow_pos) = shadow_instances
            .iter()
            .map(|instance| instance.world_pos)
            .reduce(UVec3::max)
        {
            let min_shadow_pos = shadow_instances
                .iter()
                .map(|instance| instance.world_pos)
                .reduce(UVec3::min)
                .expect("non-empty shadow instances must have a minimum");
            anyhow::ensure!(
                min_shadow_pos.cmpge(chunk_world_offset).all()
                    && (max_shadow_pos - chunk_world_offset)
                        .cmplt(UVec3::splat(1024))
                        .all(),
                "{} shadow instance span exceeds packed 10-bit local-position range",
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
        let shadow_instances_data = shadow_instances
            .iter()
            .map(|instance| {
                Ok(TreeLeafShadowInstance {
                    packed_local_pos: pack_local_pos(instance.world_pos),
                    packed_leaf_local_pos: Self::pack_tree_leaf_voxel_local_pos(
                        instance.leaf_local_pos,
                    )?,
                    billboard_size_voxels: instance.billboard_size_voxels,
                    opacity_layer_count: instance.opacity_layer_count,
                })
            })
            .collect::<Result<Vec<_>>>()?;

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
            shadow_instances.len() as u64,
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
        if !shadow_instances_data.is_empty() {
            tree_instance
                .resources
                .shadow_instances_buf
                .fill(&shadow_instances_data)?;
            tree_instance.resources.shadow_instances_len = shadow_instances_data.len() as u32;
        } else {
            tree_instance.resources.shadow_instances_len = 0;
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
        let leaf_shadow_proxies = build_leaf_shadow_proxies(&leaf_voxel_instances)?;
        let coarse_proxy_count = leaf_shadow_proxies
            .iter()
            .filter(|proxy| {
                proxy.billboard_size_voxels
                    == leaf_shadow_proxy::LEAF_SHADOW_PROXY_CELL_SIZE_VOXELS as f32
            })
            .count();
        let refined_proxy_count = leaf_shadow_proxies.len() - coarse_proxy_count;
        let shadow_instances = leaf_shadow_proxies
            .iter()
            .map(|proxy| TreeShadowRenderInstanceData {
                world_pos: proxy.world_pos,
                leaf_local_pos: proxy.leaf_local_pos,
                billboard_size_voxels: proxy.billboard_size_voxels,
                opacity_layer_count: proxy.opacity_layer_count,
            })
            .collect::<Vec<_>>();
        log::info!(
            "[LEAF_SHADOW_PROXY] tree={} sources={} proxies={} ratio={:.3} coarse_proxies={} refined_proxies={} coarse_cell_voxels={} fine_cell_voxels={} optical_area_preserved=true",
            tree_id,
            leaf_voxel_instances.len(),
            shadow_instances.len(),
            shadow_instances.len() as f32 / leaf_voxel_instances.len().max(1) as f32,
            coarse_proxy_count,
            refined_proxy_count,
            leaf_shadow_proxy::LEAF_SHADOW_PROXY_CELL_SIZE_VOXELS,
            leaf_shadow_proxy::LEAF_SHADOW_PROXY_FINE_CELL_SIZE_VOXELS,
        );
        let tree_leaves_instance = self.build_tree_render_instances(
            tree_id,
            &leaf_voxel_instances,
            &shadow_instances,
            0.2,
            "tree leaf",
        )?;
        let retired = surface_resources
            .instances
            .leaves_instances
            .insert(tree_id, tree_leaves_instance);
        if let Some(retired) = retired {
            self.retire_tree_instance(retired);
        }

        Ok(())
    }

    pub fn add_tree_apples(
        &mut self,
        surface_resources: &mut SurfaceResources,
        tree_id: u32,
        apples: &[(UVec3, u32)],
    ) -> Result<()> {
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
        let apple_shadow_instances = apple_instances
            .iter()
            .map(|instance| TreeShadowRenderInstanceData {
                world_pos: instance.world_pos,
                leaf_local_pos: instance.leaf_local_pos,
                billboard_size_voxels: leaf_shadow_proxy::SOURCE_LEAF_SHADOW_BILLBOARD_SIZE_VOXELS,
                opacity_layer_count: 1.0,
            })
            .collect::<Vec<_>>();
        let tree_apple_instance = self.build_tree_render_instances(
            tree_id,
            &apple_instances,
            &apple_shadow_instances,
            0.08,
            "tree apple",
        )?;
        let retired = surface_resources
            .instances
            .apple_instances
            .insert(tree_id, tree_apple_instance);
        if let Some(retired) = retired {
            self.retire_tree_instance(retired);
        }

        Ok(())
    }

    pub fn remove_tree_leaves(
        &mut self,
        surface_resources: &mut SurfaceResources,
        tree_id: u32,
    ) -> Result<()> {
        let removed_leaves = surface_resources
            .instances
            .leaves_instances
            .remove(&tree_id);
        let removed_apples = surface_resources.instances.apple_instances.remove(&tree_id);

        match (removed_leaves, removed_apples) {
            (Some(leaves), Some(apples)) => {
                log::info!(
                    "Removed tree {} with {} leaves and {} apples",
                    tree_id,
                    leaves.resources.instances_len,
                    apples.resources.instances_len
                );
                self.retire_tree_instance((leaves, apples));
            }
            (Some(leaves), None) => {
                log::info!(
                    "Removed tree {} with {} leaves",
                    tree_id,
                    leaves.resources.instances_len
                );
                self.retire_tree_instance(leaves);
            }
            (None, Some(apples)) => {
                log::info!(
                    "Removed tree {} with {} apples",
                    tree_id,
                    apples.resources.instances_len
                );
                self.retire_tree_instance(apples);
            }
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
                cmdbuf.use_buffer(
                    &self.resources.terrain_query.terrain_query_count,
                    BufferUse::HostWrite,
                );
                cmdbuf.use_buffer(
                    &self.resources.terrain_query.terrain_query_info,
                    BufferUse::HostWrite,
                );
                self.compute_pipelines.terrain_query_ppl.record(
                    cmdbuf,
                    Extent3D::new(query_count, 1, 1),
                    None,
                );
                cmdbuf.use_buffer(
                    &self.resources.terrain_query.terrain_query_result,
                    BufferUse::HostRead,
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

    /// Reads the experimental per-pixel Glass diagnostics after a settled frame. This is used
    /// only by the dedicated test scene, never by the normal render loop.
    pub(crate) fn capture_glass_debug_summary(&mut self) -> Result<GlassDebugSummary> {
        let texture = self
            .resources
            .extent_dependent_resources
            .glass_debug_tex
            .get_image();
        let extent = texture
            .get_desc()
            .extent
            .as_extent_2d()
            .context("Glass debug plane must be two-dimensional")?;
        let byte_count = u64::from(extent.width) * u64::from(extent.height) * 8;
        let mut readback = Buffer::new_sized(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            re_flora_vkn::BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            re_flora_vkn::MemoryLocation::GpuToCpu,
            byte_count,
        );
        texture.copy_image_to_buffer(
            &mut readback,
            &self.vulkan_ctx.get_general_queue(),
            self.vulkan_ctx.command_pool(),
            TextureLayout::GENERAL,
            0,
            TextureRegion {
                offset: [0, 0, 0],
                extent: Extent3D::new(extent.width, extent.height, 1),
            },
        );
        let bytes = readback.read_back()?;
        anyhow::ensure!(
            bytes.len() == byte_count as usize,
            "Glass debug readback returned {} bytes, expected {byte_count}",
            bytes.len(),
        );

        let mut summary = GlassDebugSummary {
            extent,
            glass_pixels: 0,
            foreground_pixels: 0,
            screen_hit_pixels: 0,
            raster_screen_hit_pixels: 0,
            fallback_pixels: 0,
            query_budget_fallback_pixels: 0,
            exhaustion_pixels: 0,
            scene_queries_max: 0,
            interfaces_max: 0,
            dda_steps_median: 0,
            dda_steps_p95: 0,
            dda_steps_max: 0,
            peak_active_paths: 0,
            tir_events: 0,
            throughput_cutoffs: 0,
            top_k_pruned: 0,
            nonfinite_pixels: 0,
        };
        let mut dda_steps = Vec::new();
        for item in bytes.chunks_exact(8) {
            let packed =
                u32::from_ne_bytes(item[..4].try_into().expect("u32-sized Glass debug item"));
            let packed_path = u32::from_ne_bytes(
                item[4..]
                    .try_into()
                    .expect("u32-sized Glass path debug item"),
            );
            let reason = packed & 0x0f;
            if reason == 0 {
                continue;
            }
            summary.glass_pixels += 1;
            summary.foreground_pixels += usize::from(reason == 7);
            summary.screen_hit_pixels += usize::from(reason == 1);
            summary.raster_screen_hit_pixels += usize::from((packed & (1_u32 << 31)) != 0);
            summary.fallback_pixels += usize::from(matches!(reason, 2..=6 | 9));
            summary.query_budget_fallback_pixels += usize::from(reason == 9);
            summary.exhaustion_pixels += usize::from(reason == 8);
            summary.interfaces_max = summary.interfaces_max.max((packed >> 4) & 0xff);
            summary.scene_queries_max = summary.scene_queries_max.max((packed >> 12) & 0x0f);
            dda_steps.push((packed >> 17) & 0x3fff);
            summary.peak_active_paths = summary.peak_active_paths.max(packed_path & 0x0f);
            summary.tir_events += ((packed_path >> 4) & 0xff) as usize;
            summary.throughput_cutoffs += ((packed_path >> 12) & 0xff) as usize;
            summary.top_k_pruned += ((packed_path >> 20) & 0xff) as usize;
            summary.nonfinite_pixels += ((packed_path >> 28) & 1) as usize;
        }
        dda_steps.sort_unstable();
        if let Some(maximum) = dda_steps.last().copied() {
            summary.dda_steps_median = dda_steps[dda_steps.len() / 2];
            summary.dda_steps_p95 =
                dda_steps[(dda_steps.len() * 95 / 100).min(dda_steps.len() - 1)];
            summary.dda_steps_max = maximum;
        }
        Ok(summary)
    }

    /// Debug-only GPU capture of the ordered dense-atlas material transitions along one straight
    /// ray. Production terrain queries retain their Contree path and never enable this mode.
    pub(crate) fn capture_glass_scene_query_events(
        &mut self,
        ray: TerrainRayQuery,
    ) -> Result<Vec<GlassSceneQueryCaptureEvent>> {
        let direction = ray.direction.normalize_or_zero();
        anyhow::ensure!(
            direction != Vec3::ZERO,
            "Glass scene-query capture ray is zero"
        );
        let mut origin = ray.origin;
        let mut events = Vec::with_capacity(8);
        for _ in 0..8 {
            let event =
                self.capture_next_glass_scene_query_event(TerrainRayQuery { origin, direction })?;
            events.push(event);
            match event.kind {
                GlassSceneQueryEventKind::Interface => {
                    let crossed_axes = Vec3::new(
                        if event.tied_axes & 0b001 != 0 {
                            1.0
                        } else {
                            0.0
                        },
                        if event.tied_axes & 0b010 != 0 {
                            1.0
                        } else {
                            0.0
                        },
                        if event.tied_axes & 0b100 != 0 {
                            1.0
                        } else {
                            0.0
                        },
                    );
                    origin = event.position + crossed_axes * direction.signum() * (1.0e-4 / 256.0);
                }
                GlassSceneQueryEventKind::Opaque | GlassSceneQueryEventKind::Miss => {
                    return Ok(events);
                }
                GlassSceneQueryEventKind::StepBudget => {
                    anyhow::bail!(
                        "Glass scene-query GPU capture exhausted DDA steps after {} events",
                        events.len(),
                    );
                }
            }
        }
        anyhow::bail!("Glass scene-query GPU capture exhausted its 8-event budget")
    }

    pub(crate) fn capture_glass_straight_transport(
        &mut self,
        ray: TerrainRayQuery,
        max_distance_world: f32,
    ) -> Result<GlassStraightTransportCapture> {
        anyhow::ensure!(
            ray.direction.normalize_or_zero() != Vec3::ZERO
                && max_distance_world.is_finite()
                && max_distance_world > 0.0,
            "Glass straight-transport capture requires a finite ray and distance"
        );
        let values = self.capture_terrain_query_debug_rows(&[[
            ray.origin.x,
            ray.origin.y,
            ray.origin.z,
            2.0,
            ray.direction.x,
            ray.direction.y,
            ray.direction.z,
            max_distance_world,
        ]])?[0];
        let packed = values[3].to_bits();
        let terminal = match packed & 0x0f {
            1 => GlassSceneQueryEventKind::Interface,
            2 => GlassSceneQueryEventKind::Opaque,
            3 => GlassSceneQueryEventKind::Miss,
            4 => GlassSceneQueryEventKind::StepBudget,
            value => anyhow::bail!(
                "Glass straight-transport GPU capture returned invalid terminal kind {value}"
            ),
        };
        Ok(GlassStraightTransportCapture {
            terminal,
            transmittance: Vec3::new(values[0], values[1], values[2]),
            interfaces: (packed >> 4) & 0xff,
            scene_queries: (packed >> 12) & 0xff,
            exhausted: ((packed >> 20) & 1) != 0,
            dda_steps: (packed >> 21) & 0x07ff,
        })
    }

    pub(crate) fn capture_ddgi_semantic_occupancy(
        &mut self,
        voxel_coordinates: &[UVec3],
    ) -> Result<Vec<DdgiSemanticOccupancyCapture>> {
        anyhow::ensure!(
            !voxel_coordinates.is_empty() && voxel_coordinates.len() <= MAX_TERRAIN_QUERIES,
            "DDGI semantic occupancy capture count is out of range"
        );
        let rows = voxel_coordinates
            .iter()
            .map(|coordinate| {
                [
                    coordinate.x as f32,
                    coordinate.y as f32,
                    coordinate.z as f32,
                    3.0,
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                ]
            })
            .collect::<Vec<_>>();
        Ok(self
            .capture_terrain_query_debug_rows(&rows)?
            .into_iter()
            .map(|values| DdgiSemanticOccupancyCapture {
                occupied: values[0] >= 0.5,
                geometry_revision: values[1] as u32,
                glass_material_revision: values[2] as u32,
            })
            .collect())
    }

    fn capture_terrain_query_debug_rows(&mut self, rows: &[[f32; 8]]) -> Result<Vec<[f32; 4]>> {
        anyhow::ensure!(
            !rows.is_empty() && rows.len() <= MAX_TERRAIN_QUERIES,
            "terrain-query debug capture count is out of range"
        );
        let query_count = rows.len() as u32;
        self.resources
            .terrain_query
            .terrain_query_count
            .fill_uniform(&crate::generated::gpu_structs::TerrainQueryCount {
                valid_query_count: query_count,
                ..bytemuck::Zeroable::zeroed()
            })?;
        let row_values = rows.iter().flatten().copied().collect::<Vec<_>>();
        self.resources
            .terrain_query
            .terrain_query_info
            .fill(&row_values)?;

        execute_one_time_gpu_job(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                cmdbuf.use_buffer(
                    &self.resources.terrain_query.terrain_query_count,
                    BufferUse::HostWrite,
                );
                cmdbuf.use_buffer(
                    &self.resources.terrain_query.terrain_query_info,
                    BufferUse::HostWrite,
                );
                self.compute_pipelines.terrain_query_ppl.record(
                    cmdbuf,
                    Extent3D::new(query_count, 1, 1),
                    None,
                );
                cmdbuf.use_buffer(
                    &self.resources.terrain_query.terrain_query_result,
                    BufferUse::HostRead,
                );
            },
        );

        let raw_data = self
            .resources
            .terrain_query
            .terrain_query_result
            .read_back()
            .context("read terrain-query debug GPU capture")?;
        let values: &[f32] = bytemuck::cast_slice(&raw_data);
        anyhow::ensure!(
            values.len() >= rows.len() * 4,
            "terrain-query debug GPU result is truncated"
        );
        Ok(values[..rows.len() * 4]
            .chunks_exact(4)
            .map(|item| [item[0], item[1], item[2], item[3]])
            .collect())
    }

    fn capture_next_glass_scene_query_event(
        &mut self,
        ray: TerrainRayQuery,
    ) -> Result<GlassSceneQueryCaptureEvent> {
        let values = self.capture_terrain_query_debug_rows(&[[
            ray.origin.x,
            ray.origin.y,
            ray.origin.z,
            1.0,
            ray.direction.x,
            ray.direction.y,
            ray.direction.z,
            0.0,
        ]])?[0];
        let packed = values[3].to_bits();
        let kind = match packed & 0x0f {
            1 => GlassSceneQueryEventKind::Interface,
            2 => GlassSceneQueryEventKind::Opaque,
            3 => GlassSceneQueryEventKind::Miss,
            4 => GlassSceneQueryEventKind::StepBudget,
            value => anyhow::bail!("Glass scene-query GPU returned invalid event kind {value}"),
        };
        Ok(GlassSceneQueryCaptureEvent {
            kind,
            position: Vec3::new(values[0], values[1], values[2]),
            from_voxel_type: (packed >> 4) & 0x0f,
            to_voxel_type: (packed >> 8) & 0x0f,
            tied_axes: ((packed >> 12) & 0x07) as u8,
            dda_steps: (packed >> 15) & 0x0fff,
        })
    }
}
