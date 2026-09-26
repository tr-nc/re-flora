use super::*;
use glam::{IVec3, UVec3};
use std::cell::Cell;
use std::collections::HashSet;

#[derive(Default)]
struct Wall {
    z: i32,
    missing: HashSet<IVec3>,
    pending: bool,
    stale: bool,
    queries: Cell<usize>,
}
impl Terrain for Wall {
    fn voxel(&self, cell: IVec3) -> Option<u8> {
        self.queries.set(self.queries.get() + 1);
        (!self.pending).then(|| u8::from(cell.z == self.z && !self.missing.contains(&cell)))
    }
    fn current(&self) -> bool {
        !self.stale
    }
}

const TUNING: Tuning = Tuning {
    spacing: 16.0,
    flexibility: 1.0,
    search: None,
};
fn seed() -> Plant {
    Plant::seed(
        Vec3::new(20.5, 4.5, 1.8),
        Vec3::Z,
        IVec3::new(20, 4, 0),
        1,
        42,
    )
}
fn play(dt: f32, speed: f32) -> Cadence {
    Cadence::Play {
        dt,
        growth_per_second: speed,
    }
}
fn dependencies(revision: u64) -> [ContreeCpuVoxelSourceDependency; 1] {
    [ContreeCpuVoxelSourceDependency {
        chunk_idx: UVec3::ZERO,
        source_revision: Some(revision),
        is_present: true,
    }]
}
fn advance(vine: &mut VineTick, terrain: &impl Terrain, revision: u64, cadence: Cadence) -> Report {
    vine.advance(
        Some(Snapshot {
            terrain,
            dependencies: &dependencies(revision),
        }),
        cadence,
        TUNING,
        false,
    )
}
fn grown(samples: usize) -> VineTick {
    let mut vine = VineTick::new(seed());
    for _ in 0..samples {
        let report = advance(
            &mut vine,
            &Wall::default(),
            1,
            Cadence::Review { growing: true },
        );
        assert!(report.publish && !report.waiting_for_terrain);
    }
    vine
}

#[test]
fn continuous_growth_and_motion_keep_the_same_order_across_frame_cadences() {
    for speed in [10.0, 12.0] {
        let mut outcomes = Vec::new();
        for (frames, dt) in [(100, 0.01), (20, 0.05), (10, 0.1)] {
            let mut vine = VineTick::new(seed());
            let (mut births, mut motion) = (0, 0);
            for _ in 0..frames {
                let report = advance(&mut vine, &Wall::default(), 1, play(dt, speed));
                assert!(report.publish && !report.waiting_for_terrain);
                births += report.growth_attempts;
                motion += report.motion_steps;
            }
            assert_eq!((births, motion), (speed as u32, 20));
            outcomes.push(vine.plant().clone());
        }
        // Includes geometry, RNG, rod phase/material/contact memory, and stable IDs.
        assert_eq!(outcomes[0], outcomes[1]);
        assert_eq!(outcomes[1], outcomes[2]);
    }
}

#[test]
fn a_birth_precedes_motion_in_the_same_quantum() {
    let wall = Wall::default();
    // A single-step contract oracle, not a second implementation of the clock loop.
    let mut expected = seed();
    assert!(expected.grow(&wall, 16.0));
    expected.step_motion(&wall, 0.05, 1.0, 16.0, true).unwrap();
    let mut vine = VineTick::new(seed());
    let report = advance(&mut vine, &wall, 1, play(0.05, 20.0));
    assert_eq!((report.growth_attempts, report.motion_steps), (1, 1));
    assert_eq!(vine.plant(), &expected);
}

#[test]
fn young_shoot_settles_without_births_independent_of_tick_cadence() {
    let initial = grown(5).plant().clone();
    let mut outcomes = Vec::new();
    for (frames, dt) in [(100, 0.01), (20, 0.05), (10, 0.1)] {
        let mut vine = VineTick::new(initial.clone());
        for _ in 0..frames {
            assert_eq!(
                advance(&mut vine, &Wall::default(), 1, play(dt, 0.0)).growth_attempts,
                0
            );
        }
        assert_eq!(vine.plant().nodes.len(), initial.nodes.len());
        assert_ne!(vine.plant().nodes, initial.nodes);
        outcomes.push(vine.plant().clone());
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(outcomes[1], outcomes[2]);
}

#[test]
fn no_world_time_holds_both_clocks_without_losing_fractional_time() {
    let mut vine = VineTick::new(seed());
    let wall = Wall::default();
    assert_eq!(
        advance(&mut vine, &wall, 1, play(0.02, 20.0)).motion_steps,
        0
    );
    let held = vine.plant().clone();
    for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let report = advance(&mut vine, &wall, 1, play(dt, 20.0));
        assert!(report.publish && !report.waiting_for_terrain);
        assert_eq!((report.growth_attempts, report.motion_steps), (0, 0));
        assert_eq!(vine.plant(), &held);
    }
    let report = advance(&mut vine, &wall, 1, play(0.03, 20.0));
    assert_eq!((report.growth_attempts, report.motion_steps), (1, 1));
    let mut reference = VineTick::new(seed());
    advance(&mut reference, &wall, 1, play(0.05, 20.0));
    assert_eq!(vine.plant(), reference.plant());
}

#[test]
fn changing_or_invalid_growth_speed_does_not_retime_motion_or_discard_birth_fraction() {
    let mut vine = VineTick::new(seed());
    let wall = Wall::default();
    assert_eq!(
        advance(&mut vine, &wall, 1, play(0.05, 12.0)).growth_attempts,
        0
    );
    for speed in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let report = advance(&mut vine, &wall, 1, play(0.05, speed));
        assert_eq!((report.growth_attempts, report.motion_steps), (0, 1));
    }
    let report = advance(&mut vine, &wall, 1, play(0.05, 8.0));
    assert_eq!((report.growth_attempts, report.motion_steps), (1, 1));
}

#[test]
fn catch_up_bounds_motion_and_birth_work_and_drops_excess_debt() {
    let wall = Wall::default();
    let mut vine = VineTick::new(seed());
    let report = advance(&mut vine, &wall, 1, play(100.0, 40.0));
    assert_eq!((report.growth_attempts, report.motion_steps), (16, 8));
    let mut reference = VineTick::new(seed());
    for _ in 0..8 {
        advance(&mut reference, &wall, 1, play(0.05, 40.0));
    }
    assert_eq!(vine.plant(), reference.plant());
    let report = advance(&mut vine, &wall, 1, play(0.025, 40.0));
    assert_eq!((report.growth_attempts, report.motion_steps), (0, 0));
    let report = advance(&mut vine, &wall, 1, play(0.025, 40.0));
    assert_eq!((report.growth_attempts, report.motion_steps), (2, 1));
    // The birth clock has its own bound, even for an out-of-GUI-range speed.
    let mut vine = VineTick::new(seed());
    let report = advance(&mut vine, &wall, 1, play(0.05, 1000.0));
    assert_eq!((report.growth_attempts, report.motion_steps), (8, 1));
    assert_eq!(
        advance(&mut vine, &wall, 1, play(0.05, 20.0)).growth_attempts,
        1
    );
}

#[test]
fn review_keeps_one_birth_then_two_pose_steps_and_does_not_spend_play_time() {
    let wall = Wall::default();
    let mut vine = VineTick::new(seed());
    advance(&mut vine, &wall, 1, play(0.025, 20.0));
    let mut expected = seed();
    expected.grow(&wall, 16.0);
    expected.step_motion(&wall, 0.05, 1.0, 16.0, true).unwrap();
    expected.step_motion(&wall, 0.05, 1.0, 16.0, true).unwrap();
    let report = advance(&mut vine, &wall, 1, Cadence::Review { growing: true });
    assert_eq!((report.growth_attempts, report.motion_steps), (1, 2));
    assert_eq!(vine.plant(), &expected);
    expected.step_motion(&wall, 0.05, 1.0, 16.0, false).unwrap();
    expected.step_motion(&wall, 0.05, 1.0, 16.0, false).unwrap();
    let report = advance(&mut vine, &wall, 1, Cadence::Review { growing: false });
    assert_eq!((report.growth_attempts, report.motion_steps), (0, 2));
    assert_eq!(vine.plant(), &expected);
    assert_eq!(
        advance(&mut vine, &wall, 1, play(0.025, 20.0)).motion_steps,
        1
    );
}

#[test]
fn pending_exports_and_stale_matching_dependencies_do_not_spend_time_or_actions() {
    let wall = Wall::default();
    let mut vine = grown(5);
    advance(&mut vine, &wall, 1, play(0.025, 20.0));
    let before = vine.plant().clone();
    vine.request(Action::PruneToRoot);
    let report = vine.advance(None::<Snapshot<'_, Wall>>, play(100.0, 20.0), TUNING, false);
    assert!(!report.publish && report.waiting_for_terrain && report.events.is_empty());
    let stale = Wall {
        stale: true,
        ..Default::default()
    };
    let report = advance(&mut vine, &stale, 1, play(100.0, 20.0));
    assert!(!report.publish && report.waiting_for_terrain && report.events.is_empty());
    assert_eq!(vine.plant(), &before);
    let report = advance(&mut vine, &wall, 1, play(0.025, 20.0));
    assert_eq!((report.growth_attempts, report.motion_steps), (1, 1));
    assert!(matches!(report.events.as_slice(), [Event::PrunedToRoot(_)]));
    assert_eq!(vine.plant().nodes.len(), 2);
    assert!(vine.plant().nodes[1].id > before.nodes.last().unwrap().id);
    assert!(advance(&mut vine, &wall, 1, play(0.0, 20.0))
        .events
        .is_empty());
}

#[test]
fn unavailable_motion_stops_catch_up_without_replaying_issued_quanta() {
    let mut vine = grown(5);
    let original = vine.plant().clone();
    // Identity is unchanged: the owner passes query-level uncertainty to Plant,
    // which independently refuses growth/motion without mutating its solver state.
    let pending = Wall {
        pending: true,
        ..Default::default()
    };
    let report = advance(&mut vine, &pending, 1, play(0.1, 40.0));
    assert!(report.publish && report.waiting_for_terrain);
    assert_eq!((report.growth_attempts, report.motion_steps), (2, 0));
    assert_eq!(vine.plant(), &original);
    let report = advance(&mut vine, &Wall::default(), 1, play(0.0, 40.0));
    assert_eq!((report.growth_attempts, report.motion_steps), (0, 0));
    let report = advance(&mut vine, &Wall::default(), 1, play(0.05, 40.0));
    assert_eq!((report.growth_attempts, report.motion_steps), (2, 1));
}

#[test]
fn later_unavailable_motion_keeps_committed_work_without_replaying_issued_quanta() {
    struct ExhaustedQueries {
        wall: Wall,
        remaining: Cell<usize>,
        unavailable: Cell<usize>,
    }
    impl Terrain for ExhaustedQueries {
        fn voxel(&self, cell: IVec3) -> Option<u8> {
            let remaining = self.remaining.get();
            if remaining == 0 {
                self.unavailable.set(self.unavailable.get() + 1);
                None
            } else {
                self.remaining.set(remaining - 1);
                self.wall.voxel(cell)
            }
        }
        fn current(&self) -> bool {
            self.wall.current()
        }
    }

    let wall = Wall::default();
    let initial = grown(5).plant().clone();
    let mut reference = VineTick::new(initial.clone());
    advance(&mut reference, &wall, 1, play(0.0, 20.0));
    wall.queries.set(0);
    let report = advance(&mut reference, &wall, 1, play(0.05, 20.0));
    assert_eq!((report.growth_attempts, report.motion_steps), (1, 1));
    assert!(initial
        .nodes
        .iter()
        .zip(&reference.plant().nodes)
        .any(|(old, new)| old.position != new.position));

    // Calibrate at the Terrain seam, not against a hardcoded solver query count:
    // one production quantum plus the next birth succeeds; the second motion's
    // first query is unavailable. This single-birth oracle is not a copied tick loop.
    let mut committed = reference.plant().clone();
    assert!(committed.grow(&wall, TUNING.spacing));
    let terrain = ExhaustedQueries {
        wall: Wall::default(),
        remaining: Cell::new(wall.queries.get()),
        unavailable: Cell::new(0),
    };
    let mut vine = VineTick::new(initial.clone());
    advance(&mut vine, &wall, 1, play(0.0, 20.0));
    let report = advance(&mut vine, &terrain, 1, play(0.15, 20.0));
    assert!(report.publish && report.waiting_for_terrain);
    assert_eq!((report.growth_attempts, report.motion_steps), (2, 1));
    assert!(terrain.unavailable.get() > 0);
    assert_eq!(committed.nodes.len(), initial.nodes.len() + 2);
    // Includes the successful pose, both births, RNG and rod memory, but no state
    // from the failed motion. Earlier successful work remains publishable.
    assert_eq!(vine.plant(), &committed);

    for dt in [0.0, 0.025] {
        let report = advance(&mut vine, &wall, 1, play(dt, 20.0));
        assert!(report.publish && !report.waiting_for_terrain);
        assert_eq!((report.growth_attempts, report.motion_steps), (0, 0));
        assert_eq!(vine.plant(), &committed);
    }
    // Neither the failed nor the unvisited issued motion quantum is replayed.
    // Only newly elapsed time advances; compare through the production interface.
    let mut resumed = VineTick::new(committed);
    let expected = advance(&mut resumed, &wall, 1, play(0.05, 20.0));
    let report = advance(&mut vine, &wall, 1, play(0.025, 20.0));
    assert_eq!((expected.growth_attempts, expected.motion_steps), (1, 1));
    assert_eq!((report.growth_attempts, report.motion_steps), (1, 1));
    assert!(report.publish && !report.waiting_for_terrain);
    assert_eq!(vine.plant(), resumed.plant());
}

#[test]
fn failed_revalidation_keeps_edit_and_dependency_changes_pending_until_recovery() {
    for changed_dependency in [false, true] {
        let mut vine = grown(5);
        let original = vine.plant().clone();
        let root = original.anchors[0].cell;
        let mut wall = Wall {
            pending: true,
            missing: HashSet::from([root]),
            ..Default::default()
        };
        let revision = if changed_dependency { 2 } else { 1 };
        if !changed_dependency {
            vine.observe_edit(UAabb3::new(root.as_uvec3(), (root + IVec3::ONE).as_uvec3()));
        }
        let report = advance(&mut vine, &wall, revision, play(100.0, 20.0));
        assert!(!report.publish && report.waiting_for_terrain);
        assert_eq!(vine.plant(), &original);
        wall.pending = false;
        let report = advance(&mut vine, &wall, revision, play(0.0, 20.0));
        assert!(report.publish && !report.waiting_for_terrain);
        assert!(matches!(
            report.events.as_slice(),
            [Event::SupportPruned { .. }]
        ));
        assert_eq!(vine.plant().nodes, original.nodes[..1]);
        assert!(!vine.plant().anchors[0].attached);
        wall.missing.clear();
        let report = advance(&mut vine, &wall, revision + 1, play(0.05, 20.0));
        assert_eq!((report.growth_attempts, report.motion_steps), (1, 1));
        assert!(vine.plant().anchors[0].attached);
        assert_eq!(vine.plant().nodes.len(), 2);
    }
}

#[test]
fn late_stale_validation_does_not_commit_a_known_cut_or_spend_time() {
    struct BecomesStale {
        calls: Cell<usize>,
        wall: Wall,
    }
    impl Terrain for BecomesStale {
        fn voxel(&self, cell: IVec3) -> Option<u8> {
            self.wall.voxel(cell)
        }
        fn current(&self) -> bool {
            let call = self.calls.get();
            self.calls.set(call + 1);
            call == 0
        }
    }
    let mut vine = grown(5);
    let before = vine.plant().clone();
    let terrain = BecomesStale {
        calls: Cell::new(0),
        wall: Wall {
            missing: HashSet::from([before.anchors[0].cell]),
            ..Default::default()
        },
    };
    let report = advance(&mut vine, &terrain, 2, play(100.0, 20.0));
    assert!(!report.publish && report.waiting_for_terrain);
    assert_eq!(vine.plant(), &before);
    let report = advance(&mut vine, &terrain.wall, 2, play(0.0, 20.0));
    assert!(report.publish && !report.waiting_for_terrain);
    assert_eq!(vine.plant().nodes.len(), 1);
}

#[test]
fn matching_exports_skip_revalidation_but_local_edits_and_new_dependencies_do_not() {
    let mut vine = VineTick::new(Plant::seed(
        Vec3::new(255.8, 10.5, 255.8),
        Vec3::Z,
        IVec3::new(255, 10, 254),
        1,
        42,
    ));
    let wall = Wall {
        z: 254,
        ..Default::default()
    };
    advance(&mut vine, &wall, 1, play(0.0, 10.0));
    let original = vine.plant().clone();
    wall.queries.set(0);
    advance(&mut vine, &wall, 1, play(0.0, 10.0));
    assert_eq!(wall.queries.get(), 0);
    vine.observe_edit(UAabb3::new(
        UVec3::new(260, 10, 260),
        UVec3::new(261, 11, 261),
    ));
    advance(&mut vine, &wall, 1, play(0.0, 10.0));
    assert_eq!(wall.queries.get(), 0, "unrelated edit revalidated the vine");
    vine.observe_edit(UAabb3::new(
        UVec3::new(256, 10, 256),
        UVec3::new(257, 11, 257),
    ));
    advance(&mut vine, &wall, 1, play(0.0, 10.0));
    assert!(
        wall.queries.replace(0) > 0,
        "nearby chunk/refill was ignored"
    );
    advance(&mut vine, &wall, 2, play(0.0, 10.0));
    assert!(
        wall.queries.get() > 0,
        "dependency changes require no edit notification"
    );
    assert_eq!(vine.plant(), &original);
}

#[test]
fn pruning_precedes_births_and_root_support_recovery_keeps_fresh_ids() {
    let mut vine = grown(40);
    let original = vine.plant().clone();
    assert!(original.anchors.len() >= 3);
    let mut wall = Wall {
        missing: HashSet::from([original.anchors[1].cell]),
        ..Default::default()
    };
    let report = advance(&mut vine, &wall, 2, play(0.0, 20.0));
    assert!(matches!(
        report.events.as_slice(),
        [Event::SupportPruned { .. }]
    ));
    let stump = vine.plant().clone();
    assert!(stump.nodes.len() < original.nodes.len());
    for node in &stump.nodes {
        let old = original.nodes.iter().find(|old| old.id == node.id).unwrap();
        // The cut can make retained free tissue young again; geometry/history stay.
        assert_eq!(
            (node.position, node.rest_length, node.parent),
            (old.position, old.rest_length, old.parent)
        );
    }
    for anchor in original.anchors.iter().skip(2) {
        assert!(!stump
            .nodes
            .iter()
            .any(|node| node.id == original.nodes[anchor.node].id));
    }
    let report = advance(&mut vine, &wall, 2, play(0.05, 20.0));
    assert_eq!(report.growth_attempts, 1);
    assert_eq!(vine.plant().nodes.len(), stump.nodes.len() + 1);
    assert_eq!(
        vine.plant().nodes[stump.nodes.len()].parent,
        Some(stump.tips[0].node)
    );
    assert!(vine.plant().nodes.last().unwrap().id > original.nodes.last().unwrap().id);
    assert!(!vine.growth_blocked());
    wall.missing.insert(original.anchors[0].cell);
    advance(&mut vine, &wall, 3, play(0.1, 20.0));
    assert_eq!(vine.plant().nodes, original.nodes[..1]);
    assert!(vine.plant().root_connected() && !vine.plant().anchors[0].attached);
    assert!(vine.growth_blocked());
    let latent = vine.plant().clone();
    advance(&mut vine, &wall, 3, play(0.1, 20.0));
    assert_eq!(vine.plant(), &latent);
    wall.missing.clear();
    advance(&mut vine, &wall, 4, play(0.05, 20.0));
    assert!(vine.plant().anchors[0].attached);
    assert_eq!(vine.plant().nodes.len(), 2);
    assert!(vine.plant().nodes[1].id > original.nodes.last().unwrap().id);
}

#[test]
fn manual_actions_coalesce_in_order_before_revalidation_and_births() {
    let mut vine = grown(40);
    // Queue in the opposite order; duplicate button requests still run once.
    vine.request(Action::DisconnectRoot);
    vine.request(Action::PruneToRoot);
    vine.request(Action::PruneHighest);
    vine.request(Action::PruneHighest);
    let root = vine.plant().anchors[0].cell;
    let wall = Wall {
        missing: HashSet::from([root]),
        ..Default::default()
    };
    let report = advance(&mut vine, &wall, 2, play(0.05, 20.0));
    assert!(matches!(
        report.events.as_slice(),
        [
            Event::PrunedHighest(_),
            Event::PrunedToRoot(_),
            Event::DisconnectedRoot
        ]
    ));
    assert_eq!(vine.plant().nodes.len(), 1);
    assert!(!vine.plant().root_connected() && !vine.plant().anchors[0].attached);
    assert!(advance(&mut vine, &Wall::default(), 3, play(0.1, 20.0))
        .events
        .is_empty());
    assert_eq!(
        vine.plant().nodes.len(),
        1,
        "manual disconnection must not recover"
    );
}

#[test]
fn replacing_a_vine_drops_old_clocks_actions_and_validation_history() {
    let wall = Wall::default();
    let mut vine = VineTick::new(seed());
    advance(&mut vine, &wall, 1, play(0.04, 20.0));
    vine.request(Action::DisconnectRoot);
    vine = VineTick::new(seed());
    wall.queries.set(0);
    let report = advance(&mut vine, &wall, 1, play(0.01, 20.0));
    assert_eq!(report.motion_steps, 0);
    assert!(report.events.is_empty() && wall.queries.get() > 0);
    assert_eq!(vine.plant(), &seed());
}
