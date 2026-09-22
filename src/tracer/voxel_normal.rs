//! Reliability of a radius-two occupancy normal, independent of tree topology/material.
//! Keep the unnormalised evidence: a unit fallback normal cannot encode uncertainty.
use glam::{IVec3, Mat3, Vec3};

pub(super) struct OccupancyNormal {
    moment: IVec3,
    second_moment: Mat3,
    distance_sum: f32,
    count: f32,
}

impl Default for OccupancyNormal {
    fn default() -> Self {
        Self {
            moment: IVec3::ZERO,
            second_moment: Mat3::ZERO,
            distance_sum: 0.,
            count: 0.,
        }
    }
}

impl OccupancyNormal {
    pub fn add(&mut self, offset: IVec3) {
        let p = offset.as_vec3();
        self.moment += offset;
        self.second_moment += Mat3::from_cols(p * p.x, p * p.y, p * p.z);
        self.distance_sum += p.length();
        self.count += 1.;
    }

    /// Original normal plus continuous confidence. A half-space has strong directional
    /// coherence and support in both tangent directions; a line/tip lacks that support.
    /// These thresholds are in voxel units for the existing radius-two estimator.
    pub fn finish(&self) -> (Vec3, f32) {
        if self.moment == IVec3::ZERO {
            return (Vec3::Y, 0.);
        }
        let moment = self.moment.as_vec3();
        let normal = -moment.normalize();
        let coherence = moment.length() / self.distance_sum.max(1e-8);
        let mean = moment / self.count;
        let covariance = self.second_moment / self.count
            - Mat3::from_cols(mean * mean.x, mean * mean.y, mean * mean.z);
        let u = normal.any_orthonormal_vector();
        let v = normal.cross(u);
        let a = u.dot(covariance * u);
        let b = u.dot(covariance * v);
        let d = v.dot(covariance * v);
        // Smaller eigenvalue of the 2x2 tangential covariance. Unlike sample count
        // or total variance this rejects support concentrated along a single line.
        let support = (0.5 * (a + d - ((a - d).powi(2) + 4. * b * b).sqrt())).max(0.);
        let confidence = smoothstep(0.05, 0.35, coherence) * smoothstep(0.2, 1.2, support);
        (normal, confidence)
    }
}

fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estimate(solid: impl Fn(IVec3) -> bool) -> (Vec3, f32) {
        let mut sample = OccupancyNormal::default();
        for z in -2..=2 {
            for y in -2..=2 {
                for x in -2..=2 {
                    let p = IVec3::new(x, y, z);
                    if solid(p) {
                        sample.add(p);
                    }
                }
            }
        }
        sample.finish()
    }

    #[test]
    fn isolated_lines_tips_and_thin_sheets_have_no_unified_surface_normal() {
        for solid in [
            (|p: IVec3| p == IVec3::ZERO) as fn(IVec3) -> bool,
            |p| p.x == 0 && p.z == 0,
            |p| p.x == 0 && p.z == 0 && p.y <= 0,
            |p| p.x == p.y && p.y == p.z,
            |p| p.x == p.y && p.y == p.z && p.y <= 0,
            |p| p.z == 0,
            |p| p.z == 0 && p.x <= 0,
        ] {
            let (normal, confidence) = estimate(solid);
            assert!(normal.is_finite() && normal.is_normalized());
            assert!(confidence < 1e-5, "{normal:?}: {confidence}");
        }
    }

    #[test]
    fn broad_surfaces_keep_original_normal_and_full_confidence() {
        for axis in [IVec3::X, IVec3::Y, IVec3::Z] {
            for sign in [-1, 1] {
                let direction = axis * sign;
                let (normal, confidence) = estimate(|p| p.dot(direction) <= 0);
                assert!(normal.distance(direction.as_vec3()) < 1e-6);
                assert_eq!(confidence, 1.);
            }
        }
        let (_, oblique) = estimate(|p| p.x + p.y + p.z <= 0);
        assert!(oblique > 0.95, "oblique half-space: {oblique}");
    }

    #[test]
    fn support_produces_intermediate_values_and_is_axis_symmetric() {
        let mut previous = 0.;
        for width in 0..=2 {
            // Surface of a rod, widened in its second tangential direction.
            let (_, confidence) = estimate(|p| p.x <= 0 && p.z.abs() <= width);
            let (_, rotated) = estimate(|p| p.z <= 0 && p.y.abs() <= width);
            assert!((confidence - rotated).abs() < 1e-6);
            assert!(confidence >= previous && (0. ..=1.).contains(&confidence));
            if width == 1 {
                assert!(confidence > 0. && confidence < 1., "{confidence}");
            }
            previous = confidence;
        }
        assert_eq!(previous, 1.);
    }

    #[test]
    fn removing_support_recomputes_confidence_without_temporal_state() {
        let full = estimate(|p| p.y <= 0);
        let edited = estimate(|p| p.x == 0 && p.z == 0 && p.y <= 0);
        assert_eq!(full.0, edited.0);
        assert_eq!(full.1, 1.);
        assert_eq!(edited.1, 0.);
        assert_eq!(full, estimate(|p| p.y <= 0));
    }
}
