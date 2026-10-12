//! Analytic containment and forward-motion proof for merged courses. Checking
//! endpoints alone would let a long quad hide an original loop or sharp reversal.
use super::*;
use crate::app::path_band::{contains_polygon, PathSample};

fn rotate(vector: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(
        vector.x * cos - vector.y * sin,
        vector.x * sin + vector.y * cos,
    )
}
fn minimum_projection(vector: Vec2, axis: Vec2, a: f32, b: f32) -> f32 {
    let low = a.min(b);
    let high = a.max(b);
    let phase = (-axis.y).atan2(-axis.x) - vector.y.atan2(vector.x);
    let extremum = phase + ((low - phase) / std::f32::consts::TAU).ceil() * std::f32::consts::TAU;
    axis.dot(rotate(vector, low))
        .min(axis.dot(rotate(vector, high)))
        .min(if extremum <= high {
            -vector.length() * axis.length()
        } else {
            f32::INFINITY
        })
}
impl RoundedPath {
    pub(in crate::app::path_band) fn follows_course(
        &self,
        start: f64,
        end: f64,
        row: PathSample,
        width: f32,
    ) -> bool {
        let polygon = [
            row.entry.point(-width * 0.5),
            row.entry.point(width * 0.5),
            row.exit.point(width * 0.5),
            row.exit.point(-width * 0.5),
        ];
        let axis = xz(row.exit.center - row.entry.center).normalize_or_zero();
        let line = |a: Vec3, b: Vec3| {
            contains_polygon(&polygon, xz(a), 1e-7)
                && contains_polygon(&polygon, xz(b), 1e-7)
                && axis.dot(xz(b - a)) >= -1e-7
        };
        if self.spans.is_empty() {
            return line(self.section(start).center, self.section(end).center);
        }
        if start < 0. && !line(self.section(start).center, self.section(0.).center) {
            return false;
        }
        if end > self.length && !line(self.section(self.length).center, self.section(end).center) {
            return false;
        }
        for span in &self.spans {
            let low = start.max(span.start);
            let high = end.min(span.start + span.length);
            if high < low {
                continue;
            }
            let a = ((low - span.start) / span.length) as f32;
            let b = ((high - span.start) / span.length) as f32;
            match span.kind {
                SpanKind::Line { from, to } => {
                    if !line(from.lerp(to, a), from.lerp(to, b)) {
                        return false;
                    }
                }
                SpanKind::Arc {
                    center,
                    radial,
                    angle,
                    ..
                } => {
                    let tangent = Vec2::new(-radial.y, radial.x) * angle.signum();
                    if minimum_projection(tangent, axis, angle * a, angle * b) < -1e-7 {
                        return false;
                    }
                    for i in 0..4 {
                        let edge = polygon[(i + 1) % 4] - polygon[i];
                        let normal = Vec2::new(-edge.y, edge.x);
                        if normal.dot(center - polygon[i])
                            + minimum_projection(radial, normal, angle * a, angle * b)
                            < -1e-8
                        {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }
}
