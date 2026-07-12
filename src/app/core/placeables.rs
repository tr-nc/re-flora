use super::App;
use crate::app::world_edits::TerrainBrushEdit;
use crate::particles::{
    MotionMode, ParticleEmitter, ParticleRenderKind, ParticleSpawn, ParticleSystem,
    ParticleUpdateConfig,
};
use crate::tracer::SprinklerRenderInstance;
use anyhow::Result;
use glam::{Vec3, Vec4};
use rand::{rngs::SmallRng, RngExt, SeedableRng};

const VOXELS_PER_WORLD_UNIT: f32 = 256.0;

// The rasterized prop mesh is four voxels tall: a three-voxel black stem and a
// one-voxel bright-orange cross-shaped head. Keep the emitter at its top surface.
const SPRINKLER_NOZZLE_HEIGHT_VOXELS: f32 = 4.0;

const SPRINKLER_SPAWN_RATE_PER_SECOND: f32 = 576.0;
const SPRINKLER_MAX_SPAWN_PER_FRAME: u32 = 192;
const SPRINKLER_DROPLET_SIZE: f32 = 0.00125;
const SPRINKLER_DROPLET_LIFETIME: f32 = 0.62;
const SPRINKLER_GRAVITY_FACTOR: f32 = 0.82;
const SPRINKLER_COLOR_LOW: Vec4 = Vec4::new(0.03, 0.20, 0.95, 0.94);
const SPRINKLER_COLOR_HIGH: Vec4 = Vec4::new(0.10, 0.48, 1.0, 0.98);
const SPRINKLER_PARTICLE_UPDATE: ParticleUpdateConfig = ParticleUpdateConfig::new(0.1, 2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlaceableKind {
    Tree,
    Sprinkler,
}

impl PlaceableKind {
    pub(super) fn from_slot(slot_idx: usize) -> Self {
        match slot_idx {
            super::ui_style::SPRINKLER_PLACEABLE_SLOT_INDEX => Self::Sprinkler,
            _ => Self::Tree,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Tree => "Tree",
            Self::Sprinkler => "Sprinkler",
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
        let horizontal_speed = self.rng.random_range(0.42..=0.95);
        let vertical_speed = self.rng.random_range(0.20..=0.44);
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
            size: SPRINKLER_DROPLET_SIZE * self.rng.random_range(0.75..=1.25),
            lifetime: SPRINKLER_DROPLET_LIFETIME * self.rng.random_range(0.82..=1.18),
            wind_factor: 0.0,
            gravity_factor: SPRINKLER_GRAVITY_FACTOR,
            drift_direction,
            drift_strength: self.rng.random_range(0.00..=0.025),
            drift_frequency: self.rng.random_range(1.5..=4.0),
            speed_noise_offset: self.rng.random_range(0.0..10_000.0) + self.id as f32,
            motion_mode: MotionMode::Free,
            sink_on_lifetime: false,
            sink_speed: 0.0,
            texture_variant: 0,
            render_kind: ParticleRenderKind::Leaf,
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
        self.sprinkler_emitters.retain(|emitter| {
            retained_records
                .iter()
                .any(|record| record.id == emitter.id)
        });
        self.sprinkler_records = retained_records;
        log::info!("Removed {} sprinkler(s) with digging brush", removed_count);
        Ok(removed_count)
    }

    pub(super) fn apply_sprinkler_placement(&mut self, base_position: Vec3) -> Result<()> {
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

        self.next_sprinkler_id = self.next_sprinkler_id.wrapping_add(1).max(1);
        self.sprinkler_records.push(SprinklerRecord {
            id,
            base_position,
            nozzle_position,
            animation_phase,
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
}
