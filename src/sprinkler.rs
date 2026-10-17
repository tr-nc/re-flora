//! Sprinkler hardware dimensions and tick-driven wetting axis, with CPU mesh-pose contract checks.
//! Rigid-part surface caches and native fallback share this pose; scene resolve stays independent.
use glam::Vec3;

pub(crate) const WORLD_SCALE: f32 = 1.0 / 256.0;
pub(crate) const NOZZLE_HEIGHT: f32 = 4.0;
pub(crate) const NOZZLE_REACH: f32 = 3.1;
pub(crate) const HEAD_LIFT: f32 = 1.0;
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
mod tests {
    use super::*;

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
                    cases += 1;
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
