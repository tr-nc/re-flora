//! Transient, tree-generation-owned root impulse. O(one oscillator per tree),
//! never a second CPU deformation pass or a change to physical collision.
use anyhow::{ensure, Result};
use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub(crate) struct KickTuning {
    pub strength: f32,
    pub frequency: f32,
    pub damping: f32,
}
impl KickTuning {
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.strength.is_finite() && (0. ..=4.).contains(&self.strength),
            "invalid tree kick strength"
        );
        ensure!(
            self.frequency.is_finite() && (1. ..=7.).contains(&self.frequency),
            "invalid tree kick frequency"
        );
        ensure!(
            self.damping.is_finite() && (0.1..=0.9).contains(&self.damping),
            "invalid tree kick damping"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RootKick {
    angle: Vec3,
    velocity: Vec3,
}
impl RootKick {
    // Bound stored oscillator energy, including repeated kicks, rather than
    // clipping rendered vertices or allowing unlimited accumulated tilt.
    const MAX_ANGLE: f32 = 0.2;
    pub fn active(&self) -> bool {
        self.angle != Vec3::ZERO || self.velocity != Vec3::ZERO
    }
    pub fn angle(&self) -> Vec3 {
        self.angle
    }
    pub fn impulse(&mut self, direction: Vec3, tuning: KickTuning) -> Result<bool> {
        tuning.validate()?;
        ensure!(direction.is_finite(), "nonfinite tree kick direction");
        let push = Vec3::new(direction.x, 0., direction.z).normalize_or_zero();
        if push == Vec3::ZERO || tuning.strength == 0. {
            return Ok(false);
        }
        self.velocity += Vec3::Y.cross(push) * tuning.strength;
        self.bound_energy(tuning.frequency);
        Ok(true)
    }
    fn bound_energy(&mut self, frequency: f32) {
        self.angle = self.angle.clamp_length_max(Self::MAX_ANGLE);
        let omega = std::f32::consts::TAU * frequency;
        let allowance = (Self::MAX_ANGLE.powi(2) - self.angle.length_squared())
            .max(0.)
            .sqrt()
            * omega;
        self.velocity = self.velocity.clamp_length_max(allowance);
    }
    pub fn advance(&mut self, dt: f32, tuning: KickTuning) -> Result<()> {
        tuning.validate()?;
        ensure!(dt.is_finite() && dt >= 0., "invalid tree kick timestep");
        if dt == 0. || !self.active() {
            return Ok(());
        }
        // Exact underdamped oscillator: frame-rate independent, with live
        // frequency/damping controls. Long suspended frames do not restart it.
        self.bound_energy(tuning.frequency);
        let omega = std::f32::consts::TAU * tuning.frequency;
        let decay = tuning.damping * omega;
        let oscillation = omega * (1. - tuning.damping * tuning.damping).sqrt();
        let c = (oscillation * dt).cos();
        let s = (oscillation * dt).sin() / oscillation;
        let envelope = (-decay * dt).exp();
        let angle = envelope * (self.angle * c + (self.velocity + decay * self.angle) * s);
        self.velocity = envelope
            * (self.velocity * c - (decay * self.velocity + omega * omega * self.angle) * s);
        self.angle = angle;
        if self.angle.length_squared() < 1e-10 && self.velocity.length_squared() < 1e-8 {
            self.angle = Vec3::ZERO;
            self.velocity = Vec3::ZERO;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tuning() -> KickTuning {
        KickTuning {
            strength: 1.6,
            frequency: 3.,
            damping: 0.22,
        }
    }
    #[test]
    fn impulse_moves_away_from_player_then_rebounds_and_settles() {
        let mut kick = RootKick::default();
        assert!(kick.impulse(Vec3::X, tuning()).unwrap());
        kick.advance(0.03, tuning()).unwrap();
        assert!(kick.angle.z < -0.01);
        kick.advance(0.2, tuning()).unwrap();
        assert!(kick.angle.z > 0.);
        kick.advance(6., tuning()).unwrap();
        assert!(!kick.active());
    }
    #[test]
    fn integration_matches_frame_rates_and_repeated_kicks_stay_bounded() {
        let mut a = RootKick::default();
        a.impulse(Vec3::X, tuning()).unwrap();
        let mut b = a.clone();
        a.advance(0.125, tuning()).unwrap();
        for _ in 0..15 {
            b.advance(1. / 120., tuning()).unwrap();
        }
        assert!(a.angle.abs_diff_eq(b.angle, 1e-6));
        assert!(a.velocity.abs_diff_eq(b.velocity, 2e-6));
        for _ in 0..2000 {
            a.impulse(Vec3::X, tuning()).unwrap();
            a.advance(1. / 120., tuning()).unwrap();
            assert!(a.angle.length() <= RootKick::MAX_ANGLE + 1e-6);
        }
    }
    #[test]
    fn invalid_inputs_and_zero_strength_leave_state_unchanged() {
        let mut kick = RootKick::default();
        assert!(kick.impulse(Vec3::NAN, tuning()).is_err());
        assert!(!kick.impulse(Vec3::Y, tuning()).unwrap());
        assert!(!kick
            .impulse(
                Vec3::X,
                KickTuning {
                    strength: 0.,
                    ..tuning()
                }
            )
            .unwrap());
        assert!(kick.advance(f32::NAN, tuning()).is_err());
        assert!(!kick.active());
    }
}
