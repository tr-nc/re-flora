use crate::gameplay::camera::CameraPose;
use glam::Vec3;

const ZOOM_TRANSITION_SECONDS: f32 = 0.45;
const WALK_EXIT_ORBIT_DISTANCE: f32 = 0.6;

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

    pub(super) fn advance(&mut self, dt: f32) -> (CameraPose, bool) {
        if dt.is_finite() && dt > 0. {
            self.elapsed += dt;
        }
        let t = (self.elapsed / ZOOM_TRANSITION_SECONDS).clamp(0., 1.);
        let eased = t * t * (3. - 2. * t);
        let delta = (self.end.yaw_deg - self.start.yaw_deg).to_radians();
        let yaw_delta = delta.sin().atan2(delta.cos()).to_degrees();
        (
            CameraPose {
                position: self.start.position.lerp(self.end.position, eased),
                yaw_deg: self.start.yaw_deg + yaw_delta * eased,
                pitch_deg: self.start.pitch_deg
                    + (self.end.pitch_deg - self.start.pitch_deg) * eased,
                fov_deg: self.start.fov_deg,
            },
            t >= 1.,
        )
    }
}

/// Orbit around the player's current eye anchor, not their current view ray.
/// The scalar yaw survives even when the player is looking straight up/down.
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
            let p = edit_pose_from_walk(pose(pitch));
            assert_eq!(p, target);
            assert_eq!(p.yaw_deg, 72.);
            assert_eq!(p.pitch_deg, -45.);
            assert_eq!(p.fov_deg, 65.);
            let offset = p.position - pose(pitch).position;
            assert!((offset.y - glam::Vec2::new(offset.x, offset.z).length()).abs() < 1e-6);
            assert!((offset.length() - WALK_EXIT_ORBIT_DISTANCE).abs() < 1e-6);
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
        assert_eq!(a.advance(f32::NAN).0, start);
        let middle = a.advance(ZOOM_TRANSITION_SECONDS / 2.).0;
        assert!((middle.yaw_deg - 180.).abs() < 1e-4);
        assert!((middle.position - start.position.lerp(end.position, 0.5)).length() < 1e-6);
        let (last, done) = a.advance(1.);
        assert!(done);
        assert!((last.position - end.position).length() < 1e-6);
        assert_eq!(last.pitch_deg, -45.);
        let mut b = ZoomTransition::new(start, end, false);
        for _ in 0..45 {
            b.advance(0.01);
        }
        assert!((b.advance(0.01).0.position - last.position).length() < 1e-6);
    }
}
