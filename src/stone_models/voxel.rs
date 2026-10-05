//! Adapter contract for the existing GPU model_voxelize + surface + Contree path.
//! The preview atlas is private and bounded; it never touches the world's atlas.
use super::StoneMesh;
use crate::builder::ModelTriangleGpu;
use anyhow::{ensure, Result};
use glam::{DVec3, UVec3, Vec3};

pub const DIM: u32 = 64;
pub const VOXELS_PER_UNIT: f32 = 128.;
pub const SURFACE_THICKNESS: f32 = 0.75;
pub const ATLAS_PIVOT_VOX: Vec3 = Vec3::new(32., 2., 32.);
pub const CHUNK_ORIGIN: Vec3 = Vec3::new(-0.25, -2. / VOXELS_PER_UNIT, -0.25);
pub const CHUNK_SIZE: f32 = DIM as f32 / VOXELS_PER_UNIT;

pub fn triangles(mesh: &StoneMesh) -> Vec<ModelTriangleGpu> {
    // Existing kernel is explicitly 256 voxels/world unit. This adapter's
    // half-scale and offset give 128 preview cells per garden world unit.
    // The direct model is not resized; Contree's local scale restores units.
    mesh.triangles()
        .map(|t| {
            let [a, b, c] = t.map(|p| (p * 0.5).extend(0.).to_array());
            ModelTriangleGpu { a, b, c }
        })
        .collect()
}
pub fn index(p: UVec3) -> usize {
    ((p.z * DIM + p.y) * DIM + p.x) as usize
}
pub fn local_center(p: UVec3) -> Vec3 {
    (p.as_vec3() + Vec3::splat(0.5) - ATLAS_PIVOT_VOX) / VOXELS_PER_UNIT
}

/// Pure winding reference, independent of the convex generator's half-space test.
pub fn inside(mesh: &StoneMesh, point: Vec3) -> bool {
    let p = point.as_dvec3();
    let sum: f64 = mesh
        .triangles()
        .map(|t| {
            let [a, b, c] = t.map(|v| v.as_dvec3() - p);
            let [la, lb, lc] = [a, b, c].map(DVec3::length);
            let numerator = a.dot(b.cross(c));
            let denominator = la * lb * lc + a.dot(b) * lc + b.dot(c) * la + c.dot(a) * lb;
            2. * numerator.atan2(denominator)
        })
        .sum();
    sum.abs() > std::f64::consts::TAU
}
fn distance_squared(p: Vec3, [a, b, c]: [Vec3; 3]) -> f32 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0. && d2 <= 0. {
        return ap.length_squared();
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0. && d4 <= d3 {
        return bp.length_squared();
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0. && d1 >= 0. && d3 <= 0. {
        return (p - (a + ab * (d1 / (d1 - d3)))).length_squared();
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0. && d5 <= d6 {
        return cp.length_squared();
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0. && d2 >= 0. && d6 <= 0. {
        return (p - (a + ac * (d2 / (d2 - d6)))).length_squared();
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0. && d4 - d3 >= 0. && d5 - d6 >= 0. {
        return (p - (b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6))))).length_squared();
    }
    let n = ab.cross(ac).normalize();
    n.dot(p - a).powi(2)
}
pub fn occupied(mesh: &StoneMesh, point: Vec3) -> bool {
    point.is_finite()
        && (inside(mesh, point)
            || mesh.triangles().any(|t| {
                distance_squared(point, t) <= (SURFACE_THICKNESS / VOXELS_PER_UNIT).powi(2)
            }))
}

/// Native fixture checks the real GPU atlas, including filled interior, padding,
/// type/state format, and 4096 deterministic sample cells against winding+shell.
pub fn validate_atlas(mesh: &StoneMesh, bytes: &[u8], voxel_type: u32) -> Result<(usize, usize)> {
    ensure!(
        bytes.len() == DIM.pow(3) as usize,
        "stone atlas dimension mismatch"
    );
    ensure!(
        bytes.iter().all(|&b| b == 0 || u32::from(b) == voxel_type),
        "stone atlas retained invalid type/state"
    );
    let count = bytes.iter().filter(|&&b| b != 0).count();
    ensure!(count > 0, "stone GPU voxelizer authored no solid");
    let mut checked = 0;
    for z in (0..DIM).step_by(4) {
        for y in (0..DIM).step_by(4) {
            for x in (0..DIM).step_by(4) {
                let p = UVec3::new(x, y, z);
                let center = local_center(p);
                let expected = occupied(mesh, center);
                let actual = bytes[index(p)] != 0;
                ensure!(expected==actual,"stone GPU occupancy differs from winding/surface reference at {p:?}: expected={expected} actual={actual}");
                checked += 1;
            }
        }
    }
    for axis in 0..3 {
        for side in [0, DIM - 1] {
            for a in 0..DIM {
                for b in 0..DIM {
                    let mut p = UVec3::ZERO;
                    p[axis] = side;
                    p[(axis + 1) % 3] = a;
                    p[(axis + 2) % 3] = b;
                    ensure!(
                        bytes[index(p)] == 0,
                        "stone clipped into preview atlas boundary"
                    );
                }
            }
        }
    }
    ensure!(
        mesh.contains((mesh.min + mesh.max) * 0.5, 0.),
        "stone has no solid at its bounded center"
    );
    let center = ((mesh.min + mesh.max) * 0.5 * VOXELS_PER_UNIT + ATLAS_PIVOT_VOX).as_uvec3();
    ensure!(
        bytes[index(center)] != 0,
        "stone GPU voxelizer made a shell without filled interior"
    );
    Ok((count, checked))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::stone_models::{generate, StoneKind, StoneSpec};
    #[test]
    fn both_adapters_take_the_same_triangle_source_and_preserve_units() {
        for kind in [StoneKind::Slab, StoneKind::Rock] {
            for seed in 0..8 {
                let mesh = generate(StoneSpec::new(kind, seed)).unwrap();
                let gpu = triangles(&mesh);
                for (a, b) in mesh.triangles().zip(gpu) {
                    for (original, adapted) in a.into_iter().zip([b.a, b.b, b.c]) {
                        assert_eq!(original, Vec3::from_slice(&adapted) * 2.);
                    }
                }
                for p in &mesh.positions {
                    let atlas = *p * VOXELS_PER_UNIT + ATLAS_PIVOT_VOX;
                    assert!(
                        atlas.cmpgt(Vec3::ONE).all()
                            && atlas.cmplt(Vec3::splat(DIM as f32 - 1.)).all()
                    );
                }
            }
        }
    }
    #[test]
    fn winding_and_convex_solid_agree_across_types_seeds_and_params() {
        for kind in [StoneKind::Slab, StoneKind::Rock] {
            for seed in 0..12 {
                let mut s = StoneSpec::new(kind, seed);
                s.variation = (seed % 3) as f32 * 0.5;
                s.rock.facets = 8 + seed;
                let mesh = generate(s).unwrap();
                for y in 0..5 {
                    for z in 0..5 {
                        for x in 0..5 {
                            let point = Vec3::new(x as f32 - 2., y as f32, z as f32 - 2.) * 0.071
                                + Vec3::Y * 0.007;
                            assert_eq!(
                                inside(&mesh, point),
                                mesh.contains(point, 0.),
                                "{kind:?} {seed} {point:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn voxel_adapter_has_filled_interior_surface_shell_and_empty_exterior() {
        for kind in [StoneKind::Slab, StoneKind::Rock] {
            let m = generate(StoneSpec::new(kind, 17)).unwrap();
            assert!(occupied(&m, Vec3::Y * m.spec.size.y * 0.5));
            assert!(occupied(&m, Vec3::Y * (m.max.y + 0.5 / VOXELS_PER_UNIT)));
            assert!(!occupied(&m, Vec3::Y * (m.max.y + 2. / VOXELS_PER_UNIT)));
            assert!(!occupied(&m, Vec3::splat(2.)));
            assert!(!occupied(&m, Vec3::splat(f32::NAN)));
        }
    }
}
