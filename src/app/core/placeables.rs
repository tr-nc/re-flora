use super::App;
use crate::particles::{
    MotionMode, ParticleEmitter, ParticleRenderKind, ParticleSpawn, ParticleSystem,
};
use anyhow::Result;
use glam::{Vec3, Vec4};
use rand::{rngs::SmallRng, RngExt, SeedableRng};
use std::f32::consts::TAU;

const VOXELS_PER_WORLD_UNIT: f32 = 256.0;

// The rasterized prop mesh is four voxels tall: a three-voxel black stem and a
// one-voxel bright-orange cross-shaped head. Keep the emitter at its top surface.
const SPRINKLER_NOZZLE_HEIGHT_VOXELS: f32 = 4.0;

const SPRINKLER_SPAWN_RATE_PER_SECOND: f32 = 72.0;
const SPRINKLER_MAX_SPAWN_PER_FRAME: u32 = 24;
const SPRINKLER_DROPLET_SIZE: f32 = 0.010;
const SPRINKLER_DROPLET_LIFETIME: f32 = 0.62;
const SPRINKLER_GRAVITY_FACTOR: f32 = 0.82;
const SPRINKLER_COLOR_LOW: Vec4 = Vec4::new(0.34, 0.68, 1.0, 0.92);
const SPRINKLER_COLOR_HIGH: Vec4 = Vec4::new(0.78, 0.93, 1.0, 0.96);

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
}

pub(super) struct SprinklerEmitter {
    id: u32,
    nozzle_position: Vec3,
    rng: SmallRng,
    spawn_accumulator: f32,
    phase: f32,
}

impl SprinklerEmitter {
    pub(super) fn new(id: u32, nozzle_position: Vec3) -> Self {
        let seed = sprinkler_seed(id, nozzle_position);
        let mut rng = SmallRng::seed_from_u64(seed);
        Self {
            id,
            nozzle_position,
            spawn_accumulator: 0.0,
            phase: rng.random_range(0.0..TAU),
            rng,
        }
    }

    fn spawn_droplet(&mut self, system: &mut ParticleSystem, time: f32) {
        let jet_sweep = time * 7.5 + self.phase;
        let angle = self.rng.random_range(0.0..TAU) + jet_sweep.sin() * 0.18;
        let horizontal_dir = Vec3::new(angle.cos(), 0.0, angle.sin());
        let horizontal_speed = self.rng.random_range(0.42..=0.95);
        let vertical_speed = self.rng.random_range(0.20..=0.44);
        let muzzle_jitter = Vec3::new(
            self.rng.random_range(-0.008..=0.008),
            self.rng.random_range(-0.002..=0.006),
            self.rng.random_range(-0.008..=0.008),
        );
        let color_mix = self.rng.random_range(0.0..=1.0);
        let color = SPRINKLER_COLOR_LOW.lerp(SPRINKLER_COLOR_HIGH, color_mix);
        let drift_angle = angle + self.rng.random_range(-0.5..=0.5);

        let spawn = ParticleSpawn {
            position: self.nozzle_position + horizontal_dir * 0.025 + muzzle_jitter,
            velocity: horizontal_dir * horizontal_speed + Vec3::Y * vertical_speed,
            color,
            size: SPRINKLER_DROPLET_SIZE * self.rng.random_range(0.75..=1.25),
            lifetime: SPRINKLER_DROPLET_LIFETIME * self.rng.random_range(0.82..=1.18),
            wind_factor: 0.0,
            gravity_factor: SPRINKLER_GRAVITY_FACTOR,
            drift_direction: Vec3::new(drift_angle.cos(), 0.0, drift_angle.sin()),
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
        };
        let _ = system.spawn(spawn);
    }
}

impl ParticleEmitter for SprinklerEmitter {
    fn update(&mut self, system: &mut ParticleSystem, dt: f32, time: f32) {
        if dt <= 0.0 {
            return;
        }

        self.spawn_accumulator += SPRINKLER_SPAWN_RATE_PER_SECOND * dt.min(0.12);
        let mut spawned = 0;
        while self.spawn_accumulator >= 1.0 && spawned < SPRINKLER_MAX_SPAWN_PER_FRAME {
            self.spawn_droplet(system, time);
            self.spawn_accumulator -= 1.0;
            spawned += 1;
        }
        if spawned == SPRINKLER_MAX_SPAWN_PER_FRAME {
            self.spawn_accumulator = self.spawn_accumulator.min(1.0);
        }
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

    pub(super) fn apply_sprinkler_placement(&mut self, base_position: Vec3) -> Result<()> {
        let nozzle_position =
            base_position + Vec3::Y * (SPRINKLER_NOZZLE_HEIGHT_VOXELS / VOXELS_PER_WORLD_UNIT);

        let mut render_positions = self
            .sprinkler_records
            .iter()
            .map(|sprinkler| sprinkler.base_position)
            .collect::<Vec<_>>();
        render_positions.push(base_position);
        self.tracer.upload_sprinklers(&render_positions)?;

        let id = self.next_sprinkler_id;
        self.next_sprinkler_id = self.next_sprinkler_id.wrapping_add(1).max(1);
        self.sprinkler_records.push(SprinklerRecord {
            id,
            base_position,
            nozzle_position,
        });
        self.sprinkler_emitters
            .push(SprinklerEmitter::new(id, nozzle_position));
        log::info!("Placed sprinkler {} at {:?}", id, base_position);
        Ok(())
    }
}
