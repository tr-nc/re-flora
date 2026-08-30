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
    pub emissive_color: Vec3,
    pub emissive_radiance: f32,
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
            emissive_color: self
                .voxel_palette
                .emissive_color
                .to_array()
                .map(f32::to_bits),
            emissive_radiance: self.voxel_palette.emissive_radiance.to_bits(),
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
    emissive_color: [u32; 3],
    emissive_radiance: u32,
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
            && self.emissive_color == other.emissive_color
            && self.emissive_radiance == other.emissive_radiance
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
        let previous_direction = previous.sun_direction;
        let next_direction = next.sun_direction;
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
    fn between_authored(
        previous: DdgiRadianceSnapshot,
        previous_identity: DdgiRadianceIdentity,
        next: DdgiRadianceSnapshot,
        next_identity: DdgiRadianceIdentity,
    ) -> Self {
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
            change: destination.change_from(source),
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct AuthoredEnvironmentLightingInput {
    pub sun_direction: Vec3,
    pub sun_color: Vec3,
    pub sun_luminance: f32,
    pub terrain_ray_origin_offset_world: f32,
    pub ddgi_receiver_visibility_bias_world: f32,
    pub voxel_palette: DdgiVoxelPaletteSnapshot,
    pub local_lights: LocalLightGpuPayload,
}

impl AuthoredEnvironmentLightingInput {
    fn normalize(self) -> DdgiRadianceSnapshot {
        DdgiRadianceSnapshot {
            sun_direction: self.sun_direction.normalize_or_zero(),
            sun_color: self.sun_color,
            sun_luminance: self.sun_luminance,
            terrain_ray_origin_offset_world: self.terrain_ray_origin_offset_world,
            ddgi_receiver_visibility_bias_world: self.ddgi_receiver_visibility_bias_world,
            voxel_palette: self.voxel_palette,
            local_lights: self.local_lights,
        }
    }
}

/// The normalized current-frame lighting fact consumed by immediate lighting and observed by
/// downstream transports. Its revision identifies live authored values, never DDGI cadence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AuthoredEnvironmentLightingFact {
    pub revision: u64,
    pub observed_at: Duration,
    snapshot: DdgiRadianceSnapshot,
    identity: DdgiRadianceIdentity,
}

impl AuthoredEnvironmentLightingFact {
    fn new(
        revision: u64,
        observed_at: Duration,
        snapshot: DdgiRadianceSnapshot,
        identity: DdgiRadianceIdentity,
    ) -> Self {
        Self {
            revision,
            observed_at,
            snapshot,
            identity,
        }
    }

    pub(crate) fn snapshot(self) -> DdgiRadianceSnapshot {
        self.snapshot
    }

    pub(crate) fn assert_same_identity(self, previous: Self) {
        assert_eq!(
            self.identity, previous.identity,
            "Authored Environment Lighting reused live revision {} for a different identity",
            self.revision
        );
    }

    pub(crate) fn change_from_transport(
        self,
        previous: EnvironmentLightingState,
    ) -> Option<DdgiRadianceChange> {
        (self.identity != previous.authored_identity).then(|| {
            DdgiRadianceChange::between_authored(
                previous.snapshot,
                previous.authored_identity,
                self.snapshot,
                self.identity,
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        revision: u64,
        observed_at: Duration,
        snapshot: DdgiRadianceSnapshot,
    ) -> Self {
        Self::new(revision, observed_at, snapshot, snapshot.identity())
    }
}

/// One immutable, scheduler-facing DDGI transport snapshot.
///
/// Direct lighting never reads this type: it continues to consume the current-frame sun uniform.
/// `source_live_revision` makes intentional transport lag observable without weakening the exact
/// revision-to-snapshot identity used by in-flight DDGI work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EnvironmentLightingState {
    revision: u32,
    source_live_revision: u64,
    published_at: Duration,
    snapshot: DdgiRadianceSnapshot,
    change: DdgiRadianceChange,
    authored_identity: DdgiRadianceIdentity,
}

impl EnvironmentLightingState {
    pub(crate) fn freeze(
        revision: u32,
        authored: AuthoredEnvironmentLightingFact,
        change: DdgiRadianceChange,
    ) -> Self {
        let mut snapshot = authored.snapshot();
        snapshot.local_lights = snapshot.local_lights.with_transport_revision(revision);
        Self {
            revision,
            source_live_revision: authored.revision,
            published_at: authored.observed_at,
            snapshot,
            change,
            authored_identity: authored.identity,
        }
    }

    pub(crate) fn snapshot(self) -> DdgiRadianceSnapshot {
        self.snapshot
    }

    pub(crate) fn revision(self) -> u32 {
        self.revision
    }

    pub(crate) fn source_live_revision(self) -> u64 {
        self.source_live_revision
    }

    pub(crate) fn published_at(self) -> Duration {
        self.published_at
    }

    pub(crate) fn change(self) -> DdgiRadianceChange {
        self.change
    }

    fn change_from(self, previous: Self) -> DdgiRadianceChange {
        DdgiRadianceChange::between_authored(
            previous.snapshot,
            previous.authored_identity,
            self.snapshot,
            self.authored_identity,
        )
    }

    #[cfg(test)]
    fn for_test(revision: u32, published_at: Duration, snapshot: DdgiRadianceSnapshot) -> Self {
        let authored =
            AuthoredEnvironmentLightingFact::for_test(u64::from(revision), published_at, snapshot);
        Self::freeze(revision, authored, DdgiRadianceChange::default())
    }
}

#[derive(Debug, Default)]
pub(crate) struct AuthoredEnvironmentLighting {
    current_live_revision: u64,
    last_live_identity: Option<DdgiRadianceIdentity>,
    last_observed_at: Option<Duration>,
}

impl AuthoredEnvironmentLighting {
    pub fn observe(
        &mut self,
        input: AuthoredEnvironmentLightingInput,
        observed_at: Duration,
    ) -> AuthoredEnvironmentLightingFact {
        self.observe_for_authored_sky(input, DDGI_AUTHORED_SKY_MODEL_IDENTITY, observed_at)
    }

    fn observe_for_authored_sky(
        &mut self,
        input: AuthoredEnvironmentLightingInput,
        authored_sky_model_identity: u64,
        observed_at: Duration,
    ) -> AuthoredEnvironmentLightingFact {
        assert!(
            self.last_observed_at
                .is_none_or(|previous| observed_at >= previous),
            "Environment Lighting observations must use a monotonic clock"
        );
        self.last_observed_at = Some(observed_at);
        let snapshot = input.normalize();
        let identity = snapshot.identity_for_authored_sky(authored_sky_model_identity);
        let live_changed = self.last_live_identity != Some(identity);
        if live_changed {
            self.current_live_revision = self.current_live_revision.wrapping_add(1).max(1);
            self.last_live_identity = Some(identity);
        }
        AuthoredEnvironmentLightingFact::new(
            self.current_live_revision,
            observed_at,
            snapshot,
            identity,
        )
    }

    #[cfg(test)]
    pub(crate) fn observe_for_test_authored_sky(
        &mut self,
        input: AuthoredEnvironmentLightingInput,
        authored_sky_model_identity: u64,
        observed_at: Duration,
    ) -> AuthoredEnvironmentLightingFact {
        self.observe_for_authored_sky(input, authored_sky_model_identity, observed_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                emissive_color: Vec3::new(1.0, 0.36, 0.08),
                emissive_radiance: 4.0,
            },
            local_lights: LocalLightGpuPayload::empty(0),
        }
    }

    fn input(value: DdgiRadianceSnapshot) -> AuthoredEnvironmentLightingInput {
        AuthoredEnvironmentLightingInput {
            sun_direction: value.sun_direction,
            sun_color: value.sun_color,
            sun_luminance: value.sun_luminance,
            terrain_ray_origin_offset_world: value.terrain_ray_origin_offset_world,
            ddgi_receiver_visibility_bias_world: value.ddgi_receiver_visibility_bias_world,
            voxel_palette: value.voxel_palette,
            local_lights: value.local_lights,
        }
    }

    fn transport(
        revision: u32,
        published_at: Duration,
        snapshot: DdgiRadianceSnapshot,
    ) -> EnvironmentLightingState {
        EnvironmentLightingState::for_test(revision, published_at, snapshot)
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
    fn identical_authored_fact_keeps_live_revision_stable() {
        let mut authored = AuthoredEnvironmentLighting::default();
        let first = authored.observe(input(snapshot()), Duration::ZERO);
        let unchanged = authored.observe(input(snapshot()), Duration::from_millis(16));

        assert_eq!(first.revision, 1);
        assert_eq!(unchanged.revision, first.revision);
        assert_eq!(unchanged.snapshot, first.snapshot);
        assert_eq!(unchanged.observed_at, Duration::from_millis(16));
    }

    #[test]
    fn every_observation_returns_the_latest_normalized_fact() {
        let mut authored = AuthoredEnvironmentLighting::default();
        let first = authored.observe(input(snapshot()), Duration::from_millis(10));
        let mut changed = snapshot();
        changed.sun_direction = Vec3::Z;
        let second = authored.observe(input(changed), Duration::from_millis(30));

        assert_eq!(first.revision, 1);
        assert_eq!(second.revision, 2);
        assert_eq!(second.observed_at, Duration::from_millis(30));
        assert_eq!(second.snapshot.sun_direction, Vec3::Z);
    }

    #[test]
    fn live_revision_covers_every_authored_lighting_input() {
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
        value = snapshot();
        value.voxel_palette.emissive_color.x += 0.1;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.emissive_radiance += 0.1;
        variants.push(value);

        for changed in variants {
            let mut authored = AuthoredEnvironmentLighting::default();
            let first = authored.observe(input(snapshot()), Duration::ZERO);
            let changed = authored.observe(input(changed), Duration::from_millis(1));
            assert_eq!(changed.revision, first.revision + 1);
        }
    }

    #[test]
    fn live_revision_covers_the_compiled_authored_sky_model() {
        assert_ne!(DDGI_AUTHORED_SKY_MODEL_IDENTITY, 0);
        assert_eq!(
            snapshot().identity().authored_sky_model_identity,
            DDGI_AUTHORED_SKY_MODEL_IDENTITY,
        );

        let mut authored = AuthoredEnvironmentLighting::default();
        let first = authored.observe_for_authored_sky(
            input(snapshot()),
            DDGI_AUTHORED_SKY_MODEL_IDENTITY,
            Duration::ZERO,
        );
        let changed = authored.observe_for_authored_sky(
            input(snapshot()),
            DDGI_AUTHORED_SKY_MODEL_IDENTITY.wrapping_add(1),
            Duration::from_millis(1),
        );

        assert_eq!(changed.revision, first.revision + 1);
        assert_eq!(changed.snapshot, first.snapshot);
    }

    #[test]
    fn non_unit_sun_direction_is_normalized_once_for_live_identity() {
        let mut authored = AuthoredEnvironmentLighting::default();
        let first = authored.observe(input(snapshot()), Duration::ZERO);
        let mut scaled = snapshot();
        scaled.sun_direction *= 10.0;
        let unchanged = authored.observe(input(scaled), Duration::from_millis(1));

        assert_eq!(unchanged.revision, first.revision);
        assert_eq!(unchanged.snapshot.sun_direction, Vec3::Y);
    }

    #[test]
    fn continuous_history_retention_uses_source_time_and_change_magnitude() {
        let source = transport(1, Duration::ZERO, snapshot());
        let mut changed = snapshot();
        changed.sun_direction = glam::Quat::from_rotation_x(1.0_f32.to_radians()) * Vec3::Y;
        let destination = transport(2, Duration::from_millis(200), changed);
        let policy = DdgiRadianceHistoryPolicy::between(source, destination);

        assert_eq!(policy.elapsed, Duration::from_millis(200));
        assert_eq!(
            policy.change.reason,
            DdgiRadianceChangeReason::ContinuousSun
        );
        assert!(!policy.resets_history());
        let retention = policy.retention(0.99);
        assert!(retention > 0.0 && retention < 0.99);

        let mut larger_snapshot = destination.snapshot();
        larger_snapshot.sun_direction = glam::Quat::from_rotation_x(4.0_f32.to_radians()) * Vec3::Y;
        let larger = transport(
            3,
            destination.published_at + Duration::from_millis(200),
            larger_snapshot,
        );
        let larger_policy = DdgiRadianceHistoryPolicy::between(destination, larger);
        assert!(larger_policy.retention(0.99) < retention);
    }

    #[test]
    fn discontinuous_history_policy_is_an_explicit_zero_weight_reset() {
        let source = transport(1, Duration::ZERO, snapshot());
        let mut changed = snapshot();
        changed.sun_direction = Vec3::Z;
        let destination = transport(2, Duration::from_millis(1), changed);
        let policy = DdgiRadianceHistoryPolicy::between(source, destination);

        assert!(policy.resets_history());
        assert_eq!(policy.retention(0.99), 0.0);
    }

    #[test]
    fn frozen_transport_cannot_hide_a_material_change_from_history_classification() {
        let source = transport(1, Duration::ZERO, snapshot());
        let mut destination_snapshot = source.snapshot();
        destination_snapshot.voxel_palette.rock_color.x += 0.1;
        let destination = transport(2, Duration::from_millis(1), destination_snapshot);

        let policy = DdgiRadianceHistoryPolicy::between(source, destination);

        assert_eq!(
            policy.change.reason,
            DdgiRadianceChangeReason::TransportInputStep
        );
        assert!(policy.resets_history());
        assert_eq!(policy.retention(0.99), 0.0);
    }

    #[test]
    fn terrain_and_raster_consumers_share_the_ddgi_sampler_contract() {
        let shared = include_str!("../shader/slang/environment_lighting.slang");
        let terrain = include_str!("../shader/slang/tracer.slang");
        let raster = include_str!("../shader/slang/flora_shadow.slang");

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
            include_str!("../shader/slang/dynamic_fruit.vert.slang"),
            include_str!("../shader/slang/sprinkler.vert.slang"),
            include_str!("../shader/slang/particle_lod_textured.vert.slang"),
        ] {
            assert!(consumer.contains("applyStylizedVoxelLighting("));
        }
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
    fn generic_ddgi_consumer_keeps_moment_visibility_until_raster_acceptance() {
        const CALL: &str = "ddgiQueryVisibilityWeight(";
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let route = query
            .split_once("DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
            .expect("generic production query route")
            .1
            .split_once("DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(")
            .expect("terrain-smooth route follows generic route")
            .0;
        assert_eq!(route.matches(CALL).count(), 1);
        let arguments = route
            .split_once(CALL)
            .expect("generic route must call the visibility owner")
            .1;
        let mut depth = 0usize;
        let mut argument_start = 0usize;
        let mut parsed = Vec::new();
        for (index, character) in arguments.char_indices() {
            match character {
                '(' => depth += 1,
                ')' if depth == 0 => {
                    parsed.push(arguments[argument_start..index].trim());
                    break;
                }
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    parsed.push(arguments[argument_start..index].trim());
                    argument_start = index + 1;
                }
                _ => {}
            }
        }
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[2], "DDGI_VISIBILITY_MOMENT");
    }

    #[test]
    fn ddgi_production_routes_resolve_invalidation_before_global_sky() {
        // Temporary route gate. Runtime acceptance in R13 replaces this source-level owner check.
        fn assert_domain_route(route: &str) {
            assert_eq!(route.matches("ddgiQueryDomain(").count(), 1);
            let owner = route.find("ddgiQueryDomain(").expect("domain owner call");
            let invalidated = route
                .find("if (domain == DDGI_QUERY_DOMAIN_INVALIDATED) return result;")
                .expect("invalidated early return");
            let sky = route
                .find("if (domain == DDGI_QUERY_DOMAIN_GLOBAL_SKY)")
                .expect("global-sky route");
            assert!(owner < invalidated && invalidated < sky);
        }

        let query = include_str!("../shader/slang/ddgi_query.slang");
        let info_owner = query
            .split_once("uint ddgiQueryDomain(DdgiQueryInfo query")
            .expect("DdgiQueryInfo domain owner")
            .1
            .split_once("uint ddgiQueryDomain(U_ShadingInfo lighting")
            .expect("U_ShadingInfo owner follows DdgiQueryInfo owner")
            .0;
        let shading_owner = query
            .split_once("uint ddgiQueryDomain(U_ShadingInfo lighting")
            .expect("U_ShadingInfo domain owner")
            .1
            .split_once("public DdgiQueryResult makeEmptyDdgiQueryResult")
            .expect("query result factory follows domain owners")
            .0;
        assert!(info_owner.contains(
            "return ddgiQueryDomain(\n        ddgiQueryIsTerrainInvalidated(query, worldPosition),\n        ddgiQueryUsesGlobalSky(query, worldPosition));"
        ));
        assert!(shading_owner.contains(
            "return ddgiQueryDomain(\n        ddgiQueryIsTerrainInvalidated(lighting, worldPosition),\n        ddgiQueryUsesGlobalSky(lighting, worldPosition));"
        ));

        let runtime = query
            .split_once("DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
            .expect("DdgiQueryInfo production route")
            .1
            .split_once("DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(")
            .expect("terrain-smooth route follows runtime")
            .0;
        let terrain_smooth = query
            .split_once("DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(")
            .expect("terrain-smooth production route")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTerrainSmoothEnvironment(")
            .expect("terrain-smooth adapter follows route")
            .0;
        let transport = query
            .split_once("DdgiQueryResult sampleDdgiTransportEnvironmentFromAtlas(")
            .expect("transport production route")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("transport adapter follows route")
            .0;
        let exact = query
            .split_once("public DdgiQueryResult sampleDdgiExactTerrainReference(")
            .expect("U_ShadingInfo production route")
            .1
            .split_once("public DdgiQueryResult sampleDdgiUnoccludedTerrainReference(")
            .expect("unoccluded route follows exact")
            .0;
        for route in [runtime, terrain_smooth, transport, exact] {
            assert_domain_route(route);
        }
    }

    #[test]
    fn exact_terrain_ray_stages_route_the_shared_origin_policy() {
        // Temporary route gate. Delete only after R13 runtime acceptance observes these callers.
        let tracer = include_str!("../shader/slang/tracer.slang");
        let hard_origin = tracer
            .split_once("float3 terrainDdgiHardVisibilityOrigin(")
            .expect("tracer hard-visibility origin route")
            .1
            .split_once("float3 terrainShadowReceiverPosition(")
            .expect("terrain shadow route follows hard origin")
            .0;
        assert_eq!(
            hard_origin.matches("terrainRayOriginFromPosition(").count(),
            3
        );
        assert!(hard_origin.contains(
            "terrainRayOriginFromPosition(\n            voxelCenter, normalDirection,\n            gui_input.terrain_ray_origin_offset_world)"
        ));
        assert!(hard_origin.contains(
            "terrainRayOriginFromPosition(\n            surfacePosition, normalDirection,\n            gui_input.terrain_ray_origin_offset_world)"
        ));
        assert!(hard_origin.contains(
            "terrainRayOriginFromPosition(\n        surfacePosition, normalDirection,\n        ddgiVisibilityBiasWorld(\n            shading_info.environment_probe_visibility_bias_world))"
        ));
        let exact_debug = tracer
            .split_once("DdgiQueryResult exactResult = sampleDdgiExactTerrainReference(")
            .expect("tracer exact-reference caller")
            .1
            .split_once("if (view == DDGI_DEBUG_EXACT_VISIBILITY)")
            .expect("exact-reference result consumer")
            .0;
        assert!(exact_debug.contains("hardVisibilityWorldPosition"));

        let readback = tracer
            .split_once("void writeDdgiSpatialWeightReadback(")
            .expect("spatial-weight readback route")
            .1
            .split_once("float3 ddgiTerrainDebugValue(")
            .expect("terrain debug route follows readback")
            .0;
        assert!(readback.contains(
            "float3 ddgiHardVisibilityOrigin = terrainDdgiHardVisibilityOrigin(\n        result.center_position, ddgiReceiverPosition, result.normal);"
        ));
        assert!(readback.contains(
            "writeDdgiSpatialWeightDiagnostics(\n        ddgi_spatial_weight_readback, receiverBase, shading_info,\n        ddgiReceiverPosition, result.normal, ddgiHardVisibilityOrigin);"
        ));

        let production = tracer
            .split_once("void getPixelColor(")
            .expect("production terrain route")
            .1
            .split_once("[shader(\"compute\")]")
            .expect("compute entry follows production terrain route")
            .0;
        assert!(production.contains(
            "float3 ddgiHardVisibilityOrigin = terrainDdgiHardVisibilityOrigin(\n            result.center_position, ddgiReceiverPosition, result.normal);"
        ));
        assert_eq!(production.matches("ddgiTerrainDebugValue(").count(), 2);
        assert!(production.contains(
            "environmentIrradiance = ddgiTerrainDebugValue(\n            screenUv, ddgiReceiverPosition, result.position, result.normal,\n            ddgiHardVisibilityOrigin, consumerResult);"
        ));
        assert!(production.contains(
            "environmentCaptureIrradiance = ddgiTerrainDebugValue(\n            screenUv, ddgiReceiverPosition, result.position, result.normal,\n            ddgiHardVisibilityOrigin, consumerResult);"
        ));

        let exact_sun = include_str!("../shader/slang/ddgi_exact_sun_visibility.slang");
        assert!(exact_sun.contains("terrainRayOriginAlongNormal("));
        assert!(exact_sun.contains("receiver.center_position, receiver.normal, originOffsetWorld"));

        let probe_trace = include_str!("../shader/slang/ddgi_probe_trace.slang");
        let transport_hit = probe_trace
            .split_once("float3 ddgiTransportHitRadiance(")
            .expect("probe transport-hit route")
            .1
            .split_once("[shader(\"compute\")]")
            .expect("probe compute entry follows transport-hit route")
            .0;
        assert!(transport_hit.contains(
            "float3 ddgiHardVisibilityPosition = terrainRayOriginAlongNormal(\n        result.center_position, normal,\n        ddgi_radiance_sun.terrain_ray_origin_offset_world);"
        ));
        assert!(transport_hit.contains(
            "sampleDdgiTransportSource(\n            ddgi_transport_query_info,\n            ddgi_transport_source_irradiance_atlas,\n            ddgi_transport_source_visibility_atlas,\n            ddgiReceiverPosition, normal, ddgiHardVisibilityPosition)"
        ));

        let moisture = include_str!("../shader/slang/terrain_moisture_dry.slang");
        let moisture_origin = moisture
            .split_once("float3 surfaceShadowSamplePosition(")
            .expect("moisture shadow-origin route")
            .1
            .split_once("float surfaceSunExposure(")
            .expect("sun-exposure route follows moisture origin")
            .0;
        assert!(moisture_origin.contains("terrainRayOriginAlongNormal("));
        assert!(moisture_origin.contains("gui_input.terrain_ray_origin_offset_world"));
    }

    #[test]
    fn ddgi_production_filters_consume_shared_policy_actions() {
        fn assert_history_usage(filter: &str, retained_copy: &str, history_blend: &str) {
            let retained = filter
                .split_once("if (historyPolicy.retain_source)")
                .expect("retained partition gate")
                .1
                .split_once("if (metadata.state_and_reserved.x")
                .expect("metadata validation follows retained partition")
                .0;
            assert!(retained.contains(retained_copy));
            assert_eq!(retained.matches("return;").count(), 1);

            let blended = filter
                .split_once("if (historyPolicy.blend_history)")
                .expect("history blend gate")
                .1
                .split_once("store")
                .expect("atlas store follows history blend")
                .0;
            assert!(blended.contains(history_blend));
        }

        let visibility = include_str!("../shader/slang/ddgi_visibility_filter.slang");
        let irradiance = include_str!("../shader/slang/ddgi_irradiance_filter.slang");
        assert_eq!(visibility.matches("ddgiFilterVisibilitySample(").count(), 1);
        assert_eq!(visibility.matches("ddgiFilterHistoryPolicy(").count(), 1);
        assert_eq!(irradiance.matches("ddgiFilterHistoryPolicy(").count(), 1);
        assert!(visibility.contains(
            "if (!sample.accepted) continue;\n        float hitDistance = sample.distance;"
        ));
        assert_history_usage(
            visibility,
            "storeVisibility(\n            atlasCoordinate, loadVisibility(pc.source_slot, atlasCoordinate));",
            "current = lerp(current,\n                       loadVisibility(pc.source_slot, atlasCoordinate),\n                       historyPolicy.retention);",
        );
        assert_history_usage(
            irradiance,
            "storeIrradiance(\n            atlasCoordinate, loadIrradiance(pc.source_slot, atlasCoordinate));",
            "current.xyz = lerp(current.xyz, history.xyz, historyPolicy.retention);",
        );
    }

    #[test]
    fn terrain_debug_modes_keep_their_production_query_wiring() {
        let tracer = include_str!("../shader/slang/tracer.slang");
        let debug_route = tracer
            .split_once("float3 ddgiTerrainDebugValue(")
            .expect("terrain debug owner")
            .1
            .split_once("void getPixelColor(")
            .expect("production pixel route follows debug owner")
            .0;
        let unoccluded = debug_route
            .split_once("if (view == DDGI_DEBUG_UNOCCLUDED_IRRADIANCE)")
            .expect("unoccluded mode")
            .1
            .split_once("if (view == DDGI_DEBUG_EQUAL_WEIGHT_IRRADIANCE)")
            .expect("equal-weight mode follows unoccluded")
            .0;
        let equal_weight = debug_route
            .split_once("if (view == DDGI_DEBUG_EQUAL_WEIGHT_IRRADIANCE)")
            .expect("equal-weight mode")
            .1
            .split_once("if (view == DDGI_DEBUG_RAW_CAGE_IRRADIANCE)")
            .expect("raw-cage mode follows equal-weight")
            .0;
        let raw_cage = debug_route
            .split_once("if (view == DDGI_DEBUG_RAW_CAGE_IRRADIANCE)")
            .expect("raw-cage mode")
            .1
            .split_once("if (view >= DDGI_DEBUG_SPATIAL_WEIGHT_CURRENT")
            .expect("spatial diagnostics follow raw cage")
            .0;
        assert_eq!(
            unoccluded
                .matches("sampleDdgiUnoccludedTerrainReference(")
                .count(),
            1
        );
        assert_eq!(
            equal_weight
                .matches("sampleDdgiEqualWeightTerrainReference(")
                .count(),
            1
        );
        assert_eq!(raw_cage.matches("sampleDdgiRawCageIrradiance(").count(), 1);
    }

    #[test]
    fn terrain_leaf_shadows_share_voxel_receiver_while_cloud_keeps_continuous_position() {
        let tracer = include_str!("../shader/slang/tracer.slang");
        let ray_origin = include_str!("../shader/slang/terrain_ray_origin.slang");
        let shadowing = include_str!("../shader/slang/tracer_shadowing.slang");

        assert!(ray_origin.contains("public float3 terrainRayOriginAlongNormal("));
        assert!(ray_origin.contains("public float3 terrainRayOriginFromPosition("));
        assert!(tracer.contains(
            "float3 terrainLeafReceiverPosition = terrainShadowReceiverPosition(\n        voxelCenter, normal);"
        ));
        assert!(tracer.contains(
            "float3 cloudReceiverPosition = terrainShadowReceiverPositionFromSurface(\n        surfacePosition, normal);"
        ));
        let receiver_factory = tracer
            .split_once("DirectSunShadowReceiver receiver = makeDirectSunShadowReceiver(")
            .expect("terrain direct-light path must construct a shadow receiver")
            .1
            .split_once("int3(0)")
            .expect("terrain direct-light receiver must retain its deterministic seed")
            .0;
        assert_eq!(
            receiver_factory
                .matches("terrainLeafReceiverPosition")
                .count(),
            2
        );
        assert_eq!(receiver_factory.matches("cloudReceiverPosition").count(), 1);
        assert!(tracer.contains(
            "directLight = directLighting(albedo, result.normal,\n                                     result.center_position, result.position,"
        ));
        for position in [
            "receiver.terrain_world_position",
            "receiver.leaf_world_position",
            "receiver.cloud_world_position",
        ] {
            assert!(shadowing.contains(position));
        }
        assert!(tracer.contains(
            "terrainVoxelSurfacePositionAlongNormal(\n        result.center_position, result.normal)"
        ));
        assert!(tracer.contains(
            "sampleDdgiTerrainSmoothEnvironment(\n        shading_info, ddgiReceiverPosition, result.position,\n        result.normal)"
        ));
    }
}
