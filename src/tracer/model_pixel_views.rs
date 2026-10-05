//! Runtime Fibonacci view bank for native triangles. The shader consumes these
//! immutable azimuths and computes actual-N latitude, not a fixed-sphere prefix.
use glam::Vec3;

pub const MIN_VIEWS: u32 = 8;
pub const MAX_VIEWS: u32 = 512;
const GOLDEN_ANGLE: f32 = std::f32::consts::PI * (3.0 - 2.236_068);

pub fn runtime_count(requested: u32) -> u32 {
    requested.clamp(MIN_VIEWS, MAX_VIEWS)
}

pub fn azimuth(index: u32) -> [f32; 4] {
    let angle = index as f32 * GOLDEN_ANGLE;
    [angle.cos(), angle.sin(), 0., 0.]
}

/// Upload all MAX_VIEWS once; changing N only changes the uniform, never this bank.
pub fn azimuths() -> Vec<[f32; 4]> {
    (0..MAX_VIEWS).map(azimuth).collect()
}

/// CPU conformance/fixture reference for the exact bank uploaded to the shader.
pub fn direction(index: u32, requested: u32) -> Vec3 {
    let count = runtime_count(requested);
    let index = index.min(count - 1);
    let y = 1. - 2. * (index as f32 + 0.5) / count as f32;
    let radius = (1. - y * y).max(0.).sqrt();
    let a = azimuth(index);
    Vec3::new(a[0] * radius, y, a[1] * radius)
}

pub fn unit_direction(input: Vec3) -> Option<Vec3> {
    if !input.is_finite() {
        return None;
    }
    let largest = input.abs().max_element();
    if largest <= 1e-20 {
        return None;
    }
    Some((input / largest).normalize())
}

/// Strict maximum-dot nearest, with lowest-index ties and invalid -> index 0.
pub fn nearest(view: Vec3, requested: u32) -> u32 {
    let Some(view) = unit_direction(view) else {
        return 0;
    };
    let mut best = -2.;
    let mut selected = 0;
    for index in 0..runtime_count(requested) {
        let score = view.dot(direction(index, requested));
        if score > best {
            best = score;
            selected = index;
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat3, Quat};

    #[test]
    fn bank_is_runtime_owned_and_count_uses_actual_latitude() {
        assert_eq!(azimuths().len(), MAX_VIEWS as usize);
        assert_eq!(runtime_count(0), 8);
        assert_eq!(runtime_count(u32::MAX), 512);
        for count in [0, 8, 9, 37, 128, 256, 511, 512, u32::MAX] {
            let n = runtime_count(count);
            let mut mean_y = 0.;
            for i in 0..n {
                let p = direction(i, count);
                assert!(p.is_finite());
                assert!((p.length() - 1.).abs() < 1e-6);
                assert_eq!(nearest(p, count), i);
                mean_y += p.y;
            }
            assert!((mean_y / n as f32).abs() < 1e-6);
            assert_eq!(direction(u32::MAX, count), direction(n - 1, count));
        }
        assert_ne!(direction(64, 128), direction(64, 256));
        assert_eq!(nearest(Vec3::Y, 128), 0);
        assert_eq!(nearest(-Vec3::Y, 256), 255);
        for invalid in [Vec3::ZERO, Vec3::NAN, Vec3::INFINITY, Vec3::splat(1e-30)] {
            assert_eq!(nearest(invalid, 256), 0);
        }
        assert_eq!(nearest(Vec3::splat(f32::MAX), 128), nearest(Vec3::ONE, 128));
    }

    #[test]
    fn bank_covers_sphere_and_128_256_are_different() {
        let mut changed = 0;
        for count in [8, 37, 128, 256, 512] {
            for y in -10..=10 {
                for longitude in 0..40 {
                    let h = y as f32 / 10.;
                    let a = longitude as f32 * std::f32::consts::TAU / 40.;
                    let r = (1. - h * h).sqrt();
                    let view = Vec3::new(r * a.cos(), h, r * a.sin());
                    let chosen = direction(nearest(view, count), count);
                    assert!(chosen.dot(view) > 1. - 5. / count as f32);
                    if count == 128 && chosen != direction(nearest(view, 256), 256) {
                        changed += 1;
                    }
                }
            }
        }
        assert_eq!(changed, 21 * 40);
    }

    #[test]
    fn rigid_pivot_camera_pose_normals_and_handedness_contract() {
        let pose = Quat::from_rotation_y(0.7) * Quat::from_rotation_z(0.4);
        let basis = Mat3::from_quat(pose);
        let center = Vec3::new(7., 2., -3.);
        let pivot = Vec3::new(0.4, 1.2, -0.1);
        let scale = 0.05;
        let world_pivot = center + basis * pivot * scale;
        for view in [
            Vec3::Y,
            -Vec3::Y,
            Vec3::Z,
            Vec3::new(0.4, 0.8, -0.6).normalize(),
        ] {
            let camera = world_pivot + view;
            let local = basis.transpose() * view;
            let chosen = direction(nearest(local, 128), 128);
            let corrected = basis * Mat3::from_quat(Quat::from_rotation_arc(chosen, local));
            let rendered_center = world_pivot - corrected * pivot * scale;
            assert!((rendered_center + corrected * pivot * scale - world_pivot).length() < 1e-6);
            assert!(
                (corrected.transpose() * (camera - world_pivot).normalize() - chosen).length()
                    < 1e-5
            );
            assert!(corrected.determinant() > 0.99999);
            let a = corrected * Vec3::X;
            let b = corrected * Vec3::Y;
            assert!((a.cross(b) - corrected * Vec3::Z).length() < 1e-5);
            assert_eq!(camera, world_pivot + view); // Never quantize the player camera.
        }
    }

    #[test]
    #[ignore = "requires Slang CPU executable backend; run explicitly during render validation"]
    fn slang_cpu_matches_uploaded_bank_and_rust_nearest_rule() {
        use std::{fmt::Write, process::Command};
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("uploaded_bank_conformance.slang");
        let executable = directory.path().join("uploaded-bank-conformance");
        let scalar = |v: f32| format!("asfloat(0x{:08x}u)", v.to_bits());
        let vector = |v: Vec3| format!("float3({},{},{})", scalar(v.x), scalar(v.y), scalar(v.z));
        let mut code = String::from("import model_view_quantization; import model_pixel_types;\nstatic const float2 azimuths[512] = {\n");
        for a in azimuths() {
            writeln!(code, "float2({},{}),", scalar(a[0]), scalar(a[1])).unwrap();
        }
        code.push_str("};\nstruct UploadedBank : IModelViewBank { float3 direction(uint i,uint n) { return modelViewDirection(azimuths[i],i,n); } }\nexport __extern_cpp int main() { UploadedBank bank; ModelPixelFrame f=identityModelPixelFrame(); f.center=float3(7,2,-3);f.scale=0.05;f.axisX=float3(0,0,-1);f.axisZ=float3(1,0,0); float3 pivot=float3(0.4,1.2,-0.1);\n");
        let physical = Mat3::from_cols(-Vec3::Z, Vec3::Y, Vec3::X);
        let pivot = Vec3::new(0.4, 1.2, -0.1);
        let center = Vec3::new(7., 2., -3.);
        let world_pivot = center + physical * pivot * 0.05;
        let mut cases = 0;
        for count in [0, 8, 37, 128, 256, 512, u32::MAX] {
            for sample in 0..48 {
                let t = sample as f32;
                let view =
                    Vec3::new((t * 0.73).sin(), (t * 0.27).cos(), (t * 0.91).cos()).normalize();
                let camera = world_pivot + view;
                let local = (physical.transpose() * (camera - world_pivot).normalize()).normalize();
                let index = nearest(local, count);
                let rendered = physical
                    * Mat3::from_quat(Quat::from_rotation_arc(direction(index, count), local));
                writeln!(code, "{{ float3 camera={}; ModelPixelFrame r=quantizeModelView(f,pivot,camera,true,{}u,bank); if(nearestModelView({},{}u,bank)!={}u)return 1; if(length(r.axisX-{})>0.00003||length(r.axisY-{})>0.00003||length(r.axisZ-{})>0.00003)return 2; }}", vector(camera), count, vector(local), count, index, vector(rendered.x_axis), vector(rendered.y_axis), vector(rendered.z_axis)).unwrap();
                cases += 1;
            }
        }
        writeln!(code, "printf(\"Rust uploaded azimuth bank -> Slang runtime helper: {cases} nearest and rigid-frame cases passed\\n\"); return 0; }}").unwrap();
        std::fs::write(&source, code).unwrap();
        let compiler = std::env::var_os("SLANGC").unwrap_or_else(|| "slangc".into());
        let output = Command::new(compiler)
            .arg(&source)
            .args(["-std", "2025", "-I"])
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/shader/slang"))
            .args(["-target", "executable", "-o"])
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(executable).output().unwrap();
        assert!(output.status.success(), "Slang/Rust mismatch: {output:?}");
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }

    #[test]
    fn native_vertex_graph_cannot_regress_to_test_only_view_math() {
        let mesh = include_str!("../../shader/slang/model_mesh.slang");
        let flower = include_str!("../../shader/slang/flower_mesh.vert.slang");
        let particle = include_str!("../../shader/slang/particle_mesh.vert.slang");
        let adapter = include_str!("../../shader/slang/model_mesh_view.slang");
        let helper = include_str!("../../shader/slang/model_view_quantization.slang");
        let frame = include_str!("model_mesh_frame.rs");
        assert!(mesh.contains("modelMeshViewFrame("));
        assert!(flower.contains("modelMeshViewFrame("));
        assert!(particle.contains("modelMeshViewFrame("));
        for apple in [
            include_str!("../../shader/slang/apple_mesh_tree.vert.slang"),
            include_str!("../../shader/slang/apple_mesh_dynamic.vert.slang"),
        ] {
            assert!(apple.contains("appleMeshVertex("));
        }
        assert!(adapter.contains("quantizeModelView("));
        assert!(adapter.contains("model_view_azimuths[index]"));
        assert!(helper.contains("nearestModelView(localView"));
        assert!(frame.contains("model_pixel_views::azimuths()"));
        assert!(frame.contains("\"model_view_azimuths\""));
        for shader in [mesh, flower, particle] {
            assert!(shader.contains("camera_info.view_proj_mat,float4(position,1)"));
            assert!(!shader.contains("SV_Depth"));
        }
    }
}
