//! One publication clock for world position, heading and articulated wing time.
//! Sampling never feeds held poses back into the high-frequency simulation.
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ButterflyFrame {
    tick: u64,
    fps: u32,
}
impl ButterflyFrame {
    pub fn at(time_seconds: f32, fps: u32) -> Self {
        let fps = fps.clamp(2, 60);
        let time = if time_seconds.is_finite() {
            time_seconds.max(0.)
        } else {
            0.
        };
        Self {
            tick: (f64::from(time) * f64::from(fps)).floor() as u64,
            fps,
        }
    }
    pub fn time_seconds(self) -> f32 {
        (self.tick as f64 / f64::from(self.fps)) as f32
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ButterflyPose {
    frame: ButterflyFrame,
    pub position: Vec3,
    pub velocity: Vec3,
}
impl ButterflyPose {
    pub fn time_seconds(self) -> f32 {
        self.frame.time_seconds()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ButterflyPresentation(Option<ButterflyPose>);
impl ButterflyPresentation {
    pub fn sample(
        &mut self,
        frame: ButterflyFrame,
        position: Vec3,
        velocity: Vec3,
    ) -> ButterflyPose {
        if self.0.is_none_or(|pose| pose.frame != frame) {
            self.0 = Some(ButterflyPose {
                frame,
                position,
                velocity,
            });
        }
        self.0.unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_clock_holds_position_heading_and_time_at_all_supported_rates() {
        for fps in 2..=60 {
            let mut state = ButterflyPresentation::default();
            for tick in 0..fps {
                let time = (tick as f32 + 0.1) / fps as f32;
                let frame = ButterflyFrame::at(time, fps);
                let first = state.sample(frame, Vec3::splat(time), Vec3::X);
                let held = state.sample(
                    ButterflyFrame::at(time + 0.5 / fps as f32, fps),
                    Vec3::Y,
                    Vec3::Z,
                );
                assert_eq!(first.position, held.position);
                assert_eq!(first.velocity, held.velocity);
                assert_eq!(first.time_seconds(), held.time_seconds());
            }
        }
    }
    #[test]
    fn live_rate_edits_and_rewind_republish_the_whole_pose() {
        let mut state = ButterflyPresentation::default();
        state.sample(ButterflyFrame::at(1., 8), Vec3::ZERO, Vec3::X);
        let changed = state.sample(ButterflyFrame::at(1., 16), Vec3::Y, Vec3::Z);
        assert_eq!(changed.position, Vec3::Y);
        assert_eq!(changed.velocity, Vec3::Z);
        assert_eq!(
            state
                .sample(ButterflyFrame::at(0., 16), Vec3::ONE, Vec3::ZERO)
                .position,
            Vec3::ONE
        );
    }
}
