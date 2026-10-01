//! Live stem sampling policy, deliberately separate from immutable head/cache
//! settings. All candidates share one continuous, tapered branch skeleton in
//! flower_stem_geometry.slang; switching never rebuilds the flower surface bank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StemExperiment {
    /// 0: original cube stems, 1: world-direction, 2: surface-attached.
    pub sampling: u32,
    pub direction_resolution: u32,
    /// B affects direction mode only; A keeps camera-origin angular cells.
    pub object_sampling: bool,
    pub object_resolution: u32,
    pub radius_scale: f32,
    pub branches: bool,
    pub freeze_motion: bool,
}

impl Default for StemExperiment {
    fn default() -> Self {
        Self {
            sampling: 0,
            direction_resolution: 512,
            object_sampling: false,
            object_resolution: 256,
            radius_scale: 0.7,
            branches: true,
            freeze_motion: false,
        }
    }
}

impl StemExperiment {
    pub fn normalized(self) -> Self {
        let finite = |v: f32, default: f32, lo: f32, hi: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                default
            }
        };
        Self {
            sampling: self.sampling.min(2),
            direction_resolution: self.direction_resolution.clamp(128, 2048),
            object_resolution: self.object_resolution.clamp(32, 512),
            radius_scale: finite(self.radius_scale, 0.7, 0.25, 2.),
            ..self
        }
    }

    pub fn enabled(self) -> bool {
        self.sampling != 0
    }

    pub fn object_grid_active(self) -> bool {
        self.sampling == 1 && self.object_sampling
    }

    /// Source-cell fringe for conservative frustum culling; unlike A this never
    /// scales with camera distance. This is not geometry coverage or thickening.
    /// maximum_extent is the existing conservative plant/root bound.
    pub fn object_padding(self, maximum_extent: f32) -> f32 {
        if self.object_grid_active() {
            maximum_extent.max(0.) * 2. * std::f32::consts::SQRT_2
                / self.normalized().object_resolution as f32
        } else {
            0.
        }
    }

    /// Same conservative angular footprint used by the proxy vertex shader.
    /// Frustum culling must include pixels whose *sample* lies inside a stem
    /// even when the displayed fragment lies just outside its geometric bound.
    pub fn angular_padding(self, maximum_distance: f32) -> f32 {
        let s = self.normalized();
        if s.sampling == 1 && !s.object_sampling {
            maximum_distance.max(0.) * 4. / s.direction_resolution as f32
        } else {
            0.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_ab_only_changes_direction_contract_and_uses_spatial_padding() {
        let b = StemExperiment {
            sampling: 1,
            object_sampling: true,
            object_resolution: 0,
            ..StemExperiment::default()
        }
        .normalized();
        assert_eq!(b.object_resolution, 32);
        assert!(b.object_grid_active());
        assert_eq!(b.angular_padding(1.), b.angular_padding(1000.));
        assert_eq!(b.angular_padding(1000.), 0.);
        assert!(b.object_padding(1.) > 0.);
        assert_eq!(
            StemExperiment {
                object_resolution: u32::MAX,
                ..b
            }
            .normalized()
            .object_resolution,
            512
        );
        for method in [0, 2] {
            let p = StemExperiment {
                sampling: method,
                ..b
            };
            assert!(!p.object_grid_active());
            assert_eq!(p.object_padding(100.), 0.);
        }
        assert!(!StemExperiment::default().object_sampling);
    }

    #[test]
    fn live_policy_is_bounded_and_off_means_original() {
        let original = StemExperiment::default();
        assert!(!original.enabled());
        assert_eq!(original.angular_padding(100.), 0.);
        let bad = StemExperiment {
            sampling: u32::MAX,
            direction_resolution: 0,
            radius_scale: f32::INFINITY,
            ..original
        }
        .normalized();
        assert_eq!(bad.sampling, 2);
        assert_eq!(bad.direction_resolution, 128);
        assert_eq!(bad.radius_scale, 0.7);
        assert_eq!(bad.angular_padding(100.), 0.);
        let direction = StemExperiment {
            sampling: 1,
            ..original
        };
        assert_eq!(direction.angular_padding(100.), 100. * 4. / 512.);
        assert_eq!(direction.angular_padding(-1.), 0.);
    }
}
