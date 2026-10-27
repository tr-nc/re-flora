//! Resolve fruit placement preferences onto visible, owned wood. A leaf-cluster
//! offset is a candidate location, never proof that a fruit is connected.
use super::*;
use crate::{flora::apple::STEM_TIP_VOXELS, geom::RoundConeClearanceIndex};

pub(super) fn bind_fruit_attachments(
    fruits: &mut Vec<TreeFruitSpec>,
    tree: &Tree,
    tree_position: Vec3,
    cones: &[RoundCone],
) {
    let wood = RoundConeClearanceIndex::new(cones);
    let mut occupied = HashSet::new();
    fruits.retain_mut(|fruit| {
        let desired = fruit.position_voxels.as_vec3() + STEM_TIP_VOXELS;
        let mut best = None;
        for (cone, &branch) in cones.iter().zip(tree.trunk_branch_indices()) {
            // Use fruiting side wood, not the clear trunk. Below, test actual
            // occupied cells: a mathematical twig can be too thin to render.
            if tree
                .axis_segments()
                .get(branch)
                .is_some_and(|axis| axis.order == 0)
            {
                continue;
            }
            let axis = cone.center_b() - cone.center_a();
            let t = ((desired - cone.center_a()).dot(axis) / axis.length_squared().max(1e-6))
                .clamp(0.1, 0.9);
            let radius = cone.radius_a() + (cone.radius_b() - cone.radius_a()) * t;
            let underside = cone.center_a() + axis * t - Vec3::Y * radius;
            let cell = underside.floor();
            for x in -1..=1 {
                for y in -1..=2 {
                    for z in -1..=1 {
                        let center =
                            cell + Vec3::new(x as f32, y as f32, z as f32) + Vec3::splat(0.5);
                        if center.min_element() < 0.
                            || center.y < tree_position.y * 256. + 14.
                            || cone.signed_distance(center) >= -0.05
                            || !wood.has_minimum_clearance(center - Vec3::Y, 0.)
                        {
                            continue;
                        }
                        // Embed the cap slightly inside the last occupied wood cell,
                        // not at a quantized fruit center or at the leaf envelope.
                        let pivot = center - Vec3::Y * 0.15;
                        if cone.signed_distance(pivot) >= 0. {
                            continue;
                        }
                        let key = (pivot - STEM_TIP_VOXELS).round();
                        if key.min_element() < 0. || occupied.contains(&key.as_uvec3()) {
                            continue;
                        }
                        let distance = pivot.distance_squared(desired);
                        if best.is_none_or(|(d, _, _, _)| distance < d) {
                            best = Some((distance, branch, pivot, key.as_uvec3()));
                        }
                    }
                }
            }
        }
        let Some((_, branch, pivot, key)) = best else {
            // No rendered support means no attached fruit, never a floating fallback.
            return false;
        };
        fruit.position_voxels = key;
        fruit.attachment = Some((branch, pivot));
        occupied.insert(key);
        true
    });
}
