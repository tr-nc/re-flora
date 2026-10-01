use crate::gameplay::camera::CameraPose;
use glam::{Quat, Vec3};

const LAND_SECONDS: f32 = 0.45;
const LIFT_SECONDS: f32 = 0.7;
const WALK_EXIT_ORBIT_DISTANCE: f32 = 0.6;
const POSE_ARC_SEGMENTS: usize = 128;
// Convert radians to scene distance when timing translation and rotation together.
const ROTATION_DISTANCE_WEIGHT: f32 = WALK_EXIT_ORBIT_DISTANCE * 0.25;

fn smootherstep(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * t * (t * (t * 6. - 15.) + 10.)
}

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

pub(super) struct ZoomTransition {
    start: CameraPose,
    end: CameraPose,
    elapsed: f32,
    pose_arc: [f32; POSE_ARC_SEGMENTS + 1],
    pub(super) walking: bool,
}
impl ZoomTransition {
    pub(super) fn new(start: CameraPose, end: CameraPose, walking: bool) -> Self {
        let mut transition = Self {
            start,
            end,
            elapsed: 0.,
            pose_arc: [0.; POSE_ARC_SEGMENTS + 1],
            walking,
        };
        let mut previous = transition.pose_at(0.);
        for i in 1..=POSE_ARC_SEGMENTS {
            let pose = transition.pose_at(i as f32 / POSE_ARC_SEGMENTS as f32);
            transition.pose_arc[i] = transition.pose_arc[i - 1]
                + previous.position.distance(pose.position)
                + ROTATION_DISTANCE_WEIGHT * orientation(previous).angle_between(orientation(pose));
            previous = pose;
        }
        transition
    }

    fn parameter_for_progress(&self, progress: f32) -> f32 {
        let total = self.pose_arc[POSE_ARC_SEGMENTS];
        if total <= f32::EPSILON {
            return progress;
        }
        let distance = progress.clamp(0., 1.) * total;
        let upper = self
            .pose_arc
            .partition_point(|arc| *arc < distance)
            .clamp(1, POSE_ARC_SEGMENTS);
        let lower = upper - 1;
        let span = self.pose_arc[upper] - self.pose_arc[lower];
        let fraction = if span > f32::EPSILON {
            (distance - self.pose_arc[lower]) / span
        } else {
            0.
        };
        (lower as f32 + fraction) / POSE_ARC_SEGMENTS as f32
    }

    fn lift_curve(&self, u: f32) -> (Vec3, Vec3) {
        let length = self.start.position.distance(self.end.position);
        let backward = orientation(self.start) * Vec3::Z;
        // Start by withdrawing, not by applying an artificial upward minimum.
        // Looking up would point the rear axis underground: keep that initial
        // tangent planar while the orientation safely aligns with the curve.
        let safe_backward = Vec3::new(backward.x, backward.y.max(0.), backward.z).normalize();
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
        // Ease the full pose's travelled distance, not just the raw path parameter.
        // Otherwise the early alignment turn can be fast despite a slow position.
        let u = self.parameter_for_progress(smootherstep(t));
        (self.pose_at(u), false)
    }

    fn pose_at(&self, u: f32) -> CameraPose {
        if u <= 0. {
            return self.start;
        }
        if u >= 1. {
            return self.end;
        }
        if self.walking {
            return oriented_pose(
                self.start.position.lerp(self.end.position, u),
                orientation(self.start).slerp(orientation(self.end), u),
                self.start.fov_deg,
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
        oriented_pose(position, q, self.start.fov_deg)
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
    fn level_and_upward_views_start_with_planar_withdrawal_not_vertical_float() {
        for pitch in [0., 45., 89.] {
            let p = pose(pitch);
            let transition = ZoomTransition::new(p, edit_pose_from_walk(p), false);
            let (start, tangent) = transition.lift_curve(0.);
            assert_eq!(start, p.position);
            assert_eq!(tangent.y, 0.);
            let yaw = p.yaw_deg.to_radians();
            let planar_back = Vec3::new(-yaw.sin(), 0., yaw.cos());
            assert!(tangent.normalize().dot(planar_back) > 0.99999);
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
                let u = transition.parameter_for_progress(smootherstep(i as f32 / 100.));
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
    fn whole_pose_speed_eases_at_both_ends_and_peaks_near_the_middle() {
        for pitch in [-89., 0., 80.] {
            let start = pose(pitch);
            let mut transition = ZoomTransition::new(start, edit_pose_from_walk(start), false);
            let mut last = start;
            let mut speeds = Vec::new();
            for _ in 0..100 {
                let p = transition.advance(LIFT_SECONDS / 100.).0;
                speeds.push(
                    last.position.distance(p.position)
                        + 0.15 * orientation(last).angle_between(orientation(p)),
                );
                last = p;
            }
            let peak = speeds
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap()
                .0;
            eprintln!(
                "pitch={pitch} peak_time={} start_step={} end_step={} peak_step={}",
                peak as f32 / 100.,
                speeds[0],
                speeds[99],
                speeds[peak]
            );
            assert!(
                (35..=65).contains(&peak),
                "whole-pose speed peak={peak} pitch={pitch}"
            );
            assert!(speeds[0] < speeds[peak] * 0.03);
            assert!(speeds[99] < speeds[peak] * 0.03);
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
        assert!(a.advance(f32::NAN).0.position.distance(start.position) < 1e-6);
        assert!(a.advance(LIFT_SECONDS / 2.).0.yaw_deg.abs() > 178.);
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
