//! Bounded-error streaming guide and tangent-continuous rounded centerline.
//! Only the last control point moves; sections more than two radii behind the
//! cursor are stable. Published terrain never needs to be erased/rebuilt.
use super::{xz, CourseSection, VOXEL_SIZE};
use glam::{Vec2, Vec3};

// Oblique cursor rays move sideways when crossing voxel-height soil risers.
// Simplify their sub-voxel lateral noise before interpreting it as a corner;
// this remains a bounded corridor, not a clearance or sharp-turn exemption.
const INPUT_GUIDE_TOLERANCE: f32 = VOXEL_SIZE;
const TERMINAL_ARC_TOLERANCE: f32 = VOXEL_SIZE * 0.125;
const MAX_GUIDE_POINTS: usize = 4096;
const MAX_CORNER_ANGLE: f32 = std::f32::consts::PI * 2. / 3.;

#[derive(Clone)]
pub(super) struct CurveGuide {
    points: Vec<Vec3>,
    reference: f32,
    low: f32,
    high: f32,
}
impl CurveGuide {
    pub fn new(start: Vec3) -> Self {
        Self {
            points: vec![start],
            reference: 0.,
            low: 0.,
            high: 0.,
        }
    }
    pub fn last(&self) -> Vec3 {
        *self.points.last().unwrap()
    }
    pub fn push(&mut self, point: Vec3) -> bool {
        let delta = xz(point - self.last());
        if delta.length() < INPUT_GUIDE_TOLERANCE * 0.5 {
            return true;
        }
        if self.points.len() == 1 {
            self.points.push(point);
            self.reset_cone();
            return true;
        }
        let anchor = self.points[self.points.len() - 2];
        let direction = xz(point - anchor);
        let length = direction.length();
        let angle = self.unwrapped_angle(direction);
        let previous = xz(self.last() - anchor);
        // A swung-door angular corridor bounds ALL discarded points, unlike
        // repeatedly testing only the last point against an ever-changing chord.
        if length > 1e-8
            && angle >= self.low
            && angle <= self.high
            && previous.dot(direction / length) <= length + INPUT_GUIDE_TOLERANCE
        {
            *self.points.last_mut().unwrap() = point;
            self.constrain(angle, length);
        } else {
            if self.points.len() >= MAX_GUIDE_POINTS {
                return false;
            }
            self.points.push(point);
            self.reset_cone();
        }
        true
    }
    fn unwrapped_angle(&self, direction: Vec2) -> f32 {
        let mut difference = direction.y.atan2(direction.x) - self.reference;
        while difference > std::f32::consts::PI {
            difference -= std::f32::consts::TAU;
        }
        while difference < -std::f32::consts::PI {
            difference += std::f32::consts::TAU;
        }
        self.reference + difference
    }
    fn reset_cone(&mut self) {
        let delta = xz(self.last() - self.points[self.points.len() - 2]);
        self.reference = delta.y.atan2(delta.x);
        self.low = f32::NEG_INFINITY;
        self.high = f32::INFINITY;
        self.constrain(self.reference, delta.length());
    }
    fn constrain(&mut self, angle: f32, length: f32) {
        let tolerance = (INPUT_GUIDE_TOLERANCE / length).min(1.).asin();
        self.low = self.low.max(angle - tolerance);
        self.high = self.high.min(angle + tolerance);
    }
    pub fn rounded(&self, radius: f32, heading: Vec3) -> RoundedPath {
        RoundedPath::new(&self.points, radius, heading)
    }
}

struct Span {
    start: f64,
    length: f64,
    continuity: u32,
    kind: SpanKind,
}
enum SpanKind {
    Line {
        from: Vec3,
        to: Vec3,
    },
    Arc {
        center: Vec2,
        radial: Vec2,
        angle: f32,
        from_y: f32,
        to_y: f32,
    },
}
pub(super) struct RoundedPath {
    spans: Vec<Span>,
    pub length: f64,
    origin: Vec3,
    heading: Vec3,
}
impl RoundedPath {
    fn new(points: &[Vec3], radius: f32, heading: Vec3) -> Self {
        let mut path = Self {
            spans: Vec::new(),
            length: 0.,
            origin: points[0],
            heading,
        };
        let n = points.len();
        if n == 1 {
            return path;
        }
        let mut trims = vec![0.; n];
        let mut broken = vec![false; n];
        let mut angles = vec![0.; n];
        for i in 1..n - 1 {
            let incoming = xz(points[i] - points[i - 1]).normalize_or_zero();
            let outgoing = xz(points[i + 1] - points[i]).normalize_or_zero();
            let angle = incoming.dot(outgoing).clamp(-1., 1.).acos();
            angles[i] = angle;
            if angle > MAX_CORNER_ANGLE {
                broken[i] = true;
            } else if angle > 1e-4 {
                trims[i] = radius * (angle * 0.5).tan();
            }
        }
        // Do not shrink below the safe radius or fold the inside lanes. A corner
        // that cannot fit gets an explicit continuity break, not inverted cells.
        let mut terminal_fraction = 1.;
        for i in 0..n - 1 {
            let length = xz(points[i + 1] - points[i]).length();
            if trims[i] + trims[i + 1] > length * 0.98 {
                // Swung-door simplification can leave a tiny final chord in an
                // otherwise gentle curve. End ON the safe-radius arc when that
                // endpoint remains within the stricter terminal arc tolerance.
                // Never shrink its radius, extend the drawn path, or forgive a
                // genuinely impossible short sharp turn.
                if i == n - 2 && !broken[i] && angles[i] > 1e-4 {
                    let fraction = 1. + (length - trims[i]) / (radius * angles[i]);
                    let incoming = xz(points[i] - points[i - 1]).normalize();
                    let sign = incoming.perp_dot(xz(points[i + 1] - points[i])).signum();
                    let start = xz(points[i]) - incoming * trims[i];
                    let center = start + Vec2::new(-incoming.y, incoming.x) * (radius * sign);
                    let radial = start - center;
                    let (sin, cos) = (angles[i] * sign * fraction).sin_cos();
                    let end = center
                        + Vec2::new(
                            radial.x * cos - radial.y * sin,
                            radial.x * sin + radial.y * cos,
                        );
                    if fraction > 0.
                        && fraction <= 1.
                        && end.distance(xz(points[i + 1])) <= TERMINAL_ARC_TOLERANCE
                    {
                        terminal_fraction = fraction;
                        continue;
                    }
                }
                if i > 0 {
                    broken[i] = true;
                }
                if i + 1 < n - 1 {
                    broken[i + 1] = true;
                }
            }
        }
        let mut from = points[0];
        let mut continuity = 0;
        for i in 1..n - 1 {
            let incoming = points[i] - points[i - 1];
            let outgoing = points[i + 1] - points[i];
            if broken[i] || trims[i] == 0. {
                path.line(from, points[i], continuity);
                from = points[i];
                if broken[i] {
                    continuity += 1;
                }
                continue;
            }
            let start = points[i] - incoming * (trims[i] / xz(incoming).length());
            let end = points[i] + outgoing * (trims[i] / xz(outgoing).length());
            path.line(from, start, continuity);
            let direction = xz(incoming).normalize();
            let sign = direction.perp_dot(xz(outgoing)).signum();
            let center = xz(start) + Vec2::new(-direction.y, direction.x) * (radius * sign);
            let fraction = if i == n - 2 { terminal_fraction } else { 1. };
            let length = f64::from(radius * angles[i] * fraction);
            path.spans.push(Span {
                start: path.length,
                length,
                continuity,
                kind: SpanKind::Arc {
                    center,
                    radial: xz(start) - center,
                    angle: angles[i] * sign * fraction,
                    from_y: start.y,
                    to_y: if fraction < 1. {
                        points[n - 1].y
                    } else {
                        end.y
                    },
                },
            });
            path.length += length;
            if fraction < 1. {
                return path;
            }
            from = end;
        }
        path.line(from, points[n - 1], continuity);
        path
    }
    fn line(&mut self, from: Vec3, to: Vec3, continuity: u32) {
        let length = f64::from(xz(to - from).length());
        if length <= 1e-8 {
            return;
        }
        self.spans.push(Span {
            start: self.length,
            length,
            continuity,
            kind: SpanKind::Line { from, to },
        });
        self.length += length;
    }
    pub fn section(&self, distance: f64) -> CourseSection {
        let Some(span) = self
            .spans
            .iter()
            .find(|span| distance <= span.start + span.length)
            .or_else(|| self.spans.last())
        else {
            return CourseSection::new(
                self.origin + self.heading * distance as f32,
                self.heading,
                0,
            );
        };
        let t = ((distance - span.start) / span.length) as f32;
        let (center, forward) = match span.kind {
            SpanKind::Line { from, to } => (
                from.lerp(to, t),
                Vec3::new(to.x - from.x, 0., to.z - from.z).normalize(),
            ),
            SpanKind::Arc {
                center,
                radial,
                angle,
                from_y,
                to_y,
            } => {
                // Extrapolate terminal caps along the tangent, never curl a cap
                // around an arc if its station lies outside the known centerline.
                let clamped = t.clamp(0., 1.);
                let rotation = angle * clamped;
                let (sin, cos) = rotation.sin_cos();
                let radial = Vec2::new(
                    radial.x * cos - radial.y * sin,
                    radial.x * sin + radial.y * cos,
                );
                let tangent = Vec2::new(-radial.y, radial.x).normalize() * angle.signum();
                let p = center + radial + tangent * ((t - clamped) * span.length as f32);
                (
                    Vec3::new(p.x, from_y + (to_y - from_y) * t, p.y),
                    Vec3::new(tangent.x, 0., tangent.y),
                )
            }
        };
        CourseSection::new(center, forward, span.continuity)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_noise_simplification_stays_within_one_voxel_corridor() {
        let mut guide = CurveGuide::new(Vec3::ZERO);
        let mut inputs = vec![Vec3::ZERO];
        for i in 1..=80 {
            let point = Vec3::new(
                i as f32 * 0.01,
                0.,
                ((i / 2) % 2) as f32 * VOXEL_SIZE * 4. / 3.,
            );
            inputs.push(point);
            assert!(guide.push(point));
            for input in &inputs {
                let distance = guide
                    .points
                    .windows(2)
                    .map(|s| {
                        let a = xz(s[0]);
                        let delta = xz(s[1]) - a;
                        let t =
                            ((xz(*input) - a).dot(delta) / delta.length_squared()).clamp(0., 1.);
                        xz(*input).distance(a + delta * t)
                    })
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    distance <= INPUT_GUIDE_TOLERANCE + 1e-6,
                    "input {input:?} drifted {distance}"
                );
            }
        }
        assert!(guide.points.len() < inputs.len());
    }

    #[test]
    fn input_filter_preserves_a_real_short_sharp_turn() {
        let mut guide = CurveGuide::new(Vec3::ZERO);
        assert!(guide.push(Vec3::X * 0.3));
        assert!(guide.push(Vec3::new(0.3, 0., 0.012)));
        let curve = guide.rounded(0.06, Vec3::X);
        assert!(curve.spans.iter().any(|s| s.continuity > 0));
    }

    #[test]
    fn gentle_terminal_residual_ends_on_safe_arc_with_bounded_error() {
        for sign in [-1., 1.] {
            let start = Vec3::ONE;
            let mut guide = CurveGuide::new(start);
            for i in 1..=90 {
                let t = std::f32::consts::FRAC_PI_2 * i as f32 / 90.;
                assert!(
                    guide.push(start + Vec3::new(0.09 * t.sin(), 0., sign * 0.09 * (1. - t.cos())))
                );
            }
            let curve = guide.rounded(0.06, Vec3::X);
            assert!(curve.spans.iter().all(|s| s.continuity == 0));
            assert!(
                xz(curve.section(curve.length).center - guide.last()).length()
                    <= TERMINAL_ARC_TOLERANCE + 1e-6
            );
        }
    }
    #[test]
    fn short_sharp_terminal_turn_still_has_an_explicit_break() {
        let guide = RoundedPath::new(
            &[Vec3::ZERO, Vec3::X * 0.3, Vec3::new(0.3, 0., 0.003)],
            0.06,
            Vec3::X,
        );
        assert!(guide.spans.iter().any(|s| s.continuity > 0));
    }
}
