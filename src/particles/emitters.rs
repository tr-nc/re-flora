use std::{f32::consts::TAU, ops::RangeInclusive};

use fastnoise_lite::{FastNoiseLite, NoiseType};
use glam::{Vec3, Vec4};
use rand::{rngs::SmallRng, RngExt, SeedableRng};

use super::{MotionMode, ParticleHandle, ParticleRenderKind, ParticleSpawn, ParticleSystem};
use crate::tracer::ButterflyPalettePreset;
use crate::wind::{Wind, WindResponseCurve};

pub const WORM_STEP_LEN: f32 = 0.15;

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
            size: 1.0 / 256.0,
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
    wind: Wind,
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
            wind: Wind::new(),
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
        };
        let _ = system.spawn(spawn);
    }

    fn wind_spawn_multiplier(&self, time: f32) -> f32 {
        self.wind.sample_response(
            self.center,
            time,
            WindResponseCurve {
                min_strength: self.wind_spawn_min_strength,
                max_strength: self.wind_spawn_max_strength,
                power: self.wind_spawn_power,
            },
        )
    }
}

impl ParticleEmitter for FallenLeafEmitter {
    fn update(&mut self, system: &mut ParticleSystem, dt: f32, time: f32) {
        if self.spawn_rate <= 0.0 {
            return;
        }
        let wind_multiplier = self.wind_spawn_multiplier(time) * self.fall_chance;
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
    pub butterfly_count: u32,
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
}

impl Default for ButterflyEmitterDesc {
    fn default() -> Self {
        Self {
            enabled: true,
            butterfly_count: 128,
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
        }
    }
}

pub struct ButterflyEmitter {
    pub center: Vec3,
    pub map_extent: Vec3,
    pub height_offset: RangeInclusive<f32>,
    pub size: f32,
    pub lifetime: RangeInclusive<f32>,
    #[allow(dead_code)]
    pub color_low: Vec4,
    #[allow(dead_code)]
    pub color_high: Vec4,
    pub enabled: bool,
    pub butterfly_count: u32,
    render_kind: ParticleRenderKind,
    pub worm_noise: FastNoiseLite,
    pub worm_noise_detail: FastNoiseLite,
    pub worm_noise_detail_weight: f32,
    rng: SmallRng,
    active_handles: Vec<ParticleHandle>,
    pending_placement_handles: Vec<ParticleHandle>,
    worm_seeds: Vec<f32>,
    worm_phases: Vec<f32>,
}

impl ButterflyEmitter {
    pub fn new(center: Vec3, extent: Vec3, seed: u64, desc: &ButterflyEmitterDesc) -> Self {
        Self::new_with_render_kind(center, extent, seed, desc, ParticleRenderKind::Butterfly)
    }

    fn new_with_render_kind(
        center: Vec3,
        extent: Vec3,
        seed: u64,
        desc: &ButterflyEmitterDesc,
        render_kind: ParticleRenderKind,
    ) -> Self {
        Self {
            center,
            map_extent: extent,
            height_offset: desc.height_offset_min.min(desc.height_offset_max)
                ..=desc.height_offset_max.max(desc.height_offset_min),
            size: desc.size.max(0.001),
            lifetime: desc.lifetime_min.min(desc.lifetime_max)
                ..=desc.lifetime_max.max(desc.lifetime_min),
            color_low: desc.color_low,
            color_high: desc.color_high,
            enabled: desc.enabled,
            butterfly_count: desc.butterfly_count,
            render_kind,
            worm_noise: butterfly_worm_noise_state(seed as i32, desc.worm_noise_frequency),
            worm_noise_detail: butterfly_worm_noise_detail_state(
                (seed as i32).wrapping_add(5000),
                desc.worm_noise_detail_frequency,
            ),
            worm_noise_detail_weight: desc.worm_noise_detail_weight,
            rng: SmallRng::seed_from_u64(seed),
            active_handles: Vec::new(),
            pending_placement_handles: Vec::new(),
            worm_seeds: Vec::new(),
            worm_phases: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn apply_desc(&mut self, desc: &ButterflyEmitterDesc) {
        self.enabled = desc.enabled;
        self.butterfly_count = desc.butterfly_count;
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
        let old_len = self.active_handles.len();
        self.active_handles
            .retain(|handle| system.is_alive_handle(*handle));
        let removed = old_len - self.active_handles.len();
        if removed > 0 {
            self.worm_seeds.resize(self.active_handles.len(), 0.0);
            self.worm_phases.resize(self.active_handles.len(), 0.0);
        }
    }

    fn enforce_size_on_active(&self, system: &mut ParticleSystem) {
        for handle in &self.active_handles {
            let _ = system.set_size(*handle, self.size);
        }
    }

    fn trim_active_to_count(&mut self, system: &mut ParticleSystem, target_count: usize) {
        while self.active_handles.len() > target_count {
            if let Some(handle) = self.active_handles.pop() {
                self.pending_placement_handles.retain(|h| *h != handle);
                self.worm_seeds.pop();
                self.worm_phases.pop();
                let _ = system.despawn(handle);
            }
        }
    }

    pub fn random_spawn_position_candidate(&mut self) -> Vec3 {
        let x = self
            .rng
            .random_range(-self.map_extent.x..=self.map_extent.x);
        let z = self
            .rng
            .random_range(-self.map_extent.z..=self.map_extent.z);
        let height_offset = random_in_range(&mut self.rng, &self.height_offset);

        Vec3::new(
            self.center.x + x,
            self.center.y + height_offset,
            self.center.z + z,
        )
    }

    pub fn spawn_butterfly(&mut self, system: &mut ParticleSystem) -> Option<ParticleHandle> {
        let position = self.random_spawn_position_candidate();
        let seed = self.rng.random_range(0.0..100_000.0);
        let phase = self.rng.random_range(0.0..TAU);
        let initial_dir = generate_worm_direction(
            &self.worm_noise,
            &self.worm_noise_detail,
            self.worm_noise_detail_weight,
            seed,
            phase,
        );

        let preset_count = ButterflyPalettePreset::COUNT;
        let texture_variant = if preset_count == 0 {
            0
        } else {
            self.rng.random_range(0..preset_count)
        };

        let lifetime = random_in_range(&mut self.rng, &self.lifetime);

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
            motion_mode: MotionMode::Free,
            sink_on_lifetime: false,
            sink_speed: 0.0,
            texture_variant,
            render_kind: self.render_kind,
            despawn_on_lifetime: true,
            despawn_below_ground: true,
        };

        match system.spawn(spawn) {
            Some(handle) => {
                self.worm_seeds.push(seed);
                self.worm_phases.push(phase);
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
    ) {
        self.prune_handles(system);
        for (i, handle) in self.active_handles.iter().enumerate() {
            if let Some(pos) = system.position(*handle) {
                out_handles.push(*handle);
                out_positions.push(pos);
                let dir = generate_worm_direction(
                    &self.worm_noise,
                    &self.worm_noise_detail,
                    self.worm_noise_detail_weight,
                    self.worm_seeds[i],
                    self.worm_phases[i],
                );
                out_directions.push(dir);
            }
        }
    }

    pub fn set_butterfly_state(&mut self, handle: ParticleHandle, position: Vec3, direction: Vec3) {
        if let Some(idx) = self.active_handles.iter().position(|h| *h == handle) {
            self.worm_phases[idx] += WORM_STEP_LEN;
            let _ = (position, direction);
        }
    }

    pub fn despawn_butterfly(&mut self, handle: ParticleHandle) {
        self.pending_placement_handles.retain(|h| *h != handle);
        if let Some(idx) = self.active_handles.iter().position(|h| *h == handle) {
            self.active_handles.swap_remove(idx);
            self.worm_seeds.swap_remove(idx);
            self.worm_phases.swap_remove(idx);
        }
    }

    pub fn drain_pending_placement_handles(&mut self) -> Vec<ParticleHandle> {
        std::mem::take(&mut self.pending_placement_handles)
    }
}

impl ParticleEmitter for ButterflyEmitter {
    fn update(&mut self, system: &mut ParticleSystem, _dt: f32, _time: f32) {
        self.prune_handles(system);
        let target_count = if self.enabled {
            self.butterfly_count as usize
        } else {
            0
        };
        self.trim_active_to_count(system, target_count);
        if target_count == 0 {
            return;
        }

        self.enforce_size_on_active(system);

        while self.active_handles.len() < target_count {
            if let Some(handle) = self.spawn_butterfly(system) {
                self.active_handles.push(handle);
                self.pending_placement_handles.push(handle);
            } else {
                break;
            }
        }
    }
}

// bird emitters and behaviors have been removed; only leaves and butterflies remain
