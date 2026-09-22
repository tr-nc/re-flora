//! Persistent, bounded climbing skeleton. Coordinates and lengths are terrain voxels.
//! Terrain queries are transactional: unknown or stale data never commits growth or motion.
use glam::{IVec3, Vec3};

pub trait Terrain {
    /// None is unavailable, not empty. The caller supplies one immutable source snapshot.
    fn voxel(&self, cell: IVec3) -> Option<u8>;
    fn current(&self) -> bool;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub position: Vec3,
    pub parent: Option<usize>,
    pub rest_length: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    pub node: usize,
    pub cell: IVec3,
    pub material: u8,
    pub position: Vec3,
    pub attached: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Tip {
    pub node: usize,
    direction: Vec3,
    lateral: f32,
    arc: f32,
    spacing: f32,
    rng: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    pub nodes: Vec<Node>,
    pub anchors: Vec<Anchor>,
    pub tips: Vec<Tip>,
    pub normal: Vec3,
    pub radius: f32,
    root_connected: bool,
}

fn random(state: &mut u64) -> f32 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*state >> 40) as f32 / (1u32 << 24) as f32
}

impl Plant {
    pub fn seed(position: Vec3, normal: Vec3, cell: IVec3, material: u8, seed: u64) -> Self {
        Self {
            nodes: vec![Node {
                position,
                parent: None,
                rest_length: 0.0,
            }],
            anchors: vec![Anchor {
                node: 0,
                cell,
                material,
                position,
                attached: true,
            }],
            tips: vec![Tip {
                node: 0,
                direction: Vec3::Y,
                lateral: 0.0,
                arc: 0.0,
                spacing: 16.0,
                rng: seed,
            }],
            normal,
            radius: 0.65,
            root_connected: true,
        }
    }

    /// Disconnect nutrient/root restraint without deleting any stem or wall attachment.
    /// This slice has one connected skeleton; all of its tips lose root connectivity.
    pub fn disconnect_root(&mut self) {
        self.root_connected = false;
    }

    pub fn root_connected(&self) -> bool {
        self.root_connected
    }

    /// One fixed growth quantum.
    /// IDs are append-only indices, including released anchors.
    /// A flat wall is deliberately the first surface contract: stop at corners/tops/holes.
    pub fn grow(&mut self, terrain: &impl Terrain, spacing: f32) -> bool {
        if !self.root_connected || !spacing.is_finite() || spacing < 4.0 || self.nodes.len() >= 512
        {
            return false;
        }
        let mut next = self.clone();
        let mut changed = false;
        for index in 0..self.tips.len() {
            if next.nodes.len() >= 512 {
                break;
            }
            let tip = &mut next.tips[index];
            if tip.node == 0 {
                tip.spacing = spacing;
            }
            let start = next.nodes[tip.node].position;
            let side = Vec3::Y.cross(next.normal).normalize_or_zero();
            let noise = (random(&mut tip.rng) - 0.5) * 0.45;
            let direction =
                (tip.direction * 0.65 + Vec3::Y * 0.35 + side * (noise + tip.lateral * 0.35))
                    .normalize_or_zero();
            let end = start + direction * 2.0;
            let cell = (end - next.normal * (next.radius + 1.1)).floor().as_ivec3();
            let Some(material) = terrain.voxel(cell) else {
                return false;
            };
            // Adhesion requires the original seed material; proximity is independent of adhesion.
            if material == 0 {
                continue;
            }
            match clear_segment(terrain, start, end, next.radius) {
                None => return false,
                Some(false) => continue,
                Some(true) => {}
            }
            let parent = tip.node;
            tip.node = next.nodes.len();
            tip.direction = direction;
            tip.arc += start.distance(end);
            next.nodes.push(Node {
                position: end,
                parent: Some(parent),
                rest_length: start.distance(end),
            });
            if tip.arc >= tip.spacing && material == next.anchors[0].material {
                next.anchors.push(Anchor {
                    node: tip.node,
                    cell,
                    material,
                    position: end,
                    attached: true,
                });
                tip.arc = 0.0;
                tip.spacing = spacing * (0.85 + 0.3 * random(&mut tip.rng));
            }
            changed = true;
        }
        if changed && next.tips.len() < 4 && next.nodes.len() / 24 > self.nodes.len() / 24 {
            let mut branch = next.tips[0].clone();
            branch.direction = (Vec3::Y
                + Vec3::Y.cross(next.normal) * if next.tips.len() % 2 == 0 { -0.8 } else { 0.8 })
            .normalize();
            branch.lateral = if next.tips.len() % 2 == 0 { -0.5 } else { 0.5 };
            branch.arc = 0.0;
            branch.spacing = spacing;
            branch.rng = branch.rng.wrapping_add(next.nodes.len() as u64);
            next.tips.push(branch);
        }
        if changed && terrain.current() {
            *self = next;
            true
        } else {
            false
        }
    }

    /// Revalidate fixed contact cells; material replacement releases adhesion, never topology.
    /// Pending/stale snapshots leave even the RNG untouched.
    pub fn revalidate(
        &mut self,
        terrain: &impl Terrain,
        candidates: impl IntoIterator<Item = usize>,
    ) -> bool {
        let mut released = Vec::new();
        for id in candidates {
            let anchor = &self.anchors[id];
            if !anchor.attached {
                continue;
            }
            let Some(material) = terrain.voxel(anchor.cell) else {
                return false;
            };
            if material != anchor.material {
                released.push(id);
            }
        }
        if !terrain.current() {
            return false;
        }
        for id in &released {
            self.anchors[*id].attached = false;
        }
        !released.is_empty()
    }

    /// Quasi-static distance-constrained relaxation. No invented slack or render-only sag.
    /// Reject the entire step if constraints or swept shell collision cannot be satisfied.
    pub fn relax(&mut self, terrain: &impl Terrain) -> bool {
        let mut positions: Vec<_> = self.nodes.iter().map(|n| n.position).collect();
        let mut pinned = vec![false; positions.len()];
        // Root connectivity and external wall adhesion are independent restraints.
        pinned[0] = self.root_connected;
        for anchor in self.anchors.iter().filter(|a| a.attached) {
            pinned[anchor.node] = true;
        }
        for (p, pin) in positions.iter_mut().zip(&pinned) {
            if !pin {
                *p -= Vec3::Y * 0.08;
            }
        }
        for _ in 0..128 {
            for (id, node) in self.nodes.iter().enumerate().skip(1) {
                let parent = node.parent.unwrap();
                let delta = positions[id] - positions[parent];
                let length = delta.length();
                if length < 1e-6 {
                    continue;
                }
                let weights = usize::from(!pinned[id]) + usize::from(!pinned[parent]);
                if weights == 0 {
                    continue;
                }
                let correction = delta * ((length - node.rest_length) / (length * weights as f32));
                if !pinned[id] {
                    positions[id] -= correction;
                }
                if !pinned[parent] {
                    positions[parent] += correction;
                }
            }
        }
        for (id, node) in self.nodes.iter().enumerate() {
            if !positions[id].is_finite()
                || clear_segment(terrain, node.position, positions[id], self.radius) != Some(true)
            {
                return false;
            }
            if let Some(parent) = node.parent {
                if (positions[id].distance(positions[parent]) - node.rest_length).abs() > 0.002
                    || clear_segment(terrain, positions[parent], positions[id], self.radius)
                        != Some(true)
                {
                    return false;
                }
            }
        }
        if !terrain.current() {
            return false;
        }
        let changed = self
            .nodes
            .iter()
            .zip(&positions)
            .any(|(n, p)| n.position.distance_squared(*p) > 1e-10);
        for (node, position) in self.nodes.iter_mut().zip(positions) {
            node.position = position;
        }
        changed
    }
}

/// Exact segment slab test against radius-expanded voxel boxes (conservative capsule).
pub fn clear_segment(terrain: &impl Terrain, start: Vec3, end: Vec3, radius: f32) -> Option<bool> {
    let min = (start.min(end) - Vec3::splat(radius)).floor().as_ivec3();
    let max = (start.max(end) + Vec3::splat(radius)).floor().as_ivec3();
    for z in min.z..=max.z {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let cell = IVec3::new(x, y, z);
                if terrain.voxel(cell)? == 0 {
                    continue;
                }
                let lo = cell.as_vec3() - Vec3::splat(radius);
                let hi = cell.as_vec3() + Vec3::splat(1.0 + radius);
                let delta = end - start;
                let mut enter: f32 = 0.0;
                let mut exit: f32 = 1.0;
                for axis in 0..3 {
                    if delta[axis].abs() < 1e-8 {
                        if start[axis] < lo[axis] || start[axis] > hi[axis] {
                            enter = 2.0;
                            break;
                        }
                    } else {
                        let a = (lo[axis] - start[axis]) / delta[axis];
                        let b = (hi[axis] - start[axis]) / delta[axis];
                        enter = enter.max(a.min(b));
                        exit = exit.min(a.max(b));
                    }
                }
                if enter <= exit {
                    return Some(false);
                }
            }
        }
    }
    Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    #[derive(Default)]
    struct Wall {
        missing: HashSet<IVec3>,
        pending: bool,
        stale: bool,
    }
    impl Terrain for Wall {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            if self.pending {
                None
            } else {
                Some(u8::from(c.z == 0 && !self.missing.contains(&c)))
            }
        }
        fn current(&self) -> bool {
            !self.stale
        }
    }
    fn seed() -> Plant {
        Plant::seed(
            Vec3::new(20.5, 4.5, 1.8),
            Vec3::Z,
            IVec3::new(20, 4, 0),
            1,
            42,
        )
    }
    #[test]
    fn deterministic_spacing_ids_and_finite_lengths() {
        let mut a = seed();
        let mut b = seed();
        let wall = Wall::default();
        for _ in 0..100 {
            a.grow(&wall, 16.0);
            b.grow(&wall, 16.0);
        }
        assert_eq!(a, b);
        assert!(a.nodes.len() > 100);
        assert!(a.tips.len() > 1);
        for n in &a.nodes {
            assert!(n.position.is_finite());
            if let Some(p) = n.parent {
                assert!((n.position.distance(a.nodes[p].position) - n.rest_length).abs() < 1e-5);
            }
        }
        let main: Vec<_> = a.anchors.iter().filter(|a| a.node < 24).collect();
        for pair in main.windows(2) {
            assert!((6..=10).contains(&(pair[1].node - pair[0].node)));
        }
    }
    #[test]
    fn releases_preserve_topology_other_supports_and_do_not_resurrect() {
        let mut p = seed();
        let mut wall = Wall::default();
        for _ in 0..40 {
            p.grow(&wall, 16.0);
        }
        let nodes = p.nodes.clone();
        let ids: Vec<_> = p.anchors.iter().map(|a| a.node).collect();
        wall.missing.insert(IVec3::new(250, 250, 0));
        assert!(!p.revalidate(&wall, 0..p.anchors.len()));
        wall.missing.insert(p.anchors[1].cell);
        assert!(p.revalidate(&wall, 0..p.anchors.len()));
        assert!(p.anchors[2].attached);
        wall.missing.insert(p.anchors[2].cell);
        assert!(p.revalidate(&wall, 0..p.anchors.len()));
        assert_eq!(p.nodes, nodes);
        assert_eq!(ids, p.anchors.iter().map(|a| a.node).collect::<Vec<_>>());
        wall.missing.clear();
        p.revalidate(&wall, 0..p.anchors.len());
        assert!(!p.anchors[1].attached);
    }
    #[test]
    fn pending_and_stale_are_transactional() {
        let original = seed();
        for wall in [
            Wall {
                pending: true,
                ..Default::default()
            },
            Wall {
                stale: true,
                ..Default::default()
            },
        ] {
            let mut p = original.clone();
            p.grow(&wall, 16.0);
            p.revalidate(&wall, 0..p.anchors.len());
            p.relax(&wall);
            assert_eq!(p, original);
        }
    }
    #[test]
    fn thin_wall_and_segment_interior_are_not_crossed() {
        let wall = Wall::default();
        assert_eq!(
            clear_segment(&wall, Vec3::new(1., 1., -4.), Vec3::new(1., 1., 4.), 0.65),
            Some(false)
        );
        assert_eq!(
            clear_segment(&wall, Vec3::new(1., 1., 2.), Vec3::new(1., 100., 2.), 0.65),
            Some(true)
        );
    }
    #[test]
    fn released_free_tip_sags_without_stretching() {
        let mut p = seed();
        let wall = Wall::default();
        for _ in 0..20 {
            p.grow(&wall, 16.0);
        }
        for a in p.anchors.iter_mut().skip(1) {
            a.attached = false;
        }
        let before = p.nodes.last().unwrap().position.y;
        for _ in 0..100 {
            p.relax(&wall);
        }
        assert!(p.nodes.last().unwrap().position.y < before);
        for n in &p.nodes {
            if let Some(parent) = n.parent {
                assert!(
                    (n.position.distance(p.nodes[parent].position) - n.rest_length).abs() < 0.0021
                );
            }
        }
    }
    #[test]
    fn material_replacement_releases_adhesion_not_collision_or_topology() {
        struct Replacement;
        impl Terrain for Replacement {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                Some(if c.z == 0 { 2 } else { 0 })
            }
            fn current(&self) -> bool {
                true
            }
        }
        let mut p = seed();
        let wall = Wall::default();
        for _ in 0..18 {
            p.grow(&wall, 16.);
        }
        let nodes = p.nodes.clone();
        assert!(p.revalidate(&Replacement, 0..p.anchors.len()));
        assert_eq!(nodes, p.nodes);
        assert!(p.anchors.iter().all(|a| !a.attached));
        assert_eq!(
            clear_segment(
                &Replacement,
                Vec3::new(20., 10., -1.),
                Vec3::new(20., 10., 2.),
                0.65
            ),
            Some(false)
        );
    }

    #[test]
    fn taut_pinned_span_does_not_acquire_rest_length() {
        let mut p = seed();
        p.nodes = (0..9)
            .map(|id| Node {
                position: Vec3::new(20.5, 4.5 + id as f32 * 2., 1.8),
                parent: if id == 0 { None } else { Some(id - 1) },
                rest_length: if id == 0 { 0. } else { 2. },
            })
            .collect();
        p.anchors.push(Anchor {
            node: 8,
            cell: IVec3::new(20, 20, 0),
            material: 1,
            position: p.nodes[8].position,
            attached: true,
        });
        let lengths: Vec<_> = p.nodes.iter().map(|n| n.rest_length).collect();
        let before = p.nodes.clone();
        for _ in 0..200 {
            p.relax(&Wall::default());
        }
        assert_eq!(
            lengths,
            p.nodes.iter().map(|n| n.rest_length).collect::<Vec<_>>()
        );
        assert_eq!(p.nodes[0], before[0]);
        assert_eq!(p.nodes[8], before[8]);
        assert!(p
            .nodes
            .iter()
            .zip(before)
            .all(|(a, b)| a.position.distance(b.position) < 0.01));
    }

    #[test]
    fn support_gap_stops_growth_and_preserves_rng() {
        let mut p = seed();
        let mut wall = Wall::default();
        for x in -100..100 {
            for y in -100..100 {
                wall.missing.insert(IVec3::new(x, y, 0));
            }
        }
        let before = p.clone();
        for _ in 0..100 {
            assert!(!p.grow(&wall, 16.));
        }
        assert_eq!(before, p);
    }
    #[test]
    fn all_supports_removed_can_settle_after_root_disconnection() {
        let mut p = seed();
        let wall = Wall::default();
        for _ in 0..18 {
            p.grow(&wall, 16.);
        }
        for anchor in &mut p.anchors {
            anchor.attached = false;
        }
        p.disconnect_root();
        let root = p.nodes[0].position;
        for _ in 0..20 {
            p.relax(&wall);
        }
        assert!(
            p.nodes[0].position.y < root.y - 1.0,
            "unsupported root remains pinned"
        );
    }
    #[test]
    fn cut_root_stops_growth_but_retains_wall_attachments_and_ids() {
        let mut p = seed();
        let wall = Wall::default();
        for _ in 0..18 {
            p.grow(&wall, 16.);
        }
        let nodes = p.nodes.clone();
        let anchors = p.anchors.clone();
        let tips = p.tips.clone();
        p.disconnect_root();
        assert!(!p.root_connected());
        for _ in 0..30 {
            assert!(!p.grow(&wall, 16.));
            p.relax(&wall);
        }
        assert_eq!(p.anchors, anchors);
        assert_eq!(p.tips, tips);
        assert_eq!(p.nodes.len(), nodes.len());
        for a in &p.anchors {
            assert_eq!(p.nodes[a.node].position, a.position);
        }
        for (a, b) in p.nodes.iter().zip(nodes) {
            assert_eq!((a.parent, a.rest_length), (b.parent, b.rest_length));
        }
    }
}
