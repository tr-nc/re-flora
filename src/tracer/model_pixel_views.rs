//! Fibonacci-sphere directions shared by all pixel models.
//! Cache only golden-angle azimuths: latitude depends on N, so a prefix of a
//! fixed sphere is NOT a uniformly distributed smaller sphere.
pub const MIN_VIEWS: u32 = 8;
pub const VIEW_COUNT: u32 = 512;
pub const MAX_VIEWS: u32 = VIEW_COUNT;

pub fn azimuth(index: u32) -> [f32; 4] {
    let angle = index as f32 * (std::f32::consts::PI * (3.0 - 5.0_f32.sqrt()));
    [angle.cos(), angle.sin(), 0., 0.]
}
/// Geometry-oracle helper; production banks all use `VIEW_COUNT`.
pub fn azimuths(count: u32) -> Vec<[f32; 4]> {
    assert!((MIN_VIEWS..=MAX_VIEWS).contains(&count));
    (0..count).map(azimuth).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    #[test]
    fn sampled_counts_have_finite_unit_directions_and_balanced_latitudes() {
        for count in MIN_VIEWS..=MAX_VIEWS {
            let azimuths = azimuths(count);
            assert_eq!(azimuths.len(), count as usize);
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
        for count in [8, 9, 37, 128, 511, 512] {
            let azimuths = azimuths(count as u32);
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
