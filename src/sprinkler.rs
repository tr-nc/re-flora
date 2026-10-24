//! Sprinkler hardware dimensions and tick-driven wetting axis, with CPU mesh-pose contract checks.
//! Rigid-part surface caches and native fallback share this pose; scene resolve stays independent.
use glam::Vec3;

/// Metadata of the selected, unmodified six-burst runtime recording.
pub(crate) const LOOP_SAMPLE_RATE: u32 = 48_000;
pub(crate) const LOOP_FRAMES: usize = 74_880;
pub(crate) const LOOP_BURSTS: f64 = 6.0;
pub(crate) const LOOP_SECONDS: f64 = LOOP_FRAMES as f64 / LOOP_SAMPLE_RATE as f64;
pub(crate) const PULSE_FREQUENCY: f32 = (LOOP_BURSTS / LOOP_SECONDS) as f32;
/// Retain the established short attack/recovery: 78 ms open per 260 ms.
pub(crate) const PULSE_DUTY_PERCENT: f32 = 30.0;
const FIRST_PEAK_SECONDS: f64 = 0.075;

/// Internal native-fixture inputs only, never GUI settings or saved overrides.
pub(crate) struct DiagnosticTiming {
    pub frequency: f32,
    pub duty_percent: f32,
}
impl Default for DiagnosticTiming {
    fn default() -> Self {
        Self {
            frequency: PULSE_FREQUENCY,
            duty_percent: PULSE_DUTY_PERCENT,
        }
    }
}

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
    audio_loop_seconds: f64,
    droplet_seconds: f64,
    quadrature_fraction: f64,
    quadrature_epoch: u32,
}

impl SprayClock {
    pub fn audio_loop_seconds(&self) -> f64 {
        self.audio_loop_seconds
    }

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
        self.audio_loop_seconds = (self.audio_loop_seconds + dt).rem_euclid(LOOP_SECONDS);
        self.pulse_phase = if frequency == PULSE_FREQUENCY {
            // Normal gameplay has one cadence owner, including exact six-burst loop wrap.
            (self.audio_loop_seconds * LOOP_BURSTS / LOOP_SECONDS).fract()
        } else {
            // Legacy opt-in native diagnostics may still exercise arbitrary valve inputs.
            (self.pulse_phase + dt * f64::from(frequency)).rem_euclid(1.0)
        };
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
    #[ignore = "requires Slang CPU executable backend; explicit rendering regression"]
    fn sprinkler_spray_uses_the_rendered_discrete_birth_pose() {
        use std::process::Command;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("sprinkler-discrete.slang");
        let executable = directory.path().join("sprinkler-discrete");
        std::fs::write(&source, r#"
import sprinkler_animation;
import sprinkler_spray;
import model_pixel_types;
export __extern_cpp int main() {
    if(SPRINKLER_ROTARY_FRAMES!=__FRAMES__u)return 4;
    uint cases=0;
    float sizes[4]={0.25,0.5,1.0,4.0};
    for(uint scale=0;scale<4;++scale)
    for(uint i=0;i<256;++i) for(uint r=0;r<3;++r) for(uint t=0;t<5;++t) {
        float now=float(i)*6.28318530718/251.0;
        float phase=0.213, rpm=float(r)*30.0, age=float(t)*0.17;
        float birth=now+phase*6.28318530718-age*rpm*6.28318530718/60.0;
        float baked=float(sprinklerRotaryFrame(birth))*6.28318530718/16.0;
        float3 base=float3(1.3,0.4,-0.7);
        ModelPixelFrame hardware=sprinklerHardwareFrame(base,sizes[scale]);
        if(length(hardware.center-base)>0.000001 || hardware.scale!=sizes[scale]) return 5;
        MistSpraySource s=mistRotaryKinematics(age,0.01,0.8,now,phase,rpm,30.0,sizes[scale]);
        if(length(base+s.outlet-(hardware.center+hardware.scale*sprinklerRotaryOutlet(baked)))>0.000001) {
            printf("outlet mismatch angle=%f birth=%f frame=%u\n",now,birth,sprinklerRotaryFrame(birth)); return 1;
        }
        if(length(s.velocity-mistSprayDeflectedVelocity(baked,0.8))>0.00001) return 2;
        if(abs(s.angular_velocity)>0.00001) return 3;
        ++cases;
    }
    // Both real source families must preserve velocity/flux while scaling the mouth.
    for(uint i=0;i<128;++i) for(uint scale=0;scale<4;++scale) {
        MistSpraySource a=mistRotarySource(i,128,0.7,0.23,15,40,0.3,0.5);
        MistSpraySource b=mistRotarySource(i,128,0.7,0.23,15,40,0.3,0.5,4.0,0.5,sizes[scale]);
        if(length(b.outlet-a.outlet*sizes[scale])>0.000001 || length(b.velocity-a.velocity)>0.000001 || b.emission!=a.emission) return 6;
        a=mistDropletSource(i,128,0.8,0.7,0.23,15,40,0.3,0.5,4);
        b=mistDropletSource(i,128,0.8,0.7,0.23,15,40,0.3,0.5,4,sizes[scale]);
        if(length(b.outlet-a.outlet*sizes[scale])>0.000001 || length(b.velocity-a.velocity)>0.000001 || b.emission!=a.emission) return 7;
    }
    if(sprinklerHardwareFrame(float3(0),0).scale!=0.25 || sprinklerHardwareFrame(float3(0),9).scale!=4.0) return 8;
    printf("%u scaled discrete birth-pose cases and 1024 mist/droplet cases passed\n",cases); return 0;
}
"#.replace("__FRAMES__", &ROTARY_FRAMES.to_string())).unwrap();
        let compile = Command::new(std::env::var_os("SLANGC").unwrap_or_else(|| "slangc".into()))
            .arg(&source)
            .args(["-std", "2025", "-I"])
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/shader/slang"))
            .args(["-target", "executable", "-o"])
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            compile.status.success(),
            "{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let output = Command::new(executable).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        print!("{}", String::from_utf8_lossy(&output.stdout));
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
    pub audio_loop_seconds: f64,
}
impl SpraySnapshot {
    pub fn new(clock: [f32; 4], frequency: f32, duty_percent: f32) -> Self {
        Self {
            clock,
            audio_loop_seconds: f64::from(clock[1]),
            frequency: frequency.max(0.25).min(20.0),
            duty: duty_percent.clamp(0.0, 100.0) * 0.01,
        }
    }
    /// Preserve all six distinct bursts, not just the phase within one burst.
    /// Valve opening corresponds to the recording's first energy peak.
    pub fn audio_loop_phase(self, animation_phase: f32) -> f32 {
        ((self.audio_loop_seconds
            + f64::from(animation_phase.rem_euclid(1.0)) / f64::from(self.frequency)
            + FIRST_PEAK_SECONDS)
            / LOOP_SECONDS)
            .rem_euclid(1.0) as f32
    }

    #[cfg(test)]
    pub fn phase(self, animation_phase: f32) -> f64 {
        // Same f32 arithmetic as the source-age-zero shader valve.
        f64::from((self.clock[1] * self.frequency + animation_phase.fract()).fract())
    }
}

#[cfg(test)]
mod audio_clock_contract_tests {
    use super::*;

    #[test]
    fn selected_loop_keeps_six_bursts_and_peak_offset_without_tick_quantization() {
        for tick_seconds in [0.017, 0.05, 0.11] {
            for fps in [15, 60, 144] {
                let mut world = crate::game_time::WorldClock::new(0, 0.0);
                let mut clock = SprayClock::default();
                clock.sample(0, 0.0, tick_seconds, 5.0, PULSE_FREQUENCY);
                for frame in 1..=fps * 600 {
                    world.advance_simulation(1.0 / fps as f32, tick_seconds, true);
                    let values = clock.sample(
                        world.flora_tick(),
                        world.flora_tick_fraction(),
                        tick_seconds,
                        5.0,
                        PULSE_FREQUENCY,
                    );
                    let mut snapshot =
                        SpraySnapshot::new(values, PULSE_FREQUENCY, PULSE_DUTY_PERCENT);
                    snapshot.audio_loop_seconds = clock.audio_loop_seconds();
                    let elapsed = frame as f64 / fps as f64;
                    let expected = ((elapsed + FIRST_PEAK_SECONDS) / LOOP_SECONDS).rem_euclid(1.0);
                    let error = (f64::from(snapshot.audio_loop_phase(0.0)) - expected + 0.5)
                        .rem_euclid(1.0)
                        - 0.5;
                    assert!(
                        error.abs() * LOOP_SECONDS < 0.002,
                        "ten-minute drift: {error}"
                    );
                    // Pause keeps the full six-burst position, not only valve phase.
                    assert_eq!(
                        values,
                        clock.sample(
                            world.flora_tick(),
                            world.flora_tick_fraction(),
                            tick_seconds,
                            5.0,
                            PULSE_FREQUENCY
                        )
                    );
                }
            }
        }
        let mut snapshot = SpraySnapshot::new([0.0; 4], PULSE_FREQUENCY, PULSE_DUTY_PERCENT);
        for burst in 0..6 {
            snapshot.audio_loop_seconds = burst as f64 * LOOP_SECONDS / LOOP_BURSTS;
            let expected = (burst as f64 * 0.26 + 0.075) / LOOP_SECONDS;
            assert!((f64::from(snapshot.audio_loop_phase(0.0)) - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn selected_wav_matches_fixed_timing_metadata() {
        let clip = petalsonic::ResidentClip::from_path(
            std::path::Path::new(&crate::util::get_project_root())
                .join(crate::audio::runtime_assets::SPRINKLER_LOOP_PATH),
        )
        .unwrap();
        assert_eq!(clip.sample_rate(), LOOP_SAMPLE_RATE);
        assert_eq!(clip.total_frames(), LOOP_FRAMES);
        assert!((f64::from(PULSE_FREQUENCY) * LOOP_SECONDS - 6.0).abs() < 1e-6);
    }
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
