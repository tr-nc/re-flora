//! Tree-generation-owned travelling bend excitation. Only a bounded packet list
//! crosses to the GPU; no expanded wood/leaf preparation or physical collider motion.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

pub(crate) const MAX_KICK_PULSES: usize = 8;
#[derive(Clone, Copy, Debug)]
pub(crate) struct KickTuning {
    pub strength: f32,
    pub frequency: f32,
    pub damping: f32,
    pub propagation_speed: f32, // skeleton voxels/second
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
        ensure!(
            self.propagation_speed.is_finite() && (100. ..=2000.).contains(&self.propagation_speed),
            "invalid tree kick propagation speed"
        );
        Ok(())
    }
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuKickPulse {
    pub velocity_age: [f32; 4],
    pub parameters: [f32; 4], // frequency, damping, speed, reserved
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuKickTree {
    pub metadata: [u32; 4], // packet count, ambient wind enabled, reserved
    pub pulses: [GpuKickPulse; MAX_KICK_PULSES],
}
#[derive(Clone, Debug)]
struct Pulse {
    velocity: Vec3,
    age: f32,
    tuning: KickTuning,
    max_delay: f32,
    canopy_delay: f32,
    canopy_pending: bool,
}
impl Pulse {
    fn decay(&self) -> f32 {
        std::f32::consts::TAU * self.tuning.frequency * self.tuning.damping
    }
    fn oscillation(&self) -> f32 {
        std::f32::consts::TAU * self.tuning.frequency * (1. - self.tuning.damping.powi(2)).sqrt()
    }
    fn angle(&self, distance_voxels: f32) -> Vec3 {
        let time = self.age - distance_voxels / self.tuning.propagation_speed;
        if time <= 0. {
            return Vec3::ZERO;
        }
        self.velocity
            * ((-self.decay() * time).exp() * (self.oscillation() * time).sin()
                / self.oscillation())
    }
    fn expired(&self) -> bool {
        self.age > self.max_delay + 12. / self.decay()
    }
}
#[derive(Clone, Debug, Default)]
pub(crate) struct KickWave {
    pulses: Vec<Pulse>,
}
impl KickWave {
    const MAX_ANGLE: f32 = 0.2;
    pub fn active(&self) -> bool {
        !self.pulses.is_empty()
    }
    pub fn angle(&self, distance_voxels: f32) -> Vec3 {
        let angle: Vec3 = self
            .pulses
            .iter()
            .map(|pulse| pulse.angle(distance_voxels))
            .sum();
        let length = angle.length();
        if length > Self::MAX_ANGLE {
            angle * (Self::MAX_ANGLE / length)
        } else {
            angle
        }
    }
    pub fn can_impulse(&self) -> bool {
        self.pulses.len() < MAX_KICK_PULSES
    }
    pub fn impulse(
        &mut self,
        direction: Vec3,
        tuning: KickTuning,
        max_distance: f32,
        canopy_distance: f32,
    ) -> Result<bool> {
        tuning.validate()?;
        ensure!(
            direction.is_finite()
                && max_distance.is_finite()
                && max_distance >= 0.
                && canopy_distance.is_finite()
                && canopy_distance >= 0.,
            "invalid tree kick path/direction"
        );
        let push = Vec3::new(direction.x, 0., direction.z).normalize_or_zero();
        if push == Vec3::ZERO || tuning.strength == 0. {
            return Ok(false);
        }
        // Never truncate an accepted front/tail or discard its canopy event.
        // Backpressure rejects admission before any state changes; K hides until space returns.
        if !self.can_impulse() {
            return Ok(false);
        }
        self.pulses.push(Pulse {
            velocity: Vec3::Y.cross(push) * tuning.strength,
            age: 0.,
            tuning,
            max_delay: max_distance.max(canopy_distance) / tuning.propagation_speed,
            canopy_delay: canopy_distance / tuning.propagation_speed,
            canopy_pending: true,
        });
        // Angle limiting is local at the travelled front (CPU and GPU), not an
        // instantaneous rescale of previously emitted waves throughout the tree.
        Ok(true)
    }
    pub fn advance(&mut self, dt: f32) -> Result<()> {
        ensure!(dt.is_finite() && dt >= 0., "invalid tree kick timestep");
        for pulse in &mut self.pulses {
            pulse.age += dt;
        }
        self.pulses.retain(|pulse| !pulse.expired());
        Ok(())
    }
    pub fn take_canopy_arrivals(&mut self) -> usize {
        let mut arrivals = 0;
        for pulse in &mut self.pulses {
            if pulse.canopy_pending && pulse.age >= pulse.canopy_delay {
                pulse.canopy_pending = false;
                // A suspended frame may skip the whole excitation. Do not play
                // a stale canopy event seconds after its visible wave has passed.
                if pulse.age - pulse.canopy_delay <= 0.25 {
                    arrivals += 1;
                }
            }
        }
        arrivals
    }
    pub fn gpu(&self, wind_enabled: bool) -> GpuKickTree {
        let mut result = GpuKickTree::zeroed();
        result.metadata = [self.pulses.len() as u32, u32::from(wind_enabled), 0, 0];
        for (destination, pulse) in result.pulses.iter_mut().zip(&self.pulses) {
            *destination = GpuKickPulse {
                velocity_age: pulse.velocity.extend(pulse.age).to_array(),
                parameters: [
                    pulse.tuning.frequency,
                    pulse.tuning.damping,
                    pulse.tuning.propagation_speed,
                    0.,
                ],
            };
        }
        result
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
            propagation_speed: 600.,
        }
    }
    #[test]
    fn wave_moves_away_rebounds_and_reaches_the_canopy_only_after_path_travel() {
        let mut kick = KickWave::default();
        kick.impulse(Vec3::X, tuning(), 180., 120.).unwrap();
        kick.advance(0.03).unwrap();
        assert!(kick.angle(0.).z < -0.01);
        assert_eq!(kick.angle(120.), Vec3::ZERO);
        assert_eq!(kick.take_canopy_arrivals(), 0);
        kick.advance(0.2).unwrap();
        assert!(kick.angle(0.).z > 0.);
        assert!(kick.angle(120.).z < -0.01);
        assert_eq!(kick.take_canopy_arrivals(), 1);
        assert_eq!(kick.take_canopy_arrivals(), 0);
        kick.advance(6.).unwrap();
        assert!(!kick.active());
    }
    #[test]
    fn integration_matches_frame_rates_and_repeated_kicks_bound_the_entire_front() {
        let mut a = KickWave::default();
        a.impulse(Vec3::X, tuning(), 180., 120.).unwrap();
        let mut b = a.clone();
        a.advance(0.125).unwrap();
        for _ in 0..15 {
            b.advance(1. / 120.).unwrap();
        }
        for distance in [0., 25., 120., 180.] {
            assert!(a.angle(distance).abs_diff_eq(b.angle(distance), 1e-6));
        }
        for _ in 0..100 {
            a.impulse(Vec3::X, tuning(), 180., 120.).unwrap();
            a.advance(1. / 120.).unwrap();
            for distance in [0., 25., 120., 180.] {
                assert!(a.angle(distance).length() <= KickWave::MAX_ANGLE + 1e-6);
            }
            assert!(a.pulses.len() <= MAX_KICK_PULSES);
        }
    }
    #[test]
    fn full_packet_bank_rejects_without_losing_any_accepted_canopy_events() {
        let mut kick = KickWave::default();
        for _ in 0..MAX_KICK_PULSES {
            assert!(kick.impulse(Vec3::X, tuning(), 180., 120.).unwrap());
        }
        assert!(!kick.can_impulse());
        assert!(!kick.impulse(Vec3::Z, tuning(), 180., 120.).unwrap());
        kick.advance(0.21).unwrap();
        assert_eq!(kick.take_canopy_arrivals(), MAX_KICK_PULSES);
        assert_eq!(kick.take_canopy_arrivals(), 0);
        kick.advance(6.).unwrap();
        assert!(kick.can_impulse());
    }
    #[test]
    fn another_strong_kick_does_not_rescale_an_existing_wave_before_its_front_arrives() {
        let mut kick = KickWave::default();
        kick.impulse(Vec3::X, tuning(), 180., 120.).unwrap();
        kick.advance(0.23).unwrap();
        let before = kick.angle(120.);
        kick.impulse(
            Vec3::X,
            KickTuning {
                strength: 4.,
                ..tuning()
            },
            180.,
            120.,
        )
        .unwrap();
        assert_eq!(kick.angle(120.), before);
    }
    #[test]
    fn invalid_zero_and_suspended_inputs_never_emit_a_late_canopy_event() {
        let mut kick = KickWave::default();
        assert!(kick.impulse(Vec3::NAN, tuning(), 180., 120.).is_err());
        assert!(!kick.impulse(Vec3::Y, tuning(), 180., 120.).unwrap());
        assert!(!kick
            .impulse(
                Vec3::X,
                KickTuning {
                    strength: 0.,
                    ..tuning()
                },
                180.,
                120.
            )
            .unwrap());
        assert!(kick.advance(f32::NAN).is_err());
        assert!(!kick.active());
        kick.impulse(Vec3::X, tuning(), 180., 120.).unwrap();
        kick.advance(1.).unwrap();
        assert_eq!(kick.take_canopy_arrivals(), 0);
        assert_eq!(std::mem::size_of::<GpuKickTree>(), 272);
    }
}
