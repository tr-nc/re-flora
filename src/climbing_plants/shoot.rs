//! Gravity-biased elastic pose of the current young shoot, not whole-plant falling.
//! Fixed stem history is immutable. Forward kinematics preserves every segment length;
//! a bounded line search accepts only a collision-clear swept pose transaction.
use super::{clear_segment, growth, Plant, Terrain};
use glam::Vec3;

const BEND_LENGTH: f32 = 40.0;
const RESPONSE: f32 = 8.0;
const MAX_DISPLACEMENT: f32 = 0.25;

pub(super) fn relax(
    plant: &mut Plant,
    terrain: &impl Terrain,
    dt: f32,
    flexibility: f32,
    spacing: f32,
) -> Option<usize> {
    if !dt.is_finite()
        || dt <= 0.0
        || !flexibility.is_finite()
        || flexibility < 0.0
        || !spacing.is_finite()
        || spacing < 4.0
        || !plant.root_connected
        || plant.tips.len() != 1
        || plant.tips[0].restart.is_some()
    {
        return Some(0);
    }
    if !terrain.current() {
        return None;
    }
    if !plant.root_supported(terrain)? {
        return Some(0);
    }
    let mut active = Vec::new();
    let mut node = plant.tips[0].node;
    while !plant.nodes[node].fixed {
        active.push(node);
        node = plant.nodes[node].parent?;
    }
    if active.is_empty() {
        return Some(0);
    }
    active.reverse();
    let base = plant.nodes[node].position;
    let length: f32 = active.iter().map(|&i| plant.nodes[i].rest_length).sum();
    let search = (growth::probe(plant, &plant.tips[0]) - plant.nodes[plant.tips[0].node].position)
        .normalize();
    let search_direction = Vec3::new(search.x, 0.0, search.z);
    let mut distance = 0.0;
    let targets: Vec<_> = active
        .iter()
        .map(|&i| {
            let node = &plant.nodes[i];
            distance += node.rest_length;
            // Integrated bending load grows with BOTH distance from the clamp and distal
            // length. Adding a new segment therefore changes previously grown young nodes.
            let load =
                flexibility.min(2.0) * distance * (2.0 * length - distance) / BEND_LENGTH.powi(2);
            let gravity = Vec3::NEG_Y - node.rest_direction * node.rest_direction.dot(Vec3::NEG_Y);
            // Circumnutation bends the growing zone, not just an isolated endpoint.
            let exploration =
                search_direction * (0.3 * flexibility.min(2.0) * (distance / length).powi(2));
            (node.rest_direction + gravity * load + exploration).normalize()
        })
        .collect();
    let response = 1.0 - (-RESPONSE * dt.min(0.1)).exp();
    for trial in 0..10 {
        let alpha = response * 0.5f32.powi(trial);
        let mut parent = base;
        let mut positions = Vec::with_capacity(active.len());
        let mut max_movement = 0.0f32;
        let mut feasible = true;
        for (&id, target) in active.iter().zip(&targets) {
            let node = &plant.nodes[id];
            let old_direction = (node.position - plant.nodes[node.parent?].position).normalize();
            let mut direction = old_direction.lerp(*target, alpha).normalize();
            if let Some(contact) = &node.contact {
                if terrain.voxel(contact.cell)? == plant.anchors[0].material {
                    let normal = contact.normal;
                    let face = contact.cell.as_vec3() + Vec3::splat(0.5) + normal * 0.5;
                    let minimum =
                        ((face - parent).dot(normal) + plant.radius + 0.1) / node.rest_length;
                    if minimum > 1.0 {
                        feasible = false;
                        break;
                    }
                    if direction.dot(normal) < minimum {
                        let tangent = (direction - normal * direction.dot(normal))
                            .try_normalize()
                            .unwrap_or_else(|| normal.any_orthonormal_vector());
                        direction =
                            normal * minimum + tangent * (1.0 - minimum * minimum).max(0.0).sqrt();
                    }
                }
            }
            let position = parent + direction * node.rest_length;
            max_movement = max_movement.max(position.distance(node.position));
            positions.push(position);
            parent = position;
        }
        if !feasible || max_movement > MAX_DISPLACEMENT {
            continue;
        }
        let mut clear = true;
        let mut parent = base;
        for (&id, &position) in active.iter().zip(&positions) {
            let node = &plant.nodes[id];
            let old_parent = plant.nodes[node.parent?].position;
            if !clear_sweep(
                terrain,
                [old_parent, node.position, parent, position],
                plant.radius,
            )? || !clear_segment(terrain, parent, position, plant.radius)?
            {
                clear = false;
                break;
            }
            parent = position;
        }
        if !clear {
            continue;
        }
        let mut next = plant.clone();
        let mut moved = 0;
        for (&id, &position) in active.iter().zip(&positions) {
            moved += usize::from(position.distance_squared(next.nodes[id].position) > 1e-10);
            next.nodes[id].position = position;
            let contact = growth::contact_at(&next, &next.nodes[id], terrain)?;
            let backing = growth::backing_for(
                &next,
                &next.nodes[next.nodes[id].parent?],
                position,
                contact.as_ref(),
                terrain,
            )?;
            if let Some(contact) = &contact {
                next.nodes[id].normal = contact.normal;
            }
            next.nodes[id].contact = contact;
            next.nodes[id].backing = backing;
        }
        let tip = &next.tips[0];
        if let Some(contact) = next.nodes[tip.node].contact.clone() {
            next.attach_tip(0, contact, spacing);
        }
        if !terrain.current() {
            return None;
        }
        *plant = next;
        return Some(moved);
    }
    Some(0)
}

// The convex hull of both endpoint pairs encloses the entire segment sweep.
// SAT against radius-expanded voxels allows tangential/away motion at contacts
// without the false blocking of an isotropically inflated old segment.
pub(super) fn clear_sweep(terrain: &impl Terrain, points: [Vec3; 4], radius: f32) -> Option<bool> {
    let min = (points.into_iter().fold(points[0], Vec3::min) - Vec3::splat(radius))
        .floor()
        .as_ivec3();
    let max = (points.into_iter().fold(points[0], Vec3::max) + Vec3::splat(radius))
        .floor()
        .as_ivec3();
    for z in min.z..=max.z {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let cell = glam::IVec3::new(x, y, z);
                let center = cell.as_vec3() + Vec3::splat(0.5);
                let p = points.map(|point| point - center);
                let extent = 0.5 + radius;
                let separated = |axis: Vec3| {
                    if axis.length_squared() < 1e-14 {
                        return false;
                    }
                    let lo = p
                        .into_iter()
                        .map(|v| v.dot(axis))
                        .fold(f32::INFINITY, f32::min);
                    let hi = p
                        .into_iter()
                        .map(|v| v.dot(axis))
                        .fold(f32::NEG_INFINITY, f32::max);
                    let bound = (extent + 0.0001) * axis.abs().element_sum();
                    lo > bound || hi < -bound
                };
                if [Vec3::X, Vec3::Y, Vec3::Z].into_iter().any(separated) {
                    continue;
                }
                if terrain.voxel(cell)? == 0 {
                    continue;
                }
                if [(0, 1, 2), (0, 1, 3), (0, 2, 3), (1, 2, 3)]
                    .into_iter()
                    .any(|(a, b, c)| separated((p[b] - p[a]).cross(p[c] - p[a])))
                {
                    continue;
                }
                if [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]
                    .into_iter()
                    .any(|(a, b)| {
                        [Vec3::X, Vec3::Y, Vec3::Z]
                            .into_iter()
                            .any(|axis| separated((p[b] - p[a]).cross(axis)))
                    })
                {
                    continue;
                }
                return Some(false);
            }
        }
    }
    Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::climbing_plants::{Contact, Node};
    use glam::IVec3;
    struct Wall;
    impl Terrain for Wall {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            Some(u8::from(c.z == 0))
        }
        fn current(&self) -> bool {
            true
        }
    }
    fn append(plant: &mut Plant) {
        let parent = plant.nodes.len() - 1;
        plant.nodes.push(Node {
            id: plant.next_node_id,
            parent: Some(parent),
            position: plant.nodes[parent].position + Vec3::X * 2.0,
            rest_length: 2.0,
            rest_direction: Vec3::X,
            fixed: false,
            normal: Vec3::Z,
            contact: None,
            backing: None,
        });
        plant.next_node_id += 1;
        plant.tips[0].node = plant.nodes.len() - 1;
        plant.tips[0].arc += 2.0;
    }
    fn shoot() -> Plant {
        let mut plant = Plant::seed(
            Vec3::new(20.5, 4.5, 1.8),
            Vec3::Z,
            IVec3::new(20, 4, 0),
            1,
            42,
        );
        for _ in 0..6 {
            append(&mut plant);
        }
        plant
    }
    fn settle(plant: &mut Plant) {
        for _ in 0..30 {
            plant.relax_shoot(&Wall, 0.05, 1.0, 16.0).unwrap();
        }
    }
    #[test]
    fn swept_segment_checks_its_interior_but_allows_tangential_wall_motion() {
        struct Cube;
        impl Terrain for Cube {
            fn voxel(&self, cell: IVec3) -> Option<u8> {
                Some(u8::from(cell == IVec3::ZERO))
            }
            fn current(&self) -> bool {
                true
            }
        }
        let points = [
            Vec3::new(-2., -2., 0.5),
            Vec3::new(-2., 2., 0.5),
            Vec3::new(2., -2., 0.5),
            Vec3::new(2., 2., 0.5),
        ];
        for (a, b) in [(0, 1), (2, 3), (0, 2), (1, 3)] {
            assert_eq!(clear_segment(&Cube, points[a], points[b], 0.1), Some(true));
        }
        assert_eq!(
            clear_sweep(&Cube, points, 0.1),
            Some(false),
            "the swept interior tunnels through a voxel"
        );
        let points = [
            Vec3::new(20., 4., 1.7),
            Vec3::new(22., 4., 1.7),
            Vec3::new(20., 3.5, 1.7),
            Vec3::new(22., 3.5, 1.7),
        ];
        assert_eq!(clear_sweep(&Wall, points, 0.65), Some(true));
    }

    #[test]
    fn extension_changes_older_young_nodes_without_stretching_or_moving_the_root() {
        let mut plant = shoot();
        settle(&mut plant);
        let before = plant.clone();
        append(&mut plant);
        settle(&mut plant);
        assert_eq!(plant.nodes[0], before.nodes[0]);
        assert!(plant.nodes[3].position.y < before.nodes[3].position.y - 0.02);
        for node in &plant.nodes[1..] {
            let parent = plant.nodes[node.parent.unwrap()].position;
            assert!((parent.distance(node.position) - node.rest_length).abs() < 0.002);
            assert_eq!(
                clear_segment(&Wall, parent, node.position, plant.radius),
                Some(true)
            );
        }
    }
    #[test]
    fn a_new_attachment_freezes_the_old_shoot_and_only_new_growth_remains_flexible() {
        let mut plant = shoot();
        settle(&mut plant);
        plant.tips[0].spacing = 8.0;
        plant.relax_shoot(&Wall, 0.05, 1.0, 16.0).unwrap();
        assert_eq!(plant.anchors.len(), 2);
        assert!(plant.nodes.iter().all(|n| n.fixed));
        let fixed = plant.nodes.clone();
        for _ in 0..5 {
            append(&mut plant);
        }
        let before = plant.nodes.last().unwrap().position;
        settle(&mut plant);
        assert_eq!(&plant.nodes[..fixed.len()], &fixed);
        assert!(plant.nodes.last().unwrap().position.y < before.y - 0.05);
    }
    #[test]
    fn unavailable_or_stale_pose_queries_do_not_mutate_history_rng_or_attachments() {
        struct Unavailable(bool);
        impl Terrain for Unavailable {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                if c.x >= 26 {
                    None
                } else {
                    Wall.voxel(c)
                }
            }
            fn current(&self) -> bool {
                self.0
            }
        }
        for current in [true, false] {
            let mut plant = shoot();
            let before = plant.clone();
            assert_eq!(
                plant.relax_shoot(&Unavailable(current), 0.05, 1.0, 16.0),
                None
            );
            assert_eq!(plant, before);
        }
    }
    #[test]
    fn sagging_stops_at_obstacles_and_never_reanimates_a_retained_cut() {
        struct Obstacle;
        impl Terrain for Obstacle {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                Some(u8::from(c.z == 0 || (c.x >= 23 && c.y <= 2 && c.z <= 3)))
            }
            fn current(&self) -> bool {
                true
            }
        }
        let mut plant = shoot();
        for _ in 0..60 {
            plant.relax_shoot(&Obstacle, 0.05, 2.0, 16.0).unwrap();
        }
        for node in &plant.nodes[1..] {
            assert_eq!(
                clear_segment(
                    &Obstacle,
                    plant.nodes[node.parent.unwrap()].position,
                    node.position,
                    plant.radius
                ),
                Some(true)
            );
        }
        // A supported endpoint is removed: the retained stump is a new stable growth base.
        let tip = plant.tips[0].node;
        plant.nodes[tip].contact = Some(Contact {
            cell: IVec3::new(99, 99, 0),
            normal: Vec3::Z,
        });
        let mut cut = vec![false; plant.nodes.len()];
        cut[tip] = true;
        plant.prune_marked(&cut);
        let stump = plant.clone();
        settle(&mut plant);
        assert_eq!(plant, stump);
        assert!(plant.nodes.iter().all(|n| n.fixed));
    }
}
