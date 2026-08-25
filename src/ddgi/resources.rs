use super::{
    DdgiAtlasLayout, DdgiBuildToken, DdgiFieldIdentity, DdgiFieldState, DdgiScheduledWork,
    DdgiScheduledWorkKind, DdgiVolumeGrid, DDGI_IRRADIANCE_INTERIOR_SIDE,
    DDGI_LOCAL_RECOVERY_MAX_ABSOLUTE_DELTA, DDGI_LOCAL_RECOVERY_MIN_EPOCH,
    DDGI_LOCAL_RECOVERY_STABLE_EPOCHS, DDGI_PROBE_BATCH_SIZE, DDGI_RAYS_PER_PROBE,
    DDGI_RAY_BUDGET_PER_FRAME, DDGI_TOPOLOGY_RECOVERY_HISTORY_RETENTION,
    DDGI_VISIBILITY_INTERIOR_SIDE,
};
use crate::environment_lighting::{DdgiRadianceHistoryPolicy, DdgiRadianceSnapshot};
use crate::generated::gpu_structs::{
    DdgiProbeMetadata, DdgiRadianceSun, DdgiRadianceVoxelPalette, DdgiTransportQueryInfo, LightGpu,
    LocalLightInfo,
};
use crate::geom::UAabb3;
use crate::lighting::{LOCAL_LIGHT_GPU_ABI_VERSION, LOCAL_LIGHT_GPU_CAPACITY};
use crate::resource::{DescriptorResource, Resource, ResourceContainer, ResourceLookup};
use anyhow::{ensure, Context, Result};
use bytemuck::Zeroable;
use glam::{Quat, UVec3};
use re_flora_vkn::vk;
use re_flora_vkn::{
    Allocator, Buffer, BufferUsage, BufferUse, Extent3D, ImageDesc, MemoryLocation, SamplerDesc,
    Texture, TextureLayout, VulkanContext,
};

const DDGI_IRRADIANCE_FORMAT: vk::Format = vk::Format::R32G32B32A32_SFLOAT;
const DDGI_VISIBILITY_FORMAT: vk::Format = vk::Format::R32G32_SFLOAT;
const DDGI_TRACE_STATS_COUNT: usize = 13;
const DDGI_RELOCATION_STATS_COUNT: usize = 14;
const DDGI_ATLAS_REDUCTION_COUNT: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiProbePriorityReason {
    TerrainEdit,
    LightingImpact,
    Camera,
}

/// One immutable starting point for a complete probe sweep. Priority rotates the round-robin
/// order; it never removes batches, so a continuously changing camera or light cannot starve the
/// rest of the volume once work has started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiProbePriority {
    voxel_bound: UAabb3,
    reason: DdgiProbePriorityReason,
}

impl DdgiProbePriority {
    pub fn new(voxel_bound: UAabb3, reason: DdgiProbePriorityReason) -> Self {
        Self {
            voxel_bound,
            reason,
        }
    }

    pub fn voxel_bound(self) -> UAabb3 {
        self.voxel_bound
    }

    pub fn reason(self) -> DdgiProbePriorityReason {
        self.reason
    }
}

/// Centralized temporal stopping policy. Delta thresholds permit an early sleep after a minimum
/// sample age. Rotated Monte Carlo samples can retain isolated high-delta texels indefinitely, so
/// the finite epoch budget is the deterministic quality contract and sleep backstop. These are
/// transport decisions only; HDR values are never clamped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DdgiConvergencePolicy {
    pub absolute_threshold: f32,
    pub relative_threshold: f32,
    pub relative_floor: f32,
    pub consecutive_epochs: u32,
    pub minimum_update_epochs: u32,
    pub maximum_update_epochs: u32,
}

pub const DDGI_CONVERGENCE_POLICY: DdgiConvergencePolicy = DdgiConvergencePolicy {
    absolute_threshold: 0.0025,
    relative_threshold: 0.02,
    relative_floor: 0.05,
    consecutive_epochs: 2,
    minimum_update_epochs: 8,
    maximum_update_epochs: 128,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
enum DdgiAtlasSlot {
    Atlas0 = 0,
    Atlas1 = 1,
}

impl DdgiAtlasSlot {
    fn other(self) -> Self {
        match self {
            Self::Atlas0 => Self::Atlas1,
            Self::Atlas1 => Self::Atlas0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Atlas0 => "atlas0",
            Self::Atlas1 => "atlas1",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DdgiSkySlot {
    Sky0,
    Sky1,
}

impl DdgiSkySlot {
    fn other(self) -> Self {
        match self {
            Self::Sky0 => Self::Sky1,
            Self::Sky1 => Self::Sky0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DdgiResidentField {
    logical: DdgiFieldIdentity,
    atlas_slot: DdgiAtlasSlot,
    sky_slot: DdgiSkySlot,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DdgiHistoryMode {
    #[default]
    Accumulating,
    TopologyRecovery,
    Stable,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DdgiResidentIteration {
    work: DdgiScheduledWork,
    logical: DdgiFieldIdentity,
    source: Option<DdgiResidentField>,
    destination: DdgiResidentField,
    local_refresh_voxel_bound: Option<UAabb3>,
    probe_priority: Option<DdgiProbePriority>,
    history_mode: DdgiHistoryMode,
    radiance_history_policy: Option<DdgiRadianceHistoryPolicy>,
}

#[cfg(test)]
fn resident_iteration_for_work(
    work: DdgiScheduledWork,
    published: Option<DdgiResidentField>,
    local_refresh_voxel_bound: Option<UAabb3>,
    history_mode: DdgiHistoryMode,
) -> Result<DdgiResidentIteration> {
    resident_iteration_for_work_with_policy(
        work,
        published,
        local_refresh_voxel_bound,
        history_mode,
        None,
        None,
    )
}

fn resident_iteration_for_work_with_policy(
    work: DdgiScheduledWork,
    published: Option<DdgiResidentField>,
    local_refresh_voxel_bound: Option<UAabb3>,
    history_mode: DdgiHistoryMode,
    radiance_history_policy: Option<DdgiRadianceHistoryPolicy>,
    probe_priority: Option<DdgiProbePriority>,
) -> Result<DdgiResidentIteration> {
    let destination = work.destination();
    match work.kind() {
        DdgiScheduledWorkKind::GeometryUpdate | DdgiScheduledWorkKind::DensityUpdate => {
            ensure!(
                published.is_none(),
                "initial DDGI update must use an unpublished staging volume"
            );
            let destination = DdgiResidentField {
                logical: destination,
                atlas_slot: DdgiAtlasSlot::Atlas0,
                sky_slot: DdgiSkySlot::Sky0,
            };
            let source = work.transport_source().map(|logical| DdgiResidentField {
                logical,
                // An inherited field lives in the active Volume. Builder descriptors expose its
                // exact published textures through the staging source bindings (logical slot 1).
                atlas_slot: DdgiAtlasSlot::Atlas1,
                sky_slot: DdgiSkySlot::Sky0,
            });
            ensure!(
                destination.logical.source() == source.map(|source| source.logical.field()),
                "DDGI initial-update source does not match its transport source"
            );
            Ok(DdgiResidentIteration {
                work,
                logical: destination.logical,
                source,
                destination,
                local_refresh_voxel_bound,
                probe_priority,
                history_mode,
                radiance_history_policy,
            })
        }
        DdgiScheduledWorkKind::RadianceUpdate | DdgiScheduledWorkKind::ConvergenceUpdate => {
            let source = published.context("temporal DDGI update requires a published source")?;
            ensure!(
                work.transport_source() == Some(source.logical),
                "DDGI scheduled transport source does not match resident source"
            );
            ensure!(
                destination.source() == Some(source.logical.field()),
                "DDGI temporal source {:?} does not match resident source {:?}",
                destination.source(),
                source.logical,
            );
            let radiance_changed = destination.field().radiance_revision()
                != source.logical.field().radiance_revision();
            let destination = DdgiResidentField {
                logical: destination,
                atlas_slot: source.atlas_slot.other(),
                sky_slot: if radiance_changed {
                    source.sky_slot.other()
                } else {
                    source.sky_slot
                },
            };
            Ok(DdgiResidentIteration {
                work,
                logical: destination.logical,
                source: Some(source),
                destination,
                local_refresh_voxel_bound,
                probe_priority,
                history_mode,
                radiance_history_policy,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DdgiRayBatch {
    pub first_probe_index: u32,
    pub probe_count: u32,
    resident: DdgiResidentIteration,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum DdgiBatchOrder {
    #[default]
    Forward = 0,
    Reverse = 1,
}

impl DdgiBatchOrder {
    pub fn from_cli_value(value: &str) -> Option<Self> {
        match value {
            "forward" => Some(Self::Forward),
            "reverse" => Some(Self::Reverse),
            _ => None,
        }
    }

    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Reverse => "reverse",
        }
    }
}

impl DdgiRayBatch {
    pub fn logical(self) -> DdgiFieldIdentity {
        self.resident.logical
    }

    pub fn geometry_revision(self) -> u32 {
        self.resident.logical.field().geometry_revision()
    }

    pub fn radiance_revision(self) -> u32 {
        self.resident.logical.field().radiance_revision()
    }

    pub fn spacing_voxels(self) -> u32 {
        self.resident.logical.field().spacing_voxels()
    }

    pub fn state(self) -> DdgiFieldState {
        self.resident.logical.field().state()
    }

    pub fn update_epoch(self) -> u32 {
        self.resident.logical.field().update_epoch()
    }

    pub fn source(self) -> Option<DdgiFieldIdentity> {
        self.resident.source.map(|source| source.logical)
    }

    pub fn source_slot_index(self) -> u32 {
        self.resident
            .source
            .map(|source| source.atlas_slot as u32)
            .unwrap_or_default()
    }

    pub fn destination_slot_index(self) -> u32 {
        self.resident.destination.atlas_slot as u32
    }

    pub fn destination_is_transport_source(self) -> bool {
        self.resident.destination.atlas_slot == DdgiAtlasSlot::Atlas1
    }

    pub fn destination_label(self) -> &'static str {
        self.resident.destination.atlas_slot.label()
    }

    pub fn source_label(self) -> &'static str {
        self.resident
            .source
            .map(|source| source.atlas_slot.label())
            .unwrap_or("none")
    }

    pub fn writes_visibility(self) -> bool {
        self.resident.work.kind() != DdgiScheduledWorkKind::RadianceUpdate
    }

    pub fn needs_visibility_preservation(self) -> bool {
        !self.writes_visibility() && self.resident.source.is_some()
    }

    pub fn local_refresh_voxel_bound(self) -> Option<UAabb3> {
        self.resident.local_refresh_voxel_bound.or_else(|| {
            (self.resident.work.kind() == DdgiScheduledWorkKind::RadianceUpdate)
                .then_some(self.resident.probe_priority)
                .flatten()
                .filter(|priority| priority.reason() == DdgiProbePriorityReason::LightingImpact)
                .map(DdgiProbePriority::voxel_bound)
        })
    }

    pub fn probe_priority(self) -> Option<DdgiProbePriority> {
        self.resident.probe_priority
    }

    pub fn local_recovery_epoch(self) -> u32 {
        self.update_epoch()
    }

    pub fn irradiance_history_is_valid(self) -> bool {
        self.resident.source.is_some_and(|source| {
            source.logical.field().spacing_voxels() == self.spacing_voxels()
                && (source.logical.field().radiance_revision() == self.radiance_revision()
                    || self
                        .resident
                        .radiance_history_policy
                        .is_some_and(|policy| !policy.resets_history()))
        })
    }

    pub fn visibility_history_is_valid(self) -> bool {
        self.resident
            .source
            .is_some_and(|source| source.logical.field().spacing_voxels() == self.spacing_voxels())
    }

    pub fn irradiance_history_retention(self, configured: f32) -> f32 {
        if !self.irradiance_history_is_valid() {
            return 0.0;
        }
        let configured = configured.clamp(0.0, 0.99);
        let radiance_changed = self.resident.source.is_some_and(|source| {
            source.logical.field().radiance_revision() != self.radiance_revision()
        });
        let configured = if radiance_changed {
            self.resident
                .radiance_history_policy
                .map_or(0.0, |policy| policy.retention(configured))
        } else {
            configured
        };
        match self.resident.history_mode {
            DdgiHistoryMode::Stable => configured,
            DdgiHistoryMode::TopologyRecovery => {
                configured.min(DDGI_TOPOLOGY_RECOVERY_HISTORY_RETENTION)
            }
            DdgiHistoryMode::Accumulating if radiance_changed => configured,
            DdgiHistoryMode::Accumulating => {
                configured.min(self.update_epoch() as f32 / (self.update_epoch() as f32 + 1.0))
            }
        }
    }

    pub fn visibility_history_retention(self, configured: f32) -> f32 {
        if !self.visibility_history_is_valid() {
            return 0.0;
        }
        let configured = configured.clamp(0.0, 0.99);
        match self.resident.history_mode {
            DdgiHistoryMode::Stable => configured,
            DdgiHistoryMode::TopologyRecovery => {
                configured.min(DDGI_TOPOLOGY_RECOVERY_HISTORY_RETENTION)
            }
            DdgiHistoryMode::Accumulating => {
                configured.min(self.update_epoch() as f32 / (self.update_epoch() as f32 + 1.0))
            }
        }
    }

    pub fn epoch_rotation(self) -> [f32; 4] {
        ddgi_epoch_rotation(
            self.geometry_revision(),
            self.radiance_revision(),
            self.update_epoch(),
        )
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn unit_f64(bits: u64) -> f64 {
    ((bits >> 11) as f64 + 0.5) * (1.0 / ((1_u64 << 53) as f64))
}

/// Returns a deterministic, uniformly distributed SO(3) rotation for one complete update epoch.
/// Every batch in that epoch receives the same quaternion, so batch scheduling cannot create
/// directional seams. The quaternion uses Slang/glam's shared `(x, y, z, w)` convention.
fn ddgi_epoch_rotation(
    geometry_revision: u32,
    radiance_revision: u32,
    update_epoch: u32,
) -> [f32; 4] {
    let seed = u64::from(geometry_revision)
        | (u64::from(radiance_revision) << 21)
        | (u64::from(update_epoch) << 42);
    let u1 = unit_f64(splitmix64(seed));
    let u2 = unit_f64(splitmix64(seed ^ 0xa076_1d64_78bd_642f));
    let u3 = unit_f64(splitmix64(seed ^ 0xe703_7ed1_a0b4_28db));
    let angle2 = std::f64::consts::TAU * u2;
    let angle3 = std::f64::consts::TAU * u3;
    let radius1 = (1.0 - u1).sqrt();
    let radius2 = u1.sqrt();
    Quat::from_xyzw(
        (radius1 * angle2.sin()) as f32,
        (radius1 * angle2.cos()) as f32,
        (radius2 * angle3.sin()) as f32,
        (radius2 * angle3.cos()) as f32,
    )
    .normalize()
    .to_array()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DdgiTraceStats {
    pub ray_records: u32,
    pub valid_probe_rays: u32,
    pub misses: u32,
    pub frontface_hits: u32,
    pub backface_hits: u32,
    pub non_finite_records: u32,
    pub invalid_probe_rays: u32,
    pub local_light_candidates: u32,
    pub local_light_visible: u32,
    pub local_light_occluded: u32,
    /// Scene-linear point-light irradiance luminance accumulated as unsigned Q24.8 values.
    pub local_light_irradiance_luma_q8: u32,
    pub emissive_surface_hits: u32,
    /// Scene-linear emitted radiance luminance accumulated as unsigned Q24.8 values.
    pub emissive_surface_radiance_luma_q8: u32,
}

impl DdgiTraceStats {
    fn from_array(values: [u32; DDGI_TRACE_STATS_COUNT]) -> Self {
        Self {
            ray_records: values[0],
            valid_probe_rays: values[1],
            misses: values[2],
            frontface_hits: values[3],
            backface_hits: values[4],
            non_finite_records: values[5],
            invalid_probe_rays: values[6],
            local_light_candidates: values[7],
            local_light_visible: values[8],
            local_light_occluded: values[9],
            local_light_irradiance_luma_q8: values[10],
            emissive_surface_hits: values[11],
            emissive_surface_radiance_luma_q8: values[12],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DdgiLocalLightTraceTotals {
    pub candidates: u64,
    pub visible: u64,
    pub occluded: u64,
    pub irradiance_luma_q8: u64,
}

impl DdgiLocalLightTraceTotals {
    pub fn accumulate(&mut self, batch: DdgiTraceStats) {
        self.candidates += u64::from(batch.local_light_candidates);
        self.visible += u64::from(batch.local_light_visible);
        self.occluded += u64::from(batch.local_light_occluded);
        self.irradiance_luma_q8 += u64::from(batch.local_light_irradiance_luma_q8);
        assert_eq!(
            self.candidates,
            self.visible + self.occluded,
            "accumulated DDGI local-light visibility partition diverged",
        );
    }

    pub fn irradiance_luma(self) -> f64 {
        self.irradiance_luma_q8 as f64 / 256.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DdgiRelocationReadbackStats {
    pub probes: u32,
    pub valid: u32,
    pub failed: u32,
    pub fast_target: u32,
    pub local_target: u32,
    pub outer_target: u32,
    pub outer_best_effort: u32,
    pub full_escape: u32,
    pub clearance_sum: u32,
    pub distance_squared_twice_sum: u32,
    pub moved: u32,
    pub clearance_below_half_target: u32,
    pub clearance_half_to_target: u32,
    pub clearance_target: u32,
}

impl DdgiRelocationReadbackStats {
    fn from_array(values: [u32; DDGI_RELOCATION_STATS_COUNT]) -> Self {
        Self {
            probes: values[0],
            valid: values[1],
            failed: values[2],
            fast_target: values[3],
            local_target: values[4],
            outer_target: values[5],
            outer_best_effort: values[6],
            full_escape: values[7],
            clearance_sum: values[8],
            distance_squared_twice_sum: values[9],
            moved: values[10],
            clearance_below_half_target: values[11],
            clearance_half_to_target: values[12],
            clearance_target: values[13],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DdgiAtlasValidationStats {
    pub max_absolute_rgb_delta: f32,
    pub max_relative_rgb_delta: f32,
    /// Maximum non-negative RGB component in the destination interior. A completed all-black
    /// atlas is never publication-safe for the authored sky used by the game.
    pub max_rgb_value: f32,
    pub non_finite_count: u32,
    pub negative_rgb_texel_count: u32,
    /// Valid 8x8 interior texels included in convergence deltas.
    pub valid_texel_count: u32,
    /// Valid 10x10 stored texels checked for finite values, including gutters.
    pub scanned_stored_texel_count: u32,
}

impl DdgiAtlasValidationStats {
    fn from_array(values: [u32; DDGI_ATLAS_REDUCTION_COUNT]) -> Self {
        Self {
            max_absolute_rgb_delta: f32::from_bits(values[0]),
            max_relative_rgb_delta: f32::from_bits(values[1]),
            max_rgb_value: f32::from_bits(values[6]),
            non_finite_count: values[2],
            negative_rgb_texel_count: values[3],
            valid_texel_count: values[4],
            scanned_stored_texel_count: values[5],
        }
    }
}

fn validate_atlas_stats(stats: DdgiAtlasValidationStats) -> Result<()> {
    ensure!(
        stats.non_finite_count == 0,
        "DDGI full-atlas validation found non-finite stored texels: {stats:?}"
    );
    ensure!(
        stats.negative_rgb_texel_count == 0,
        "DDGI full-atlas validation found negative RGB stored texels: {stats:?}"
    );
    ensure!(
        stats.max_absolute_rgb_delta.is_finite()
            && stats.max_relative_rgb_delta.is_finite()
            && stats.max_rgb_value.is_finite()
            && stats.max_absolute_rgb_delta >= 0.0
            && stats.max_relative_rgb_delta >= 0.0
            && stats.max_rgb_value >= 0.0,
        "DDGI atlas reduction produced invalid delta metrics: {stats:?}"
    );
    ensure!(
        stats.valid_texel_count > 0 && stats.scanned_stored_texel_count > 0,
        "DDGI full-atlas validation found no valid probe texels: {stats:?}"
    );
    ensure!(
        stats.max_rgb_value > 0.0,
        "DDGI full-atlas validation rejected an all-black irradiance atlas: {stats:?}"
    );
    ensure!(
        u64::from(stats.scanned_stored_texel_count) * 64
            == u64::from(stats.valid_texel_count) * 100,
        "DDGI atlas reduction did not cover complete 10x10 stored and 8x8 interior tiles: {stats:?}"
    );
    Ok(())
}

fn iteration_completes_after_batch(
    completed_probe_count: u32,
    batch_probe_count: u32,
    probe_count: u32,
) -> bool {
    completed_probe_count + batch_probe_count == probe_count
}

fn ddgi_probe_batch_range(
    probe_count: u32,
    batch_size: u32,
    batch_ordinal: u32,
    order: DdgiBatchOrder,
    priority_physical_ordinal: Option<u32>,
) -> Option<(u32, u32)> {
    debug_assert!(batch_size > 0);
    let batch_count = probe_count.div_ceil(batch_size);
    if batch_ordinal >= batch_count {
        return None;
    }
    let physical_ordinal = match (order, priority_physical_ordinal) {
        (DdgiBatchOrder::Forward, Some(priority)) => (priority + batch_ordinal) % batch_count,
        (DdgiBatchOrder::Reverse, Some(priority)) => {
            (priority + batch_count - batch_ordinal) % batch_count
        }
        (DdgiBatchOrder::Forward, None) => batch_ordinal,
        (DdgiBatchOrder::Reverse, None) => batch_count - 1 - batch_ordinal,
    };
    let first_probe_index = physical_ordinal * batch_size;
    Some((
        first_probe_index,
        (probe_count - first_probe_index).min(batch_size),
    ))
}

fn priority_probe_batch(
    grid: DdgiVolumeGrid,
    edit_voxel_bound: Option<UAabb3>,
    batch_size: u32,
) -> Option<u32> {
    let center = edit_voxel_bound?.center();
    let coordinate = (center / grid.spacing_voxels() as f32)
        .round()
        .as_uvec3()
        .min(grid.dimensions() - UVec3::ONE);
    grid.flatten(coordinate).map(|index| index / batch_size)
}

fn local_refresh_probe_partition(grid: DdgiVolumeGrid, bound: UAabb3) -> (u32, u32) {
    let spacing = grid.spacing_voxels();
    let dimensions = grid.dimensions();
    let mut dirty = 0;
    for z in 0..dimensions.z {
        for y in 0..dimensions.y {
            for x in 0..dimensions.x {
                let nominal = UVec3::new(x, y, z) * spacing;
                if nominal.cmpge(bound.min()).all() && nominal.cmple(bound.max()).all() {
                    dirty += 1;
                }
            }
        }
    }
    (dirty, grid.probe_count() - dirty)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiResourceBytes {
    pub irradiance_atlas: u64,
    pub transport_source_irradiance_atlas: u64,
    pub visibility_atlas: u64,
    pub transport_source_visibility_atlas: u64,
    pub global_sky_irradiance: u64,
    pub probe_metadata: u64,
    pub transient_ray_data: u64,
    pub trace_stats: u64,
    pub relocation_stats: u64,
    pub atlas_reduction: u64,
    pub radiance_sun: u64,
    pub radiance_voxel_palette: u64,
    pub transport_query_info: u64,
    pub local_light_info: u64,
    pub local_lights: u64,
}

impl DdgiResourceBytes {
    pub fn for_grid(grid: DdgiVolumeGrid) -> Result<Self> {
        let irradiance_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_IRRADIANCE_INTERIOR_SIDE)?;
        let visibility_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_VISIBILITY_INTERIOR_SIDE)?;
        Ok(Self::new(grid, irradiance_layout, visibility_layout))
    }

    fn new(
        grid: DdgiVolumeGrid,
        irradiance_layout: DdgiAtlasLayout,
        visibility_layout: DdgiAtlasLayout,
    ) -> Self {
        let irradiance_extent = irradiance_layout.extent();
        let visibility_extent = visibility_layout.extent();
        Self {
            irradiance_atlas: irradiance_extent.x as u64
                * irradiance_extent.y as u64
                * std::mem::size_of::<[f32; 4]>() as u64,
            transport_source_irradiance_atlas: irradiance_extent.x as u64
                * irradiance_extent.y as u64
                * std::mem::size_of::<[f32; 4]>() as u64,
            visibility_atlas: visibility_extent.x as u64
                * visibility_extent.y as u64
                * std::mem::size_of::<[f32; 2]>() as u64,
            transport_source_visibility_atlas: visibility_extent.x as u64
                * visibility_extent.y as u64
                * std::mem::size_of::<[f32; 2]>() as u64,
            global_sky_irradiance: super::DDGI_IRRADIANCE_STORED_SIDE as u64
                * super::DDGI_IRRADIANCE_STORED_SIDE as u64
                * std::mem::size_of::<[f32; 4]>() as u64
                * 2,
            probe_metadata: grid.probe_count() as u64
                * std::mem::size_of::<DdgiProbeMetadata>() as u64,
            transient_ray_data: DDGI_PROBE_BATCH_SIZE as u64
                * DDGI_RAYS_PER_PROBE as u64
                * std::mem::size_of::<[f32; 4]>() as u64,
            trace_stats: (DDGI_TRACE_STATS_COUNT * std::mem::size_of::<u32>()) as u64,
            relocation_stats: (DDGI_RELOCATION_STATS_COUNT * std::mem::size_of::<u32>()) as u64,
            atlas_reduction: (DDGI_ATLAS_REDUCTION_COUNT * std::mem::size_of::<u32>()) as u64,
            radiance_sun: std::mem::size_of::<DdgiRadianceSun>() as u64,
            radiance_voxel_palette: std::mem::size_of::<DdgiRadianceVoxelPalette>() as u64,
            transport_query_info: std::mem::size_of::<DdgiTransportQueryInfo>() as u64,
            local_light_info: std::mem::size_of::<LocalLightInfo>() as u64,
            local_lights: (LOCAL_LIGHT_GPU_CAPACITY * std::mem::size_of::<LightGpu>()) as u64,
        }
    }

    pub fn total(self) -> u64 {
        self.irradiance_atlas
            + self.transport_source_irradiance_atlas
            + self.visibility_atlas
            + self.transport_source_visibility_atlas
            + self.global_sky_irradiance
            + self.probe_metadata
            + self.transient_ray_data
            + self.trace_stats
            + self.relocation_stats
            + self.atlas_reduction
            + self.radiance_sun
            + self.radiance_voxel_palette
            + self.transport_query_info
            + self.local_light_info
            + self.local_lights
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiVerifiedBatchOutcome {
    Continue,
    AwaitingAtlasValidation(DdgiFieldIdentity),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiValidatedIterationOutcome {
    Published {
        work: DdgiScheduledWork,
        field: DdgiFieldIdentity,
        consecutive_below_threshold: u32,
    },
    Converged {
        work: DdgiScheduledWork,
        field: DdgiFieldIdentity,
        reason: DdgiConvergenceReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiConvergenceReason {
    Threshold,
    SampleBudget,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DdgiVolumeStage {
    #[default]
    Allocated,
    GlobalSkyReady,
    RelocationPending,
    Relocated,
    RayBatchReady,
    AtlasReady,
    Rebuilding,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DdgiVolumeStatus {
    pub(crate) build_token: Option<DdgiBuildToken>,
    pub(crate) grid: DdgiVolumeGrid,
    pub(crate) irradiance_layout: DdgiAtlasLayout,
    pub(crate) visibility_layout: DdgiAtlasLayout,
    pub(crate) resource_bytes: DdgiResourceBytes,
    pub(crate) stage: DdgiVolumeStage,
    pub(crate) scheduled_work: Option<DdgiScheduledWork>,
    pub(crate) complete_field: Option<DdgiFieldIdentity>,
    pub(crate) published_field: Option<DdgiFieldIdentity>,
    pub(crate) building_field: Option<DdgiFieldIdentity>,
    pub(crate) consecutive_below_threshold: u32,
    pub(crate) last_atlas_validation: Option<DdgiAtlasValidationStats>,
    pub(crate) global_sky_revision: u32,
    pub(crate) radiance_revision: Option<u32>,
    pub(crate) relocated_terrain_revision: Option<u32>,
    pub(crate) active_ray_batch: Option<DdgiRayBatch>,
    pub(crate) filtered_probe_count: u32,
    pub(crate) probe_priority: Option<DdgiProbePriority>,
    pub(crate) promotion_ready: bool,
}

impl DdgiVolumeStatus {
    pub(crate) fn is_ready(self) -> bool {
        self.published_field.is_some()
    }
}

/// The consumer-visible DDGI volume and an optional volume being built for a later promotion.
///
/// Callers can inspect revisions and readiness without learning which atlas or ray batch the
/// builder currently owns. Consumers must use `active`; builder passes must use `builder`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DdgiStatus {
    active: DdgiVolumeStatus,
    staging: Option<DdgiVolumeStatus>,
}

impl DdgiStatus {
    pub(crate) fn new(active: DdgiVolumeStatus, staging: Option<DdgiVolumeStatus>) -> Self {
        Self { active, staging }
    }

    pub(crate) fn active(self) -> DdgiVolumeStatus {
        self.active
    }

    pub(crate) fn staging(self) -> Option<DdgiVolumeStatus> {
        self.staging
    }

    pub(crate) fn builder(self) -> DdgiVolumeStatus {
        self.staging.unwrap_or(self.active)
    }

    pub(crate) fn staging_is_ready(self) -> bool {
        self.staging()
            .is_some_and(|staging| staging.promotion_ready)
    }
}

pub struct DdgiVolume {
    build_token: Option<DdgiBuildToken>,
    grid: DdgiVolumeGrid,
    irradiance_layout: DdgiAtlasLayout,
    visibility_layout: DdgiAtlasLayout,
    resource_bytes: DdgiResourceBytes,
    stage: DdgiVolumeStage,
    scheduled_work: Option<DdgiScheduledWork>,
    building_iteration: Option<DdgiResidentIteration>,
    complete_field: Option<DdgiResidentField>,
    published_field: Option<DdgiResidentField>,
    consecutive_below_threshold: u32,
    last_atlas_validation: Option<DdgiAtlasValidationStats>,
    global_sky_revisions: [u32; 2],
    radiance_revision: Option<u32>,
    radiance_snapshot: Option<DdgiRadianceSnapshot>,
    requested_terrain_revision: Option<u32>,
    relocated_terrain_revision: Option<u32>,
    active_ray_batch: Option<DdgiRayBatch>,
    batch_order: DdgiBatchOrder,
    filtered_probe_count: u32,
    next_batch_ordinal: u32,
    local_refresh_voxel_bound: Option<UAabb3>,
    local_recovery_stable_epochs: u32,
    history_mode: DdgiHistoryMode,
    visibility_preserved_for_iteration: bool,
    pub ddgi_probe_metadata: Resource<Buffer>,
    pub ddgi_transient_ray_data: Resource<Buffer>,
    pub ddgi_trace_stats: Resource<Buffer>,
    ddgi_trace_stats_readback: Buffer,
    pub ddgi_relocation_stats: Resource<Buffer>,
    ddgi_relocation_stats_readback: Buffer,
    pub ddgi_atlas_reduction: Resource<Buffer>,
    ddgi_atlas_reduction_readback: Buffer,
    pub ddgi_irradiance_atlas: Resource<Texture>,
    pub ddgi_transport_source_irradiance_atlas: Resource<Texture>,
    pub ddgi_visibility_atlas: Resource<Texture>,
    pub ddgi_transport_source_visibility_atlas: Resource<Texture>,
    pub ddgi_global_sky_irradiance: Resource<Texture>,
    pub ddgi_global_sky_irradiance_alt: Resource<Texture>,
    pub ddgi_radiance_sun: Resource<Buffer>,
    pub ddgi_radiance_voxel_palette: Resource<Buffer>,
    pub ddgi_transport_query_info: Resource<Buffer>,
    pub ddgi_local_light_info: Resource<Buffer>,
    pub ddgi_local_lights: Resource<Buffer>,
    transport_query_snapshot: DdgiTransportQueryInfo,
}

/// Owns the DDGI active/staging lifecycle.
///
/// A staging volume is never returned by [`Self::active`]. Promotion is the only operation that
/// can make it consumer-visible, and promotion rejects incomplete volumes.
pub struct DdgiVolumes {
    active: DdgiVolume,
    staging: Option<DdgiVolume>,
}

impl DdgiVolumes {
    pub fn new(active: DdgiVolume) -> Self {
        Self {
            active,
            staging: None,
        }
    }

    pub(crate) fn status(&self) -> DdgiStatus {
        DdgiStatus::new(
            self.active.status(),
            self.staging.as_ref().map(DdgiVolume::status),
        )
    }

    pub fn active(&self) -> &DdgiVolume {
        &self.active
    }

    pub fn builder(&self) -> &DdgiVolume {
        self.staging.as_ref().unwrap_or(&self.active)
    }

    pub fn builder_mut(&mut self) -> &mut DdgiVolume {
        self.staging.as_mut().unwrap_or(&mut self.active)
    }

    pub fn builder_is_active(&self) -> bool {
        self.staging.is_none()
    }

    /// Installs a new builder target while returning the previous staging volume, if any.
    /// The caller must rebind builder descriptors before dropping the returned volume.
    pub fn prepare_staging(&mut self, staging: DdgiVolume) -> Option<DdgiVolume> {
        self.staging.replace(staging)
    }

    /// Promotes a complete staging volume and returns the previous active volume.
    /// The caller must rebind consumer descriptors before dropping the returned volume.
    pub fn promote_staging(&mut self, expected_token: DdgiBuildToken) -> Result<DdgiVolume> {
        let staging = self
            .staging
            .as_ref()
            .context("cannot promote DDGI staging volume: no staging volume exists")?;
        ensure!(
            staging.status().promotion_ready,
            "cannot promote DDGI staging volume before it is ready (stage={:?})",
            staging.status().stage,
        );
        ensure!(
            staging.status().build_token == Some(expected_token),
            "cannot promote DDGI staging volume with token {:?}; expected {:?}",
            staging.status().build_token,
            expected_token,
        );
        let mut staging = self.staging.take().expect("staging presence checked above");
        staging.finish_local_recovery();
        Ok(std::mem::replace(&mut self.active, staging))
    }
}

impl ResourceContainer for DdgiVolume {
    fn resolve_resource(&self, name: &str) -> ResourceLookup<'_> {
        match name {
            "ddgi_probe_metadata" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.ddgi_probe_metadata))
            }
            "ddgi_transient_ray_data" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.ddgi_transient_ray_data))
            }
            "ddgi_trace_stats" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.ddgi_trace_stats))
            }
            "ddgi_relocation_stats" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.ddgi_relocation_stats))
            }
            "ddgi_atlas_reduction" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.ddgi_atlas_reduction))
            }
            "ddgi_radiance_sun" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.ddgi_radiance_sun))
            }
            "ddgi_radiance_voxel_palette" => ResourceLookup::Unique(DescriptorResource::Buffer(
                &self.ddgi_radiance_voxel_palette,
            )),
            "ddgi_transport_query_info" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.ddgi_transport_query_info))
            }
            "ddgi_local_light_info" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.ddgi_local_light_info))
            }
            "ddgi_local_lights" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.ddgi_local_lights))
            }
            "ddgi_irradiance_atlas" => {
                ResourceLookup::Unique(DescriptorResource::Texture(&self.ddgi_irradiance_atlas))
            }
            "ddgi_transport_source_irradiance_atlas" => ResourceLookup::Unique(
                DescriptorResource::Texture(&self.ddgi_transport_source_irradiance_atlas),
            ),
            "ddgi_visibility_atlas" => {
                ResourceLookup::Unique(DescriptorResource::Texture(&self.ddgi_visibility_atlas))
            }
            "ddgi_transport_source_visibility_atlas" => ResourceLookup::Unique(
                DescriptorResource::Texture(&self.ddgi_transport_source_visibility_atlas),
            ),
            "ddgi_global_sky_irradiance" => ResourceLookup::Unique(DescriptorResource::Texture(
                &self.ddgi_global_sky_irradiance,
            )),
            "ddgi_global_sky_irradiance_alt" => ResourceLookup::Unique(
                DescriptorResource::Texture(&self.ddgi_global_sky_irradiance_alt),
            ),
            _ => ResourceLookup::Missing,
        }
    }
}

impl DdgiVolume {
    pub fn new(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        world_extent_voxels: UVec3,
        spacing_voxels: u32,
        voxels_per_world_unit: UVec3,
        batch_order: DdgiBatchOrder,
    ) -> Result<Self> {
        let grid = DdgiVolumeGrid::new(world_extent_voxels, spacing_voxels)?;
        let irradiance_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_IRRADIANCE_INTERIOR_SIDE)?;
        let visibility_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_VISIBILITY_INTERIOR_SIDE)?;
        debug_assert_eq!(
            visibility_layout.stored_side(),
            super::atlas::DDGI_VISIBILITY_STORED_SIDE
        );
        let resource_bytes = DdgiResourceBytes::new(grid, irradiance_layout, visibility_layout);

        let physical_device_properties = unsafe {
            vulkan_ctx
                .instance()
                .as_raw()
                .get_physical_device_properties(vulkan_ctx.physical_device().as_raw())
        };
        let max_image_dimension = physical_device_properties.limits.max_image_dimension2_d;
        for (name, extent) in [
            ("irradiance", irradiance_layout.extent()),
            ("visibility", visibility_layout.extent()),
        ] {
            ensure!(
                extent.max_element() <= max_image_dimension,
                "DDGI {name} atlas {}x{} exceeds device 2D texture limit {max_image_dimension}",
                extent.x,
                extent.y
            );
        }

        let device = vulkan_ctx.device().clone();
        let sampled_storage_usage = ddgi_atlas_image_usage();
        let sampler_desc = SamplerDesc {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            ..Default::default()
        };

        let irradiance_desc = atlas_image_desc(
            irradiance_layout,
            DDGI_IRRADIANCE_FORMAT,
            sampled_storage_usage,
        );
        let visibility_desc = atlas_image_desc(
            visibility_layout,
            DDGI_VISIBILITY_FORMAT,
            sampled_storage_usage,
        );
        let global_sky_layout = DdgiAtlasLayout::new(1, DDGI_IRRADIANCE_INTERIOR_SIDE)?;
        let global_sky_desc = atlas_image_desc(
            global_sky_layout,
            DDGI_IRRADIANCE_FORMAT,
            sampled_storage_usage,
        );

        let probe_metadata = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            resource_bytes.probe_metadata,
        );
        let transient_ray_data = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            resource_bytes.transient_ray_data,
        );
        let trace_stats = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            resource_bytes.trace_stats,
        );
        let trace_stats_readback = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            resource_bytes.trace_stats,
        );
        let relocation_stats = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            resource_bytes.relocation_stats,
        );
        let relocation_stats_readback = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            resource_bytes.relocation_stats,
        );
        let atlas_reduction = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            resource_bytes.atlas_reduction,
        );
        let atlas_reduction_readback = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            resource_bytes.atlas_reduction,
        );
        let uniform_buffer = |size| {
            Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::UNIFORM_BUFFER),
                MemoryLocation::CpuToGpu,
                size,
            )
        };
        let radiance_sun = uniform_buffer(resource_bytes.radiance_sun);
        radiance_sun.fill_uniform(&DdgiRadianceSun::zeroed())?;
        let radiance_voxel_palette = uniform_buffer(resource_bytes.radiance_voxel_palette);
        radiance_voxel_palette.fill_uniform(&DdgiRadianceVoxelPalette::zeroed())?;
        let transport_query_info = uniform_buffer(resource_bytes.transport_query_info);
        let transport_query_snapshot = DdgiTransportQueryInfo {
            grid_dimensions: grid.dimensions().to_array(),
            visibility_bias_world: 0.0,
            world_to_grid_scale: (voxels_per_world_unit.as_vec3() / spacing_voxels as f32)
                .to_array(),
            source_ready: 0,
            irradiance_tile_columns: irradiance_layout.tile_grid().x,
            visibility_tile_columns: visibility_layout.tile_grid().x,
            geometry_revision: 0,
            padding: 0,
        };
        transport_query_info.fill_uniform(&transport_query_snapshot)?;
        let ddgi_local_light_info = uniform_buffer(resource_bytes.local_light_info);
        ddgi_local_light_info.fill_uniform(&LocalLightInfo {
            abi_version: LOCAL_LIGHT_GPU_ABI_VERSION,
            count: 0,
            capacity: LOCAL_LIGHT_GPU_CAPACITY as u32,
            overflow_count: 0,
            source_revision_low: 0,
            source_revision_high: 0,
            registry_revision_low: 0,
            registry_revision_high: 0,
            live_revision_low: 0,
            live_revision_high: 0,
            transport_revision: 0,
            flags: 0,
        })?;
        let ddgi_local_lights = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            resource_bytes.local_lights,
        );
        ddgi_local_lights.fill(&[LightGpu::zeroed(); LOCAL_LIGHT_GPU_CAPACITY])?;
        let irradiance_atlas = Texture::new(
            device.clone(),
            allocator.clone(),
            &irradiance_desc,
            &sampler_desc,
        );
        let transport_source_irradiance_atlas = Texture::new(
            device.clone(),
            allocator.clone(),
            &irradiance_desc,
            &sampler_desc,
        );
        let visibility_atlas = Texture::new(
            device.clone(),
            allocator.clone(),
            &visibility_desc,
            &sampler_desc,
        );
        let transport_source_visibility_atlas = Texture::new(
            device.clone(),
            allocator.clone(),
            &visibility_desc,
            &sampler_desc,
        );
        let global_sky_irradiance = Texture::new(
            device.clone(),
            allocator.clone(),
            &global_sky_desc,
            &sampler_desc,
        );
        let global_sky_irradiance_alt =
            Texture::new(device, allocator, &global_sky_desc, &sampler_desc);

        log::info!(
            "[DDGI] allocated stage=allocated spacing_voxels={} grid={}x{}x{} probes={} irradiance={}x{} RGBA32F visibility={}x{} RG32F ray_budget_per_frame={} ray_batch={}x{} metadata_bytes={} irradiance_bytes={} transport_source_irradiance_bytes={} visibility_bytes={} transport_source_visibility_bytes={} ray_bytes={} trace_stats_bytes={} relocation_stats_bytes={} atlas_reduction_bytes={} global_sky_bytes={} snapshot_uniform_bytes={} transport_query_bytes={} local_light_bytes={} total_mib={:.2}",
            spacing_voxels,
            grid.dimensions().x,
            grid.dimensions().y,
            grid.dimensions().z,
            grid.probe_count(),
            irradiance_layout.extent().x,
            irradiance_layout.extent().y,
            visibility_layout.extent().x,
            visibility_layout.extent().y,
            DDGI_RAY_BUDGET_PER_FRAME,
            DDGI_PROBE_BATCH_SIZE,
            DDGI_RAYS_PER_PROBE,
            resource_bytes.probe_metadata,
            resource_bytes.irradiance_atlas,
            resource_bytes.transport_source_irradiance_atlas,
            resource_bytes.visibility_atlas,
            resource_bytes.transport_source_visibility_atlas,
            resource_bytes.transient_ray_data,
            resource_bytes.trace_stats,
            resource_bytes.relocation_stats,
            resource_bytes.atlas_reduction,
            resource_bytes.global_sky_irradiance,
            resource_bytes.radiance_sun + resource_bytes.radiance_voxel_palette,
            resource_bytes.transport_query_info,
            resource_bytes.local_light_info + resource_bytes.local_lights,
            resource_bytes.total() as f64 / (1024.0 * 1024.0),
        );

        Ok(Self {
            build_token: None,
            grid,
            irradiance_layout,
            visibility_layout,
            resource_bytes,
            stage: DdgiVolumeStage::Allocated,
            scheduled_work: None,
            building_iteration: None,
            complete_field: None,
            published_field: None,
            consecutive_below_threshold: 0,
            last_atlas_validation: None,
            global_sky_revisions: [0; 2],
            radiance_revision: None,
            radiance_snapshot: None,
            requested_terrain_revision: None,
            relocated_terrain_revision: None,
            active_ray_batch: None,
            batch_order,
            filtered_probe_count: 0,
            next_batch_ordinal: 0,
            local_refresh_voxel_bound: None,
            local_recovery_stable_epochs: 0,
            history_mode: DdgiHistoryMode::Accumulating,
            visibility_preserved_for_iteration: false,
            ddgi_probe_metadata: Resource::new(probe_metadata),
            ddgi_transient_ray_data: Resource::new(transient_ray_data),
            ddgi_trace_stats: Resource::new(trace_stats),
            ddgi_trace_stats_readback: trace_stats_readback,
            ddgi_relocation_stats: Resource::new(relocation_stats),
            ddgi_relocation_stats_readback: relocation_stats_readback,
            ddgi_atlas_reduction: Resource::new(atlas_reduction),
            ddgi_atlas_reduction_readback: atlas_reduction_readback,
            ddgi_irradiance_atlas: Resource::new(irradiance_atlas),
            ddgi_transport_source_irradiance_atlas: Resource::new(
                transport_source_irradiance_atlas,
            ),
            ddgi_visibility_atlas: Resource::new(visibility_atlas),
            ddgi_transport_source_visibility_atlas: Resource::new(
                transport_source_visibility_atlas,
            ),
            ddgi_global_sky_irradiance: Resource::new(global_sky_irradiance),
            ddgi_global_sky_irradiance_alt: Resource::new(global_sky_irradiance_alt),
            ddgi_radiance_sun: Resource::new(radiance_sun),
            ddgi_radiance_voxel_palette: Resource::new(radiance_voxel_palette),
            ddgi_transport_query_info: Resource::new(transport_query_info),
            ddgi_local_light_info: Resource::new(ddgi_local_light_info),
            ddgi_local_lights: Resource::new(ddgi_local_lights),
            transport_query_snapshot,
        })
    }

    pub(crate) fn status(&self) -> DdgiVolumeStatus {
        DdgiVolumeStatus {
            build_token: self.build_token,
            grid: self.grid,
            irradiance_layout: self.irradiance_layout,
            visibility_layout: self.visibility_layout,
            resource_bytes: self.resource_bytes,
            stage: self.stage,
            scheduled_work: self.scheduled_work,
            complete_field: self.complete_field.map(|field| field.logical),
            published_field: self.published_field.map(|field| field.logical),
            building_field: self.building_iteration.map(|iteration| iteration.logical),
            consecutive_below_threshold: self.consecutive_below_threshold,
            last_atlas_validation: self.last_atlas_validation,
            global_sky_revision: self
                .published_field
                .map(|field| self.global_sky_revision(field.sky_slot))
                .or_else(|| {
                    self.building_iteration
                        .map(|iteration| self.global_sky_revision(iteration.destination.sky_slot))
                })
                .unwrap_or_default(),
            radiance_revision: self.radiance_revision,
            relocated_terrain_revision: self.relocated_terrain_revision,
            active_ray_batch: self.active_ray_batch,
            filtered_probe_count: self.filtered_probe_count,
            probe_priority: self
                .building_iteration
                .and_then(|iteration| iteration.probe_priority),
            promotion_ready: self.promotion_is_ready(),
        }
    }

    pub(crate) fn local_refresh_probe_partition(&self) -> Option<(u32, u32)> {
        self.local_refresh_voxel_bound
            .map(|bound| local_refresh_probe_partition(self.grid, bound))
    }

    fn promotion_is_ready(&self) -> bool {
        let Some(published) = self.published_field else {
            return false;
        };
        self.local_refresh_voxel_bound.is_none()
            || (published.logical.field().update_epoch() >= DDGI_LOCAL_RECOVERY_MIN_EPOCH
                && self.local_recovery_stable_epochs >= DDGI_LOCAL_RECOVERY_STABLE_EPOCHS)
    }

    fn finish_local_recovery(&mut self) {
        if self.local_refresh_voxel_bound.take().is_some() {
            assert!(
                self.promotion_is_ready_after_local_clear(),
                "DDGI local recovery cannot finish before its private candidate is stable"
            );
            self.history_mode = DdgiHistoryMode::TopologyRecovery;
            self.local_recovery_stable_epochs = 0;
        }
    }

    fn promotion_is_ready_after_local_clear(&self) -> bool {
        self.published_field.is_some_and(|published| {
            published.logical.field().update_epoch() >= DDGI_LOCAL_RECOVERY_MIN_EPOCH
                && self.local_recovery_stable_epochs >= DDGI_LOCAL_RECOVERY_STABLE_EPOCHS
        })
    }

    pub fn assign_build_token(&mut self, build_token: DdgiBuildToken) {
        assert!(
            self.build_token.is_none(),
            "DDGI build token may only be assigned once"
        );
        self.build_token = Some(build_token);
    }

    pub fn should_latch_radiance_snapshot(&self, latest_revision: u32) -> bool {
        self.scheduled_work.is_some_and(|work| {
            work.destination().field().radiance_revision() == latest_revision
                && self.radiance_revision != Some(latest_revision)
        })
    }

    pub fn latch_radiance_snapshot(
        &mut self,
        revision: u32,
        snapshot: DdgiRadianceSnapshot,
    ) -> Result<()> {
        ensure!(
            self.should_latch_radiance_snapshot(revision),
            "cannot replace DDGI radiance revision {:?} with {revision} while stage {:?} is in flight",
            self.radiance_revision,
            self.stage,
        );
        ensure!(
            snapshot.local_lights.info.transport_revision == revision,
            "DDGI local-light transport revision {} does not match radiance revision {revision}",
            snapshot.local_lights.info.transport_revision,
        );
        self.ddgi_radiance_sun.fill_uniform(&DdgiRadianceSun {
            direction: snapshot.sun_direction.to_array(),
            terrain_ray_origin_offset_world: snapshot.terrain_ray_origin_offset_world,
            color: snapshot.sun_color.to_array(),
            luminance: snapshot.sun_luminance,
        })?;
        self.ddgi_radiance_voxel_palette
            .fill_uniform(&DdgiRadianceVoxelPalette {
                dirt_color: snapshot.voxel_palette.dirt_color.to_array(),
                sand_color: snapshot.voxel_palette.sand_color.to_array(),
                cherry_wood_color: snapshot.voxel_palette.cherry_wood_color.to_array(),
                oak_wood_color: snapshot.voxel_palette.oak_wood_color.to_array(),
                rock_color: snapshot.voxel_palette.rock_color.to_array(),
                hash_color_variance: snapshot.voxel_palette.hash_color_variance,
                emissive_color: snapshot.voxel_palette.emissive_color.to_array(),
                emissive_radiance: snapshot.voxel_palette.emissive_radiance,
                ..DdgiRadianceVoxelPalette::zeroed()
            })?;
        self.transport_query_snapshot.visibility_bias_world =
            snapshot.ddgi_receiver_visibility_bias_world;
        self.ddgi_transport_query_info
            .fill_uniform(&self.transport_query_snapshot)?;
        self.ddgi_local_light_info
            .fill_uniform(&snapshot.local_lights.info)?;
        self.ddgi_local_lights.fill(&snapshot.local_lights.lights)?;
        self.radiance_revision = Some(revision);
        self.radiance_snapshot = Some(snapshot);
        Ok(())
    }

    pub(crate) fn radiance_snapshot(&self) -> Option<DdgiRadianceSnapshot> {
        self.radiance_snapshot
    }

    pub fn global_sky_needs_update(&self) -> bool {
        self.building_iteration.is_some_and(|iteration| {
            self.radiance_revision == Some(iteration.logical.field().radiance_revision())
                && self.global_sky_revision(iteration.destination.sky_slot)
                    != iteration.logical.field().radiance_revision()
        })
    }

    pub fn mark_global_sky_ready(&mut self, environment_revision: u32) -> Result<()> {
        let iteration = self
            .building_iteration
            .context("cannot publish DDGI global sky without scheduled work")?;
        ensure!(
            iteration.logical.field().radiance_revision() == environment_revision
                && self.radiance_revision == Some(environment_revision),
            "DDGI global sky revision {environment_revision} does not match building field {:?}",
            iteration.logical,
        );
        self.set_global_sky_revision(iteration.destination.sky_slot, environment_revision);
        self.stage = stage_after_global_sky_update(self.stage);
        Ok(())
    }

    /// Installs scheduler-authoritative work and derives only its physical residency here.
    pub fn begin_scheduled_work(
        &mut self,
        work: DdgiScheduledWork,
        local_refresh_voxel_bound: Option<UAabb3>,
        radiance_history_policy: Option<DdgiRadianceHistoryPolicy>,
        probe_priority: Option<DdgiProbePriority>,
    ) -> Result<()> {
        ensure!(
            self.scheduled_work.is_none() && self.building_iteration.is_none(),
            "DDGI volume already owns scheduled work {:?}",
            self.scheduled_work,
        );
        let destination = work.destination();
        ensure!(
            destination.field().spacing_voxels() == self.grid.spacing_voxels(),
            "DDGI work spacing {} does not match volume spacing {}",
            destination.field().spacing_voxels(),
            self.grid.spacing_voxels(),
        );

        let radiance_changed = self.published_field.is_some_and(|source| {
            destination.field().radiance_revision() != source.logical.field().radiance_revision()
        });
        self.transport_query_snapshot.geometry_revision = destination.field().geometry_revision();
        match work.kind() {
            DdgiScheduledWorkKind::GeometryUpdate => {
                self.local_refresh_voxel_bound = local_refresh_voxel_bound;
                self.local_recovery_stable_epochs = 0;
                self.history_mode = DdgiHistoryMode::Accumulating;
            }
            DdgiScheduledWorkKind::DensityUpdate => {
                ensure!(
                    local_refresh_voxel_bound.is_none(),
                    "density DDGI work cannot install a local refresh bound"
                );
                // This physical volume may have been the previous active geometry volume. Do not
                // carry its completed or preempted topology-recovery region into a density build.
                self.local_refresh_voxel_bound = None;
                self.local_recovery_stable_epochs = 0;
                self.history_mode = DdgiHistoryMode::Accumulating;
            }
            DdgiScheduledWorkKind::RadianceUpdate | DdgiScheduledWorkKind::ConvergenceUpdate => {
                ensure!(
                    local_refresh_voxel_bound.is_none(),
                    "temporal DDGI work inherits rather than replaces its local refresh bound"
                );
                if radiance_changed {
                    self.history_mode = DdgiHistoryMode::Accumulating;
                }
            }
        }
        let resident = resident_iteration_for_work_with_policy(
            work,
            self.published_field,
            self.local_refresh_voxel_bound,
            self.history_mode,
            radiance_history_policy,
            probe_priority,
        )?;
        match work.kind() {
            DdgiScheduledWorkKind::GeometryUpdate | DdgiScheduledWorkKind::DensityUpdate => {
                ensure!(
                    self.complete_field.is_none(),
                    "initial DDGI update must not retain a current-revision complete field"
                );
                self.consecutive_below_threshold = 0;
                self.set_transport_source_ready(false)?;
            }
            DdgiScheduledWorkKind::RadianceUpdate | DdgiScheduledWorkKind::ConvergenceUpdate => {
                if radiance_changed {
                    self.consecutive_below_threshold = 0;
                }
                self.set_transport_source_ready(true)?;
                self.stage = DdgiVolumeStage::Rebuilding;
            }
        }
        self.scheduled_work = Some(work);
        self.building_iteration = Some(resident);
        self.visibility_preserved_for_iteration = false;
        self.filtered_probe_count = 0;
        self.next_batch_ordinal = 0;
        self.active_ray_batch = None;
        self.last_atlas_validation = None;
        Ok(())
    }

    pub fn request_initialization(&mut self, terrain_revision: u32) -> bool {
        if initialization_request_is_duplicate(
            self.stage,
            self.requested_terrain_revision,
            terrain_revision,
        ) {
            return false;
        }

        self.requested_terrain_revision = Some(terrain_revision);
        self.relocated_terrain_revision = None;
        self.active_ray_batch = None;
        self.filtered_probe_count = 0;
        self.next_batch_ordinal = 0;
        self.scheduled_work = None;
        self.building_iteration = None;
        self.complete_field = None;
        self.published_field = None;
        self.consecutive_below_threshold = 0;
        self.last_atlas_validation = None;
        self.local_recovery_stable_epochs = 0;
        self.history_mode = DdgiHistoryMode::Accumulating;
        self.local_refresh_voxel_bound = None;
        self.stage = DdgiVolumeStage::RelocationPending;
        true
    }

    pub fn pending_relocation_terrain_revision(&self) -> Option<u32> {
        (self.stage == DdgiVolumeStage::RelocationPending)
            .then_some(self.requested_terrain_revision)
            .flatten()
    }

    pub fn mark_relocated(&mut self, terrain_revision: u32) -> Result<()> {
        assert_eq!(self.requested_terrain_revision, Some(terrain_revision));
        self.relocated_terrain_revision = Some(terrain_revision);
        self.filtered_probe_count = 0;
        self.next_batch_ordinal = 0;
        ensure!(
            self.building_iteration.is_some(),
            "DDGI relocation completed before scheduler work was installed"
        );
        self.stage = DdgiVolumeStage::Relocated;
        Ok(())
    }

    pub fn next_ray_batch_to_trace(&self) -> Option<DdgiRayBatch> {
        if !matches!(
            self.stage,
            DdgiVolumeStage::Relocated | DdgiVolumeStage::Rebuilding
        ) || self.active_ray_batch.is_some()
            || self.visibility_preservation_needed()
            || self.filtered_probe_count >= self.grid.probe_count()
        {
            return None;
        }
        let resident = self.building_iteration?;
        let priority_physical_ordinal = priority_probe_batch(
            self.grid,
            resident.probe_priority.map(DdgiProbePriority::voxel_bound),
            DDGI_PROBE_BATCH_SIZE,
        );
        let (first_probe_index, probe_count) = ddgi_probe_batch_range(
            self.grid.probe_count(),
            DDGI_PROBE_BATCH_SIZE,
            self.next_batch_ordinal,
            self.batch_order,
            priority_physical_ordinal,
        )?;
        Some(DdgiRayBatch {
            first_probe_index,
            probe_count,
            resident,
        })
    }

    pub fn visibility_preservation_needed(&self) -> bool {
        self.building_iteration.is_some_and(|iteration| {
            DdgiRayBatch {
                first_probe_index: 0,
                probe_count: 0,
                resident: iteration,
            }
            .needs_visibility_preservation()
                && !self.visibility_preserved_for_iteration
        })
    }

    pub fn record_visibility_preservation(&self, cmdbuf: &re_flora_vkn::CommandBuffer) {
        assert!(self.visibility_preservation_needed());
        let iteration = self
            .building_iteration
            .expect("visibility preservation requires a building iteration");
        let source = iteration
            .source
            .expect("visibility preservation requires a resident source");
        self.visibility_atlas(source.atlas_slot)
            .get_image()
            .record_copy_to(
                cmdbuf,
                self.visibility_atlas(iteration.destination.atlas_slot)
                    .get_image(),
                TextureLayout::GENERAL,
                TextureLayout::GENERAL,
            );
    }

    pub fn mark_visibility_preserved(&mut self) {
        assert!(self.visibility_preservation_needed());
        self.visibility_preserved_for_iteration = true;
    }

    pub fn iteration_will_complete(&self, batch: DdgiRayBatch) -> bool {
        assert_eq!(self.next_ray_batch_to_trace(), Some(batch));
        // Completion is authoritative volume progress, independent of the batch's atlas offset.
        // A later reverse-order scheduler can change `first_probe_index` without changing this
        // gate or accidentally running reduction before every probe has completed.
        iteration_completes_after_batch(
            self.filtered_probe_count,
            batch.probe_count,
            self.grid.probe_count(),
        )
    }

    pub fn mark_ray_batch_ready(&mut self, batch: DdgiRayBatch) {
        assert_eq!(self.next_ray_batch_to_trace(), Some(batch));
        self.active_ray_batch = Some(batch);
        self.stage = DdgiVolumeStage::RayBatchReady;
    }

    pub fn mark_ray_batch_filtered(&mut self, batch: DdgiRayBatch) {
        assert_eq!(self.stage, DdgiVolumeStage::RayBatchReady);
        assert_eq!(self.active_ray_batch, Some(batch));
        self.filtered_probe_count += batch.probe_count;
        self.next_batch_ordinal += 1;
        // Keep the exact batch identity live until the later-frame trace-stat readback has been
        // validated. This prevents the next batch, iteration advance, and publication from
        // overtaking GPU validation.
        self.stage = if self.filtered_probe_count == self.grid.probe_count() {
            DdgiVolumeStage::AtlasReady
        } else {
            DdgiVolumeStage::Rebuilding
        };
    }

    pub fn pending_trace_stats_batch_is(&self, batch: DdgiRayBatch) -> bool {
        pending_trace_stats_batch_matches(self.active_ray_batch, self.stage, batch)
    }

    pub fn mark_trace_stats_verified(
        &mut self,
        batch: DdgiRayBatch,
    ) -> Result<DdgiVerifiedBatchOutcome> {
        ensure!(
            self.pending_trace_stats_batch_is(batch),
            "stale DDGI trace-stat readback identity {batch:?}; current batch={:?} stage={:?}",
            self.active_ray_batch,
            self.stage,
        );
        ensure!(
            self.filtered_probe_count <= self.grid.probe_count(),
            "DDGI iteration filtered {}/{} probes",
            self.filtered_probe_count,
            self.grid.probe_count(),
        );
        if self.filtered_probe_count < self.grid.probe_count() {
            self.active_ray_batch = None;
            self.stage = DdgiVolumeStage::Rebuilding;
            return Ok(DdgiVerifiedBatchOutcome::Continue);
        }
        let identity = self
            .building_iteration
            .context("DDGI full iteration lost its transport identity")?;
        ensure!(
            identity == batch.resident,
            "DDGI full iteration identity changed"
        );
        Ok(DdgiVerifiedBatchOutcome::AwaitingAtlasValidation(
            identity.logical,
        ))
    }

    pub fn mark_atlas_validated(
        &mut self,
        identity: DdgiFieldIdentity,
        stats: DdgiAtlasValidationStats,
        policy: DdgiConvergencePolicy,
    ) -> Result<DdgiValidatedIterationOutcome> {
        let classified_field = self.preview_validated_field(identity, stats, policy)?;
        self.publish_validated_field(identity, classified_field, stats, policy)
    }

    /// Classifies a completed GPU iteration without mutating residency. The runtime uses this to
    /// ask the scheduler whether the completion is still authoritative before publication.
    pub fn preview_validated_field(
        &self,
        identity: DdgiFieldIdentity,
        stats: DdgiAtlasValidationStats,
        policy: DdgiConvergencePolicy,
    ) -> Result<DdgiFieldIdentity> {
        ensure!(
            self.active_ray_batch
                .is_some_and(|batch| batch.logical() == identity)
                && self.stage == DdgiVolumeStage::AtlasReady
                && self.filtered_probe_count == self.grid.probe_count(),
            "stale DDGI atlas validation identity {identity:?}; batch={:?} stage={:?} filtered={}/{}",
            self.active_ray_batch,
            self.stage,
            self.filtered_probe_count,
            self.grid.probe_count(),
        );
        ensure!(
            self.building_iteration
                .is_some_and(|iteration| iteration.logical == identity),
            "DDGI atlas validation no longer matches the builder iteration"
        );
        validate_atlas_stats(stats)?;
        ensure!(
            identity.field().state() == DdgiFieldState::Converging,
            "converged DDGI field cannot be used as a build destination: {identity:?}"
        );
        if identity.source().is_none() {
            return Ok(identity);
        }
        match classify_temporal_epoch(
            policy,
            identity.field().update_epoch(),
            self.consecutive_below_threshold,
            stats,
        ) {
            DdgiConvergenceDecision::Continue { .. } => Ok(identity),
            DdgiConvergenceDecision::Converged { .. } => identity
                .with_state(DdgiFieldState::Converged)
                .map_err(|error| anyhow::anyhow!("invalid converged field: {error:?}")),
        }
    }

    fn publish_validated_field(
        &mut self,
        identity: DdgiFieldIdentity,
        classified_field: DdgiFieldIdentity,
        stats: DdgiAtlasValidationStats,
        policy: DdgiConvergencePolicy,
    ) -> Result<DdgiValidatedIterationOutcome> {
        let iteration = self
            .building_iteration
            .context("DDGI atlas validation lost resident iteration")?;
        let previous_complete = self.complete_field;
        self.active_ray_batch = None;
        self.complete_field = Some(iteration.destination);
        self.last_atlas_validation = Some(stats);
        self.filtered_probe_count = 0;
        self.next_batch_ordinal = 0;
        if self.local_refresh_voxel_bound.is_some()
            && identity.field().update_epoch() >= DDGI_LOCAL_RECOVERY_MIN_EPOCH
            && stats.max_absolute_rgb_delta <= DDGI_LOCAL_RECOVERY_MAX_ABSOLUTE_DELTA
        {
            self.local_recovery_stable_epochs = self.local_recovery_stable_epochs.saturating_add(1);
        } else if self.local_refresh_voxel_bound.is_some() {
            self.local_recovery_stable_epochs = 0;
        }

        if identity.source().is_none() {
            ensure!(
                iteration.source.is_none() && previous_complete.is_none(),
                "initial DDGI epoch must not consume a current-revision field"
            );
            self.published_field = Some(iteration.destination);
            self.consecutive_below_threshold = 0;
            self.building_iteration = None;
            self.scheduled_work = None;
            self.stage = DdgiVolumeStage::Ready;
            return Ok(DdgiValidatedIterationOutcome::Published {
                work: iteration.work,
                field: identity,
                consecutive_below_threshold: 0,
            });
        }

        let inherited_geometry_source = iteration.work.kind()
            == DdgiScheduledWorkKind::GeometryUpdate
            && previous_complete.is_none()
            && iteration.source.is_some();
        ensure!(
            inherited_geometry_source || iteration.source == previous_complete,
            "DDGI temporal epoch {} did not consume the expected complete field",
            identity.field().update_epoch()
        );
        match classify_temporal_epoch(
            policy,
            identity.field().update_epoch(),
            self.consecutive_below_threshold,
            stats,
        ) {
            DdgiConvergenceDecision::Continue {
                consecutive_below_threshold,
            } => {
                self.consecutive_below_threshold = consecutive_below_threshold;
                self.published_field = Some(iteration.destination);
                self.building_iteration = None;
                self.scheduled_work = None;
                self.stage = DdgiVolumeStage::Ready;
                Ok(DdgiValidatedIterationOutcome::Published {
                    work: iteration.work,
                    field: identity,
                    consecutive_below_threshold,
                })
            }
            DdgiConvergenceDecision::Converged {
                consecutive_below_threshold,
                reason,
            } => {
                let field = classified_field;
                ensure!(
                    field.field().state() == DdgiFieldState::Converged,
                    "DDGI convergence preview changed before publication"
                );
                self.complete_field = Some(DdgiResidentField {
                    logical: field,
                    ..iteration.destination
                });
                self.published_field = self.complete_field;
                self.history_mode = DdgiHistoryMode::Stable;
                self.building_iteration = None;
                self.scheduled_work = None;
                self.consecutive_below_threshold = consecutive_below_threshold;
                self.stage = DdgiVolumeStage::Ready;
                Ok(DdgiValidatedIterationOutcome::Converged {
                    work: iteration.work,
                    field,
                    reason,
                })
            }
        }
    }

    fn irradiance_atlas(&self, slot: DdgiAtlasSlot) -> &Resource<Texture> {
        match slot {
            DdgiAtlasSlot::Atlas0 => &self.ddgi_irradiance_atlas,
            DdgiAtlasSlot::Atlas1 => &self.ddgi_transport_source_irradiance_atlas,
        }
    }

    fn visibility_atlas(&self, slot: DdgiAtlasSlot) -> &Resource<Texture> {
        match slot {
            DdgiAtlasSlot::Atlas0 => &self.ddgi_visibility_atlas,
            DdgiAtlasSlot::Atlas1 => &self.ddgi_transport_source_visibility_atlas,
        }
    }

    pub fn published_irradiance_atlas(&self) -> Option<&Resource<Texture>> {
        self.published_field
            .map(|field| self.irradiance_atlas(field.atlas_slot))
    }

    pub fn published_visibility_atlas(&self) -> Option<&Resource<Texture>> {
        self.published_field
            .map(|field| self.visibility_atlas(field.atlas_slot))
    }

    pub fn published_irradiance_label(&self) -> Option<&'static str> {
        self.published_field.map(|field| field.atlas_slot.label())
    }

    pub fn building_global_sky_irradiance(&self) -> &Resource<Texture> {
        self.global_sky_irradiance(
            self.building_iteration
                .map(|iteration| iteration.destination.sky_slot)
                .or_else(|| self.published_field.map(|field| field.sky_slot))
                .unwrap_or(DdgiSkySlot::Sky0),
        )
    }

    pub fn published_global_sky_irradiance(&self) -> &Resource<Texture> {
        self.global_sky_irradiance(
            self.published_field
                .map(|field| field.sky_slot)
                .unwrap_or(DdgiSkySlot::Sky0),
        )
    }

    fn global_sky_irradiance(&self, slot: DdgiSkySlot) -> &Resource<Texture> {
        match slot {
            DdgiSkySlot::Sky0 => &self.ddgi_global_sky_irradiance,
            DdgiSkySlot::Sky1 => &self.ddgi_global_sky_irradiance_alt,
        }
    }

    fn global_sky_revision(&self, slot: DdgiSkySlot) -> u32 {
        self.global_sky_revisions[match slot {
            DdgiSkySlot::Sky0 => 0,
            DdgiSkySlot::Sky1 => 1,
        }]
    }

    fn set_global_sky_revision(&mut self, slot: DdgiSkySlot, revision: u32) {
        self.global_sky_revisions[match slot {
            DdgiSkySlot::Sky0 => 0,
            DdgiSkySlot::Sky1 => 1,
        }] = revision;
    }

    fn set_transport_source_ready(&mut self, ready: bool) -> Result<()> {
        self.transport_query_snapshot.source_ready = u32::from(ready);
        self.ddgi_transport_query_info
            .fill_uniform(&self.transport_query_snapshot)
    }

    /// Declares CPU writes before the frame's reflected DDGI descriptors consume them.
    pub fn record_cpu_buffer_writes(&self, cmdbuf: &re_flora_vkn::CommandBuffer) {
        for buffer in [
            &*self.ddgi_radiance_sun,
            &*self.ddgi_radiance_voxel_palette,
            &*self.ddgi_transport_query_info,
            &*self.ddgi_local_light_info,
            &*self.ddgi_local_lights,
        ] {
            cmdbuf.use_buffer(buffer, BufferUse::HostWrite);
        }
    }

    pub fn record_trace_stats_readback(&self, cmdbuf: &re_flora_vkn::CommandBuffer) {
        self.ddgi_trace_stats.record_copy_to_buffer(
            cmdbuf,
            &self.ddgi_trace_stats_readback,
            self.resource_bytes.trace_stats,
            0,
            0,
        );
        cmdbuf.use_buffer(&self.ddgi_trace_stats_readback, BufferUse::HostRead);
    }

    pub fn record_relocation_stats_readback(&self, cmdbuf: &re_flora_vkn::CommandBuffer) {
        self.ddgi_relocation_stats.record_copy_to_buffer(
            cmdbuf,
            &self.ddgi_relocation_stats_readback,
            self.resource_bytes.relocation_stats,
            0,
            0,
        );
        cmdbuf.use_buffer(&self.ddgi_relocation_stats_readback, BufferUse::HostRead);
    }

    pub fn update_relocation_stats_from_readback(&self) -> Result<DdgiRelocationReadbackStats> {
        let bytes = self.ddgi_relocation_stats_readback.read_back()?;
        ensure!(
            bytes.len() == self.resource_bytes.relocation_stats as usize,
            "DDGI relocation stats readback returned {} bytes, expected {}",
            bytes.len(),
            self.resource_bytes.relocation_stats,
        );
        let mut values = [0_u32; DDGI_RELOCATION_STATS_COUNT];
        for (value, bytes) in values.iter_mut().zip(bytes.chunks_exact(4)) {
            *value = u32::from_ne_bytes(bytes.try_into().expect("u32-sized chunk"));
        }
        Ok(DdgiRelocationReadbackStats::from_array(values))
    }

    pub fn record_atlas_reduction_readback(&self, cmdbuf: &re_flora_vkn::CommandBuffer) {
        self.ddgi_atlas_reduction.record_copy_to_buffer(
            cmdbuf,
            &self.ddgi_atlas_reduction_readback,
            self.resource_bytes.atlas_reduction,
            0,
            0,
        );
        cmdbuf.use_buffer(&self.ddgi_atlas_reduction_readback, BufferUse::HostRead);
    }

    pub fn update_atlas_validation_from_readback(&self) -> Result<DdgiAtlasValidationStats> {
        let bytes = self.ddgi_atlas_reduction_readback.read_back()?;
        ensure!(
            bytes.len() == self.resource_bytes.atlas_reduction as usize,
            "DDGI atlas reduction readback returned {} bytes, expected {}",
            bytes.len(),
            self.resource_bytes.atlas_reduction,
        );
        let mut values = [0_u32; DDGI_ATLAS_REDUCTION_COUNT];
        for (value, bytes) in values.iter_mut().zip(bytes.chunks_exact(4)) {
            *value = u32::from_ne_bytes(bytes.try_into().expect("u32-sized chunk"));
        }
        Ok(DdgiAtlasValidationStats::from_array(values))
    }

    pub fn update_trace_stats_from_readback(&self) -> Result<DdgiTraceStats> {
        let bytes = self.ddgi_trace_stats_readback.read_back()?;
        ensure!(
            bytes.len() == self.resource_bytes.trace_stats as usize,
            "DDGI trace stats readback returned {} bytes, expected {}",
            bytes.len(),
            self.resource_bytes.trace_stats,
        );
        let mut values = [0_u32; DDGI_TRACE_STATS_COUNT];
        for (value, bytes) in values.iter_mut().zip(bytes.chunks_exact(4)) {
            *value = u32::from_ne_bytes(bytes.try_into().expect("u32-sized chunk"));
        }
        let stats = DdgiTraceStats::from_array(values);
        ensure!(
            stats.ray_records
                == stats
                    .valid_probe_rays
                    .saturating_add(stats.invalid_probe_rays),
            "DDGI trace stats ray partition is inconsistent: {stats:?}",
        );
        ensure!(
            stats.valid_probe_rays
                == stats
                    .misses
                    .saturating_add(stats.frontface_hits)
                    .saturating_add(stats.backface_hits),
            "DDGI trace stats hit partition is inconsistent: {stats:?}",
        );
        ensure!(
            stats.local_light_candidates
                == stats
                    .local_light_visible
                    .saturating_add(stats.local_light_occluded),
            "DDGI trace stats local-light visibility partition is inconsistent: {stats:?}",
        );
        Ok(stats)
    }
}

fn pending_trace_stats_batch_matches(
    active_batch: Option<DdgiRayBatch>,
    stage: DdgiVolumeStage,
    candidate: DdgiRayBatch,
) -> bool {
    active_batch == Some(candidate)
        && matches!(
            stage,
            DdgiVolumeStage::Rebuilding | DdgiVolumeStage::AtlasReady
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DdgiConvergenceDecision {
    Continue {
        consecutive_below_threshold: u32,
    },
    Converged {
        consecutive_below_threshold: u32,
        reason: DdgiConvergenceReason,
    },
}

fn classify_temporal_epoch(
    policy: DdgiConvergencePolicy,
    update_epoch: u32,
    previous_consecutive_below_threshold: u32,
    stats: DdgiAtlasValidationStats,
) -> DdgiConvergenceDecision {
    let below = stats.max_absolute_rgb_delta <= policy.absolute_threshold
        && stats.max_relative_rgb_delta <= policy.relative_threshold;
    let consecutive_below_threshold = if below {
        previous_consecutive_below_threshold + 1
    } else {
        0
    };
    let completed_epoch_count = update_epoch.saturating_add(1);
    let threshold_converged = completed_epoch_count >= policy.minimum_update_epochs
        && consecutive_below_threshold >= policy.consecutive_epochs;
    let sample_budget_complete = completed_epoch_count >= policy.maximum_update_epochs;
    if threshold_converged || sample_budget_complete {
        DdgiConvergenceDecision::Converged {
            consecutive_below_threshold,
            reason: if threshold_converged {
                DdgiConvergenceReason::Threshold
            } else {
                DdgiConvergenceReason::SampleBudget
            },
        }
    } else {
        DdgiConvergenceDecision::Continue {
            consecutive_below_threshold,
        }
    }
}

fn initialization_request_is_duplicate(
    stage: DdgiVolumeStage,
    requested_terrain_revision: Option<u32>,
    terrain_revision: u32,
) -> bool {
    requested_terrain_revision == Some(terrain_revision)
        && matches!(
            stage,
            DdgiVolumeStage::RelocationPending
                | DdgiVolumeStage::Relocated
                | DdgiVolumeStage::RayBatchReady
                | DdgiVolumeStage::AtlasReady
                | DdgiVolumeStage::Ready
        )
}

fn stage_after_global_sky_update(stage: DdgiVolumeStage) -> DdgiVolumeStage {
    match stage {
        DdgiVolumeStage::Allocated | DdgiVolumeStage::GlobalSkyReady => {
            DdgiVolumeStage::GlobalSkyReady
        }
        DdgiVolumeStage::RelocationPending => DdgiVolumeStage::RelocationPending,
        DdgiVolumeStage::Relocated
        | DdgiVolumeStage::RayBatchReady
        | DdgiVolumeStage::AtlasReady
        | DdgiVolumeStage::Rebuilding
        | DdgiVolumeStage::Ready => DdgiVolumeStage::Rebuilding,
    }
}

fn atlas_image_desc(
    layout: DdgiAtlasLayout,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
) -> ImageDesc {
    let extent = layout.extent();
    ImageDesc {
        extent: Extent3D::new(extent.x, extent.y, 1),
        format,
        usage,
        initial_layout: TextureLayout::UNDEFINED,
        aspect: vk::ImageAspectFlags::COLOR,
        ..Default::default()
    }
}

fn ddgi_atlas_image_usage() -> vk::ImageUsageFlags {
    vk::ImageUsageFlags::SAMPLED
        | vk::ImageUsageFlags::STORAGE
        | vk::ImageUsageFlags::TRANSFER_SRC
        | vk::ImageUsageFlags::TRANSFER_DST
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initial_work(
        geometry_revision: u32,
        radiance_revision: u32,
        spacing_voxels: u32,
    ) -> DdgiScheduledWork {
        let mut scheduler = super::super::DdgiTransportScheduler::new();
        scheduler.observe_radiance(radiance_revision);
        scheduler.request_geometry(geometry_revision, spacing_voxels);
        scheduler.claim_next().unwrap().unwrap()
    }

    #[test]
    fn spacing_32_resource_contract_is_full_precision_and_batch_bounded() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 32).unwrap();
        let irradiance =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_IRRADIANCE_INTERIOR_SIDE).unwrap();
        let visibility =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_VISIBILITY_INTERIOR_SIDE).unwrap();
        let bytes = DdgiResourceBytes::new(grid, irradiance, visibility);

        assert_eq!(irradiance.extent(), glam::UVec2::new(710, 700));
        assert_eq!(visibility.extent(), glam::UVec2::new(1_278, 1_260));
        assert_eq!(bytes.irradiance_atlas, 7_952_000);
        assert_eq!(bytes.transport_source_irradiance_atlas, 7_952_000);
        assert_eq!(bytes.visibility_atlas, 12_882_240);
        assert_eq!(bytes.transport_source_visibility_atlas, 12_882_240);
        assert_eq!(bytes.probe_metadata, 235_824);
        assert_eq!(bytes.transient_ray_data, 524_288);
        assert_eq!(bytes.trace_stats, 52);
        assert_eq!(bytes.relocation_stats, 56);
        assert_eq!(bytes.atlas_reduction, 28);
        assert_eq!(bytes.global_sky_irradiance, 3_200);
        assert_eq!(bytes.radiance_sun, 32);
        assert_eq!(bytes.radiance_voxel_palette, 96);
        assert_eq!(bytes.transport_query_info, 48);
    }

    #[test]
    fn local_light_trace_totals_widen_complete_sweep_accumulation() {
        let batch = |candidates, visible, occluded, irradiance_luma_q8| DdgiTraceStats {
            local_light_candidates: candidates,
            local_light_visible: visible,
            local_light_occluded: occluded,
            local_light_irradiance_luma_q8: irradiance_luma_q8,
            ..Default::default()
        };
        let mut totals = DdgiLocalLightTraceTotals::default();
        totals.accumulate(batch(7, 5, 2, u32::MAX));
        totals.accumulate(batch(11, 3, 8, 256));

        assert_eq!(totals.candidates, 18);
        assert_eq!(totals.visible, 8);
        assert_eq!(totals.occluded, 10);
        assert_eq!(totals.irradiance_luma_q8, u64::from(u32::MAX) + 256);
        assert_eq!(
            totals.irradiance_luma(),
            (u64::from(u32::MAX) + 256) as f64 / 256.0
        );
    }

    #[test]
    fn trace_stats_decode_real_emissive_hit_counters() {
        let mut values = [0_u32; DDGI_TRACE_STATS_COUNT];
        values[11] = 37;
        values[12] = 9_472;

        let stats = DdgiTraceStats::from_array(values);

        assert_eq!(stats.emissive_surface_hits, 37);
        assert_eq!(stats.emissive_surface_radiance_luma_q8, 9_472);
    }

    #[test]
    fn texture_descriptors_use_required_oracle_formats() {
        let irradiance = DdgiAtlasLayout::new(1, DDGI_IRRADIANCE_INTERIOR_SIDE).unwrap();
        let visibility = DdgiAtlasLayout::new(1, DDGI_VISIBILITY_INTERIOR_SIDE).unwrap();
        let usage = ddgi_atlas_image_usage();
        let irradiance_desc = atlas_image_desc(irradiance, DDGI_IRRADIANCE_FORMAT, usage);
        let visibility_desc = atlas_image_desc(visibility, DDGI_VISIBILITY_FORMAT, usage);

        assert_eq!(irradiance_desc.format, vk::Format::R32G32B32A32_SFLOAT);
        assert_eq!(visibility_desc.format, vk::Format::R32G32_SFLOAT);
        assert_eq!(irradiance_desc.extent, Extent3D::new(10, 10, 1));
        assert_eq!(visibility_desc.extent, Extent3D::new(18, 18, 1));
        assert!(irradiance_desc.usage.contains(vk::ImageUsageFlags::STORAGE));
        assert!(visibility_desc.usage.contains(vk::ImageUsageFlags::SAMPLED));
        assert!(visibility_desc
            .usage
            .contains(vk::ImageUsageFlags::TRANSFER_SRC));
        assert!(visibility_desc
            .usage
            .contains(vk::ImageUsageFlags::TRANSFER_DST));
    }

    #[test]
    fn volume_is_not_ready_when_resources_are_only_allocated() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 32).unwrap();
        let status = DdgiVolumeStatus {
            build_token: None,
            grid,
            irradiance_layout: DdgiAtlasLayout::new(
                grid.probe_count(),
                DDGI_IRRADIANCE_INTERIOR_SIDE,
            )
            .unwrap(),
            visibility_layout: DdgiAtlasLayout::new(
                grid.probe_count(),
                DDGI_VISIBILITY_INTERIOR_SIDE,
            )
            .unwrap(),
            resource_bytes: DdgiResourceBytes::new(
                grid,
                DdgiAtlasLayout::new(grid.probe_count(), DDGI_IRRADIANCE_INTERIOR_SIDE).unwrap(),
                DdgiAtlasLayout::new(grid.probe_count(), DDGI_VISIBILITY_INTERIOR_SIDE).unwrap(),
            ),
            stage: DdgiVolumeStage::Allocated,
            scheduled_work: None,
            complete_field: None,
            published_field: None,
            building_field: None,
            consecutive_below_threshold: 0,
            last_atlas_validation: None,
            global_sky_revision: 0,
            radiance_revision: None,
            relocated_terrain_revision: None,
            active_ray_batch: None,
            filtered_probe_count: 0,
            probe_priority: None,
            promotion_ready: false,
        };
        assert!(!status.is_ready());
    }

    #[test]
    fn runtime_status_keeps_staging_out_of_the_consumer_view_until_ready() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 32).unwrap();
        let irradiance_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_IRRADIANCE_INTERIOR_SIDE).unwrap();
        let visibility_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_VISIBILITY_INTERIOR_SIDE).unwrap();
        let field_for = |terrain_revision| initial_work(terrain_revision, 3, 32).destination();
        let status_for = |stage: DdgiVolumeStage,
                          terrain_revision: u32,
                          published: bool|
         -> DdgiVolumeStatus {
            DdgiVolumeStatus {
                build_token: None,
                grid,
                irradiance_layout,
                visibility_layout,
                resource_bytes: DdgiResourceBytes::new(grid, irradiance_layout, visibility_layout),
                stage,
                scheduled_work: None,
                complete_field: published.then(|| field_for(terrain_revision)),
                published_field: published.then(|| field_for(terrain_revision)),
                building_field: None,
                consecutive_below_threshold: 0,
                last_atlas_validation: None,
                global_sky_revision: 3,
                radiance_revision: Some(3),
                relocated_terrain_revision: Some(terrain_revision),
                active_ray_batch: None,
                filtered_probe_count: 0,
                probe_priority: None,
                promotion_ready: published,
            }
        };

        let active = status_for(DdgiVolumeStage::Rebuilding, 7, true);
        let staging = status_for(DdgiVolumeStage::Rebuilding, 8, false);
        let status = DdgiStatus {
            active,
            staging: Some(staging),
        };

        assert_eq!(status.active(), active);
        assert_eq!(status.builder(), staging);
        assert!(!status.staging_is_ready());

        // A complete finite epoch zero can promote while a later epoch writes the other slot.
        let ready_staging = status_for(DdgiVolumeStage::Rebuilding, 8, true);
        let status = DdgiStatus {
            active,
            staging: Some(ready_staging),
        };
        assert_eq!(status.active().relocated_terrain_revision, Some(7));
        assert_eq!(status.builder().relocated_terrain_revision, Some(8));
        assert!(status.staging_is_ready());
    }

    #[test]
    fn sky_update_preserves_initialization_and_temporal_residency() {
        assert_eq!(
            stage_after_global_sky_update(DdgiVolumeStage::Allocated),
            DdgiVolumeStage::GlobalSkyReady
        );
        assert_eq!(
            stage_after_global_sky_update(DdgiVolumeStage::Ready),
            DdgiVolumeStage::Rebuilding
        );
        assert_eq!(
            stage_after_global_sky_update(DdgiVolumeStage::RelocationPending),
            DdgiVolumeStage::RelocationPending
        );
    }

    #[test]
    fn initialization_request_is_idempotent_for_the_same_terrain_revision() {
        assert!(!initialization_request_is_duplicate(
            DdgiVolumeStage::Allocated,
            None,
            7,
        ));
        assert!(initialization_request_is_duplicate(
            DdgiVolumeStage::RelocationPending,
            Some(7),
            7,
        ));
        assert!(!initialization_request_is_duplicate(
            DdgiVolumeStage::RelocationPending,
            Some(7),
            8,
        ));
    }

    #[test]
    fn physical_slots_are_derived_only_by_toggling_the_resident_source() {
        assert_eq!(DdgiAtlasSlot::Atlas0.other(), DdgiAtlasSlot::Atlas1);
        assert_eq!(DdgiAtlasSlot::Atlas1.other(), DdgiAtlasSlot::Atlas0);
        assert_eq!(DdgiSkySlot::Sky0.other(), DdgiSkySlot::Sky1);
        assert_eq!(DdgiSkySlot::Sky1.other(), DdgiSkySlot::Sky0);
    }

    #[test]
    fn resident_temporal_update_uses_the_exact_source_slot_and_rotates_sky_for_new_radiance() {
        let initial = initial_work(7, 3, 32);
        let published = DdgiResidentField {
            logical: initial.destination(),
            atlas_slot: DdgiAtlasSlot::Atlas0,
            sky_slot: DdgiSkySlot::Sky0,
        };
        let mut scheduler = super::super::DdgiTransportScheduler::new();
        scheduler.install_published(published.logical).unwrap();
        let same_radiance = scheduler.claim_next().unwrap().unwrap();
        let same = resident_iteration_for_work(
            same_radiance,
            Some(published),
            None,
            DdgiHistoryMode::Stable,
        )
        .unwrap();
        assert_eq!(same.source, Some(published));
        assert_eq!(same.destination.atlas_slot, DdgiAtlasSlot::Atlas1);
        assert_eq!(same.destination.sky_slot, DdgiSkySlot::Sky0);
        let same_batch = DdgiRayBatch {
            first_probe_index: 0,
            probe_count: 64,
            resident: same,
        };
        assert_eq!(same_batch.irradiance_history_retention(0.98), 0.98);
        assert_eq!(same_batch.visibility_history_retention(0.98), 0.98);
        let recovery_batch = DdgiRayBatch {
            resident: DdgiResidentIteration {
                history_mode: DdgiHistoryMode::TopologyRecovery,
                ..same
            },
            ..same_batch
        };
        assert_eq!(
            recovery_batch.irradiance_history_retention(0.99),
            DDGI_TOPOLOGY_RECOVERY_HISTORY_RETENTION
        );
        assert_eq!(
            recovery_batch.visibility_history_retention(0.99),
            DDGI_TOPOLOGY_RECOVERY_HISTORY_RETENTION
        );

        let published = same.destination;
        scheduler
            .complete_in_flight(same_radiance, same_radiance.destination())
            .unwrap();
        scheduler.observe_radiance(4);
        let new_radiance = scheduler.claim_next().unwrap().unwrap();
        let changed = resident_iteration_for_work(
            new_radiance,
            Some(published),
            None,
            DdgiHistoryMode::Stable,
        )
        .unwrap();
        assert_eq!(changed.source, Some(published));
        assert_eq!(changed.destination.atlas_slot, DdgiAtlasSlot::Atlas0);
        assert_eq!(changed.destination.sky_slot, DdgiSkySlot::Sky1);
        assert_eq!(changed.logical.field().update_epoch(), 0);
        let continuous_history = crate::environment_lighting::DdgiRadianceHistoryPolicy {
            change: crate::environment_lighting::DdgiRadianceChange {
                reason: crate::environment_lighting::DdgiRadianceChangeReason::ContinuousSun,
                delta: crate::environment_lighting::DdgiRadianceDelta {
                    sun_angle_radians: 1.0_f32.to_radians(),
                    ..Default::default()
                },
            },
            elapsed: std::time::Duration::from_millis(200),
        };
        let changed_batch = DdgiRayBatch {
            resident: resident_iteration_for_work_with_policy(
                new_radiance,
                Some(published),
                None,
                DdgiHistoryMode::Stable,
                Some(continuous_history),
                None,
            )
            .unwrap(),
            ..same_batch
        };
        assert!(changed_batch.irradiance_history_retention(0.98) > 0.0);
        assert_eq!(changed_batch.visibility_history_retention(0.98), 0.98);
        assert!(!changed_batch.writes_visibility());
        assert!(changed_batch.needs_visibility_preservation());
    }

    #[test]
    fn terrain_staging_reads_resident_history_through_the_external_source_slot() {
        let initial = initial_work(7, 3, 32).destination();
        let mut scheduler = super::super::DdgiTransportScheduler::new();
        scheduler.install_published(initial).unwrap();
        scheduler.request_geometry(8, 32);
        let work = scheduler.claim_next().unwrap().unwrap();
        let local_refresh = UAabb3::new(UVec3::splat(68), UVec3::splat(152));
        let resident = resident_iteration_for_work(
            work,
            None,
            Some(local_refresh),
            DdgiHistoryMode::Accumulating,
        )
        .unwrap();
        let batch = DdgiRayBatch {
            first_probe_index: 0,
            probe_count: 64,
            resident,
        };

        assert_eq!(batch.source(), Some(initial));
        assert_eq!(batch.source_slot_index(), DdgiAtlasSlot::Atlas1 as u32);
        assert_eq!(batch.destination_slot_index(), DdgiAtlasSlot::Atlas0 as u32);
        assert_eq!(batch.local_refresh_voxel_bound(), Some(local_refresh));
        assert_eq!(batch.irradiance_history_retention(0.99), 0.0);
        assert_eq!(batch.visibility_history_retention(0.99), 0.0);
    }

    #[test]
    fn every_epoch_updates_visibility_in_the_matching_ping_pong_slot() {
        let work = initial_work(7, 3, 32);
        let initial =
            resident_iteration_for_work(work, None, None, DdgiHistoryMode::Accumulating).unwrap();
        let initial_batch = DdgiRayBatch {
            first_probe_index: 0,
            probe_count: 64,
            resident: initial,
        };
        let published = initial.destination;
        let mut scheduler = super::super::DdgiTransportScheduler::new();
        scheduler.install_published(published.logical).unwrap();
        let temporal_work = scheduler.claim_next().unwrap().unwrap();
        let temporal_batch = DdgiRayBatch {
            resident: resident_iteration_for_work(
                temporal_work,
                Some(published),
                None,
                DdgiHistoryMode::Accumulating,
            )
            .unwrap(),
            ..initial_batch
        };
        assert!(initial_batch.writes_visibility());
        assert!(temporal_batch.writes_visibility());
    }

    #[test]
    fn epoch_rotation_is_unit_length_deterministic_and_epoch_scoped() {
        let first = ddgi_epoch_rotation(7, 3, 11);
        let repeated = ddgi_epoch_rotation(7, 3, 11);
        let next = ddgi_epoch_rotation(7, 3, 12);
        assert_eq!(first, repeated);
        assert_ne!(first, next);
        let norm_squared: f32 = first.into_iter().map(|value| value * value).sum();
        assert!((norm_squared - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn iteration_completion_uses_authoritative_completed_count_not_batch_offset() {
        assert!(!iteration_completes_after_batch(0, 64, 128));
        assert!(iteration_completes_after_batch(64, 64, 128));

        // The completion calculation intentionally has no `first_probe_index`; scheduling the
        // same two batches in reverse order still reaches completion after the second batch.
        let reverse_offsets = [64, 0];
        let mut completed_probe_count = 0;
        for (index, _first_probe_index) in reverse_offsets.into_iter().enumerate() {
            let completes = iteration_completes_after_batch(completed_probe_count, 64, 128);
            assert_eq!(completes, index == 1);
            completed_probe_count += 64;
        }
    }

    fn assert_batch_traversal_covers_every_probe_once(
        probe_count: u32,
        batch_size: u32,
        order: DdgiBatchOrder,
    ) -> Vec<(u32, u32)> {
        let mut ranges = Vec::new();
        let mut seen = vec![false; probe_count as usize];
        let mut processed_probe_count = 0;
        let mut ordinal = 0;
        while let Some((first_probe_index, batch_probe_count)) =
            ddgi_probe_batch_range(probe_count, batch_size, ordinal, order, None)
        {
            assert!(batch_probe_count > 0);
            assert!(first_probe_index + batch_probe_count <= probe_count);
            for probe_index in first_probe_index..first_probe_index + batch_probe_count {
                assert!(
                    !seen[probe_index as usize],
                    "probe {probe_index} visited twice"
                );
                seen[probe_index as usize] = true;
            }
            let completes = iteration_completes_after_batch(
                processed_probe_count,
                batch_probe_count,
                probe_count,
            );
            processed_probe_count += batch_probe_count;
            assert_eq!(completes, processed_probe_count == probe_count);
            ranges.push((first_probe_index, batch_probe_count));
            ordinal += 1;
        }
        assert_eq!(processed_probe_count, probe_count);
        assert!(seen.into_iter().all(|visited| visited));
        ranges
    }

    #[test]
    fn forward_and_reverse_batch_traversal_have_complete_identical_coverage() {
        for probe_count in [1, 64, 128, 129, 256, 257, 4_913] {
            let forward = assert_batch_traversal_covers_every_probe_once(
                probe_count,
                DDGI_PROBE_BATCH_SIZE,
                DdgiBatchOrder::Forward,
            );
            let reverse = assert_batch_traversal_covers_every_probe_once(
                probe_count,
                DDGI_PROBE_BATCH_SIZE,
                DdgiBatchOrder::Reverse,
            );
            assert_eq!(reverse, forward.iter().copied().rev().collect::<Vec<_>>());
        }

        let tail_probe_count = (4_913 - 1) % DDGI_PROBE_BATCH_SIZE + 1;
        let tail_first_probe = 4_913 - tail_probe_count;
        assert_eq!(
            assert_batch_traversal_covers_every_probe_once(
                4_913,
                DDGI_PROBE_BATCH_SIZE,
                DdgiBatchOrder::Reverse,
            )
            .first(),
            Some(&(tail_first_probe, tail_probe_count)),
            "reverse traversal must process the short tail as ordinal zero",
        );
        assert_eq!(
            ddgi_probe_batch_range(0, DDGI_PROBE_BATCH_SIZE, 0, DdgiBatchOrder::Forward, None,),
            None,
        );
    }

    #[test]
    fn immutable_priority_starts_near_the_bound_then_wraps_without_starvation() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 32).unwrap();
        let edit = UAabb3::new(UVec3::splat(240), UVec3::splat(272));
        let priority = priority_probe_batch(grid, Some(edit), DDGI_PROBE_BATCH_SIZE).unwrap();
        let center_coordinate = UVec3::splat(8);
        assert_eq!(
            priority,
            grid.flatten(center_coordinate).unwrap() / DDGI_PROBE_BATCH_SIZE
        );

        let batch_count = grid.probe_count().div_ceil(DDGI_PROBE_BATCH_SIZE);
        let forward = (0..batch_count)
            .map(|ordinal| {
                ddgi_probe_batch_range(
                    grid.probe_count(),
                    DDGI_PROBE_BATCH_SIZE,
                    ordinal,
                    DdgiBatchOrder::Forward,
                    Some(priority),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(forward[0].0 / DDGI_PROBE_BATCH_SIZE, priority);
        let visited: u32 = forward.iter().map(|(_, count)| count).sum();
        assert_eq!(visited, grid.probe_count());
    }

    #[test]
    fn convergence_requires_two_consecutive_epochs_below_both_thresholds() {
        let low = DdgiAtlasValidationStats {
            max_absolute_rgb_delta: DDGI_CONVERGENCE_POLICY.absolute_threshold,
            max_relative_rgb_delta: DDGI_CONVERGENCE_POLICY.relative_threshold,
            ..Default::default()
        };
        assert_eq!(
            classify_temporal_epoch(DDGI_CONVERGENCE_POLICY, 6, 0, low),
            DdgiConvergenceDecision::Continue {
                consecutive_below_threshold: 1
            }
        );
        assert_eq!(
            classify_temporal_epoch(DDGI_CONVERGENCE_POLICY, 7, 1, low),
            DdgiConvergenceDecision::Converged {
                consecutive_below_threshold: 2,
                reason: DdgiConvergenceReason::Threshold,
            }
        );

        let high_relative = DdgiAtlasValidationStats {
            max_absolute_rgb_delta: 0.0,
            max_relative_rgb_delta: DDGI_CONVERGENCE_POLICY.relative_threshold * 2.0,
            ..Default::default()
        };
        assert_eq!(
            classify_temporal_epoch(DDGI_CONVERGENCE_POLICY, 7, 1, high_relative),
            DdgiConvergenceDecision::Continue {
                consecutive_below_threshold: 0
            }
        );

        assert_eq!(
            classify_temporal_epoch(
                DDGI_CONVERGENCE_POLICY,
                DDGI_CONVERGENCE_POLICY.maximum_update_epochs - 1,
                0,
                high_relative,
            ),
            DdgiConvergenceDecision::Converged {
                consecutive_below_threshold: 0,
                reason: DdgiConvergenceReason::SampleBudget,
            }
        );
    }

    #[test]
    fn full_atlas_validation_fails_closed_on_nonfinite_or_negative_rgb() {
        let finite = DdgiAtlasValidationStats {
            max_absolute_rgb_delta: 0.25,
            max_relative_rgb_delta: 0.5,
            max_rgb_value: 0.75,
            valid_texel_count: 64,
            scanned_stored_texel_count: 100,
            ..Default::default()
        };
        validate_atlas_stats(finite).unwrap();

        let nonfinite = DdgiAtlasValidationStats {
            non_finite_count: 1,
            ..finite
        };
        assert!(validate_atlas_stats(nonfinite)
            .unwrap_err()
            .to_string()
            .contains("non-finite"));

        let negative = DdgiAtlasValidationStats {
            negative_rgb_texel_count: 1,
            ..finite
        };
        assert!(validate_atlas_stats(negative)
            .unwrap_err()
            .to_string()
            .contains("negative RGB"));

        let all_black = DdgiAtlasValidationStats {
            max_rgb_value: 0.0,
            ..finite
        };
        assert!(validate_atlas_stats(all_black)
            .unwrap_err()
            .to_string()
            .contains("all-black"));
    }

    #[test]
    fn trace_stat_readback_requires_exact_batch_and_epoch_identity() {
        let work = initial_work(7, 3, 32);
        let initial = work.destination();
        let resident = DdgiResidentField {
            logical: initial,
            atlas_slot: DdgiAtlasSlot::Atlas0,
            sky_slot: DdgiSkySlot::Sky0,
        };
        let batch = DdgiRayBatch {
            first_probe_index: 64,
            probe_count: 64,
            resident: DdgiResidentIteration {
                work,
                logical: initial,
                source: None,
                destination: resident,
                local_refresh_voxel_bound: None,
                probe_priority: None,
                history_mode: DdgiHistoryMode::Accumulating,
                radiance_history_policy: None,
            },
        };
        assert!(pending_trace_stats_batch_matches(
            Some(batch),
            DdgiVolumeStage::Rebuilding,
            batch,
        ));
        assert!(!pending_trace_stats_batch_matches(
            Some(batch),
            DdgiVolumeStage::RayBatchReady,
            batch,
        ));
        assert!(!pending_trace_stats_batch_matches(
            Some(batch),
            DdgiVolumeStage::Rebuilding,
            DdgiRayBatch {
                resident: DdgiResidentIteration {
                    logical: DdgiFieldIdentity::new(
                        super::super::DdgiFieldKey::new(
                            99,
                            7,
                            3,
                            32,
                            DdgiFieldState::Converging,
                            1,
                        )
                        .unwrap(),
                        Some(initial.field()),
                    )
                    .unwrap(),
                    ..batch.resident
                },
                ..batch
            },
        ));
        assert!(!pending_trace_stats_batch_matches(
            None,
            DdgiVolumeStage::AtlasReady,
            batch,
        ));
    }
}
