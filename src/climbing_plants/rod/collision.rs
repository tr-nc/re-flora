//! Local exposed-face constraints sampled at segment/expanded-voxel intersections.
//! Unlike endpoint-only contact, these distribute obstacle reaction through the
//! segment interior. Final segment and swept-volume validation remains authoritative.
use crate::climbing_plants::{segment_box_interval, Plant, Terrain};
use glam::{IVec3, Vec3};

pub(super) struct Constraint {
    edge: usize,
    t: f32,
    normal: Vec3,
    plane: f32,
}
pub(super) fn gather(
    plant: &Plant,
    positions: &[Vec3],
    terrain: &impl Terrain,
) -> Option<Vec<Constraint>> {
    let mut constraints = Vec::new();
    let reach = plant.radius + 0.2;
    for i in 1..plant.nodes.len() {
        let a = positions[i - 1];
        let b = positions[i];
        let min = (a.min(b) - Vec3::splat(reach)).floor().as_ivec3();
        let max = (a.max(b) + Vec3::splat(reach)).floor().as_ivec3();
        for z in min.z..=max.z {
            for y in min.y..=max.y {
                for x in min.x..=max.x {
                    let cell = IVec3::new(x, y, z);
                    let Some((enter, exit)) = segment_box_interval(
                        a,
                        b,
                        cell.as_vec3() - Vec3::splat(reach),
                        cell.as_vec3() + Vec3::splat(1.0 + reach),
                    ) else {
                        continue;
                    };
                    if terrain.voxel(cell)? == 0 {
                        continue;
                    }
                    let t = (enter + exit) * 0.5;
                    let point = a.lerp(b, t);
                    let mut best: Option<(f32, Vec3, f32)> = None;
                    for axis in [
                        IVec3::X,
                        IVec3::NEG_X,
                        IVec3::Y,
                        IVec3::NEG_Y,
                        IVec3::Z,
                        IVec3::NEG_Z,
                    ] {
                        if terrain.voxel(cell + axis)? != 0 {
                            continue;
                        }
                        let normal = axis.as_vec3();
                        let face = cell.as_vec3() + Vec3::splat(0.5) + normal * 0.5;
                        let plane = face.dot(normal) + plant.radius + 0.06;
                        let depth = plane - point.dot(normal);
                        if best.is_none_or(|(d, _, _)| depth < d) {
                            best = Some((depth, normal, plane));
                        }
                    }
                    if let Some((_, normal, plane)) = best {
                        // A midpoint can be clear while an interval endpoint still
                        // penetrates (notably a tip approaching a hole ceiling).
                        // Both ends bound the entire linear segment/voxel interval.
                        for t in [enter, exit] {
                            constraints.push(Constraint {
                                edge: i,
                                t,
                                normal,
                                plane,
                            });
                        }
                    }
                }
            }
        }
    }
    Some(constraints)
}
pub(super) fn project(constraints: &[Constraint], mobility: &[f32], positions: &mut [Vec3]) {
    for c in constraints {
        let a = c.edge - 1;
        let b = c.edge;
        let point = positions[a].lerp(positions[b], c.t);
        let depth = (c.plane - point.dot(c.normal)).max(0.0);
        let wa = 1.0 - c.t;
        let wb = c.t;
        let denominator = mobility[a] * wa * wa + mobility[b] * wb * wb;
        if denominator > 0.0 {
            positions[a] += c.normal * (depth * mobility[a] * wa / denominator);
            positions[b] += c.normal * (depth * mobility[b] * wb / denominator);
        }
    }
}
