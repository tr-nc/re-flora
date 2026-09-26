//! Deterministic math oracle for the shared Slang orthographic/display seam.
//! These are correctness tests, not performance measurements.
use glam::{Mat3, Mat4, Quat, Vec2, Vec3, Vec4};

fn basis(direction: Vec3) -> Mat3 {
    let up = if direction.y.abs() < 0.99 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let x = up.cross(direction).normalize();
    Mat3::from_cols(x, direction.cross(x), direction)
}
fn rotate(p: Vec2, roll: Vec2) -> Vec2 {
    Vec2::new(roll.x * p.x - roll.y * p.y, roll.y * p.x + roll.x * p.y)
}

#[test]
fn fixed_bake_is_independent_of_instance_translation_scale_and_rigid_pose() {
    for direction in [
        Vec3::Y,
        -Vec3::Y,
        Vec3::Z,
        Vec3::new(1., 2., 3.).normalize(),
    ] {
        let canonical = basis(direction);
        assert!((canonical.determinant() - 1.).abs() < 1e-6);
        let point = Vec3::new(0.31, -0.24, 0.52);
        let expected = canonical.transpose() * point;
        for angle in [0., 0.7, 2.4, 5.9] {
            let rotation = Mat3::from_quat(Quat::from_euler(
                glam::EulerRot::XYZ,
                angle,
                angle * 0.3,
                -angle * 0.2,
            ));
            for scale in [0.01, 0.1, 1., 4.] {
                let pivot = Vec3::new(1., -0.7, 2.);
                let world = pivot + rotation * point * scale;
                let actual = (rotation * canonical).transpose() * (world - pivot) / scale;
                assert!((actual - expected).length() < 3e-5);
            }
        }
    }
}
#[test]
fn fixed_view_allows_a_full_continuous_roll_without_changing_the_view_key() {
    let direction = Vec3::new(1., 2., 3.).normalize();
    let canonical = basis(direction);
    let to_camera = Quat::from_rotation_arc(direction, Vec3::Z);
    let initial = (to_camera * canonical.x_axis).truncate().normalize();
    for step in 0..=36 {
        let angle = step as f32 * std::f32::consts::TAU / 36.;
        let pose = Quat::from_axis_angle(direction, angle);
        assert!((pose.inverse() * direction - direction).length() < 1e-6);
        let actual = (to_camera * (pose * canonical.x_axis))
            .truncate()
            .normalize();
        let expected = rotate(initial, Vec2::new(angle.cos(), angle.sin()));
        assert!((actual - expected).length() < 2e-6);
    }
}
#[test]
fn bake_projection_has_parallel_rays_and_depth_maps_back_to_the_scene() {
    let bake = Mat4::from_cols(
        Vec4::X,
        Vec4::Y,
        Vec4::new(0., 0., -0.5, 0.),
        Vec4::new(0., 0., 0.5, 1.),
    );
    for p in [
        Vec3::ZERO,
        Vec3::new(-0.7, 0.8, 0.9),
        Vec3::new(0.4, -0.2, -0.9),
    ] {
        let clip = bake * p.extend(1.);
        assert!((bake.inverse() * clip - p.extend(1.)).length() < 1e-6);
        let near = (bake.inverse() * Vec4::new(p.x, p.y, 0., 1.)).truncate();
        let far = (bake.inverse() * Vec4::new(p.x, p.y, 1., 1.)).truncate();
        assert_eq!((far - near).normalize(), -Vec3::Z);
        for center_z in [-0.1, -1., -10.] {
            let radius = 0.03;
            let recovered_z = center_z + (1. - 2. * clip.z) * radius;
            assert!((recovered_z - (center_z + p.z * radius)).abs() < 1e-6);
            let camera = Mat4::perspective_rh(1., 1.6, 0.01, 100.);
            let scene = camera * Vec4::new(0., 0., recovered_z, 1.);
            assert!((0.0..1.0).contains(&(scene.z / scene.w)));
        }
    }
}
#[test]
fn authored_shapes_fit_the_fixed_frame_at_every_roll() {
    for p in &super::apple_preview::mesh().positions {
        assert!(
            (p * 0.5).length() < 1.55,
            "apple outside orthographic frame"
        );
    }
    for model in [
        crate::model_assets::leaf_variants(),
        crate::model_assets::butterfly(),
    ] {
        for step in 0..16 {
            let pose = model.transforms(step as f32 / 16., 0);
            for tri in &model.triangles {
                for p in tri.positions {
                    assert!(
                        pose[tri.node].transform_point3(p).length() < 1.7,
                        "particle outside fixed sphere"
                    );
                }
            }
        }
    }
}

#[test]
fn all_three_adapters_use_the_shared_bake_and_rotating_display_is_geometry_free() {
    let particle = include_str!("../../shader/slang/particle_model_shading.slang");
    let apple = include_str!("../../shader/slang/apple_pixel_tile.slang");
    for adapter in [particle, apple] {
        assert!(adapter.contains("modelSurface("));
    }
    let display = include_str!("../../shader/slang/model_pixel_display.slang");
    assert!(!display.contains("sampleModelPixelGeometry("));
    assert!(!display.contains("sampleOrthographicModelPixel("));
    assert!(display.contains("modelOrthographicDepth("));
    assert!(!display.contains("modelPixelCellOverlap("));
    assert!(!display.contains("modelResamplePixel("));
    let cache = include_str!("../../shader/slang/model_pixel_cache.slang");
    let generator = include_str!("../../shader/slang/model_pixel_bake.slang");
    assert!(cache.contains("return bakeModelSurface("));
    assert!(generator.contains("sampleOrthographicModelPixel("));
    let bake = include_str!("../../shader/slang/model_pixel_projection.slang");
    let sampler = bake
        .split("public ModelPixelHit sampleOrthographicModelPixel")
        .nth(1)
        .unwrap()
        .split("public float4 modelOrthographicQuad")
        .next()
        .unwrap();
    assert!(!bake.contains("screenAligned"));
    assert!(bake.contains("p=modelRotatePixel(p,view.roll);"));
    assert!(!sampler.contains("camera_info"));
}
