//! Stable model-local view keys shared by the live preview and a future atlas.
pub const VIEW_COUNT: usize = 128;
pub fn directions() -> [[f32; 4]; VIEW_COUNT] {
    std::array::from_fn(|i| {
        let y = 1.0 - 2.0 * (i as f32 + 0.5) / VIEW_COUNT as f32;
        let angle = i as f32 * (std::f32::consts::PI * (3.0 - 5.0_f32.sqrt()));
        let radius = (1.0 - y * y).sqrt();
        [angle.cos() * radius, y, angle.sin() * radius, 0.0]
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    #[test]
    fn stable_views_are_distinct_unit_vectors_covering_the_sphere() {
        let views = directions().map(|p| Vec3::from_slice(&p));
        for (i, p) in views.iter().enumerate() {
            assert!((p.length() - 1.).abs() < 1e-6);
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
                assert!(chosen.dot(view) > 0.96, "excessive angular hole");
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
