use super::App;
use crate::app::world_edits::TerrainBrushEdit;
use crate::builder::GrassGrowthInfluence;
use crate::particles::{
    MotionMode, ParticleEmitter, ParticleRenderKind, ParticleSpawn, ParticleSystem,
    ParticleUpdateConfig, STANDARD_PARTICLE_SIZE,
};
use crate::tracer::{
    IrrigationPipeRenderData, IrrigationPipeRenderSegment, SprinklerRenderInstance,
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
const PIPE_ATTACHMENT_MAX_DISTANCE_VOXELS: f32 = 8.0;
const PIPE_START_MAX_DISTANCE_VOXELS: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IrrigationNodeKind {
    Source,
    Junction,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct IrrigationNode {
    pub id: u32,
    pub position_voxels: IVec3,
    pub kind: IrrigationNodeKind,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PipeSegment {
    pub id: u32,
    pub start_node: u32,
    pub end_node: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PipeAttachment {
    pub segment_id: u32,
    pub position_voxels: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PipeDrag {
    pub start_voxels: IVec3,
    pub end_voxels: IVec3,
}

#[derive(Debug, Default)]
pub(super) struct IrrigationNetwork {
    source_node: Option<u32>,
    nodes: Vec<IrrigationNode>,
    segments: Vec<PipeSegment>,
    next_node_id: u32,
    next_segment_id: u32,
    powered_nodes: HashSet<u32>,
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

    #[cfg(test)]
    fn segment_is_powered(&self, segment_id: u32) -> bool {
        self.segments
            .iter()
            .find(|segment| segment.id == segment_id)
            .is_some_and(|segment| self.segment_is_connected(segment))
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

    pub(super) fn begin_drag(&self, world_position: Vec3) -> Option<PipeDrag> {
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

    pub(super) fn commit_drag(&mut self, drag: PipeDrag, world_end: Vec3) -> Result<()> {
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
            let id = self.next_segment_id.max(1);
            self.next_segment_id = id.wrapping_add(1).max(1);
            self.segments.push(PipeSegment {
                id,
                start_node: start_id,
                end_node: end_id,
            });
        }
        self.refresh_connectivity();
        Ok(())
    }

    pub(super) fn nearest_attachment(&self, world_position: Vec3) -> Option<PipeAttachment> {
        let point_voxels = world_position * VOXELS_PER_WORLD_UNIT;
        self.segments
            .iter()
            .filter_map(|segment| {
                let start = self.node(segment.start_node)?.position_voxels.as_vec3();
                let end = self.node(segment.end_node)?.position_voxels.as_vec3();
                let axis = end - start;
                let length_sq = axis.length_squared();
                if length_sq <= f32::EPSILON {
                    return None;
                }
                let t = ((point_voxels - start).dot(axis) / length_sq).clamp(0.0, 1.0);
                let attachment = start + axis * t;
                let distance_sq = attachment.distance_squared(point_voxels);
                (distance_sq <= PIPE_ATTACHMENT_MAX_DISTANCE_VOXELS.powi(2)).then_some((
                    distance_sq,
                    PipeAttachment {
                        segment_id: segment.id,
                        position_voxels: attachment,
                    },
                ))
            })
            .min_by(|left, right| left.0.total_cmp(&right.0))
            .map(|(_, attachment)| attachment)
    }

    pub(super) fn preview_render_data(&self, drag: PipeDrag) -> IrrigationPipeRenderData {
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

    pub(super) fn render_data(&self) -> IrrigationPipeRenderData {
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
    pub(super) fn from_slot(slot_idx: usize) -> Self {
        match slot_idx {
            super::ui_style::SPRINKLER_PLACEABLE_SLOT_INDEX => Self::Sprinkler,
            super::ui_style::PIPE_PLACEABLE_SLOT_INDEX => Self::Pipe,
            _ => Self::Tree,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Tree => "Tree",
            Self::Sprinkler => "Sprinkler",
            Self::Pipe => "Pipe",
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(super) struct SprinklerRecord {
    pub id: u32,
    pub base_position: Vec3,
    pub nozzle_position: Vec3,
    pub animation_phase: f32,
    pub pipe_segment_id: u32,
}

pub(super) struct SprinklerEmitter {
    id: u32,
    nozzle_position: Vec3,
    rng: SmallRng,
    spawn_accumulator: f32,
    animation_phase: f32,
    animation_tick: u32,
    animation_tick_seconds: f32,
}

impl SprinklerEmitter {
    pub(super) fn new(id: u32, nozzle_position: Vec3, animation_phase: f32) -> Self {
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

    pub(super) fn set_animation_clock(&mut self, tick: u32, tick_seconds: f32) {
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

pub(super) fn distance_sq_to_segment(point: Vec3, start: Vec3, end: Vec3) -> f32 {
    let segment = end - start;
    let length_sq = segment.length_squared();
    if length_sq <= f32::EPSILON {
        return point.distance_squared(start);
    }
    let t = ((point - start).dot(segment) / length_sq).clamp(0.0, 1.0);
    point.distance_squared(start + segment * t)
}

pub(super) fn sprinkler_sprays_along_x(tick: u32, tick_seconds: f32, animation_phase: f32) -> bool {
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
    pub(super) fn current_placeable_kind(&self) -> PlaceableKind {
        PlaceableKind::from_slot(self.player_tools.selected_placeable_panel_slot)
    }

    pub(super) fn current_placeable_label(&self) -> &'static str {
        self.current_placeable_kind().label()
    }

    fn upload_irrigation_network(&mut self) -> Result<()> {
        self.tracer
            .upload_irrigation_pipes(&self.irrigation_network.render_data())
    }

    pub(super) fn begin_pipe_drag(&mut self, world_position: Vec3) {
        self.active_pipe_drag = self.irrigation_network.begin_drag(world_position);
        if let Some(drag) = self.active_pipe_drag {
            if let Err(err) = self
                .tracer
                .upload_irrigation_pipe_preview(&self.irrigation_network.preview_render_data(drag))
            {
                log::error!("Failed to show irrigation pipe preview: {err}");
            }
        } else {
            log::info!("Pipe drag must start near the source or an existing junction");
        }
    }

    pub(super) fn update_pipe_drag_preview(&mut self, world_position: Vec3) -> Result<()> {
        let Some(drag) = self.active_pipe_drag.as_mut() else {
            return Ok(());
        };
        let end_voxels = IrrigationNetwork::snap_surface_position(world_position);
        if drag.end_voxels == end_voxels {
            return Ok(());
        }
        drag.end_voxels = end_voxels;
        self.tracer
            .upload_irrigation_pipe_preview(&self.irrigation_network.preview_render_data(*drag))
    }

    pub(super) fn finish_pipe_drag(&mut self, world_position: Vec3) -> Result<()> {
        let Some(drag) = self.active_pipe_drag.take() else {
            return Ok(());
        };
        let result = self
            .irrigation_network
            .commit_drag(drag, world_position)
            .and_then(|()| self.upload_irrigation_network());
        self.tracer.clear_irrigation_pipe_preview();
        result?;
        log::info!(
            "Committed irrigation pipe route from {:?}",
            drag.start_voxels
        );
        Ok(())
    }

    pub(super) fn cancel_pipe_drag(&mut self) {
        if self.active_pipe_drag.take().is_some() {
            self.tracer.clear_irrigation_pipe_preview();
        }
    }

    pub(super) fn remove_sprinklers_in_brush(&mut self, edit: TerrainBrushEdit) -> Result<usize> {
        let retained_records = self
            .sprinkler_records
            .iter()
            .copied()
            .filter(|sprinkler| {
                distance_sq_to_segment(sprinkler.base_position, edit.start, edit.end)
                    > edit.radius * edit.radius
            })
            .collect::<Vec<_>>();
        let removed_count = self.sprinkler_records.len() - retained_records.len();
        if removed_count == 0 {
            return Ok(0);
        }

        let render_instances = retained_records
            .iter()
            .map(|sprinkler| SprinklerRenderInstance {
                base_position: sprinkler.base_position,
                animation_phase: sprinkler.animation_phase,
            })
            .collect::<Vec<_>>();
        self.tracer.upload_sprinklers(&render_instances)?;
        let removed_ids = self
            .sprinkler_records
            .iter()
            .filter(|record| {
                !retained_records
                    .iter()
                    .any(|retained| retained.id == record.id)
            })
            .map(|record| sprinkler_grass_influence_id(record.id))
            .collect::<Vec<_>>();
        self.surface_builder
            .remove_external_grass_growth_influences(
                &removed_ids,
                self.time_info.time_since_start_duration().as_millis() as u32,
            )?;
        self.sprinkler_emitters.retain(|emitter| {
            retained_records
                .iter()
                .any(|record| record.id == emitter.id)
        });
        self.sprinkler_records = retained_records;
        log::info!("Removed {} sprinkler(s) with terrain brush", removed_count);
        Ok(removed_count)
    }

    pub(super) fn apply_sprinkler_placement(&mut self, cursor_position: Vec3) -> Result<()> {
        let attachment = self
            .irrigation_network
            .nearest_attachment(cursor_position)
            .ok_or_else(|| anyhow!("sprinkler must attach to an irrigation pipe"))?;
        let base_position = attachment.position_voxels / VOXELS_PER_WORLD_UNIT;
        let nozzle_position =
            base_position + Vec3::Y * (SPRINKLER_NOZZLE_HEIGHT_VOXELS / VOXELS_PER_WORLD_UNIT);

        let id = self.next_sprinkler_id;
        let animation_phase = sprinkler_animation_phase(id, base_position);
        let mut render_instances = self
            .sprinkler_records
            .iter()
            .map(|sprinkler| SprinklerRenderInstance {
                base_position: sprinkler.base_position,
                animation_phase: sprinkler.animation_phase,
            })
            .collect::<Vec<_>>();
        render_instances.push(SprinklerRenderInstance {
            base_position,
            animation_phase,
        });
        self.tracer.upload_sprinklers(&render_instances)?;
        self.surface_builder
            .upsert_external_grass_growth_influence(
                sprinkler_grass_influence_id(id),
                sprinkler_grass_influence(base_position),
                self.time_info.time_since_start_duration().as_millis() as u32,
            )?;

        self.next_sprinkler_id = self.next_sprinkler_id.wrapping_add(1).max(1);
        self.sprinkler_records.push(SprinklerRecord {
            id,
            base_position,
            nozzle_position,
            animation_phase,
            pipe_segment_id: attachment.segment_id,
        });
        self.sprinkler_emitters
            .push(SprinklerEmitter::new(id, nozzle_position, animation_phase));
        log::info!("Placed sprinkler {} at {:?}", id, base_position);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::UVec3;

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
        let network = IrrigationNetwork::default();
        let mut drag = network.begin_drag(Vec3::new(0.5, 0.25, 0.5)).unwrap();
        drag.end_voxels = IrrigationNetwork::snap_surface_position(Vec3::new(0.75, 0.25, 0.5));
        let preview = network.preview_render_data(drag);

        assert!(preview.source_position.is_some());
        assert_eq!(preview.segments.len(), 1);
        assert!(network.source_node.is_none());
        assert!(network.segments.is_empty());
    }

    #[test]
    fn pipe_preview_excludes_the_committed_network() {
        let mut network = IrrigationNetwork::default();
        let first_drag = network.begin_drag(Vec3::new(0.5, 0.25, 0.5)).unwrap();
        network
            .commit_drag(first_drag, Vec3::new(0.75, 0.25, 0.5))
            .unwrap();
        let committed_segment_count = network.segments.len();
        let start = network.nodes.last().unwrap().position_voxels;
        let drag = PipeDrag {
            start_voxels: start,
            end_voxels: start + IVec3::new(4, 3, 2),
        };

        let preview = network.preview_render_data(drag);

        assert_eq!(preview.source_position, None);
        assert_eq!(preview.segments.len(), 3);
        assert!(committed_segment_count > 0);
    }

    #[test]
    fn pipe_route_is_axis_aligned_and_connected_to_source() {
        let mut network = IrrigationNetwork::default();
        let drag = network.begin_drag(Vec3::new(0.5, 0.25, 0.5)).unwrap();
        network
            .commit_drag(drag, Vec3::new(0.75, 0.5, 0.9))
            .unwrap();

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
    fn sprinkler_attaches_to_middle_of_pipe() {
        let mut network = IrrigationNetwork::default();
        let drag = network.begin_drag(Vec3::new(0.5, 0.25, 0.5)).unwrap();
        network
            .commit_drag(drag, Vec3::new(0.75, 0.25, 0.5))
            .unwrap();
        let attachment = network
            .nearest_attachment(Vec3::new(0.625, 0.25, 0.5))
            .unwrap();

        assert!(network.segment_is_powered(attachment.segment_id));
        assert!((attachment.position_voxels.x / VOXELS_PER_WORLD_UNIT - 0.625).abs() < 0.01);
    }

    #[test]
    fn sprinkler_attachment_does_not_require_a_powered_pipe() {
        let mut network = IrrigationNetwork::default();
        let source = network.upsert_node(IVec3::ZERO, IrrigationNodeKind::Source);
        network.source_node = Some(source);
        let first = network.upsert_node(IVec3::new(100, 0, 0), IrrigationNodeKind::Junction);
        let second = network.upsert_node(IVec3::new(120, 0, 0), IrrigationNodeKind::Junction);
        network.segments.push(PipeSegment {
            id: 1,
            start_node: first,
            end_node: second,
        });
        network.refresh_connectivity();

        let near_disconnected = Vec3::new(110.0, 0.0, 0.0) / VOXELS_PER_WORLD_UNIT;
        let attachment = network.nearest_attachment(near_disconnected).unwrap();
        assert!(!network.segment_is_powered(attachment.segment_id));

        network.source_node = None;
        network.refresh_connectivity();
        assert!(network.nearest_attachment(near_disconnected).is_some());
    }

    #[test]
    fn sprinkler_attachment_requires_an_existing_pipe() {
        let network = IrrigationNetwork::default();
        assert!(network.nearest_attachment(Vec3::ZERO).is_none());
    }
}
