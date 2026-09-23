//! Rooted wall vines: loss of support prunes the downstream subtree and leaves a regrowth bud.
//! Unknown/stale terrain never commits pruning or growth. Coordinates are terrain voxels.
use glam::{IVec3, Vec3};

const MAX_NODES: usize = 512;
const MAX_TIPS: usize = 4;

pub mod fixtures;
mod growth;
mod rod;
mod shoot;

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
    pub fixed: bool,
    rest_direction: Vec3,
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
    // The first severed step is retried exactly, rather than picking a new direction
    // every frame and eventually jumping past the missing wall.
    restart: Option<Restart>,
    phase: u8,
    clockwise: bool,
    exterior: Vec3,
    under_ceiling: bool,
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
            restart: None,
            phase,
            clockwise: seed & 1 == 0,
            exterior: Vec3::Z,
            under_ceiling: false,
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
    rod: Option<rod::Rod>,
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
                rest_direction: Vec3::Y,
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
            tips: vec![tip],
            radius: 0.65,
            root_connected: true,
            seed,
            next_node_id: 1,
            rod: None,
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
        if self.rod.is_some() {
            rod::step(self, terrain, dt, flexibility, spacing, exploring)
        } else {
            shoot::relax(self, terrain, dt, flexibility, spacing)
        }
    }

    pub fn with_continuous_stem(mut self, enabled: bool) -> Self {
        self.rod = enabled.then(|| rod::Rod::new(&self.tips[0]));
        self
    }

    pub fn continuous_stem(&self) -> bool {
        self.rod.is_some()
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

    fn attach_tip(&mut self, index: usize, contact: Contact, spacing: f32) {
        if self.rod.is_some() {
            return;
        } // the rod establishes persistent contacts behind the apex
        let tip = &self.tips[index];
        if tip.arc < tip.spacing {
            return;
        }
        let node = tip.node;
        self.anchors.push(Anchor {
            node,
            cell: contact.cell,
            normal: contact.normal,
            material: self.anchors[0].material,
            position: self.nodes[node].position,
            attached: true,
        });
        self.freeze_path(node);
        let tip = &mut self.tips[index];
        tip.arc = 0.0;
        tip.spacing = spacing * (0.85 + 0.3 * random(&mut tip.rng));
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

    pub fn search_probes(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.tips.iter().map(|tip| {
            tip.restart
                .as_ref()
                .map_or_else(|| growth::probe(self, tip), |restart| restart.position)
        })
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
            if self.continuous_stem() {
                cut[anchor.node] |= !clear_segment(
                    terrain,
                    self.nodes[anchor.node].position,
                    anchor.surface_position(),
                    0.0,
                )?;
            }
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
            bud.arc = 0.0;
            bud.restart = Some(Restart::from(node));
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
        // A retained cut is a stable stump. Repair starts a new flexible shoot;
        // it must not reanimate the geometry that survived the cut.
        let stumps: Vec<_> = self
            .tips
            .iter()
            .filter(|tip| tip.restart.is_some())
            .map(|tip| tip.node)
            .collect();
        for stump in stumps {
            self.freeze_path(stump);
            if let Some(rod) = &mut self.rod {
                rod.locked_through = rod.locked_through.max(self.nodes[stump].id);
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
        for index in 0..self.tips.len() {
            if next.nodes.len() >= MAX_NODES {
                break;
            }
            let mut tip = next.tips[index].clone();
            let start = next.nodes[tip.node].position;
            let Some(step) = growth::advance(&next, &mut tip, terrain, spacing) else {
                return false;
            };
            let Some(step) = step else {
                next.tips[index] = tip;
                continue;
            };
            let end = step.position;
            if step.normal.y.abs() < 0.7 {
                tip.exterior = step.normal;
            }
            if step.normal.y < -0.5 && step.contact.is_some() {
                tip.under_ceiling = true;
            }
            if tip.node == 0 || tip.restart.is_some() {
                tip.spacing = spacing;
            }
            let parent = tip.node;
            tip.node = next.nodes.len();
            tip.arc += start.distance(end);
            tip.restart = None;
            next.nodes.push(Node {
                id: next.next_node_id,
                position: end,
                parent: Some(parent),
                rest_length: start.distance(end),
                normal: step.normal,
                fixed: false,
                rest_direction: (end - start).normalize(),
                contact: step.contact.clone(),
                backing: step.backing,
            });
            next.next_node_id += 1;
            next.tips[index] = tip;
            if let Some(contact) = step.contact {
                next.attach_tip(index, contact, spacing);
            }
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
        assert!(plant.grow(&Ledge, 16.0));
        let outside = plant.nodes.last().unwrap().position;
        assert!(outside.z > 1.65 && (outside.y - 1.17).abs() < 0.001);
        assert!(plant.grow(&Ledge, 16.0));
        assert!(plant.nodes.last().unwrap().position.y > outside.y + 0.5);
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
            assert!(plant.tips[0].arc <= 48.001);
            assert_eq!(plant.revalidate(&Slope).unwrap().removed, 0);
            plant
        };
        for seed in [0, 43, 65535] {
            assert_eq!(run(seed), run(seed));
        }
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
        let a = seed();
        let b = Plant::seed(a.nodes[0].position, Vec3::Z, a.anchors[0].cell, 1, 44);
        assert_ne!(
            a.search_probes().next(),
            b.search_probes().next(),
            "same-handed seeds still have the same initial state"
        );
        assert_eq!(a, seed(), "a fixed seed must be reproducible");
    }

    #[test]
    fn an_existing_unattached_shoot_sags_while_the_base_stays_fixed() {
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
                .any(|(a, b)| a.position.y < b.position.y - 0.05),
            "the unattached shoot is still permanently frozen"
        );
        check_structure(&plant);
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
        let mut original = seed();
        original.nodes.push(Node {
            id: 1,
            position: Vec3::new(20.5, 6.5, 1.8),
            parent: Some(0),
            rest_length: 2.0,
            normal: Vec3::Z,
            fixed: true,
            rest_direction: Vec3::Y,
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
                    let frozen: Vec<_> = plant
                        .nodes
                        .iter()
                        .filter(|node| node.fixed)
                        .cloned()
                        .collect();
                    plant.grow(&terrain, 16.0);
                    for _ in 0..2 {
                        plant.relax_shoot(&terrain, 0.05, 1.0, 16.0).unwrap();
                    }
                    for old in &frozen {
                        assert_eq!(plant.nodes.iter().find(|node| node.id == old.id), Some(old));
                    }
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
    fn clockwise_and_counterclockwise_search_are_mirrored_and_air_growth_is_bounded() {
        let mut a = seed();
        let mut b = a.clone();
        b.tips[0].clockwise = false;
        let pa = growth::probe(&a, &a.tips[0]) - a.nodes[0].position;
        let pb = growth::probe(&b, &b.tips[0]) - b.nodes[0].position;
        assert!(pa.x * pb.x < 0.0 && (pa.y - pb.y).abs() < 1e-6 && (pa.z - pb.z).abs() < 1e-6);
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
        assert!(a.tips.iter().all(|tip| tip.arc <= 50.0));
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
                unsupported <= 48.001,
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
