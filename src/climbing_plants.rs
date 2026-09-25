//! Rooted wall vines: loss of support prunes the downstream subtree and leaves a regrowth bud.
//! Unknown/stale terrain never commits pruning or growth. Coordinates are terrain voxels.
use glam::{IVec3, Vec3};

const MAX_NODES: usize = 512;
// Gameplay cap on the *surviving* rooted stem, in voxel arc length. Pruning
// removes rest length and makes room for new growth; IDs and age do not spend it.
const MAX_LIVE_ARC: f32 = 512.0;
const MAX_TIPS: usize = 4;

pub mod fixtures;
mod growth;
mod rod;
mod sweep;

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
    pub normal: Vec3,
    /// Established history (coloring), not a mechanical lock.
    pub fixed: bool,
    contact: Option<Contact>,
    // Record only backing that actually existed when this edge grew. Searching
    // arcs across an existing hole do not invent wall dependencies in empty space.
    backing: Option<(Vec3, Vec3)>,
}
#[derive(Clone, Debug, PartialEq)]
struct Contact {
    cell: IVec3,
    normal: Vec3,
}
#[derive(Clone, Debug, PartialEq)]
struct Restart {
    position: Vec3,
    normal: Vec3,
    contact: Option<Contact>,
    backing: Option<(Vec3, Vec3)>,
}
impl From<&Node> for Restart {
    fn from(node: &Node) -> Self {
        Self {
            position: node.position,
            normal: node.normal,
            contact: node.contact.clone(),
            backing: node.backing,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    pub node: usize,
    pub cell: IVec3,
    pub material: u8,
    pub position: Vec3,
    pub attached: bool,
    pub normal: Vec3,
}
impl Anchor {
    /// Fixed rootlet footprint just outside the exposed support face.
    pub fn surface_position(&self) -> Vec3 {
        surface_position(self.position, self.cell, self.normal)
    }
}
fn surface_position(position: Vec3, cell: IVec3, normal: Vec3) -> Vec3 {
    let mut surface = position.clamp(cell.as_vec3(), cell.as_vec3() + Vec3::ONE);
    let face = cell.as_vec3() + Vec3::splat(0.5) + normal * 0.5;
    surface += normal * (0.03 - (surface - face).dot(normal));
    surface
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tip {
    pub node: usize,
    lateral: f32,
    arc: f32,
    spacing: f32,
    rng: u64,
    regrowing: bool,
    phase: u8,
    clockwise: bool,
    exterior: Vec3,
}
impl Tip {
    fn seed(node: usize, seed: u64) -> Self {
        let mut rng = seed;
        let phase = growth::initial_phase(random(&mut rng));
        let lateral = (random(&mut rng) - 0.5) * 0.7;
        Self {
            node,
            lateral,
            arc: 0.0,
            spacing: 16.0,
            rng,
            regrowing: false,
            phase,
            clockwise: seed & 1 == 0,
            exterior: Vec3::Z,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    pub nodes: Vec<Node>,
    pub anchors: Vec<Anchor>,
    pub tips: Vec<Tip>,
    pub radius: f32,
    root_connected: bool,
    seed: u64,
    next_node_id: u64,
    rod: rod::Rod,
    search_turn: f32,
    search_reach: f32,
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
        let mut tip = Tip::seed(0, seed);
        if normal.y.abs() < 0.7 {
            tip.exterior = normal;
        }
        Self {
            nodes: vec![Node {
                id: 0,
                position,
                parent: None,
                rest_length: 0.0,
                normal,
                fixed: true,
                contact: Some(Contact { cell, normal }),
                backing: None,
            }],
            anchors: vec![Anchor {
                node: 0,
                cell,
                material,
                position,
                attached: true,
                normal,
            }],
            rod: rod::Rod::new(&tip),
            search_turn: 1.0,
            search_reach: rod::AIR_BUDGET,
            tips: vec![tip],
            radius: 0.65,
            root_connected: true,
            seed,
            next_node_id: 1,
        }
    }

    #[cfg(test)]
    pub fn relax_shoot(
        &mut self,
        terrain: &impl Terrain,
        dt: f32,
        flexibility: f32,
        spacing: f32,
    ) -> Option<usize> {
        self.step_motion(terrain, dt, flexibility, spacing, true)
    }

    /// Advance the independent motion clock; pausing exploration still permits settling.
    pub fn step_motion(
        &mut self,
        terrain: &impl Terrain,
        dt: f32,
        flexibility: f32,
        spacing: f32,
        exploring: bool,
    ) -> Option<usize> {
        rod::step(self, terrain, dt, flexibility, spacing, exploring)
    }

    /// Live artistic controls. They do not reset material, phase or existing geometry.
    pub fn set_search_tuning(&mut self, turn: f32, reach: f32) {
        if turn.is_finite() && (0.0..=3.0).contains(&turn) {
            self.search_turn = turn;
        }
        if reach.is_finite() && (16.0..=96.0).contains(&reach) {
            self.search_reach = reach;
        }
    }

    pub fn with_clockwise(mut self, clockwise: bool) -> Self {
        for tip in &mut self.tips {
            tip.clockwise = clockwise;
        }
        self
    }

    fn freeze_path(&mut self, mut node: usize) {
        while !self.nodes[node].fixed {
            self.nodes[node].fixed = true;
            let Some(parent) = self.nodes[node].parent else {
                break;
            };
            node = parent;
        }
    }

    pub fn disconnect_root(&mut self) {
        self.root_connected = false;
    }
    pub fn root_connected(&self) -> bool {
        self.root_connected
    }
    pub fn live_arc(&self) -> f32 {
        self.nodes.iter().map(|node| node.rest_length).sum()
    }
    pub fn max_live_arc(&self) -> f32 {
        MAX_LIVE_ARC
    }
    pub fn regrowth_nodes(&self) -> impl Iterator<Item = &Node> {
        self.tips
            .iter()
            .filter(|tip| tip.regrowing)
            .map(|tip| &self.nodes[tip.node])
    }

    pub fn search_probes(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.tips.iter().map(|tip| growth::probe(self, tip))
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
        // A compliant stem may move away from its provisional surface contact;
        // established attachment dependencies remain authoritative until pruning.
        for anchor in self.anchors.iter().skip(1) {
            cut[anchor.node] = terrain.voxel(anchor.cell)? != anchor.material;
            cut[anchor.node] |= !clear_segment(
                terrain,
                self.nodes[anchor.node].position,
                anchor.surface_position(),
                0.0,
            )?;
        }
        for (id, node) in self.nodes.iter().enumerate().skip(1) {
            let parent = node.parent.expect("non-root stem has a parent");
            if cut[parent] {
                cut[id] = true;
                continue;
            }
            cut[id] |= !growth::restart_valid(
                self,
                self.nodes[parent].position,
                &Restart::from(node),
                terrain,
            )?;
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
        let existing_tips = tips.len();
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
            bud.regrowing = true;
            // The severed step may depend on the wall that was just removed.
            // Resume searching from the surviving stump, not that old footprint.
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
        // Only a surviving established attachment marks old, dark tissue. A
        // severed attachment must not turn the free stem above the last anchor
        // into fixed history (or immobilize that part of the searching shoot).
        for node in &mut self.nodes {
            node.fixed = false;
        }
        let attached_nodes: Vec<_> = self
            .anchors
            .iter()
            .filter(|anchor| anchor.attached)
            .map(|anchor| anchor.node)
            .collect();
        for node in attached_nodes {
            self.freeze_path(node);
        }
        let last_anchor = self.anchors.last().unwrap().node;
        self.rod.locked_through = self.nodes[last_anchor].id;
        for tip_index in existing_tips..self.tips.len() {
            let stump = self.tips[tip_index].node;
            let last_anchor = self
                .anchors
                .iter()
                .rev()
                .find(|anchor| self.is_descendant(stump, anchor.node))
                .unwrap()
                .node;
            let mut arc = 0.0;
            let mut node = stump;
            while node != last_anchor {
                arc += self.nodes[node].rest_length;
                node = self.nodes[node].parent.unwrap();
            }
            self.tips[tip_index].arc = arc;
            if self.tips.len() == 1 {
                self.rod.reorient_at(&self.nodes, stump);
            }
        }
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
        let mut live_arc = next.live_arc();
        for index in 0..self.tips.len() {
            if next.nodes.len() >= MAX_NODES {
                break;
            }
            let mut tip = next.tips[index].clone();
            let start = next.nodes[tip.node].position;
            let Some(step) = growth::advance(&next, &mut tip, terrain) else {
                return false;
            };
            let Some(step) = step else {
                next.tips[index] = tip;
                continue;
            };
            let end = step.position;
            let length = start.distance(end);
            if live_arc + length > MAX_LIVE_ARC + 1e-4 {
                continue;
            }
            live_arc += length;
            if step.normal.y.abs() < 0.7 {
                tip.exterior = step.normal;
            }
            if tip.node == 0 {
                tip.spacing = spacing;
            }
            let parent = tip.node;
            tip.node = next.nodes.len();
            tip.regrowing = false;
            tip.arc += length;
            next.nodes.push(Node {
                id: next.next_node_id,
                position: end,
                parent: Some(parent),
                rest_length: length,
                normal: step.normal,
                fixed: false,
                contact: step.contact.clone(),
                backing: step.backing,
            });
            next.next_node_id += 1;
            next.tips[index] = tip;
            changed = true;
        }
        // A seed makes one shoot. Automatic branching is intentionally disabled.
        if terrain.current() {
            *self = next;
            changed
        } else {
            false
        }
    }
}

/// Exact slab intersection, shared by exposed-stem collision and backing-surface checks.
fn intersects_box(start: Vec3, end: Vec3, lo: Vec3, hi: Vec3) -> bool {
    segment_box_interval(start, end, lo, hi).is_some()
}
fn segment_box_interval(start: Vec3, end: Vec3, lo: Vec3, hi: Vec3) -> Option<(f32, f32)> {
    let delta = end - start;
    let mut enter: f32 = 0.0;
    let mut exit: f32 = 1.0;
    for axis in 0..3 {
        if delta[axis].abs() < 1e-8 {
            if start[axis] < lo[axis] || start[axis] > hi[axis] {
                return None;
            }
        } else {
            let a = (lo[axis] - start[axis]) / delta[axis];
            let b = (hi[axis] - start[axis]) / delta[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
        }
    }
    (enter <= exit).then_some((enter, exit))
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
            for _ in 0..2 {
                plant
                    .step_motion(&Wall::default(), 0.05, 1.0, 16.0, true)
                    .unwrap();
            }
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
            assert!(a.position.distance(plant.nodes[a.node].position) <= 0.3);
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
    fn a_sagged_tip_clears_its_whole_radius_past_a_ledge_before_turning_up() {
        struct Ledge;
        impl Terrain for Ledge {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                Some(u8::from(c.y >= 2 && c.z < 1))
            }
            fn current(&self) -> bool {
                true
            }
        }
        let mut plant = Plant::seed(
            Vec3::new(0.5, 1.17, 1.1),
            Vec3::NEG_Y,
            IVec3::new(0, 2, 0),
            1,
            42,
        );
        for _ in 0..60 {
            plant.grow(&Ledge, 16.0);
            plant.step_motion(&Ledge, 0.05, 1.0, 16.0, true).unwrap();
            for n in &plant.nodes[1..] {
                assert_eq!(
                    clear_segment(
                        &Ledge,
                        plant.nodes[n.parent.unwrap()].position,
                        n.position,
                        plant.radius
                    ),
                    Some(true)
                );
            }
        }
        let outside = plant.nodes.last().unwrap().position;
        assert!(outside.z > 1.65 && outside.y > 2.0);
    }

    #[test]
    fn randomized_single_shoots_are_reproducible_and_safe_even_when_they_stop_at_edges() {
        struct Slope;
        impl Terrain for Slope {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                Some(u8::from(fixtures::Fixture::Slope.solid(c)))
            }
            fn current(&self) -> bool {
                true
            }
        }
        let run = |seed| {
            let (position, normal, cell) = fixtures::Fixture::Slope.seed();
            let mut plant = Plant::seed(position, normal, cell, 1, seed);
            for _ in 0..180 {
                plant.grow(&Slope, 16.0);
                for _ in 0..2 {
                    plant.relax_shoot(&Slope, 0.05, 1.0, 16.0).unwrap();
                }
            }
            check_structure(&plant);
            assert_eq!(plant.tips.len(), 1);
            assert!(plant.tips[0].arc <= rod::AIR_BUDGET + 0.001);
            assert_eq!(plant.revalidate(&Slope).unwrap().removed, 0);
            plant
        };
        for seed in [0, 43, 65535] {
            assert_eq!(run(seed), run(seed));
        }
    }

    #[test]
    fn live_length_cap_is_reclaimed_by_pruning() {
        let wall = Wall::default();
        let mut plant = seed();
        for i in 1..=256 {
            let mut node = plant.nodes[0].clone();
            node.id = i as u64;
            node.parent = Some(i - 1);
            node.position.y += 2.0 * i as f32;
            node.rest_length = 2.0;
            node.contact = None;
            node.backing = None;
            plant.nodes.push(node);
        }
        plant.next_node_id = 257;
        plant.tips[0].node = 256;
        plant.tips[0].arc = 0.0;
        assert!(!plant.grow(&wall, 16.0), "live stem exceeded its arc cap");
        plant.prune_to_root();
        assert!(
            plant.grow(&wall, 16.0),
            "pruning must reclaim length budget"
        );
        assert_eq!(plant.nodes[1].id, 257);
    }

    #[test]
    fn normal_growth_stays_a_single_unbranched_vine() {
        let plant = grown();
        assert_eq!(plant.tips.len(), 1, "normal growth still creates branches");
        assert!(plant
            .nodes
            .iter()
            .enumerate()
            .skip(1)
            .all(|(i, node)| node.parent == Some(i - 1)));
    }

    #[test]
    fn random_seed_changes_the_initial_exploration_not_just_later_attachment_spacing() {
        let mut a = seed();
        let mut b = Plant::seed(a.nodes[0].position, Vec3::Z, a.anchors[0].cell, 1, 44);
        assert_eq!(a, seed(), "a fixed seed must be reproducible");
        for _ in 0..10 {
            for p in [&mut a, &mut b] {
                p.grow(&Wall::default(), 64.0);
                p.step_motion(&Wall::default(), 0.05, 1.0, 64.0, true)
                    .unwrap();
            }
        }
        assert_ne!(
            a.search_probes().next(),
            b.search_probes().next(),
            "same-handed seeds must bend differently before attaching"
        );
    }

    #[test]
    fn an_existing_unattached_shoot_moves_while_the_base_stays_fixed() {
        let mut plant = seed();
        for _ in 0..5 {
            plant.grow(&Wall::default(), 64.0);
        }
        assert_eq!(plant.anchors.len(), 1);
        let before = plant.clone();
        for _ in 0..30 {
            plant.relax_shoot(&Wall::default(), 0.05, 1.0, 64.0);
        }
        assert_eq!(plant.nodes[0], before.nodes[0]);
        assert!(
            plant
                .nodes
                .iter()
                .zip(&before.nodes)
                .skip(1)
                .any(|(a, b)| a.position.distance(b.position) > 0.05),
            "the unattached shoot is still permanently frozen"
        );
        check_structure(&plant);
    }

    #[test]
    fn a_cut_requires_its_established_attachment_even_after_provisional_contact_moves() {
        let mut plant = seed();
        let mut node = plant.nodes[0].clone();
        node.id = 1;
        node.parent = Some(0);
        node.position.y += 2.0;
        node.rest_length = 2.0;
        node.contact = Some(Contact {
            cell: IVec3::new(21, 6, 0),
            normal: Vec3::Z,
        });
        let mut anchor = plant.anchors[0].clone();
        anchor.node = 1;
        anchor.cell = IVec3::new(20, 6, 0);
        anchor.position = node.position;
        plant.nodes.push(node);
        plant.anchors.push(anchor.clone());
        plant.tips[0].node = 1;
        plant.next_node_id = 2;
        let mut terrain = Wall::default();
        terrain.missing.insert(anchor.cell);
        assert_eq!(plant.revalidate(&terrain).unwrap().removed, 1);
        let stump = plant.clone();
        assert!(plant.grow(&terrain, 16.0));
        assert_eq!(plant.nodes[1].id, 2);
        assert_eq!(plant.nodes[0], stump.nodes[0]);
        assert_eq!(terrain.voxel(anchor.cell), Some(0));
        assert!(plant.anchors.iter().all(|a| a.cell != anchor.cell));
        assert_eq!(
            clear_segment(
                &terrain,
                plant.nodes[0].position,
                plant.nodes[1].position,
                plant.radius
            ),
            Some(true)
        );
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
        let last_anchor = plant.anchors.last().unwrap().node;
        assert!(
            plant.nodes.len() > last_anchor + 1,
            "cut must leave a free stump"
        );
        assert!(
            plant.nodes[last_anchor + 1..].iter().all(|n| !n.fixed),
            "unattached stem above the surviving anchor became dark established tissue"
        );
        let free_arc: f32 = plant.nodes[last_anchor + 1..]
            .iter()
            .map(|n| n.rest_length)
            .sum();
        assert!(
            (plant.tips[0].arc - free_arc).abs() < 0.001,
            "search reach must be measured from the last surviving attachment"
        );
        assert_eq!(plant.rod.locked_through, plant.nodes[last_anchor].id);
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
        let mut regrown = false;
        for _ in 0..40 {
            regrown |= plant.grow(&wall, 16.0);
            plant.step_motion(&wall, 0.05, 1.0, 16.0, true).unwrap();
        }
        assert!(
            regrown,
            "surviving rooted stem must explore without wall repair"
        );
        assert_eq!(
            plant.nodes[stump.nodes.len()].parent,
            Some(stump.tips[0].node)
        );
        assert!(plant.nodes[stump.nodes.len()].id >= original.next_node_id);
        for old in &stump.nodes {
            assert!(plant.nodes.iter().any(|n| n.id == old.id));
        }
        check_structure(&plant);
    }

    #[test]
    fn local_branch_cut_preserves_other_branches_and_their_tips() {
        // Pruning still supports an explicit tree, although gameplay no longer makes forks.
        let mut original = grown();
        original.tips.push(Tip::seed(12, 99));
        for _ in 0..24 {
            original.grow(&Wall::default(), 16.0);
        }
        assert_eq!(original.tips.len(), 2);
        let (cut, node) = original
            .nodes
            .iter()
            .enumerate()
            .rev()
            .find(|(id, n)| {
                let Some(contact) = &n.contact else {
                    return false;
                };
                let cell = contact.cell;
                original
                    .tips
                    .iter()
                    .filter(|t| original.is_descendant(t.node, *id))
                    .count()
                    == 1
                    && original.nodes.iter().enumerate().all(|(other, node)| {
                        original.is_descendant(other, *id)
                            || (node.contact.as_ref().is_none_or(|c| c.cell != cell)
                                && node.backing.is_none_or(|(a, b)| {
                                    !intersects_box(
                                        a,
                                        b,
                                        cell.as_vec3(),
                                        cell.as_vec3() + Vec3::ONE,
                                    )
                                }))
                    })
            })
            .expect("at least one branch has reached its own support");
        let mut wall = Wall::default();
        wall.missing.insert(node.contact.as_ref().unwrap().cell);
        let mut plant = original.clone();
        assert!(plant.revalidate(&wall).unwrap().removed > 0);
        assert_survivors_unchanged(&original, &plant);
        for tip in original
            .tips
            .iter()
            .filter(|t| !original.is_descendant(t.node, cut))
        {
            let id = original.nodes[tip.node].id;
            let kept = plant
                .tips
                .iter()
                .find(|t| plant.nodes[t.node].id == id)
                .expect("unrelated tip deleted");
            let mut expected = tip.clone();
            expected.node = kept.node; // compaction changes only the storage index
            assert_eq!(kept, &expected);
        }
        let stump = plant.nodes.last().unwrap().id;
        plant.grow(&wall, 16.0);
        assert!(
            plant.nodes.iter().any(|n| n.id == stump),
            "regrowth must not disturb the surviving branch"
        );
    }

    #[test]
    fn edits_between_sparse_anchors_and_between_nodes_are_detected() {
        let mut original = seed();
        original.nodes.push(Node {
            id: 1,
            position: Vec3::new(20.5, 6.5, 1.8),
            parent: Some(0),
            rest_length: 2.0,
            normal: Vec3::Z,
            fixed: true,
            contact: Some(Contact {
                cell: IVec3::new(20, 6, 0),
                normal: Vec3::Z,
            }),
            backing: Some((Vec3::new(20.5, 4.5, 0.9), Vec3::new(20.5, 6.5, 0.9))),
        });
        original.next_node_id = 2;
        original.tips[0].node = 1;
        let node = &original.nodes[1];
        let cell = IVec3::new(20, 5, 0);
        assert!(!original.anchors.iter().any(|a| a.cell == cell));
        let mut wall = Wall::default();
        wall.missing.insert(cell);
        let mut plant = original.clone();
        assert!(plant.revalidate(&wall).unwrap().removed > 0);
        assert!(!plant.nodes.iter().any(|n| n.id == node.id));
        assert_survivors_unchanged(&original, &plant);
        let old_id = node.id;
        plant.grow(&wall, 16.0);
        assert!(plant.nodes.iter().all(|n| n.id != old_id));
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
            plant.grow(&terrain, 16.0);
            assert!(plant
                .nodes
                .iter()
                .all(|n| n.id != original.nodes[original.anchors[1].node].id));
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
    fn rotating_tips_climb_holes_ledges_recesses_and_slopes_without_penetration() {
        struct Scene(fixtures::Fixture);
        impl Terrain for Scene {
            fn voxel(&self, cell: IVec3) -> Option<u8> {
                Some(u8::from(self.0.solid(cell)))
            }
            fn current(&self) -> bool {
                true
            }
        }
        for fixture in fixtures::Fixture::ALL {
            for clockwise in [true, false] {
                let seed = 42;
                let terrain = Scene(fixture);
                let (position, normal, cell) = fixture.seed();
                let mut plant =
                    Plant::seed(position, normal, cell, 1, seed).with_clockwise(clockwise);
                for _ in 0..180 {
                    let root = plant.nodes[0].clone();
                    plant.grow(&terrain, 16.0);
                    for _ in 0..2 {
                        plant.relax_shoot(&terrain, 0.05, 1.0, 16.0).unwrap();
                    }
                    assert_eq!(plant.nodes[0], root);
                }
                let height = plant
                    .anchors
                    .iter()
                    .map(|a| a.position.y)
                    .fold(0.0, f32::max);
                assert!(height >= 262.0, "{fixture:?} seed={seed} clockwise={clockwise} failed to attach above obstacle: height={height} nodes={} anchors={} tips={:?}", plant.nodes.len(), plant.anchors.len(), plant.tips.iter().map(|t| (plant.nodes[t.node].position, t.arc)).collect::<Vec<_>>());
                let before = plant.clone();
                assert_eq!(
                    plant.revalidate(&terrain),
                    Some(Pruned::default()),
                    "newly grown shape pruned itself in {fixture:?}"
                );
                assert_eq!(plant, before);
                check_structure(&plant);
            }
        }
    }

    #[test]
    fn changing_search_reach_live_holds_and_resumes_the_same_tip() {
        struct TinySupport;
        impl Terrain for TinySupport {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                Some(u8::from(c == IVec3::new(20, 4, 0)))
            }
            fn current(&self) -> bool {
                true
            }
        }
        let mut plant = seed();
        plant.set_search_tuning(1.0, 16.0);
        for _ in 0..100 {
            plant.grow(&TinySupport, 16.0);
        }
        let short = plant.clone();
        assert!(short.tips[0].arc <= 16.0);
        plant.set_search_tuning(1.0, 64.0);
        for _ in 0..100 {
            plant.grow(&TinySupport, 16.0);
        }
        assert!(plant.nodes.len() > short.nodes.len());
        let long = plant.clone();
        plant.set_search_tuning(1.0, 16.0);
        for _ in 0..10 {
            assert!(!plant.grow(&TinySupport, 16.0));
        }
        assert_eq!(
            plant.nodes, long.nodes,
            "lowering reach must not delete existing stem"
        );
    }

    #[test]
    fn air_growth_is_bounded() {
        let mut a = seed();
        struct TinySupport;
        impl Terrain for TinySupport {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                Some(u8::from(c == IVec3::new(20, 4, 0)))
            }
            fn current(&self) -> bool {
                true
            }
        }
        for _ in 0..100 {
            a.grow(&TinySupport, 16.0);
        }
        let count = a.nodes.len();
        for _ in 0..100 {
            a.grow(&TinySupport, 16.0);
        }
        assert_eq!(
            a.nodes.len(),
            count,
            "unsupported search kept extending forever"
        );
        assert!(a.tips.iter().all(|tip| tip.arc <= rod::AIR_BUDGET));
        for (id, _) in a.nodes.iter().enumerate() {
            let mut node = id;
            let mut unsupported = 0.0;
            while let Some(parent) = a.nodes[node].parent {
                if a.nodes[node].contact.is_some() {
                    break;
                }
                unsupported += a.nodes[node].rest_length;
                node = parent;
            }
            assert!(
                unsupported <= rod::AIR_BUDGET + 0.001,
                "forks renewed the air-growth budget: {unsupported}"
            );
        }
    }

    #[test]
    fn ground_seed_searches_upward_and_reaches_a_wall() {
        struct GroundAndWall;
        impl Terrain for GroundAndWall {
            fn voxel(&self, c: IVec3) -> Option<u8> {
                Some(u8::from(c.y <= 191 || (c.z == 305 && c.y < 300)))
            }
            fn current(&self) -> bool {
                true
            }
        }
        let mut plant = Plant::seed(
            Vec3::new(255.5, 192.83, 310.5),
            Vec3::Y,
            IVec3::new(255, 191, 310),
            1,
            42,
        );
        for _ in 0..90 {
            plant.grow(&GroundAndWall, 16.0);
            for _ in 0..2 {
                plant
                    .step_motion(&GroundAndWall, 0.05, 1.0, 16.0, true)
                    .unwrap();
            }
        }
        assert!(
            plant
                .anchors
                .iter()
                .any(|a| a.normal == Vec3::Z && a.position.y > 202.0),
            "ground shoot never found the wall"
        );
        assert_eq!(plant.revalidate(&GroundAndWall), Some(Pruned::default()));
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
