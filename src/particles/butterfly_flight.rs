//! Art-directed, seeded acceleration events. See docs/research/butterfly_block_flight_motion_research.md.
use glam::Vec3;
use rand::{rngs::SmallRng, RngExt, SeedableRng};

pub(super) const FLIGHT_STEP_SECONDS: f64 = 1.0 / 120.0;
const BUTTERFLY_BLOCK_CRUISE_SPEED_MIN: f32 = 0.10;
const BUTTERFLY_BLOCK_CRUISE_SPEED_MAX: f32 = 0.16;
const BUTTERFLY_BLOCK_MAX_SPEED: f32 = 0.30;
const BUTTERFLY_BLOCK_MAX_VERTICAL_SPEED: f32 = 0.20;
const BUTTERFLY_BLOCK_MAX_ACCELERATION: f32 = 1.8;
const BUTTERFLY_BLOCK_MAX_JERK: f32 = 14.0;
const BUTTERFLY_BLOCK_EVENT_DURATION_MIN: f32 = 0.07;
const BUTTERFLY_BLOCK_EVENT_DURATION_MAX: f32 = 0.20;
const BUTTERFLY_BLOCK_EVENT_WAIT_MIN: f32 = 0.12;
const BUTTERFLY_BLOCK_EVENT_WAIT_MAX: f32 = 0.85;
const BUTTERFLY_BLOCK_RAPID_GAP_MIN: f32 = 0.035;
const BUTTERFLY_BLOCK_RAPID_GAP_MAX: f32 = 0.11;
const BUTTERFLY_BLOCK_HABITAT_RADIUS: f32 = 0.32;
const BUTTERFLY_BLOCK_HABITAT_HEIGHT: f32 = 0.20;
// WindFieldFrame carries authored strength, not world metres/second.
const WIND_DRIFT_WORLD_SPEED_PER_STRENGTH: f32 = 0.06;
const MAX_WIND_DRIFT_SPEED: f32 = 0.45;
const MAX_WIND_DRIFT_ACCELERATION: f32 = 0.60;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ButterflyFlightVariant {
    /// Legacy flight retained for old recordings and diagnostics, not the appearance checkbox.
    OriginalSprite,
    #[default]
    DartingSprite,
    DartingBlock,
}

impl ButterflyFlightVariant {
    pub const fn uses_darting_flight(self) -> bool {
        !matches!(self, Self::OriginalSprite)
    }
    pub const fn is_darting_block(self) -> bool {
        matches!(self, Self::DartingBlock)
    }
}

/// Saved B-only art controls. Self propulsion and environmental drift are independent.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ButterflyFlightTuning {
    pub flight_frequency_hz: f32,
    /// World units above local terrain; the walking camera's default eye height is 0.08.
    pub height_above_ground: f32,
    /// Retained for saved settings compatibility, no longer exposed as a misleading GUI tempo.
    pub maneuver_tempo: f32,
    pub vertical_strength: f32,
    pub turn_sharpness: f32,
    pub speed: f32,
    pub wind_drift: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ButterflyFlightSettings {
    pub variant: ButterflyFlightVariant,
    pub tuning: ButterflyFlightTuning,
}

impl Default for ButterflyFlightTuning {
    fn default() -> Self {
        Self {
            flight_frequency_hz: 10.0,
            height_above_ground: 0.08,
            maneuver_tempo: 1.0,
            vertical_strength: 4.0,
            turn_sharpness: 1.0,
            speed: 0.35,
            wind_drift: 1.0,
        }
    }
}

impl ButterflyFlightTuning {
    pub const FREQUENCY_RANGE: std::ops::RangeInclusive<f32> = 0.0..=40.0;
    pub const HEIGHT_RANGE: std::ops::RangeInclusive<f32> = 0.03..=0.24;
    pub const TEMPO_RANGE: std::ops::RangeInclusive<f32> = 0.25..=4.0;
    pub const VERTICAL_RANGE: std::ops::RangeInclusive<f32> = 0.0..=4.0;
    pub const SHARPNESS_RANGE: std::ops::RangeInclusive<f32> = 0.25..=4.0;
    pub const SPEED_RANGE: std::ops::RangeInclusive<f32> = 0.25..=2.5;
    pub const WIND_DRIFT_RANGE: std::ops::RangeInclusive<f32> = 0.0..=3.0;

    pub fn sanitized(self) -> Self {
        let defaults = Self::default();
        let bounded = |value: f32, range: std::ops::RangeInclusive<f32>, fallback: f32| {
            if value.is_finite() {
                value.clamp(*range.start(), *range.end())
            } else {
                fallback
            }
        };
        Self {
            height_above_ground: bounded(
                self.height_above_ground,
                Self::HEIGHT_RANGE,
                defaults.height_above_ground,
            ),
            flight_frequency_hz: bounded(
                self.flight_frequency_hz,
                Self::FREQUENCY_RANGE,
                defaults.flight_frequency_hz,
            ),
            maneuver_tempo: bounded(
                self.maneuver_tempo,
                Self::TEMPO_RANGE,
                defaults.maneuver_tempo,
            ),
            vertical_strength: bounded(
                self.vertical_strength,
                Self::VERTICAL_RANGE,
                defaults.vertical_strength,
            ),
            turn_sharpness: bounded(
                self.turn_sharpness,
                Self::SHARPNESS_RANGE,
                defaults.turn_sharpness,
            ),
            speed: if self.speed.is_finite() {
                bounded(self.speed, Self::SPEED_RANGE, defaults.speed)
            } else {
                Self::default().speed
            },
            wind_drift: bounded(self.wind_drift, Self::WIND_DRIFT_RANGE, defaults.wind_drift),
        }
    }
}

/// One beat owns both the vertical intent and the publication of a real trajectory
/// point. No independent display timer, phase jitter, or vertical pulse timer.
#[derive(Debug)]
struct SharedFlightRhythm {
    position: Vec3,
    acceleration: f32,
    phase: f64,
    frequency_hz: f32,
    publish_due: bool,
    rng: SmallRng,
}

impl SharedFlightRhythm {
    fn new(seed: f32, position: Vec3) -> Self {
        Self {
            position,
            acceleration: 0.0,
            phase: 0.0,
            frequency_hz: 0.0,
            publish_due: false,
            rng: SmallRng::seed_from_u64(u64::from(seed.to_bits()) ^ 0x7665_7274_6963_616c),
        }
    }

    fn advance(&mut self, position: Vec3, dt: f32, frequency_hz: f32) -> f32 {
        self.publish_due = false;
        if frequency_hz <= 0.0 {
            // Shared stepping off: show continuous physical motion and stop new
            // vertical intent. The bounded integrator still settles existing velocity.
            self.phase = 0.0;
            self.frequency_hz = 0.0;
            self.acceleration = 0.0;
            self.publish_due = true;
            return 0.0;
        }
        // Keep fractional phase on live edits; rounding must not accumulate a second
        // cadence. The 40 Hz maximum is below the 120 Hz integrator's step frequency.
        let starting = self.frequency_hz <= 0.0;
        self.frequency_hz = frequency_hz;
        self.phase += f64::from(dt) * f64::from(frequency_hz);
        let beat_due = self.phase + 1e-8 >= 1.0;
        let displacement_guard = self.position.distance_squared(position) >= 0.025_f32.powi(2);
        if starting || beat_due || displacement_guard {
            self.phase = if beat_due {
                (self.phase - 1.0).max(0.0)
            } else {
                0.0
            };
            self.sample_vertical_intent();
            self.publish_due = true;
        }
        self.acceleration
    }

    fn sample_vertical_intent(&mut self) {
        let roll = self.rng.random_range(0.0..1.0_f32);
        // Symmetric signed bursts have zero expected vertical impulse. A biased
        // random walk otherwise spends long flights against the upper habitat edge.
        let vertical = if roll < 0.21 {
            self.rng.random_range(0.35..=0.95) * if self.rng.random_bool(0.5) { 1.0 } else { -1.0 }
        } else {
            self.rng.random_range(-0.20..=0.20)
        };
        self.acceleration = vertical * self.rng.random_range(0.75..=1.55);
    }

    fn reset(&mut self, position: Vec3) {
        self.position = position;
        self.phase = 0.0;
        self.frequency_hz = 0.0;
        self.acceleration = 0.0;
        self.publish_due = false;
    }

    fn publish(&mut self, position: Vec3) {
        if self.publish_due {
            self.position = position;
        }
    }
}

fn sample_event_wait(rng: &mut SmallRng) -> f32 {
    let unit = rng.random_range(0.0..=1.0_f32);
    BUTTERFLY_BLOCK_EVENT_WAIT_MIN
        + (BUTTERFLY_BLOCK_EVENT_WAIT_MAX - BUTTERFLY_BLOCK_EVENT_WAIT_MIN) * unit * unit
}

#[derive(Debug)]
pub(super) struct DartingFlightState {
    habitat_center: Vec3,
    ground_height: Option<f32>,
    cruise_direction: Vec3,
    cruise_speed: f32,
    acceleration: Vec3,
    wind_velocity: Vec3,
    rhythm: SharedFlightRhythm,
    event_acceleration: Vec3,
    event_time_remaining: f32,
    time_until_event: f32,
    rapid_pulses_remaining: u8,
    turn_sign: f32,
    rng: SmallRng,
}

impl DartingFlightState {
    pub(super) fn new(seed: f32, habitat_center: Vec3, initial_direction: Vec3) -> Self {
        let mut rng =
            SmallRng::seed_from_u64(u64::from(seed.to_bits()).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let cruise_direction =
            Vec3::new(initial_direction.x, 0.0, initial_direction.z).normalize_or(Vec3::X);
        Self {
            habitat_center,
            ground_height: None,
            cruise_direction,
            cruise_speed: rng
                .random_range(BUTTERFLY_BLOCK_CRUISE_SPEED_MIN..=BUTTERFLY_BLOCK_CRUISE_SPEED_MAX),
            acceleration: Vec3::ZERO,
            wind_velocity: Vec3::ZERO,
            rhythm: SharedFlightRhythm::new(seed, habitat_center),
            event_acceleration: Vec3::ZERO,
            event_time_remaining: 0.0,
            time_until_event: rng.random_range(0.04..=0.30),
            rapid_pulses_remaining: 0,
            turn_sign: if rng.random_bool(0.5) { 1.0 } else { -1.0 },
            rng,
        }
    }

    fn start_maneuver(&mut self, velocity: Vec3) {
        let continuing_cluster = self.rapid_pulses_remaining > 0;
        if continuing_cluster {
            self.rapid_pulses_remaining -= 1;
            self.turn_sign = -self.turn_sign;
        } else {
            self.turn_sign = if self.rng.random_bool(0.5) { 1.0 } else { -1.0 };
            if self.rng.random_bool(0.28) {
                self.rapid_pulses_remaining = self.rng.random_range(1..=2);
            }
        }

        let current_direction = Vec3::new(velocity.x, 0.0, velocity.z)
            .normalize_or_zero()
            .lerp(self.cruise_direction, 0.35)
            .normalize_or(self.cruise_direction);
        let current_planar =
            Vec3::new(current_direction.x, 0.0, current_direction.z).normalize_or(Vec3::X);
        let turn_radians = self.turn_sign * self.rng.random_range(0.35..=1.75);
        let (sin_turn, cos_turn) = turn_radians.sin_cos();
        let turned_planar = Vec3::new(
            current_planar.x * cos_turn - current_planar.z * sin_turn,
            0.0,
            current_planar.x * sin_turn + current_planar.z * cos_turn,
        );
        let maneuver_direction = turned_planar.normalize_or(current_planar);
        self.event_acceleration = maneuver_direction * self.rng.random_range(0.75..=1.55);
        self.event_time_remaining = self
            .rng
            .random_range(BUTTERFLY_BLOCK_EVENT_DURATION_MIN..=BUTTERFLY_BLOCK_EVENT_DURATION_MAX);
        self.cruise_direction = current_direction
            .lerp(maneuver_direction, 0.65)
            .normalize_or(maneuver_direction);

        let gap = if self.rapid_pulses_remaining > 0 {
            self.rng
                .random_range(BUTTERFLY_BLOCK_RAPID_GAP_MIN..=BUTTERFLY_BLOCK_RAPID_GAP_MAX)
        } else {
            sample_event_wait(&mut self.rng)
        };
        self.time_until_event = self.event_time_remaining + gap;
    }

    fn habitat_recovery_acceleration(
        &self,
        position: Vec3,
        velocity: Vec3,
        emerging: bool,
    ) -> Vec3 {
        let offset = position - self.habitat_center;
        let planar_offset = Vec3::new(offset.x, 0.0, offset.z);
        let planar_distance = planar_offset.length();
        let mut recovery = Vec3::ZERO;
        if planar_distance > BUTTERFLY_BLOCK_HABITAT_RADIUS {
            let overshoot = planar_distance - BUTTERFLY_BLOCK_HABITAT_RADIUS;
            recovery -= planar_offset.normalize_or_zero() * (0.35 + overshoot * 3.0);
            recovery -= Vec3::new(velocity.x, 0.0, velocity.z) * 0.8;
        }
        if self.ground_height.is_some() && !emerging {
            // Soft terrain-relative attraction, not a position clamp. Birth can
            // still emerge through its plant before joining this flight band.
            recovery.y -= offset.y * 4.0 + velocity.y * 1.5;
        } else if offset.y.abs() > BUTTERFLY_BLOCK_HABITAT_HEIGHT {
            let overshoot = offset.y.abs() - BUTTERFLY_BLOCK_HABITAT_HEIGHT;
            recovery.y -= offset.y.signum() * (0.30 + overshoot * 3.5);
            recovery.y -= velocity.y * 0.8;
        }
        recovery
    }

    pub(super) fn resume(&mut self, velocity: Vec3) {
        self.acceleration = Vec3::ZERO;
        self.wind_velocity = Vec3::ZERO;
        self.cruise_direction =
            Vec3::new(velocity.x, 0.0, velocity.z).normalize_or(self.cruise_direction);
    }

    pub(super) fn reset_render_pose(&mut self, position: Vec3) {
        self.rhythm.reset(position);
    }

    pub(super) fn publish_render_pose(&mut self, position: Vec3) {
        self.rhythm.publish(position);
    }

    pub(super) fn render_position(&self) -> Vec3 {
        self.rhythm.position
    }

    pub(super) fn advance(
        &mut self,
        position: Vec3,
        velocity: Vec3,
        dt: f32,
        world_max: Vec3,
        emerging: bool,
        tuning: ButterflyFlightTuning,
        local_wind: Vec3,
        terrain_distance: &mut impl FnMut(Vec3, Vec3) -> Option<f32>,
    ) -> Vec3 {
        let tuning = tuning.sanitized();
        let max_speed = BUTTERFLY_BLOCK_MAX_SPEED * tuning.speed;
        let max_vertical_speed = BUTTERFLY_BLOCK_MAX_VERTICAL_SPEED * tuning.speed;
        let dt = dt.clamp(0.0, 0.1);
        if dt <= 0.0 {
            return velocity;
        }
        let vertical_acceleration = self
            .rhythm
            .advance(position, dt, tuning.flight_frequency_hz);
        if self.rhythm.publish_due {
            // Reuse the shared beat rather than adding a terrain or display clock.
            // The top-down CPU query sees terrain only, not the birth leaf/canopy.
            let origin = Vec3::new(position.x, world_max.y, position.z);
            if let Some(distance) = terrain_distance(origin, -Vec3::Y)
                .filter(|d| d.is_finite() && *d >= 0.0 && *d <= world_max.y)
            {
                self.ground_height = Some(world_max.y - distance);
            }
        }
        if let Some(ground) = self.ground_height {
            self.habitat_center.y = (ground + tuning.height_above_ground).min(world_max.y - 0.01);
        }

        // Self-speed acts on air-relative steering, never on wind advection.
        let air_velocity = velocity - self.wind_velocity;
        let wind_target = if local_wind.is_finite() {
            (Vec3::new(local_wind.x, 0.0, local_wind.z)
                * (WIND_DRIFT_WORLD_SPEED_PER_STRENGTH * tuning.wind_drift))
                .clamp_length_max(MAX_WIND_DRIFT_SPEED)
        } else {
            Vec3::ZERO
        };
        let wind_delta = (wind_target - self.wind_velocity) * (1.0 - (-dt / 0.25).exp());
        self.wind_velocity += wind_delta.clamp_length_max(MAX_WIND_DRIFT_ACCELERATION * dt);

        if !emerging {
            // Only maneuver clocks change tempo; physics and lifetime keep real time.
            let event_dt = dt * tuning.maneuver_tempo;
            self.time_until_event -= event_dt;
            self.event_time_remaining = (self.event_time_remaining - event_dt).max(0.0);
            if self.time_until_event <= 0.0 {
                self.start_maneuver(air_velocity);
            }
        }

        let mut cruise_velocity = if emerging {
            Vec3::Y * self.cruise_speed
        } else {
            self.cruise_direction * self.cruise_speed
        };
        cruise_velocity = (cruise_velocity * tuning.speed).clamp_length_max(max_speed);
        let cruise_acceleration = (cruise_velocity - air_velocity) * 2.4;
        let mut maneuver_acceleration = if !emerging && self.event_time_remaining > 0.0 {
            self.event_acceleration
        } else {
            Vec3::ZERO
        };
        if !emerging {
            maneuver_acceleration.y = vertical_acceleration;
        }
        maneuver_acceleration *= tuning.speed;
        maneuver_acceleration.y *= tuning.vertical_strength;
        let mut recovery = self.habitat_recovery_acceleration(position, velocity, emerging);
        for axis in 0..3 {
            let near_low = ((0.10 - position[axis]) / 0.10).clamp(0.0, 1.0);
            let near_high = ((position[axis] - world_max[axis] + 0.10) / 0.10).clamp(0.0, 1.0);
            recovery[axis] += (near_low - near_high) * 2.5;
        }
        if !emerging && velocity.length_squared() > 1e-6 {
            if terrain_distance(position, velocity.normalize())
                .is_some_and(|distance| distance < 0.07)
            {
                // Brake and climb before contact. Terrain remains the existing CPU authority.
                recovery += Vec3::Y * 1.5 - velocity * 8.0;
            }
        }
        let target_acceleration = (cruise_acceleration + maneuver_acceleration + recovery)
            .clamp_length_max(BUTTERFLY_BLOCK_MAX_ACCELERATION * tuning.speed);
        self.acceleration = approach_vec3(
            self.acceleration,
            target_acceleration,
            BUTTERFLY_BLOCK_MAX_JERK * tuning.speed * tuning.turn_sharpness * dt,
        );

        let mut next_velocity = (air_velocity + self.acceleration * dt).clamp_length_max(max_speed);
        next_velocity.y = next_velocity
            .y
            .clamp(-max_vertical_speed, max_vertical_speed);
        next_velocity += self.wind_velocity;
        // Contact constraints bound the next displacement, never relocate the particle.
        // A hard contact may interrupt acceleration continuity, as in an ordinary collision.
        for axis in 0..3 {
            let intended = next_velocity[axis];
            next_velocity[axis] = next_velocity[axis].clamp(
                -position[axis].max(0.0) / dt,
                (world_max[axis] - position[axis]).max(0.0) / dt,
            );
            if intended != next_velocity[axis] {
                self.wind_velocity[axis] *= next_velocity[axis] / intended;
            }
        }
        let distance = next_velocity.length() * dt;
        if !emerging && distance > 1e-7 {
            if let Some(hit_distance) = terrain_distance(position, next_velocity.normalize()) {
                let allowed = (hit_distance - 0.001).max(0.0);
                if allowed < distance {
                    next_velocity *= allowed / distance;
                    self.wind_velocity *= allowed / distance;
                }
            }
        }
        next_velocity
    }
}

fn approach_vec3(current: Vec3, target: Vec3, max_delta: f32) -> Vec3 {
    let delta = target - current;
    let distance = delta.length();
    if distance <= max_delta || distance <= f32::EPSILON {
        target
    } else {
        current + delta * (max_delta / distance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn butterfly_long_flights_return_to_player_height_over_local_terrain() {
        let ground = 0.25;
        for initial_y in [ground + 0.08, ground + 0.65] {
            for seed in 0..8 {
                let mut position = Vec3::new(1., initial_y, 1.);
                let mut velocity = Vec3::ZERO;
                let mut flight = DartingFlightState::new(seed as f32, position, Vec3::X);
                let tuning = ButterflyFlightTuning {
                    flight_frequency_hz: 5.5,
                    maneuver_tempo: 2.75,
                    vertical_strength: 2.,
                    ..Default::default()
                };
                let mut sum_height = 0.;
                let mut peak_height = 0.0_f32;
                for tick in 0..7200 {
                    velocity = flight.advance(
                        position,
                        velocity,
                        1. / 120.,
                        Vec3::splat(2.),
                        false,
                        tuning,
                        Vec3::ZERO,
                        &mut |origin, direction| {
                            (direction.y < -1e-6 && origin.y >= ground)
                                .then(|| (ground - origin.y) / direction.y)
                        },
                    );
                    let previous = position;
                    position += velocity / 120.;
                    flight.publish_render_pose(position);
                    assert!(position.y >= ground, "terrain containment");
                    assert!(position.distance(previous) < 0.003, "no height teleport");
                    if tick >= 3600 {
                        let height = position.y - ground;
                        sum_height += height;
                        peak_height = peak_height.max(height);
                    }
                }
                let mean_height = sum_height / 3600.;
                println!(
                    "height initial={initial_y} seed={seed} mean={mean_height} peak={peak_height}"
                );
                assert!(
                    (0.03..0.14).contains(&mean_height),
                    "must return near the 0.08-world-unit player eye height, got {mean_height}"
                );
                assert!(peak_height < 0.22, "must not remain in the canopy");
            }
        }
    }

    #[test]
    fn butterfly_height_control_tracks_sloped_terrain_and_live_edits_without_teleporting() {
        let mut position = Vec3::new(1., 0.31, 1.);
        let mut velocity = Vec3::ZERO;
        let mut flight = DartingFlightState::new(31., position, Vec3::X);
        let mut means = [0.; 3];
        for phase in 0..3 {
            let requested_height = [0.04, 0.18, 0.08][phase];
            for tick in 0..3600 {
                velocity = flight.advance(
                    position,
                    velocity,
                    1. / 120.,
                    Vec3::splat(2.),
                    false,
                    ButterflyFlightTuning {
                        height_above_ground: requested_height,
                        vertical_strength: 0.,
                        ..Default::default()
                    },
                    Vec3::new(0.4, 0., 0.),
                    &mut |origin, direction| {
                        let denominator = direction.y - direction.x * 0.03;
                        (denominator < -1e-6)
                            .then(|| (0.2 + 0.03 * origin.x - origin.y) / denominator)
                            .filter(|d| *d >= 0.)
                    },
                );
                let previous = position;
                position += velocity / 120.;
                flight.publish_render_pose(position);
                let height = position.y - (0.2 + 0.03 * position.x);
                assert!(height >= 0.);
                assert!(position.distance(previous) < 0.003);
                if tick >= 2400 {
                    means[phase] += height / 1200.;
                }
            }
            assert!(
                (means[phase] - requested_height).abs() < 0.02,
                "phase {phase}, heights {means:?}"
            );
        }
        assert!(
            means[1] - means[0] > 0.10,
            "the replacement height slider must change the actual trajectory"
        );
    }

    #[test]
    fn butterfly_vertical_intent_has_no_systematic_upward_bias() {
        let mut rhythm = SharedFlightRhythm::new(19., Vec3::ZERO);
        let mut sum = 0.;
        for _ in 0..100_000 {
            rhythm.sample_vertical_intent();
            sum += rhythm.acceleration;
        }
        assert!(
            (sum / 100_000.).abs() < 0.004,
            "signed bursts must not push long flights upward"
        );
    }

    #[test]
    fn butterfly_shared_frequency_publishes_pose_and_vertical_intent_on_the_same_beat() {
        let run = |frequency| {
            let mut rhythm = SharedFlightRhythm::new(17.0, Vec3::ZERO);
            let mut position = Vec3::ZERO;
            let mut events = Vec::new();
            for tick in 0..7200 {
                let previous_rng = rhythm.rng.clone().random::<u64>();
                let previous_pose = rhythm.position;
                rhythm.advance(position, FLIGHT_STEP_SECONDS as f32, frequency);
                assert_eq!(
                    rhythm.rng.clone().random::<u64>() != previous_rng,
                    rhythm.publish_due
                );
                position.x += 0.00001; // Real fixture trajectory, below the displacement guard.
                rhythm.publish(position);
                assert_eq!(
                    rhythm.position,
                    if rhythm.publish_due {
                        position
                    } else {
                        previous_pose
                    }
                );
                if rhythm.publish_due {
                    events.push((tick, rhythm.acceleration));
                }
            }
            events
        };
        let normal = run(10.0);
        let fast = run(40.0);
        assert_eq!(normal.len(), 600);
        assert_eq!(fast.len(), 2400);
        assert!(normal.windows(2).all(|pair| pair[1].0 - pair[0].0 == 12));
        assert!(fast.windows(2).all(|pair| pair[1].0 - pair[0].0 == 3));
        for ((_, normal_force), (_, fast_force)) in normal.iter().zip(&fast) {
            assert_eq!(normal_force, fast_force);
        }
        assert!(normal.iter().any(|(_, force)| *force > 0.4));
        assert!(normal.iter().any(|(_, force)| *force < -0.2));
    }

    #[test]
    fn butterfly_shared_frequency_edits_preserve_one_fractional_phase() {
        let mut rhythm = SharedFlightRhythm::new(17.0, Vec3::ZERO);
        let dt = FLIGHT_STEP_SECONDS as f32;
        rhythm.advance(Vec3::ZERO, dt, 10.0);
        for _ in 0..5 {
            rhythm.advance(Vec3::ZERO, dt, 10.0);
        }
        let phase = rhythm.phase;
        let force = rhythm.acceleration;
        rhythm.advance(Vec3::ZERO, dt, 20.0);
        assert!((rhythm.phase - phase - f64::from(dt) * 20.0).abs() < 1e-8);
        assert!(!rhythm.publish_due);
        assert_eq!(rhythm.acceleration, force);
        for _ in 0..3 {
            rhythm.advance(Vec3::ZERO, dt, 20.0);
        }
        assert!(rhythm.publish_due);
        assert_ne!(rhythm.acceleration, force);
    }

    #[test]
    fn butterfly_user_tuning_is_the_startup_and_sanitization_default() {
        let defaults = ButterflyFlightTuning::default();
        assert_eq!(defaults.speed, 0.35);
        assert_eq!(defaults.vertical_strength, 4.0);
        assert_eq!(defaults.flight_frequency_hz, 10.0);
        assert_eq!(
            ButterflyFlightTuning {
                flight_frequency_hz: 400.0,
                ..defaults
            }
            .sanitized()
            .flight_frequency_hz,
            40.0
        );
    }

    #[test]
    fn butterfly_shared_zero_is_continuous_and_reenable_starts_one_shared_beat() {
        let mut rhythm = SharedFlightRhythm::new(7.0, Vec3::ZERO);
        for tick in 0..240 {
            let position = Vec3::X * tick as f32 * 0.001;
            assert_eq!(
                rhythm.advance(position, FLIGHT_STEP_SECONDS as f32, 0.0),
                0.0
            );
            rhythm.publish(position);
            assert_eq!(rhythm.position, position);
        }
        let position = rhythm.position;
        assert_ne!(
            rhythm.advance(position, FLIGHT_STEP_SECONDS as f32, 10.0),
            0.0
        );
        assert!(rhythm.publish_due);
        rhythm.reset(position);
        assert_eq!(rhythm.acceleration, 0.0);
        assert_eq!(rhythm.phase, 0.0);
    }

    #[test]
    fn butterfly_vertical_and_horizontal_event_clocks_are_independent() {
        let run = |flight_frequency_hz, maneuver_tempo| {
            let mut state = DartingFlightState::new(3.0, Vec3::ONE, Vec3::X);
            for _ in 0..360 {
                // Fixed inputs isolate intent clocks from terrain and shared speed limits.
                state.advance(
                    Vec3::ONE,
                    Vec3::X * 0.1,
                    FLIGHT_STEP_SECONDS as f32,
                    Vec3::splat(2.0),
                    false,
                    ButterflyFlightTuning {
                        flight_frequency_hz,
                        maneuver_tempo,
                        ..Default::default()
                    },
                    Vec3::ZERO,
                    &mut |_, _| None,
                );
            }
            state
        };
        let mut low = run(0.25, 1.0);
        let mut high = run(40.0, 1.0);
        assert_eq!(low.time_until_event, high.time_until_event);
        assert_eq!(low.event_time_remaining, high.event_time_remaining);
        assert_eq!(low.event_acceleration, high.event_acceleration);
        assert_eq!(low.rng.random::<u64>(), high.rng.random::<u64>());
        assert_ne!(low.rhythm.acceleration, high.rhythm.acceleration);
        let mut lateral_fast = run(0.25, 4.0);
        assert_eq!(low.rhythm.phase, lateral_fast.rhythm.phase);
        assert_eq!(low.rhythm.acceleration, lateral_fast.rhythm.acceleration);
        assert_eq!(
            low.rhythm.rng.random::<u64>(),
            lateral_fast.rhythm.rng.random::<u64>()
        );
    }

    #[test]
    fn butterfly_wind_drift_is_independent_of_self_speed_and_has_its_own_gain() {
        let run = |speed, wind_drift| {
            let mut state = DartingFlightState::new(5.0, Vec3::ONE, Vec3::X);
            // Isolate environmental drift without a competing steering event.
            state.cruise_speed = 0.0;
            state.time_until_event = 100.0;
            let mut position = Vec3::ONE;
            let mut velocity = Vec3::ZERO;
            let dt = FLIGHT_STEP_SECONDS as f32;
            for _ in 0..60 {
                let previous_wind = state.wind_velocity;
                velocity = state.advance(
                    position,
                    velocity,
                    dt,
                    Vec3::splat(2.0),
                    false,
                    ButterflyFlightTuning {
                        speed,
                        wind_drift,
                        flight_frequency_hz: 0.0,
                        ..Default::default()
                    },
                    Vec3::X * 2.0,
                    &mut |_, _| None,
                );
                assert!(
                    (state.wind_velocity - previous_wind).length()
                        <= MAX_WIND_DRIFT_ACCELERATION * dt + 1e-6
                );
                position += velocity * dt;
                state.publish_render_pose(position);
            }
            (position, velocity)
        };
        let slow = run(0.25, 1.0);
        let fast = run(2.5, 1.0);
        assert!(slow.0.distance(fast.0) < 1e-6);
        assert!(slow.1.distance(fast.1) < 1e-6);
        assert!(
            slow.1.x > BUTTERFLY_BLOCK_MAX_SPEED * 0.25,
            "wind must not be capped by self-speed"
        );
        assert_eq!(run(0.25, 0.0), (Vec3::ONE, Vec3::ZERO));
        let stronger = run(0.25, 2.0);
        assert!(stronger.1.x > slow.1.x * 1.5);
    }

    #[test]
    fn butterfly_strong_wind_reversal_and_decay_are_bounded() {
        let mut state = DartingFlightState::new(2.0, Vec3::ONE, Vec3::X);
        let tuning = ButterflyFlightTuning {
            speed: 2.5,
            wind_drift: 3.0,
            ..Default::default()
        };
        let mut position = Vec3::ONE;
        let mut velocity = Vec3::ZERO;
        let dt = FLIGHT_STEP_SECONDS as f32;
        for tick in 0..2400 {
            let wind = match (tick / 120) % 3 {
                0 => Vec3::X * 100.0,
                1 => -Vec3::X * 100.0,
                _ => Vec3::splat(f32::NAN),
            };
            velocity = state.advance(
                position,
                velocity,
                dt,
                Vec3::splat(2.0),
                false,
                tuning,
                wind,
                &mut |_, _| None,
            );
            position += velocity * dt;
            state.publish_render_pose(position);
            assert!(position.is_finite() && velocity.is_finite());
            assert!(position.cmpge(Vec3::ZERO).all() && position.cmple(Vec3::splat(2.0)).all());
            assert!(state.wind_velocity.length() <= MAX_WIND_DRIFT_SPEED + 1e-6);
            assert!(velocity.length() <= 1.2 + 1e-6);
        }
        // Calming wind releases smoothly instead of retaining permanent advection.
        for _ in 0..180 {
            velocity = state.advance(
                position,
                velocity,
                dt,
                Vec3::splat(2.0),
                false,
                tuning,
                Vec3::ZERO,
                &mut |_, _| None,
            );
            position += velocity * dt;
            state.publish_render_pose(position);
        }
        assert!(state.wind_velocity.length() < 0.002);
    }

    #[test]
    fn butterfly_live_knobs_change_their_intended_motion_terms() {
        let run = |tuning: ButterflyFlightTuning| {
            let mut state = DartingFlightState::new(9.0, Vec3::ONE, Vec3::X);
            state.event_acceleration = Vec3::new(0.4, 0.0, 0.2);
            state.rhythm.acceleration = 0.5;
            state.rhythm.frequency_hz = tuning.flight_frequency_hz;
            state.event_time_remaining = 0.2;
            state.time_until_event = 0.6;
            let velocity = state.advance(
                Vec3::ONE,
                Vec3::ZERO,
                FLIGHT_STEP_SECONDS as f32,
                Vec3::splat(2.0),
                false,
                tuning,
                Vec3::ZERO,
                &mut |_, _| None,
            );
            (velocity, state.time_until_event)
        };
        let baseline = ButterflyFlightTuning {
            vertical_strength: 1.0,
            ..Default::default()
        };
        let (velocity, remaining) = run(baseline);
        assert!(
            run(ButterflyFlightTuning {
                vertical_strength: 4.0,
                ..baseline
            })
            .0
            .y > velocity.y
        );
        assert!(
            run(ButterflyFlightTuning {
                turn_sharpness: 4.0,
                ..baseline
            })
            .0
            .length()
                > velocity.length() * 2.0
        );
        assert!(
            run(ButterflyFlightTuning {
                speed: 2.5,
                ..baseline
            })
            .0
            .length()
                > velocity.length() * 2.0
        );
        assert!(
            run(ButterflyFlightTuning {
                maneuver_tempo: 4.0,
                ..baseline
            })
            .1 < remaining
        );
    }

    #[test]
    fn butterfly_tuning_extremes_and_live_edits_stay_finite_and_in_world() {
        assert_eq!(
            ButterflyFlightTuning {
                flight_frequency_hz: f32::NAN,
                height_above_ground: f32::NAN,
                maneuver_tempo: f32::INFINITY,
                vertical_strength: f32::NAN,
                turn_sharpness: f32::NEG_INFINITY,
                speed: f32::NAN,
                wind_drift: f32::NAN,
            }
            .sanitized(),
            ButterflyFlightTuning::default()
        );
        let dt = FLIGHT_STEP_SECONDS as f32;
        for seed in 0..4 {
            let mut state = DartingFlightState::new(seed as f32, Vec3::ONE, Vec3::X);
            let mut position = Vec3::ONE;
            let mut velocity = Vec3::X * 0.15;
            for tick in 0..2400 {
                let tuning = if (tick / 240) % 2 == 0 {
                    ButterflyFlightTuning {
                        flight_frequency_hz: 40.0,
                        height_above_ground: 0.24,
                        maneuver_tempo: 4.0,
                        vertical_strength: 4.0,
                        turn_sharpness: 4.0,
                        speed: 2.5,
                        wind_drift: 3.0,
                    }
                } else {
                    ButterflyFlightTuning {
                        flight_frequency_hz: 0.0,
                        height_above_ground: 0.03,
                        maneuver_tempo: 0.25,
                        vertical_strength: 0.0,
                        turn_sharpness: 0.25,
                        speed: 0.25,
                        wind_drift: 0.0,
                    }
                };
                velocity = state.advance(
                    position,
                    velocity,
                    dt,
                    Vec3::splat(2.0),
                    false,
                    tuning,
                    Vec3::ZERO,
                    &mut |_, _| None,
                );
                position += velocity * dt;
                state.publish_render_pose(position);
                assert!(position.is_finite() && velocity.is_finite());
                assert!(position.cmpge(Vec3::ZERO).all() && position.cmple(Vec3::splat(2.0)).all());
                assert!(velocity.length() <= 0.75 + 1e-6);
                assert!(state.acceleration.length() <= 4.5 + 1e-5);
            }
        }
    }

    #[test]
    fn butterfly_displacement_guard_advances_intent_and_pose_together() {
        let mut rhythm = SharedFlightRhythm::new(7.0, Vec3::ZERO);
        let dt = FLIGHT_STEP_SECONDS as f32;
        let mut position = Vec3::ZERO;
        let mut guarded_beats = 0;
        for _ in 0..240 {
            let old_rng = rhythm.rng.clone().random::<u64>();
            rhythm.advance(position, dt, 0.25);
            if rhythm.publish_due {
                guarded_beats += 1;
            }
            assert_eq!(
                rhythm.publish_due,
                rhythm.rng.clone().random::<u64>() != old_rng
            );
            position.x += 0.3 * dt;
            rhythm.publish(position);
            assert!(rhythm.position.distance(position) <= 0.025 + 0.3 * dt + 1e-6);
        }
        assert!(
            guarded_beats > 10,
            "slow cadence must retain the displacement guard"
        );
    }

    #[test]
    fn seeded_flight_is_continuous_bounded_and_has_irregular_events() {
        let dt = FLIGHT_STEP_SECONDS as f32;
        for seed in 0..12 {
            let center = Vec3::splat(1.0);
            let mut flight = DartingFlightState::new(seed as f32, center, Vec3::X);
            let mut position = center;
            let mut velocity = Vec3::X * (0.15 * ButterflyFlightTuning::default().speed);
            let mut intervals = std::collections::BTreeSet::new();
            let mut last_event = 0;
            let mut max_offset = 0.0_f32;
            for tick in 0..3600 {
                let old_acceleration = flight.acceleration;
                let event_due = flight.time_until_event <= dt;
                let next = flight.advance(
                    position,
                    velocity,
                    dt,
                    Vec3::splat(2.0),
                    false,
                    ButterflyFlightTuning::default(),
                    Vec3::ZERO,
                    &mut |_, _| None,
                );
                assert!(next.is_finite());
                assert!(next.length() <= BUTTERFLY_BLOCK_MAX_SPEED + 1e-6);
                assert!(next.y.abs() <= BUTTERFLY_BLOCK_MAX_VERTICAL_SPEED + 1e-6);
                assert!(flight.acceleration.length() <= BUTTERFLY_BLOCK_MAX_ACCELERATION + 1e-5);
                assert!(
                    (flight.acceleration - old_acceleration).length()
                        <= BUTTERFLY_BLOCK_MAX_JERK * dt + 1e-5
                );
                assert!((next - velocity).length() <= BUTTERFLY_BLOCK_MAX_ACCELERATION * dt + 1e-5);
                position += next * dt;
                flight.publish_render_pose(position);
                velocity = next;
                max_offset = max_offset.max(position.distance(center));
                assert!(position.cmpge(Vec3::ZERO).all() && position.cmple(Vec3::splat(2.0)).all());
                if event_due {
                    intervals.insert(tick - last_event);
                    last_event = tick;
                }
            }
            assert!(intervals.len() >= 12, "seed {seed}: periodic event spacing");
            assert!(
                max_offset < 0.65,
                "seed {seed}: escaped habitat by {max_offset}"
            );
        }
    }

    #[test]
    fn world_and_terrain_contacts_never_teleport_or_cross_the_swept_segment() {
        let dt = FLIGHT_STEP_SECONDS as f32;
        let mut state = DartingFlightState::new(5.0, Vec3::ONE, Vec3::X);
        let position = Vec3::new(1.9999, 1.0, 1.0);
        let velocity = state.advance(
            position,
            Vec3::X * 0.3,
            dt,
            Vec3::splat(2.0),
            false,
            ButterflyFlightTuning::default(),
            Vec3::ZERO,
            &mut |_, _| None,
        );
        assert!((position + velocity * dt).x <= 2.0);
        let position = Vec3::ONE;
        let velocity = state.advance(
            position,
            Vec3::X * 0.3,
            dt,
            Vec3::splat(2.0),
            false,
            ButterflyFlightTuning::default(),
            Vec3::ZERO,
            &mut |_, _| Some(0.0011),
        );
        assert!((velocity * dt).length() <= 0.000101);
    }
}
