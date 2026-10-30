//! Contact points come from the same authored triangles as the visible flower,
//! never the root, a bounding-sphere center or the stem socket plane.
use super::models::{Flower, Part};
use glam::Vec3;

pub(crate) fn flower_landing(flower: &Flower, head: &Part) -> Option<(Vec3, Vec3)> {
    // The first shrub ships in fruit, not as a pollinator flower.
    if flower.assembly == super::models::Assembly::Rooted {
        return None;
    }
    let up = flower.socket_normal;
    let origin = head.anchor + up * (head.radius * 3. + 1.);
    flower.triangles[head.triangles.start as usize..head.triangles.end as usize]
        .iter()
        .filter(|t| t.normal.dot(up) > 0.35)
        .filter_map(|t| {
            let [a, b, c] = t.positions;
            let e1 = b - a;
            let e2 = c - a;
            let p = (-up).cross(e2);
            let determinant = e1.dot(p);
            if determinant.abs() < 1e-8 {
                return None;
            }
            let inv = determinant.recip();
            let delta = origin - a;
            let u = delta.dot(p) * inv;
            let q = delta.cross(e1);
            let v = (-up).dot(q) * inv;
            let distance = e2.dot(q) * inv;
            (u >= -1e-5 && v >= -1e-5 && u + v <= 1.00001 && distance >= 0.).then_some((
                distance,
                origin - up * distance,
                t.normal,
            ))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, point, normal)| (point, normal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flora::models::{flowers, Shape};

    #[test]
    fn every_authored_flower_has_a_real_upward_contact_at_all_head_scales() {
        for authored in flowers()
            .iter()
            .filter(|f| f.assembly == super::super::models::Assembly::Stemmed)
        {
            for scale in [0.25, 1., 4.] {
                let flower = authored.transformed(Shape {
                    head_scale: scale,
                    ..Shape::default()
                });
                for head in &flower.heads {
                    let (point, normal) = flower_landing(&flower, head)
                        .expect("central nectar surface must be landable");
                    assert!(point.is_finite() && normal.is_finite());
                    assert!(normal.dot(flower.socket_normal) > 0.35);
                    assert!(point.distance(head.center) <= head.radius + 1e-4);
                    assert!(point.y > flower.column.tip().y - head.radius);
                }
            }
        }
    }
}
