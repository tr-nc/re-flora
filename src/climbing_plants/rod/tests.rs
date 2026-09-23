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
    Plant::seed(p, n, c, 1, s)
        .with_clockwise(false)
        .with_continuous_stem(true)
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
    let mut original = a.clone().with_continuous_stem(false);
    let mut old_motion = 0.0f32;
    let mut max_angle = 0.0f32;
    let mut original_angle = 0.0f32;
    for _ in 0..180 {
        let before = a.clone();
        tick(&mut a, &scene, 10.0);
        tick(&mut b, &scene, 10.0);
        assert_eq!(a, b);
        tick(&mut original, &scene, 10.0);
        for nodes in original.nodes.windows(3) {
            let incoming = (nodes[1].position - nodes[0].position).normalize();
            let outgoing = (nodes[2].position - nodes[1].position).normalize();
            original_angle =
                original_angle.max(incoming.dot(outgoing).clamp(-1.0, 1.0).acos().to_degrees());
        }
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
    println!("established_motion={old_motion} max_joint_angle={max_angle} original_max_angle={original_angle}");
    assert!(
        max_angle < original_angle * 0.6,
        "continuous body did not improve the original kink"
    );
    assert!(old_motion > 0.001, "established body still frozen");
    assert!(max_angle < 35.0, "sharp joint: {max_angle}");
}
#[test]
fn hole_contacts_do_not_freeze_the_free_shoot_after_climbing() {
    struct Translated;
    const OFFSET: IVec3 = IVec3::new(128, -63, 0);
    impl Terrain for Translated {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            Some(u8::from(Fixture::Hole.solid(c - OFFSET)))
        }
        fn current(&self) -> bool {
            true
        }
    }
    let (p, n, c) = Fixture::Hole.seed();
    let mut plant = Plant::seed(p + OFFSET.as_vec3(), n, c + OFFSET, 1, 42)
        .with_clockwise(true)
        .with_continuous_stem(true);
    let mut late_motion = 0;
    for frame in 0..180 {
        plant.grow(&Translated, 16.0);
        for _ in 0..2 {
            let moved = plant
                .step_motion(&Translated, 0.05, 1.0, 16.0, true)
                .unwrap();
            if frame >= 160 {
                late_motion += moved;
            }
        }
    }
    assert!(
        late_motion > 0,
        "one old contact froze the whole exploring body"
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
    assert_eq!(a.rod.as_ref().unwrap().phase, b.rod.as_ref().unwrap().phase);
    let phase = a.rod.as_ref().unwrap().phase;
    a.step_motion(&scene, 0.05, 1.0, 16.0, false).unwrap();
    assert_eq!(phase, a.rod.as_ref().unwrap().phase);
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
fn cut_waits_exactly_then_repairs_with_fresh_ids_and_a_stable_base() {
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
    }
    assert_eq!(plant, stump);
    plant.revalidate(&Cut(None)).unwrap();
    for _ in 0..20 {
        tick(&mut plant, &Cut(None), 10.0);
    }
    assert!(plant.nodes.len() > stump.nodes.len());
    assert!(plant.nodes.starts_with(&stump.nodes));
    assert!(plant.nodes[stump.nodes.len()..]
        .iter()
        .all(|n| n.id > old_max));
}
