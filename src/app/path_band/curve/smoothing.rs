//! Optional bounded inward fitting. Original input endpoints and ray directions
//! remain authoritative; this does not relax footprint or overlap validation.
use super::*;

pub(super) fn balance_radii(corners: &mut [Corner], i: usize, available: f32, points: &[Vec3]) {
    let caps = [corners[i], corners[i + 1]];
    let fit = |j: usize, radius: f32| {
        let cap = caps[j - i];
        if cap.trim == 0. {
            return cap;
        }
        Corner::new(
            xz(points[j] - points[j - 1]),
            xz(points[j + 1] - points[j]),
            radius.min(cap.radius),
            j == points.len() - 2,
        )
    };
    let mut low = 0.;
    let mut high = caps[0].radius.max(caps[1].radius);
    for _ in 0..40 {
        let radius = (low + high) * 0.5;
        if fit(i, radius).trim + fit(i + 1, radius).trim <= available {
            low = radius;
        } else {
            high = radius;
        }
    }
    corners[i] = fit(i, low);
    corners[i + 1] = fit(i + 1, low);
}

impl CurveGuide {
    pub fn smoothed(&self, radius: f32, heading: Vec3) -> RoundedPath {
        struct Node {
            point: Vec3,
            first: usize,
            last: usize,
        }
        let mut nodes: Vec<_> = self
            .points
            .iter()
            .enumerate()
            .map(|(i, &point)| Node {
                point,
                first: i,
                last: i,
            })
            .collect();
        for _ in 0..self.points.len() {
            let mut changed = false;
            for i in 1..nodes.len().saturating_sub(2) {
                let [a, b, c, d] = [
                    nodes[i - 1].point,
                    nodes[i].point,
                    nodes[i + 1].point,
                    nodes[i + 2].point,
                ];
                let incoming = xz(b - a).normalize_or_zero();
                let bridge = xz(c - b).normalize_or_zero();
                let outgoing = xz(d - c).normalize_or_zero();
                let alpha = incoming.perp_dot(bridge).atan2(incoming.dot(bridge));
                let beta = bridge.perp_dot(outgoing).atan2(bridge.dot(outgoing));
                if alpha * beta <= 0. || (alpha + beta).abs() >= std::f32::consts::PI - 1e-4 {
                    continue;
                }
                let divisor = incoming.perp_dot(outgoing);
                if divisor.abs() < 1e-8 {
                    continue;
                }
                let advance = xz(c - b).perp_dot(outgoing) / divisor;
                let retreat = xz(c - b).perp_dot(incoming) / divisor;
                if advance < 0. || retreat > 0. || advance > 2. * radius || -retreat > 2. * radius {
                    continue;
                }
                let p = xz(b) + incoming * advance;
                // Heights are only a fitting reference; the adapter still checks
                // actual terrain support at every deformed footprint.
                let point = Vec3::new(
                    p.x,
                    b.y + (c.y - b.y) * advance / (advance - retreat).max(1e-8),
                    p.y,
                );
                let first = nodes[i].first;
                let last = nodes[i + 1].last;
                let source = &self.points[first..=last];
                if source.iter().any(|&q| xz(point - q).length() > 2. * radius) {
                    continue;
                }
                if !source.windows(2).any(|s| {
                    let edge = xz(s[1] - s[0]);
                    let t = (xz(point - s[0]).dot(edge) / edge.length_squared().max(1e-20))
                        .clamp(0., 1.);
                    xz(point - s[0]).distance(edge * t) <= radius * 0.25
                }) {
                    continue;
                }
                let minimum = |points: &[Vec3]| {
                    RoundedPath::fit(points, radius, Vec3::new(incoming.x, 0., incoming.y), true)
                        .minimum_radius
                };
                let before = minimum(&[a, b, c, d]);
                let after = minimum(&[a, point, d]);
                if before >= radius || after <= before + 1e-8 {
                    continue;
                }
                nodes.splice(i..=i + 1, [Node { point, first, last }]);
                changed = true;
                break;
            }
            if !changed {
                break;
            }
        }
        RoundedPath::fit(
            &nodes.iter().map(|n| n.point).collect::<Vec<_>>(),
            radius,
            heading,
            true,
        )
    }
}
