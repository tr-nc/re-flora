//! Experimental rigid square flight; only detached, falling leaves opt in.
//! Phenomenological 3-D extension of the lift/drag/rotation coupling discussed in
//! docs/research/fallen_leaf_flight.md, not a fitted model of a particular species.
use glam::{Quat, Vec3};

use super::STANDARD_PARTICLE_SIZE;

const STEP: f32 = 1.0 / 120.0;
const MAX_CATCHUP: f32 = 0.25;
pub(super) const MAX_ANGULAR_SPEED: f32 = 12.0;
const MAX_AIR_SPEED: f32 = 0.75;
const GRAVITY: f32 = 0.22;
const EDGE_DRAG: f32 = 7.0;
const BROADSIDE_DRAG: f32 = 70.0;
const TRANSLATIONAL_LIFT: f32 = 45.0;
const ROTATIONAL_LIFT: f32 = 0.65;
const PITCH_TORQUE: f32 = 900.0;
/// Existing field strength is artistic, not a speed in world units per second.
const WIND_TO_WORLD_SPEED: f32 = 0.035;

#[derive(Clone, Copy, Debug)]
pub(super) struct LeafFlight {
    pub orientation: Quat,
    angular_velocity: Vec3,
    remainder: f32,
}

impl LeafFlight {
    pub fn new(seed: u32) -> Self {
        fn unit(mut x: u32) -> f32 {
            x = (x ^ (x >> 16)).wrapping_mul(0x7feb_352d);
            x = (x ^ (x >> 15)).wrapping_mul(0x846c_a68b);
            ((x ^ (x >> 16)) & 65535) as f32 / 65535.0
        }
        let yaw = unit(seed) * std::f32::consts::TAU;
        let tilt = (unit(seed ^ 0xa511_e9b3) - 0.5) * 2.4;
        let orientation =
            Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2 + tilt);
        Self {
            orientation,
            // Seeded release conditions, not a continuously prescribed rotation.
            angular_velocity: orientation
                * Vec3::new(0.6 + unit(seed ^ 0x63d8_3595) * 2.4, 0.25, 0.35),
            remainder: 0.0,
        }
    }

    pub fn advance(
        &mut self,
        position: &mut Vec3,
        velocity: &mut Vec3,
        dt: f32,
        size: f32,
        gravity_factor: f32,
        wind_factor: f32,
        sample_wind: impl Fn(Vec3) -> Vec3,
    ) {
        // A suspended window does not cause an unbounded catch-up loop. Lifetime
        // still advances by the caller's full dt; only mechanical catch-up is capped.
        self.remainder += dt.clamp(0.0, MAX_CATCHUP);
        let size_ratio = (size / STANDARD_PARTICLE_SIZE).clamp(0.25, 8.0);
        while self.remainder + 1.0e-7 >= STEP {
            self.remainder = (self.remainder - STEP).max(0.0);
            let air = (sample_wind(*position) * (WIND_TO_WORLD_SPEED * wind_factor))
                .clamp_length_max(MAX_AIR_SPEED);
            let normal = self.orientation * Vec3::Z;
            let relative = (*velocity - air).clamp_length_max(2.0);
            let pitch = normal.cross(relative) * relative.dot(normal);
            self.angular_velocity += pitch * (PITCH_TORQUE / size_ratio * STEP);
            let angular_drag = 0.4 + 0.15 * self.angular_velocity.length();
            self.angular_velocity /= 1.0 + angular_drag * STEP;
            self.angular_velocity = self.angular_velocity.clamp_length_max(MAX_ANGULAR_SPEED);

            let previous_velocity = *velocity;
            *velocity = air
                + advance_relative_velocity(
                    relative,
                    normal,
                    self.angular_velocity,
                    size_ratio,
                    gravity_factor,
                    STEP,
                );
            *position += (previous_velocity + *velocity) * (0.5 * STEP);
            self.orientation = (Quat::from_scaled_axis(self.angular_velocity * STEP)
                * self.orientation)
                .normalize();
        }
    }

    pub fn settle(&mut self, dt: f32) {
        // The existing sinking lifecycle owns translation once a leaf expires.
        self.angular_velocity *= (-3.0 * dt).exp();
        self.remainder = 0.0;
    }
}

fn advance_relative_velocity(
    mut relative: Vec3,
    normal: Vec3,
    omega: Vec3,
    size_ratio: f32,
    gravity_factor: f32,
    dt: f32,
) -> Vec3 {
    let gravity = Vec3::NEG_Y * (GRAVITY * gravity_factor);
    relative += gravity * (0.5 * dt);
    let speed = relative.length();
    if speed > 1.0e-7 {
        let direction = relative / speed;
        let incidence = normal.dot(direction);
        // Both lift terms are B cross u. Rotating u integrates this frozen lift
        // without adding kinetic energy; explicit Euler would spuriously add it.
        let circulation = normal.cross(direction) * (TRANSLATIONAL_LIFT * normal.dot(relative))
            + (omega - normal * omega.dot(normal)) * (ROTATIONAL_LIFT * size_ratio);
        relative = Quat::from_scaled_axis(circulation * dt) * relative;
        let drag = EDGE_DRAG + (BROADSIDE_DRAG - EDGE_DRAG) * incidence * incidence;
        relative /= 1.0 + drag * speed * dt;
    }
    relative + gravity * (0.5 * dt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_pose_fall(normal: Vec3) -> (Vec3, Vec3) {
        let mut position = Vec3::ZERO;
        let mut velocity = Vec3::ZERO;
        for _ in 0..1200 {
            velocity = advance_relative_velocity(velocity, normal, Vec3::ZERO, 1., 1., STEP);
            position += velocity * STEP;
        }
        (position, velocity)
    }

    #[test]
    fn broadside_falls_slower_than_edge_on_in_still_air() {
        let (_, broad) = fixed_pose_fall(Vec3::Y);
        let (_, edge) = fixed_pose_fall(Vec3::X);
        assert!(broad.y < 0. && edge.y < broad.y * 2.5, "{broad:?} {edge:?}");
    }

    #[test]
    fn tilted_plates_glide_in_opposite_directions() {
        let (left, _) = fixed_pose_fall(Vec3::new(-0.6, 0.8, 0.));
        let (right, _) = fixed_pose_fall(Vec3::new(0.6, 0.8, 0.));
        assert!(right.x > 0.1 && left.x < -0.1, "{left:?} {right:?}");
        assert!((left.x + right.x).abs() < 1e-5);
        assert!((left.y - right.y).abs() < 1e-5);
    }

    #[test]
    fn rotation_produces_lateral_velocity_even_at_broadside() {
        let input = Vec3::NEG_Y * 0.1;
        let still = advance_relative_velocity(input, Vec3::Y, Vec3::ZERO, 1., 1., STEP);
        let turning = advance_relative_velocity(input, Vec3::Y, Vec3::Z * 3., 1., 1., STEP);
        assert!(still.x.abs() < 1e-7 && turning.x > 0.001);
        let reversed = advance_relative_velocity(input, Vec3::Y, Vec3::NEG_Z * 3., 1., 1., STEP);
        assert!((turning.x + reversed.x).abs() < 1e-7);
    }

    fn simulate(fps: u32, seconds: u32) -> (Vec3, Vec3, LeafFlight) {
        let mut flight = LeafFlight::new(217);
        let mut position = Vec3::Y;
        let mut velocity = Vec3::new(0.06, 0., -0.03);
        for _ in 0..fps * seconds {
            flight.advance(
                &mut position,
                &mut velocity,
                1. / fps as f32,
                STANDARD_PARTICLE_SIZE,
                1.,
                1.,
                |_| Vec3::new(0.7, 0., 0.2),
            );
        }
        (position, velocity, flight)
    }

    #[test]
    fn fixed_substeps_are_frame_rate_independent() {
        let reference = simulate(120, 12);
        for fps in [20, 30, 60, 144, 240] {
            let actual = simulate(fps, 12);
            assert!(actual.0.distance(reference.0) < 2e-5, "fps={fps}");
            assert!(actual.1.distance(reference.1) < 2e-5, "fps={fps}");
            assert!(actual.2.orientation.dot(reference.2.orientation).abs() > 0.99999);
        }
    }

    #[test]
    fn long_fall_has_bounded_rotation_and_coupled_sideways_motion() {
        let mut flight = LeafFlight::new(217);
        let mut position = Vec3::Y;
        let mut velocity = Vec3::ZERO;
        let mut min_y = 1.0_f32;
        let mut max_y = -1.0_f32;
        let mut peak_omega = 0.0_f32;
        let mut max_side = 0.0_f32;
        for _ in 0..120 * 120 {
            flight.advance(
                &mut position,
                &mut velocity,
                STEP,
                STANDARD_PARTICLE_SIZE,
                1.,
                1.,
                |_| Vec3::ZERO,
            );
            let normal_y = (flight.orientation * Vec3::Z).y;
            min_y = min_y.min(normal_y);
            max_y = max_y.max(normal_y);
            peak_omega = peak_omega.max(flight.angular_velocity.length());
            max_side = max_side.max(velocity.with_y(0.).length());
            assert!(position.is_finite() && velocity.is_finite());
            assert!((flight.orientation.length() - 1.).abs() < 1e-5);
            assert!(velocity.length() < 0.5);
        }
        eprintln!(
            "leaf flight: normal_y={min_y}..{max_y} peak_omega={peak_omega} max_side={max_side}"
        );
        assert!(max_y - min_y > 0.5);
        assert!(max_side > 0.015);
        assert!(
            peak_omega < MAX_ANGULAR_SPEED * 0.95,
            "ordinary flight must not ride the safety cap"
        );
        assert!(position.y < 0.);
    }

    #[test]
    fn zero_relative_flow_has_no_aerodynamic_acceleration() {
        assert_eq!(
            advance_relative_velocity(Vec3::ZERO, Vec3::Y, Vec3::X, 1., 0., STEP),
            Vec3::ZERO
        );
        let v = Vec3::new(0.07, -0.1, 0.03);
        let n = Vec3::new(0.6, 0.8, 0.);
        let front = advance_relative_velocity(v, n, Vec3::Z, 1., 0., STEP);
        let back = advance_relative_velocity(v, -n, Vec3::Z, 1., 0., STEP);
        assert!(front.distance(back) < 1e-7);
        assert!(
            front.length() < v.length(),
            "lift and drag must not add relative energy"
        );
    }

    #[test]
    fn gusts_and_long_frames_stay_finite() {
        let mut flight = LeafFlight::new(97);
        let mut position = Vec3::Y;
        let mut velocity = Vec3::ZERO;
        for i in 0..2000 {
            let sign = if i % 2 == 0 { 1. } else { -1. };
            flight.advance(
                &mut position,
                &mut velocity,
                2.,
                STANDARD_PARTICLE_SIZE,
                1.,
                1.4,
                |_| Vec3::X * (1000. * sign),
            );
            assert!(position.is_finite() && velocity.is_finite());
            assert!(flight.orientation.is_finite());
            assert!(flight.angular_velocity.length() <= MAX_ANGULAR_SPEED + 1e-5);
        }
    }
}
