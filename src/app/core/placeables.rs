use super::App;
use crate::app::world_edits::TerrainBrushEdit;
use crate::builder::GrassGrowthInfluence;
use crate::particles::{
    MotionMode, ParticleEmitter, ParticleRenderKind, ParticleSpawn, ParticleSystem,
    ParticleUpdateConfig, STANDARD_PARTICLE_SIZE,
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
}
