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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ButterflyFlightVariant {
    OriginalSprite,
    #[default]
    DartingBlock,
}

impl ButterflyFlightVariant {
    pub const fn is_darting_block(self) -> bool {
        matches!(self, Self::DartingBlock)
    }
}

/// Live B-only art controls. Self propulsion and environmental drift are independent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButterflyFlightTuning {
    pub position_step_ms: f32,
    pub maneuver_tempo: f32,
    pub vertical_strength: f32,
    pub turn_sharpness: f32,
    pub speed: f32,
    pub wind_drift: f32,
}

impl Default for ButterflyFlightTuning {
    fn default() -> Self {
        Self {
            position_step_ms: 60.0,
            maneuver_tempo: 1.0,
            vertical_strength: 1.0,
            turn_sharpness: 1.0,
            speed: 0.65,
            wind_drift: 1.0,
        }
    }
}

impl ButterflyFlightTuning {
    pub const POSITION_STEP_RANGE: std::ops::RangeInclusive<f32> = 0.0..=160.0;
    pub const TEMPO_RANGE: std::ops::RangeInclusive<f32> = 0.25..=4.0;
    pub const VERTICAL_RANGE: std::ops::RangeInclusive<f32> = 0.0..=4.0;
    pub const SHARPNESS_RANGE: std::ops::RangeInclusive<f32> = 0.25..=4.0;
    pub const SPEED_RANGE: std::ops::RangeInclusive<f32> = 0.25..=2.5;
    pub const WIND_DRIFT_RANGE: std::ops::RangeInclusive<f32> = 0.0..=3.0;

    pub fn sanitized(self) -> Self {
        let bounded = |value: f32, range: std::ops::RangeInclusive<f32>| {
            if value.is_finite() {
                value.clamp(*range.start(), *range.end())
            } else {
                1.0
            }
        };
        Self {
            position_step_ms: if self.position_step_ms.is_finite() {
                self.position_step_ms.clamp(0.0, 160.0)
            } else {
                Self::default().position_step_ms
            },
            maneuver_tempo: bounded(self.maneuver_tempo, Self::TEMPO_RANGE),
            vertical_strength: bounded(self.vertical_strength, Self::VERTICAL_RANGE),
            turn_sharpness: bounded(self.turn_sharpness, Self::SHARPNESS_RANGE),
            speed: if self.speed.is_finite() {
                bounded(self.speed, Self::SPEED_RANGE)
            } else {
                Self::default().speed
            },
            wind_drift: bounded(self.wind_drift, Self::WIND_DRIFT_RANGE),
        }
    }
}

/// Sample-and-hold of the real trajectory, not a second simulation or position noise.
#[derive(Debug)]
pub(super) struct SteppedFlightPose {
    pub(super) position: Vec3,
    remaining: f32,
    interval_ms: f32,
    rng: SmallRng,
}

impl SteppedFlightPose {
    pub(super) fn new(seed: f32, position: Vec3) -> Self {
        Self {
            position,
            remaining: 0.0,
            interval_ms: -1.0,
            rng: SmallRng::seed_from_u64(u64::from(seed.to_bits()) ^ 0x706f_7365_5f73_7465),
        }
    }

    pub(super) fn reset(&mut self, position: Vec3) {
        self.position = position;
        self.remaining = 0.0;
    }

    pub(super) fn advance(&mut self, position: Vec3, dt: f32, interval_ms: f32) {
        self.remaining -= dt;
        if interval_ms != self.interval_ms
            || interval_ms <= 0.0
            || self.remaining <= 0.0
            || self.position.distance_squared(position) >= 0.025_f32.powi(2)
        {
            self.position = position;
            self.interval_ms = interval_ms;
            // Different individuals have different, slightly irregular publication times.
            // Independent RNG means changing this knob cannot change the flight/population.
            self.remaining = interval_ms * 0.001 * self.rng.random_range(0.8..=1.2);
        }
    }
}

#[derive(Debug)]
pub(super) struct DartingFlightState {
    habitat_center: Vec3,
    cruise_direction: Vec3,
    cruise_speed: f32,
    acceleration: Vec3,
    wind_velocity: Vec3,
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
        let cruise_direction = initial_direction.normalize_or(Vec3::Y);
        Self {
            habitat_center,
            cruise_direction,
            cruise_speed: rng
                .random_range(BUTTERFLY_BLOCK_CRUISE_SPEED_MIN..=BUTTERFLY_BLOCK_CRUISE_SPEED_MAX),
            acceleration: Vec3::ZERO,
            wind_velocity: Vec3::ZERO,
            event_acceleration: Vec3::ZERO,
            event_time_remaining: 0.0,
            time_until_event: rng.random_range(0.04..=0.30),
            rapid_pulses_remaining: 0,
            turn_sign: if rng.random_bool(0.5) { 1.0 } else { -1.0 },
            rng,
        }
    }

    fn sample_event_wait(&mut self) -> f32 {
        let unit = self.rng.random_range(0.0..=1.0_f32);
        BUTTERFLY_BLOCK_EVENT_WAIT_MIN
            + (BUTTERFLY_BLOCK_EVENT_WAIT_MAX - BUTTERFLY_BLOCK_EVENT_WAIT_MIN) * unit * unit
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

        let current_direction = velocity
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
        let vertical_roll = self.rng.random_range(0.0..1.0_f32);
        let vertical = if vertical_roll < 0.13 {
            self.rng.random_range(0.55..=0.95)
        } else if vertical_roll < 0.21 {
            -self.rng.random_range(0.35..=0.70)
        } else {
            self.rng.random_range(-0.18..=0.22)
        };
        let maneuver_direction = (turned_planar + Vec3::Y * vertical).normalize_or(current_planar);
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
            self.sample_event_wait()
        };
        self.time_until_event = self.event_time_remaining + gap;
    }

    fn habitat_recovery_acceleration(&self, position: Vec3, velocity: Vec3) -> Vec3 {
        let offset = position - self.habitat_center;
        let planar_offset = Vec3::new(offset.x, 0.0, offset.z);
        let planar_distance = planar_offset.length();
        let mut recovery = Vec3::ZERO;
        if planar_distance > BUTTERFLY_BLOCK_HABITAT_RADIUS {
            let overshoot = planar_distance - BUTTERFLY_BLOCK_HABITAT_RADIUS;
            recovery -= planar_offset.normalize_or_zero() * (0.35 + overshoot * 3.0);
            recovery -= Vec3::new(velocity.x, 0.0, velocity.z) * 0.8;
        }
        if offset.y.abs() > BUTTERFLY_BLOCK_HABITAT_HEIGHT {
            let overshoot = offset.y.abs() - BUTTERFLY_BLOCK_HABITAT_HEIGHT;
            recovery.y -= offset.y.signum() * (0.30 + overshoot * 3.5);
            recovery.y -= velocity.y * 0.8;
        }
        recovery
    }

    pub(super) fn resume(&mut self, velocity: Vec3) {
        self.acceleration = Vec3::ZERO;
        self.wind_velocity = Vec3::ZERO;
        self.cruise_direction = velocity.normalize_or(self.cruise_direction);
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
        if !emerging {
            cruise_velocity.y *= tuning.vertical_strength;
        }
        cruise_velocity = (cruise_velocity * tuning.speed).clamp_length_max(max_speed);
        let cruise_acceleration = (cruise_velocity - air_velocity) * 2.4;
        let mut maneuver_acceleration = if !emerging && self.event_time_remaining > 0.0 {
            self.event_acceleration
        } else {
            Vec3::ZERO
        };
        maneuver_acceleration *= tuning.speed;
        maneuver_acceleration.y *= tuning.vertical_strength;
        let mut recovery = self.habitat_recovery_acceleration(position, velocity);
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
        }
        assert!(state.wind_velocity.length() < 0.002);
    }

    #[test]
    fn butterfly_live_knobs_change_their_intended_motion_terms() {
        let run = |tuning| {
            let mut state = DartingFlightState::new(9.0, Vec3::ONE, Vec3::X);
            state.event_acceleration = Vec3::new(0.4, 0.5, 0.2);
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
        let baseline = ButterflyFlightTuning::default();
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
                position_step_ms: f32::NAN,
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
                        position_step_ms: 160.0,
                        maneuver_tempo: 4.0,
                        vertical_strength: 4.0,
                        turn_sharpness: 4.0,
                        speed: 2.5,
                        wind_drift: 3.0,
                    }
                } else {
                    ButterflyFlightTuning {
                        position_step_ms: 0.0,
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
                assert!(position.is_finite() && velocity.is_finite());
                assert!(position.cmpge(Vec3::ZERO).all() && position.cmple(Vec3::splat(2.0)).all());
                assert!(velocity.length() <= 0.75 + 1e-6);
                assert!(state.acceleration.length() <= 4.5 + 1e-5);
            }
        }
    }

    #[test]
    fn butterfly_pose_steps_sample_real_positions_with_bounded_irregular_holds() {
        let mut pose = SteppedFlightPose::new(7.0, Vec3::ZERO);
        let dt = FLIGHT_STEP_SECONDS as f32;
        let mut position = Vec3::ZERO;
        let mut intervals = std::collections::BTreeSet::new();
        let mut last_publication = 0;
        for tick in 1..=240 {
            position.x += 0.3 * dt;
            let previous = pose.position;
            pose.advance(position, dt, 60.0);
            if pose.position != previous {
                assert_eq!(pose.position, position);
                intervals.insert(tick - last_publication);
                last_publication = tick;
            }
            assert!(pose.position.distance(position) < 0.025 + 1e-6);
        }
        assert!(intervals.len() >= 3);
        assert!(*intervals.iter().max().unwrap() <= 9);
        pose.advance(position, dt, 0.0);
        assert_eq!(pose.position, position);
    }

    #[test]
    fn seeded_flight_is_continuous_bounded_and_has_irregular_events() {
        let dt = FLIGHT_STEP_SECONDS as f32;
        for seed in 0..12 {
            let center = Vec3::splat(1.0);
            let mut flight = DartingFlightState::new(seed as f32, center, Vec3::X);
            let mut position = center;
            let mut velocity = Vec3::X * 0.15;
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
