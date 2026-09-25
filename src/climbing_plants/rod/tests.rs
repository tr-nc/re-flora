use super::*;
use crate::climbing_plants::fixtures::Fixture;
use glam::IVec3;

struct Scene(Fixture);
impl Terrain for Scene {
    fn voxel(&self, cell: IVec3) -> Option<u8> {
        Some(u8::from(self.0.solid(cell)))
    }
    fn current(&self) -> bool {
        true
    }
}
fn seed(f: Fixture, s: u64) -> Plant {
    let (p, n, c) = f.seed();
    Plant::seed(p, n, c, 1, s).with_clockwise(false)
}
fn tick(plant: &mut Plant, terrain: &impl Terrain, spacing: f32) {
    plant.grow(terrain, spacing);
    for _ in 0..2 {
        plant
            .step_motion(terrain, 0.05, 2.0, spacing, true)
            .unwrap();
    }
}
fn safe(plant: &Plant, terrain: &impl Terrain) {
    assert!(plant.nodes.iter().all(|n| n.position.is_finite()));
    assert_eq!(plant.tips.len(), 1);
    assert!(lengths_valid(
        plant,
        &plant.nodes.iter().map(|n| n.position).collect::<Vec<_>>()
    ));
    for node in &plant.nodes {
        if let Some(p) = node.parent {
            assert_eq!(
                clear_segment(
                    terrain,
                    plant.nodes[p].position,
                    node.position,
                    plant.radius
                ),
                Some(true)
            );
        }
    }
    for a in &plant.anchors {
        assert!(plant.nodes[a.node].position.distance(a.position) <= MAX_ANCHOR_DRIFT);
        assert_eq!(terrain.voxel(a.cell), Some(a.material));
        assert_eq!(
            clear_segment(
                terrain,
                plant.nodes[a.node].position,
                a.surface_position(),
                0.0
            ),
            Some(true)
        );
    }
}
#[test]
fn self_weight_does_not_kick_the_free_shoot_backwards_on_consecutive_steps() {
    let scene = Scene(Fixture::Inward);
    let mut plant = seed(Fixture::Inward, 3500);
    let mut previous = std::collections::BTreeMap::<u64, Vec3>::new();
    for tick in 0..240 {
        plant.grow(&scene, 10.0);
        for _ in 0..2 {
            let before = plant.nodes.clone();
            plant.step_motion(&scene, 0.05, 2.0, 10.0, true).unwrap();
            for (old, node) in before.iter().zip(&plant.nodes) {
                let displacement = node.position - old.position;
                if let Some(prior) = previous.insert(node.id, displacement) {
                    assert!(
                        !(prior.dot(displacement) < -0.01
                            && prior.length() > 0.1
                            && displacement.length() > 0.1),
                        "self-weight kicked node {} backwards at tick {tick}: {prior:?} then {displacement:?}",
                        node.id
                    );
                }
            }
        }
    }
}

#[test]
fn search_turn_changes_the_live_oscillation_without_restarting() {
    let terrain = Scene(Fixture::Flat);
    let mut ordinary = seed(Fixture::Flat, 42);
    for _ in 0..12 {
        tick(&mut ordinary, &terrain, 16.0);
    }
    let mut stronger = ordinary.clone();
    stronger.set_search_tuning(2.0, AIR_BUDGET);
    for _ in 0..12 {
        ordinary
            .step_motion(&terrain, 0.05, 1.0, 16.0, true)
            .unwrap();
        stronger
            .step_motion(&terrain, 0.05, 1.0, 16.0, true)
            .unwrap();
    }
    assert!(stronger.rod.amplitude > ordinary.rod.amplitude * 1.4);
    assert_ne!(
        stronger.nodes.last().unwrap().position,
        ordinary.nodes.last().unwrap().position
    );
    safe(&stronger, &terrain);
    stronger.set_search_tuning(3.0, 96.0);
    for _ in 0..30 {
        tick(&mut stronger, &terrain, 16.0);
        safe(&stronger, &terrain);
    }
}

#[test]
fn a_young_shoot_still_establishes_its_first_slope_attachment_under_weight() {
    let scene = Scene(Fixture::Slope);
    let mut plant = seed(Fixture::Slope, 42);
    for _ in 0..40 {
        tick(&mut plant, &scene, 16.0);
    }
    println!(
        "slope anchors={} tip={:?}",
        plant.anchors.len(),
        plant.nodes.last().unwrap().position
    );
    assert!(
        plant.anchors.len() > 1,
        "weight response pulled the young shoot off its first slope contact"
    );
}

#[test]
fn continuous_fixtures_are_safe_and_reach_supported_upper_wall() {
    for fixture in Fixture::ALL {
        let scene = Scene(fixture);
        let mut plant = seed(fixture, 42);
        for _ in 0..180 {
            tick(&mut plant, &scene, 16.0);
            safe(&plant, &scene);
        }
        let height = plant
            .anchors
            .iter()
            .map(|a| a.position.y)
            .fold(0.0, f32::max);
        println!(
            "fixture={} nodes={} anchors={} height={} tip={:?}",
            fixture.name(),
            plant.nodes.len(),
            plant.anchors.len(),
            height,
            plant.nodes.last().unwrap().position
        );
        assert!(
            height >= 262.0,
            "{} did not reach upper wall: {height}",
            fixture.name()
        );
    }
}
#[test]
fn seed_3500_reproduces_and_older_stem_moves_without_sharp_joints() {
    let scene = Scene(Fixture::Inward);
    let mut a = seed(Fixture::Inward, 3500);
    let mut b = a.clone();
    let mut old_motion = 0.0f32;
    let mut max_angle = 0.0f32;
    for _ in 0..180 {
        let before = a.clone();
        tick(&mut a, &scene, 10.0);
        tick(&mut b, &scene, 10.0);
        assert_eq!(a, b);
        safe(&a, &scene);
        for (old, new) in before.nodes.iter().zip(&a.nodes) {
            if old.fixed {
                old_motion = old_motion.max(old.position.distance(new.position));
            }
        }
        for nodes in a.nodes.windows(3) {
            let incoming = (nodes[1].position - nodes[0].position).normalize();
            let outgoing = (nodes[2].position - nodes[1].position).normalize();
            max_angle = max_angle.max(incoming.dot(outgoing).clamp(-1.0, 1.0).acos().to_degrees());
        }
    }
    println!("established_motion={old_motion} max_joint_angle={max_angle}");
    assert!(old_motion > 0.001, "established body still frozen");
    assert!(max_angle < 35.0, "sharp joint: {max_angle}");
}
#[test]
fn hole_contacts_do_not_freeze_the_free_shoot_after_climbing() {
    struct Translated;
    const OFFSET: IVec3 = IVec3::new(128, -63, 0);
    impl Terrain for Translated {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            let c = c - OFFSET;
            // Real terrain exports retain the solid surface shell, not hidden
            // interior voxels. Those extra inward faces must not change which
            // side of a wall receives a contact reaction.
            Some(u8::from(
                Fixture::Hole.solid(c)
                    && [
                        IVec3::X,
                        IVec3::NEG_X,
                        IVec3::Y,
                        IVec3::NEG_Y,
                        IVec3::Z,
                        IVec3::NEG_Z,
                    ]
                    .into_iter()
                    .any(|n| !Fixture::Hole.solid(c + n)),
            ))
        }
        fn current(&self) -> bool {
            true
        }
    }
    struct Snapshot {
        min: IVec3,
        max: IVec3,
    }
    impl Terrain for Snapshot {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            if c.cmpge(self.min).all() && c.cmplt(self.max).all() {
                Translated.voxel(c)
            } else {
                None
            }
        }
        fn current(&self) -> bool {
            true
        }
    }
    let (p, n, c) = Fixture::Hole.seed();
    let mut plant = Plant::seed(p + OFFSET.as_vec3(), n, c + OFFSET, 1, 42).with_clockwise(true);
    let mut late_motion = 0;
    for frame in 0..180 {
        let min = plant
            .nodes
            .iter()
            .fold(Vec3::splat(f32::INFINITY), |b, n| b.min(n.position));
        let max = plant
            .nodes
            .iter()
            .fold(Vec3::splat(f32::NEG_INFINITY), |b, n| b.max(n.position));
        let terrain = Snapshot {
            min: (min - Vec3::splat(8.0)).floor().as_ivec3(),
            max: (max + Vec3::splat(9.0)).ceil().as_ivec3(),
        };
        plant.grow(&terrain, 16.0);
        for _ in 0..2 {
            let moved = plant
                .step_motion(&terrain, 0.05, 1.0, 16.0, true)
                .expect("motion lookahead exceeded the app's two-tick terrain snapshot");
            if frame >= 160 {
                late_motion += moved;
            }
        }
    }
    assert!(
        late_motion > 0,
        "one old contact froze the whole exploring body"
    );
    let height = plant
        .anchors
        .iter()
        .map(|a| a.position.y)
        .fold(0.0f32, f32::max);
    assert!(
        height >= 262.0 + OFFSET.y as f32,
        "translated hole failed to climb: {height}"
    );
}

#[test]
fn stem_above_wall_bends_under_its_own_weight_instead_of_staying_upright() {
    let terrain = Scene(Fixture::Flat);
    let mut plant = Plant::seed(
        Vec3::new(255.5, 294.5, 306.8),
        Vec3::Z,
        IVec3::new(255, 294, 305),
        1,
        3500,
    )
    .with_clockwise(false);
    let mut peak = plant.nodes[0].position.y;
    let mut downward = false;
    for _ in 0..360 {
        tick(&mut plant, &terrain, 10.0);
        safe(&plant, &terrain);
        peak = peak.max(plant.nodes.last().unwrap().position.y);
        // A growing apex may still point up on a drooping stem. Measure the
        // simultaneous body, not just the apex tangent.
        downward |= plant
            .nodes
            .windows(2)
            .any(|n| (n[1].position - n[0].position).normalize().y < -0.2);
    }
    let end = plant.nodes.last().unwrap().position;
    println!("wall_top=300 peak={peak} end={end:?} downward={downward}");
    assert!(
        downward && peak - end.y > 8.0,
        "unsupported stem remains an upright antenna instead of bending down"
    );
}

#[test]
fn a_downward_tip_can_extend_along_its_tangent_without_penetration() {
    let terrain = Scene(Fixture::Flat);
    let mut plant = Plant::seed(
        Vec3::new(255.5, 294.5, 306.8),
        Vec3::Z,
        IVec3::new(255, 294, 305),
        1,
        3500,
    );
    let mut node = plant.nodes[0].clone();
    node.id = 1;
    node.parent = Some(0);
    node.position.y -= 2.0;
    node.rest_length = 2.0;
    node.fixed = false;
    node.contact = growth::contact_at(&plant, &node, &terrain).unwrap();
    plant.nodes.push(node);
    plant.next_node_id = 2;
    plant.tips[0].node = 1;
    plant.tips[0].arc = 2.0;
    assert!(
        plant.grow(&terrain, 10.0),
        "a safe downward extension was rejected"
    );
    assert!(plant.nodes[2].position.y < plant.nodes[1].position.y - 1.0);
    safe(&plant, &terrain);
}

#[test]
fn an_unattached_stem_rests_and_moves_tangentially_on_a_wall_lip() {
    struct NonAdhesiveLip;
    impl Terrain for NonAdhesiveLip {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            Some(if c == IVec3::new(255, 299, 303) {
                1
            } else if Fixture::Flat.solid(c) {
                2
            } else {
                0
            })
        }
        fn current(&self) -> bool {
            true
        }
    }
    let terrain = NonAdhesiveLip;
    let mut plant = Plant::seed(
        Vec3::new(255.5, 300.83, 303.0),
        Vec3::Y,
        IVec3::new(255, 299, 303),
        1,
        42,
    );
    for i in 1..=20 {
        let mut node = plant.nodes[0].clone();
        node.id = i as u64;
        node.parent = Some(i - 1);
        node.position += Vec3::X * (i as f32 * 2.0);
        node.rest_length = 2.0;
        node.fixed = false;
        node.contact = None;
        plant.nodes.push(node);
    }
    plant.next_node_id = 21;
    plant.tips[0].node = 20;
    plant.tips[0].arc = 40.0;
    plant.tips[0].spacing = 64.0;
    let mut touched = false;
    let mut slid = false;
    for _ in 0..180 {
        let before = plant.nodes.clone();
        plant.step_motion(&terrain, 0.05, 2.0, 64.0, false).unwrap();
        safe(&plant, &terrain);
        assert_eq!(
            plant.anchors.len(),
            1,
            "ordinary contact became instant adhesion"
        );
        for (a, b) in before.iter().zip(&plant.nodes).skip(2).take(12) {
            if a.position.y < 300.8 && b.position.y < 300.8 {
                touched = true;
                let d = b.position - a.position;
                slid |= d.x * d.x + d.z * d.z > 1e-6;
            }
        }
    }
    println!("lip touched={touched} slid={slid}");
    assert!(
        touched && slid,
        "an unglued stem must rest without being welded to the lip"
    );
}

#[test]
fn a_drooping_shoot_can_attach_to_a_roof_and_its_far_side() {
    struct BroadWall;
    impl Terrain for BroadWall {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            Some(u8::from(
                (224..288).contains(&c.x) && (192..300).contains(&c.y) && (264..306).contains(&c.z),
            ))
        }
        fn current(&self) -> bool {
            true
        }
    }
    let mut plant = Plant::seed(
        Vec3::new(255.5, 294.5, 306.8),
        Vec3::Z,
        IVec3::new(255, 294, 305),
        1,
        3500,
    )
    .with_clockwise(false);
    for _ in 0..480 {
        tick(&mut plant, &BroadWall, 10.0);
        safe(&plant, &BroadWall);
    }
    println!(
        "roof/back anchors={:?}",
        plant
            .anchors
            .iter()
            .map(|a| (a.normal, a.position))
            .collect::<Vec<_>>()
    );
    assert!(
        plant.anchors.iter().any(|a| a.normal == Vec3::Y),
        "drooping stem did not establish roof contact"
    );
    assert!(
        plant.anchors.iter().any(|a| a.normal == Vec3::NEG_Z),
        "drooping stem did not establish far-side contact"
    );
}

#[test]
fn phase_is_elapsed_time_not_growth_attempts_and_pause_holds_it() {
    let scene = Scene(Fixture::Flat);
    let mut a = seed(Fixture::Flat, 42);
    let mut b = a.clone();
    for _ in 0..10 {
        a.grow(&scene, 16.0);
        for _ in 0..3 {
            b.grow(&scene, 16.0);
        }
        a.step_motion(&scene, 0.05, 1.0, 16.0, true).unwrap();
        b.step_motion(&scene, 0.05, 1.0, 16.0, true).unwrap();
    }
    assert_eq!(a.rod.phase, b.rod.phase);
    let phase = a.rod.phase;
    a.step_motion(&scene, 0.05, 1.0, 16.0, false).unwrap();
    assert_eq!(phase, a.rod.phase);
}
#[test]
fn contact_requires_dwell_instead_of_freezing_at_growth() {
    let scene = Scene(Fixture::Flat);
    let mut plant = seed(Fixture::Flat, 42);
    for _ in 0..12 {
        plant.grow(&scene, 8.0);
    }
    assert_eq!(plant.anchors.len(), 1);
    plant.step_motion(&scene, 0.05, 1.0, 8.0, true).unwrap();
    assert_eq!(plant.anchors.len(), 1);
    for _ in 0..20 {
        plant.step_motion(&scene, 0.05, 1.0, 8.0, true).unwrap();
    }
    assert!(plant.anchors.len() > 1);
}
#[test]
fn unknown_or_stale_queries_roll_back_motion_material_phase_and_attachment() {
    struct Unready(bool);
    impl Terrain for Unready {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            if self.0 {
                None
            } else {
                Scene(Fixture::Flat).voxel(c)
            }
        }
        fn current(&self) -> bool {
            self.0
        }
    }
    let mut plant = seed(Fixture::Flat, 42);
    for _ in 0..12 {
        tick(&mut plant, &Scene(Fixture::Flat), 16.0);
    }
    for unready in [Unready(true), Unready(false)] {
        let before = plant.clone();
        assert_eq!(plant.step_motion(&unready, 0.05, 1.0, 16.0, true), None);
        assert_eq!(plant, before);
    }
}
#[test]
fn late_unavailability_or_revision_change_rolls_back_the_whole_transaction() {
    use std::cell::Cell;
    struct Changing {
        queries: Cell<usize>,
        unavailable: bool,
    }
    impl Terrain for Changing {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            self.queries.set(self.queries.get() + 1);
            if self.unavailable && self.queries.get() >= 200 {
                None
            } else {
                Scene(Fixture::Flat).voxel(c)
            }
        }
        fn current(&self) -> bool {
            self.unavailable || self.queries.get() < 200
        }
    }
    let mut plant = seed(Fixture::Flat, 42);
    for _ in 0..12 {
        tick(&mut plant, &Scene(Fixture::Flat), 16.0);
    }
    for unavailable in [true, false] {
        let terrain = Changing {
            queries: Cell::new(0),
            unavailable,
        };
        let before = plant.clone();
        assert_eq!(plant.step_motion(&terrain, 0.05, 1.0, 16.0, true), None);
        assert!(terrain.queries.get() >= 200);
        assert_eq!(plant, before);
    }
}

#[test]
fn unsupported_reach_is_finite_and_independent_of_adhesion_spacing() {
    struct RootOnly(IVec3);
    impl Terrain for RootOnly {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            Some(u8::from(c == self.0))
        }
        fn current(&self) -> bool {
            true
        }
    }
    let mut lengths = Vec::new();
    for spacing in [4.0, 64.0] {
        let mut plant = seed(Fixture::Flat, 42);
        let terrain = RootOnly(plant.anchors[0].cell);
        for _ in 0..100 {
            plant.grow(&terrain, spacing);
        }
        let arc: f32 = plant.nodes.iter().map(|n| n.rest_length).sum();
        assert!(arc <= AIR_BUDGET);
        assert!(!plant.grow(&terrain, spacing));
        lengths.push(arc);
    }
    assert_eq!(lengths[0], lengths[1]);
}

#[test]
fn segment_interior_obstacle_reaction_does_not_need_endpoint_contacts() {
    struct Cube;
    impl Terrain for Cube {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            Some(u8::from(c == IVec3::ZERO))
        }
        fn current(&self) -> bool {
            true
        }
    }
    let mut plant = seed(Fixture::Flat, 42);
    plant.nodes[0].position = Vec3::new(-1.0, 1.8, 0.5);
    let mut node = plant.nodes[0].clone();
    node.position = Vec3::new(2.0, 1.8, 0.5);
    node.parent = Some(0);
    node.contact = None;
    plant.nodes.push(node);
    let old: Vec<_> = plant.nodes.iter().map(|n| n.position).collect();
    let constraints = collision::gather(&plant, &old, &Cube).unwrap();
    let mut proposed = [Vec3::new(-1.0, 1.6, 0.5), Vec3::new(2.0, 1.6, 0.5)];
    assert_eq!(
        clear_segment(&Cube, proposed[0], proposed[1], plant.radius),
        Some(false)
    );
    collision::project(&constraints, &[1.0, 1.0], &mut proposed);
    assert_eq!(
        clear_segment(&Cube, proposed[0], proposed[1], plant.radius),
        Some(true)
    );
}

#[test]
fn cut_regrows_with_fresh_ids_while_the_original_support_is_missing() {
    struct Cut(Option<IVec3>);
    impl Terrain for Cut {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            if self.0 == Some(c) {
                Some(0)
            } else {
                Scene(Fixture::Flat).voxel(c)
            }
        }
        fn current(&self) -> bool {
            true
        }
    }
    let mut plant = seed(Fixture::Flat, 42);
    for _ in 0..60 {
        tick(&mut plant, &Cut(None), 10.0);
    }
    let old_max = plant.nodes.last().unwrap().id;
    let cell = plant.anchors[1].cell;
    let cut = Cut(Some(cell));
    assert!(plant.revalidate(&cut).unwrap().removed > 0);
    let stump = plant.clone();
    for _ in 0..30 {
        tick(&mut plant, &cut, 10.0);
        safe(&plant, &cut);
    }
    assert!(plant.nodes.len() > stump.nodes.len());
    assert_eq!(plant.nodes[0], stump.nodes[0]);
    assert!(plant.nodes[..stump.nodes.len()]
        .iter()
        .zip(&stump.nodes)
        .all(|(new, old)| new.id == old.id
            && new.parent == old.parent
            && new.rest_length == old.rest_length));
    assert!(plant.nodes[stump.nodes.len()..]
        .iter()
        .all(|n| n.id > old_max));
    assert!(plant.anchors.iter().all(|a| a.cell != cell));
}
