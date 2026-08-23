use glam::Vec3;
use std::time::Duration;

use crate::lighting::{LocalLightGpuPayload, LocalLightInfluenceBound};

// Authored sky lighting is compiled into these shaders rather than supplied through a runtime
// uniform. Hash the authoritative sources so a capture or cached field can still name the exact
// sky model that produced it. Adding runtime sky controls later should replace this compilation-
// bound identity with their explicit snapshot values.
pub(crate) const DDGI_AUTHORED_SKY_MODEL_IDENTITY: u64 = authored_sky_model_identity();
const FNV1A64_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100000001b3;
pub(crate) const DDGI_TRANSPORT_MIN_PUBLICATION_INTERVAL: Duration = Duration::from_millis(200);
const DDGI_LARGE_SUN_STEP_ANGLE_RADIANS: f32 = 5.0_f32.to_radians();
const DDGI_LARGE_SUN_STEP_COLOR_RELATIVE: f32 = 0.25;
const DDGI_LARGE_SUN_STEP_LUMINANCE_RELATIVE: f32 = 0.35;
const DDGI_CONTINUOUS_HISTORY_TIME_CONSTANT_SECONDS: f32 = 1.5;
const DDGI_CONTINUOUS_HISTORY_MAX_CHANGE_REDUCTION: f32 = 0.35;

const fn authored_sky_model_identity() -> u64 {
    let mut hash = FNV1A64_OFFSET_BASIS;
    hash = hash_bytes(
        hash,
        include_bytes!("../shader/slang/sky_environment_data.slang"),
    );
    hash = hash_bytes(hash, include_bytes!("../shader/slang/skylight.slang"));
    hash = hash_bytes(
        hash,
        include_bytes!("../shader/slang/ddgi_global_sky_filter.slang"),
    );
    hash_bytes(
        hash,
        include_bytes!("../shader/slang/ddgi_probe_trace.slang"),
    )
}

const fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(FNV1A64_PRIME);
        index += 1;
    }
    hash
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DdgiVoxelPaletteSnapshot {
    pub dirt_color: Vec3,
    pub sand_color: Vec3,
    pub cherry_wood_color: Vec3,
    pub oak_wood_color: Vec3,
    pub rock_color: Vec3,
    pub hash_color_variance: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DdgiRadianceSnapshot {
    pub sun_direction: Vec3,
    pub sun_color: Vec3,
    pub sun_luminance: f32,
    pub terrain_ray_origin_offset_world: f32,
    pub ddgi_receiver_visibility_bias_world: f32,
    pub voxel_palette: DdgiVoxelPaletteSnapshot,
    pub local_lights: LocalLightGpuPayload,
}

impl DdgiRadianceSnapshot {
    fn identity(self) -> DdgiRadianceIdentity {
        self.identity_for_authored_sky(DDGI_AUTHORED_SKY_MODEL_IDENTITY)
    }

    fn identity_for_authored_sky(self, authored_sky_model_identity: u64) -> DdgiRadianceIdentity {
        DdgiRadianceIdentity {
            authored_sky_model_identity,
            sun_direction: self.sun_direction.to_array().map(f32::to_bits),
            sun_color: self.sun_color.to_array().map(f32::to_bits),
            sun_luminance: self.sun_luminance.to_bits(),
            terrain_ray_origin_offset_world: self.terrain_ray_origin_offset_world.to_bits(),
            ddgi_receiver_visibility_bias_world: self.ddgi_receiver_visibility_bias_world.to_bits(),
            dirt_color: self.voxel_palette.dirt_color.to_array().map(f32::to_bits),
            sand_color: self.voxel_palette.sand_color.to_array().map(f32::to_bits),
            cherry_wood_color: self
                .voxel_palette
                .cherry_wood_color
                .to_array()
                .map(f32::to_bits),
            oak_wood_color: self
                .voxel_palette
                .oak_wood_color
                .to_array()
                .map(f32::to_bits),
            rock_color: self.voxel_palette.rock_color.to_array().map(f32::to_bits),
            hash_color_variance: self.voxel_palette.hash_color_variance.to_bits(),
            local_lights: self.local_lights.for_radiance_identity(),
        }
    }
}

impl PartialEq for DdgiRadianceSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DdgiRadianceIdentity {
    authored_sky_model_identity: u64,
    sun_direction: [u32; 3],
    sun_color: [u32; 3],
    sun_luminance: u32,
    terrain_ray_origin_offset_world: u32,
    ddgi_receiver_visibility_bias_world: u32,
    dirt_color: [u32; 3],
    sand_color: [u32; 3],
    cherry_wood_color: [u32; 3],
    oak_wood_color: [u32; 3],
    rock_color: [u32; 3],
    hash_color_variance: u32,
    local_lights: LocalLightGpuPayload,
}

impl DdgiRadianceIdentity {
    fn non_solar_eq(self, other: Self) -> bool {
        self.authored_sky_model_identity == other.authored_sky_model_identity
            && self.terrain_ray_origin_offset_world == other.terrain_ray_origin_offset_world
            && self.ddgi_receiver_visibility_bias_world == other.ddgi_receiver_visibility_bias_world
            && self.dirt_color == other.dirt_color
            && self.sand_color == other.sand_color
            && self.cherry_wood_color == other.cherry_wood_color
            && self.oak_wood_color == other.oak_wood_color
            && self.rock_color == other.rock_color
            && self.hash_color_variance == other.hash_color_variance
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DdgiRadianceDelta {
    pub sun_angle_radians: f32,
    pub sun_color_relative: f32,
    pub sun_luminance_relative: f32,
    pub non_solar_changed: bool,
    pub local_lights_changed: bool,
    pub local_light_impact_bound: Option<LocalLightInfluenceBound>,
}

impl DdgiRadianceDelta {
    fn between(
        previous: DdgiRadianceSnapshot,
        next: DdgiRadianceSnapshot,
        non_solar_changed: bool,
        local_lights_changed: bool,
    ) -> Self {
        let previous_direction = previous.sun_direction.normalize_or_zero();
        let next_direction = next.sun_direction.normalize_or_zero();
        let sun_angle_radians = if previous_direction == Vec3::ZERO || next_direction == Vec3::ZERO
        {
            if previous_direction == next_direction {
                0.0
            } else {
                std::f32::consts::PI
            }
        } else {
            previous_direction
                .dot(next_direction)
                .clamp(-1.0, 1.0)
                .acos()
        };
        Self {
            sun_angle_radians,
            sun_color_relative: max_vec3_relative_delta(previous.sun_color, next.sun_color),
            sun_luminance_relative: relative_delta(previous.sun_luminance, next.sun_luminance),
            non_solar_changed,
            local_lights_changed,
            local_light_impact_bound: local_lights_changed
                .then(|| {
                    match (
                        previous.local_lights.influence_bound(),
                        next.local_lights.influence_bound(),
                    ) {
                        (Some(previous), Some(next)) => Some(previous.union(next)),
                        (Some(bound), None) | (None, Some(bound)) => Some(bound),
                        (None, None) => None,
                    }
                })
                .flatten(),
        }
    }

    fn is_large_sun_step(self) -> bool {
        self.sun_angle_radians >= DDGI_LARGE_SUN_STEP_ANGLE_RADIANS
            || self.sun_color_relative >= DDGI_LARGE_SUN_STEP_COLOR_RELATIVE
            || self.sun_luminance_relative >= DDGI_LARGE_SUN_STEP_LUMINANCE_RELATIVE
    }
}

fn relative_delta(previous: f32, next: f32) -> f32 {
    (next - previous).abs() / previous.abs().max(next.abs()).max(1.0e-4)
}

fn max_vec3_relative_delta(previous: Vec3, next: Vec3) -> f32 {
    relative_delta(previous.x, next.x)
        .max(relative_delta(previous.y, next.y))
        .max(relative_delta(previous.z, next.z))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DdgiRadianceChangeReason {
    #[default]
    Initial,
    ContinuousSun,
    LargeSunStep,
    LocalLights,
    TransportInputStep,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DdgiRadianceChange {
    pub reason: DdgiRadianceChangeReason,
    pub delta: DdgiRadianceDelta,
}

impl DdgiRadianceChange {
    pub fn between(previous: DdgiRadianceSnapshot, next: DdgiRadianceSnapshot) -> Self {
        let previous_identity = previous.identity();
        let next_identity = next.identity();
        let delta = DdgiRadianceDelta::between(
            previous,
            next,
            !previous_identity.non_solar_eq(next_identity),
            previous_identity.local_lights != next_identity.local_lights,
        );
        let reason = if delta.non_solar_changed {
            DdgiRadianceChangeReason::TransportInputStep
        } else if delta.is_large_sun_step() {
            DdgiRadianceChangeReason::LargeSunStep
        } else if delta.local_lights_changed {
            DdgiRadianceChangeReason::LocalLights
        } else {
            DdgiRadianceChangeReason::ContinuousSun
        };
        Self { reason, delta }
    }

    pub fn resets_irradiance_history(self) -> bool {
        matches!(
            self.reason,
            DdgiRadianceChangeReason::Initial
                | DdgiRadianceChangeReason::LargeSunStep
                | DdgiRadianceChangeReason::TransportInputStep
        )
    }
}

/// History policy for a complete immutable field transition. It is derived from the field that is
/// actually resident, not merely from the previous requested transport revision, so latest-wins
/// coalescing cannot understate elapsed time or accumulated sun movement.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DdgiRadianceHistoryPolicy {
    pub change: DdgiRadianceChange,
    pub elapsed: Duration,
}

impl DdgiRadianceHistoryPolicy {
    pub fn between(
        source: EnvironmentLightingState,
        destination: EnvironmentLightingState,
    ) -> Self {
        Self {
            change: DdgiRadianceChange::between(source.snapshot, destination.snapshot),
            elapsed: destination.published_at.saturating_sub(source.published_at),
        }
    }

    pub fn resets_history(self) -> bool {
        self.change.resets_irradiance_history()
    }

    pub fn retention(self, configured: f32) -> f32 {
        if self.resets_history() {
            return 0.0;
        }
        let elapsed_seconds = self.elapsed.as_secs_f32().max(1.0 / 240.0);
        let time_retention =
            (-elapsed_seconds / DDGI_CONTINUOUS_HISTORY_TIME_CONSTANT_SECONDS).exp();
        let normalized_change = (self.change.delta.sun_angle_radians
            / DDGI_LARGE_SUN_STEP_ANGLE_RADIANS)
            .max(self.change.delta.sun_color_relative / DDGI_LARGE_SUN_STEP_COLOR_RELATIVE)
            .max(self.change.delta.sun_luminance_relative / DDGI_LARGE_SUN_STEP_LUMINANCE_RELATIVE)
            .clamp(0.0, 1.0);
        let change_retention =
            1.0 - normalized_change * DDGI_CONTINUOUS_HISTORY_MAX_CHANGE_REDUCTION;
        configured
            .clamp(0.0, 0.99)
            .min(time_retention * change_retention)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LiveEnvironmentLightingState {
    pub revision: u64,
    pub observed_at: Duration,
    pub snapshot: DdgiRadianceSnapshot,
}

/// One immutable, scheduler-facing DDGI transport snapshot.
///
/// Direct lighting never reads this type: it continues to consume the current-frame sun uniform.
/// `source_live_revision` makes intentional transport lag observable without weakening the exact
/// revision-to-snapshot identity used by in-flight DDGI work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EnvironmentLightingState {
    pub revision: u32,
    pub source_live_revision: u64,
    pub published_at: Duration,
    pub snapshot: DdgiRadianceSnapshot,
    pub change: DdgiRadianceChange,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EnvironmentLightingUpdate {
    pub live: LiveEnvironmentLightingState,
    pub transport: EnvironmentLightingState,
    pub transport_published: bool,
    pub coalesced_live_revisions: u64,
}

impl EnvironmentLightingUpdate {
    pub fn revision_lag(self) -> u64 {
        self.live
            .revision
            .saturating_sub(self.transport.source_live_revision)
    }

    pub fn transport_age(self, now: Duration) -> Duration {
        now.saturating_sub(self.transport.published_at)
    }
}

#[derive(Debug, Default)]
pub(crate) struct EnvironmentLightingCache {
    current_live_revision: u64,
    current_transport_revision: u32,
    last_live_identity: Option<DdgiRadianceIdentity>,
    current_transport: Option<EnvironmentLightingState>,
    current_transport_identity: Option<DdgiRadianceIdentity>,
    pending_transport_identity: Option<DdgiRadianceIdentity>,
    coalesced_live_revisions: u64,
    last_observed_at: Option<Duration>,
}

impl EnvironmentLightingCache {
    pub(crate) fn revision_lag(&self) -> u64 {
        self.current_transport.map_or(0, |transport| {
            self.current_live_revision
                .saturating_sub(transport.source_live_revision)
        })
    }

    pub(crate) fn coalesced_live_revisions(&self) -> u64 {
        self.coalesced_live_revisions
    }

    pub fn update(
        &mut self,
        mut snapshot: DdgiRadianceSnapshot,
        observed_at: Duration,
    ) -> EnvironmentLightingUpdate {
        self.update_for_authored_sky(&mut snapshot, DDGI_AUTHORED_SKY_MODEL_IDENTITY, observed_at)
    }

    fn update_for_authored_sky(
        &mut self,
        snapshot: &mut DdgiRadianceSnapshot,
        authored_sky_model_identity: u64,
        observed_at: Duration,
    ) -> EnvironmentLightingUpdate {
        assert!(
            self.last_observed_at
                .is_none_or(|previous| observed_at >= previous),
            "Environment Lighting observations must use a monotonic clock"
        );
        self.last_observed_at = Some(observed_at);
        snapshot.sun_direction = snapshot.sun_direction.normalize_or_zero();
        let identity = snapshot.identity_for_authored_sky(authored_sky_model_identity);
        let live_changed = self.last_live_identity != Some(identity);
        if live_changed {
            self.current_live_revision = self.current_live_revision.wrapping_add(1).max(1);
            self.last_live_identity = Some(identity);
        }
        let live = LiveEnvironmentLightingState {
            revision: self.current_live_revision,
            observed_at,
            snapshot: *snapshot,
        };
        let mut transport_published = false;
        if self.current_transport.is_none() {
            self.publish_transport(
                live,
                identity,
                DdgiRadianceChange {
                    reason: DdgiRadianceChangeReason::Initial,
                    delta: DdgiRadianceDelta::default(),
                },
            );
            transport_published = true;
        } else if live_changed {
            if self.current_transport_identity == Some(identity) {
                if self.pending_transport_identity.take().is_some() {
                    self.coalesced_live_revisions = self.coalesced_live_revisions.saturating_add(1);
                }
            } else {
                let change = self.classify_transport_change(live.snapshot, identity);
                if change.resets_irradiance_history() {
                    if self.pending_transport_identity.take().is_some() {
                        self.coalesced_live_revisions =
                            self.coalesced_live_revisions.saturating_add(1);
                    }
                    self.publish_transport(live, identity, change);
                    transport_published = true;
                } else {
                    if self.pending_transport_identity.replace(identity).is_some() {
                        self.coalesced_live_revisions =
                            self.coalesced_live_revisions.saturating_add(1);
                    }
                }
            }
        }
        let publication_due = self.current_transport.is_some_and(|transport| {
            observed_at.saturating_sub(transport.published_at)
                >= DDGI_TRANSPORT_MIN_PUBLICATION_INTERVAL
        });
        if !transport_published && publication_due && self.pending_transport_identity.is_some() {
            let change = self.classify_transport_change(live.snapshot, identity);
            self.publish_transport(live, identity, change);
            transport_published = true;
        }
        EnvironmentLightingUpdate {
            live,
            transport: self
                .current_transport
                .expect("initial Environment Lighting observation must publish transport"),
            transport_published,
            coalesced_live_revisions: self.coalesced_live_revisions,
        }
    }

    fn classify_transport_change(
        &self,
        snapshot: DdgiRadianceSnapshot,
        identity: DdgiRadianceIdentity,
    ) -> DdgiRadianceChange {
        let current = self
            .current_transport
            .expect("transport change classification requires a current snapshot");
        let current_identity = self
            .current_transport_identity
            .expect("transport change classification requires a current identity");
        let mut change = DdgiRadianceChange::between(current.snapshot, snapshot);
        if !current_identity.non_solar_eq(identity) {
            change.delta.non_solar_changed = true;
            change.reason = DdgiRadianceChangeReason::TransportInputStep;
        }
        change
    }

    fn publish_transport(
        &mut self,
        live: LiveEnvironmentLightingState,
        identity: DdgiRadianceIdentity,
        change: DdgiRadianceChange,
    ) {
        self.current_transport_revision = self.current_transport_revision.wrapping_add(1).max(1);
        let mut snapshot = live.snapshot;
        snapshot.local_lights = snapshot
            .local_lights
            .with_transport_revision(self.current_transport_revision);
        self.current_transport = Some(EnvironmentLightingState {
            revision: self.current_transport_revision,
            source_live_revision: live.revision,
            published_at: live.observed_at,
            snapshot,
            change,
        });
        self.current_transport_identity = Some(identity);
        self.pending_transport_identity = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::{
        LocalLight, LocalLightBudget, LocalLightGpuSnapshot, LocalLightRegistry, PointLight,
    };

    fn gui_param<'a>(config: &'a toml::Value, id: &str) -> &'a toml::Value {
        config["section"]
            .as_array()
            .expect("GUI config must contain sections")
            .iter()
            .flat_map(|section| section["param"].as_array().into_iter().flatten())
            .find(|param| param["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("missing GUI parameter {id}"))
    }

    fn snapshot() -> DdgiRadianceSnapshot {
        DdgiRadianceSnapshot {
            sun_direction: Vec3::Y,
            sun_color: Vec3::new(1.0, 0.9, 0.8),
            sun_luminance: 2.0,
            terrain_ray_origin_offset_world: 0.005,
            ddgi_receiver_visibility_bias_world: 0.001,
            voxel_palette: DdgiVoxelPaletteSnapshot {
                dirt_color: Vec3::new(0.1, 0.2, 0.3),
                sand_color: Vec3::new(0.4, 0.5, 0.6),
                cherry_wood_color: Vec3::new(0.7, 0.2, 0.1),
                oak_wood_color: Vec3::new(0.2, 0.3, 0.1),
                rock_color: Vec3::splat(0.4),
                hash_color_variance: 0.5,
            },
            local_lights: LocalLightGpuPayload::empty(0),
        }
    }

    fn point_payload(domain: &LocalLightRegistry) -> crate::lighting::LocalLightGpuPayload {
        LocalLightGpuSnapshot::from_authoritative(
            &domain.snapshot(),
            LocalLightBudget::point_lights(1),
            0,
        )
        .payload()
    }

    #[test]
    fn local_light_transport_is_rate_limited_latest_wins_and_keeps_live_separate() {
        let mut cache = EnvironmentLightingCache::default();
        let mut lights = LocalLightRegistry::default();
        let mut initial = snapshot();
        initial.local_lights = point_payload(&lights);
        let first = cache.update(initial, Duration::ZERO);
        assert!(first.transport_published);
        assert_eq!(first.transport.snapshot.local_lights.count(), 0);

        let id = lights.add(LocalLight::Point(
            PointLight::new(Vec3::new(1.0, 2.0, 3.0), Vec3::ONE, 4.0, 0.05, 0.5).unwrap(),
        ));
        let mut added = snapshot();
        added.local_lights = point_payload(&lights);
        let pending = cache.update(added, Duration::from_millis(1));
        assert!(!pending.transport_published);
        assert_eq!(pending.live.snapshot.local_lights.count(), 1);
        assert_eq!(pending.transport.snapshot.local_lights.count(), 0);

        lights
            .update(
                id,
                LocalLight::Point(
                    PointLight::new(
                        Vec3::new(2.0, 2.0, 3.0),
                        Vec3::new(0.5, 0.75, 1.0),
                        8.0,
                        0.05,
                        0.5,
                    )
                    .unwrap(),
                ),
            )
            .unwrap();
        let mut moved = snapshot();
        moved.local_lights = point_payload(&lights);
        let coalesced = cache.update(moved, Duration::from_millis(2));
        assert!(!coalesced.transport_published);
        assert_eq!(coalesced.transport.snapshot.local_lights.count(), 0);

        let published = cache.update(moved, Duration::from_millis(200));
        assert!(published.transport_published);
        assert_eq!(
            published.transport.change.reason,
            DdgiRadianceChangeReason::LocalLights
        );
        assert_eq!(published.transport.snapshot.local_lights.count(), 1);
        assert_eq!(
            published.transport.snapshot.local_lights.source_revision(),
            lights.snapshot().revision()
        );
        assert_eq!(
            published
                .transport
                .snapshot
                .local_lights
                .info
                .transport_revision,
            published.transport.revision
        );
        assert_eq!(published.coalesced_live_revisions, 1);

        lights.remove(id).unwrap();
        let mut removed = snapshot();
        removed.local_lights = point_payload(&lights);
        let removal_pending = cache.update(removed, Duration::from_millis(201));
        assert!(!removal_pending.transport_published);
        assert_eq!(removal_pending.live.snapshot.local_lights.count(), 0);
        assert_eq!(removal_pending.transport.snapshot.local_lights.count(), 1);

        let removal_published = cache.update(removed, Duration::from_millis(400));
        assert!(removal_published.transport_published);
        assert_eq!(removal_published.transport.snapshot.local_lights.count(), 0);
        assert_eq!(
            removal_published
                .transport
                .snapshot
                .local_lights
                .source_revision(),
            lights.snapshot().revision()
        );
    }

    fn sample_linear_probe_field(position_in_probe_cells: f64) -> f64 {
        let base = position_in_probe_cells.floor();
        let fraction = position_in_probe_cells - base;
        base * (1.0 - fraction) + (base + 1.0) * fraction
    }

    fn canonical_terrain_voxel_center_in_probe_cells(
        position_in_probe_cells: f64,
        terrain_voxels_per_probe: f64,
    ) -> f64 {
        ((position_in_probe_cells * terrain_voxels_per_probe).floor() + 0.5)
            / terrain_voxels_per_probe
    }

    #[test]
    fn continuous_terrain_position_basis_does_not_quantize_a_linear_probe_field() {
        let epsilon = 1.0e-6;
        let left = 1.0 - epsilon;
        let right = 1.0 + epsilon;
        let exact_delta = sample_linear_probe_field(right) - sample_linear_probe_field(left);

        assert!((exact_delta - 2.0 * epsilon).abs() < 1.0e-12);

        let terrain_voxels_per_probe = 32.0;
        let canonical_left =
            canonical_terrain_voxel_center_in_probe_cells(left, terrain_voxels_per_probe);
        let canonical_right =
            canonical_terrain_voxel_center_in_probe_cells(right, terrain_voxels_per_probe);
        let quantized_delta =
            sample_linear_probe_field(canonical_right) - sample_linear_probe_field(canonical_left);

        assert!((quantized_delta - 1.0 / terrain_voxels_per_probe).abs() < 1.0e-12);
        assert!(quantized_delta > exact_delta * 10_000.0);
    }

    #[test]
    fn cache_revision_is_stable_for_an_identical_radiance_snapshot() {
        let mut cache = EnvironmentLightingCache::default();
        let first = cache.update(snapshot(), std::time::Duration::ZERO);
        let unchanged = cache.update(snapshot(), std::time::Duration::from_millis(16));

        assert_eq!(first.live.revision, 1);
        assert_eq!(first.transport.revision, 1);
        assert_eq!(first.transport.source_live_revision, 1);
        assert!(first.transport_published);
        assert_eq!(unchanged.live.revision, first.live.revision);
        assert_eq!(unchanged.transport, first.transport);
        assert!(!unchanged.transport_published);
        assert_eq!(unchanged.revision_lag(), 0);
        assert_eq!(
            unchanged.transport_age(std::time::Duration::from_millis(16)),
            std::time::Duration::from_millis(16)
        );
    }

    #[test]
    fn live_and_transport_observability_names_the_exact_published_snapshot() {
        let mut cache = EnvironmentLightingCache::default();
        let first = cache.update(snapshot(), std::time::Duration::from_millis(10));
        let mut changed = snapshot();
        changed.sun_direction = Vec3::Z;
        let second = cache.update(changed, std::time::Duration::from_millis(30));

        assert_eq!(second.live.revision, 2);
        assert_eq!(
            second.live.observed_at,
            std::time::Duration::from_millis(30)
        );
        assert_eq!(second.live.snapshot.sun_direction, Vec3::Z);
        assert_eq!(second.transport.revision, first.transport.revision + 1);
        assert_eq!(second.transport.source_live_revision, second.live.revision);
        assert_eq!(
            second.transport.published_at,
            std::time::Duration::from_millis(30)
        );
        assert_eq!(second.transport.snapshot, second.live.snapshot);
        assert_eq!(second.revision_lag(), 0);
        assert!(second.transport_published);
    }

    #[test]
    fn cache_revision_covers_every_transport_radiance_input() {
        let mut variants = Vec::new();
        let mut value = snapshot();
        value.sun_direction = Vec3::Z;
        variants.push(value);
        value = snapshot();
        value.sun_color.x += 0.1;
        variants.push(value);
        value = snapshot();
        value.sun_luminance += 0.1;
        variants.push(value);
        value = snapshot();
        value.terrain_ray_origin_offset_world += 0.001;
        variants.push(value);
        value = snapshot();
        value.ddgi_receiver_visibility_bias_world += 0.001;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.dirt_color.x += 0.1;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.sand_color.y += 0.1;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.cherry_wood_color.z += 0.1;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.oak_wood_color.x += 0.1;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.rock_color.y += 0.1;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.hash_color_variance += 0.1;
        variants.push(value);

        for changed in variants {
            let mut cache = EnvironmentLightingCache::default();
            let first = cache.update(snapshot(), std::time::Duration::ZERO);
            let changed = cache.update(changed, DDGI_TRANSPORT_MIN_PUBLICATION_INTERVAL);
            assert_eq!(changed.transport.revision, first.transport.revision + 1);
        }
    }

    #[test]
    fn cache_revision_covers_the_compiled_authored_sky_model() {
        assert_ne!(DDGI_AUTHORED_SKY_MODEL_IDENTITY, 0);
        assert_eq!(
            snapshot().identity().authored_sky_model_identity,
            DDGI_AUTHORED_SKY_MODEL_IDENTITY,
        );

        let mut cache = EnvironmentLightingCache::default();
        let mut value = snapshot();
        let first = cache.update_for_authored_sky(
            &mut value,
            DDGI_AUTHORED_SKY_MODEL_IDENTITY,
            std::time::Duration::ZERO,
        );
        let changed = cache.update_for_authored_sky(
            &mut value,
            DDGI_AUTHORED_SKY_MODEL_IDENTITY.wrapping_add(1),
            std::time::Duration::from_millis(1),
        );

        assert_eq!(changed.transport.revision, first.transport.revision + 1);
        assert_eq!(changed.transport.snapshot, first.transport.snapshot);
    }

    #[test]
    fn cache_identity_uses_the_normalized_sun_direction() {
        let mut cache = EnvironmentLightingCache::default();
        let first = cache.update(snapshot(), std::time::Duration::ZERO);
        let mut scaled = snapshot();
        scaled.sun_direction *= 10.0;
        let unchanged = cache.update(scaled, std::time::Duration::from_millis(1));

        assert_eq!(unchanged.transport.revision, first.transport.revision);
        assert_eq!(unchanged.transport.snapshot.sun_direction, Vec3::Y);
    }

    #[test]
    fn continuous_sun_changes_coalesce_into_rate_limited_latest_transport() {
        let mut cache = EnvironmentLightingCache::default();
        let initial = cache.update(snapshot(), std::time::Duration::ZERO);

        for (step, elapsed_ms) in [(1.0_f32, 50_u64), (2.0, 100), (3.0, 150)] {
            let mut changed = snapshot();
            changed.sun_direction = glam::Quat::from_rotation_x(step.to_radians()) * Vec3::Y;
            let pending = cache.update(changed, std::time::Duration::from_millis(elapsed_ms));
            assert!(!pending.transport_published);
            assert_eq!(pending.transport, initial.transport);
            assert_eq!(pending.revision_lag(), step as u64);
        }

        let mut latest = snapshot();
        latest.sun_direction = glam::Quat::from_rotation_x(4.0_f32.to_radians()) * Vec3::Y;
        let published = cache.update(latest, DDGI_TRANSPORT_MIN_PUBLICATION_INTERVAL);

        assert!(published.transport_published);
        assert_eq!(published.transport.revision, 2);
        assert_eq!(published.transport.source_live_revision, 5);
        assert_eq!(published.transport.snapshot, published.live.snapshot);
        assert_eq!(
            published.transport.change.reason,
            DdgiRadianceChangeReason::ContinuousSun
        );
        assert!(published.transport.change.delta.sun_angle_radians > 0.06);
        assert_eq!(published.coalesced_live_revisions, 3);
        assert_eq!(published.revision_lag(), 0);
    }

    #[test]
    fn large_sun_step_publishes_immediately_with_reset_policy() {
        let mut cache = EnvironmentLightingCache::default();
        let initial = cache.update(snapshot(), std::time::Duration::ZERO);
        let mut changed = snapshot();
        changed.sun_direction = Vec3::Z;
        let stepped = cache.update(changed, std::time::Duration::from_millis(1));

        assert!(stepped.transport_published);
        assert_eq!(stepped.transport.revision, initial.transport.revision + 1);
        assert_eq!(
            stepped.transport.change.reason,
            DdgiRadianceChangeReason::LargeSunStep
        );
        assert!(stepped.transport.change.resets_irradiance_history());
    }

    #[test]
    fn pause_drains_one_pending_snapshot_and_resume_does_not_replay_old_revisions() {
        let mut cache = EnvironmentLightingCache::default();
        cache.update(snapshot(), std::time::Duration::ZERO);
        let mut changed = snapshot();
        changed.sun_direction = glam::Quat::from_rotation_x(1.0_f32.to_radians()) * Vec3::Y;
        let pending = cache.update(changed, std::time::Duration::from_millis(50));
        assert!(!pending.transport_published);

        let paused = cache.update(changed, std::time::Duration::from_millis(250));
        assert!(paused.transport_published);
        assert_eq!(paused.transport.source_live_revision, pending.live.revision);
        let still_paused = cache.update(changed, std::time::Duration::from_secs(1));
        assert!(!still_paused.transport_published);
        assert_eq!(still_paused.transport, paused.transport);

        let mut resumed = changed;
        resumed.sun_direction = glam::Quat::from_rotation_x(2.0_f32.to_radians()) * Vec3::Y;
        let resumed = cache.update(resumed, std::time::Duration::from_millis(1_010));
        assert!(resumed.transport_published);
        assert_eq!(resumed.transport.revision, paused.transport.revision + 1);
        assert_eq!(
            resumed.transport.source_live_revision,
            resumed.live.revision
        );
    }

    #[test]
    fn non_solar_transport_change_is_an_immediate_discontinuity() {
        let mut cache = EnvironmentLightingCache::default();
        cache.update(snapshot(), std::time::Duration::ZERO);
        let mut changed = snapshot();
        changed.voxel_palette.rock_color.x += 0.01;
        let changed = cache.update(changed, std::time::Duration::from_millis(1));

        assert!(changed.transport_published);
        assert_eq!(
            changed.transport.change.reason,
            DdgiRadianceChangeReason::TransportInputStep
        );
        assert!(changed.transport.change.delta.non_solar_changed);
        assert!(changed.transport.change.resets_irradiance_history());
    }

    #[test]
    fn continuous_history_retention_uses_source_time_and_change_magnitude() {
        let mut cache = EnvironmentLightingCache::default();
        let source = cache
            .update(snapshot(), std::time::Duration::ZERO)
            .transport;
        let mut changed = snapshot();
        changed.sun_direction = glam::Quat::from_rotation_x(1.0_f32.to_radians()) * Vec3::Y;
        let destination = cache
            .update(changed, DDGI_TRANSPORT_MIN_PUBLICATION_INTERVAL)
            .transport;
        let policy = DdgiRadianceHistoryPolicy::between(source, destination);

        assert_eq!(policy.elapsed, DDGI_TRANSPORT_MIN_PUBLICATION_INTERVAL);
        assert_eq!(
            policy.change.reason,
            DdgiRadianceChangeReason::ContinuousSun
        );
        assert!(!policy.resets_history());
        let retention = policy.retention(0.99);
        assert!(retention > 0.0 && retention < 0.99);

        let mut larger = destination;
        larger.published_at += DDGI_TRANSPORT_MIN_PUBLICATION_INTERVAL;
        larger.snapshot.sun_direction = glam::Quat::from_rotation_x(4.0_f32.to_radians()) * Vec3::Y;
        let larger_policy = DdgiRadianceHistoryPolicy::between(destination, larger);
        assert!(larger_policy.retention(0.99) < retention);
    }

    #[test]
    fn discontinuous_history_policy_is_an_explicit_zero_weight_reset() {
        let mut cache = EnvironmentLightingCache::default();
        let source = cache
            .update(snapshot(), std::time::Duration::ZERO)
            .transport;
        let mut changed = snapshot();
        changed.sun_direction = Vec3::Z;
        let destination = cache
            .update(changed, std::time::Duration::from_millis(1))
            .transport;
        let policy = DdgiRadianceHistoryPolicy::between(source, destination);

        assert!(policy.resets_history());
        assert_eq!(policy.retention(0.99), 0.0);
    }

    #[test]
    fn terrain_and_raster_consumers_share_the_ddgi_sampler_contract() {
        let shared = include_str!("../shader/slang/environment_lighting.slang");
        let terrain = include_str!("../shader/slang/tracer.slang");
        let raster = include_str!("../shader/slang/flora_shadow.slang");
        let pipeline_builder = include_str!("tracer/pipeline_builder.rs");
        let tracer_host = include_str!("tracer/mod.rs");

        assert!(shared.contains("import ddgi_query;"));
        assert!(shared.contains("return sampleDdgiDiffuseEnvironment("));
        assert!(!shared.contains("SH"));
        assert!(!shared.contains("environment_probe_coefficients"));
        assert!(!shared.contains("environment_lighting_backend"));
        assert!(terrain.contains("consumerResult = sampleDdgiTerrainSmoothEnvironment("));
        assert!(terrain.contains("environmentIrradiance = consumerResult.irradiance"));
        assert!(terrain.contains("environmentCaptureIrradiance = consumerResult.irradiance"));
        assert!(terrain.contains("color = environmentIrradiance * albedo"));
        assert!(raster.contains("sampleDiffuseEnvironment("));
        assert!(raster.contains("shading, voxelCenter, shadingNormal"));
        for consumer in [
            include_str!("../shader/slang/flora.vert.slang"),
            include_str!("../shader/slang/flora_lod.vert.slang"),
            include_str!("../shader/slang/leaves.vert.slang"),
            include_str!("../shader/slang/leaves_lod.vert.slang"),
        ] {
            assert!(consumer.contains("import flora_vertex;"));
        }
        for consumer in [
            include_str!("../shader/slang/dynamic_fruit.vert.slang"),
            include_str!("../shader/slang/sprinkler.vert.slang"),
            include_str!("../shader/slang/particle_lod_textured.vert.slang"),
        ] {
            assert!(consumer.contains("import flora_shadow;"));
            assert!(consumer.contains("applyStylizedVoxelLighting("));
        }
        assert!(pipeline_builder.contains("environment_lighting_resources"));
        assert!(!pipeline_builder.contains("environment_probes"));

        let consumer_update = tracer_host
            .split_once("fn stage_ddgi_consumer_descriptors")
            .expect("DDGI consumer promotion seam must exist")
            .1
            .split_once("fn publish_ddgi_consumer_descriptors")
            .expect("staged consumer descriptors must have an explicit publication seam")
            .0;
        assert!(consumer_update.contains("compute_pipelines"));
        assert!(consumer_update.contains("tracer_ppl"));
        assert!(consumer_update.contains("graphics_pipelines"));
        assert!(consumer_update.contains("flora_ppl"));
        assert!(consumer_update.contains("ddgi_probe_metadata"));
        assert!(consumer_update.contains("ddgi_irradiance_atlas"));
        assert!(consumer_update.contains("ddgi_visibility_atlas"));

        let promotion = tracer_host
            .split_once("fn promote_ready_ddgi_staging")
            .expect("DDGI promotion must exist")
            .1;
        let descriptor_rebind = promotion
            .find("publish_ddgi_consumer_descriptors")
            .expect("promotion must rebind every shared consumer");
        let ownership_swap = promotion
            .find("promote_staging(build_token)")
            .expect("promotion must swap the same builder token");
        assert!(descriptor_rebind < ownership_swap);
        assert!(promotion.contains("[DDGI][CONSUMERS]"));
        assert!(promotion.contains("consumer_set=terrain_compute,flora_raster"));
        assert!(tracer_host.contains("[DDGI][FLORA_CONSUMER] draw_recorded"));
        assert!(tracer_host.contains("recorded_flora_instance_count > 0"));
    }

    #[test]
    fn path_tracing_reference_is_terrain_only_and_bypasses_ddgi() {
        let shared = include_str!("../shader/slang/environment_lighting.slang");
        let terrain = include_str!("../shader/slang/tracer.slang");
        let raster = include_str!("../shader/slang/flora_shadow.slang");
        let path_tracing_branch = terrain
            .split_once("if (gui_input.path_tracing_reference != 0u")
            .expect("terrain shader must expose the path-tracing GUI switch")
            .1
            .split_once("// The moment-visibility receiver remains fixed")
            .expect("path-tracing branch must remain ahead of the DDGI query")
            .0;

        assert!(!shared.contains("path_tracing_reference"));
        assert!(!terrain.contains("raster_flora_ddgi_lighting"));
        assert!(shared.contains("return sampleDdgiDiffuseEnvironment("));
        assert!(path_tracing_branch.contains("pathTraceTerrainReference("));
        assert!(path_tracing_branch.contains("return;"));
        assert!(!path_tracing_branch.contains("sampleDdgiDiffuseEnvironment"));
        assert!(raster.contains("applyStylizedVoxelLighting(U_GuiInput gui"));
        assert!(raster.contains("sampleDiffuseEnvironment(\n        gui, shading"));
        for consumer in [
            include_str!("../shader/slang/flora_vertex.slang"),
            include_str!("../shader/slang/dynamic_fruit.vert.slang"),
            include_str!("../shader/slang/sprinkler.vert.slang"),
            include_str!("../shader/slang/particle_lod_textured.vert.slang"),
        ] {
            assert!(consumer.contains("gui_input, sun_info, shading_info"));
        }
    }

    #[test]
    fn raster_flora_lighting_switch_preserves_legacy_and_ddgi_paths() {
        let config: toml::Value = toml::from_str(include_str!("../config/gui.toml"))
            .expect("GUI config must be valid TOML");
        let switch = gui_param(&config, "raster_flora_ddgi_lighting");
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let lighting = include_str!("../shader/slang/flora_shadow.slang");
        let shared = include_str!("../shader/slang/flora_vertex.slang");
        let flora_cache = include_str!("../shader/slang/flora_lighting_cache.comp.slang");
        let flora = include_str!("../shader/slang/flora.vert.slang");
        let flora_lod = include_str!("../shader/slang/flora_lod.vert.slang");
        let tree_leaf_cache = include_str!("../shader/slang/tree_leaf_lighting_cache.comp.slang");
        let leaves = include_str!("../shader/slang/leaves.vert.slang");
        let leaves_lod = include_str!("../shader/slang/leaves_lod.vert.slang");

        assert_eq!(switch["kind"].as_str(), Some("bool"));
        assert_eq!(switch["data"]["value"].as_bool(), Some(true));
        assert!(lighting.contains("float3(24.0 / 255.0)"));
        assert!(lighting.contains("sunLight * shadowWeight + LEGACY_RASTER_FLORA_AMBIENT_LIGHT"));
        assert!(shared.contains("applyLegacyRasterFloraLighting("));
        let runtime_query = query
            .split_once("DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
            .expect("DDGI must expose the runtime consumer query")
            .1
            .split_once("DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(")
            .expect("runtime consumer query must remain isolated from terrain smoothing")
            .0;
        assert!(runtime_query.contains("getDdgiMomentProbeContribution("));
        assert!(runtime_query.contains("contribution.moment_visibility"));
        assert!(!runtime_query.contains("getDdgiMomentExactProbeContribution("));
        let flora_environment = shared
            .split_once("public float3 sampleFloraEnvironment(")
            .expect("raster flora must have a shared environment query")
            .1
            .split_once("public float3 shadeFloraVertexWithEnvironment(")
            .expect("flora environment query must remain isolated from shading")
            .0;
        assert!(flora_environment.contains("sampleDiffuseEnvironment("));
        assert_eq!(flora_cache.matches("sampleFloraEnvironment(").count(), 1);
        for shader in [flora, flora_lod] {
            let lighting_branch = shader
                .split_once("if (rasterFloraUsesDdgiLighting())")
                .expect("raster flora shader must branch before cache access")
                .1;
            assert!(lighting_branch.contains("flora_lighting_cache.irradiance["));
            assert!(lighting_branch.contains("shadeLegacyRasterFloraVertex("));
        }
        assert_eq!(
            tree_leaf_cache.matches("sampleFloraEnvironment(").count(),
            1
        );
        assert!(
            tree_leaf_cache.contains("floraLightingCacheIndex(floraPc, localInstanceIndex, 0u)")
        );
        assert!(!tree_leaf_cache.contains("vertexOffset"));
        for shader in [leaves, leaves_lod] {
            let lighting_branch = shader
                .split_once("if (rasterFloraUsesDdgiLighting())")
                .expect("tree-leaf shader must branch before cache access")
                .1;
            assert!(lighting_branch.contains("flora_lighting_cache.irradiance["));
            assert!(lighting_branch.contains("shadeTreeLeafVertexWithEnvironment("));
            assert!(lighting_branch.contains("shadeLegacyTreeLeafVertex("));
            assert!(!shader.contains("sampleFloraEnvironment("));
        }
        let tree_leaf_finish = shared
            .split_once("float3 finishTreeLeafShading(")
            .expect("tree-leaf view-dependent finishing helper must exist")
            .1;
        assert!(tree_leaf_finish.contains("backlightVisibility"));
        assert!(tree_leaf_finish.contains("applyTerrainEditPreviewTint("));
    }

    #[test]
    fn path_tracing_controls_and_transport_are_validated_semantically() {
        let terrain = include_str!("../shader/slang/tracer.slang");
        let skylight = include_str!("../shader/slang/skylight.slang");
        let ddgi_trace = include_str!("../shader/slang/ddgi_probe_trace.slang");
        let ddgi_sky = include_str!("../shader/slang/ddgi_global_sky_filter.slang");
        let config: toml::Value = toml::from_str(include_str!("../config/gui.toml"))
            .expect("GUI config must be valid TOML");
        let reference = gui_param(&config, "path_tracing_reference");
        let ambient = gui_param(&config, "path_tracing_ambient_light");
        let max_bounces = gui_param(&config, "path_tracing_max_bounces");
        let ray_origin_offset = gui_param(&config, "terrain_ray_origin_offset_world");
        let receiver_visibility_bias = gui_param(&config, "ddgi_receiver_visibility_bias_world");

        assert_eq!(reference["kind"].as_str(), Some("bool"));
        assert_eq!(ambient["kind"].as_str(), Some("color"));
        assert_eq!(max_bounces["kind"].as_str(), Some("uint"));
        assert_eq!(ray_origin_offset["kind"].as_str(), Some("float"));
        assert_eq!(ray_origin_offset["data"]["min"].as_integer(), Some(0));
        assert_eq!(ray_origin_offset["data"]["max"].as_float(), Some(0.02));
        assert_eq!(receiver_visibility_bias["kind"].as_str(), Some("float"));
        assert_eq!(
            receiver_visibility_bias["data"]["min"].as_integer(),
            Some(0)
        );
        assert_eq!(
            receiver_visibility_bias["data"]["max"].as_float(),
            Some(0.02)
        );
        assert_eq!(
            receiver_visibility_bias["data"]["value"].as_float(),
            Some(1.0 / 256.0),
            "the default visibility receiver bias must remain one terrain voxel"
        );
        for dependent in [ambient, max_bounces] {
            assert_eq!(
                dependent["enabled_if"]["param"].as_str(),
                Some("path_tracing_reference")
            );
            assert_eq!(dependent["enabled_if"]["equals"].as_bool(), Some(true));
        }

        let transport = terrain
            .split_once("float3 pathTracingDirectIrradiance(")
            .expect("path tracer must evaluate direct sun independently")
            .1
            .split_once("float depthFromWorldPosition(")
            .expect("path-tracing transport must remain a bounded terrain helper")
            .0;
        assert!(terrain.contains("import skylight;"));
        assert!(transport.contains("getAuthoredSkyRadiance("));
        assert!(transport.contains("sampleDiffuseBounce("));
        assert!(transport.contains("sampleSunDisk("));
        assert!(transport.contains("generalSceneMarching(shadowRay"));
        assert!(transport.contains("generalSceneMarching(indirectRay"));
        assert!(transport.contains("gui_input.path_tracing_max_bounces"));
        assert!(!transport.contains("sampleDdgi"));
        assert!(!transport.contains("directSunShadowTransmittance"));
        assert!(!transport.contains("shadow_map"));
        assert!(!transport.contains("leaf_shadow"));
        assert!(!transport.contains("cloud_shadow"));

        assert!(skylight.contains("public float3 getAuthoredSkyRadiance("));
        assert!(ddgi_trace.contains("getAuthoredSkyRadiance("));
        assert!(ddgi_sky.contains("getAuthoredSkyRadiance("));
    }

    #[test]
    fn ddgi_visibility_policy_uses_adjustable_world_bias_and_rejects_distant_hits() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let filter = include_str!("../shader/slang/ddgi_visibility_filter.slang");

        assert!(query.contains("max(0.0, query.visibility_bias_world)"));
        assert!(!query.contains("visibility_bias_world * 0.125"));
        assert!(!query.contains("0.25 / max(gridScale"));
        assert!(filter.contains("hitDistance > supportDistance"));
        assert!(filter.contains("signedDistance >= pc.far_distance_world * 0.999"));
        assert!(filter.contains("if (!skyMiss) continue;"));
        assert!(filter.contains("hitDistance = supportDistance;"));
    }

    #[test]
    fn terrain_invalidation_fails_closed_before_the_global_sky_fallback() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let sampler = query
            .split_once("DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
            .expect("shared DDGI sampler must exist")
            .1;
        let invalidation = sampler
            .find("ddgiQueryIsTerrainInvalidated")
            .expect("shared sampler must reject invalidated terrain");
        let global_sky = sampler
            .find("ddgiQueryUsesGlobalSky")
            .expect("shared sampler must retain the outside-volume sky fallback");

        assert!(invalidation < global_sky);
        assert!(sampler[invalidation..global_sky].contains("return result;"));
    }

    #[test]
    fn consumer_and_transport_adapters_share_probe_core_but_keep_distinct_visibility() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let consumer_implementation = query
            .split_once("DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
            .expect("runtime consumer implementation must exist")
            .1
            .split_once("DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(")
            .expect("terrain smoothing must follow the runtime consumer")
            .0;
        let consumer = query
            .split_once("public DdgiQueryResult sampleDdgiDiffuseEnvironment(")
            .expect("consumer adapter must exist")
            .1
            .split_once("DdgiQueryResult sampleDdgiTransportEnvironmentFromAtlas(")
            .expect("transport implementation must follow the consumer adapter")
            .0;
        let transport = query
            .split_once("DdgiQueryResult sampleDdgiTransportEnvironmentFromAtlas(")
            .expect("transport implementation must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("transport adapter must follow its implementation")
            .0;

        assert_eq!(
            consumer_implementation
                .matches("for (uint z = 0u; z < 2u; ++z)")
                .count(),
            1
        );
        assert_eq!(
            transport.matches("for (uint z = 0u; z < 2u; ++z)").count(),
            1
        );
        assert!(consumer.contains("sampleDdgiDiffuseEnvironmentFromAtlas("));
        assert!(consumer.contains("ddgi_irradiance_atlas"));
        assert!(transport.contains("getDdgiMomentExactProbeContributionFromAtlases("));

        let trace = include_str!("../shader/slang/ddgi_probe_trace.slang");
        assert!(!trace.contains("ConstantBuffer<U_SunInfo>"));
        assert!(!trace.contains("ConstantBuffer<U_ShadingInfo>"));
        assert!(trace.contains("[[vk::binding(29, 0)]]"));
        assert!(trace.contains("[[vk::binding(30, 0)]]"));
        assert!(trace.contains("[[vk::binding(31, 0)]]"));
        assert!(trace.contains("[[vk::binding(32, 0)]]"));
    }

    #[test]
    fn transport_multiplies_exact_and_moment_while_runtime_consumers_use_moment_only() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let runtime = query
            .split_once("DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
            .expect("runtime DDGI sampler must exist")
            .1
            .split_once("DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(")
            .expect("terrain smoothing must follow runtime sampler")
            .0;
        let transport = query
            .split_once("DdgiQueryResult sampleDdgiTransportEnvironmentFromAtlas(")
            .expect("transport DDGI sampler must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("transport adapter must follow transport sampler")
            .0;
        assert!(query.contains("import ddgi_voxel_visibility;"));
        assert!(query.contains("ddgiVoxelSegmentVisibility("));
        assert!(query.contains("worldPosition + normal * biasWorld"));
        assert!(query.contains("float3 hardVisibilityWorldPosition"));
        assert!(!query.contains("surfaceOutward"));
        let probe_trace = include_str!("../shader/slang/ddgi_probe_trace.slang");
        assert!(!probe_trace.contains("surfaceOutward"));
        assert!(probe_trace.contains(
            "terrainVoxelSurfacePositionAlongNormal(\n        result.center_position, normal)"
        ));
        assert!(probe_trace.contains(
            "terrainRayOriginAlongNormal(\n        result.center_position, normal,\n        ddgi_radiance_sun.terrain_ray_origin_offset_world)"
        ));
        assert!(probe_trace.contains("ddgiHardVisibilityPosition"));
        assert!(!probe_trace.contains("ddgi_transport_query_info, result.position"));
        assert!(runtime.contains("result, contribution, contribution.moment_visibility"));
        assert!(!runtime.contains("contribution.hard_visibility"));
        assert!(transport.contains("contribution.moment_visibility *"));
        assert!(transport.contains("contribution.hard_visibility"));

        let tracer = include_str!("../shader/slang/tracer.slang");
        assert!(!tracer.contains("result.position, result.normal, -ray.direction"));
        assert!(tracer.contains(
            "terrainVoxelSurfacePositionAlongNormal(\n        result.center_position, result.normal)"
        ));
        assert!(tracer.contains("terrainDdgiHardVisibilityOrigin("));
        assert!(tracer.contains("gui_input.terrain_ray_origin_offset_world"));
        assert!(tracer.contains(
            "surfacePosition +\n            normalDirection * gui_input.terrain_ray_origin_offset_world"
        ));
        assert!(tracer.contains(
            "sampleDdgiTerrainSmoothEnvironment(\n        shading_info, ddgiReceiverPosition, result.position,\n        result.normal)"
        ));
        assert!(!tracer.contains("register(t39"));
        assert!(!tracer.contains("register(t40"));
        assert!(!tracer.contains("register(t42"));
        let terrain_smooth = query
            .split_once("DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(")
            .expect("terrain smooth Moment query must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTerrainSmoothEnvironment(")
            .expect("terrain smooth adapter must follow its implementation")
            .0;
        assert!(terrain_smooth.contains("getDdgiMomentSpatialWeightProbeContributionAt("));
        assert!(terrain_smooth.contains("DDGI_SPATIAL_WEIGHT_NOMINAL_HARD"));
        assert!(!terrain_smooth.contains("hardVisibilityWorldPosition"));
        assert!(query.contains("getDdgiMomentSpatialWeightProbeContributionAt("));
        assert!(query.contains("surfaceSideWeight = sqrt(max(0.0, surfaceAlignment));"));
        assert!(query.contains("float3 biasedWorldPosition = worldPosition + normal * biasWorld;"));
        assert!(query.contains(
            "ddgiVoxelSegmentVisibility(\n        hardVisibilityWorldPosition, contribution.actual_position"
        ));
        assert!(
            query.contains("float3 surfaceToProbe = actualPosition - positionWeightWorldPosition;")
        );
        let exact_reference = query
            .split_once("public DdgiQueryResult sampleDdgiExactTerrainReference(")
            .expect("exact voxel reference must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiUnoccludedTerrainReference(")
            .expect("exact reference must remain isolated")
            .0;
        assert!(exact_reference.contains("contribution.hard_visibility"));
    }

    #[test]
    fn irradiance_filter_forbids_relative_rgb_history_resets() {
        let filter = include_str!("../shader/slang/ddgi_irradiance_filter.slang");
        let history_block = filter
            .split_once("if (pc.has_history != 0u)")
            .expect("irradiance history block must exist")
            .1
            .split_once("storeIrradiance(atlasCoordinate, current);")
            .expect("irradiance history block must precede the atlas store")
            .0;

        assert!(history_block.contains("if (localRecoveryProbe)"));
        assert!(history_block.contains("historyRetention = recoveryEpoch / (recoveryEpoch + 1.0);"));
        assert!(!history_block.contains("historyRetention = 0.0;"));
        assert!(!history_block.contains("relativeChange"));
        assert!(!history_block.contains("relativeDarkening"));
        assert!(!history_block.contains("DDGI_IRRADIANCE_CHANGE_THRESHOLD"));
        assert!(!history_block.contains("DDGI_IRRADIANCE_MIN_DARKENING_STEP"));
    }

    #[test]
    fn runtime_consumers_are_moment_only_while_transport_and_reference_keep_exact_visibility() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let types = include_str!("../shader/slang/tracer_types.slang");

        assert!(types.contains("public uint ddgi_terrain_hard_origin;"));
        assert!(query.contains("getDdgiMomentSpatialWeightProbeContributionAt("));
        assert!(query.contains("getDdgiMomentExactSpatialWeightProbeContributionAt("));
        let transport = query
            .split_once("DdgiQueryResult sampleDdgiTransportEnvironmentFromAtlas(")
            .expect("transport must retain an explicit moment-plus-exact query")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("transport implementation must remain behind its adapter")
            .0;
        assert!(transport.contains("getDdgiMomentExactProbeContributionFromAtlases("));
        assert!(transport.contains("contribution.hard_visibility"));
    }

    #[test]
    fn unoccluded_irradiance_debug_isolated_from_final_visibility_path() {
        let tracer = include_str!("../shader/slang/tracer.slang");
        let query = include_str!("../shader/slang/ddgi_query.slang");

        assert!(tracer.contains("DDGI_DEBUG_UNOCCLUDED_IRRADIANCE = 12u"));
        assert!(tracer.contains("DDGI_DEBUG_EQUAL_WEIGHT_IRRADIANCE = 13u"));
        assert!(tracer.contains("DDGI_DEBUG_RAW_CAGE_IRRADIANCE = 14u"));
        assert!(tracer.contains("sampleDdgiUnoccludedTerrainReference("));
        assert!(tracer.contains("sampleDdgiEqualWeightTerrainReference("));
        assert!(tracer.contains("sampleDdgiRawCageIrradiance("));
        assert!(query.contains("getDdgiUnoccludedProbeContribution("));
        assert!(query.contains("accumulateDdgiEqualWeightContribution("));
        assert!(query.contains("sampleDdgiRawCageIrradiance("));
        assert!(query.contains("accumulateDdgiContribution(result, contribution, 1.0);"));
        assert!(!query.contains("public DdgiProbeContribution getDdgiProbeContribution("));
        assert!(!tracer.contains("accumulateDdgiContribution("));
        assert!(query.contains("public void writeDdgiSpatialWeightDiagnostics("));
        assert!(tracer.contains("writeDdgiSpatialWeightDiagnostics("));
        assert!(tracer.contains("if (view == DDGI_DEBUG_EXACT_IRRADIANCE)"));
    }

    #[test]
    fn direct_terrain_shadow_uses_exact_surface_hit_and_keeps_ddgi_voxel_receiver() {
        let tracer = include_str!("../shader/slang/tracer.slang");
        let ray_origin = include_str!("../shader/slang/terrain_ray_origin.slang");

        assert!(ray_origin.contains("public float3 terrainRayOriginFromSurface("));
        assert!(ray_origin.contains(
            "return surfacePosition +\n        normalDirection * max(0.0, offsetWorld);"
        ));
        assert!(tracer.contains(
            "shadowRay.origin = terrainShadowReceiverPositionFromSurface(\n        surfacePosition, normal);"
        ));
        assert!(
            tracer.contains("directLight = directLighting(albedo, result.normal, result.position,")
        );
        assert!(tracer.contains(
            "terrainVoxelSurfacePositionAlongNormal(\n        result.center_position, result.normal)"
        ));
        assert!(tracer.contains(
            "sampleDdgiTerrainSmoothEnvironment(\n        shading_info, ddgiReceiverPosition, result.position,\n        result.normal)"
        ));
    }

    #[test]
    fn terrain_ray_origin_offset_is_shared_by_every_exact_terrain_ray_stage() {
        let shared = include_str!("../shader/slang/terrain_ray_origin.slang");
        let tracer = include_str!("../shader/slang/tracer.slang");
        let exact_sun = include_str!("../shader/slang/ddgi_exact_sun_visibility.slang");
        let probe_trace = include_str!("../shader/slang/ddgi_probe_trace.slang");
        let moisture = include_str!("../shader/slang/terrain_moisture_dry.slang");

        assert!(shared.contains("public float3 terrainRayOriginAlongNormal("));
        assert!(shared.contains("public float3 terrainRayOriginFromSurface("));
        assert!(tracer.contains("import terrain_ray_origin;"));
        assert!(tracer.contains("gui_input.terrain_ray_origin_offset_world"));
        assert!(exact_sun.contains("originOffsetWorld"));
        assert!(exact_sun.contains("terrainRayOriginAlongNormal("));
        assert!(probe_trace.contains("import terrain_ray_origin;"));
        assert!(probe_trace.contains("ddgi_radiance_sun.terrain_ray_origin_offset_world"));
        assert!(probe_trace.contains("ddgiHardVisibilityPosition"));
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let transport = query
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("transport query adapter must exist")
            .1
            .split_once("public uint ddgiNearestNominalProbeIndex")
            .expect("transport query adapter must remain isolated")
            .0;
        assert!(transport.contains("float3 hardVisibilityWorldPosition"));
        assert!(transport.contains("hardVisibilityWorldPosition);"));
        assert!(!transport.contains("visibility_bias_world *"));
        assert!(moisture.contains("import terrain_ray_origin;"));
        assert!(moisture.contains("gui_input.terrain_ray_origin_offset_world"));
    }
}
