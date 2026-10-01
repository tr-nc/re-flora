use crate::gameplay::camera::CameraPose;
use glam::{Quat, Vec3};

const LAND_SECONDS: f32 = 0.45;
const LIFT_SECONDS: f32 = 0.7;
const WALK_EXIT_ORBIT_DISTANCE: f32 = 0.6;
const PREVIEW_HEIGHT: f32 = 0.025;
const PREVIEW_IDLE_SECONDS: f32 = 0.65;
const PREVIEW_FREQUENCY: f32 = 18.;
const PROGRESS_PER_LINE: f32 = 0.22;
const MAX_EVENT_PROGRESS: f32 = 0.35;
const COMMIT_PROGRESS: f32 = 0.8;

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn orientation(pose: CameraPose) -> Quat {
    Quat::from_rotation_y(-pose.yaw_deg.to_radians())
        * Quat::from_rotation_x(pose.pitch_deg.to_radians())
}
fn oriented_pose(position: Vec3, q: Quat, fov_deg: f32) -> CameraPose {
    let front = q * Vec3::NEG_Z;
    CameraPose {
        position,
        yaw_deg: front.x.atan2(-front.z).to_degrees(),
        pitch_deg: front
            .y
            .atan2(glam::Vec2::new(front.x, front.z).length())
            .to_degrees(),
        fov_deg,
    }
}

pub(super) enum PreviewStep {
    Active(CameraPose),
    Recovered(CameraPose),
    Commit {
        start: CameraPose,
        anchor: CameraPose,
    },
}

/// Reversible camera-only intent. The player stays at the original collision anchor.
/// Mouse look remains live: recovery restores position, never an obsolete orientation.
pub(super) struct ZoomPreview {
    anchor: CameraPose,
    progress: f32,
    velocity: f32,
    target: f32,
    idle: f32,
}
impl ZoomPreview {
    pub(super) fn new(anchor: CameraPose) -> Self {
        Self {
            anchor,
            progress: 0.,
            velocity: 0.,
            target: 0.,
            idle: 0.,
        }
    }
    pub(super) fn scroll(&mut self, lines: f32) {
        if !lines.is_finite() {
            return;
        }
        self.target = (self.target
            - (lines * PROGRESS_PER_LINE).clamp(-MAX_EVENT_PROGRESS, MAX_EVENT_PROGRESS))
        .clamp(0., 1.);
        self.idle = 0.;
    }
    pub(super) fn advance(&mut self, dt: f32, look: CameraPose) -> PreviewStep {
        self.anchor.yaw_deg = look.yaw_deg;
        self.anchor.pitch_deg = look.pitch_deg;
        if dt.is_finite() && dt > 0. {
            // Split at the idle deadline so recovery timing is frame-rate independent.
            let until_idle = (PREVIEW_IDLE_SECONDS - self.idle).max(0.);
            let active_dt = dt.min(until_idle);
            self.spring(active_dt);
            self.idle += dt;
            if self.idle >= PREVIEW_IDLE_SECONDS {
                self.target = 0.;
                self.spring(dt - active_dt);
            }
        }
        let start = CameraPose {
            position: self.anchor.position + Vec3::Y * (PREVIEW_HEIGHT * self.progress),
            ..self.anchor
        };
        if self.target >= 1. - 1e-6 && self.progress >= COMMIT_PROGRESS {
            PreviewStep::Commit {
                start,
                anchor: self.anchor,
            }
        } else if self.target == 0. && self.progress < 0.0005 && self.velocity.abs() < 0.005 {
            PreviewStep::Recovered(self.anchor)
        } else {
            PreviewStep::Active(start)
        }
    }
    fn spring(&mut self, dt: f32) {
        if dt <= 0. {
            return;
        }
        // Exact critically-damped spring for a fixed target; no Euler dt instability.
        let offset = self.progress - self.target;
        let c = self.velocity + PREVIEW_FREQUENCY * offset;
        let decay = (-PREVIEW_FREQUENCY * dt).exp();
        self.progress = (self.target + (offset + c * dt) * decay).clamp(0., 1.);
        self.velocity = (self.velocity - PREVIEW_FREQUENCY * c * dt) * decay;
    }
}

pub(super) struct ZoomTransition {
    start: CameraPose,
    end: CameraPose,
    elapsed: f32,
    pub(super) walking: bool,
}
impl ZoomTransition {
    pub(super) fn new(start: CameraPose, end: CameraPose, walking: bool) -> Self {
        Self {
            start,
            end,
            elapsed: 0.,
            walking,
        }
    }
    fn lift_curve(&self, u: f32) -> (Vec3, Vec3) {
        let length = self.start.position.distance(self.end.position);
        let backward = orientation(self.start) * Vec3::Z;
        // Looking up points the rear axis underground: a short alignment region
        // relaxes that constraint rather than diving into the ground first.
        let safe_backward = Vec3::new(backward.x, backward.y.max(0.25), backward.z).normalize();
        let m0 = safe_backward * length * 0.4;
        let m1 = orientation(self.end) * Vec3::Z * length * 0.7;
        let a = self.start.position;
        let b = self.end.position;
        let u2 = u * u;
        let u3 = u2 * u;
        let position = a * (2. * u3 - 3. * u2 + 1.)
            + m0 * (u3 - 2. * u2 + u)
            + b * (-2. * u3 + 3. * u2)
            + m1 * (u3 - u2);
        let tangent = a * (6. * u2 - 6. * u)
            + m0 * (3. * u2 - 4. * u + 1.)
            + b * (-6. * u2 + 6. * u)
            + m1 * (3. * u2 - 2. * u);
        (position, tangent)
    }
    pub(super) fn advance(&mut self, dt: f32) -> (CameraPose, bool) {
        if dt.is_finite() && dt > 0. {
            self.elapsed += dt;
        }
        let duration = if self.walking {
            LAND_SECONDS
        } else {
            LIFT_SECONDS
        };
        let t = (self.elapsed / duration).clamp(0., 1.);
        if t >= 1. {
            return (self.end, true);
        }
        let u = smoothstep(t);
        if self.walking {
            return (
                oriented_pose(
                    self.start.position.lerp(self.end.position, u),
                    orientation(self.start).slerp(orientation(self.end), u),
                    self.start.fov_deg,
                ),
                false,
            );
        }
        let (position, tangent) = self.lift_curve(u);
        let back = tangent.normalize();
        let tangent_pose = CameraPose {
            yaw_deg: (-back.x).atan2(back.z).to_degrees(),
            pitch_deg: -back
                .y
                .atan2(glam::Vec2::new(back.x, back.z).length())
                .to_degrees(),
            ..self.end
        };
        let aligned = smoothstep(u / 0.25);
        let q = orientation(self.start).slerp(orientation(tangent_pose), aligned);
        (oriented_pose(position, q, self.start.fov_deg), false)
    }
}

pub(super) fn edit_pose_from_walk(pose: CameraPose) -> CameraPose {
    let yaw = pose.yaw_deg.to_radians();
    let radius = WALK_EXIT_ORBIT_DISTANCE * std::f32::consts::FRAC_1_SQRT_2;
    CameraPose {
        position: pose.position + Vec3::new(-yaw.sin(), 1., yaw.cos()) * radius,
        pitch_deg: -45.,
        ..pose
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pose(pitch: f32) -> CameraPose {
        CameraPose {
            position: Vec3::new(1., 0.2, 3.),
            yaw_deg: 72.,
            pitch_deg: pitch,
            fov_deg: 65.,
        }
    }
    #[test]
    fn walking_exit_ignores_pitch_and_preserves_yaw_and_fov() {
        let target = edit_pose_from_walk(pose(0.));
        for pitch in [-89., -45., 0., 45., 89.] {
            assert_eq!(edit_pose_from_walk(pose(pitch)), target);
            let offset = target.position - pose(pitch).position;
            assert!((offset.y - glam::Vec2::new(offset.x, offset.z).length()).abs() < 1e-6);
            assert!((offset.length() - WALK_EXIT_ORBIT_DISTANCE).abs() < 1e-6);
        }
    }
    #[test]
    fn single_notch_and_large_coalesced_event_preview_then_recover() {
        for lines in [-1., -100.] {
            let mut preview = ZoomPreview::new(pose(80.));
            preview.scroll(lines);
            let mut height: f32 = 0.;
            let mut recovered = false;
            for _ in 0..180 {
                match preview.advance(1. / 60., pose(80.)) {
                    PreviewStep::Active(p) => {
                        height = height.max(p.position.y - pose(80.).position.y);
                        assert_eq!(p.pitch_deg, 80.);
                    }
                    PreviewStep::Recovered(p) => {
                        assert_eq!(p, pose(80.));
                        recovered = true;
                        break;
                    }
                    PreviewStep::Commit { .. } => panic!("one event cannot confirm a mode switch"),
                }
            }
            assert!(height > 0.002 && height <= PREVIEW_HEIGHT);
            assert!(recovered);
        }
    }
    #[test]
    fn deliberate_slow_or_fast_scroll_commits_and_reverse_recovers() {
        for interval in [0.05, 0.4] {
            let mut preview = ZoomPreview::new(pose(0.));
            let mut committed = false;
            'input: for _ in 0..6 {
                preview.scroll(-1.);
                for _ in 0..(interval * 100.) as usize {
                    if matches!(preview.advance(0.01, pose(0.)), PreviewStep::Commit { .. }) {
                        committed = true;
                        break 'input;
                    }
                }
            }
            assert!(committed, "interval={interval}");
        }
        let mut preview = ZoomPreview::new(pose(0.));
        preview.scroll(-1.);
        preview.advance(0.1, pose(0.));
        preview.scroll(1.);
        assert!(matches!(
            preview.advance(1., pose(0.)),
            PreviewStep::Recovered(_)
        ));
    }
    #[test]
    fn recovery_tracks_mouse_look_and_is_frame_rate_independent() {
        let mut a = ZoomPreview::new(pose(0.));
        let mut b = ZoomPreview::new(pose(0.));
        a.scroll(-1.);
        b.scroll(-1.);
        let look = CameraPose {
            yaw_deg: -150.,
            pitch_deg: 40.,
            ..pose(0.)
        };
        let mut pa = look;
        let mut pb = look;
        for _ in 0..100 {
            if let PreviewStep::Active(p) = a.advance(0.01, look) {
                pa = p;
            }
        }
        for _ in 0..20 {
            if let PreviewStep::Active(p) = b.advance(0.05, look) {
                pb = p;
            }
        }
        assert!(pa.position.distance(pb.position) < 1e-5);
        match a.advance(2., look) {
            PreviewStep::Recovered(p) => {
                assert_eq!(p.yaw_deg, -150.);
                assert_eq!(p.pitch_deg, 40.);
                assert_eq!(p.position, pose(0.).position);
            }
            _ => panic!(),
        }
    }
    #[test]
    fn lift_is_monotone_and_rear_axis_matches_path_after_alignment() {
        for pitch in [-89., -45., 0., 45., 89.] {
            let start = pose(pitch);
            let end = edit_pose_from_walk(start);
            let mut transition = ZoomTransition::new(start, end, false);
            let mut last = start.position;
            for i in 1..100 {
                let (p, _) = transition.advance(LIFT_SECONDS / 100.);
                assert!(p.position.is_finite());
                assert!(p.position.y >= last.y - 1e-6);
                let u = smoothstep(i as f32 / 100.);
                if u > 0.25 {
                    let (_, tangent) = transition.lift_curve(u);
                    assert!((orientation(p) * Vec3::Z).dot(tangent.normalize()) > 0.9999);
                    assert!((orientation(p) * Vec3::X).y.abs() < 1e-5, "no roll");
                }
                last = p.position;
            }
            let (p, done) = transition.advance(1.);
            assert!(done);
            assert_eq!(p, end);
        }
    }
    #[test]
    fn transition_is_bounded_time_based_and_takes_short_yaw_path() {
        let start = CameraPose {
            yaw_deg: 179.,
            ..pose(80.)
        };
        let end = CameraPose {
            yaw_deg: -179.,
            ..edit_pose_from_walk(start)
        };
        let mut a = ZoomTransition::new(start, end, false);
        let original = a.advance(f32::NAN).0;
        assert!(original.position.distance(start.position) < 1e-6);
        let middle = a.advance(LIFT_SECONDS / 2.).0;
        assert!(middle.yaw_deg.abs() > 178.);
        let (last, done) = a.advance(1.);
        assert!(done);
        assert_eq!(last, end);
        let mut b = ZoomTransition::new(start, end, false);
        for _ in 0..100 {
            b.advance(LIFT_SECONDS / 100.);
        }
        assert_eq!(b.advance(0.01).0, last);
    }
}
