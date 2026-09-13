use std::{f32::consts::TAU, ops::RangeInclusive};

use fastnoise_lite::{FastNoiseLite, NoiseType};
use glam::{Vec3, Vec4};
use rand::{rngs::SmallRng, RngExt, SeedableRng};

use super::butterfly_flight::{DartingFlightState, FLIGHT_STEP_SECONDS};
use super::{ButterflyFlightTuning, ButterflyFlightVariant};
use super::{
    MotionMode, ParticleHandle, ParticleRenderKind, ParticleSpawn, ParticleSystem,
    ParticleUpdateConfig, STANDARD_PARTICLE_SIZE,
};
use crate::tracer::ButterflyPalettePreset;
use crate::wind_field::WindFieldFrame;
use crate::wind_response::WindResponseCurve;

pub const WORM_STEP_LEN: f32 = 0.15;

const LEAF_UPDATE: ParticleUpdateConfig = ParticleUpdateConfig::new(0.1, 2);
const BUTTERFLY_UPDATE: ParticleUpdateConfig = ParticleUpdateConfig::new(0.1, 2);

pub trait ParticleEmitter {
    fn update(&mut self, system: &mut ParticleSystem, dt: f32, time: f32);
}

// bird spritesheet support has been removed

fn random_in_range(rng: &mut SmallRng, range: &RangeInclusive<f32>) -> f32 {
    let start = *range.start();
    let end = *range.end();
    if (end - start).abs() <= f32::EPSILON {
        start
    } else {
        rng.random_range(start..=end)
    }
}

fn random_color(rng: &mut SmallRng, low: Vec4, high: Vec4) -> Vec4 {
    let ordered = |a: f32, b: f32| -> (f32, f32) { (a.min(b), a.max(b)) };
    let (min_x, max_x) = ordered(low.x, high.x);
    let (min_y, max_y) = ordered(low.y, high.y);
    let (min_z, max_z) = ordered(low.z, high.z);
    let (min_w, max_w) = ordered(low.w, high.w);

    Vec4::new(
        rng.random_range(min_x..=max_x),
        rng.random_range(min_y..=max_y),
        rng.random_range(min_z..=max_z),
        rng.random_range(min_w..=max_w),
    )
}

fn butterfly_worm_noise_state(seed: i32, frequency: f32) -> FastNoiseLite {
    let mut state = FastNoiseLite::with_seed(seed);
    state.set_noise_type(Some(NoiseType::Perlin));
    log::info!("Butterfly worm noise frequency: {}", frequency);
    state.set_frequency(Some(frequency));
    state
}

fn butterfly_worm_noise_detail_state(seed: i32, frequency: f32) -> FastNoiseLite {
    let mut state = FastNoiseLite::with_seed(seed);
    state.set_noise_type(Some(NoiseType::Perlin));
    log::info!("Butterfly worm noise detail frequency: {}", frequency);
    state.set_frequency(Some(frequency));
    state
}

pub fn generate_worm_direction(
    noise: &FastNoiseLite,
    noise_detail: &FastNoiseLite,
    detail_weight: f32,
    seed: f32,
    time: f32,
) -> Vec3 {
    let nx = noise.get_noise_3d(seed, time, 0.0);
    let ny = noise.get_noise_3d(seed + 100.0, time, 0.0);
    let nz = noise.get_noise_3d(seed + 200.0, time, 0.0);

    let dx = noise_detail.get_noise_3d(seed, time, 0.0);
    let dy = noise_detail.get_noise_3d(seed + 100.0, time, 0.0);
    let dz = noise_detail.get_noise_3d(seed + 200.0, time, 0.0);

    let broad = Vec3::new(nx, ny, nz);
    let detail = Vec3::new(dx, dy, dz);

    let combined = broad + detail * detail_weight;
    combined.normalize_or_zero()
}

#[derive(Clone, Copy, Debug)]
pub struct LeafEmitterDesc {
    pub spawn_rate: f32,
    pub size: f32,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub color_low: Vec4,
    pub color_high: Vec4,
    pub wind_spawn_min_strength: f32,
    pub wind_spawn_max_strength: f32,
    pub wind_spawn_power: f32,
}

impl Default for LeafEmitterDesc {
    fn default() -> Self {
        Self {
            spawn_rate: 0.5,
            size: STANDARD_PARTICLE_SIZE,
            lifetime_min: 120.0,
            lifetime_max: 240.0,
            color_low: Vec4::new(212.0 / 255.0, 111.0 / 255.0, 0.0, 1.0),
            color_high: Vec4::new(242.0 / 255.0, 205.0 / 255.0, 0.0, 1.0),
            wind_spawn_min_strength: 0.5,
            wind_spawn_max_strength: 1.0,
            wind_spawn_power: 1.0,
        }
    }
}

impl LeafEmitterDesc {
    #[allow(dead_code)]
    pub fn wind_response_curve(&self) -> WindResponseCurve {
        WindResponseCurve {
            min_strength: self.wind_spawn_min_strength,
            max_strength: self.wind_spawn_max_strength,
            power: self.wind_spawn_power,
        }
    }
}

pub struct FallenLeafEmitter {
    pub center: Vec3,
    pub spawn_rate: f32,
    pub fall_chance: f32,
    pub size: f32,
    pub lifetime: RangeInclusive<f32>,
    pub color_low: Vec4,
    pub color_high: Vec4,
    pub wind_spawn_min_strength: f32,
    pub wind_spawn_max_strength: f32,
    pub wind_spawn_power: f32,
    leaf_positions: Vec<Vec3>,
    rng: SmallRng,
    spawn_accumulator: f32,
    pub enabled: bool,
}

impl FallenLeafEmitter {
    pub fn new(center: Vec3, leaf_positions: Vec<Vec3>, seed: u64, desc: &LeafEmitterDesc) -> Self {
        let mut rng = SmallRng::seed_from_u64(seed);
        let fall_chance = rng.random_range(0.2..=1.0);
        Self {
            center,
            spawn_rate: desc.spawn_rate,
            fall_chance,
            size: desc.size,
            lifetime: desc.lifetime_min..=desc.lifetime_max,
            color_low: desc.color_low,
            color_high: desc.color_high,
            wind_spawn_min_strength: desc.wind_spawn_min_strength,
            wind_spawn_max_strength: desc.wind_spawn_max_strength,
            wind_spawn_power: desc.wind_spawn_power,
            leaf_positions,
            rng,
            spawn_accumulator: 0.0,
            enabled: true,
        }
    }

    fn spawn_leaf(&mut self, system: &mut ParticleSystem) {
        let spawn_position = if self.leaf_positions.is_empty() {
            self.center
        } else {
            let leaf_idx = self.rng.random_range(0..self.leaf_positions.len());
            self.leaf_positions[leaf_idx]
        };
        let mut velocity = Vec3::ZERO;
        let roll_angle = self.rng.random_range(0.0..TAU);
        let roll_strength = self.rng.random_range(0.05..=0.2);
        velocity.x += roll_angle.cos() * roll_strength;
        velocity.z += roll_angle.sin() * roll_strength;

        let wind_factor = self.rng.random_range(0.6..=1.4);
        let gravity_factor = self.rng.random_range(0.8..=1.0);

        // Randomize drift direction for turbulent motion
        let drift_angle = self.rng.random_range(0.0..TAU);
        let drift_direction = Vec3::new(
            drift_angle.cos(),
            self.rng.random_range(-0.2..=0.2),
            drift_angle.sin(),
        );
        let drift_strength = self.rng.random_range(0.3..=0.8);
        let drift_frequency = self.rng.random_range(0.5..=2.0);

        let spawn = ParticleSpawn {
            position: spawn_position,
            velocity,
            color: random_color(&mut self.rng, self.color_low, self.color_high),
            size: self.size,
            lifetime: random_in_range(&mut self.rng, &self.lifetime),
            wind_factor,
            gravity_factor,
            drift_direction,
            drift_strength,
            drift_frequency,
            speed_noise_offset: self.rng.random_range(0.0..10_000.0),
            motion_mode: MotionMode::Falling,
            sink_on_lifetime: true,
            sink_speed: self.rng.random_range(0.08..=0.18),
            texture_variant: 0,
            render_kind: ParticleRenderKind::Leaf,
            despawn_on_lifetime: true,
            despawn_below_ground: true,
            update: LEAF_UPDATE,
        };
        let _ = system.spawn(spawn);
    }

    fn wind_spawn_multiplier(&self, wind: &WindFieldFrame) -> f32 {
        WindResponseCurve {
            min_strength: self.wind_spawn_min_strength,
            max_strength: self.wind_spawn_max_strength,
            power: self.wind_spawn_power,
        }
        .factor(wind.sample_world(self.center).length())
    }
}

impl FallenLeafEmitter {
    pub fn update(&mut self, system: &mut ParticleSystem, dt: f32, wind: &WindFieldFrame) {
        if !self.enabled || self.spawn_rate <= 0.0 {
            return;
        }
        let wind_multiplier = self.wind_spawn_multiplier(wind) * self.fall_chance;
        if wind_multiplier <= 0.0 {
            return;
        }
        let effective_spawn_rate = self.spawn_rate * wind_multiplier;
        self.spawn_accumulator += effective_spawn_rate * dt;
        while self.spawn_accumulator >= 1.0 {
            self.spawn_leaf(system);
            self.spawn_accumulator -= 1.0;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ButterflyEmitterDesc {
    pub enabled: bool,
    pub spawn_rate_per_source: f32,
    pub max_active_butterflies: usize,
    pub height_offset_min: f32,
    pub height_offset_max: f32,
    pub size: f32,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub color_low: Vec4,
    pub color_high: Vec4,
    pub worm_noise_frequency: f32,
    pub worm_noise_detail_frequency: f32,
    pub worm_noise_detail_weight: f32,
    pub flight_variant: ButterflyFlightVariant,
    pub flight_tuning: ButterflyFlightTuning,
}

impl Default for ButterflyEmitterDesc {
    fn default() -> Self {
        Self {
            enabled: true,
            spawn_rate_per_source: 0.000_02,
            max_active_butterflies: 16,
            height_offset_min: 0.06,
            height_offset_max: 0.14,
            size: 0.018,
            lifetime_min: 10.0,
            lifetime_max: 15.0,
            color_low: Vec4::new(0.95, 0.9, 0.55, 1.0),
            color_high: Vec4::new(1.0, 0.97, 0.72, 1.0),
            worm_noise_frequency: 2.0,
            worm_noise_detail_frequency: 8.0,
            worm_noise_detail_weight: 0.5,
            flight_variant: ButterflyFlightVariant::default(),
            flight_tuning: ButterflyFlightTuning::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButterflySpawnSourceKind {
    GroundFlora,
    TreeLeaf,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButterflySpawnSource {
    pub position_ws: Vec3,
    pub kind: ButterflySpawnSourceKind,
}

impl ButterflySpawnSource {
    pub fn ground_flora(position_ws: Vec3) -> Self {
        Self {
            position_ws,
            kind: ButterflySpawnSourceKind::GroundFlora,
        }
    }

    pub fn tree_leaf(position_ws: Vec3) -> Self {
        Self {
            position_ws,
            kind: ButterflySpawnSourceKind::TreeLeaf,
        }
    }
}

#[derive(Debug)]
struct ActiveButterfly {
    handle: ParticleHandle,
    worm_seed: f32,
    worm_phase: f32,
    emergence_target_y: Option<f32>,
    darting_flight: DartingFlightState,
}

pub struct ButterflyEmitter {
    pub height_offset: RangeInclusive<f32>,
    pub size: f32,
    pub lifetime: RangeInclusive<f32>,
    #[allow(dead_code)]
    pub color_low: Vec4,
    #[allow(dead_code)]
    pub color_high: Vec4,
    pub enabled: bool,
    pub spawn_rate_per_source: f32,
    pub max_active_butterflies: usize,
    flight_variant: ButterflyFlightVariant,
    flight_tuning: ButterflyFlightTuning,
    pub worm_noise: FastNoiseLite,
    pub worm_noise_detail: FastNoiseLite,
    pub worm_noise_detail_weight: f32,
    rng: SmallRng,
    spawn_sources: Vec<ButterflySpawnSource>,
    spawn_hazard: f32,
    next_spawn_hazard: f32,
    active_butterflies: Vec<ActiveButterfly>,
    flight_elapsed: f64,
}

impl ButterflyEmitter {
    pub fn new(seed: u64, desc: &ButterflyEmitterDesc) -> Self {
        let mut emitter = Self {
            height_offset: desc.height_offset_min.min(desc.height_offset_max)
                ..=desc.height_offset_max.max(desc.height_offset_min),
            size: desc.size.max(0.001),
            lifetime: desc.lifetime_min.min(desc.lifetime_max)
                ..=desc.lifetime_max.max(desc.lifetime_min),
            color_low: desc.color_low,
            color_high: desc.color_high,
            enabled: desc.enabled,
            spawn_rate_per_source: desc.spawn_rate_per_source.max(0.0),
            max_active_butterflies: desc.max_active_butterflies,
            flight_variant: desc.flight_variant,
            flight_tuning: desc.flight_tuning.sanitized(),
            worm_noise: butterfly_worm_noise_state(seed as i32, desc.worm_noise_frequency),
            worm_noise_detail: butterfly_worm_noise_detail_state(
                (seed as i32).wrapping_add(5000),
                desc.worm_noise_detail_frequency,
            ),
            worm_noise_detail_weight: desc.worm_noise_detail_weight,
            rng: SmallRng::seed_from_u64(seed),
            spawn_sources: Vec::new(),
            spawn_hazard: 0.0,
            next_spawn_hazard: 1.0,
            active_butterflies: Vec::new(),
            flight_elapsed: 0.0,
        };
        emitter.next_spawn_hazard = emitter.sample_next_spawn_hazard();
        emitter
    }

    #[allow(dead_code)]
    pub fn apply_desc(&mut self, desc: &ButterflyEmitterDesc) {
        self.enabled = desc.enabled;
        self.spawn_rate_per_source = desc.spawn_rate_per_source.max(0.0);
        self.max_active_butterflies = desc.max_active_butterflies;
        self.flight_variant = desc.flight_variant;
        self.flight_tuning = desc.flight_tuning.sanitized();
        self.height_offset = desc.height_offset_min.min(desc.height_offset_max)
            ..=desc.height_offset_max.max(desc.height_offset_min);
        self.size = desc.size.max(0.001);
        self.lifetime =
            desc.lifetime_min.min(desc.lifetime_max)..=desc.lifetime_max.max(desc.lifetime_min);
        self.color_low = desc.color_low;
        self.color_high = desc.color_high;
        self.worm_noise
            .set_frequency(Some(desc.worm_noise_frequency.max(0.0001)));
        self.worm_noise_detail
            .set_frequency(Some(desc.worm_noise_detail_frequency.max(0.0001)));
        self.worm_noise_detail_weight = desc.worm_noise_detail_weight;
    }

    fn prune_handles(&mut self, system: &ParticleSystem) {
        self.active_butterflies
            .retain(|butterfly| system.is_alive_handle(butterfly.handle));
    }

    fn sync_active_style(&mut self, system: &mut ParticleSystem) {
        for butterfly in &mut self.active_butterflies {
            let _ = system.set_size(butterfly.handle, self.size);
            if system.set_butterfly_flight_style(
                butterfly.handle,
                self.flight_variant.uses_darting_flight(),
                self.flight_variant.is_darting_block(),
            ) {
                butterfly
                    .darting_flight
                    .resume(system.velocity(butterfly.handle).unwrap_or(Vec3::ZERO));
                if let Some(position) = system.position(butterfly.handle) {
                    butterfly.darting_flight.reset_render_pose(position);
                }
            }
        }
    }

    fn trim_active_to_count(&mut self, system: &mut ParticleSystem, target_count: usize) {
        while self.active_butterflies.len() > target_count {
            if let Some(butterfly) = self.active_butterflies.pop() {
                let _ = system.despawn(butterfly.handle);
            }
        }
    }

    fn sample_next_spawn_hazard(&mut self) -> f32 {
        let unit = 1.0 - self.rng.random_range(0.0..1.0_f32);
        -unit.ln()
    }

    pub fn set_spawn_sources(&mut self, spawn_sources: Vec<ButterflySpawnSource>) {
        self.spawn_sources = spawn_sources;
    }

    pub fn spawn_source_count(&self) -> usize {
        self.spawn_sources.len()
    }

    fn spawn_butterfly(&mut self, system: &mut ParticleSystem) -> Option<ParticleHandle> {
        if self.spawn_sources.is_empty() {
            return None;
        }
        let source = self.spawn_sources[self.rng.random_range(0..self.spawn_sources.len())];
        let height_offset = random_in_range(&mut self.rng, &self.height_offset);
        let emergence_target_y = (source.kind == ButterflySpawnSourceKind::GroundFlora)
            .then_some(source.position_ws.y + STANDARD_PARTICLE_SIZE * 0.5 + height_offset);
        let position = source.position_ws;
        let seed = self.rng.random_range(0.0..100_000.0);
        let phase = self.rng.random_range(0.0..TAU);
        let initial_dir = if emergence_target_y.is_some() {
            Vec3::Y
        } else {
            generate_worm_direction(
                &self.worm_noise,
                &self.worm_noise_detail,
                self.worm_noise_detail_weight,
                seed,
                phase,
            )
        };

        let preset_count = ButterflyPalettePreset::COUNT;
        let texture_variant = if preset_count == 0 {
            0
        } else {
            self.rng.random_range(0..preset_count)
        };

        let lifetime = random_in_range(&mut self.rng, &self.lifetime);
        let block_mode = self.flight_variant.uses_darting_flight();
        let habitat_center = Vec3::new(
            position.x,
            emergence_target_y.unwrap_or(position.y),
            position.z,
        );

        let spawn = ParticleSpawn {
            position,
            velocity: initial_dir * WORM_STEP_LEN,
            color: Vec4::ONE,
            size: self.size,
            lifetime,
            wind_factor: 0.0,
            gravity_factor: 0.0,
            drift_direction: initial_dir,
            drift_strength: 0.0,
            drift_frequency: 1.0,
            speed_noise_offset: seed,
            motion_mode: if block_mode {
                MotionMode::GuidedFlight
            } else {
                MotionMode::Free
            },
            sink_on_lifetime: false,
            sink_speed: 0.0,
            texture_variant,
            render_kind: ParticleRenderKind::Butterfly,
            despawn_on_lifetime: true,
            despawn_below_ground: true,
            update: BUTTERFLY_UPDATE,
        };

        match system.spawn(spawn) {
            Some(handle) => {
                system.set_butterfly_flight_style(
                    handle,
                    block_mode,
                    self.flight_variant.is_darting_block(),
                );
                let mut darting_flight = DartingFlightState::new(seed, habitat_center, initial_dir);
                darting_flight.reset_render_pose(position);
                self.active_butterflies.push(ActiveButterfly {
                    handle,
                    worm_seed: seed,
                    worm_phase: phase,
                    emergence_target_y,
                    darting_flight,
                });
                Some(handle)
            }
            None => None,
        }
    }

    pub fn collect_butterfly_states(
        &mut self,
        system: &ParticleSystem,
        out_handles: &mut Vec<ParticleHandle>,
        out_positions: &mut Vec<Vec3>,
        out_directions: &mut Vec<Vec3>,
        out_emerging: &mut Vec<bool>,
    ) {
        self.prune_handles(system);
        for butterfly in &mut self.active_butterflies {
            if let Some(pos) = system.position(butterfly.handle) {
                let emerging = butterfly
                    .emergence_target_y
                    .is_some_and(|target_y| pos.y < target_y);
                if !emerging {
                    butterfly.emergence_target_y = None;
                }

                out_handles.push(butterfly.handle);
                out_positions.push(pos);
                let dir = if emerging {
                    Vec3::Y
                } else {
                    generate_worm_direction(
                        &self.worm_noise,
                        &self.worm_noise_detail,
                        self.worm_noise_detail_weight,
                        butterfly.worm_seed,
                        butterfly.worm_phase,
                    )
                };
                out_directions.push(dir);
                out_emerging.push(emerging);
            }
        }
    }

    pub fn set_butterfly_state(&mut self, handle: ParticleHandle, position: Vec3, direction: Vec3) {
        if let Some(butterfly) = self
            .active_butterflies
            .iter_mut()
            .find(|butterfly| butterfly.handle == handle)
        {
            if !self.flight_variant.uses_darting_flight() {
                butterfly.worm_phase += WORM_STEP_LEN;
            }
            let _ = (position, direction);
        }
    }

    /// Advances existing live handles only; the spawn hazard and palette RNG stay untouched.
    pub fn advance_block_flight(
        &mut self,
        system: &mut ParticleSystem,
        dt: f32,
        world_max: Vec3,
        wind: &WindFieldFrame,
        mut terrain_distance: impl FnMut(Vec3, Vec3) -> Option<f32>,
    ) {
        if !self.flight_variant.uses_darting_flight() {
            self.flight_elapsed = 0.0;
            return;
        }
        // Bound catch-up after a stall. No giant position step is ever published.
        self.flight_elapsed += f64::from(dt.clamp(0.0, 0.25));
        let step = FLIGHT_STEP_SECONDS;
        while self.flight_elapsed + 1e-9 >= step {
            self.flight_elapsed = (self.flight_elapsed - step).max(0.0);
            for butterfly in &mut self.active_butterflies {
                let Some(position) = system.position(butterfly.handle) else {
                    continue;
                };
                let velocity = system.velocity(butterfly.handle).unwrap_or(Vec3::ZERO);
                let emerging = butterfly.emergence_target_y.is_some_and(|y| position.y < y);
                if !emerging {
                    butterfly.emergence_target_y = None;
                }
                let next_velocity = butterfly.darting_flight.advance(
                    position,
                    velocity,
                    step as f32,
                    world_max,
                    emerging,
                    self.flight_tuning,
                    wind.sample_world(position),
                    &mut terrain_distance,
                );
                system.advance_guided_flight(butterfly.handle, next_velocity, step as f32);
                if let Some(position) = system.position(butterfly.handle) {
                    butterfly.darting_flight.publish_render_pose(position);
                }
            }
        }
        self.prune_handles(system);
    }

    pub const fn flight_variant(&self) -> ButterflyFlightVariant {
        self.flight_variant
    }

    pub fn block_render_position(&self, handle: ParticleHandle) -> Option<Vec3> {
        if !self.flight_variant.uses_darting_flight() {
            return None;
        }
        self.active_butterflies
            .iter()
            .find(|butterfly| butterfly.handle == handle)
            .map(|butterfly| butterfly.darting_flight.render_position())
    }

    pub fn despawn_butterfly(&mut self, handle: ParticleHandle) {
        if let Some(idx) = self
            .active_butterflies
            .iter()
            .position(|butterfly| butterfly.handle == handle)
        {
            self.active_butterflies.swap_remove(idx);
        }
    }
}

impl ParticleEmitter for ButterflyEmitter {
    fn update(&mut self, system: &mut ParticleSystem, dt: f32, _time: f32) {
        self.prune_handles(system);
        if !self.enabled {
            self.trim_active_to_count(system, 0);
            return;
        }

        self.trim_active_to_count(system, self.max_active_butterflies);

        self.sync_active_style(system);
        if self.spawn_sources.is_empty()
            || self.spawn_rate_per_source <= 0.0
            || self.active_butterflies.len() >= self.max_active_butterflies
        {
            return;
        }

        self.spawn_hazard +=
            self.spawn_rate_per_source * self.spawn_sources.len() as f32 * dt.max(0.0);
        while self.spawn_hazard >= self.next_spawn_hazard
            && self.active_butterflies.len() < self.max_active_butterflies
        {
            self.spawn_hazard -= self.next_spawn_hazard;
            self.next_spawn_hazard = self.sample_next_spawn_hazard();
            if self.spawn_butterfly(system).is_none() {
                self.spawn_hazard = 0.0;
                break;
            }
        }
    }
}

// bird emitters and behaviors have been removed; only leaves and butterflies remain

#[cfg(test)]
mod tests {
    use super::*;

    fn butterfly_test_desc() -> ButterflyEmitterDesc {
        ButterflyEmitterDesc {
            flight_variant: ButterflyFlightVariant::OriginalSprite,
            spawn_rate_per_source: 1.0,
            height_offset_min: 0.05,
            height_offset_max: 0.05,
            lifetime_min: 100.0,
            lifetime_max: 100.0,
            ..ButterflyEmitterDesc::default()
        }
    }

    #[test]
    fn butterfly_samples_local_wind_and_zero_gain_preserves_calm_motion() {
        let mut local = WindFieldFrame::uniform(glam::Vec2::ZERO);
        for (i, pair) in local.cells.iter_mut().enumerate() {
            pair[0] = if (i * 2) % 32 == 31 { 2.0 } else { 0.0 };
            pair[2] = if (i * 2 + 1) % 32 == 31 { 2.0 } else { 0.0 };
        }
        let run = |position, wind: &WindFieldFrame, gain| {
            let mut desc = butterfly_test_desc();
            desc.flight_variant = ButterflyFlightVariant::DartingBlock;
            desc.flight_tuning.wind_drift = gain;
            let mut emitter = ButterflyEmitter::new(17, &desc);
            emitter.set_spawn_sources(vec![ButterflySpawnSource::tree_leaf(position)]);
            let mut system = ParticleSystem::new(2);
            let handle = emitter.spawn_butterfly(&mut system).unwrap();
            emitter.advance_block_flight(&mut system, 0.1, Vec3::splat(4.0), wind, |_, _| None);
            (
                system.position(handle).unwrap(),
                system.velocity(handle).unwrap(),
                emitter.rng.random::<u64>(),
            )
        };
        let calm = WindFieldFrame::default();
        let sheltered = Vec3::new(0.5, 1.0, 1.0);
        assert_eq!(run(sheltered, &local, 1.0), run(sheltered, &calm, 1.0));
        let exposed = Vec3::new(1.99, 1.0, 1.0);
        assert_eq!(run(exposed, &local, 0.0), run(exposed, &calm, 0.0));
        let with_wind = run(exposed, &local, 1.0);
        let no_wind = run(exposed, &calm, 1.0);
        assert!(with_wind.0.x > no_wind.0.x + 0.0005);
        assert!(with_wind.1.x > no_wind.1.x + 0.005);
        assert_eq!(
            with_wind.2, no_wind.2,
            "wind must not consume population RNG"
        );
    }

    #[test]
    fn butterfly_ab_retains_handles_palette_lifetime_and_spawn_rng() {
        let mut desc = butterfly_test_desc();
        let mut a = ButterflyEmitter::new(17, &desc);
        desc.flight_variant = ButterflyFlightVariant::DartingBlock;
        let mut b = ButterflyEmitter::new(17, &desc);
        let sources = vec![ButterflySpawnSource::tree_leaf(Vec3::ONE)];
        a.set_spawn_sources(sources.clone());
        b.set_spawn_sources(sources);
        let mut a_system = ParticleSystem::new(16);
        let mut b_system = ParticleSystem::new(16);
        for _ in 0..8 {
            let ah = a.spawn_butterfly(&mut a_system).unwrap();
            let bh = b.spawn_butterfly(&mut b_system).unwrap();
            assert_eq!(ah, bh);
            assert_eq!(a_system.position(ah), b_system.position(bh));
            // Flight RNG must never advance the population/palette RNG.
            b.advance_block_flight(
                &mut b_system,
                0.25,
                Vec3::splat(2.0),
                &WindFieldFrame::default(),
                |_, _| None,
            );
        }
        let mut snaps = Vec::new();
        b_system.write_snapshots(&mut snaps);
        let before = snaps.clone();
        assert!(before
            .iter()
            .all(|s| s.kind == ParticleRenderKind::ButterflyBlock));
        desc.flight_variant = ButterflyFlightVariant::OriginalSprite;
        b.apply_desc(&desc);
        b.sync_active_style(&mut b_system);
        b_system.write_snapshots(&mut snaps);
        for (old, new) in before.iter().zip(&snaps) {
            assert_eq!(old.position_ws, new.position_ws);
            assert_eq!(old.velocity, new.velocity);
            assert_eq!(old.texture_variant, new.texture_variant);
            assert_eq!(old.color.w, new.color.w);
            assert_eq!(new.kind, ParticleRenderKind::Butterfly);
        }
        desc.flight_variant = ButterflyFlightVariant::DartingBlock;
        b.apply_desc(&desc);
        b.sync_active_style(&mut b_system);
        b_system.write_snapshots(&mut snaps);
        for (old, new) in before.iter().zip(&snaps) {
            assert_eq!(old.color, new.color);
            assert_eq!(old.position_ws, new.position_ws);
        }
        let mut a_snaps = Vec::new();
        a_system.write_snapshots(&mut a_snaps);
        assert_eq!(
            a_snaps
                .iter()
                .map(|s| s.texture_variant)
                .collect::<Vec<_>>(),
            before.iter().map(|s| s.texture_variant).collect::<Vec<_>>()
        );
        assert_eq!(a.rng.random::<u64>(), b.rng.random::<u64>());
    }

    #[test]
    fn butterfly_appearance_switch_preserves_motion_and_visible_area() {
        let atlas = image::open("assets/texture/butterfly_16px/butterfly.png")
            .unwrap()
            .to_rgba8();
        let mut opaque_pixels = 0;
        for row in super::super::BUTTERFLY_ATLAS_ROW_FOR_VIEW {
            for y in row * 16..(row + 1) * 16 {
                for x in 0..80 {
                    opaque_pixels += usize::from(atlas.get_pixel(x, y)[3] > 0);
                }
            }
        }
        let mean_coverage = opaque_pixels as f32 / (2.0 * 5.0 * 16.0 * 16.0);
        let mut desc = butterfly_test_desc();
        desc.flight_variant = ButterflyFlightVariant::DartingSprite;
        let mut reference = ButterflyEmitter::new(82, &desc);
        let mut switched = ButterflyEmitter::new(82, &desc);
        let sources = vec![ButterflySpawnSource::tree_leaf(Vec3::ONE)];
        reference.set_spawn_sources(sources.clone());
        switched.set_spawn_sources(sources);
        let mut reference_system = ParticleSystem::new(4);
        let mut switched_system = ParticleSystem::new(4);
        let handle = reference.spawn_butterfly(&mut reference_system).unwrap();
        assert_eq!(
            handle,
            switched.spawn_butterfly(&mut switched_system).unwrap()
        );
        let mut frames = std::collections::BTreeSet::new();
        for step in 0..360 {
            if step == 80 || step == 200 {
                desc.flight_variant = if step == 80 {
                    ButterflyFlightVariant::DartingBlock
                } else {
                    ButterflyFlightVariant::DartingSprite
                };
                switched.apply_desc(&desc);
                switched.sync_active_style(&mut switched_system);
            }
            for (emitter, system) in [
                (&mut reference, &mut reference_system),
                (&mut switched, &mut switched_system),
            ] {
                emitter.advance_block_flight(
                    system,
                    1.0 / 120.0,
                    Vec3::splat(4.0),
                    &WindFieldFrame::default(),
                    |_, _| None,
                );
            }
            let mut a = Vec::new();
            let mut b = Vec::new();
            reference_system.write_snapshots_with_block_pose(&mut a, |h, p| {
                reference.block_render_position(h).unwrap_or(p)
            });
            switched_system.write_snapshots_with_block_pose(&mut b, |h, p| {
                switched.block_render_position(h).unwrap_or(p)
            });
            assert_eq!(a.len(), 1);
            assert_eq!(b.len(), 1);
            let (a, b) = (&a[0], &b[0]);
            assert_eq!(
                a.position_ws, b.position_ws,
                "appearance must not restart the display beat"
            );
            assert_eq!(a.velocity, b.velocity);
            if (80..200).contains(&step) {
                assert_eq!(b.size, super::super::STANDARD_PARTICLE_SIZE);
                let area_ratio = mean_coverage * (a.size / b.size).powi(2);
                assert!(
                    (0.85..=1.15).contains(&area_ratio),
                    "sprite/block mean visible area ratio {area_ratio}, atlas coverage {mean_coverage}"
                );
            } else {
                assert_eq!(a.size, b.size);
            }
            assert_eq!(a.texture_variant, b.texture_variant);
            assert_eq!(a.animation_frame_offset, b.animation_frame_offset);
            assert_eq!(a.color.w, b.color.w);
            assert_eq!(a.kind, ParticleRenderKind::Butterfly);
            assert_eq!(
                b.kind,
                if (80..200).contains(&step) {
                    ParticleRenderKind::ButterflyBlock
                } else {
                    ParticleRenderKind::Butterfly
                }
            );
            frames.insert(a.animation_frame_offset);
        }
        assert!(
            frames.len() > 1,
            "sprite animation must still advance at the compact size"
        );
    }

    #[test]
    fn butterfly_block_fixed_steps_ignore_render_partition_and_world_tick() {
        let run = |frame_dt: f32, world_tick: f32| {
            let mut desc = butterfly_test_desc();
            desc.flight_variant = ButterflyFlightVariant::DartingBlock;
            let mut emitter = ButterflyEmitter::new(19, &desc);
            emitter.set_spawn_sources(vec![ButterflySpawnSource::tree_leaf(Vec3::ONE)]);
            let mut system = ParticleSystem::new(4);
            system.set_bucket_step_seconds(world_tick);
            let handle = emitter.spawn_butterfly(&mut system).unwrap();
            for _ in 0..(4.0 / frame_dt).round() as usize {
                system.update(frame_dt, super::super::ParticleForces::default());
                emitter.advance_block_flight(
                    &mut system,
                    frame_dt,
                    Vec3::splat(2.0),
                    &WindFieldFrame::default(),
                    |_, _| None,
                );
            }
            (
                system.position(handle).unwrap(),
                system.velocity(handle).unwrap(),
                emitter.block_render_position(handle).unwrap(),
            )
        };
        let reference = run(1.0 / 120.0, 0.05);
        for (dt, tick) in [(1.0 / 60.0, 0.1), (1.0 / 30.0, 0.02), (1.0 / 20.0, 0.05)] {
            let actual = run(dt, tick);
            assert!(actual.0.distance(reference.0) < 1e-6);
            assert!(actual.1.distance(reference.1) < 1e-6);
            assert!(actual.2.distance(reference.2) < 1e-6);
        }
    }

    #[test]
    fn butterfly_100ms_shared_beats_publish_actual_integrated_positions() {
        let mut desc = butterfly_test_desc();
        desc.flight_variant = ButterflyFlightVariant::DartingBlock;
        desc.flight_tuning.flight_frequency_hz = 10.0;
        let mut emitter = ButterflyEmitter::new(23, &desc);
        emitter.set_spawn_sources(vec![ButterflySpawnSource::tree_leaf(Vec3::ONE)]);
        let mut system = ParticleSystem::new(4);
        let handle = emitter.spawn_butterfly(&mut system).unwrap();
        for tick in 0..240 {
            let previous = emitter.block_render_position(handle).unwrap();
            emitter.advance_block_flight(
                &mut system,
                FLIGHT_STEP_SECONDS as f32,
                Vec3::splat(2.0),
                &WindFieldFrame::default(),
                |_, _| None,
            );
            let expected = if tick % 12 == 0 {
                system.position(handle).unwrap()
            } else {
                previous
            };
            assert_eq!(
                emitter.block_render_position(handle).unwrap(),
                expected,
                "tick={tick}"
            );
        }
    }

    #[test]
    fn butterfly_block_lifetime_and_disable_retire_the_existing_slots() {
        let mut desc = butterfly_test_desc();
        desc.flight_variant = ButterflyFlightVariant::DartingBlock;
        desc.lifetime_min = 0.2;
        desc.lifetime_max = 0.2;
        let mut emitter = ButterflyEmitter::new(7, &desc);
        emitter.set_spawn_sources(vec![ButterflySpawnSource::tree_leaf(Vec3::ONE)]);
        let mut system = ParticleSystem::new(4);
        let first = emitter.spawn_butterfly(&mut system).unwrap();
        emitter.advance_block_flight(
            &mut system,
            0.25,
            Vec3::splat(2.0),
            &WindFieldFrame::default(),
            |_, _| None,
        );
        assert!(!system.is_alive_handle(first));
        let second = emitter.spawn_butterfly(&mut system).unwrap();
        emitter.enabled = false;
        emitter.update(&mut system, 0.0, 0.0);
        assert!(!system.is_alive_handle(second));
        assert_eq!(system.available_capacity(), 4);
        assert!(emitter.active_butterflies.is_empty());
    }

    #[test]
    fn butterfly_spawn_hazard_scales_with_source_count() {
        let source = ButterflySpawnSource::tree_leaf(Vec3::new(0.5, 0.5, 0.5));

        let mut one_source_system = ParticleSystem::new(4);
        let mut one_source = ButterflyEmitter::new(7, &butterfly_test_desc());
        one_source.set_spawn_sources(vec![source]);
        one_source.next_spawn_hazard = 0.5;
        one_source.update(&mut one_source_system, 0.25, 0.0);

        let mut two_source_system = ParticleSystem::new(4);
        let mut two_sources = ButterflyEmitter::new(7, &butterfly_test_desc());
        two_sources.set_spawn_sources(vec![source, source]);
        two_sources.next_spawn_hazard = 0.5;
        two_sources.update(&mut two_source_system, 0.25, 0.0);

        assert_eq!(one_source_system.alive_count(), 0);
        assert_eq!(two_source_system.alive_count(), 1);
    }

    #[test]
    fn butterfly_hard_limit_stops_many_same_frame_spawn_attempts() {
        let source = ButterflySpawnSource::tree_leaf(Vec3::new(0.5, 0.5, 0.5));
        let mut desc = butterfly_test_desc();
        desc.spawn_rate_per_source = 100.0;
        desc.max_active_butterflies = 3;
        let mut system = ParticleSystem::new(32);
        let mut emitter = ButterflyEmitter::new(17, &desc);
        emitter.set_spawn_sources(vec![source; 20]);
        emitter.next_spawn_hazard = 0.01;

        emitter.update(&mut system, 1.0, 0.0);

        assert_eq!(system.alive_count(), 3);
    }

    #[test]
    fn existing_butterflies_count_toward_limit_and_death_reopens_one_slot() {
        let source = ButterflySpawnSource::tree_leaf(Vec3::new(0.5, 0.5, 0.5));
        let mut desc = butterfly_test_desc();
        desc.spawn_rate_per_source = 100.0;
        desc.max_active_butterflies = 2;
        let mut system = ParticleSystem::new(8);
        let mut emitter = ButterflyEmitter::new(19, &desc);
        emitter.set_spawn_sources(vec![source; 4]);
        let first = emitter.spawn_butterfly(&mut system).unwrap();
        emitter.spawn_butterfly(&mut system).unwrap();
        emitter.next_spawn_hazard = 0.01;

        emitter.update(&mut system, 1.0, 0.0);
        assert_eq!(system.alive_count(), 2);

        assert!(system.despawn(first));
        emitter.update(&mut system, 1.0, 0.0);
        assert_eq!(system.alive_count(), 2);
    }

    #[test]
    fn shared_particle_capacity_stops_butterfly_spawning_without_exceeding_the_limit() {
        let source = ButterflySpawnSource::tree_leaf(Vec3::new(0.5, 0.5, 0.5));
        let mut desc = butterfly_test_desc();
        desc.spawn_rate_per_source = 100.0;
        desc.max_active_butterflies = 2;
        let mut system = ParticleSystem::new(1);
        let mut emitter = ButterflyEmitter::new(29, &desc);
        emitter.set_spawn_sources(vec![source; 4]);
        emitter.spawn_butterfly(&mut system).unwrap();
        emitter.next_spawn_hazard = 0.01;

        emitter.update(&mut system, 1.0, 0.0);

        assert_eq!(system.alive_count(), 1);
    }

    #[test]
    fn lowering_butterfly_limit_removes_existing_excess_immediately() {
        let source = ButterflySpawnSource::tree_leaf(Vec3::new(0.5, 0.5, 0.5));
        let mut desc = butterfly_test_desc();
        desc.max_active_butterflies = 3;
        let mut system = ParticleSystem::new(8);
        let mut emitter = ButterflyEmitter::new(23, &desc);
        emitter.set_spawn_sources(vec![source]);
        for _ in 0..3 {
            emitter.spawn_butterfly(&mut system).unwrap();
        }

        desc.max_active_butterflies = 1;
        emitter.apply_desc(&desc);
        emitter.update(&mut system, 0.0, 0.0);

        assert_eq!(system.alive_count(), 1);
    }

    #[test]
    fn ground_flora_butterflies_start_below_the_plant_and_emerge_upward() {
        let source_position = Vec3::new(0.5, 0.4, 0.5);
        let mut system = ParticleSystem::new(4);
        let mut emitter = ButterflyEmitter::new(11, &butterfly_test_desc());
        emitter.set_spawn_sources(vec![ButterflySpawnSource::ground_flora(source_position)]);
        let handle = emitter.spawn_butterfly(&mut system).unwrap();

        let mut handles = Vec::new();
        let mut positions = Vec::new();
        let mut directions = Vec::new();
        let mut emerging = Vec::new();
        emitter.collect_butterfly_states(
            &system,
            &mut handles,
            &mut positions,
            &mut directions,
            &mut emerging,
        );

        assert_eq!(system.position(handle), Some(source_position));
        assert_eq!(directions, vec![Vec3::Y]);
        assert_eq!(emerging, vec![true]);
    }

    #[test]
    fn tree_leaf_butterflies_start_inside_the_selected_leaf_voxel() {
        let source_position = Vec3::new(0.8, 0.9, 1.1);
        let mut system = ParticleSystem::new(4);
        let mut emitter = ButterflyEmitter::new(13, &butterfly_test_desc());
        emitter.set_spawn_sources(vec![ButterflySpawnSource::tree_leaf(source_position)]);
        let handle = emitter.spawn_butterfly(&mut system).unwrap();

        let mut handles = Vec::new();
        let mut positions = Vec::new();
        let mut directions = Vec::new();
        let mut emerging = Vec::new();
        emitter.collect_butterfly_states(
            &system,
            &mut handles,
            &mut positions,
            &mut directions,
            &mut emerging,
        );

        assert_eq!(system.position(handle), Some(source_position));
        assert_eq!(emerging, vec![false]);
    }

    #[test]
    fn disabling_fallen_leaf_emitter_preserves_existing_particles() {
        let mut system = ParticleSystem::new(4);
        let mut emitter =
            FallenLeafEmitter::new(Vec3::ZERO, Vec::new(), 1, &LeafEmitterDesc::default());
        emitter.spawn_leaf(&mut system);

        emitter.enabled = false;
        emitter.update(&mut system, 10.0, &WindFieldFrame::default());

        assert_eq!(system.alive_count(), 1);
    }

    #[test]
    fn leaf_emission_uses_the_supplied_spatial_wind_and_stops_when_it_is_calm() {
        let mut system = ParticleSystem::new(16);
        let mut emitter =
            FallenLeafEmitter::new(Vec3::ZERO, Vec::new(), 1, &LeafEmitterDesc::default());
        emitter.spawn_rate = 2.;
        emitter.fall_chance = 1.;
        let mut wind = WindFieldFrame::uniform(glam::Vec2::X);
        // Only the far X edge has wind; the tree starts outside that edge.
        for (i, pair) in wind.cells.iter_mut().enumerate() {
            pair[0] = if (i * 2) % 32 == 31 { 1. } else { 0. };
            pair[2] = if (i * 2 + 1) % 32 == 31 { 1. } else { 0. };
        }
        emitter.update(&mut system, 1., &wind);
        assert_eq!(system.alive_count(), 0);
        emitter.center = Vec3::new(2., 0., 0.);
        emitter.update(&mut system, 1., &wind);
        assert_eq!(system.alive_count(), 2);
        emitter.update(&mut system, 1., &WindFieldFrame::default());
        assert_eq!(system.alive_count(), 2);
    }
    #[test]
    fn butterfly_shared_cadence_changes_motion_but_preserves_population_and_palette() {
        let mut desc = butterfly_test_desc();
        desc.flight_variant = ButterflyFlightVariant::DartingBlock;
        let mut stepped = ButterflyEmitter::new(23, &desc);
        desc.flight_tuning.flight_frequency_hz = 0.0;
        let mut continuous = ButterflyEmitter::new(23, &desc);
        let mut a = ParticleSystem::new(4);
        let mut b = ParticleSystem::new(4);
        for emitter in [&mut stepped, &mut continuous] {
            emitter.set_spawn_sources(vec![ButterflySpawnSource::tree_leaf(Vec3::ONE)]);
        }
        let ah = stepped.spawn_butterfly(&mut a).unwrap();
        let bh = continuous.spawn_butterfly(&mut b).unwrap();
        let mut snapshots = Vec::new();
        let mut held_frames = 0;
        let mut previous = Vec3::ONE;
        for _ in 0..120 {
            stepped.advance_block_flight(
                &mut a,
                1.0 / 60.0,
                Vec3::splat(2.0),
                &WindFieldFrame::default(),
                |_, _| None,
            );
            continuous.advance_block_flight(
                &mut b,
                1.0 / 60.0,
                Vec3::splat(2.0),
                &WindFieldFrame::default(),
                |_, _| None,
            );
            assert_eq!(continuous.block_render_position(bh), b.position(bh));
            a.write_snapshots_with_block_pose(&mut snapshots, |handle, position| {
                stepped.block_render_position(handle).unwrap_or(position)
            });
            let displayed = snapshots[0].position_ws;
            held_frames += usize::from(displayed == previous);
            previous = displayed;
        }
        assert!(held_frames > 50);
        assert_ne!(a.position(ah), b.position(bh));
        let mut b_snapshots = Vec::new();
        b.write_snapshots(&mut b_snapshots);
        assert_eq!(snapshots[0].texture_variant, b_snapshots[0].texture_variant);
        assert_eq!(snapshots[0].color, b_snapshots[0].color);
        let (position, velocity) = (a.position(ah), a.velocity(ah));
        desc.flight_tuning.vertical_strength = 4.0;
        stepped.apply_desc(&desc);
        stepped.sync_active_style(&mut a);
        assert_eq!((a.position(ah), a.velocity(ah)), (position, velocity));
        assert_eq!(a.alive_count(), b.alive_count());
        assert_eq!(stepped.rng.random::<u64>(), continuous.rng.random::<u64>());
        desc.flight_variant = ButterflyFlightVariant::OriginalSprite;
        stepped.apply_desc(&desc);
        stepped.sync_active_style(&mut a);
        a.write_snapshots_with_block_pose(&mut snapshots, |_, _| {
            panic!("A must not use held B pose")
        });
        assert_eq!(snapshots[0].position_ws, a.position(ah).unwrap());
    }
}
