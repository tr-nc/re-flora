//! Independent live sampling and shading policies for continuous flower stems.
//! Neither switch rebuilds the immutable flower surface bank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StemExperiment {
    pub pixelized: bool,
    pub surface_cells: bool,
    pub fixed_cell_height: bool,
    pub cell_height_voxels: f32,
    pub model_resolution: u32,
    pub radius_scale: f32,
    pub branches: bool,
}

impl Default for StemExperiment {
    fn default() -> Self {
        Self {
            pixelized: false,
            surface_cells: true,
            fixed_cell_height: true,
            cell_height_voxels: 0.5,
            model_resolution: 128,
            radius_scale: 4.0,
            branches: true,
        }
    }
}

impl StemExperiment {
    pub fn normalized(self) -> Self {
        Self {
            cell_height_voxels: if self.cell_height_voxels.is_finite() {
                self.cell_height_voxels.clamp(0.1, 4.)
            } else {
                Self::default().cell_height_voxels
            },
            model_resolution: self.model_resolution.clamp(32, 512),
            radius_scale: if self.radius_scale.is_finite() {
                self.radius_scale.clamp(0.25, 10.)
            } else {
                Self::default().radius_scale
            },
            ..self
        }
    }

    /// A plant's height is at most twice its conservative root extent. The
    /// shader's perspective sample fringe is bounded by two model-cell widths.
    pub fn model_padding(self, maximum_extent: f32) -> f32 {
        if self.pixelized {
            maximum_extent.max(0.) * 4. / self.normalized().model_resolution as f32
        } else {
            0.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_height_is_bounded_without_changing_ab_mode() {
        for fixed_cell_height in [false, true] {
            for (height, expected) in [(0., 0.1), (0.5, 0.5), (8., 4.), (f32::NAN, 0.5)] {
                let p = StemExperiment {
                    fixed_cell_height,
                    cell_height_voxels: height,
                    ..Default::default()
                }
                .normalized();
                assert_eq!(p.fixed_cell_height, fixed_cell_height);
                assert_eq!(p.cell_height_voxels, expected);
            }
        }
    }

    #[test]
    fn pixelization_has_distance_independent_padding() {
        for pixelized in [false, true] {
            let p = StemExperiment {
                pixelized,
                model_resolution: 0,
                ..StemExperiment::default()
            }
            .normalized();
            assert_eq!(p.model_resolution, 32);
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
    fn radius_scale_accepts_the_expanded_range_and_defaults_to_four() {
        assert_eq!(StemExperiment::default().radius_scale, 4.0);
        for (radius_scale, expected) in [
            (0.0, 0.25),
            (4.0, 4.0),
            (10.0, 10.0),
            (20.0, 10.0),
            (f32::NAN, 4.0),
        ] {
            assert_eq!(
                StemExperiment {
                    radius_scale,
                    ..Default::default()
                }
                .normalized()
                .radius_scale,
                expected
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
                    model_resolution: 0,
                    radius_scale: f32::INFINITY,
                    ..StemExperiment::default()
                }
                .normalized();
                assert_eq!(p.pixelized, pixelized);
                assert_eq!(p.surface_cells, surface_cells);
                assert_eq!(p.model_resolution, 32);
                assert_eq!(p.radius_scale, 4.0);
                assert_eq!(
                    p.model_padding(100.),
                    if pixelized { 100. * 4. / 32. } else { 0. }
                );
                assert_eq!(p.model_padding(-1.), 0.);
            }
        }
    }
}
