//! Rooted wall vines: loss of support prunes the downstream subtree and leaves a regrowth bud.
//! Unknown/stale terrain never commits pruning or growth. Coordinates are terrain voxels.
use glam::{IVec3, Vec3};

const MAX_NODES: usize = 512;
const MAX_TIPS: usize = 4;

pub trait Terrain {
    /// None is unavailable, not empty. Queries belong to one immutable snapshot.
    fn voxel(&self, cell: IVec3) -> Option<u8>;
    fn current(&self) -> bool;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Stable identity, independent of the compact storage index in `parent`.
    pub id: u64,
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
    // The first severed step is retried exactly, rather than picking a new direction
    // every frame and eventually jumping past the missing wall.
    restart: Option<Vec3>,
}
impl Tip {
    fn seed(node: usize, rng: u64) -> Self {
        Self {
            node,
            direction: Vec3::Y,
            lateral: 0.0,
            arc: 0.0,
            spacing: 16.0,
            rng,
            restart: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    pub nodes: Vec<Node>,
    pub anchors: Vec<Anchor>,
    pub tips: Vec<Tip>,
    pub normal: Vec3,
    pub radius: f32,
    root_connected: bool,
    seed: u64,
    next_node_id: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pruned {
    pub removed: usize,
    pub buds: usize,
}

fn random(state: &mut u64) -> f32 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*state >> 40) as f32 / (1u32 << 24) as f32
}

impl Plant {
    pub fn seed(position: Vec3, normal: Vec3, cell: IVec3, material: u8, seed: u64) -> Self {
        Self {
            nodes: vec![Node {
                id: 0,
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
            tips: vec![Tip::seed(0, seed)],
            normal,
            radius: 0.65,
            root_connected: true,
            seed,
            next_node_id: 1,
        }
    }

    pub fn disconnect_root(&mut self) {
        self.root_connected = false;
    }
    pub fn root_connected(&self) -> bool {
        self.root_connected
    }
    pub fn regrowth_nodes(&self) -> impl Iterator<Item = &Node> {
        self.tips
            .iter()
            .filter(|tip| tip.restart.is_some())
            .map(|tip| &self.nodes[tip.node])
    }

    /// Each stem step needs a continuous backing surface, not just sparse adhesion markers.
    /// This also detects edits between two anchors or between two sampled stem nodes.
    fn supported_segment(&self, terrain: &impl Terrain, start: Vec3, end: Vec3) -> Option<bool> {
        let offset = self.normal * (self.radius + 1.1);
        segment_material(
            terrain,
            start - offset,
            end - offset,
            0.0,
            self.anchors[0].material,
        )
    }

    fn root_supported(&self, terrain: &impl Terrain) -> Option<bool> {
        let root = &self.anchors[0];
        let material = terrain.voxel(root.cell)?;
        let exposed = clear_segment(terrain, root.position, root.position, self.radius)?;
        Some(material == root.material && exposed)
    }

    /// First invalid step on each root-to-tip path cuts that whole downstream subtree,
    /// even when higher anchors are still valid. The root seed is retained for repair.
    /// None means no mutation was committed; callers must keep the revalidation pending.
    pub fn revalidate(&mut self, terrain: &impl Terrain) -> Option<Pruned> {
        let mut cut = vec![false; self.nodes.len()];
        let root_supported = self.root_supported(terrain)?;
        cut[0] = !root_supported;
        for (id, node) in self.nodes.iter().enumerate().skip(1) {
            let parent = node.parent.expect("non-root stem has a parent");
            if cut[parent] {
                cut[id] = true;
                continue;
            }
            let start = self.nodes[parent].position;
            let supported = self.supported_segment(terrain, start, node.position)?;
            let exposed = clear_segment(terrain, start, node.position, self.radius)?;
            cut[id] = !supported || !exposed;
        }
        if !terrain.current() {
            return None;
        }
        let result = self.prune_marked(&cut);
        self.anchors[0].attached = root_supported;
        Some(result)
    }

    pub fn prune_highest_attachment(&mut self) -> Option<Pruned> {
        let anchor = self
            .anchors
            .iter()
            .filter(|a| a.attached && a.node != 0)
            .max_by(|a, b| {
                a.position
                    .y
                    .total_cmp(&b.position.y)
                    .then(a.node.cmp(&b.node))
            })?;
        let mut cut = vec![false; self.nodes.len()];
        cut[anchor.node] = true;
        Some(self.prune_marked(&cut))
    }

    pub fn prune_to_root(&mut self) -> Pruned {
        self.prune_marked(&vec![true; self.nodes.len()])
    }

    fn is_descendant(&self, mut node: usize, ancestor: usize) -> bool {
        loop {
            if node == ancestor {
                return true;
            }
            let Some(parent) = self.nodes[node].parent else {
                return false;
            };
            node = parent;
        }
    }

    /// Compact live storage to reclaim the 512-node budget. IDs and retained geometry
    /// stay unchanged; all internal node indices are remapped in one transaction.
    fn prune_marked(&mut self, marked: &[bool]) -> Pruned {
        let mut cut = marked.to_vec();
        for (id, node) in self.nodes.iter().enumerate().skip(1) {
            cut[id] |= cut[node.parent.unwrap()];
        }
        cut[0] = false; // a latent root seed survives even loss of the entire backing wall
        let removed = cut.iter().filter(|&&cut| cut).count();
        if removed == 0 {
            return Pruned::default();
        }

        let mut tips: Vec<_> = self
            .tips
            .iter()
            .filter(|tip| !cut[tip.node])
            .cloned()
            .collect();
        let mut buds = 0;
        for (id, node) in self.nodes.iter().enumerate().skip(1) {
            let parent = node.parent.unwrap();
            if !cut[id] || cut[parent] {
                continue;
            }
            let mut bud = self
                .tips
                .iter()
                .find(|tip| self.is_descendant(tip.node, id))
                .cloned()
                .unwrap_or_else(|| Tip::seed(parent, self.seed.wrapping_add(node.id)));
            bud.node = parent;
            bud.direction = (node.position - self.nodes[parent].position).normalize_or_zero();
            bud.arc = 0.0;
            bud.restart = Some(node.position);
            tips.push(bud);
            buds += 1;
        }
        // Every removed frontier replaces at least one old leaf tip in the bounded tree.
        debug_assert!(tips.len() <= MAX_TIPS);
        let mut remap = vec![None; self.nodes.len()];
        let mut nodes = Vec::with_capacity(self.nodes.len() - removed);
        for (old, node) in self.nodes.iter().enumerate() {
            if cut[old] {
                continue;
            }
            let mut node = node.clone();
            node.parent = node.parent.map(|p| remap[p].unwrap());
            remap[old] = Some(nodes.len());
            nodes.push(node);
        }
        self.anchors.retain(|anchor| !cut[anchor.node]);
        for anchor in &mut self.anchors {
            anchor.node = remap[anchor.node].unwrap();
        }
        for tip in &mut tips {
            tip.node = remap[tip.node].unwrap();
        }
        self.nodes = nodes;
        self.tips = tips;
        Pruned { removed, buds }
    }

    /// One fixed growth quantum. Paused/blocked tips retain their RNG, including when
    /// a different tip succeeds. A restart bud retries the exact cut step after repair.
    pub fn grow(&mut self, terrain: &impl Terrain, spacing: f32) -> bool {
        if !self.root_connected
            || !spacing.is_finite()
            || spacing < 4.0
            || self.nodes.len() >= MAX_NODES
            || self.root_supported(terrain) != Some(true)
        {
            return false;
        }
        let mut next = self.clone();
        let mut changed = false;
        let mut branch_source = 0;
        for index in 0..self.tips.len() {
            if next.nodes.len() >= MAX_NODES {
                break;
            }
            let mut tip = next.tips[index].clone();
            if tip.node == 0 || tip.restart.is_some() {
                tip.spacing = spacing;
            }
            let start = next.nodes[tip.node].position;
            let side = Vec3::Y.cross(next.normal).normalize_or_zero();
            let end = if let Some(end) = tip.restart {
                end
            } else {
                let noise = (random(&mut tip.rng) - 0.5) * 0.45;
                let direction =
                    (tip.direction * 0.65 + Vec3::Y * 0.35 + side * (noise + tip.lateral * 0.35))
                        .normalize_or_zero();
                start + direction * 2.0
            };
            match next.supported_segment(terrain, start, end) {
                None => return false,
                Some(false) => continue,
                Some(true) => {}
            }
            match clear_segment(terrain, start, end, next.radius) {
                None => return false,
                Some(false) => continue,
                Some(true) => {}
            }
            let cell = (end - next.normal * (next.radius + 1.1)).floor().as_ivec3();
            let parent = tip.node;
            tip.node = next.nodes.len();
            tip.direction = (end - start).normalize_or_zero();
            tip.arc += start.distance(end);
            tip.restart = None;
            next.nodes.push(Node {
                id: next.next_node_id,
                position: end,
                parent: Some(parent),
                rest_length: start.distance(end),
            });
            next.next_node_id += 1;
            if tip.arc >= tip.spacing {
                next.anchors.push(Anchor {
                    node: tip.node,
                    cell,
                    material: next.anchors[0].material,
                    position: end,
                    attached: true,
                });
                tip.arc = 0.0;
                tip.spacing = spacing * (0.85 + 0.3 * random(&mut tip.rng));
            }
            next.tips[index] = tip;
            if !changed {
                branch_source = index;
            }
            changed = true;
        }
        if changed && next.tips.len() < MAX_TIPS && next.nodes.len() / 24 > self.nodes.len() / 24 {
            let mut branch = next.tips[branch_source].clone();
            branch.direction = (Vec3::Y
                + Vec3::Y.cross(next.normal) * if next.tips.len() % 2 == 0 { -0.8 } else { 0.8 })
            .normalize();
            branch.lateral = if next.tips.len() % 2 == 0 { -0.5 } else { 0.5 };
            branch.arc = 0.0;
            branch.spacing = spacing;
            branch.rng = branch.rng.wrapping_add(next.next_node_id);
            next.tips.push(branch);
        }
        if changed && terrain.current() {
            *self = next;
            true
        } else {
            false
        }
    }
}

/// Exact slab intersection, shared by exposed-stem collision and backing-surface checks.
fn intersects_box(start: Vec3, end: Vec3, lo: Vec3, hi: Vec3) -> bool {
    let delta = end - start;
    let mut enter: f32 = 0.0;
    let mut exit: f32 = 1.0;
    for axis in 0..3 {
        if delta[axis].abs() < 1e-8 {
            if start[axis] < lo[axis] || start[axis] > hi[axis] {
                return false;
            }
        } else {
            let a = (lo[axis] - start[axis]) / delta[axis];
            let b = (hi[axis] - start[axis]) / delta[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
        }
    }
    enter <= exit
}
fn segment_material(
    terrain: &impl Terrain,
    start: Vec3,
    end: Vec3,
    radius: f32,
    material: u8,
) -> Option<bool> {
    let min = (start.min(end) - Vec3::splat(radius)).floor().as_ivec3();
    let max = (start.max(end) + Vec3::splat(radius)).floor().as_ivec3();
    let mut matches = true;
    for z in min.z..=max.z {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let cell = IVec3::new(x, y, z);
                let lo = cell.as_vec3() - Vec3::splat(radius);
                let hi = cell.as_vec3() + Vec3::splat(1.0 + radius);
                if intersects_box(start, end, lo, hi) {
                    matches &= terrain.voxel(cell)? == material;
                }
            }
        }
    }
    Some(matches)
}
/// Conservative capsule against radius-expanded solid voxel boxes.
pub fn clear_segment(terrain: &impl Terrain, start: Vec3, end: Vec3, radius: f32) -> Option<bool> {
    segment_material(terrain, start, end, radius, 0)
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
        fn voxel(&self, cell: IVec3) -> Option<u8> {
            if self.pending {
                None
            } else {
                Some(u8::from(cell.z == 0 && !self.missing.contains(&cell)))
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
    fn grown() -> Plant {
        let mut plant = seed();
        for _ in 0..50 {
            plant.grow(&Wall::default(), 16.0);
        }
        plant
    }
    fn check_structure(plant: &Plant) {
        let ids: HashSet<_> = plant.nodes.iter().map(|n| n.id).collect();
        assert_eq!(ids.len(), plant.nodes.len());
        assert!(plant.nodes.len() <= MAX_NODES && plant.tips.len() <= MAX_TIPS);
        for (index, node) in plant.nodes.iter().enumerate() {
            assert!(node.position.is_finite());
            if let Some(parent) = node.parent {
                assert!(parent < index);
                assert!(
                    (node.position.distance(plant.nodes[parent].position) - node.rest_length).abs()
                        < 0.002
                );
            }
        }
        for a in &plant.anchors {
            assert_eq!(a.position, plant.nodes[a.node].position);
        }
        for tip in &plant.tips {
            assert!(tip.node < plant.nodes.len());
        }
    }
    fn assert_survivors_unchanged(before: &Plant, after: &Plant) {
        for node in &after.nodes {
            let original = before.nodes.iter().find(|old| old.id == node.id).unwrap();
            assert_eq!(
                (node.position, node.rest_length),
                (original.position, original.rest_length)
            );
            assert_eq!(
                node.parent.map(|p| after.nodes[p].id),
                original.parent.map(|p| before.nodes[p].id)
            );
        }
        check_structure(after);
    }

    #[test]
    fn removed_wall_prunes_upper_attached_stem_and_regrows_from_cut() {
        let mut plant = grown();
        let mut wall = Wall::default();
        let original = plant.clone();
        let cell = plant.anchors[1].cell;
        wall.missing.insert(cell);
        let result = plant.revalidate(&wall).unwrap();
        assert!(
            result.removed > 0 && result.buds == 1,
            "unsupported upper stem is still suspended"
        );
        assert_survivors_unchanged(&original, &plant);
        assert!(original
            .anchors
            .iter()
            .skip(2)
            .all(|a| wall.voxel(a.cell) == Some(1)));
        assert!(
            original.anchors.iter().skip(2).all(|a| !plant
                .nodes
                .iter()
                .any(|n| n.id == original.nodes[a.node].id)),
            "upper valid attachments must not retain severed stems"
        );
        let stump = plant.clone();
        for _ in 0..20 {
            assert!(!plant.grow(&wall, 16.0));
        }
        assert_eq!(plant, stump, "waiting must preserve the bud and RNG");
        wall.missing.clear();
        assert!(plant.grow(&wall, 16.0));
        assert_eq!(plant.nodes.last().unwrap().parent, Some(stump.tips[0].node));
        assert!(plant.nodes.last().unwrap().id >= original.next_node_id);
        for old in &stump.nodes {
            assert!(plant.nodes.iter().any(|n| n == old));
        }
        check_structure(&plant);
    }

    #[test]
    fn local_branch_cut_preserves_other_branches_and_their_tips() {
        let original = grown();
        let tip_node = original.tips[1].node;
        let anchor = original
            .anchors
            .iter()
            .rev()
            .find(|a| {
                original.is_descendant(tip_node, a.node)
                    && original
                        .tips
                        .iter()
                        .filter(|t| original.is_descendant(t.node, a.node))
                        .count()
                        == 1
            })
            .unwrap();
        let mut wall = Wall::default();
        wall.missing.insert(anchor.cell);
        let mut plant = original.clone();
        assert!(plant.revalidate(&wall).unwrap().removed > 0);
        assert_survivors_unchanged(&original, &plant);
        for tip in original
            .tips
            .iter()
            .filter(|t| !original.is_descendant(t.node, anchor.node))
        {
            let id = original.nodes[tip.node].id;
            let kept = plant
                .tips
                .iter()
                .find(|t| plant.nodes[t.node].id == id)
                .expect("unrelated tip deleted");
            assert_eq!(
                (kept.rng, kept.direction, kept.arc, kept.lateral),
                (tip.rng, tip.direction, tip.arc, tip.lateral)
            );
        }
        let waiting = plant
            .tips
            .iter()
            .find(|t| t.restart.is_some())
            .unwrap()
            .clone();
        plant.grow(&wall, 16.0);
        assert_eq!(
            plant.tips.iter().find(|t| t.restart.is_some()).unwrap(),
            &waiting,
            "another growing tip consumed the waiting bud RNG"
        );
    }

    #[test]
    fn edits_between_sparse_anchors_and_between_nodes_are_detected() {
        let original = grown();
        let node = &original.nodes[3];
        let parent = &original.nodes[node.parent.unwrap()];
        let cell = (((node.position + parent.position) * 0.5) - Vec3::Z * 1.75)
            .floor()
            .as_ivec3();
        assert!(!original.anchors.iter().any(|a| a.cell == cell));
        let mut wall = Wall::default();
        wall.missing.insert(cell);
        let mut plant = original.clone();
        assert!(plant.revalidate(&wall).unwrap().removed > 0);
        assert!(!plant.nodes.iter().any(|n| n.id == node.id));
        assert_survivors_unchanged(&original, &plant);
        assert!(!plant.grow(&wall, 16.0));
    }

    #[test]
    fn missing_root_retains_only_a_latent_seed_then_recovers() {
        let mut plant = grown();
        let original = plant.clone();
        let mut wall = Wall::default();
        wall.missing.insert(plant.anchors[0].cell);
        plant.revalidate(&wall).unwrap();
        assert_eq!(plant.nodes, original.nodes[..1]);
        assert!(plant.root_connected() && !plant.anchors[0].attached);
        assert!(!plant.grow(&wall, 16.0));
        wall.missing.clear();
        plant.revalidate(&wall).unwrap();
        assert!(plant.anchors[0].attached);
        assert!(plant.grow(&wall, 16.0));
    }

    #[test]
    fn pending_and_stale_leave_even_known_cuts_transactional() {
        let original = grown();
        for wall in [
            Wall {
                pending: true,
                ..Default::default()
            },
            Wall {
                stale: true,
                missing: HashSet::from([original.anchors[1].cell]),
                ..Default::default()
            },
        ] {
            let mut plant = original.clone();
            assert_eq!(plant.revalidate(&wall), None);
            assert!(!plant.grow(&wall, 16.0));
            assert_eq!(plant, original);
        }
        struct Partial {
            missing: IVec3,
            pending: IVec3,
        }
        impl Terrain for Partial {
            fn voxel(&self, cell: IVec3) -> Option<u8> {
                if cell == self.pending {
                    None
                } else {
                    Some(u8::from(cell.z == 0 && cell != self.missing))
                }
            }
            fn current(&self) -> bool {
                true
            }
        }
        let mut plant = original.clone();
        let terrain = Partial {
            missing: original.anchors.last().unwrap().cell,
            pending: original.anchors[1].cell,
        };
        assert_eq!(plant.revalidate(&terrain), None);
        assert_eq!(plant, original);
    }

    #[test]
    fn foreign_material_and_buried_stems_prune_instead_of_rebinding() {
        let original = grown();
        let support = original.anchors[1].cell;
        struct Changed {
            cell: IVec3,
            buried: bool,
        }
        impl Terrain for Changed {
            fn voxel(&self, cell: IVec3) -> Option<u8> {
                if self.buried && cell == self.cell + IVec3::Z {
                    Some(1)
                } else if !self.buried && cell == self.cell {
                    Some(2)
                } else {
                    Some(u8::from(cell.z == 0))
                }
            }
            fn current(&self) -> bool {
                true
            }
        }
        for buried in [false, true] {
            let mut plant = original.clone();
            let terrain = Changed {
                cell: support,
                buried,
            };
            assert!(plant.revalidate(&terrain).unwrap().removed > 0);
            assert_survivors_unchanged(&original, &plant);
            assert!(!plant.grow(&terrain, 16.0));
        }
    }

    #[test]
    fn unrelated_edits_do_not_change_shape_or_history() {
        let mut plant = grown();
        let original = plant.clone();
        let wall = Wall {
            missing: HashSet::from([IVec3::new(200, 200, 0)]),
            ..Default::default()
        };
        assert_eq!(plant.revalidate(&wall), Some(Pruned::default()));
        assert_eq!(plant, original);
    }

    #[test]
    fn repeated_pruning_reclaims_capacity_and_never_reuses_node_ids() {
        let mut a = grown();
        let mut b = a.clone();
        let wall = Wall::default();
        for _ in 0..24 {
            let next_id = a.next_node_id;
            a.prune_to_root();
            b.prune_to_root();
            assert_eq!(a.nodes.len(), 1);
            for _ in 0..30 {
                a.grow(&wall, 16.0);
                b.grow(&wall, 16.0);
            }
            assert_eq!(a, b);
            assert!(a.nodes.iter().skip(1).all(|n| n.id >= next_id));
            check_structure(&a);
        }
        assert!(
            a.next_node_id > MAX_NODES as u64,
            "history must not exhaust the live-node cap"
        );
    }

    #[test]
    fn pruning_nested_failures_makes_only_frontier_buds_and_root_cut_stops_growth() {
        let mut plant = grown();
        let original = plant.clone();
        let mut wall = Wall::default();
        wall.missing
            .extend(plant.anchors.iter().skip(1).map(|a| a.cell));
        assert_eq!(plant.revalidate(&wall).unwrap().buds, 1);
        assert_survivors_unchanged(&original, &plant);
        plant.disconnect_root();
        let stump = plant.clone();
        assert!(!plant.grow(&Wall::default(), 16.0));
        assert_eq!(plant, stump);
    }

    #[test]
    fn manual_pruning_and_deterministic_growth_keep_structure_valid() {
        let mut a = grown();
        let b = grown();
        assert_eq!(a, b);
        assert!(a.prune_highest_attachment().unwrap().removed > 0);
        assert_survivors_unchanged(&b, &a);
        assert!(a.grow(&Wall::default(), 16.0));
        check_structure(&a);
        a.prune_to_root();
        assert_eq!(a.prune_highest_attachment(), None);
        assert_eq!(a.prune_to_root(), Pruned::default());
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
}
