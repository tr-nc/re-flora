//! Shared approved wing kinematics and bounded, art-directed flight coupling.
//! This is a phase-dependent perturbation of the existing mean flight controller,
//! not a dimensional lift/drag solver. See docs/research/butterfly_wing_motion_coupling.md.
use glam::{Quat, Vec3};
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Animation {
    source_fps: usize,
    keys: Vec<[f32; 3]>,
    #[serde(skip)]
    stroke_integrals: Vec<(f32, f32)>,
}

fn animation() -> &'static Animation {
    static ANIMATION: OnceLock<Animation> = OnceLock::new();
    ANIMATION.get_or_init(|| {
        let mut animation: Animation =
            serde_json::from_str(include_str!("../../assets/butterfly/wing-mesh.json"))
                .expect("embedded approved wing animation");
        assert_eq!(animation.keys.len(), animation.source_fps + 1);
        let mut integral = (0., 0.);
        animation.stroke_integrals.push(integral);
        for keys in animation.keys.windows(2) {
            let delta = keys[1][0] - keys[0][0];
            integral.0 += (-delta).max(0.);
            integral.1 += delta.max(0.);
            animation.stroke_integrals.push(integral);
        }
        for value in &mut animation.stroke_integrals {
            value.0 /= integral.0.max(1e-6);
            value.1 /= integral.1.max(1e-6);
        }
        animation
    })
}

/// One authored cycle per second, independently of source or display FPS.
pub(crate) fn wing_pose(phase: f32) -> [f32; 3] {
    let a = animation();
    let key = phase.rem_euclid(1.) * a.source_fps as f32;
    let i = (key.floor() as usize).min(a.source_fps - 1);
    std::array::from_fn(|c| a.keys[i][c] + (a.keys[i + 1][c] - a.keys[i][c]) * key.fract())
}

/// Positive wing rotation raises the right wing in the exported Y-up geometry.
/// Normalize each stroke's angular travel to a cycle mean of one. Using the
/// actual key slopes keeps force and artwork aligned, including wrap-around.
#[cfg(test)]
fn stroke_pulses(phase: f32) -> (f32, f32) {
    let a = animation();
    let i = ((phase.rem_euclid(1.) * a.source_fps as f32).floor() as usize).min(a.source_fps - 1);
    let start = a.stroke_integrals[i];
    let end = a.stroke_integrals[i + 1];
    (
        (end.0 - start.0) * a.source_fps as f32,
        (end.1 - start.1) * a.source_fps as f32,
    )
}

// Integrate the piecewise-linear animation exactly across key boundaries. Point
// sampling 25 source intervals at 120 Hz otherwise biases the per-cycle impulse.
fn stroke_integral(phase: f64) -> (f64, f64) {
    let a = animation();
    let key = phase.rem_euclid(1.) * a.source_fps as f64;
    let i = (key.floor() as usize).min(a.source_fps - 1);
    let start = a.stroke_integrals[i];
    let end = a.stroke_integrals[i + 1];
    (
        phase.floor() + f64::from(start.0) + f64::from(end.0 - start.0) * key.fract(),
        phase.floor() + f64::from(start.1) + f64::from(end.1 - start.1) * key.fract(),
    )
}

fn average_strokes(phase: f32, dt: f32) -> (f32, f32) {
    let end = stroke_integral(f64::from(phase));
    let start = stroke_integral(f64::from(phase) - f64::from(dt));
    (
        ((end.0 - start.0) / f64::from(dt)) as f32,
        ((end.1 - start.1) / f64::from(dt)) as f32,
    )
}

/// Published atomically with position/velocity. Blend fades bob and attitude
/// without touching the simulated particle's position or resetting its lifetime.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ButterflyWingbeatPose {
    pub phase: f32,
    pub blend: f32,
    pub orientation: Quat,
}

#[derive(Debug, Default)]
pub(super) struct WingbeatCoupling {
    pub pose: ButterflyWingbeatPose,
    heading: Option<f32>,
    pitch: f32,
    bank: f32,
}

impl WingbeatCoupling {
    pub fn advance(
        &mut self,
        phase: f32,
        enabled: bool,
        dt: f32,
        air_velocity: Vec3,
        steering: Vec3,
    ) {
        self.pose.phase = phase.rem_euclid(1.);
        // Linear ramp reaches exact zero: the unchecked path becomes the original
        // renderer and dynamics, rather than retaining an infinitesimal experiment.
        let target = if enabled { 1. } else { 0. };
        self.pose.blend += (target - self.pose.blend).clamp(-dt * 4., dt * 4.);
        let speed = air_velocity.x.hypot(air_velocity.z);
        let yaw_target = if speed > 0.001 {
            (-air_velocity.x).atan2(-air_velocity.z)
        } else {
            self.heading.unwrap_or(0.)
        };
        let heading = self.heading.get_or_insert(yaw_target);
        let delta = (yaw_target - *heading + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        *heading += (delta * (1. - (-dt / 0.15).exp())).clamp(-3. * dt, 3. * dt);
        let forward = Quat::from_rotation_y(*heading) * -Vec3::Z;
        let bank_target = (forward.cross(steering).y * 3.).clamp(-0.45, 0.45);
        self.bank += (bank_target - self.bank) * (1. - (-dt / 0.18).exp());
        // Low-pass air-relative pitch so beat-induced vertical velocity does not
        // amplify the source animation's existing cycle pitch.
        let pitch_target = air_velocity.y.atan2(speed.max(0.01)).clamp(-0.3, 0.3);
        self.pitch += (pitch_target - self.pitch) * (1. - (-dt / 0.35).exp());
        self.pose.orientation = Quat::from_rotation_y(*heading)
            * Quat::from_rotation_x(self.pitch)
            * Quat::from_rotation_z(self.bank);
    }

    pub fn acceleration(&self, speed: f32, vertical: f32, dt: f32) -> Vec3 {
        if self.pose.blend == 0. || dt <= 0. {
            return Vec3::ZERO;
        }
        let (down, up) = average_strokes(self.pose.phase, dt);
        let support = self.pose.orientation * Vec3::Y;
        let forward = self.pose.orientation * -Vec3::Z;
        self.pose.blend
            * speed
            * (support * (0.08 * vertical * (down - 1.)) + forward * (0.10 * (up - 1.)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_animation_strokes_have_unit_mean_and_correct_direction() {
        let mut sum = (0., 0.);
        for i in 0..1000 {
            let phase = (i as f32 + 0.5) / 1000.;
            let (down, up) = stroke_pulses(phase);
            let derivative = wing_pose(phase + 0.00001)[0] - wing_pose(phase - 0.00001)[0];
            assert!(down >= 0. && up >= 0. && down * up == 0.);
            assert!(if derivative > 0. { up > 0. } else { down > 0. });
            sum.0 += down / 1000.;
            sum.1 += up / 1000.;
        }
        assert!((sum.0 - 1.).abs() < 1e-5);
        assert!((sum.1 - 1.).abs() < 1e-5);
        assert_eq!(wing_pose(0.), wing_pose(1.));
        assert_eq!(stroke_pulses(0.), stroke_pulses(1.));
    }

    #[test]
    fn fixed_step_impulses_balance_without_source_key_aliasing() {
        for offset in [0., 0.137, 0.999] {
            let mut integral = Vec3::ZERO;
            for tick in 1..=120 {
                let mut coupling = WingbeatCoupling::default();
                coupling.pose = ButterflyWingbeatPose {
                    phase: (offset + tick as f32 / 120.).rem_euclid(1.),
                    blend: 1.,
                    orientation: Quat::IDENTITY,
                };
                integral += coupling.acceleration(1., 4., 1. / 120.) / 120.;
            }
            assert!(integral.length() < 1e-6, "net cycle impulse {integral:?}");
        }
    }

    #[test]
    fn opposite_turns_bank_symmetrically_and_toggle_settles_exactly() {
        let mut left = WingbeatCoupling::default();
        let mut right = WingbeatCoupling::default();
        for i in 0..120 {
            left.advance(i as f32 / 120., true, 1. / 120., -Vec3::Z * 0.05, -Vec3::X);
            right.advance(i as f32 / 120., true, 1. / 120., -Vec3::Z * 0.05, Vec3::X);
        }
        assert!(left.bank > 0. && right.bank < 0.);
        assert!((left.bank + right.bank).abs() < 1e-6);
        assert!(left.bank <= 0.45);
        for _ in 0..31 {
            left.advance(0., false, 1. / 120., Vec3::ZERO, Vec3::ZERO);
        }
        assert_eq!(left.pose.blend, 0.);
        assert_eq!(left.acceleration(1., 4., 1. / 120.), Vec3::ZERO);
    }
}
