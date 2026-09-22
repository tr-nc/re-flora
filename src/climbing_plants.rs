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
            let Some(exposed) =
                clear_segment(terrain, anchor.position, anchor.position, self.radius)
            else {
                return false;
            };
            if material != anchor.material || !exposed {
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

    /// Quasi-static distance projection. Pinned supports partition independent movable spans;
    /// an infeasible span never prevents another span from settling. Pending/stale queries
    /// cannot commit motion. Newly inserted terrain is recovered toward the known exterior.
    pub fn relax(&mut self, terrain: &impl Terrain) -> bool {
        let original: Vec<_> = self.nodes.iter().map(|n| n.position).collect();
        let mut positions = original.clone();
        let mut pinned = vec![false; positions.len()];
        pinned[0] = self.root_connected;
        for anchor in self.anchors.iter().filter(|a| a.attached) {
            pinned[anchor.node] = true;
        }
        // Topology is an append-only tree: a movable child inherits its movable parent's
        // span; a pin is a mechanical boundary shared by otherwise independent spans.
        let mut membership = vec![None; positions.len()];
        let mut spans: Vec<(Vec<usize>, Vec<usize>)> = Vec::new();
        for (id, node) in self.nodes.iter().enumerate() {
            if !pinned[id] {
                let span = node.parent.and_then(|p| membership[p]).unwrap_or_else(|| {
                    spans.push((Vec::new(), Vec::new()));
                    spans.len() - 1
                });
                membership[id] = Some(span);
                spans[span].0.push(id);
            }
            if let Some(parent) = node.parent {
                if let Some(span) = membership[id].or(membership[parent]) {
                    spans[span].1.push(id);
                }
            }
        }
        for (nodes, edges) in spans {
            if let Some(candidate) = self.relax_span(terrain, &original, &pinned, &nodes, &edges) {
                for id in nodes {
                    positions[id] = candidate[id];
                }
            }
        }
        if !terrain.current() {
            return false;
        }
        let changed = original
            .iter()
            .zip(&positions)
            .any(|(a, b)| a.distance_squared(*b) > 1e-10);
        for (node, position) in self.nodes.iter_mut().zip(positions) {
            node.position = position;
        }
        changed
    }

    fn relax_span(
        &self,
        terrain: &impl Terrain,
        original: &[Vec3],
        pinned: &[bool],
        nodes: &[usize],
        edges: &[usize],
    ) -> Option<Vec<Vec3>> {
        let mut positions = original.to_vec();
        // These constraints are temporary collision contacts, not new adhesion. Preserve the
        // original outward exit plane throughout projection, rather than finding "air" behind
        // a sparse terrain shell or accepting an arbitrary teleport to its opposite side.
        let mut recovery = Vec::new();
        for &id in edges {
            let parent = self.nodes[id].parent?;
            for contact in segment_contacts(terrain, original[parent], original[id], self.radius)? {
                recovery.push((
                    parent,
                    id,
                    (contact.enter + contact.exit) * 0.5,
                    contact.exit_plane(self.normal),
                ));
            }
        }
        // Also handle a single disconnected root with no edges.
        for &id in nodes {
            for contact in segment_contacts(terrain, original[id], original[id], self.radius)? {
                recovery.push((id, id, 0.0, contact.exit_plane(self.normal)));
            }
        }
        if recovery.is_empty() {
            for &id in nodes {
                positions[id] -= Vec3::Y * 0.08;
            }
        }
        let iterations = if recovery.is_empty() { 128 } else { 512 };
        for _ in 0..iterations {
            for &id in edges {
                let parent = self.nodes[id].parent?;
                let delta = positions[id] - positions[parent];
                let length = delta.length();
                if length < 1e-6 {
                    continue;
                }
                let weights = usize::from(!pinned[id]) + usize::from(!pinned[parent]);
                if weights == 0 {
                    continue;
                }
                let correction =
                    delta * ((length - self.nodes[id].rest_length) / (length * weights as f32));
                if !pinned[id] {
                    positions[id] -= correction;
                }
                if !pinned[parent] {
                    positions[parent] += correction;
                }
            }
            for &(a, b, t, plane) in &recovery {
                project_contact(&mut positions, pinned, a, b, t, self.normal, plane);
            }
            if nodes
                .iter()
                .any(|&id| !positions[id].is_finite() || positions[id].distance(original[id]) > 6.0)
            {
                return None;
            }
            for &id in edges {
                let parent = self.nodes[id].parent?;
                for contact in
                    segment_contacts(terrain, positions[parent], positions[id], self.radius)?
                {
                    let normal = if recovery.is_empty() {
                        contact.separating_normal(original[parent], original[id], self.normal)
                    } else {
                        self.normal
                    };
                    project_contact(
                        &mut positions,
                        pinned,
                        parent,
                        id,
                        (contact.enter + contact.exit) * 0.5,
                        normal,
                        contact.exit_plane(normal),
                    );
                }
            }
        }
        for &id in nodes {
            if !positions[id].is_finite() || positions[id].distance(original[id]) > 6.0 {
                return None;
            }
            // Recovery exits outward first, then follows the tangential correction. A single
            // diagonal chord can falsely collide with neighbouring cells of the inserted patch.
            // Never exempt a box that did not already contain the old particle.
            let lift = if recovery.is_empty() {
                original[id]
            } else {
                original[id]
                    + self.normal * (positions[id] - original[id]).dot(self.normal).max(0.0)
            };
            for contact in segment_contacts(terrain, original[id], lift, self.radius)? {
                if !contact.contains(original[id])
                    || lift.dot(self.normal) < contact.exit_plane(self.normal) - 0.0001
                {
                    return None;
                }
            }
            if !segment_contacts(terrain, lift, positions[id], self.radius)?.is_empty() {
                return None;
            }
        }
        for &id in edges {
            let parent = self.nodes[id].parent?;
            if (positions[id].distance(positions[parent]) - self.nodes[id].rest_length).abs()
                > 0.002
                || !segment_contacts(terrain, positions[parent], positions[id], self.radius)?
                    .is_empty()
            {
                return None;
            }
        }
        // The no-edge case must also be clear after recovery.
        for &id in nodes {
            if !segment_contacts(terrain, positions[id], positions[id], self.radius)?.is_empty() {
                return None;
            }
        }
        Some(positions)
    }
}

fn project_contact(
    positions: &mut [Vec3],
    pinned: &[bool],
    a: usize,
    b: usize,
    t: f32,
    normal: Vec3,
    plane: f32,
) {
    let depth = plane - positions[a].lerp(positions[b], t).dot(normal);
    if depth <= 0.0 {
        return;
    }
    if a == b {
        if !pinned[a] {
            positions[a] += normal * depth;
        }
        return;
    }
    let wa = if pinned[a] { 0.0 } else { 1.0 - t };
    let wb = if pinned[b] { 0.0 } else { t };
    let denominator = wa * wa + wb * wb;
    if denominator > 1e-8 {
        positions[a] += normal * (depth * wa / denominator);
        positions[b] += normal * (depth * wb / denominator);
    }
}

#[derive(Clone, Copy)]
struct SolidContact {
    lo: Vec3,
    hi: Vec3,
    enter: f32,
    exit: f32,
}
impl SolidContact {
    fn contains(self, p: Vec3) -> bool {
        p.cmpge(self.lo).all() && p.cmple(self.hi).all()
    }
    fn separating_normal(self, a: Vec3, b: Vec3, fallback: Vec3) -> Vec3 {
        let mut best = f32::INFINITY;
        let mut normal = fallback;
        for axis in 0..3 {
            for (distance, sign) in [
                (a[axis].min(b[axis]) - self.hi[axis], 1.0),
                (self.lo[axis] - a[axis].max(b[axis]), -1.0),
            ] {
                if distance >= 0.0 && distance < best {
                    best = distance;
                    normal = Vec3::ZERO;
                    normal[axis] = sign;
                }
            }
        }
        normal
    }
    fn exit_plane(self, normal: Vec3) -> f32 {
        self.hi.dot(normal.max(Vec3::ZERO)) + self.lo.dot(normal.min(Vec3::ZERO)) + 0.001
    }
}

/// Exact segment slab test against radius-expanded voxel boxes (conservative capsule).
pub fn clear_segment(terrain: &impl Terrain, start: Vec3, end: Vec3, radius: f32) -> Option<bool> {
    Some(segment_contacts(terrain, start, end, radius)?.is_empty())
}
fn segment_contacts(
    terrain: &impl Terrain,
    start: Vec3,
    end: Vec3,
    radius: f32,
) -> Option<Vec<SolidContact>> {
    let min = (start.min(end) - Vec3::splat(radius)).floor().as_ivec3();
    let max = (start.max(end) + Vec3::splat(radius)).floor().as_ivec3();
    let mut contacts = Vec::new();
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
                    contacts.push(SolidContact {
                        lo,
                        hi,
                        enter,
                        exit,
                    });
                }
            }
        }
    }
    Some(contacts)
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
    #[test]
    fn refill_then_support_removal_recovers_without_freezing_other_spans() {
        struct Refilled {
            wall: Wall,
        }
        impl Terrain for Refilled {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                if (17..=24).contains(&c.x) && (17..=23).contains(&c.y) && (1..=2).contains(&c.z) {
                    Some(1)
                } else {
                    self.wall.voxel(c)
                }
            }
            fn current(&self) -> bool {
                true
            }
        }
        let wall = Wall::default();
        let mut p = seed();
        for _ in 0..10 {
            p.grow(&wall, 16.);
        }
        let lengths: Vec<_> = p.nodes.iter().map(|n| n.rest_length).collect();
        let terrain = Refilled { wall };
        assert!(p
            .nodes
            .iter()
            .any(|n| clear_segment(&terrain, n.position, n.position, p.radius) == Some(false)));
        for a in p.anchors.iter_mut().skip(1) {
            a.attached = false;
        }
        let mut replay = p.clone();
        for _ in 0..80 {
            p.relax(&terrain);
            replay.relax(&terrain);
        }
        assert_eq!(p, replay);
        assert!(
            p.nodes
                .iter()
                .all(|n| n.parent.is_none_or(|parent| clear_segment(
                    &terrain,
                    p.nodes[parent].position,
                    n.position,
                    p.radius
                ) == Some(true))),
            "refill overlap never recovered"
        );
        assert_eq!(
            lengths,
            p.nodes.iter().map(|n| n.rest_length).collect::<Vec<_>>()
        );
        for n in &p.nodes {
            if let Some(parent) = n.parent {
                assert!(
                    (n.position.distance(p.nodes[parent].position) - n.rest_length).abs() < 0.0021
                );
            }
        }
        let before = p.nodes.last().unwrap().position;
        for _ in 0..20 {
            p.relax(&terrain);
        }
        assert!(
            p.nodes.last().unwrap().position.distance(before) > 0.1,
            "settling remained frozen after recovery"
        );
    }
    #[test]
    fn unsatisfiable_local_refill_does_not_freeze_another_supported_span() {
        struct Obstacle;
        impl Terrain for Obstacle {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                Some(u8::from(c.z == 0 || c == IVec3::new(22, 4, 1)))
            }
            fn current(&self) -> bool {
                true
            }
        }
        let mut p = seed();
        p.nodes = (0..4)
            .map(|id| Node {
                position: Vec3::new(20.5 + 2.0 * id as f32, 4.5, 1.8),
                parent: if id == 0 { None } else { Some(id - 1) },
                rest_length: if id == 0 { 0. } else { 2. },
            })
            .collect();
        p.anchors.push(Anchor {
            node: 2,
            cell: IVec3::new(24, 4, 0),
            material: 1,
            position: p.nodes[2].position,
            attached: true,
        });
        let original = p.nodes.clone();
        for _ in 0..10 {
            p.relax(&Obstacle);
        }
        // No feasible path around the block for a perfectly taut pinned span: leave just that
        // span unchanged, but independently relax the free span beyond the surviving anchor.
        assert_eq!(p.nodes[..3], original[..3]);
        assert!(p.nodes[3].position.y < original[3].position.y - 0.5);
        assert!((p.nodes[3].position.distance(p.nodes[2].position) - 2.0).abs() < 0.0021);
    }

    #[test]
    fn buried_contact_releases_without_rebinding_and_pending_recovery_is_transactional() {
        struct Buried {
            pending: bool,
            stale: bool,
        }
        impl Terrain for Buried {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                if self.pending {
                    None
                } else {
                    Some(u8::from(c.z == 0 || c == IVec3::new(20, 4, 1)))
                }
            }
            fn current(&self) -> bool {
                !self.stale
            }
        }
        let mut p = seed();
        let original = p.clone();
        for terrain in [
            Buried {
                pending: true,
                stale: false,
            },
            Buried {
                pending: false,
                stale: true,
            },
        ] {
            p.revalidate(&terrain, 0..p.anchors.len());
            p.relax(&terrain);
            assert_eq!(p, original);
        }
        assert!(p.revalidate(
            &Buried {
                pending: false,
                stale: false
            },
            0..p.anchors.len()
        ));
        assert!(!p.anchors[0].attached);
        assert!(p.root_connected());
        assert_eq!(p.nodes, original.nodes);
        assert_eq!(p.anchors[0].position, original.anchors[0].position);
    }
    #[test]
    fn completely_detached_skeleton_settles_on_solid_floor_without_stretch() {
        struct Floor;
        impl Terrain for Floor {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                Some(u8::from(c.z == 0 || c.y == 0))
            }
            fn current(&self) -> bool {
                true
            }
        }
        let mut p = seed();
        for _ in 0..10 {
            p.grow(&Floor, 16.);
        }
        p.disconnect_root();
        for a in &mut p.anchors {
            a.attached = false;
        }
        let before = p.nodes.clone();
        for _ in 0..200 {
            p.relax(&Floor);
        }
        assert!(p.nodes[0].position.y < before[0].position.y - 1.);
        for n in &p.nodes {
            assert!(n.position.is_finite() && n.position.y >= 1.65);
            if let Some(parent) = n.parent {
                assert!(
                    (n.position.distance(p.nodes[parent].position) - n.rest_length).abs() < 0.0021
                );
                assert_eq!(
                    clear_segment(&Floor, p.nodes[parent].position, n.position, p.radius),
                    Some(true)
                );
            }
        }
    }
}
