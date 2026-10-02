//! Independent live sampling and shading policies for continuous flower stems.
//! Neither switch rebuilds the immutable flower surface bank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StemExperiment {
    pub pixelized: bool,
    pub surface_cells: bool,
    pub direction_resolution: u32,
    pub model_sampling: bool,
    pub model_resolution: u32,
    pub radius_scale: f32,
    pub branches: bool,
}

impl Default for StemExperiment {
    fn default() -> Self {
        Self {
            pixelized: false,
            surface_cells: true,
            direction_resolution: 512,
            model_sampling: false,
            model_resolution: 128,
            radius_scale: 0.7,
            branches: true,
        }
    }
}

impl StemExperiment {
    pub fn normalized(self) -> Self {
        Self {
            direction_resolution: self.direction_resolution.clamp(128, 2048),
            model_resolution: self.model_resolution.clamp(32, 512),
            radius_scale: if self.radius_scale.is_finite() {
                self.radius_scale.clamp(0.25, 2.)
            } else {
                0.7
            },
            ..self
        }
    }

    /// A plant's height is at most twice its conservative root extent. The
    /// shader's perspective sample fringe is bounded by two model-cell widths.
    pub fn model_padding(self, maximum_extent: f32) -> f32 {
        if self.pixelized && self.model_sampling {
            maximum_extent.max(0.) * 4. / self.normalized().model_resolution as f32
        } else {
            0.
        }
    }

    /// Include fragments whose quantized sample lies inside the stem even if
    /// the display ray lies outside its geometric bound. Shading is independent.
    pub fn angular_padding(self, maximum_distance: f32) -> f32 {
        if self.pixelized && !self.model_sampling {
            maximum_distance.max(0.) * 4. / self.normalized().direction_resolution as f32
        } else {
            0.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_sampling_has_distance_independent_padding_and_is_opt_in() {
        assert!(!StemExperiment::default().model_sampling);
        for pixelized in [false, true] {
            let p = StemExperiment {
                pixelized,
                model_sampling: true,
                model_resolution: 0,
                ..StemExperiment::default()
            }
            .normalized();
            assert_eq!(p.model_resolution, 32);
            assert_eq!(p.angular_padding(1000.), 0.);
            assert_eq!(p.model_padding(2.), if pixelized { 0.25 } else { 0. });
            assert_eq!(p.model_padding(-1.), 0.);
            assert_eq!(
                StemExperiment {
                    model_resolution: u32::MAX,
                    ..p
                }
                .normalized()
                .model_resolution,
                512
            );
        }
    }

    #[test]
    fn shading_and_pixel_sampling_are_independent_and_bounded() {
        for pixelized in [false, true] {
            for surface_cells in [false, true] {
                let p = StemExperiment {
                    pixelized,
                    surface_cells,
                    direction_resolution: 0,
                    radius_scale: f32::INFINITY,
                    ..StemExperiment::default()
                }
                .normalized();
                assert_eq!(p.pixelized, pixelized);
                assert_eq!(p.surface_cells, surface_cells);
                assert_eq!(p.direction_resolution, 128);
                assert_eq!(p.radius_scale, 0.7);
                assert_eq!(
                    p.angular_padding(100.),
                    if pixelized { 100. * 4. / 128. } else { 0. }
                );
                assert_eq!(p.angular_padding(-1.), 0.);
            }
        }
    }
}
