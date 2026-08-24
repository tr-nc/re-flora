use super::App;
use crate::app::world_edits::TerrainBrushEdit;
use crate::builder::GrassGrowthInfluence;
use crate::lighting::{
    LightId, RasterEmitterComponent, RasterEmitterKey, RasterEmitterPartId, RasterEntityId,
    RASTER_ENTITY_LIGHT_PROVIDER_ID,
};
use crate::particles::{
    MotionMode, ParticleEmitter, ParticleRenderKind, ParticleSpawn, ParticleSystem,
    ParticleUpdateConfig, STANDARD_PARTICLE_SIZE,
};
use crate::tracer::{
    IrrigationPipeRenderData, IrrigationPipeRenderSegment, SprinklerRenderInstance,
    IRRIGATION_PIPE_END_CAP_VOXELS, IRRIGATION_PIPE_RADIUS_VOXELS,
};
use anyhow::{anyhow, Result};
use glam::{IVec3, Vec3, Vec4};
use rand::{rngs::SmallRng, RngExt, SeedableRng};
use std::collections::{HashSet, VecDeque};

const VOXELS_PER_WORLD_UNIT: f32 = 256.0;

// The rasterized prop mesh is four voxels tall: a three-voxel black stem and a
// one-voxel bright-orange cross-shaped head. Keep the emitter at its top surface.
const SPRINKLER_NOZZLE_HEIGHT_VOXELS: f32 = 4.0;

const SPRINKLER_SPAWN_RATE_PER_SECOND: f32 = 576.0;
const SPRINKLER_MAX_SPAWN_PER_FRAME: u32 = 192;
const SPRINKLER_DROPLET_SIZE: f32 = STANDARD_PARTICLE_SIZE;
const SPRINKLER_GRAVITY_FACTOR: f32 = 0.82;
const SPRINKLER_GRAVITY: f32 = 3.6 * SPRINKLER_GRAVITY_FACTOR;
const SPRINKLER_MIN_LANDING_RADIUS: f32 = 0.025;
const SPRINKLER_MAX_LANDING_RADIUS: f32 = 0.28;
const SPRINKLER_MIN_ELEVATION: f32 = 42.0_f32.to_radians();
const SPRINKLER_MAX_ELEVATION: f32 = 54.0_f32.to_radians();
const SPRINKLER_COLOR_LOW: Vec4 = Vec4::new(0.03, 0.20, 0.95, 0.42);
const SPRINKLER_COLOR_HIGH: Vec4 = Vec4::new(0.10, 0.48, 1.0, 0.68);
const SPRINKLER_PARTICLE_UPDATE: ParticleUpdateConfig = ParticleUpdateConfig::new(0.1, 2);
// Keep the hardware footprint clear without suppressing the much larger watered area.
// A smooth ten-voxel influence extends slightly beyond the five-voxel-wide sprinkler head.
const SPRINKLER_GRASS_SUPPRESSION_RADIUS_VOXELS: u32 = 10;
const SPRINKLER_GRASS_SUPPRESSION_MIN_LEVEL: u8 = 0;
const SPRINKLER_GRASS_INFLUENCE_ID_PREFIX: u64 = 0x5350_524B_0000_0000;
const SPRINKLER_RASTER_ENTITY_NAMESPACE: u32 = 0x5350_524B;
const SPRINKLER_HEAD_EMITTER_PART: RasterEmitterPartId = RasterEmitterPartId::new(1);
const PIPE_START_MAX_DISTANCE_VOXELS: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IrrigationNodeKind {
    Source,
    Junction,
}

#[derive(Clone, Copy, Debug)]
struct IrrigationNode {
    id: u32,
    position_voxels: IVec3,
    kind: IrrigationNodeKind,
}

#[derive(Clone, Copy, Debug)]
struct PipeSegment {
    start_node: u32,
    end_node: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PipeAttachment {
    pub position_voxels: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PipeRayHit {
    pub distance: f32,
    pub attachment: PipeAttachment,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum SprinklerPlacementTarget {
    Terrain(Vec3),
    Pipe(PipeAttachment),
}

impl SprinklerPlacementTarget {
    pub(super) fn position(self) -> Vec3 {
        match self {
            Self::Terrain(position) => position,
            Self::Pipe(attachment) => attachment.position_voxels / VOXELS_PER_WORLD_UNIT,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PipeDrag {
    start_voxels: IVec3,
    end_voxels: IVec3,
}

struct PipeRoutePreviewPlan {
    expected_revision: u64,
    next_drag: PipeDrag,
    render_data: IrrigationPipeRenderData,
}

impl PipeRoutePreviewPlan {
    fn render_data(&self) -> &IrrigationPipeRenderData {
        &self.render_data
    }
}

struct PipeRouteCommitPlan {
    expected_revision: u64,
    start_voxels: IVec3,
    next_network: IrrigationNetwork,
    render_data: IrrigationPipeRenderData,
}

impl PipeRouteCommitPlan {
    fn render_data(&self) -> &IrrigationPipeRenderData {
        &self.render_data
    }

    fn start_voxels(&self) -> IVec3 {
        self.start_voxels
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct IrrigationNetwork {
    source_node: Option<u32>,
    nodes: Vec<IrrigationNode>,
    segments: Vec<PipeSegment>,
    next_node_id: u32,
    powered_nodes: HashSet<u32>,
    active_drag: Option<PipeDrag>,
    revision: u64,
}

impl IrrigationNetwork {
    fn snap_surface_position(world_position: Vec3) -> IVec3 {
        (world_position * VOXELS_PER_WORLD_UNIT).round().as_ivec3() + IVec3::Y
    }

    fn node(&self, id: u32) -> Option<&IrrigationNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    fn node_at(&self, position_voxels: IVec3) -> Option<u32> {
        self.nodes
            .iter()
            .find(|node| node.position_voxels == position_voxels)
            .map(|node| node.id)
    }

    fn upsert_node(&mut self, position_voxels: IVec3, kind: IrrigationNodeKind) -> u32 {
        if let Some(id) = self.node_at(position_voxels) {
            if kind == IrrigationNodeKind::Source {
                if let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) {
                    node.kind = kind;
                }
            }
            return id;
        }
        let id = self.next_node_id.max(1);
        self.next_node_id = id.wrapping_add(1).max(1);
        self.nodes.push(IrrigationNode {
            id,
            position_voxels,
            kind,
        });
        id
    }

    fn connected_nodes(&self) -> HashSet<u32> {
        let Some(source) = self.source_node else {
            return HashSet::new();
        };
        let mut connected = HashSet::from([source]);
        let mut queue = VecDeque::from([source]);
        while let Some(node_id) = queue.pop_front() {
            for segment in &self.segments {
                let neighbor = if segment.start_node == node_id {
                    Some(segment.end_node)
                } else if segment.end_node == node_id {
                    Some(segment.start_node)
                } else {
                    None
                };
                if let Some(neighbor) = neighbor {
                    if connected.insert(neighbor) {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
        connected
    }

    fn refresh_connectivity(&mut self) {
        self.powered_nodes = self.connected_nodes();
    }

    #[cfg(test)]
    fn segment_is_connected(&self, segment: &PipeSegment) -> bool {
        self.powered_nodes.contains(&segment.start_node)
            && self.powered_nodes.contains(&segment.end_node)
    }

    fn nearest_node(&self, position: Vec3, max_distance: f32) -> Option<u32> {
        self.nodes
            .iter()
            .filter(|node| self.powered_nodes.contains(&node.id))
            .filter_map(|node| {
                let distance_sq = node.position_voxels.as_vec3().distance_squared(position);
                (distance_sq <= max_distance * max_distance).then_some((distance_sq, node.id))
            })
            .min_by(|left, right| left.0.total_cmp(&right.0))
            .map(|(_, id)| id)
    }

    fn drag_from(&self, world_position: Vec3) -> Option<PipeDrag> {
        let snapped = Self::snap_surface_position(world_position);
        if self.source_node.is_none() {
            return Some(PipeDrag {
                start_voxels: snapped,
                end_voxels: snapped,
            });
        }
        self.nearest_node(snapped.as_vec3(), PIPE_START_MAX_DISTANCE_VOXELS)
            .and_then(|id| self.node(id))
            .map(|node| PipeDrag {
                start_voxels: node.position_voxels,
                end_voxels: node.position_voxels,
            })
    }

    fn commit_drag_topology(&mut self, drag: PipeDrag, world_end: Vec3) -> Result<()> {
        let end = Self::snap_surface_position(world_end);
        let source_id = if let Some(source) = self.source_node {
            source
        } else {
            let source = self.upsert_node(drag.start_voxels, IrrigationNodeKind::Source);
            self.source_node = Some(source);
            source
        };
        self.refresh_connectivity();
        let start_node = self.node_at(drag.start_voxels).unwrap_or(source_id);
        if !self.powered_nodes.contains(&start_node) {
            return Err(anyhow!(
                "pipe drag must start on the powered irrigation network"
            ));
        }

        for (start, end) in orthogonal_pipe_route(drag.start_voxels, end) {
            let start_id = self.upsert_node(start, IrrigationNodeKind::Junction);
            let end_id = self.upsert_node(end, IrrigationNodeKind::Junction);
            if self.segments.iter().any(|segment| {
                (segment.start_node == start_id && segment.end_node == end_id)
                    || (segment.start_node == end_id && segment.end_node == start_id)
            }) {
                continue;
            }
            self.segments.push(PipeSegment {
                start_node: start_id,
                end_node: end_id,
            });
        }
        self.refresh_connectivity();
        Ok(())
    }

    pub(super) fn route_active(&self) -> bool {
        self.active_drag.is_some()
    }

    fn plan_begin_route(&self, world_position: Vec3) -> Option<PipeRoutePreviewPlan> {
        if self.active_drag.is_some() {
            return None;
        }
        let next_drag = self.drag_from(world_position)?;
        Some(PipeRoutePreviewPlan {
            expected_revision: self.revision,
            render_data: self.preview_render_data(next_drag),
            next_drag,
        })
    }

    fn plan_update_route(&self, world_position: Vec3) -> Option<PipeRoutePreviewPlan> {
        let mut next_drag = self.active_drag?;
        let end_voxels = Self::snap_surface_position(world_position);
        if next_drag.end_voxels == end_voxels {
            return None;
        }
        next_drag.end_voxels = end_voxels;
        Some(PipeRoutePreviewPlan {
            expected_revision: self.revision,
            render_data: self.preview_render_data(next_drag),
            next_drag,
        })
    }

    fn commit_route_preview(&mut self, plan: PipeRoutePreviewPlan) {
        assert_eq!(
            self.revision, plan.expected_revision,
            "pipe route preview must commit against its source revision"
        );
        self.active_drag = Some(plan.next_drag);
        self.revision = self.revision.wrapping_add(1);
    }

    fn plan_finish_route(&self, world_end: Vec3) -> Result<Option<PipeRouteCommitPlan>> {
        let Some(active_drag) = self.active_drag else {
            return Ok(None);
        };
        let mut next_network = self.clone();
        next_network.active_drag = None;
        next_network.commit_drag_topology(active_drag, world_end)?;
        next_network.revision = self.revision.wrapping_add(1);
        let render_data = next_network.render_data();
        Ok(Some(PipeRouteCommitPlan {
            expected_revision: self.revision,
            start_voxels: active_drag.start_voxels,
            next_network,
            render_data,
        }))
    }

    fn commit_route(&mut self, plan: PipeRouteCommitPlan) {
        assert_eq!(
            self.revision, plan.expected_revision,
            "pipe route must commit against its source revision"
        );
        debug_assert_eq!(
            plan.next_network.revision,
            self.revision.wrapping_add(1),
            "committed pipe route must advance the network revision"
        );
        *self = plan.next_network;
    }

    fn cancel_route(&mut self) -> bool {
        if self.active_drag.take().is_none() {
            return false;
        }
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub(super) fn ray_attachment(
        &self,
        ray_origin: Vec3,
        ray_direction: Vec3,
        max_distance: f32,
    ) -> Option<PipeRayHit> {
        if !ray_origin.is_finite()
            || !ray_direction.is_finite()
            || !max_distance.is_finite()
            || max_distance <= 0.0
        {
            return None;
        }
        let ray_direction = ray_direction.normalize_or_zero();
        if ray_direction == Vec3::ZERO {
            return None;
        }
        let radius = IRRIGATION_PIPE_RADIUS_VOXELS / VOXELS_PER_WORLD_UNIT;
        let end_cap = IRRIGATION_PIPE_END_CAP_VOXELS / VOXELS_PER_WORLD_UNIT;
        self.segments
            .iter()
            .filter_map(|segment| {
                let start_voxels = self.node(segment.start_node)?.position_voxels.as_vec3();
                let end_voxels = self.node(segment.end_node)?.position_voxels.as_vec3();
                let start = start_voxels / VOXELS_PER_WORLD_UNIT;
                let end = end_voxels / VOXELS_PER_WORLD_UNIT;
                let segment_direction = (end - start).normalize_or_zero();
                if segment_direction == Vec3::ZERO {
                    return None;
                }
                let distance = ray_capped_cylinder_intersection_distance(
                    ray_origin,
                    ray_direction,
                    start - segment_direction * end_cap,
                    end + segment_direction * end_cap,
                    radius,
                )?;
                if distance > max_distance {
                    return None;
                }
                let hit_voxels = (ray_origin + ray_direction * distance) * VOXELS_PER_WORLD_UNIT;
                let axis = end_voxels - start_voxels;
                let length_sq = axis.length_squared();
                let t = ((hit_voxels - start_voxels).dot(axis) / length_sq).clamp(0.0, 1.0);
                Some(PipeRayHit {
                    distance,
                    attachment: PipeAttachment {
                        position_voxels: start_voxels + axis * t,
                    },
                })
            })
            .min_by(|left, right| left.distance.total_cmp(&right.distance))
    }

    fn preview_render_data(&self, drag: PipeDrag) -> IrrigationPipeRenderData {
        IrrigationPipeRenderData {
            source_position: self
                .source_node
                .is_none()
                .then_some(drag.start_voxels.as_vec3() / VOXELS_PER_WORLD_UNIT),
            segments: orthogonal_pipe_route(drag.start_voxels, drag.end_voxels)
                .into_iter()
                .map(|(start, end)| IrrigationPipeRenderSegment {
                    start: start.as_vec3() / VOXELS_PER_WORLD_UNIT,
                    end: end.as_vec3() / VOXELS_PER_WORLD_UNIT,
                })
                .collect(),
        }
    }

    fn render_data(&self) -> IrrigationPipeRenderData {
        IrrigationPipeRenderData {
            source_position: self
                .source_node
                .and_then(|id| self.node(id))
                .map(|node| node.position_voxels.as_vec3() / VOXELS_PER_WORLD_UNIT),
            segments: self
                .segments
                .iter()
                .filter_map(|segment| {
                    let start = self.node(segment.start_node)?.position_voxels.as_vec3()
                        / VOXELS_PER_WORLD_UNIT;
                    let end = self.node(segment.end_node)?.position_voxels.as_vec3()
                        / VOXELS_PER_WORLD_UNIT;
                    Some(IrrigationPipeRenderSegment { start, end })
                })
                .collect(),
        }
    }
}

fn ray_capped_cylinder_intersection_distance(
    ray_origin: Vec3,
    ray_direction: Vec3,
    cylinder_start: Vec3,
    cylinder_end: Vec3,
    radius: f32,
) -> Option<f32> {
    let ray_direction = ray_direction.normalize_or_zero();
    let axis = cylinder_end - cylinder_start;
    let axis_length = axis.length();
    if ray_direction == Vec3::ZERO
        || !axis_length.is_finite()
        || axis_length <= f32::EPSILON
        || !radius.is_finite()
        || radius <= 0.0
    {
        return None;
    }

    let axis_direction = axis / axis_length;
    let origin_from_start = ray_origin - cylinder_start;
    let origin_axial = origin_from_start.dot(axis_direction);
    let direction_axial = ray_direction.dot(axis_direction);
    let origin_radial = origin_from_start - axis_direction * origin_axial;
    let direction_radial = ray_direction - axis_direction * direction_axial;
    let radius_sq = radius * radius;

    if (0.0..=axis_length).contains(&origin_axial) && origin_radial.length_squared() <= radius_sq {
        return Some(0.0);
    }

    let mut closest = f32::INFINITY;
    let quadratic_a = direction_radial.length_squared();
    if quadratic_a > f32::EPSILON {
        let quadratic_b = 2.0 * origin_radial.dot(direction_radial);
        let quadratic_c = origin_radial.length_squared() - radius_sq;
        let discriminant = quadratic_b * quadratic_b - 4.0 * quadratic_a * quadratic_c;
        if discriminant >= 0.0 {
            let sqrt_discriminant = discriminant.sqrt();
            for distance in [
                (-quadratic_b - sqrt_discriminant) / (2.0 * quadratic_a),
                (-quadratic_b + sqrt_discriminant) / (2.0 * quadratic_a),
            ] {
                let axial = origin_axial + direction_axial * distance;
                if distance >= 0.0 && (0.0..=axis_length).contains(&axial) {
                    closest = closest.min(distance);
                }
            }
        }
    }

    if direction_axial.abs() > f32::EPSILON {
        for cap_axial in [0.0, axis_length] {
            let distance = (cap_axial - origin_axial) / direction_axial;
            if distance < 0.0 {
                continue;
            }
            let cap_hit = origin_from_start + ray_direction * distance - axis_direction * cap_axial;
            if cap_hit.length_squared() <= radius_sq {
                closest = closest.min(distance);
            }
        }
    }

    closest.is_finite().then_some(closest)
}

fn orthogonal_pipe_route(start: IVec3, end: IVec3) -> Vec<(IVec3, IVec3)> {
    let corners = [
        start,
        IVec3::new(end.x, start.y, start.z),
        IVec3::new(end.x, start.y, end.z),
        end,
    ];
    corners
        .windows(2)
        .filter_map(|pair| (pair[0] != pair[1]).then_some((pair[0], pair[1])))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlaceableKind {
    Tree,
    Sprinkler,
    Pipe,
}

impl PlaceableKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Tree => "Tree",
            Self::Sprinkler => "Sprinkler",
            Self::Pipe => "Pipe",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SprinklerRecord {
    id: u32,
    base_position: Vec3,
    animation_phase: f32,
    emitter_component: Option<RasterEmitterComponent>,
}

impl SprinklerRecord {
    fn raster_entity_id(self) -> RasterEntityId {
        RasterEntityId::new(SPRINKLER_RASTER_ENTITY_NAMESPACE, self.id, 1)
    }

    fn emitter_source(self) -> Option<(RasterEmitterKey, RasterEmitterComponent)> {
        self.emitter_component.map(|component| {
            (
                RasterEmitterKey::new(self.raster_entity_id(), SPRINKLER_HEAD_EMITTER_PART),
                component,
            )
        })
    }
}

struct SprinklerEmitter {
    id: u32,
    nozzle_position: Vec3,
    rng: SmallRng,
    spawn_accumulator: f32,
    animation_phase: f32,
    animation_tick: u32,
    animation_tick_seconds: f32,
}

impl SprinklerEmitter {
    fn new(id: u32, nozzle_position: Vec3, animation_phase: f32) -> Self {
        let seed = sprinkler_seed(id, nozzle_position);
        Self {
            id,
            nozzle_position,
            rng: SmallRng::seed_from_u64(seed),
            spawn_accumulator: 0.0,
            animation_phase,
            animation_tick: 0,
            animation_tick_seconds: crate::game_time::WORLD_TICK_SECONDS_DEFAULT,
        }
    }

    fn set_animation_clock(&mut self, tick: u32, tick_seconds: f32) {
        self.animation_tick = tick;
        self.animation_tick_seconds = crate::game_time::clamp_world_tick_seconds(tick_seconds);
    }

    fn sprays_along_x(&self) -> bool {
        sprinkler_sprays_along_x(
            self.animation_tick,
            self.animation_tick_seconds,
            self.animation_phase,
        )
    }

    fn spawn_droplet(&mut self, system: &mut ParticleSystem) {
        let sign = if self.rng.random_bool(0.5) { 1.0 } else { -1.0 };
        let fan_angle = self
            .rng
            .random_range(-std::f32::consts::FRAC_PI_4..=std::f32::consts::FRAC_PI_4);
        let (sin_angle, cos_angle) = fan_angle.sin_cos();
        let horizontal_dir = if self.sprays_along_x() {
            Vec3::new(sign * cos_angle, 0.0, sin_angle)
        } else {
            Vec3::new(sin_angle, 0.0, sign * cos_angle)
        };

        // Uniformly sample the covered disk by area, then solve the ballistic launch speed
        // for that landing radius. This fills the spray footprint instead of concentrating
        // droplets at its rim, while every near and far droplet follows the same gravity.
        let area_sample = self.rng.random_range(0.0_f32..=1.0).sqrt();
        let landing_radius = SPRINKLER_MIN_LANDING_RADIUS
            + (SPRINKLER_MAX_LANDING_RADIUS - SPRINKLER_MIN_LANDING_RADIUS) * area_sample;
        let elevation = self
            .rng
            .random_range(SPRINKLER_MIN_ELEVATION..=SPRINKLER_MAX_ELEVATION);
        let (horizontal_speed, vertical_speed, flight_time) =
            sprinkler_ballistic_launch(landing_radius, elevation);
        let muzzle_jitter = Vec3::new(
            self.rng.random_range(-0.008..=0.008),
            self.rng.random_range(-0.002..=0.006),
            self.rng.random_range(-0.008..=0.008),
        );
        let color_mix = self.rng.random_range(0.0..=1.0);
        let color = SPRINKLER_COLOR_LOW.lerp(SPRINKLER_COLOR_HIGH, color_mix);
        let drift_direction = horizontal_dir;

        let spawn = ParticleSpawn {
            position: self.nozzle_position + horizontal_dir * 0.025 + muzzle_jitter,
            velocity: horizontal_dir * horizontal_speed + Vec3::Y * vertical_speed,
            color,
            size: SPRINKLER_DROPLET_SIZE,
            lifetime: flight_time + SPRINKLER_PARTICLE_UPDATE.interval_seconds,
            wind_factor: 0.0,
            gravity_factor: SPRINKLER_GRAVITY_FACTOR,
            drift_direction,
            drift_strength: self.rng.random_range(0.00..=0.008),
            drift_frequency: self.rng.random_range(1.5..=4.0),
            speed_noise_offset: self.rng.random_range(0.0..10_000.0) + self.id as f32,
            motion_mode: MotionMode::Free,
            sink_on_lifetime: false,
            sink_speed: 0.0,
            texture_variant: 0,
            render_kind: ParticleRenderKind::WaterDroplet,
            despawn_on_lifetime: true,
            despawn_below_ground: true,
            update: SPRINKLER_PARTICLE_UPDATE,
        };
        let _ = system.spawn(spawn);
    }
}

impl ParticleEmitter for SprinklerEmitter {
    fn update(&mut self, system: &mut ParticleSystem, dt: f32, _time: f32) {
        if dt <= 0.0 {
            return;
        }

        self.spawn_accumulator += SPRINKLER_SPAWN_RATE_PER_SECOND * dt.min(0.12);
        let mut spawned = 0;
        while self.spawn_accumulator >= 1.0 && spawned < SPRINKLER_MAX_SPAWN_PER_FRAME {
            self.spawn_droplet(system);
            self.spawn_accumulator -= 1.0;
            spawned += 1;
        }
        if spawned == SPRINKLER_MAX_SPAWN_PER_FRAME {
            self.spawn_accumulator = self.spawn_accumulator.min(1.0);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SprinklerMoistureSource {
    pub(super) base_position: Vec3,
    animation_phase: f32,
}

impl SprinklerMoistureSource {
    pub(super) fn spray_axis(self, tick: u32, tick_seconds: f32) -> Vec3 {
        if sprinkler_sprays_along_x(tick, tick_seconds, self.animation_phase) {
            Vec3::X
        } else {
            Vec3::Z
        }
    }
}

struct SprinklerPlacementPlan {
    expected_revision: u64,
    record: SprinklerRecord,
    emitter: SprinklerEmitter,
    render_instances: Vec<SprinklerRenderInstance>,
    grass_influence_id: u64,
    grass_influence: GrassGrowthInfluence,
}

impl SprinklerPlacementPlan {
    fn render_instances(&self) -> &[SprinklerRenderInstance] {
        &self.render_instances
    }

    fn grass_influence(&self) -> (u64, GrassGrowthInfluence) {
        (self.grass_influence_id, self.grass_influence)
    }

    fn id(&self) -> u32 {
        self.record.id
    }

    fn base_position(&self) -> Vec3 {
        self.record.base_position
    }

    fn raster_entity_id(&self) -> RasterEntityId {
        self.record.raster_entity_id()
    }

    fn emitter_source(&self) -> Option<(RasterEmitterKey, RasterEmitterComponent)> {
        self.record.emitter_source()
    }
}

struct SprinklerEmitterUpdatePlan {
    expected_revision: u64,
    entity: RasterEntityId,
    base_position: Vec3,
    component: RasterEmitterComponent,
    changed: bool,
    render_instances: Vec<SprinklerRenderInstance>,
    grass_influence: GrassGrowthInfluence,
}

impl SprinklerEmitterUpdatePlan {
    fn emitter_source(&self) -> (RasterEmitterKey, RasterEmitterComponent) {
        (
            RasterEmitterKey::new(self.entity, SPRINKLER_HEAD_EMITTER_PART),
            self.component,
        )
    }

    fn changed(&self) -> bool {
        self.changed
    }
}

struct SprinklerRemovalPlan {
    expected_revision: u64,
    removed_ids: HashSet<u32>,
    removed_grass_influence_ids: Vec<u64>,
    removed_emitter_entities: Vec<RasterEntityId>,
    render_instances: Vec<SprinklerRenderInstance>,
}

impl SprinklerRemovalPlan {
    fn removed_count(&self) -> usize {
        self.removed_ids.len()
    }

    fn render_instances(&self) -> &[SprinklerRenderInstance] {
        &self.render_instances
    }

    fn removed_grass_influence_ids(&self) -> &[u64] {
        &self.removed_grass_influence_ids
    }

    fn removed_emitter_entities(&self) -> &[RasterEntityId] {
        &self.removed_emitter_entities
    }
}

#[derive(Default)]
pub(super) struct SprinklerRuntime {
    records: Vec<SprinklerRecord>,
    emitters: Vec<SprinklerEmitter>,
    /// Monotonic, never-reused identity source. `u32::MAX + 1` means exhausted; placement then
    /// fails explicitly so every u32 consumer (removal, grass influence, RNG, raster key) shares
    /// the same no-ABA contract.
    next_id: u64,
    revision: u64,
}

impl SprinklerRuntime {
    pub(super) fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    pub(super) fn len(&self) -> usize {
        self.records.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub(super) fn render_instances(&self) -> Vec<SprinklerRenderInstance> {
        self.records
            .iter()
            .map(|record| SprinklerRenderInstance {
                base_position: record.base_position,
                animation_phase: record.animation_phase,
            })
            .collect()
    }

    pub(super) fn moisture_sources(&self) -> Vec<SprinklerMoistureSource> {
        self.records
            .iter()
            .map(|record| SprinklerMoistureSource {
                base_position: record.base_position,
                animation_phase: record.animation_phase,
            })
            .collect()
    }

    pub(super) fn advance_particles(
        &mut self,
        system: &mut ParticleSystem,
        dt: f32,
        time: f32,
        tick: u32,
        tick_seconds: f32,
    ) {
        for emitter in &mut self.emitters {
            emitter.set_animation_clock(tick, tick_seconds);
            emitter.update(system, dt, time);
        }
    }

    fn plan_placement(&self, target: SprinklerPlacementTarget) -> Result<SprinklerPlacementPlan> {
        self.plan_placement_with_emitter(target, None)
    }

    fn plan_placement_with_emitter(
        &self,
        target: SprinklerPlacementTarget,
        emitter_component: Option<RasterEmitterComponent>,
    ) -> Result<SprinklerPlacementPlan> {
        let base_position = target.position();
        let nozzle_position =
            base_position + Vec3::Y * (SPRINKLER_NOZZLE_HEIGHT_VOXELS / VOXELS_PER_WORLD_UNIT);
        let id = u32::try_from(self.next_id)
            .map_err(|_| anyhow!("sprinkler identity space exhausted; ids are never reused"))?;
        let animation_phase = sprinkler_animation_phase(id, base_position);
        let record = SprinklerRecord {
            id,
            base_position,
            animation_phase,
            emitter_component,
        };
        let mut render_instances = self.render_instances();
        render_instances.push(SprinklerRenderInstance {
            base_position,
            animation_phase,
        });
        Ok(SprinklerPlacementPlan {
            expected_revision: self.revision,
            record,
            emitter: SprinklerEmitter::new(id, nozzle_position, animation_phase),
            render_instances,
            grass_influence_id: sprinkler_grass_influence_id(id),
            grass_influence: sprinkler_grass_influence(base_position),
        })
    }

    fn commit_placement(&mut self, plan: SprinklerPlacementPlan) {
        assert_eq!(
            self.revision, plan.expected_revision,
            "sprinkler placement plan must commit against its source revision"
        );
        self.next_id = u64::from(plan.record.id) + 1;
        self.records.push(plan.record);
        self.emitters.push(plan.emitter);
        self.revision = self.revision.wrapping_add(1);
        debug_assert_eq!(self.records.len(), self.emitters.len());
    }

    fn plan_emitter_update(
        &self,
        entity: RasterEntityId,
        base_position: Vec3,
        component: RasterEmitterComponent,
    ) -> Option<SprinklerEmitterUpdatePlan> {
        let record = self
            .records
            .iter()
            .find(|record| record.raster_entity_id() == entity)?;
        let changed =
            record.base_position != base_position || record.emitter_component != Some(component);
        let mut render_instances = self.render_instances();
        let render = render_instances
            .iter_mut()
            .zip(self.records.iter())
            .find_map(|(render, candidate)| {
                (candidate.raster_entity_id() == entity).then_some(render)
            })
            .expect("live sprinkler record must have one raster instance");
        render.base_position = base_position;
        Some(SprinklerEmitterUpdatePlan {
            expected_revision: self.revision,
            entity,
            base_position,
            component,
            changed,
            render_instances,
            grass_influence: sprinkler_grass_influence(base_position),
        })
    }

    fn commit_emitter_update(&mut self, plan: SprinklerEmitterUpdatePlan) {
        assert_eq!(self.revision, plan.expected_revision);
        if !plan.changed {
            return;
        }
        let index = self
            .records
            .iter()
            .position(|record| record.raster_entity_id() == plan.entity)
            .expect("emitter update entity must remain live until commit");
        self.records[index].base_position = plan.base_position;
        self.records[index].emitter_component = Some(plan.component);
        self.emitters[index].nozzle_position =
            plan.base_position + Vec3::Y * (SPRINKLER_NOZZLE_HEIGHT_VOXELS / VOXELS_PER_WORLD_UNIT);
        self.revision = self.revision.wrapping_add(1);
    }

    fn emitter_source(
        &self,
        entity: RasterEntityId,
    ) -> Option<(RasterEmitterKey, RasterEmitterComponent)> {
        self.records
            .iter()
            .find(|record| record.raster_entity_id() == entity)
            .and_then(|record| record.emitter_source())
    }

    fn plan_removal(&self, edit: TerrainBrushEdit) -> Option<SprinklerRemovalPlan> {
        let removed_ids = self
            .records
            .iter()
            .filter(|record| {
                distance_sq_to_segment(record.base_position, edit.start, edit.end)
                    <= edit.radius * edit.radius
            })
            .map(|record| record.id)
            .collect::<HashSet<_>>();
        self.plan_removal_ids(removed_ids)
    }

    fn plan_removal_entity(&self, entity: RasterEntityId) -> Option<SprinklerRemovalPlan> {
        let removed_ids = self
            .records
            .iter()
            .filter(|record| record.raster_entity_id() == entity)
            .map(|record| record.id)
            .collect::<HashSet<_>>();
        self.plan_removal_ids(removed_ids)
    }

    fn plan_removal_ids(&self, removed_ids: HashSet<u32>) -> Option<SprinklerRemovalPlan> {
        if removed_ids.is_empty() {
            return None;
        }
        let render_instances = self
            .records
            .iter()
            .filter(|record| !removed_ids.contains(&record.id))
            .map(|record| SprinklerRenderInstance {
                base_position: record.base_position,
                animation_phase: record.animation_phase,
            })
            .collect();
        let removed_grass_influence_ids = self
            .records
            .iter()
            .filter(|record| removed_ids.contains(&record.id))
            .map(|record| sprinkler_grass_influence_id(record.id))
            .collect();
        let removed_emitter_entities = self
            .records
            .iter()
            .filter(|record| removed_ids.contains(&record.id) && record.emitter_component.is_some())
            .map(|record| record.raster_entity_id())
            .collect();
        Some(SprinklerRemovalPlan {
            expected_revision: self.revision,
            removed_ids,
            removed_grass_influence_ids,
            removed_emitter_entities,
            render_instances,
        })
    }

    fn commit_removal(&mut self, plan: SprinklerRemovalPlan) -> usize {
        assert_eq!(
            self.revision, plan.expected_revision,
            "sprinkler removal plan must commit against its source revision"
        );
        let removed_count = plan.removed_count();
        self.records
            .retain(|record| !plan.removed_ids.contains(&record.id));
        self.emitters
            .retain(|emitter| !plan.removed_ids.contains(&emitter.id));
        self.revision = self.revision.wrapping_add(1);
        debug_assert_eq!(self.records.len(), self.emitters.len());
        removed_count
    }
}

fn sprinkler_ballistic_launch(landing_radius: f32, elevation: f32) -> (f32, f32, f32) {
    let nozzle_height = SPRINKLER_NOZZLE_HEIGHT_VOXELS / VOXELS_PER_WORLD_UNIT;
    let radius = landing_radius.max(f32::EPSILON);
    let horizontal_speed = (0.5 * SPRINKLER_GRAVITY * radius * radius
        / (nozzle_height + radius * elevation.tan()))
    .sqrt();
    let vertical_speed = horizontal_speed * elevation.tan();
    let flight_time = radius / horizontal_speed;
    (horizontal_speed, vertical_speed, flight_time)
}

fn distance_sq_to_segment(point: Vec3, start: Vec3, end: Vec3) -> f32 {
    let segment = end - start;
    let length_sq = segment.length_squared();
    if length_sq <= f32::EPSILON {
        return point.distance_squared(start);
    }
    let t = ((point - start).dot(segment) / length_sq).clamp(0.0, 1.0);
    point.distance_squared(start + segment * t)
}

fn sprinkler_sprays_along_x(tick: u32, tick_seconds: f32, animation_phase: f32) -> bool {
    let pair_cycle_ticks = (1.0 / tick_seconds).round().max(2.0) as u32;
    let full_cycle_ticks = pair_cycle_ticks * 2;
    let phase_offset = (animation_phase.rem_euclid(1.0) * full_cycle_ticks as f32).round() as u32;
    (tick.wrapping_add(phase_offset) % full_cycle_ticks) >= pair_cycle_ticks
}

fn sprinkler_animation_phase(id: u32, position: Vec3) -> f32 {
    let seed = sprinkler_seed(id, position);
    ((seed >> 40) as u32 & 0x00FF_FFFF) as f32 / 16_777_216.0
}

fn sprinkler_grass_influence_id(id: u32) -> u64 {
    SPRINKLER_GRASS_INFLUENCE_ID_PREFIX | u64::from(id)
}

fn sprinkler_grass_influence(base_position: Vec3) -> GrassGrowthInfluence {
    GrassGrowthInfluence {
        center_world_vox: (base_position * VOXELS_PER_WORLD_UNIT).floor().as_uvec3(),
        radius_voxels: SPRINKLER_GRASS_SUPPRESSION_RADIUS_VOXELS,
        min_level: SPRINKLER_GRASS_SUPPRESSION_MIN_LEVEL,
    }
}

fn sprinkler_seed(id: u32, position: Vec3) -> u64 {
    let mut seed = 0xA24B_AED4_963E_E407u64 ^ id as u64;
    for bits in [
        position.x.to_bits(),
        position.y.to_bits(),
        position.z.to_bits(),
    ] {
        seed ^= bits as u64;
        seed = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        seed ^= seed >> 32;
    }
    seed
}

impl App {
    fn stage_raster_emitter_upsert(
        &self,
        entity: RasterEntityId,
        component: RasterEmitterComponent,
    ) -> Result<(
        crate::lighting::RasterEntityEmitterProvider,
        crate::lighting::LocalLightRegistry,
        crate::lighting::RasterEntityEmitterChange,
        LightId,
    )> {
        let mut provider = self.raster_entity_emitters.clone();
        let change = provider
            .publish_entity(entity, [(SPRINKLER_HEAD_EMITTER_PART, component)])
            .map_err(|err| anyhow!("raster emitter publication failed: {err:?}"))?;
        let mut registry = self.local_lights.clone();
        registry
            .reconcile(provider.snapshot())
            .map_err(|err| anyhow!("raster emitter reconcile failed: {err:?}"))?;
        let key = RasterEmitterKey::new(entity, SPRINKLER_HEAD_EMITTER_PART);
        let light_id = registry
            .light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, key.source_key())
            .ok_or_else(|| anyhow!("raster emitter reconcile omitted the published source"))?;
        Ok((provider, registry, change, light_id))
    }

    fn stage_raster_emitter_removals(
        &self,
        entities: &[RasterEntityId],
    ) -> Result<
        Option<(
            crate::lighting::RasterEntityEmitterProvider,
            crate::lighting::LocalLightRegistry,
        )>,
    > {
        if entities.is_empty() {
            return Ok(None);
        }
        let mut provider = self.raster_entity_emitters.clone();
        provider
            .remove_entities(entities.iter().copied())
            .map_err(|err| anyhow!("raster emitter removal failed: {err:?}"))?;
        let mut registry = self.local_lights.clone();
        registry
            .reconcile(provider.snapshot())
            .map_err(|err| anyhow!("raster emitter removal reconcile failed: {err:?}"))?;
        Ok(Some((provider, registry)))
    }

    pub(super) fn current_placeable_kind(&self) -> PlaceableKind {
        self.player_tools.selected_placeable()
    }

    pub(super) fn current_placeable_label(&self) -> &'static str {
        self.current_placeable_kind().label()
    }

    pub(super) fn begin_pipe_drag(&mut self, world_position: Vec3) {
        let Some(plan) = self.irrigation_network.plan_begin_route(world_position) else {
            log::info!("Pipe drag must start near the source or an existing junction");
            return;
        };
        if let Err(err) = self
            .tracer
            .upload_irrigation_pipe_preview(plan.render_data())
        {
            log::error!("Failed to show irrigation pipe preview: {err}");
            return;
        }
        self.irrigation_network.commit_route_preview(plan);
    }

    pub(super) fn update_pipe_drag_preview(&mut self, world_position: Vec3) -> Result<()> {
        let Some(plan) = self.irrigation_network.plan_update_route(world_position) else {
            return Ok(());
        };
        self.tracer
            .upload_irrigation_pipe_preview(plan.render_data())?;
        self.irrigation_network.commit_route_preview(plan);
        Ok(())
    }

    pub(super) fn finish_pipe_drag(&mut self, world_position: Vec3) -> Result<()> {
        let Some(plan) = self.irrigation_network.plan_finish_route(world_position)? else {
            return Ok(());
        };
        self.tracer.upload_irrigation_pipes(plan.render_data())?;
        let start_voxels = plan.start_voxels();
        self.irrigation_network.commit_route(plan);
        self.tracer.clear_irrigation_pipe_preview();
        log::info!("Committed irrigation pipe route from {:?}", start_voxels);
        Ok(())
    }

    pub(super) fn cancel_pipe_drag(&mut self) {
        if self.irrigation_network.cancel_route() {
            self.tracer.clear_irrigation_pipe_preview();
        }
    }

    pub(super) fn remove_sprinklers_in_brush(&mut self, edit: TerrainBrushEdit) -> Result<usize> {
        let Some(plan) = self.sprinklers.plan_removal(edit) else {
            return Ok(0);
        };

        let staged_lights = self.stage_raster_emitter_removals(plan.removed_emitter_entities())?;

        let previous_render_instances = self.sprinklers.render_instances();
        self.tracer.upload_sprinklers(plan.render_instances())?;
        if let Err(effect_error) = self
            .surface_builder
            .remove_external_grass_growth_influences(
                plan.removed_grass_influence_ids(),
                self.time_info.time_since_start_duration().as_millis() as u32,
            )
        {
            if let Err(rollback_error) = self.tracer.upload_sprinklers(&previous_render_instances) {
                return Err(anyhow!(
                    "failed to remove sprinkler grass influences: {effect_error}; \
                     failed to restore sprinkler render instances: {rollback_error}"
                ));
            }
            return Err(effect_error);
        }

        let removed_count = self.sprinklers.commit_removal(plan);
        if let Some((provider, registry)) = staged_lights {
            self.raster_entity_emitters = provider;
            self.local_lights = registry;
        }
        log::info!("Removed {} sprinkler(s) with terrain brush", removed_count);
        Ok(removed_count)
    }

    pub(super) fn apply_sprinkler_placement(
        &mut self,
        target: SprinklerPlacementTarget,
    ) -> Result<()> {
        self.apply_sprinkler_placement_internal(target, None)
            .map(|_| ())
    }

    fn apply_sprinkler_placement_internal(
        &mut self,
        target: SprinklerPlacementTarget,
        component: Option<RasterEmitterComponent>,
    ) -> Result<(RasterEntityId, Option<LightId>)> {
        let plan = self
            .sprinklers
            .plan_placement_with_emitter(target, component)?;
        let staged_lights = plan
            .emitter_source()
            .map(|(_, component)| {
                self.stage_raster_emitter_upsert(plan.raster_entity_id(), component)
            })
            .transpose()?;
        let previous_render_instances = self.sprinklers.render_instances();
        self.tracer.upload_sprinklers(plan.render_instances())?;
        let (influence_id, influence) = plan.grass_influence();
        if let Err(effect_error) = self.surface_builder.upsert_external_grass_growth_influence(
            influence_id,
            influence,
            self.time_info.time_since_start_duration().as_millis() as u32,
        ) {
            if let Err(rollback_error) = self.tracer.upload_sprinklers(&previous_render_instances) {
                return Err(anyhow!(
                    "failed to add sprinkler grass influence: {effect_error}; \
                     failed to restore sprinkler render instances: {rollback_error}"
                ));
            }
            return Err(effect_error);
        }

        let id = plan.id();
        let base_position = plan.base_position();
        let entity = plan.raster_entity_id();
        self.sprinklers.commit_placement(plan);
        let light_id = staged_lights.map(|(provider, registry, change, light_id)| {
            self.raster_entity_emitters = provider;
            self.local_lights = registry;
            log::info!(
                "[LOCAL_LIGHT][RASTER_PROVIDER] action=spawn entity={:?} part={} provider_source_revision={} provider_source_count={} registry_revision={} registry_source_revision={} light_slot={} light_generation={} surface_emissive_pixels=false",
                entity,
                SPRINKLER_HEAD_EMITTER_PART.get(),
                change.source_revision,
                change.source_count,
                self.local_lights.registry_revision(),
                self.local_lights.snapshot().source_revision(),
                light_id.slot(),
                light_id.generation(),
            );
            light_id
        });
        log::info!("Placed sprinkler {} at {:?}", id, base_position);
        Ok((entity, light_id))
    }

    pub(super) fn apply_emissive_sprinkler_placement(
        &mut self,
        target: SprinklerPlacementTarget,
        component: RasterEmitterComponent,
    ) -> Result<(RasterEntityId, LightId)> {
        let (entity, light_id) =
            self.apply_sprinkler_placement_internal(target, Some(component))?;
        Ok((
            entity,
            light_id.expect("emissive sprinkler placement must publish one light"),
        ))
    }

    pub(super) fn update_emissive_sprinkler(
        &mut self,
        entity: RasterEntityId,
        base_position: Vec3,
        component: RasterEmitterComponent,
    ) -> Result<LightId> {
        let plan = self
            .sprinklers
            .plan_emitter_update(entity, base_position, component)
            .ok_or_else(|| anyhow!("raster emitter entity is not live: {entity:?}"))?;
        if !plan.changed() {
            let (key, _) = plan.emitter_source();
            return self
                .local_lights
                .light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, key.source_key())
                .ok_or_else(|| anyhow!("live raster emitter has no registry light: {entity:?}"));
        }
        let (_, staged_component) = plan.emitter_source();
        let (provider, registry, change, light_id) =
            self.stage_raster_emitter_upsert(entity, staged_component)?;
        let previous_render_instances = self.sprinklers.render_instances();
        self.tracer.upload_sprinklers(&plan.render_instances)?;
        if let Err(effect_error) = self.surface_builder.upsert_external_grass_growth_influence(
            sprinkler_grass_influence_id(entity.slot()),
            plan.grass_influence,
            self.time_info.time_since_start_duration().as_millis() as u32,
        ) {
            if let Err(rollback_error) = self.tracer.upload_sprinklers(&previous_render_instances) {
                return Err(anyhow!(
                    "failed to move sprinkler grass influence: {effect_error}; failed to restore sprinkler render instances: {rollback_error}"
                ));
            }
            return Err(effect_error);
        }
        self.sprinklers.commit_emitter_update(plan);
        self.raster_entity_emitters = provider;
        self.local_lights = registry;
        log::info!(
            "[LOCAL_LIGHT][RASTER_PROVIDER] action=update entity={:?} part={} provider_changed={} provider_source_revision={} provider_source_count={} registry_revision={} registry_source_revision={} light_slot={} light_generation={} surface_emissive_pixels=false",
            entity,
            SPRINKLER_HEAD_EMITTER_PART.get(),
            change.changed,
            change.source_revision,
            change.source_count,
            self.local_lights.registry_revision(),
            self.local_lights.snapshot().source_revision(),
            light_id.slot(),
            light_id.generation(),
        );
        Ok(light_id)
    }

    pub(super) fn remove_emissive_sprinkler(&mut self, entity: RasterEntityId) -> Result<LightId> {
        self.sprinklers
            .emitter_source(entity)
            .ok_or_else(|| anyhow!("raster emitter entity is not live: {entity:?}"))?;
        let key = RasterEmitterKey::new(entity, SPRINKLER_HEAD_EMITTER_PART);
        let light_id = self
            .local_lights
            .light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, key.source_key())
            .ok_or_else(|| anyhow!("live raster emitter has no registry light: {entity:?}"))?;
        let plan = self
            .sprinklers
            .plan_removal_entity(entity)
            .expect("live raster emitter entity must have a removal plan");
        let staged_lights = self
            .stage_raster_emitter_removals(plan.removed_emitter_entities())?
            .expect("emissive entity removal must stage a provider publication");
        let previous_render_instances = self.sprinklers.render_instances();
        self.tracer.upload_sprinklers(plan.render_instances())?;
        if let Err(effect_error) = self
            .surface_builder
            .remove_external_grass_growth_influences(
                plan.removed_grass_influence_ids(),
                self.time_info.time_since_start_duration().as_millis() as u32,
            )
        {
            if let Err(rollback_error) = self.tracer.upload_sprinklers(&previous_render_instances) {
                return Err(anyhow!(
                    "failed to remove emissive sprinkler grass influence: {effect_error}; failed to restore sprinkler render instances: {rollback_error}"
                ));
            }
            return Err(effect_error);
        }
        self.sprinklers.commit_removal(plan);
        self.raster_entity_emitters = staged_lights.0;
        self.local_lights = staged_lights.1;
        log::info!(
            "[LOCAL_LIGHT][RASTER_PROVIDER] action=despawn entity={:?} part={} provider_source_count={} registry_revision={} registry_source_revision={} removed_light_slot={} removed_light_generation={} surface_emissive_pixels=false",
            entity,
            SPRINKLER_HEAD_EMITTER_PART.get(),
            self.raster_entity_emitters.source_count(),
            self.local_lights.registry_revision(),
            self.local_lights.snapshot().source_revision(),
            light_id.slot(),
            light_id.generation(),
        );
        Ok(light_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::UVec3;

    fn test_raster_emitter(position: Vec3, intensity: f32) -> RasterEmitterComponent {
        RasterEmitterComponent::new(crate::lighting::LocalLight::Point(
            crate::lighting::PointLight::new(
                position,
                Vec3::new(1.0, 0.5, 0.25),
                intensity,
                0.02,
                0.5,
            )
            .unwrap(),
        ))
    }

    fn commit_test_route(network: &mut IrrigationNetwork, start: Vec3, end: Vec3) {
        let begin = network
            .plan_begin_route(start)
            .expect("route should begin at a valid network position");
        network.commit_route_preview(begin);
        let finish = network
            .plan_finish_route(end)
            .expect("route planning should succeed")
            .expect("active route should produce a commit plan");
        network.commit_route(finish);
    }

    #[test]
    fn digging_brush_overlap_uses_capsule_distance() {
        let start = Vec3::ZERO;
        let end = Vec3::X;
        let side_distance = distance_sq_to_segment(Vec3::new(0.5, 0.2, 0.0), start, end);
        let end_distance = distance_sq_to_segment(Vec3::new(1.2, 0.0, 0.0), start, end);
        assert!((side_distance - 0.04).abs() < 1e-6);
        assert!((end_distance - 0.04).abs() < 1e-6);
    }

    #[test]
    fn sprinkler_grass_suppression_is_centered_on_footprint() {
        let influence = sprinkler_grass_influence(Vec3::new(0.5, 0.25, 0.75));
        assert_eq!(influence.center_world_vox, UVec3::new(128, 64, 192));
        assert_eq!(influence.radius_voxels, 10);
        assert_eq!(influence.min_level, 0);
        assert_ne!(
            sprinkler_grass_influence_id(1),
            sprinkler_grass_influence_id(2)
        );
    }

    #[test]
    fn sprinkler_runtime_plans_then_atomically_commits_lifecycle_changes() {
        let mut runtime = SprinklerRuntime::new();
        let base_position = Vec3::new(0.5, 0.25, 0.75);
        let placement = runtime
            .plan_placement(SprinklerPlacementTarget::Terrain(base_position))
            .unwrap();

        assert!(runtime.is_empty());
        assert_eq!(placement.render_instances().len(), 1);
        assert_eq!(placement.base_position(), base_position);
        let (influence_id, influence) = placement.grass_influence();
        assert_eq!(influence_id, sprinkler_grass_influence_id(1));
        assert_eq!(influence, sprinkler_grass_influence(base_position));

        runtime.commit_placement(placement);
        assert_eq!(runtime.len(), 1);
        assert_eq!(runtime.records.len(), runtime.emitters.len());
        assert_eq!(runtime.moisture_sources().len(), 1);

        let removal = runtime
            .plan_removal(TerrainBrushEdit {
                start: base_position,
                end: base_position,
                radius: 0.01,
            })
            .expect("overlapping brush should plan sprinkler removal");
        assert_eq!(removal.removed_count(), 1);
        assert!(removal.render_instances().is_empty());
        assert_eq!(
            removal.removed_grass_influence_ids(),
            &[sprinkler_grass_influence_id(1)]
        );
        assert_eq!(runtime.len(), 1, "planning must not mutate canonical state");

        assert_eq!(runtime.commit_removal(removal), 1);
        assert!(runtime.is_empty());
        assert_eq!(runtime.records.len(), runtime.emitters.len());
    }

    #[test]
    fn ordinary_sprinklers_have_no_emitter_component() {
        let runtime = SprinklerRuntime::new();
        let plan = runtime
            .plan_placement(SprinklerPlacementTarget::Terrain(Vec3::ZERO))
            .unwrap();

        assert!(plan.emitter_source().is_none());
        assert_eq!(plan.raster_entity_id().generation(), 1);
    }

    #[test]
    fn sprinkler_identity_exhaustion_is_explicit_and_never_reuses_u32_consumers() {
        let mut runtime = SprinklerRuntime::new();
        runtime.next_id = u64::from(u32::MAX);
        let last = runtime
            .plan_placement(SprinklerPlacementTarget::Terrain(Vec3::ZERO))
            .unwrap();
        assert_eq!(last.id(), u32::MAX);
        assert_eq!(last.raster_entity_id().slot(), u32::MAX);
        assert_eq!(
            last.grass_influence().0,
            sprinkler_grass_influence_id(u32::MAX)
        );
        runtime.commit_placement(last);
        let revision_after_last = runtime.revision;

        let error = runtime
            .plan_placement(SprinklerPlacementTarget::Terrain(Vec3::ONE))
            .err()
            .expect("the u32 identity space must be exhausted");
        assert!(error.to_string().contains("identity space exhausted"));
        assert_eq!(runtime.next_id, u64::from(u32::MAX) + 1);
        assert_eq!(runtime.revision, revision_after_last);
        assert_eq!(
            runtime.len(),
            1,
            "failed preparation must not mutate records"
        );
        assert_eq!(runtime.records.len(), runtime.emitters.len());
    }

    #[test]
    fn emissive_sprinkler_update_keeps_identity_and_noop_keeps_all_revisions() {
        let mut runtime = SprinklerRuntime::new();
        let initial_position = Vec3::new(0.25, 0.5, 0.75);
        let initial_component = test_raster_emitter(initial_position, 2.0);
        let placement = runtime
            .plan_placement_with_emitter(
                SprinklerPlacementTarget::Terrain(initial_position),
                Some(initial_component),
            )
            .unwrap();
        let entity = placement.raster_entity_id();
        let (key, _) = placement.emitter_source().unwrap();
        runtime.commit_placement(placement);

        let mut provider = crate::lighting::RasterEntityEmitterProvider::default();
        provider
            .publish_entity(entity, [(SPRINKLER_HEAD_EMITTER_PART, initial_component)])
            .unwrap();
        let mut registry = crate::lighting::LocalLightRegistry::default();
        registry.reconcile(provider.snapshot()).unwrap();
        let light_id = registry
            .light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, key.source_key())
            .unwrap();
        let revisions_before_noop = (
            runtime.revision,
            provider.snapshot().source_revision(),
            registry.registry_revision(),
            registry.snapshot().source_revision(),
        );

        let noop = runtime
            .plan_emitter_update(entity, initial_position, initial_component)
            .unwrap();
        assert!(!noop.changed());
        runtime.commit_emitter_update(noop);
        let provider_noop = provider
            .publish_entity(entity, [(SPRINKLER_HEAD_EMITTER_PART, initial_component)])
            .unwrap();
        assert!(!provider_noop.changed);
        registry.reconcile(provider.snapshot()).unwrap();
        assert_eq!(
            (
                runtime.revision,
                provider.snapshot().source_revision(),
                registry.registry_revision(),
                registry.snapshot().source_revision(),
            ),
            revisions_before_noop
        );

        let moved_position = Vec3::new(0.5, 0.5, 0.75);
        let moved_component = test_raster_emitter(moved_position, 4.0);
        let update = runtime
            .plan_emitter_update(entity, moved_position, moved_component)
            .unwrap();
        assert!(update.changed());
        assert_eq!(runtime.records[0].base_position, initial_position);
        runtime.commit_emitter_update(update);
        provider
            .publish_entity(entity, [(SPRINKLER_HEAD_EMITTER_PART, moved_component)])
            .unwrap();
        registry.reconcile(provider.snapshot()).unwrap();
        assert_eq!(runtime.records[0].raster_entity_id(), entity);
        assert_eq!(runtime.records[0].base_position, moved_position);
        assert_eq!(
            registry.light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, key.source_key()),
            Some(light_id),
            "move and photometric update must preserve LightId"
        );

        let removal = runtime.plan_removal_entity(entity).unwrap();
        let provider_revision = provider.snapshot().source_revision();
        provider
            .remove_entities(removal.removed_emitter_entities().iter().copied())
            .unwrap();
        assert_eq!(provider.snapshot().source_revision(), provider_revision + 1);
        registry.reconcile(provider.snapshot()).unwrap();
        runtime.commit_removal(removal);
        assert!(runtime.is_empty());
        assert!(registry
            .light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, key.source_key())
            .is_none());
    }

    #[test]
    fn sprinkler_removal_plan_ignores_non_overlapping_brushes() {
        let mut runtime = SprinklerRuntime::new();
        let placement = runtime
            .plan_placement(SprinklerPlacementTarget::Terrain(Vec3::ZERO))
            .unwrap();
        runtime.commit_placement(placement);

        assert!(runtime
            .plan_removal(TerrainBrushEdit {
                start: Vec3::ONE,
                end: Vec3::ONE,
                radius: 0.01,
            })
            .is_none());
        assert_eq!(runtime.len(), 1);
    }

    #[test]
    fn sprinkler_droplets_match_fallen_leaf_particle_size() {
        assert_eq!(
            SPRINKLER_DROPLET_SIZE,
            crate::particles::LeafEmitterDesc::default().size
        );
    }

    #[test]
    fn sprinkler_launch_lands_at_sampled_radius() {
        let nozzle_height = SPRINKLER_NOZZLE_HEIGHT_VOXELS / VOXELS_PER_WORLD_UNIT;
        for radius in [
            SPRINKLER_MIN_LANDING_RADIUS,
            0.12,
            SPRINKLER_MAX_LANDING_RADIUS,
        ] {
            for elevation in [SPRINKLER_MIN_ELEVATION, SPRINKLER_MAX_ELEVATION] {
                let (horizontal_speed, vertical_speed, flight_time) =
                    sprinkler_ballistic_launch(radius, elevation);
                assert!((horizontal_speed * flight_time - radius).abs() < 1e-6);
                let landing_height = nozzle_height + vertical_speed * flight_time
                    - 0.5 * SPRINKLER_GRAVITY * flight_time * flight_time;
                assert!(landing_height.abs() < 1e-6);
                assert!(vertical_speed > 0.0);
            }
        }
    }

    #[test]
    fn sprinkler_spray_axis_alternates_by_opposing_pair() {
        let tick_seconds = 0.05;
        assert!(!sprinkler_sprays_along_x(0, tick_seconds, 0.0));
        assert!(!sprinkler_sprays_along_x(19, tick_seconds, 0.0));
        assert!(sprinkler_sprays_along_x(20, tick_seconds, 0.0));
        assert!(sprinkler_sprays_along_x(39, tick_seconds, 0.0));
        assert!(!sprinkler_sprays_along_x(40, tick_seconds, 0.0));
    }

    #[test]
    fn sprinkler_phase_is_stable_and_varies_per_instance() {
        let position = Vec3::new(0.5, 0.25, 0.75);
        let first = sprinkler_animation_phase(1, position);
        assert_eq!(first, sprinkler_animation_phase(1, position));
        assert_ne!(first, sprinkler_animation_phase(2, position));
        assert!((0.0..1.0).contains(&first));
    }

    #[test]
    fn pipe_preview_does_not_commit_topology() {
        let mut network = IrrigationNetwork::default();
        let begin = network.plan_begin_route(Vec3::new(0.5, 0.25, 0.5)).unwrap();
        assert!(
            !network.route_active(),
            "planning must not mutate route state"
        );
        network.commit_route_preview(begin);
        let update = network
            .plan_update_route(Vec3::new(0.75, 0.25, 0.5))
            .unwrap();
        let preview = update.render_data();

        assert!(preview.source_position.is_some());
        assert_eq!(preview.segments.len(), 1);
        assert_eq!(
            network.active_drag.unwrap().end_voxels,
            IrrigationNetwork::snap_surface_position(Vec3::new(0.5, 0.25, 0.5)),
            "planning a preview update must preserve the committed endpoint"
        );
        assert!(network.source_node.is_none());
        assert!(network.segments.is_empty());
    }

    #[test]
    fn pipe_finish_plan_preserves_topology_until_committed() {
        let mut network = IrrigationNetwork::default();
        let begin = network.plan_begin_route(Vec3::new(0.5, 0.25, 0.5)).unwrap();
        network.commit_route_preview(begin);

        let finish = network
            .plan_finish_route(Vec3::new(0.75, 0.25, 0.5))
            .unwrap()
            .unwrap();

        assert!(network.route_active());
        assert!(network.source_node.is_none());
        assert!(network.segments.is_empty());
        assert!(finish.render_data().source_position.is_some());
        assert_eq!(finish.render_data().segments.len(), 1);

        network.commit_route(finish);
        assert!(!network.route_active());
        assert!(network.source_node.is_some());
        assert_eq!(network.segments.len(), 1);
    }

    #[test]
    fn pipe_preview_excludes_the_committed_network() {
        let mut network = IrrigationNetwork::default();
        commit_test_route(
            &mut network,
            Vec3::new(0.5, 0.25, 0.5),
            Vec3::new(0.75, 0.25, 0.5),
        );
        let committed_segment_count = network.segments.len();
        let start = network.nodes.last().unwrap().position_voxels;
        let begin_world = (start - IVec3::Y).as_vec3() / VOXELS_PER_WORLD_UNIT;
        let end_world = (start + IVec3::new(4, 3, 2) - IVec3::Y).as_vec3() / VOXELS_PER_WORLD_UNIT;
        let begin = network.plan_begin_route(begin_world).unwrap();
        network.commit_route_preview(begin);
        let preview = network.plan_update_route(end_world).unwrap();

        assert_eq!(preview.render_data().source_position, None);
        assert_eq!(preview.render_data().segments.len(), 3);
        assert!(committed_segment_count > 0);
    }

    #[test]
    fn pipe_route_is_axis_aligned_and_connected_to_source() {
        let mut network = IrrigationNetwork::default();
        commit_test_route(
            &mut network,
            Vec3::new(0.5, 0.25, 0.5),
            Vec3::new(0.75, 0.5, 0.9),
        );

        assert!(network.source_node.is_some());
        assert!(!network.segments.is_empty());
        assert!(network.segments.iter().all(|segment| {
            let start = network.node(segment.start_node).unwrap().position_voxels;
            let end = network.node(segment.end_node).unwrap().position_voxels;
            let changed_axes =
                (start.x != end.x) as u8 + (start.y != end.y) as u8 + (start.z != end.z) as u8;
            changed_axes == 1
        }));
        assert!(network
            .segments
            .iter()
            .all(|segment| network.segment_is_connected(segment)));
    }

    #[test]
    fn sprinkler_ray_attaches_to_middle_of_pipe() {
        let mut network = IrrigationNetwork::default();
        commit_test_route(
            &mut network,
            Vec3::new(0.5, 0.25, 0.5),
            Vec3::new(0.75, 0.25, 0.5),
        );
        let pipe_y = network
            .node(network.segments[0].start_node)
            .unwrap()
            .position_voxels
            .y as f32
            / VOXELS_PER_WORLD_UNIT;
        let attachment = network
            .ray_attachment(Vec3::new(0.625, pipe_y + 0.25, 0.5), Vec3::NEG_Y, 1.0)
            .unwrap();

        assert!(network.segment_is_connected(&network.segments[0]));
        assert!(
            (attachment.attachment.position_voxels.x / VOXELS_PER_WORLD_UNIT - 0.625).abs() < 0.01
        );
    }

    #[test]
    fn sprinkler_ray_does_not_require_a_powered_pipe() {
        let mut network = IrrigationNetwork::default();
        let source = network.upsert_node(IVec3::ZERO, IrrigationNodeKind::Source);
        network.source_node = Some(source);
        let first = network.upsert_node(IVec3::new(100, 0, 0), IrrigationNodeKind::Junction);
        let second = network.upsert_node(IVec3::new(120, 0, 0), IrrigationNodeKind::Junction);
        network.segments.push(PipeSegment {
            start_node: first,
            end_node: second,
        });
        network.refresh_connectivity();

        let attachment = network
            .ray_attachment(
                Vec3::new(110.0, 20.0, 0.0) / VOXELS_PER_WORLD_UNIT,
                Vec3::NEG_Y,
                1.0,
            )
            .unwrap();
        assert!(!network.segment_is_connected(&network.segments[0]));
        assert_eq!(attachment.attachment.position_voxels.x, 110.0);

        network.source_node = None;
        network.refresh_connectivity();
        assert!(network
            .ray_attachment(
                Vec3::new(110.0, 20.0, 0.0) / VOXELS_PER_WORLD_UNIT,
                Vec3::NEG_Y,
                1.0,
            )
            .is_some());
    }

    #[test]
    fn sprinkler_ray_requires_an_existing_pipe() {
        let network = IrrigationNetwork::default();
        assert!(network.ray_attachment(Vec3::Y, Vec3::NEG_Y, 2.0).is_none());
    }

    #[test]
    fn sprinkler_ray_does_not_snap_from_a_nearby_miss() {
        let mut network = IrrigationNetwork::default();
        commit_test_route(
            &mut network,
            Vec3::new(0.5, 0.25, 0.5),
            Vec3::new(0.75, 0.25, 0.5),
        );
        let segment = network.segments[0];
        let pipe_y = network.node(segment.start_node).unwrap().position_voxels.y as f32
            / VOXELS_PER_WORLD_UNIT;
        let miss_z = 0.5 + (IRRIGATION_PIPE_RADIUS_VOXELS + 0.25) / VOXELS_PER_WORLD_UNIT;

        assert!(network
            .ray_attachment(Vec3::new(0.625, pipe_y + 0.25, miss_z), Vec3::NEG_Y, 1.0,)
            .is_none());
    }

    #[test]
    fn sprinkler_ray_selects_the_frontmost_pipe() {
        let mut network = IrrigationNetwork::default();
        let low_start = network.upsert_node(IVec3::new(100, 100, 0), IrrigationNodeKind::Junction);
        let low_end = network.upsert_node(IVec3::new(120, 100, 0), IrrigationNodeKind::Junction);
        let high_start = network.upsert_node(IVec3::new(100, 200, 0), IrrigationNodeKind::Junction);
        let high_end = network.upsert_node(IVec3::new(120, 200, 0), IrrigationNodeKind::Junction);
        network.segments.extend([
            PipeSegment {
                start_node: low_start,
                end_node: low_end,
            },
            PipeSegment {
                start_node: high_start,
                end_node: high_end,
            },
        ]);

        let hit = network
            .ray_attachment(
                Vec3::new(110.0, 256.0, 0.0) / VOXELS_PER_WORLD_UNIT,
                Vec3::NEG_Y,
                2.0,
            )
            .unwrap();

        assert_eq!(hit.attachment.position_voxels.y, 200.0);
    }
}
