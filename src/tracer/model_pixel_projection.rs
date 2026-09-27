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
fn overlap(source: Vec2, destination: Vec2, inverse_roll: Vec2) -> bool {
    let d = source - destination;
    let reach = 0.5 * (1. + inverse_roll.x.abs() + inverse_roll.y.abs()) - 1e-6;
    d.abs().cmple(Vec2::splat(reach)).all()
        && rotate(d, Vec2::new(inverse_roll.x, -inverse_roll.y))
            .abs()
            .cmple(Vec2::splat(reach))
            .all()
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
fn resampling_does_not_dilate_an_unrotated_grid() {
    let center = Vec2::splat(4.5);
    for y in -2..=2 {
        for x in -2..=2 {
            assert_eq!(
                overlap(center + Vec2::new(x as f32, y as f32), center, Vec2::X),
                x == 0 && y == 0
            );
        }
    }
}
#[test]
fn three_by_three_gather_covers_all_positive_area_cell_overlaps() {
    for step in 0..=24 {
        let a = step as f32 * std::f32::consts::TAU / 24.;
        let inverse = Vec2::new(a.cos(), -a.sin());
        for center in [
            Vec2::new(3.01, 5.99),
            Vec2::new(3.5, 5.5),
            Vec2::new(3.99, 5.01),
        ] {
            for y in -3i32..=3 {
                for x in -3i32..=3 {
                    let source = center.floor() + Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    if overlap(source, center, inverse) {
                        assert!(x.abs() <= 1 && y.abs() <= 1);
                    }
                }
            }
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
fn all_model_pixel_entrypoints_have_geometry_free_transitive_interfaces() {
    use std::collections::{HashMap, HashSet};
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shader/slang");
    let modules: HashMap<String, String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_stem().unwrap().to_str().unwrap().to_owned(),
                std::fs::read_to_string(path).unwrap(),
            )
        })
        .collect();
    fn visit(name: &str, modules: &HashMap<String, String>, seen: &mut HashSet<String>) {
        if !seen.insert(name.to_owned()) {
            return;
        }
        for statement in modules[name].split(';') {
            // Imports may follow a module declaration, attributes or comments.
            if let Some(import) = statement
                .rsplit_once("import ")
                .map(|(_, name)| name.trim())
            {
                if modules.contains_key(import) {
                    visit(import, modules, seen);
                }
            }
        }
    }
    let mut checked = HashSet::new();
    for name in modules
        .keys()
        .filter(|n| n.ends_with(".comp") || n.ends_with(".vert") || n.ends_with(".frag"))
    {
        let mut closure = HashSet::new();
        visit(name, &modules, &mut closure);
        if name == "model_pixel_bake.comp" {
            continue;
        }
        // Discover future consumers too: not an allowlist of today's four adapters.
        if !closure.iter().any(|m| {
            modules[m].contains("model_pixel_tiles")
                || m == "model_pixel_cache"
                || m == "model_pixel_object"
        }) {
            continue;
        }
        checked.insert(name.as_str());
        for module in closure {
            for forbidden in [
                "sampleModelPixelGeometry",
                "sampleOrthographicModelPixel",
                "bakeModelSurface",
                "ButterflyMeshTriangle",
                "model_bake_triangles",
                "model_bake_ranges",
                "continuous_oracle",
                "modelDiscreteViews",
                "modelSingleLight",
            ] {
                assert!(
                    !modules[&module].contains(forbidden),
                    "{name} imports geometry/legacy route via {module}: {forbidden}"
                );
            }
        }
    }
    for required in [
        "butterfly_tile.comp",
        "apple_pixel_tree.comp",
        "apple_pixel_dynamic.comp",
        "flower_pixel.comp",
        "butterfly_tile.frag",
        "apple_pixel.frag",
        "flower_pixel.frag",
    ] {
        assert!(checked.contains(required), "unexamined consumer {required}");
    }
    let display = include_str!("../../shader/slang/model_pixel_display.slang");
    assert!(display.contains("modelOrthographicDepth("));
    assert!(display.contains("modelPixelCellOverlap("));
    let bake = include_str!("../../shader/slang/model_pixel_bake_projection.slang");
    assert!(!bake.contains("screenAligned"));
    assert!(!bake.contains("camera_info"));
}

#[test]
fn rust_frame_adapter_publishes_no_geometry_or_reference_stream() {
    for text in [
        include_str!("model_pixel_frame.rs"),
        include_str!("model_pixel_tiles.rs"),
        include_str!("butterfly_mesh.rs"),
    ] {
        // The authored-mesh tests are deliberately isolated from frame preparation.
        let production = text.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        for forbidden in [
            "discrete_views",
            "canonical_frames",
            "reference_tile",
            "repair_nodes",
            "butterfly_mesh_triangles",
            "triangles: upload",
            "Vec<Triangle>",
        ] {
            assert!(
                !production.contains(forbidden),
                "frame route retained: {forbidden}"
            );
        }
    }
}
