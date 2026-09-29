//! Live stem sampling policy, deliberately separate from immutable head/cache
//! settings. All candidates share one continuous, tapered branch skeleton in
//! flower_stem_geometry.slang; switching never rebuilds the flower surface bank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StemExperiment {
    pub enabled: bool,
    /// 0: continuous reference, 1: world-direction cells, 2: surface cells.
    pub sampling: u32,
    pub direction_resolution: u32,
    pub surface_cell_scale: f32,
    pub radius_scale: f32,
    pub branches: bool,
    pub freeze_motion: bool,
}

impl Default for StemExperiment {
    fn default() -> Self {
        Self {
            enabled: false,
            sampling: 1,
            direction_resolution: 512,
            surface_cell_scale: 1.,
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
            surface_cell_scale: finite(self.surface_cell_scale, 1., 0.25, 4.),
            radius_scale: finite(self.radius_scale, 0.7, 0.25, 2.),
            ..self
        }
    }

    /// Same conservative angular footprint used by the proxy vertex shader.
    /// Frustum culling must include pixels whose *sample* lies inside a stem
    /// even when the displayed fragment lies just outside its geometric bound.
    pub fn angular_padding(self, maximum_distance: f32) -> f32 {
        let s = self.normalized();
        if s.enabled && s.sampling == 1 {
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
    fn live_policy_is_bounded_and_off_means_original() {
        let original = StemExperiment::default();
        assert!(!original.enabled);
        assert_eq!(original.angular_padding(100.), 0.);
        let bad = StemExperiment {
            enabled: true,
            sampling: u32::MAX,
            direction_resolution: 0,
            surface_cell_scale: f32::NAN,
            radius_scale: f32::INFINITY,
            ..original
        }
        .normalized();
        assert_eq!(bad.sampling, 2);
        assert_eq!(bad.direction_resolution, 128);
        assert_eq!(bad.surface_cell_scale, 1.);
        assert_eq!(bad.radius_scale, 0.7);
        assert_eq!(bad.angular_padding(100.), 0.);
        let direction = StemExperiment {
            enabled: true,
            ..original
        };
        assert_eq!(direction.angular_padding(100.), 100. * 4. / 512.);
        assert_eq!(direction.angular_padding(-1.), 0.);
    }
}
