//! Sprinkler hardware dimensions and tick-driven wetting axis, with CPU mesh-pose contract checks.
//! Rigid-part surface caches and native fallback share this pose; scene resolve stays independent.
use glam::Vec3;

pub(crate) const WORLD_SCALE: f32 = 1.0 / 256.0;
#[cfg(test)]
pub(crate) const NOZZLE_HEIGHT: f32 = 4.0;
#[cfg(test)]
pub(crate) const NOZZLE_REACH: f32 = 3.1;
#[cfg(test)]
pub(crate) const HEAD_LIFT: f32 = 1.0;
#[cfg(test)]
pub(crate) const NOZZLE_EXTENSION: f32 = 0.2;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Pose {
    #[cfg(test)]
    pub extension: f32,
    pub along_x: bool,
}

impl Pose {
    pub fn sample(tick: u32, tick_seconds: f32, phase: f32) -> Self {
        let seconds = crate::game_time::clamp_world_tick_seconds(tick_seconds);
        let pair_ticks = (1.0 / seconds).round().max(2.0) as u32;
        let cycle_ticks = pair_ticks * 2;
        let offset = (phase.rem_euclid(1.0) * cycle_ticks as f32).round() as u32;
        let cycle_tick = tick.wrapping_add(offset) % cycle_ticks;
        #[cfg(test)]
        let pair_phase = (cycle_tick % pair_ticks) as f32 / pair_ticks as f32;
        Self {
            #[cfg(test)]
            extension: 0.5 - 0.5 * (pair_phase * std::f32::consts::TAU).cos(),
            along_x: cycle_tick >= pair_ticks,
        }
    }

    pub fn axis(self) -> Vec3 {
        if self.along_x {
            Vec3::X
        } else {
            Vec3::Z
        }
    }

    /// Authoring contract: center of one active nozzle face, not a particle spawn.
    #[cfg(test)]
    pub fn outlet(self, sign: f32) -> Vec3 {
        (Vec3::Y * (NOZZLE_HEIGHT + HEAD_LIFT * self.extension)
            + self.axis() * sign * (NOZZLE_REACH + NOZZLE_EXTENSION * self.extension))
            * WORLD_SCALE
    }
}

#[cfg(test)]
pub(crate) const ROTARY_PERIOD_SECONDS: f32 = 4.0;
pub(crate) const ROTARY_FRAMES: u32 = 16;

/// Appearance only. Stable indices select complete assemblies, never water sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum Design {
    Current,
    BrassImpact,
    GardenPulse,
    EnclosedRotor,
}
impl Design {
    pub const ALL: [Self; 4] = [
        Self::Current,
        Self::BrassImpact,
        Self::GardenPulse,
        Self::EnclosedRotor,
    ];

    pub fn comparison(enabled: bool, candidate: u32) -> Self {
        if enabled {
            Self::ALL[1 + candidate.min(Self::ALL.len() as u32 - 2) as usize]
        } else {
            Self::Current
        }
    }
}
pub(crate) const ROTARY_NOZZLE_HEIGHT: f32 = 4.2;
pub(crate) const ROTARY_NOZZLE_X: f32 = 3.15;
pub(crate) const ROTARY_NOZZLE_Z: f32 = -0.85;
pub(crate) const ROTARY_NOZZLE_YAW: f32 = -0.65;

/// Shared continuous rotor pose. Reduce integer ticks before float conversion.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RotorPose {
    pub angle: f32,
}

impl RotorPose {
    #[cfg(test)]
    pub fn sample(tick: u32, fraction: f32, tick_seconds: f32, phase: f32) -> Self {
        let ticks = (ROTARY_PERIOD_SECONDS
            / crate::game_time::clamp_world_tick_seconds(tick_seconds))
        .round()
        .max(4.0) as u32;
        let time = (tick % ticks) as f32 + fraction.clamp(0.0, 1.0);
        Self {
            angle: ((time / ticks as f32 + phase.rem_euclid(1.0)).rem_euclid(1.0))
                * std::f32::consts::TAU,
        }
    }
    pub fn rotate(self, value: Vec3) -> Vec3 {
        let (s, c) = self.angle.sin_cos();
        Vec3::new(
            c * value.x - s * value.z,
            value.y,
            s * value.x + c * value.z,
        )
    }
    pub fn direction(self) -> Vec3 {
        self.rotate(Vec3::new(
            ROTARY_NOZZLE_YAW.cos(),
            0.0,
            ROTARY_NOZZLE_YAW.sin(),
        ))
    }
    pub fn outlet(self) -> Vec3 {
        self.rotate(Vec3::new(
            ROTARY_NOZZLE_X,
            ROTARY_NOZZLE_HEIGHT,
            ROTARY_NOZZLE_Z,
        )) * WORLD_SCALE
    }
}

/// Phase integration follows the canonical world clock, not rendered frame count.
/// Changing RPM does not jump the head; pausing never accumulates deferred water.
#[derive(Default)]
pub(crate) struct SprayClock {
    previous: Option<(u32, f32)>,
    angle: f64,
    pulse_phase: f64,
    droplet_seconds: f64,
    quadrature_fraction: f64,
    quadrature_epoch: u32,
}

impl SprayClock {
    pub fn sample(
        &mut self,
        tick: u32,
        fraction: f32,
        seconds: f32,
        rpm: f32,
        frequency: f32,
    ) -> [f32; 4] {
        let frequency = frequency.max(0.25).min(20.0);
        let fraction = fraction.clamp(0.0, 1.0);
        let seconds = crate::game_time::clamp_world_tick_seconds(seconds);
        let rpm = if rpm.is_finite() {
            rpm.clamp(0.0, 60.0)
        } else {
            15.0
        };
        let dt = self.previous.map_or(0.0, |(old_tick, old_fraction)| {
            (f64::from(tick.wrapping_sub(old_tick)) + f64::from(fraction - old_fraction)).max(0.0)
                * f64::from(seconds)
        });
        self.angle = (self.angle + dt * f64::from(rpm) * std::f64::consts::TAU / 60.0)
            .rem_euclid(std::f64::consts::TAU);
        self.pulse_phase = (self.pulse_phase + dt * f64::from(frequency)).rem_euclid(1.0);
        self.droplet_seconds = (self.droplet_seconds + dt).rem_euclid(3.0);
        // A bounded world-time quadrature, not frame randomness or a new source.
        // Weyl scrambling avoids locking a finite velocity lattice to valve Hz,
        // rotation or skipped render frames. 64 strata resolve sub-voxel reaches.
        let samples = self.quadrature_fraction + dt * 60.0;
        self.quadrature_fraction = samples.fract();
        self.quadrature_epoch = self.quadrature_epoch.wrapping_add(samples.floor() as u32);
        let stratum = self.quadrature_epoch.wrapping_mul(0x9e3779b9) >> 26;
        self.previous = Some((tick, fraction));
        [
            self.angle as f32,
            (self.pulse_phase / f64::from(frequency)) as f32,
            self.droplet_seconds as f32,
            (stratum as f32 + 0.5) / 64.0,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spray_clock_is_continuous_under_speed_changes_pause_and_tick_wrap() {
        let mut clock = SprayClock::default();
        assert_eq!(
            clock.sample(u32::MAX, 0.5, 0.05, 15.0, 4.0),
            [0.0, 0.0, 0.0, 0.5 / 64.0]
        );
        let a = clock.sample(0, 0.5, 0.05, 15.0, 4.0);
        assert!((a[0] - std::f32::consts::TAU / 80.0).abs() < 1e-6);
        assert_eq!(clock.sample(0, 0.5, 0.05, 60.0, 4.0), a);
        let stopped = clock.sample(10, 0.5, 0.05, 0.0, 4.0);
        assert_eq!(stopped[0], a[0]);
        let b = clock.sample(20, 0.5, 0.05, 30.0, 4.0);
        assert!((b[0] - a[0] - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn spray_quadrature_covers_strata_at_multiple_cadences_and_freezes_when_paused() {
        for stride in [1, 2, 4] {
            let mut clock = SprayClock::default();
            let mut seen = [false; 64];
            for i in 0..2048 {
                let t = i * stride;
                let s = clock.sample(t / 4, (t % 4) as f32 / 4.0, 1.0 / 15.0, 15.0, 4.0);
                assert!(s[3] > 0.0 && s[3] < 1.0);
                seen[(s[3] * 64.0) as usize] = true;
                assert_eq!(
                    s,
                    clock.sample(t / 4, (t % 4) as f32 / 4.0, 1.0 / 15.0, 15.0, 4.0)
                );
            }
            assert!(
                seen.into_iter().all(|v| v),
                "velocity strata aliased with frame cadence"
            );
        }
        let mut a = SprayClock::default();
        let mut b = SprayClock::default();
        a.sample(0, 0.0, 0.05, 15.0, 4.0);
        b.sample(0, 0.0, 0.05, 15.0, 4.0);
        for tick in 1..=20 {
            a.sample(tick, 0.0, 0.05, 15.0, 4.0);
        }
        assert_eq!(
            a.sample(20, 0.0, 0.05, 15.0, 4.0)[3],
            b.sample(20, 0.0, 0.05, 15.0, 4.0)[3]
        );
    }

    #[test]
    fn frequency_changes_preserve_valve_phase_and_pause_droplet_motion() {
        let mut clock = SprayClock::default();
        clock.sample(0, 0.0, 0.05, 15.0, 4.0);
        let before = clock.sample(1, 0.0, 0.05, 15.0, 4.0);
        let after = clock.sample(1, 0.0, 0.05, 15.0, 0.25);
        assert_eq!(before[0], after[0]);
        assert_eq!(before[2], after[2]);
        assert!((before[1] * 4.0 - after[1] * 0.25).abs() < 1e-6);
        let advanced = clock.sample(2, 0.0, 0.05, 15.0, 20.0);
        assert!((advanced[1] * 20.0 - after[1] * 0.25).abs() < 1e-6);
        assert!((advanced[2] - after[2] - 0.05).abs() < 1e-6);
    }

    #[test]
    fn rotor_is_continuous_periodic_and_has_one_matching_spout() {
        let first = RotorPose::sample(7, 0.0, 0.05, 0.125);
        let half = RotorPose::sample(7, 0.5, 0.05, 0.125);
        assert!(first.direction().distance(half.direction()) > 0.01);
        assert!(
            first
                .direction()
                .distance(RotorPose::sample(87, 0.0, 0.05, 0.125).direction())
                < 1e-6
        );
        assert!(
            RotorPose::sample(7, 1.0, 0.05, 0.125)
                .direction()
                .distance(RotorPose::sample(8, 0.0, 0.05, 0.125).direction())
                < 1e-6
        );
        assert!((half.direction().length() - 1.0).abs() < 1e-6);
        assert!((half.outlet().y - ROTARY_NOZZLE_HEIGHT * WORLD_SCALE).abs() < 1e-8);
    }

    #[test]
    fn outlets_follow_both_pairs_lift_and_extension_on_the_world_clock() {
        for phase in [0., 0.125, 0.5, 0.975] {
            for tick in 0..80 {
                let pose = Pose::sample(tick, 0.05, phase);
                let positive = pose.outlet(1.) / WORLD_SCALE;
                let negative = pose.outlet(-1.) / WORLD_SCALE;
                assert!((positive.y - negative.y).abs() < 1e-6);
                assert!((positive.x + negative.x).abs() < 1e-6);
                assert!((positive.z + negative.z).abs() < 1e-6);
                assert!((NOZZLE_HEIGHT..=NOZZLE_HEIGHT + HEAD_LIFT).contains(&positive.y));
                assert!(
                    (positive.dot(pose.axis()) - NOZZLE_REACH - pose.extension * NOZZLE_EXTENSION)
                        .abs()
                        < 1e-6
                );
            }
        }
        assert_eq!(Pose::sample(10, 0.05, 0.).extension, 1.);
        assert!(!Pose::sample(19, 0.05, 0.).along_x);
        assert!(Pose::sample(20, 0.05, 0.).along_x);
        assert!(!Pose::sample(40, 0.05, 0.).along_x);
    }

    #[test]
    #[ignore = "requires Slang CPU executable backend; explicit render validation"]
    fn sprinkler_shader_pose_matches_water_outlets() {
        use std::{fmt::Write, process::Command};
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("sprinkler-pose.slang");
        let executable = directory.path().join("sprinkler-pose");
        let mut code = String::from(
            "import sprinkler_animation;\nexport __extern_cpp int main() { float e; bool x;\n",
        );
        let parts = crate::tracer::sprinkler_bake_motions_for_test();
        let mut cases = 0;
        for seconds in [1. / 240., 0.05, 0.08, 0.4, 1.] {
            for phase in [0., 0.0125, 0.125, 0.5, 0.975, 1.] {
                for tick in [0, 1, 9, 10, 19, 20, 39, 40, 479, u32::MAX] {
                    let pose = Pose::sample(tick, seconds, phase);
                    writeln!(code, "sprinklerAnimation({tick}u, {seconds:.9}, {phase:.9}, e, x); if(abs(e - {:.9}) > 0.00001 || x != {}) return 1;", pose.extension, pose.along_x).unwrap();
                    for motion in &parts {
                        let mut active = *motion;
                        let is_x = motion.x != 0.;
                        let is_z = motion.z != 0.;
                        if (is_x || is_z) && !(if pose.along_x { is_x } else { is_z }) {
                            active.x = 0.;
                            active.z = 0.;
                        }
                        let expected = active * pose.extension;
                        writeln!(code, "{{ float3 moved=sprinklerPartTranslation(float3({:.12},{:.12},{:.12}), e, x); if(length(moved-float3({:.12},{:.12},{:.12})) > 0.0000001) return 2; }}",
                            motion.x, motion.y, motion.z, expected.x, expected.y, expected.z).unwrap();
                    }
                    for nozzle in 0..4 {
                        let x_arm = nozzle < 2;
                        let sign = if nozzle % 2 == 0 { 1.0 } else { -1.0 };
                        let direction = if x_arm { Vec3::X } else { Vec3::Z };
                        let reach = NOZZLE_REACH
                            + if x_arm == pose.along_x {
                                NOZZLE_EXTENSION * pose.extension
                            } else {
                                0.0
                            };
                        let expected = (direction * sign * reach
                            + Vec3::Y * (NOZZLE_HEIGHT + HEAD_LIFT * pose.extension))
                            * WORLD_SCALE;
                        writeln!(code, "{{ float3 outlet=sprinklerOutlet({tick}u,{seconds:.9},{phase:.9},{nozzle}u); if(length(outlet-float3({:.12},{:.12},{:.12}))>0.0000001) return 3; }}", expected.x, expected.y, expected.z).unwrap();
                    }
                    cases += 1;
                }
            }
        }
        for seconds in [1.0 / 240.0, 0.05, 0.08, 0.4, 1.0] {
            let period = (ROTARY_PERIOD_SECONDS / seconds).round().max(4.0) as u32;
            for tick in [0, 1, 79, 80, 100000007, u32::MAX] {
                for fraction in [0.0, 0.5, 0.999] {
                    for phase in [0.0, 0.125, 0.975, 1.0] {
                        let rotor = RotorPose::sample(tick, fraction, seconds, phase);
                        let reduced = (tick % period) as f32 + fraction;
                        let d = rotor.direction();
                        let p = rotor.outlet();
                        writeln!(code,"{{ float a=sprinklerRotaryAngle({tick}u,{fraction:.9},{seconds:.9},{phase:.9}); if(length(sprinklerRotate(float3(1,0,0),a)-float3({:.12},0,{:.12}))>0.00001)return 4; float3 p=sprinklerRotaryOutletAtTick({reduced:.9},{seconds:.9},{phase:.9}),d=sprinklerRotaryDirectionAtTick({reduced:.9},{seconds:.9},{phase:.9}); if(length(p-float3({:.12},{:.12},{:.12}))>0.0000002 || length(d-float3({:.12},{:.12},{:.12}))>0.00001)return 5; }}",rotor.angle.cos(),rotor.angle.sin(),p.x,p.y,p.z,d.x,d.y,d.z).unwrap();
                        cases += 1;
                    }
                }
            }
        }
        writeln!(
            code,
            "printf(\"{cases} sprinkler GPU/CPU pose cases passed\\n\"); return 0; }}"
        )
        .unwrap();
        std::fs::write(&source, code).unwrap();
        let output = Command::new(std::env::var_os("SLANGC").unwrap_or_else(|| "slangc".into()))
            .arg(&source)
            .args(["-std", "2025", "-I"])
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/shader/slang"))
            .args(["-target", "executable", "-o"])
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(executable).output().unwrap();
        assert!(
            output.status.success(),
            "Slang/Rust pose mismatch: {output:?}"
        );
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
}

/// Exact render-input sample shared with audio. Index 1 remains pulse *seconds*.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SpraySnapshot {
    pub clock: [f32; 4],
    pub frequency: f32,
    pub duty: f32,
}
impl SpraySnapshot {
    pub fn new(clock: [f32; 4], frequency: f32, duty_percent: f32) -> Self {
        Self {
            clock,
            frequency: frequency.max(0.25).min(20.0),
            duty: duty_percent.clamp(0.0, 100.0) * 0.01,
        }
    }
    pub fn phase(self, animation_phase: f32) -> f64 {
        // Same f32 arithmetic as the source-age-zero shader valve.
        f64::from((self.clock[1] * self.frequency + animation_phase.fract()).fract())
    }
}

#[cfg(test)]
mod audio_clock_contract_tests {
    use super::*;
    #[test]
    fn upload_snapshot_and_audio_share_exact_normalization_and_instance_valve() {
        for duty in [0.0, 1.0, 10.0, 30.0, 50.0, 90.0, 100.0] {
            for frequency in [0.25, 5.0, 20.0] {
                for fps in [15, 30, 60, 120] {
                    let mut clock = SprayClock::default();
                    for frame in 0..fps {
                        let ticks = frame as f32 / fps as f32 / 0.05;
                        let gpu =
                            clock.sample(ticks.floor() as u32, ticks.fract(), 0.05, 0.0, frequency);
                        let snapshot = SpraySnapshot::new(gpu, frequency, duty);
                        assert_eq!(snapshot.clock, gpu);
                        for offset in [0.0_f32, 0.213, 1.7, 12.875] {
                            let shader_valve =
                                (gpu[1] * frequency + offset.fract()).fract() < duty * 0.01;
                            assert_eq!(
                                snapshot.phase(offset) < f64::from(snapshot.duty),
                                shader_valve
                            );
                        }
                    }
                }
            }
        }
    }
}
