use fastnoise_lite::{FastNoiseLite, NoiseType};
use glam::{Quat, Vec3, Vec4};

use super::animation::{BUTTERFLY_ANIM_FRAME_DURATION_SEC, BUTTERFLY_FRAMES_PER_VARIANT};
use super::leaf_flight::LeafFlight;
use crate::wind_field::WindFieldFrame;

/// Default maximum particle capacity shared between the CPU simulation and GPU buffer.
pub const PARTICLE_CAPACITY: usize = 16_384;
/// Standard world-space particle quad size, matching one terrain voxel.
pub const STANDARD_PARTICLE_SIZE: f32 = 1.0 / 256.0;
/// The two active atlas rows average 58 opaque pixels per 16x16 frame.
/// A fixed sqrt(256 / 58) scale matches a block's mean visible area without
/// cancelling wingbeats through per-frame resizing. Presentation only.
const BUTTERFLY_SPRITE_SIZE_COMPENSATION: f32 = 2.1;
pub const PARTICLE_UPDATE_BUCKET_COUNT: usize = 2;

#[derive(Clone, Copy, Debug)]
pub struct ParticleUpdateConfig {
    /// Time between physics updates for each particle.
    pub interval_seconds: f32,
    /// Number of phase buckets used to spread those updates over the interval.
    pub bucket_count: u32,
}

impl ParticleUpdateConfig {
    pub const fn new(interval_seconds: f32, bucket_count: u32) -> Self {
        Self {
            interval_seconds,
            bucket_count,
        }
    }
}

impl Default for ParticleUpdateConfig {
    fn default() -> Self {
        Self::new(0.1, 2)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParticleTickStep {
    pub did_step: bool,
    pub active_bucket: u32,
    pub step_seconds: f32,
    pub bucket_count: u32,
}

/// Handle that uniquely identifies a live particle.
/// Internally, it keeps track of the slot index and a generation counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ParticleHandle {
    index: u32,
    generation: u32,
}

impl ParticleHandle {
    #[allow(dead_code)]
    pub const fn invalid() -> Self {
        Self {
            index: u32::MAX,
            generation: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionMode {
    /// Coupled flight for leaves; legacy noise/gravity for other falling particles.
    Falling,
    /// Free-flight particles that keep their velocity, only damped over time.
    Free,
    /// Butterfly B: an emitter guides fixed substeps; the shared lifecycle still owns the slot.
    GuidedFlight,
}

/// Parameters used when spawning a new particle.
#[derive(Clone, Copy, Debug)]
pub struct ParticleSpawn {
    pub position: Vec3,
    pub velocity: Vec3,
    pub color: Vec4,
    pub size: f32,
    pub lifetime: f32,
    pub wind_factor: f32,
    pub gravity_factor: f32,
    /// Random drift direction for turbulent motion
    pub drift_direction: Vec3,
    /// Strength of the drift/turbulence
    pub drift_strength: f32,
    /// How quickly the drift changes over time
    pub drift_frequency: f32,
    /// Per-particle offset for the Perlin speed sampling to decorrelate leaves
    pub speed_noise_offset: f32,
    /// Motion integration mode for this particle.
    pub motion_mode: MotionMode,
    /// If true, particle transitions to a sinking phase when lifetime elapses.
    pub sink_on_lifetime: bool,
    /// Downward speed used during the sinking phase.
    pub sink_speed: f32,
    /// Optional texture variant for render-time atlas selection.
    pub texture_variant: u32,
    /// Render classification used by the particle texture LUT.
    pub render_kind: ParticleRenderKind,
    /// If false, particle lifetime does not trigger automatic despawn.
    pub despawn_on_lifetime: bool,
    /// If false, particle can go below y=0 without automatic despawn.
    pub despawn_below_ground: bool,
    /// Per-particle-type physics cadence and update spreading.
    pub update: ParticleUpdateConfig,
}

impl Default for ParticleSpawn {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            color: Vec4::ONE,
            size: 1.0,
            lifetime: 1.0,
            wind_factor: 1.0,
            gravity_factor: 1.0,
            drift_direction: Vec3::ZERO,
            drift_strength: 0.0,
            drift_frequency: 1.0,
            speed_noise_offset: 0.0,
            motion_mode: MotionMode::Falling,
            sink_on_lifetime: false,
            sink_speed: 0.1,
            texture_variant: 0,
            render_kind: ParticleRenderKind::Leaf,
            despawn_on_lifetime: true,
            despawn_below_ground: true,
            update: ParticleUpdateConfig::default(),
        }
    }
}

/// Parameters driving the global forces applied during simulation.
#[derive(Clone, Copy, Debug)]
pub struct SpeedNoise {
    /// Frequency of the Perlin sampling along time.
    pub frequency: f32,
    /// Minimum downward speed (positive value) mapped from noise.
    pub min_speed: f32,
    /// Maximum downward speed (positive value) mapped from noise.
    pub max_speed: f32,
}

impl Default for SpeedNoise {
    fn default() -> Self {
        Self {
            min_speed: -0.05,
            max_speed: 0.14,
            frequency: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParticleForces {
    /// Linear damping factor (0..1). Use small values to avoid instability.
    pub linear_damping: f32,
    /// Perlin-driven speed profile for non-leaf falling particles.
    pub speed_noise: SpeedNoise,
    /// Multiplier for planar velocity on falling particles.
    pub leaf_planar_speed_multiplier: f32,
}

impl Default for ParticleForces {
    fn default() -> Self {
        Self {
            linear_damping: 0.0,
            speed_noise: SpeedNoise::default(),
            leaf_planar_speed_multiplier: 0.23,
        }
    }
}

/// A lightweight copy of particle data used by the renderer.
#[derive(Clone, Copy, Debug)]
pub struct ParticleSnapshot {
    pub position_ws: Vec3,
    pub velocity: Vec3,
    pub color: Vec4,
    pub size: f32,
    pub kind: ParticleRenderKind,
    pub texture_variant: u32,
    pub animation_frame_offset: u32,
    /// Stable per-life phase seed for render-only articulated wing animation.
    pub animation_phase_offset: f32,
    /// Held simulation orientation for falling-leaf optics. Geometry stays screen-facing.
    /// None for other kinds/motion modes, which retain their existing optical inputs.
    pub leaf_orientation: Option<Quat>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleRenderKind {
    Leaf,
    Butterfly,
    ButterflyBlock,
    WaterDroplet,
    TerrainVoxel,
}

/// Sample-and-hold presentation, never an input to particle mechanics.
/// Position, velocity and optical orientation publish together.
#[derive(Clone, Copy, Debug)]
struct LeafDisplayPose {
    position: Vec3,
    velocity: Vec3,
    orientation: Quat,
}

/// Keeps particle data in a struct-of-arrays layout for cache-friendly updates.
pub struct ParticleSystem {
    positions: Vec<Vec3>,
    velocities: Vec<Vec3>,
    colors: Vec<Vec4>,
    sizes: Vec<f32>,
    wind_factors: Vec<f32>,
    gravity_factors: Vec<f32>,
    drift_directions: Vec<Vec3>,
    drift_strengths: Vec<f32>,
    drift_frequencies: Vec<f32>,
    lifetimes: Vec<f32>,
    ages: Vec<f32>,
    generations: Vec<u32>,
    motion_modes: Vec<MotionMode>,
    sink_on_lifetime: Vec<bool>,
    sink_speeds: Vec<f32>,
    is_sinking: Vec<bool>,
    is_alive: Vec<bool>,
    alive_indices: Vec<usize>,
    free_list: Vec<usize>,
    max_particles: usize,
    speed_noise_offsets: Vec<f32>,
    texture_variants: Vec<u32>,
    butterfly_block_appearance: Vec<bool>,
    animation_elapsed: Vec<f32>,
    animation_frame_offsets: Vec<u32>,
    render_kinds: Vec<ParticleRenderKind>,
    despawn_on_lifetime: Vec<bool>,
    despawn_below_ground: Vec<bool>,
    update_buckets: Vec<u32>,
    update_intervals: Vec<f32>,
    update_bucket_counts: Vec<u32>,
    update_elapsed: Vec<f32>,
    pending_sim_dt: Vec<f32>,
    update_bucket_phase: u32,
    update_bucket_elapsed: f32,
    update_bucket_step_seconds: f32,
    last_tick_step: ParticleTickStep,
    speed_noise: FastNoiseLite,
    leaf_flight: Vec<LeafFlight>,
    leaf_display: Vec<LeafDisplayPose>,
}

impl ParticleSystem {
    pub fn new(max_particles: usize) -> Self {
        assert!(max_particles > 0, "ParticleSystem needs capacity > 0");
        let zero_vec3 = Vec3::ZERO;
        let zero_vec4 = Vec4::ZERO;
        let mut free_list = Vec::with_capacity(max_particles);
        for idx in (0..max_particles).rev() {
            free_list.push(idx);
        }

        let mut speed_noise = FastNoiseLite::with_seed(1337);
        speed_noise.set_noise_type(Some(NoiseType::Perlin));
        speed_noise.set_frequency(Some(SpeedNoise::default().frequency));

        Self {
            positions: vec![zero_vec3; max_particles],
            velocities: vec![zero_vec3; max_particles],
            colors: vec![zero_vec4; max_particles],
            sizes: vec![0.0; max_particles],
            wind_factors: vec![1.0; max_particles],
            gravity_factors: vec![1.0; max_particles],
            drift_directions: vec![zero_vec3; max_particles],
            drift_strengths: vec![0.0; max_particles],
            drift_frequencies: vec![1.0; max_particles],
            lifetimes: vec![0.0; max_particles],
            ages: vec![0.0; max_particles],
            generations: vec![0; max_particles],
            motion_modes: vec![MotionMode::Falling; max_particles],
            sink_on_lifetime: vec![false; max_particles],
            sink_speeds: vec![0.1; max_particles],
            is_sinking: vec![false; max_particles],
            is_alive: vec![false; max_particles],
            alive_indices: Vec::with_capacity(max_particles),
            free_list,
            max_particles,
            speed_noise_offsets: vec![0.0; max_particles],
            texture_variants: vec![0; max_particles],
            butterfly_block_appearance: vec![false; max_particles],
            animation_elapsed: vec![0.0; max_particles],
            animation_frame_offsets: vec![0; max_particles],
            render_kinds: vec![ParticleRenderKind::Leaf; max_particles],
            despawn_on_lifetime: vec![true; max_particles],
            despawn_below_ground: vec![true; max_particles],
            update_buckets: vec![0; max_particles],
            update_intervals: vec![ParticleUpdateConfig::default().interval_seconds; max_particles],
            update_bucket_counts: vec![ParticleUpdateConfig::default().bucket_count; max_particles],
            update_elapsed: vec![0.0; max_particles],
            pending_sim_dt: vec![0.0; max_particles],
            update_bucket_phase: 0,
            update_bucket_elapsed: 0.0,
            update_bucket_step_seconds: crate::game_time::WORLD_TICK_SECONDS_DEFAULT,
            last_tick_step: ParticleTickStep {
                did_step: false,
                active_bucket: 0,
                step_seconds: crate::game_time::WORLD_TICK_SECONDS_DEFAULT,
                bucket_count: PARTICLE_UPDATE_BUCKET_COUNT as u32,
            },
            speed_noise,
            leaf_flight: vec![LeafFlight::new(0); max_particles],
            leaf_display: vec![
                LeafDisplayPose {
                    position: Vec3::ZERO,
                    velocity: Vec3::ZERO,
                    orientation: Quat::IDENTITY,
                };
                max_particles
            ],
        }
    }

    pub fn set_bucket_step_seconds(&mut self, seconds: f32) {
        self.update_bucket_step_seconds = crate::game_time::clamp_world_tick_seconds(seconds);

        let step_seconds = self.bucket_step_seconds();
        if self.update_bucket_elapsed >= step_seconds {
            self.update_bucket_elapsed %= step_seconds;
        }
    }

    fn occupy_slot(&mut self) -> Option<usize> {
        self.free_list.pop()
    }

    fn retire_slot(&mut self, slot: usize) {
        self.is_alive[slot] = false;
        self.pending_sim_dt[slot] = 0.0;
        self.free_list.push(slot);
    }

    fn validate_handle(&self, handle: ParticleHandle) -> Option<usize> {
        let idx = handle.index as usize;
        if idx >= self.max_particles {
            return None;
        }
        if !self.is_alive[idx] {
            return None;
        }
        if self.generations[idx] != handle.generation {
            return None;
        }
        Some(idx)
    }

    /// Number of currently active particles.
    #[allow(dead_code)]
    pub fn alive_count(&self) -> usize {
        self.alive_indices.len()
    }

    /// Maximum number of particles that can exist at once.
    #[allow(dead_code)]
    pub fn capacity(&self) -> usize {
        self.max_particles
    }

    pub fn available_capacity(&self) -> usize {
        self.free_list.len()
    }

    /// Spawns a new particle using the provided description.
    /// Returns a handle that can be used to manipulate the particle later.
    pub fn spawn(&mut self, spawn: ParticleSpawn) -> Option<ParticleHandle> {
        let slot = self.occupy_slot()?;

        let new_generation = self.generations[slot].wrapping_add(1).max(1);
        self.generations[slot] = new_generation;
        self.leaf_flight[slot] = LeafFlight::new(
            (slot as u32).wrapping_mul(0x9e37_79b9)
                ^ new_generation.wrapping_mul(0x85eb_ca6b)
                ^ spawn.speed_noise_offset.to_bits(),
        );
        self.positions[slot] = spawn.position;
        self.velocities[slot] = spawn.velocity;
        self.leaf_display[slot] = LeafDisplayPose {
            position: spawn.position,
            velocity: spawn.velocity,
            orientation: self.leaf_flight[slot].orientation,
        };
        self.colors[slot] = spawn.color;
        self.sizes[slot] = spawn.size.max(0.0001);
        self.wind_factors[slot] = spawn.wind_factor.max(0.0);
        self.gravity_factors[slot] = spawn.gravity_factor.max(0.0);
        self.drift_directions[slot] = spawn.drift_direction.normalize_or_zero();
        self.drift_strengths[slot] = spawn.drift_strength.max(0.0);
        self.drift_frequencies[slot] = spawn.drift_frequency.max(0.001);
        self.motion_modes[slot] = spawn.motion_mode;
        self.sink_on_lifetime[slot] = spawn.sink_on_lifetime;
        self.sink_speeds[slot] = spawn.sink_speed.max(0.01);
        self.is_sinking[slot] = false;
        self.lifetimes[slot] = spawn.lifetime.max(0.001);
        self.ages[slot] = 0.0;
        self.is_alive[slot] = true;
        self.alive_indices.push(slot);
        self.speed_noise_offsets[slot] = spawn.speed_noise_offset;
        self.texture_variants[slot] = spawn.texture_variant;
        self.butterfly_block_appearance[slot] = spawn.motion_mode == MotionMode::GuidedFlight;
        self.animation_elapsed[slot] = 0.0;
        self.animation_frame_offsets[slot] = 0;
        self.render_kinds[slot] = spawn.render_kind;
        self.despawn_on_lifetime[slot] = spawn.despawn_on_lifetime;
        self.despawn_below_ground[slot] = spawn.despawn_below_ground;
        let update_interval = spawn.update.interval_seconds.max(1.0 / 1000.0);
        let update_bucket_count = spawn.update.bucket_count.max(1);
        let update_bucket =
            self.assign_update_bucket(slot, spawn.speed_noise_offset, update_bucket_count);
        self.update_buckets[slot] = update_bucket;
        self.update_intervals[slot] = update_interval;
        self.update_bucket_counts[slot] = update_bucket_count;
        let first_delay = update_interval * (update_bucket + 1) as f32 / update_bucket_count as f32;
        self.update_elapsed[slot] = update_interval - first_delay;
        self.pending_sim_dt[slot] = 0.0;

        Some(ParticleHandle {
            index: slot as u32,
            generation: new_generation,
        })
    }

    /// Marks a particle as dead immediately.
    #[allow(dead_code)]
    pub fn despawn(&mut self, handle: ParticleHandle) -> bool {
        if let Some(idx) = self.validate_handle(handle) {
            if let Some(alive_idx) = self
                .alive_indices
                .iter()
                .position(|alive_slot| *alive_slot == idx)
            {
                // this is O(1), orders of magnitude faster than remove(idx)
                // the only downside is that the order of the alive_indices is not preserved,
                // but we don't care about that in this use case
                self.alive_indices.swap_remove(alive_idx);
            }
            self.retire_slot(idx);
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.alive_indices.clear();
        self.free_list.clear();
        for idx in (0..self.max_particles).rev() {
            self.is_alive[idx] = false;
            self.pending_sim_dt[idx] = 0.0;
            self.free_list.push(idx);
        }
    }

    fn kill_dead_particle(&mut self, alive_list_idx: usize, slot: usize) {
        self.alive_indices.swap_remove(alive_list_idx);
        self.retire_slot(slot);
    }

    fn assign_update_bucket(&self, slot: usize, spawn_seed: f32, bucket_count: u32) -> u32 {
        if bucket_count <= 1 {
            return 0;
        }

        let seed = (slot as u32)
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add(self.generations[slot].wrapping_mul(0x85EB_CA6B))
            .wrapping_add(spawn_seed.to_bits().wrapping_mul(0xC2B2_AE35));

        (seed ^ (seed >> 16)).wrapping_mul(0x7FEB_352D) % bucket_count
    }

    fn bucket_step_seconds(&self) -> f32 {
        self.update_bucket_step_seconds
    }

    fn step_animation_frame(
        elapsed: &mut f32,
        frame_offset: &mut u32,
        dt: f32,
        frame_duration_sec: f32,
        frame_count: u32,
    ) {
        if frame_count <= 1 || frame_duration_sec <= f32::EPSILON || dt <= 0.0 {
            return;
        }

        *elapsed += dt;
        while *elapsed >= frame_duration_sec {
            *elapsed -= frame_duration_sec;
            *frame_offset = (*frame_offset + 1) % frame_count;
        }
    }

    fn is_falling_leaf(&self, slot: usize) -> bool {
        self.render_kinds[slot] == ParticleRenderKind::Leaf
            && self.motion_modes[slot] == MotionMode::Falling
    }

    /// Advances the simulation by `dt` seconds and applies forces/damping.
    /// Falling leaves use coupled flight; other kinds/modes retain their existing motion.
    #[allow(dead_code)]
    pub fn update(&mut self, dt: f32, forces: ParticleForces) {
        self.update_with_wind(dt, forces, &WindFieldFrame::default());
    }

    pub fn update_with_wind(&mut self, dt: f32, forces: ParticleForces, wind: &WindFieldFrame) {
        if !dt.is_finite() || dt <= 0.0 || self.alive_indices.is_empty() {
            let bucket_count = PARTICLE_UPDATE_BUCKET_COUNT.max(1) as u32;
            let step_seconds = self.bucket_step_seconds();
            self.last_tick_step = ParticleTickStep {
                did_step: false,
                active_bucket: self.update_bucket_phase % bucket_count.max(1),
                step_seconds,
                bucket_count,
            };
            return;
        }

        let bucket_count = PARTICLE_UPDATE_BUCKET_COUNT.max(1) as u32;
        let bucket_step_seconds = self.bucket_step_seconds();
        let mut active_bucket = 0;
        let mut should_step_bucket = false;
        if bucket_count > 1 {
            self.update_bucket_elapsed += dt;
            if self.update_bucket_elapsed >= bucket_step_seconds {
                self.update_bucket_elapsed -= bucket_step_seconds;
                active_bucket = self.update_bucket_phase % bucket_count;
                self.update_bucket_phase = (self.update_bucket_phase + 1) % bucket_count;
                should_step_bucket = true;
            }
        }
        self.last_tick_step = ParticleTickStep {
            did_step: if bucket_count > 1 {
                should_step_bucket
            } else {
                true
            },
            active_bucket,
            step_seconds: bucket_step_seconds,
            bucket_count,
        };

        let base_damping = 1.0_f32 - forces.linear_damping.clamp(0.0, 0.999);
        let clamped_freq = forces.speed_noise.frequency.max(0.0001);
        self.speed_noise.set_frequency(Some(clamped_freq));

        let mut alive_cursor = 0;
        while alive_cursor < self.alive_indices.len() {
            let slot = self.alive_indices[alive_cursor];
            let mode = self.motion_modes[slot];
            if mode == MotionMode::GuidedFlight {
                alive_cursor += 1;
                continue;
            }
            let uses_leaf_flight = self.is_falling_leaf(slot);
            self.pending_sim_dt[slot] += dt;
            self.update_elapsed[slot] += dt;
            let update_interval = self.update_intervals[slot];
            if !uses_leaf_flight && self.update_elapsed[slot] < update_interval {
                alive_cursor += 1;
                continue;
            }
            self.update_elapsed[slot] %= update_interval;

            let sim_dt = self.pending_sim_dt[slot];
            self.pending_sim_dt[slot] = 0.0;
            let damping = base_damping;

            let is_sinking = self.is_sinking[slot];
            if uses_leaf_flight {
                if is_sinking {
                    let vel = &mut self.velocities[slot];
                    let sink_damping = (base_damping * 0.96).powf(sim_dt / update_interval);
                    vel.x *= sink_damping;
                    vel.z *= sink_damping;
                    vel.y = -self.sink_speeds[slot];
                    self.positions[slot] += *vel * sim_dt;
                    self.leaf_flight[slot].settle(sim_dt);
                } else {
                    self.leaf_flight[slot].advance(
                        &mut self.positions[slot],
                        &mut self.velocities[slot],
                        sim_dt,
                        self.sizes[slot],
                        self.gravity_factors[slot],
                        self.wind_factors[slot],
                        |position| wind.sample_world(position),
                    );
                }
            } else {
                let vel = &mut self.velocities[slot];

                // Apply randomized turbulent drift
                let age = self.ages[slot];
                let drift_phase = age * self.drift_frequencies[slot];
                let drift_offset_x =
                    (drift_phase * 2.3).sin() * 0.7 + (drift_phase * 1.1).cos() * 0.3;
                let drift_offset_y = (drift_phase * 1.7).sin() * 0.5;
                let drift_offset_z =
                    (drift_phase * 3.1).cos() * 0.7 + (drift_phase * 1.9).sin() * 0.3;

                let turbulence = Vec3::new(drift_offset_x, drift_offset_y, drift_offset_z);
                let drift_force =
                    (self.drift_directions[slot] + turbulence * 0.5) * self.drift_strengths[slot];
                *vel += drift_force * sim_dt;

                if is_sinking {
                    let sink_damping = base_damping * 0.96;
                    vel.x *= sink_damping;
                    vel.z *= sink_damping;
                    vel.y = -self.sink_speeds[slot];
                } else {
                    match mode {
                        MotionMode::Falling => {
                            let gravity_scale = self.gravity_factors[slot];
                            let planar_speed_multiplier =
                                forces.leaf_planar_speed_multiplier.max(0.0);
                            // Clamp and order the speed range
                            let (min_speed, max_speed) =
                                if forces.speed_noise.min_speed <= forces.speed_noise.max_speed {
                                    (forces.speed_noise.min_speed, forces.speed_noise.max_speed)
                                } else {
                                    (forces.speed_noise.max_speed, forces.speed_noise.min_speed)
                                };

                            let noise_t = age + self.speed_noise_offsets[slot];
                            let noise_val =
                                self.speed_noise.get_noise_2d(noise_t, 0.0).clamp(-1.0, 1.0);
                            let normalized = noise_val * 0.5 + 0.5; // 0..1
                            let target_speed =
                                (min_speed + (max_speed - min_speed) * normalized) * gravity_scale;

                            // Keep horizontal motion damped; vertical comes purely from noise.
                            vel.x *= damping;
                            vel.z *= damping;
                            vel.x *= planar_speed_multiplier;
                            vel.z *= planar_speed_multiplier;
                            vel.y = -target_speed;
                        }
                        MotionMode::Free => {
                            vel.y -= 3.6 * self.gravity_factors[slot] * sim_dt;
                            *vel *= damping;
                            let max_speed = 3.0;
                            let speed = vel.length();
                            if speed > max_speed {
                                *vel *= max_speed / speed;
                            }
                        }
                        MotionMode::GuidedFlight => {
                            unreachable!("guided flight advances separately")
                        }
                    }
                }

                self.positions[slot] += *vel * sim_dt;
            }
            self.ages[slot] += sim_dt;

            match self.render_kinds[slot] {
                ParticleRenderKind::Butterfly => {
                    Self::step_animation_frame(
                        &mut self.animation_elapsed[slot],
                        &mut self.animation_frame_offsets[slot],
                        sim_dt,
                        BUTTERFLY_ANIM_FRAME_DURATION_SEC,
                        BUTTERFLY_FRAMES_PER_VARIANT,
                    );
                }
                ParticleRenderKind::Leaf
                | ParticleRenderKind::ButterflyBlock
                | ParticleRenderKind::WaterDroplet
                | ParticleRenderKind::TerrainVoxel => {}
            }

            if !self.is_sinking[slot]
                && self.sink_on_lifetime[slot]
                && self.ages[slot] >= self.lifetimes[slot]
            {
                self.is_sinking[slot] = true;
            }

            // Sink-enabled particles only despawn once they go below the ground plane.
            let should_despawn = if self.is_sinking[slot] {
                self.despawn_below_ground[slot] && self.positions[slot].y < 0.0
            } else {
                (self.despawn_on_lifetime[slot] && self.ages[slot] >= self.lifetimes[slot])
                    || (self.despawn_below_ground[slot] && self.positions[slot].y < 0.0)
            };
            if should_despawn {
                self.kill_dead_particle(alive_cursor, slot);
                continue;
            }

            alive_cursor += 1;
        }

        // Reuse the existing world-tick particle buckets for presentation only.
        // In particular, the 120 Hz mechanical integrator above still runs on
        // every frame and must never read this held display state back.
        if self.last_tick_step.did_step {
            for &slot in &self.alive_indices {
                if self.is_falling_leaf(slot)
                    && self.update_buckets[slot] % self.last_tick_step.bucket_count
                        == self.last_tick_step.active_bucket
                {
                    self.leaf_display[slot] = LeafDisplayPose {
                        position: self.positions[slot],
                        velocity: self.velocities[slot],
                        orientation: self.leaf_flight[slot].orientation,
                    };
                }
            }
        }
    }

    /// Copies the alive particle data into the provided buffer for rendering.
    #[cfg(test)]
    pub fn write_snapshots(&self, out: &mut Vec<ParticleSnapshot>) {
        self.write_snapshots_with_block_pose(out, |_, position| position);
    }

    /// Leaves keep their world-tick pose; only B butterflies use the supplied shared-rhythm pose.
    /// Neither presentation path feeds back into authoritative simulation state.
    pub fn write_snapshots_with_block_pose(
        &self,
        out: &mut Vec<ParticleSnapshot>,
        mut block_pose: impl FnMut(ParticleHandle, Vec3) -> Vec3,
    ) {
        out.clear();
        out.reserve(self.alive_indices.len());
        for slot in &self.alive_indices {
            let mut kind = self.render_kinds[*slot];
            let mut color = self.colors[*slot];

            if kind == ParticleRenderKind::Butterfly {
                let age = self.ages[*slot];
                let lifetime = self.lifetimes[*slot];
                let fade = Self::butterfly_fade_factor(age, lifetime);

                // fade butterflies by modulating alpha
                color.w *= fade;
                if self.butterfly_block_appearance[*slot] {
                    kind = ParticleRenderKind::ButterflyBlock;
                    let rgb = crate::tracer::ButterflyPalettePreset::from_index(
                        self.texture_variants[*slot],
                    )
                    .config()
                    .mid_shade;
                    // The sprite LUT is sRGB; vertex colors are linear.
                    for axis in 0..3 {
                        let srgb = rgb[axis] as f32 / 255.0;
                        color[axis] = if srgb <= 0.04045 {
                            srgb / 12.92
                        } else {
                            ((srgb + 0.055) / 1.055).powf(2.4)
                        };
                    }
                }
            }

            let (position_ws, velocity) = if self.is_falling_leaf(*slot) {
                let displayed = self.leaf_display[*slot];
                (displayed.position, displayed.velocity)
            } else {
                (self.positions[*slot], self.velocities[*slot])
            };
            out.push(ParticleSnapshot {
                position_ws: if self.motion_modes[*slot] == MotionMode::GuidedFlight {
                    block_pose(
                        ParticleHandle {
                            index: *slot as u32,
                            generation: self.generations[*slot],
                        },
                        position_ws,
                    )
                } else {
                    position_ws
                },
                velocity,
                color,
                size: if self.motion_modes[*slot] == MotionMode::GuidedFlight {
                    STANDARD_PARTICLE_SIZE
                        * if kind == ParticleRenderKind::Butterfly {
                            BUTTERFLY_SPRITE_SIZE_COMPENSATION
                        } else {
                            1.0
                        }
                } else {
                    self.sizes[*slot]
                },
                kind,
                texture_variant: self.texture_variants[*slot],
                animation_frame_offset: self.animation_frame_offsets[*slot],
                animation_phase_offset: (((*slot as u32).wrapping_mul(0x9e37_79b9)
                    ^ self.generations[*slot].wrapping_mul(0x85eb_ca6b))
                    >> 8) as f32
                    / 16_777_216.0,
                leaf_orientation: self
                    .is_falling_leaf(*slot)
                    .then_some(self.leaf_display[*slot].orientation),
            });
        }
    }

    fn butterfly_fade_factor(age: f32, lifetime: f32) -> f32 {
        if lifetime <= 0.0 {
            return 1.0;
        }

        let fade_duration = (lifetime * 0.1).min(1.0);
        if fade_duration <= f32::EPSILON {
            return 1.0;
        }

        let clamped_age = age.clamp(0.0, lifetime);

        if clamped_age < fade_duration {
            clamped_age / fade_duration
        } else if clamped_age > lifetime - fade_duration {
            (lifetime - clamped_age) / fade_duration
        } else {
            1.0
        }
    }

    /// Changes style on the same slot without resetting position, age, palette or animation.
    #[cfg(test)]
    pub fn set_butterfly_block_mode(&mut self, handle: ParticleHandle, enabled: bool) -> bool {
        self.set_butterfly_flight_style(handle, enabled, enabled)
    }

    pub fn set_butterfly_flight_style(
        &mut self,
        handle: ParticleHandle,
        guided: bool,
        blocks: bool,
    ) -> bool {
        let Some(idx) = self.validate_handle(handle) else {
            return false;
        };
        if self.render_kinds[idx] != ParticleRenderKind::Butterfly {
            return false;
        }
        self.butterfly_block_appearance[idx] = blocks;
        let mode = if guided {
            MotionMode::GuidedFlight
        } else {
            MotionMode::Free
        };
        if mode == self.motion_modes[idx] {
            return false;
        }
        self.motion_modes[idx] = mode;
        // Account for time accumulated by A, but do not move during a checkbox change.
        self.ages[idx] += self.pending_sim_dt[idx];
        self.pending_sim_dt[idx] = 0.0;
        self.update_elapsed[idx] = 0.0;
        true
    }

    pub fn advance_guided_flight(&mut self, handle: ParticleHandle, velocity: Vec3, dt: f32) {
        let Some(idx) = self.validate_handle(handle) else {
            return;
        };
        debug_assert_eq!(self.motion_modes[idx], MotionMode::GuidedFlight);
        debug_assert!(velocity.is_finite() && dt.is_finite() && dt >= 0.0);
        self.velocities[idx] = velocity;
        self.positions[idx] += velocity * dt;
        self.ages[idx] += dt;
        Self::step_animation_frame(
            &mut self.animation_elapsed[idx],
            &mut self.animation_frame_offsets[idx],
            dt,
            BUTTERFLY_ANIM_FRAME_DURATION_SEC,
            BUTTERFLY_FRAMES_PER_VARIANT,
        );
        if (self.despawn_on_lifetime[idx] && self.ages[idx] >= self.lifetimes[idx])
            || (self.despawn_below_ground[idx] && self.positions[idx].y < 0.0)
        {
            self.despawn(handle);
        }
    }

    #[allow(dead_code)]
    pub fn position(&self, handle: ParticleHandle) -> Option<Vec3> {
        self.validate_handle(handle).map(|idx| self.positions[idx])
    }

    #[allow(dead_code)]
    pub fn velocity(&self, handle: ParticleHandle) -> Option<Vec3> {
        self.validate_handle(handle).map(|idx| self.velocities[idx])
    }

    #[allow(dead_code)]
    pub fn flip_planar_motion(
        &mut self,
        handle: ParticleHandle,
        flip_x: bool,
        flip_z: bool,
    ) -> bool {
        if let Some(idx) = self.validate_handle(handle) {
            if flip_x {
                self.velocities[idx].x = -self.velocities[idx].x;
                self.drift_directions[idx].x = -self.drift_directions[idx].x;
            }
            if flip_z {
                self.velocities[idx].z = -self.velocities[idx].z;
                self.drift_directions[idx].z = -self.drift_directions[idx].z;
            }
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn set_position(&mut self, handle: ParticleHandle, pos: Vec3) -> bool {
        if let Some(idx) = self.validate_handle(handle) {
            self.positions[idx] = pos;
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn set_velocity(&mut self, handle: ParticleHandle, vel: Vec3) -> bool {
        if let Some(idx) = self.validate_handle(handle) {
            self.velocities[idx] = vel;
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn set_color(&mut self, handle: ParticleHandle, color: Vec4) -> bool {
        if let Some(idx) = self.validate_handle(handle) {
            self.colors[idx] = color;
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn set_size(&mut self, handle: ParticleHandle, size: f32) -> bool {
        if let Some(idx) = self.validate_handle(handle) {
            self.sizes[idx] = size.max(0.0001);
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn set_texture_variant(&mut self, handle: ParticleHandle, texture_variant: u32) -> bool {
        if let Some(idx) = self.validate_handle(handle) {
            self.texture_variants[idx] = texture_variant;
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn add_velocity(&mut self, handle: ParticleHandle, delta: Vec3) -> bool {
        if let Some(idx) = self.validate_handle(handle) {
            self.velocities[idx] += delta;
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn is_alive_handle(&self, handle: ParticleHandle) -> bool {
        self.validate_handle(handle).is_some()
    }

    pub fn last_tick_step(&self) -> ParticleTickStep {
        self.last_tick_step
    }

    pub fn handle_bucket(&self, handle: ParticleHandle) -> Option<u32> {
        self.validate_handle(handle)
            .map(|idx| self.update_buckets[idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free_particle(update: ParticleUpdateConfig) -> ParticleSpawn {
        ParticleSpawn {
            velocity: Vec3::X,
            gravity_factor: 0.0,
            motion_mode: MotionMode::Free,
            despawn_below_ground: false,
            update,
            ..ParticleSpawn::default()
        }
    }

    #[test]
    fn particles_can_use_independent_update_intervals() {
        let mut system = ParticleSystem::new(2);
        let fast = system
            .spawn(free_particle(ParticleUpdateConfig::new(0.05, 1)))
            .unwrap();
        let slow = system
            .spawn(free_particle(ParticleUpdateConfig::new(0.10, 1)))
            .unwrap();

        system.update(0.05, ParticleForces::default());
        assert!(system.position(fast).unwrap().x > 0.0);
        assert_eq!(system.position(slow).unwrap().x, 0.0);

        system.update(0.05, ParticleForces::default());
        assert!(system.position(slow).unwrap().x > 0.0);
    }

    #[test]
    fn particle_bucket_assignment_respects_its_own_bucket_count() {
        let mut system = ParticleSystem::new(8);
        for bucket_count in [1, 3, 5] {
            let handle = system
                .spawn(free_particle(ParticleUpdateConfig::new(0.12, bucket_count)))
                .unwrap();
            let slot = system.validate_handle(handle).unwrap();
            assert_eq!(system.update_intervals[slot], 0.12);
            assert_eq!(system.update_bucket_counts[slot], bucket_count);
            assert!(system.handle_bucket(handle).unwrap() < bucket_count);
        }
    }

    #[test]
    fn terrain_voxel_particle_falls_and_expires() {
        let mut system = ParticleSystem::new(1);
        let handle = system
            .spawn(ParticleSpawn {
                position: Vec3::Y,
                lifetime: 0.05,
                gravity_factor: 1.0,
                motion_mode: MotionMode::Free,
                render_kind: ParticleRenderKind::TerrainVoxel,
                despawn_below_ground: false,
                update: ParticleUpdateConfig::new(0.01, 1),
                ..ParticleSpawn::default()
            })
            .unwrap();

        system.update(0.02, ParticleForces::default());
        assert!(system.velocity(handle).unwrap().y < 0.0);
        assert!(system.position(handle).unwrap().y < 1.0);

        system.update(0.04, ParticleForces::default());
        assert!(!system.is_alive_handle(handle));
    }

    fn long_lived_leaf() -> ParticleSpawn {
        ParticleSpawn {
            position: Vec3::Y,
            velocity: Vec3::new(0.06, 0., -0.03),
            lifetime: 120.,
            size: STANDARD_PARTICLE_SIZE,
            despawn_below_ground: false,
            ..ParticleSpawn::default()
        }
    }

    #[test]
    fn butterfly_ab_and_leaf_flight_keep_independent_update_and_display_authority() {
        let mut mixed = ParticleSystem::new(2);
        let mut leaf_only = ParticleSystem::new(2);
        let leaf = mixed.spawn(long_lived_leaf()).unwrap();
        let reference_leaf = leaf_only.spawn(long_lived_leaf()).unwrap();
        let butterfly = mixed
            .spawn(ParticleSpawn {
                position: Vec3::new(1., 2., 1.),
                motion_mode: MotionMode::GuidedFlight,
                render_kind: ParticleRenderKind::Butterfly,
                texture_variant: 3,
                ..long_lived_leaf()
            })
            .unwrap();
        let butterfly_slot = mixed.validate_handle(butterfly).unwrap();
        let wind = WindFieldFrame::uniform(glam::Vec2::new(0.4, -0.2));
        let dt = 1. / 120.;
        let mut actual = Vec::new();
        let mut reference = Vec::new();

        for frame in 0..240 {
            if frame == 80 || frame == 160 {
                assert!(mixed.set_butterfly_block_mode(butterfly, frame == 160));
            }
            let block_mode = mixed.motion_modes[butterfly_slot] == MotionMode::GuidedFlight;
            let before = (
                mixed.position(butterfly),
                mixed.velocity(butterfly),
                mixed.ages[butterfly_slot],
            );
            mixed.update_with_wind(dt, ParticleForces::default(), &wind);
            leaf_only.update_with_wind(dt, ParticleForces::default(), &wind);
            if block_mode {
                assert_eq!(
                    before,
                    (
                        mixed.position(butterfly),
                        mixed.velocity(butterfly),
                        mixed.ages[butterfly_slot],
                    ),
                    "the shared leaf update must not also integrate or age a guided butterfly",
                );
                mixed.advance_guided_flight(butterfly, Vec3::new(0.03, 0.02, -0.01), dt);
            }
            assert_eq!(mixed.position(leaf), leaf_only.position(reference_leaf));
            assert_eq!(mixed.velocity(leaf), leaf_only.velocity(reference_leaf));

            let held_butterfly_pose = Vec3::new(1., 2., 1.);
            let mut block_pose_calls = 0;
            mixed.write_snapshots_with_block_pose(&mut actual, |handle, physical_position| {
                assert_eq!(handle, butterfly);
                assert_eq!(physical_position, mixed.position(butterfly).unwrap());
                block_pose_calls += 1;
                held_butterfly_pose
            });
            leaf_only.write_snapshots(&mut reference);
            assert_eq!(block_pose_calls, usize::from(block_mode));
            let leaf_snapshot = actual
                .iter()
                .find(|s| s.kind == ParticleRenderKind::Leaf)
                .unwrap();
            assert_eq!(leaf_snapshot.position_ws, reference[0].position_ws);
            assert_eq!(leaf_snapshot.velocity, reference[0].velocity);
            assert_eq!(
                leaf_snapshot.leaf_orientation,
                reference[0].leaf_orientation
            );
            let butterfly_snapshot = actual
                .iter()
                .find(|s| s.kind != ParticleRenderKind::Leaf)
                .unwrap();
            assert_eq!(butterfly_snapshot.texture_variant, 3);
            assert!(butterfly_snapshot.leaf_orientation.is_none());
            assert_eq!(
                butterfly_snapshot.position_ws,
                if block_mode {
                    held_butterfly_pose
                } else {
                    mixed.position(butterfly).unwrap()
                },
            );
        }
        assert_eq!(mixed.alive_count(), 2);
    }

    #[test]
    fn leaf_display_holds_between_world_ticks_while_physics_advances() {
        let mut system = ParticleSystem::new(1);
        system.spawn(long_lived_leaf()).unwrap();
        system.set_bucket_step_seconds(0.05);
        let mut snapshots = Vec::new();
        system.write_snapshots(&mut snapshots);
        let displayed = snapshots[0];

        system.update(1. / 120., ParticleForces::default());
        assert!(!system.last_tick_step().did_step);
        assert_ne!(system.positions[0], displayed.position_ws);
        assert_ne!(
            system.leaf_flight[0].orientation,
            displayed.leaf_orientation.unwrap(),
            "physical pose must continue to integrate between display ticks",
        );
        system.write_snapshots(&mut snapshots);
        assert_eq!(
            snapshots[0].position_ws, displayed.position_ws,
            "rendered leaf position must hold until its world-tick bucket publishes",
        );
        assert_eq!(snapshots[0].velocity, displayed.velocity);
        assert_eq!(snapshots[0].leaf_orientation, displayed.leaf_orientation);
    }

    #[test]
    fn leaf_world_tick_controls_only_display_not_physical_trajectory() {
        let steps = [0.025, 0.05, 0.1];
        let mut systems = steps.map(|step| {
            let mut system = ParticleSystem::new(1);
            system.spawn(long_lived_leaf()).unwrap();
            system.set_bucket_step_seconds(step);
            system
        });
        let mut changes = [0_u32; 3];
        let mut previous = [long_lived_leaf().position; 3];
        let mut snapshots = Vec::new();
        for frame in 0..240 {
            for (index, system) in systems.iter_mut().enumerate() {
                // Exercise the same per-frame setter as the GUI adapter.
                system.set_bucket_step_seconds(steps[index]);
                system.update_with_wind(
                    1. / 120.,
                    ParticleForces::default(),
                    &WindFieldFrame::uniform(glam::Vec2::new(0.7, 0.2)),
                );
                system.write_snapshots(&mut snapshots);
                let displayed = snapshots[0];
                assert!(displayed.leaf_orientation.is_some());
                if displayed.position_ws != previous[index] {
                    let tick = system.last_tick_step();
                    assert!(tick.did_step, "frame={frame}");
                    assert_eq!(system.update_buckets[0], tick.active_bucket);
                    assert_eq!(displayed.position_ws, system.positions[0]);
                    assert_eq!(displayed.velocity, system.velocities[0]);
                    assert_eq!(
                        displayed.leaf_orientation,
                        Some(system.leaf_flight[0].orientation)
                    );
                    changes[index] += 1;
                }
                previous[index] = displayed.position_ws;
            }
            for system in &systems[1..] {
                assert_eq!(system.positions, systems[0].positions);
                assert_eq!(system.velocities, systems[0].velocities);
                assert_eq!(system.ages, systems[0].ages);
                assert_eq!(
                    system.leaf_flight[0].orientation,
                    systems[0].leaf_flight[0].orientation
                );
            }
        }
        // Two existing particle buckets: each leaf publishes once per cycle.
        for (actual, expected) in changes.into_iter().zip([40, 20, 10]) {
            assert!(actual.abs_diff(expected) <= 1, "{changes:?}");
        }
    }

    #[test]
    fn leaf_display_cadence_edit_and_slot_reuse_keep_publication_coherent() {
        let mut system = ParticleSystem::new(1);
        let handle = system.spawn(long_lived_leaf()).unwrap();
        let mut snapshots = Vec::new();
        system.write_snapshots(&mut snapshots);
        let initial = snapshots[0];
        system.update(1. / 120., ParticleForces::default());
        let physical = (system.positions[0], system.leaf_flight[0].orientation);
        // A live GUI cadence edit also cannot reset the physical state or publish early.
        system.set_bucket_step_seconds(0.1);
        system.write_snapshots(&mut snapshots);
        assert_eq!(snapshots[0].position_ws, initial.position_ws);
        assert_eq!(snapshots[0].leaf_orientation, initial.leaf_orientation);
        assert_eq!(
            physical,
            (system.positions[0], system.leaf_flight[0].orientation)
        );

        assert!(system.despawn(handle));
        system.write_snapshots(&mut snapshots);
        assert!(snapshots.is_empty());
        let replacement = ParticleSpawn {
            position: Vec3::splat(3.),
            ..long_lived_leaf()
        };
        system.spawn(replacement).unwrap();
        system.write_snapshots(&mut snapshots);
        assert_eq!(snapshots[0].position_ws, replacement.position);
        assert_eq!(
            snapshots[0].leaf_orientation,
            Some(system.leaf_flight[0].orientation)
        );
        assert_ne!(snapshots[0].leaf_orientation, initial.leaf_orientation);
    }

    #[test]
    fn falling_leaf_flight_is_unconditional_and_preserves_identity_and_lifetime() {
        let mut system = ParticleSystem::new(1);
        let handle = system.spawn(long_lived_leaf()).unwrap();
        let mut snapshots = Vec::new();
        system.write_snapshots(&mut snapshots);
        assert_eq!(
            snapshots[0].leaf_orientation,
            Some(system.leaf_flight[0].orientation)
        );
        let initial_pose = system.leaf_flight[0].orientation;
        for _ in 0..3 {
            system.update(0.2, ParticleForces::default());
            system.write_snapshots(&mut snapshots);
            assert!(snapshots[0].leaf_orientation.is_some());
            assert!(system.is_alive_handle(handle) && system.alive_count() == 1);
        }
        let pose = system.leaf_flight[0].orientation;
        assert_ne!(pose, initial_pose);
        assert!((system.ages[0] - 0.6).abs() < 1e-6);
        assert!(system.despawn(handle));
        let replacement = system.spawn(long_lived_leaf()).unwrap();
        assert!(!system.is_alive_handle(handle));
        assert_ne!(replacement, handle);
        assert_ne!(pose, system.leaf_flight[0].orientation);
        system.clear();
        assert_eq!(system.available_capacity(), 1);
    }

    #[test]
    fn coupled_leaf_flight_does_not_apply_to_other_kinds_or_free_particles() {
        for kind in [
            ParticleRenderKind::Butterfly,
            ParticleRenderKind::WaterDroplet,
            ParticleRenderKind::TerrainVoxel,
            ParticleRenderKind::Leaf,
        ] {
            for mode in [MotionMode::Free, MotionMode::Falling] {
                if kind == ParticleRenderKind::Leaf && mode == MotionMode::Falling {
                    continue;
                }
                let mut calm = ParticleSystem::new(1);
                let mut windy = ParticleSystem::new(1);
                let spawn = ParticleSpawn {
                    render_kind: kind,
                    motion_mode: mode,
                    drift_strength: 0.3,
                    ..long_lived_leaf()
                };
                calm.spawn(spawn).unwrap();
                windy.spawn(spawn).unwrap();
                let unused_pose = windy.leaf_flight[0].orientation;
                let forces = ParticleForces {
                    speed_noise: SpeedNoise {
                        min_speed: 0.12,
                        max_speed: 0.12,
                        ..SpeedNoise::default()
                    },
                    ..ParticleForces::default()
                };
                for _ in 0..120 {
                    calm.update(1. / 60., forces);
                    windy.update_with_wind(
                        1. / 60.,
                        forces,
                        &WindFieldFrame::uniform(glam::Vec2::splat(10.)),
                    );
                    assert_eq!(calm.positions, windy.positions);
                    assert_eq!(calm.velocities, windy.velocities);
                    assert_eq!(windy.leaf_flight[0].orientation, unused_pose);
                }
                if mode == MotionMode::Falling {
                    assert_eq!(windy.velocities[0].y, -0.12);
                }
                let mut snapshots = Vec::new();
                windy.write_snapshots(&mut snapshots);
                assert!(snapshots[0].leaf_orientation.is_none());
            }
        }
    }

    #[test]
    fn plate_motion_ignores_legacy_drift_and_samples_relative_wind() {
        let mut a = ParticleSystem::new(1);
        let mut b = ParticleSystem::new(1);
        let mut windy = ParticleSystem::new(1);
        a.spawn(long_lived_leaf()).unwrap();
        b.spawn(ParticleSpawn {
            drift_strength: 500.,
            drift_frequency: 20.,
            ..long_lived_leaf()
        })
        .unwrap();
        windy.spawn(long_lived_leaf()).unwrap();
        for _ in 0..240 {
            a.update(1. / 60., ParticleForces::default());
            b.update(
                1. / 60.,
                ParticleForces {
                    leaf_planar_speed_multiplier: 50.,
                    ..ParticleForces::default()
                },
            );
            windy.update_with_wind(
                1. / 60.,
                ParticleForces::default(),
                &WindFieldFrame::uniform(glam::Vec2::new(2., 0.)),
            );
        }
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.velocities, b.velocities);
        assert!(windy.positions[0].x > a.positions[0].x + 0.1);
        assert_ne!(
            a.leaf_flight[0].orientation,
            windy.leaf_flight[0].orientation
        );
    }

    #[test]
    fn plate_system_keeps_sink_and_despawn_authority() {
        let mut system = ParticleSystem::new(1);
        let handle = system
            .spawn(ParticleSpawn {
                position: Vec3::Y * 0.02,
                lifetime: 0.1,
                sink_on_lifetime: true,
                sink_speed: 0.1,
                despawn_below_ground: true,
                ..long_lived_leaf()
            })
            .unwrap();
        system.update(0.11, ParticleForces::default());
        assert!(system.is_alive_handle(handle) && system.is_sinking[0]);
        system.update(0.3, ParticleForces::default());
        assert!(!system.is_alive_handle(handle));
        assert_eq!(system.available_capacity(), 1);
    }
}
