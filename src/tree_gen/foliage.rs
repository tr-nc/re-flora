//! Local-space masks for voxel leaf sprays, independent of tree identity and pose.
use glam::Vec3;

#[derive(Clone, Copy)]
pub(super) enum SprayShape {
    Ellipsoid,
    Palmate,
    Fan,
}

impl SprayShape {
    /// All masks include the origin, the bounded sampler's tiny-sapling fallback.
    pub(super) fn contains(self, sample: Vec3) -> bool {
        if sample.length_squared() > 1.0 {
            return false;
        }
        match self {
            Self::Ellipsoid => true,
            Self::Palmate => {
                let angle = sample.y.atan2(sample.x);
                sample.x.hypot(sample.y) <= 0.72 + 0.28 * (5. * angle).cos()
            }
            Self::Fan => {
                // Petiole end at -X, broad rounded distal edge at +X. A shallow
                // V notch separates two lobes without turning the fan into a heart.
                let from_base = sample.x + 0.80;
                from_base >= 0.
                    && sample.y.abs() <= from_base * 0.95
                    && sample.x <= 0.70 + 0.65 * sample.y.abs()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_has_a_narrow_base_two_broad_lobes_and_a_shallow_notch() {
        let fan = SprayShape::Fan;
        for point in [
            Vec3::ZERO,
            Vec3::new(-0.75, 0., 0.),
            Vec3::new(0.75, 0.3, 0.),
            Vec3::new(0.75, -0.3, 0.),
        ] {
            assert!(fan.contains(point));
        }
        for point in [
            Vec3::new(-0.75, 0.25, 0.),
            Vec3::new(0.9, 0., 0.),
            Vec3::new(0.8, 0.8, 0.),
        ] {
            assert!(!fan.contains(point));
        }
        for shape in [SprayShape::Ellipsoid, SprayShape::Palmate, fan] {
            assert!(shape.contains(Vec3::ZERO));
            assert!(!shape.contains(Vec3::splat(1.)));
        }
    }
}
