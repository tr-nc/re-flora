//! Bounded-error streaming guide and locally fitted, tangent-continuous curves.
//! Radius is a planning preference, not a minimum legal turn. Consumers validate
//! actual band/paver geometry; published exit sections remain immutable.
use super::{xz, CourseSection, VOXEL_SIZE};
use glam::{Vec2, Vec3};
mod containment;
mod smoothing;

// Oblique cursor rays move sideways when crossing voxel-height soil risers.
// Simplify their sub-voxel lateral noise before interpreting it as a corner;
// this remains a bounded corridor, not a clearance or sharp-turn exemption.
const INPUT_GUIDE_TOLERANCE: f32 = VOXEL_SIZE;
const MAX_GUIDE_POINTS: usize = 4096;

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

#[derive(Clone, Copy, Default)]
struct Corner {
    radius: f32,
    trim: f32,
    angle: f32,
    partial: bool,
    broken: bool,
}
impl Corner {
    fn new(incoming: Vec2, outgoing: Vec2, radius: f32, open_end: bool) -> Self {
        let direction = incoming.normalize_or_zero();
        let next = outgoing.normalize_or_zero();
        let angle = direction.perp_dot(next).atan2(direction.dot(next)).abs();
        if angle < 1e-4 {
            return Self::default();
        }
        // A reversal has no tangent fillet. Other turns are judged by their
        // resulting cells, not an arbitrary maximum angle or minimum radius.
        if angle >= std::f32::consts::PI - 1e-4 || radius <= 1e-8 {
            return Self {
                broken: true,
                ..Self::default()
            };
        }
        let trim = radius * (angle * 0.5).tan();
        if open_end && outgoing.length() < trim {
            // The cursor fixes a POSITION, not the eventual outgoing tangent.
            // End exactly at it on an incomplete circle tangent to the incoming
            // line. Its radius can stay generous even for a tiny turning tail.
            let lateral = direction.perp_dot(outgoing).abs();
            let along = direction.dot(outgoing);
            let advance = (2. * radius * lateral - lateral * lateral).max(0.).sqrt();
            Self {
                radius,
                trim: advance - along,
                angle: advance.atan2(radius - lateral),
                partial: true,
                broken: false,
            }
        } else {
            Self {
                radius,
                trim,
                angle,
                partial: false,
                broken: false,
            }
        }
    }

    fn fit_trim(&mut self, trim: f32, incoming: Vec2, outgoing: Vec2, open_end: bool) {
        let direction = incoming.normalize();
        let next = outgoing.normalize();
        let angle = direction.perp_dot(next).atan2(direction.dot(next)).abs();
        let radius = if open_end && trim > outgoing.length() {
            let along = direction.dot(outgoing);
            let lateral = direction.perp_dot(outgoing).abs();
            let advance = trim + along;
            if advance <= 0. || lateral <= 1e-8 {
                self.broken = true;
                return;
            }
            (advance * advance + lateral * lateral) / (2. * lateral)
        } else {
            trim / (angle * 0.5).tan()
        };
        *self = Self::new(incoming, outgoing, radius.min(self.radius), open_end);
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
    minimum_radius: f32,
    origin: Vec3,
    heading: Vec3,
}
impl RoundedPath {
    fn new(points: &[Vec3], radius: f32, heading: Vec3) -> Self {
        Self::fit(points, radius, heading, false)
    }
    fn fit(points: &[Vec3], radius: f32, heading: Vec3, smooth: bool) -> Self {
        let mut path = Self {
            spans: Vec::new(),
            length: 0.,
            minimum_radius: f32::INFINITY,
            origin: points[0],
            heading,
        };
        let n = points.len();
        if n == 1 {
            return path;
        }
        let mut corners = vec![Corner::default(); n];
        for i in 1..n - 1 {
            let incoming = xz(points[i] - points[i - 1]);
            let outgoing = xz(points[i + 1] - points[i]);
            let angle = incoming
                .perp_dot(outgoing)
                .atan2(incoming.dot(outgoing))
                .abs();
            // Estimate the drawn curvature instead of sharpening an already
            // gentle curve to the default radius. Keep the local smoothing
            // support bounded; neither limit is a minimum legal turn radius.
            let tangent = (angle * 0.5).tan();
            let observed = (incoming.length() + outgoing.length()) / (4. * tangent);
            let preferred = radius
                .max(observed)
                .min(2. * radius)
                .min(2. * radius / tangent);
            corners[i] = Corner::new(incoming, outgoing, preferred, i == n - 2);
        }
        // Allocate the available chord between neighboring fillets. Shrinking
        // only frees space on previously fitted chords; no continuity break is
        // needed merely because the preferred radii do not fit.
        for i in 0..n - 2 {
            let length = xz(points[i + 1] - points[i]).length();
            let total = corners[i].trim + corners[i + 1].trim;
            if total > length && smooth {
                smoothing::balance_radii(&mut corners, i, length, points);
            } else if total > length {
                let scale = length / total;
                for j in [i, i + 1] {
                    if corners[j].trim > 0. {
                        let trim = corners[j].trim * scale;
                        corners[j].fit_trim(
                            trim,
                            xz(points[j] - points[j - 1]),
                            xz(points[j + 1] - points[j]),
                            j == n - 2,
                        );
                    }
                }
            }
        }
        path.minimum_radius = corners
            .iter()
            .filter(|c| c.radius > 0.)
            .map(|c| c.radius)
            .fold(f32::INFINITY, f32::min);
        let mut from = points[0];
        let mut continuity = 0;
        for i in 1..n - 1 {
            let incoming = points[i] - points[i - 1];
            let outgoing = points[i + 1] - points[i];
            let corner = corners[i];
            if corner.broken || corner.trim <= 0. {
                path.line(from, points[i], continuity);
                from = points[i];
                if corner.broken {
                    continuity += 1;
                }
                continue;
            }
            let start = points[i] - incoming * (corner.trim / xz(incoming).length());
            let end = if corner.partial {
                points[n - 1]
            } else {
                points[i] + outgoing * (corner.trim / xz(outgoing).length())
            };
            path.line(from, start, continuity);
            let direction = xz(incoming).normalize();
            let sign = direction.perp_dot(xz(outgoing)).signum();
            let center = xz(start) + Vec2::new(-direction.y, direction.x) * (corner.radius * sign);
            let length = f64::from(corner.radius * corner.angle);
            path.spans.push(Span {
                start: path.length,
                length,
                continuity,
                kind: SpanKind::Arc {
                    center,
                    radial: xz(start) - center,
                    angle: corner.angle * sign,
                    from_y: start.y,
                    to_y: end.y,
                },
            });
            path.length += length;
            if corner.partial {
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
    fn input_filter_preserves_a_real_short_turn_with_an_open_tangent() {
        let mut guide = CurveGuide::new(Vec3::ZERO);
        assert!(guide.push(Vec3::X * 0.3));
        assert!(guide.push(Vec3::new(0.3, 0., 0.012)));
        let curve = guide.rounded(0.06, Vec3::X);
        assert_eq!(guide.points.len(), 3);
        assert!(curve.spans.iter().all(|s| s.continuity == 0));
        assert!(curve.section(curve.length).center.distance(guide.last()) < 1e-6);
    }

    #[test]
    fn gentle_terminal_residual_ends_at_the_cursor() {
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
            assert!(xz(curve.section(curve.length).center - guide.last()).length() <= 1e-6);
        }
    }
    #[test]
    fn microscopic_partial_arc_stays_finite_and_reaches_its_end() {
        for length in [1e-9, 0.003, 0.04] {
            for sign in [-1., 1.] {
                let end = Vec3::new(0.3, 0., sign * length);
                let curve = RoundedPath::new(&[Vec3::ZERO, Vec3::X * 0.3, end], 0.12, Vec3::X);
                let section = curve.section(curve.length);
                assert!(section.center.is_finite() && section.across.is_finite());
                assert!(section.center.distance(end) < 1e-6);
            }
        }
    }

    #[test]
    fn short_terminal_turn_uses_a_partial_arc_not_a_break() {
        let guide = RoundedPath::new(
            &[Vec3::ZERO, Vec3::X * 0.3, Vec3::new(0.3, 0., 0.003)],
            0.06,
            Vec3::X,
        );
        assert!(guide.spans.iter().all(|s| s.continuity == 0));
        assert!(
            guide
                .section(guide.length)
                .center
                .distance(Vec3::new(0.3, 0., 0.003))
                < 1e-6
        );
        let SpanKind::Arc { angle, .. } = guide.spans.last().unwrap().kind else {
            panic!("short tail must end on an arc");
        };
        assert!(angle > 0. && angle < std::f32::consts::FRAC_PI_2);
    }
}
