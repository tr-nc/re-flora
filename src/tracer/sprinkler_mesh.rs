//! Authored lathed garden hardware, not voxel cubes. Large material regions survive scene pixels.
use crate::sprinkler::WORLD_SCALE;
#[cfg(test)]
use crate::sprinkler::{HEAD_LIFT, NOZZLE_EXTENSION, NOZZLE_HEIGHT, NOZZLE_REACH};
use glam::Vec3;

mod candidates;

pub(super) fn build_design(design: crate::sprinkler::Design) -> (Vec<SprinklerVertex>, Vec<u32>) {
    candidates::build(design)
}

const SAGE: Vec3 = Vec3::new(0.39, 0.57, 0.43);
const SAGE_LIGHT: Vec3 = Vec3::new(0.55, 0.69, 0.48);
const SAGE_DARK: Vec3 = Vec3::new(0.23, 0.36, 0.29);
#[cfg(test)]
const CREAM: Vec3 = Vec3::new(0.89, 0.83, 0.64);
const COPPER: Vec3 = Vec3::new(0.72, 0.39, 0.20);
const BRASS: Vec3 = Vec3::new(0.91, 0.66, 0.35);
const DARK: Vec3 = Vec3::new(0.18, 0.23, 0.20);

/// Authoring vertices; GPU draws consume the shared model-geometry ABI.
#[derive(Clone, Copy)]
pub(super) struct SprinklerVertex {
    pub position: [f32; 3],
    pub shading_normal: [f32; 3],
    pub color_srgb: [f32; 3],
    pub animation_direction: [f32; 3],
}

#[derive(Default)]
struct Mesh {
    vertices: Vec<SprinklerVertex>,
    indices: Vec<u32>,
}

impl Mesh {
    fn triangle(&mut self, points: [Vec3; 3], color: Vec3, motion: Vec3) {
        let cross = (points[1] - points[0]).cross(points[2] - points[0]);
        // Axis endpoints close a lathe with triangles, never zero-area quads.
        if cross.length_squared() < 1e-12 {
            return;
        }
        let normal = cross.normalize();
        for point in points {
            self.indices.push(self.vertices.len() as u32);
            self.vertices.push(SprinklerVertex {
                position: (point * WORLD_SCALE).to_array(),
                shading_normal: normal.to_array(),
                color_srgb: color.to_array(),
                animation_direction: motion.to_array(),
            });
        }
    }

    /// Each profile row owns the band ending at it. Constant band colors and real
    /// face normals give broad, quiet facets instead of specular/noise highlights.
    fn lathe(
        &mut self,
        origin: Vec3,
        axis: Vec3,
        profile: &[(f32, f32, Vec3)],
        sides: u32,
        motion: Vec3,
    ) {
        let reference = if axis.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
        let u = (reference - axis * reference.dot(axis)).normalize();
        let v = axis.cross(u);
        for band in profile.windows(2) {
            let [(h0, r0, _), (h1, r1, color)] = [band[0], band[1]];
            for side in 0..sides {
                let radial = |i: u32| {
                    let angle = (i % sides) as f32 * std::f32::consts::TAU / sides as f32;
                    u * angle.cos() + v * angle.sin()
                };
                let a = radial(side);
                let b = radial(side + 1);
                let points = [
                    origin + axis * h0 + a * r0,
                    origin + axis * h0 + b * r0,
                    origin + axis * h1 + b * r1,
                    origin + axis * h1 + a * r1,
                ];
                self.triangle([points[0], points[1], points[2]], color, motion);
                self.triangle([points[0], points[2], points[3]], color, motion);
            }
        }
    }
}

#[cfg(test)]
pub(super) fn build_sprinkler_mesh() -> (Vec<SprinklerVertex>, Vec<u32>) {
    let mut mesh = Mesh::default();
    // Low, rolled enamel foot: a stable planted silhouette, not an industrial spike.
    mesh.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (0., 0., DARK),
            (0., 1.5, DARK),
            (0.18, 1.75, SAGE_DARK),
            (0.42, 1.75, SAGE),
            (0.62, 1.48, SAGE_LIGHT),
            (0.88, 0.80, SAGE),
            (0.88, 0., SAGE),
        ],
        16,
        Vec3::ZERO,
    );
    // Cream barrel and brass compression collars. The inner piston reaches up
    // inside the moving head at both travel extremes; there is never a floating cap.
    mesh.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (0.65, 0., COPPER),
            (0.65, 0.70, COPPER),
            (0.90, 0.70, BRASS),
            (1.05, 0.53, CREAM),
            (2.65, 0.53, CREAM),
            (2.76, 0.63, BRASS),
            (3.00, 0.63, COPPER),
            (3.00, 0.39, BRASS),
            (4.55, 0.39, BRASS),
            (4.55, 0., BRASS),
        ],
        12,
        Vec3::ZERO,
    );
    let lift = Vec3::Y * HEAD_LIFT;
    mesh.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (3.05, 0., DARK),
            (3.05, 0.72, DARK),
            (3.38, 0.72, SAGE_DARK),
            (3.55, 1.04, COPPER),
            (3.82, 1.04, BRASS),
            (4.27, 0.96, SAGE),
            (4.63, 0.68, SAGE_LIGHT),
            (4.75, 0.33, SAGE_LIGHT),
            (4.75, 0., SAGE_LIGHT),
        ],
        16,
        lift,
    );
    // Four warm copper spouts, with thick rolled lips and dark recessed bores.
    // Only the active opposing pair extends. Small overlaps hide the sliding seams.
    for axis in [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z] {
        mesh.lathe(
            Vec3::Y * NOZZLE_HEIGHT,
            axis,
            &[
                (0.70, 0., COPPER),
                (0.70, 0.31, COPPER),
                (2.20, 0.31, COPPER),
                (2.34, 0.49, BRASS),
                (2.83, 0.49, COPPER),
                (NOZZLE_REACH, 0.39, BRASS),
                (NOZZLE_REACH, 0.23, BRASS),
                (NOZZLE_REACH - 0.10, 0.23, DARK),
                (NOZZLE_REACH - 0.10, 0., DARK),
            ],
            12,
            lift + axis * NOZZLE_EXTENSION,
        );
    }
    // Oversized cream adjustment screw reads as a single glint at play distance.
    mesh.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (4.70, 0., COPPER),
            (4.70, 0.30, COPPER),
            (4.88, 0.30, CREAM),
            (4.94, 0.22, CREAM),
            (4.94, 0., CREAM),
        ],
        8,
        lift,
    );
    // An inset slot, not a texture or second per-object pixel filter.
    let slot = [
        Vec3::new(-0.19, 4.945, -0.045),
        Vec3::new(-0.19, 4.945, 0.045),
        Vec3::new(0.19, 4.945, 0.045),
        Vec3::new(0.19, 4.945, -0.045),
    ];
    mesh.triangle([slot[0], slot[1], slot[2]], COPPER, lift);
    mesh.triangle([slot[0], slot[2], slot[3]], COPPER, lift);
    (mesh.vertices, mesh.indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprinkler::Pose;

    #[test]
    fn mesh_has_real_normals_outward_winding_and_bounded_finite_geometry() {
        let (vertices, indices) = build_sprinkler_mesh();
        assert!(indices.len() / 3 < 1600, "small shared prop mesh budget");
        for tri in indices.chunks_exact(3) {
            let points = [tri[0], tri[1], tri[2]]
                .map(|i| Vec3::from_array(vertices[i as usize].position) / WORLD_SCALE);
            let normal = (points[1] - points[0])
                .cross(points[2] - points[0])
                .normalize();
            assert!(normal.is_finite());
            for &i in tri {
                let vertex = vertices[i as usize];
                let shading = Vec3::from_array(vertex.shading_normal);
                assert!(normal.dot(shading) > 0.999);
                assert!((shading.length() - 1.).abs() < 1e-6);
                let p = Vec3::from_array(vertex.position) / WORLD_SCALE;
                assert!(p.is_finite() && p.y >= 0. && p.y <= 4.95);
                assert!(p.x.abs() <= NOZZLE_REACH && p.z.abs() <= NOZZLE_REACH);
                assert!(Vec3::from_array(vertex.color_srgb).min_element() >= 0.);
                assert!(Vec3::from_array(vertex.color_srgb).max_element() <= 1.);
            }
        }
    }

    #[test]
    fn approved_compact_and_tall_hardware_have_complete_geometry_and_the_same_mouth() {
        let mut signatures = Vec::new();
        for design in crate::sprinkler::Design::ALL {
            let lift = if design == crate::sprinkler::Design::Tall {
                crate::sprinkler::TALL_LIFT_VOXELS
            } else {
                0.0
            };
            let outlet =
                crate::sprinkler::RotorPose { angle: 0. }.outlet() + Vec3::Y * lift * WORLD_SCALE;
            let (vertices, indices) = build_design(design);
            assert!(indices.len() / 3 < 2200, "bounded prop mesh: {design:?}");
            assert!(vertices.iter().any(|v| v.animation_direction == [0.; 3]));
            assert!(vertices.iter().any(|v| v.animation_direction != [0.; 3]));
            let mut rim = Vec::new();
            for v in &vertices {
                let p = Vec3::from_array(v.position) / WORLD_SCALE;
                let normal = Vec3::from_array(v.shading_normal);
                assert!(
                    p.is_finite()
                        && p.y >= -1e-5
                        && p.y < 6.5 + lift
                        && p.x.abs() < 5.
                        && p.z.abs() < 5.
                );
                assert!(normal.is_finite() && (normal.length() - 1.).abs() < 1e-5);
                assert!(v.color_srgb.iter().all(|c| (0.0..=1.0).contains(c)));
                if v.animation_direction != [0.; 3]
                    && ((Vec3::from_array(v.position) - outlet).length() / WORLD_SCALE - 0.34).abs()
                        < 1e-4
                {
                    rim.push(Vec3::from_array(v.position));
                }
            }
            assert!(rim.len() >= 20, "one real outlet rim for {design:?}");
            let center = rim.iter().sum::<Vec3>() / rim.len() as f32;
            for frame in 0..crate::sprinkler::ROTARY_FRAMES {
                let pose = crate::sprinkler::RotorPose {
                    angle: frame as f32 * std::f32::consts::TAU
                        / crate::sprinkler::ROTARY_FRAMES as f32,
                };
                assert!(
                    pose.rotate(center)
                        .distance(pose.outlet() + Vec3::Y * lift * WORLD_SCALE)
                        < 1e-6
                );
            }
            for tri in indices.chunks_exact(3) {
                let p = tri
                    .iter()
                    .map(|i| Vec3::from_array(vertices[*i as usize].position))
                    .collect::<Vec<_>>();
                let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
                assert!(n.dot(Vec3::from_array(vertices[tri[0] as usize].shading_normal)) > 0.999);
            }
            let signature = vertices.iter().fold(0u64, |hash, v| {
                v.position.iter().fold(hash, |h, x| {
                    h.wrapping_mul(1099511628211) ^ u64::from(x.to_bits())
                })
            });
            assert!(!signatures.contains(&signature));
            signatures.push(signature);
        }
    }

    #[test]
    fn animated_nozzle_rims_enclose_the_real_water_outlets() {
        let (vertices, _) = build_sprinkler_mesh();
        for tick in 0..40 {
            let pose = Pose::sample(tick, 0.05, 0.);
            for sign in [-1., 1.] {
                let axis = pose.axis() * sign;
                let rim: Vec<_> = vertices
                    .iter()
                    .filter(|v| {
                        let p = Vec3::from_array(v.position) / WORLD_SCALE;
                        (p.dot(axis) - NOZZLE_REACH).abs() < 1e-5
                    })
                    .collect();
                assert!(!rim.is_empty());
                for v in rim {
                    let animated = Vec3::from_array(v.position)
                        + Vec3::from_array(v.animation_direction) * pose.extension * WORLD_SCALE;
                    let difference = (animated - pose.outlet(sign)) / WORLD_SCALE;
                    assert!(difference.dot(axis).abs() < 1e-5);
                    assert!(difference.length() <= 0.391);
                }
            }
        }
    }
}
