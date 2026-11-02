//! One smooth approach supplies mowing, flower clearance, lamp rows and walk audits.
use super::{ARRIVAL, POCKET, ROUTE};
use glam::Vec2;

const STEPS_PER_SPAN: usize = 12;
const KNOTS: [Vec2; 5] = [Vec2::new(0.5, 0.91), ARRIVAL, ROUTE[1], ROUTE[2], POCKET];
const SPANS: usize = KNOTS.len() - 1;
const LANTERN_PAIRS: usize = 5;

fn point(t: f32) -> Vec2 {
    let span = t.clamp(0., 1.) * SPANS as f32;
    let i = (span as usize).min(SPANS - 1);
    let u = span - i as f32;
    let a = KNOTS[i.saturating_sub(1)];
    let b = KNOTS[i];
    let c = KNOTS[i + 1];
    let d = KNOTS[(i + 2).min(KNOTS.len() - 1)];
    0.5 * ((2. * b)
        + (-a + c) * u
        + (2. * a - 5. * b + 4. * c - d) * u * u
        + (-a + 3. * b - 3. * c + d) * u * u * u)
}

pub fn route_samples() -> Vec<Vec2> {
    (0..=SPANS * STEPS_PER_SPAN)
        .map(|i| point(i as f32 / (SPANS * STEPS_PER_SPAN) as f32))
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct ApproachSample {
    pub center: Vec2,
    pub half_width: f32,
}

pub fn approach_path(size: Vec2) -> Vec<ApproachSample> {
    let mut distance = 0.;
    let mut previous = None;
    route_samples()
        .into_iter()
        .map(|p| {
            let center = p * size;
            if let Some(last) = previous {
                distance += center.distance(last);
            }
            previous = Some(center);
            ApproachSample {
                center,
                // Gently varying worn edges, rather than a constant-width road.
                half_width: 0.070 + 0.006 * (distance * 9.).sin() + 0.003 * (distance * 21.).sin(),
            }
        })
        .collect()
}

/// Paired rows at equal arc-length stations, offset from the local path tangent.
/// Placement samples each foot's real soil height independently on the hillside.
pub fn lantern_pairs(size: Vec2) -> Vec<[Vec2; 2]> {
    let path = approach_path(size);
    let mut distances = vec![0.];
    for span in path.windows(2) {
        distances.push(distances.last().unwrap() + span[0].center.distance(span[1].center));
    }
    let length = *distances.last().unwrap();
    (1..=LANTERN_PAIRS)
        .map(|i| {
            let station = length * i as f32 / (LANTERN_PAIRS + 1) as f32;
            let next = distances.partition_point(|s| *s < station);
            let a = path[next - 1];
            let b = path[next];
            let u = (station - distances[next - 1]) / (distances[next] - distances[next - 1]);
            let center = a.center.lerp(b.center, u);
            let forward = (b.center - a.center).normalize();
            let normal = Vec2::new(-forward.y, forward.x);
            // Keep the complete lantern roof clear of the walked / mown strip.
            let offset = a.half_width + (b.half_width - a.half_width) * u + 0.065;
            [center + normal * offset, center - normal * offset]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approach_interpolates_the_gate_arrival_and_temple_without_corners() {
        for (i, knot) in KNOTS.iter().enumerate() {
            let t = i as f32 / SPANS as f32;
            assert!(point(t).distance(*knot) < 1e-6);
            if i > 0 && i < SPANS {
                let before = (point(t) - point(t - 0.001)).normalize();
                let after = (point(t + 0.001) - point(t)).normalize();
                assert!(
                    before.dot(after) > 0.999,
                    "the trail must not kink at a knot"
                );
            }
        }
        let path = approach_path(Vec2::splat(4.));
        assert!(path.iter().all(|p| (0.06..0.08).contains(&p.half_width)));
        assert!(
            path.iter()
                .map(|p| p.half_width)
                .fold(f32::NEG_INFINITY, f32::max)
                - path
                    .iter()
                    .map(|p| p.half_width)
                    .fold(f32::INFINITY, f32::min)
                > 0.008
        );
    }

    #[test]
    fn lantern_rows_are_paired_evenly_spaced_and_clear_of_the_walked_strip() {
        for size in [Vec2::splat(2.), Vec2::splat(4.), Vec2::new(4., 3.)] {
            let path = approach_path(size);
            let pairs = lantern_pairs(size);
            assert_eq!(pairs.len(), 5);
            let mut stations = Vec::new();
            for [left, right] in pairs {
                let center = (left + right) * 0.5;
                let mut distance = 0.;
                let mut matched = None;
                for span in path.windows(2) {
                    let d = span[1].center - span[0].center;
                    let u = ((center - span[0].center).dot(d) / d.length_squared()).clamp(0., 1.);
                    let projected = span[0].center + d * u;
                    if center.distance(projected) < 1e-5 {
                        assert!((left - center).dot(d.normalize()).abs() < 1e-5);
                        assert!((right - center).dot(d.normalize()).abs() < 1e-5);
                        assert!(left.distance(center) - 0.030 > span[0].half_width + 0.025);
                        matched = Some(distance + d.length() * u);
                        break;
                    }
                    distance += d.length();
                }
                stations.push(matched.expect("every pair must be centered on the actual path"));
                assert!(left.cmpgt(Vec2::ZERO).all() && left.cmplt(size).all());
                assert!(right.cmpgt(Vec2::ZERO).all() && right.cmplt(size).all());
            }
            let pitch = stations[1] - stations[0];
            for span in stations.windows(2) {
                assert!((span[1] - span[0] - pitch).abs() < 1e-5);
            }
        }
    }
}
