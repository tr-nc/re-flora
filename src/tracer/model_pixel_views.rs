//! Fibonacci-sphere directions, parameterized by the live view count.
//! Cache only golden-angle azimuths: latitude depends on N, so a prefix of a
//! fixed sphere is NOT a uniformly distributed smaller sphere.
pub const MIN_VIEWS: u32 = 8;
pub const MAX_VIEWS: u32 = 512;

pub fn effective_count(requested: u32, continuous_oracle: bool) -> u32 {
    // Zero is internal to numerical validation, never a GUI/off option.
    if continuous_oracle {
        0
    } else {
        requested.clamp(MIN_VIEWS, MAX_VIEWS)
    }
}
pub fn azimuths() -> [[f32; 4]; MAX_VIEWS as usize] {
    std::array::from_fn(|i| {
        let angle = i as f32 * (std::f32::consts::PI * (3.0 - 5.0_f32.sqrt()));
        [angle.cos(), angle.sin(), 0., 0.]
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    #[test]
    fn only_the_internal_oracle_can_disable_quantization() {
        assert_eq!(effective_count(0, false), MIN_VIEWS);
        assert_eq!(effective_count(u32::MAX, false), MAX_VIEWS);
        assert_eq!(effective_count(37, false), 37);
        assert_eq!(effective_count(128, true), 0);
    }
    #[test]
    fn every_slider_count_has_finite_unit_directions_and_balanced_latitudes() {
        let azimuths = azimuths();
        for count in MIN_VIEWS..=MAX_VIEWS {
            let mut mean_y = 0.;
            for i in 0..count {
                let y = 1. - 2. * (i as f32 + 0.5) / count as f32;
                let r = (1. - y * y).sqrt();
                let a = azimuths[i as usize];
                let p = Vec3::new(a[0] * r, y, a[1] * r);
                assert!(p.is_finite());
                assert!((p.length() - 1.).abs() < 1e-6);
                mean_y += y;
            }
            assert!((mean_y / count as f32).abs() < 1e-6);
        }
    }
    #[test]
    fn view_counts_cover_the_whole_sphere_and_preserve_rigid_handedness() {
        let azimuths = azimuths();
        for count in [8, 9, 37, 128, 511, 512] {
            let views: Vec<_> = (0..count)
                .map(|i| {
                    let y = 1. - 2. * (i as f32 + 0.5) / count as f32;
                    let r = (1. - y * y).sqrt();
                    let a = azimuths[i];
                    Vec3::new(a[0] * r, y, a[1] * r)
                })
                .collect();
            for (i, p) in views.iter().enumerate() {
                assert_eq!(
                    views
                        .iter()
                        .enumerate()
                        .max_by(|(_, a), (_, b)| a.dot(*p).total_cmp(&b.dot(*p)))
                        .unwrap()
                        .0,
                    i
                );
            }
            for y in -20..=20 {
                for longitude in 0..80 {
                    let h = y as f32 / 20.;
                    let a = longitude as f32 * std::f32::consts::TAU / 80.;
                    let r = (1. - h * h).sqrt();
                    let view = Vec3::new(r * a.cos(), h, r * a.sin());
                    let chosen = views
                        .iter()
                        .max_by(|a, b| a.dot(view).total_cmp(&b.dot(view)))
                        .unwrap();
                    assert!(
                        chosen.dot(view) > 1. - 5. / count as f32,
                        "angular hole at count={count}"
                    );
                    let rotation = glam::Quat::from_rotation_arc(*chosen, view);
                    assert!((rotation.inverse() * view - *chosen).length() < 1e-5);
                    assert!(
                        (rotation * Vec3::X)
                            .cross(rotation * Vec3::Y)
                            .dot(rotation * Vec3::Z)
                            > 0.9999
                    );
                }
            }
        }
    }
}
