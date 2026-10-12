//! Recorded prototype inputs through the native interaction/compiler seam.
//! No GPU/window, random workload or frame-time benchmark in normal unit tests.
use super::*;
use serde_json::Value;

fn point(action: &Value, transform: usize) -> Vec3 {
    let p = action["point"].as_array().unwrap();
    let (x, z) = (p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32);
    let p = match transform {
        1 => Vec3::new(-z, 0., x),
        2 => Vec3::new(-x, 0., z),
        _ => Vec3::new(x, 0., z),
    };
    Vec3::ONE + p
}
pub(super) fn begin(start: Vec3, recipe: PavementRecipe) -> ActivePavement {
    let mut a = super::tests::pending_pavement_runtime().active.unwrap();
    a.recipe = recipe;
    a.stroke = recipe.stroke(start, Vec3::X).unwrap();
    a.intent = PathIntent::new(start);
    a
}
pub(super) fn validate(a: &mut ActivePavement, force: bool) {
    let Some(c) = a.candidate.as_ref() else {
        return;
    };
    assert_eq!(c.expected, a.continuity.expected());
    assert!(
        a.continuity.blocked().is_none(),
        "blocked at {:?}: {:?}",
        a.intent.cursor(),
        a.continuity.blocked().map(|b| b.reason)
    );
    let mut staged = PavementCoverage::default();
    for row in c.stroke.pending_rows() {
        assert!(row.valid, "folded row {}", row.index);
        let pieces = a.recipe.pieces(row);
        assert!(
            !pieces.is_empty(),
            "thin row {} at {:?}",
            row.index,
            a.intent.cursor()
        );
        for piece in pieces {
            assert!(
                a.coverage.accepts(piece) && staged.accepts(piece),
                "overlap row {} at {:?}",
                row.index,
                a.intent.cursor()
            );
            staged.record(piece);
        }
    }
    if force || a.intent.spaced(a.stroke.config.pitch) {
        a.fix_candidate();
    }
}
pub(super) fn publish(a: &mut ActivePavement) {
    assert!(a.stroke.prepare());
    let mut exit = a.continuity.accepted().map(|r| r.exit);
    while let Some(row) = a.continuity.next(&mut a.stroke.rows) {
        if let Some(exit) = exit {
            assert_eq!(row.entry, exit, "shared physical seam");
        }
        let pieces = a.recipe.pieces(row);
        assert!(!pieces.is_empty());
        for piece in pieces {
            assert!(a.coverage.accepts(piece));
            a.coverage.record(piece);
        }
        exit = Some(row.exit);
        a.continuity.accept(row);
        a.intent.published(row.exit.center);
    }
}
fn replay(actions: &[Value], recipe: PavementRecipe, transform: usize) -> Vec<ActivePavement> {
    let mut active = None;
    let mut complete = Vec::new();
    for action in actions {
        match action["type"].as_str().unwrap() {
            "begin" => active = Some(begin(point(action, transform), recipe)),
            "observe" => active
                .as_mut()
                .unwrap()
                .observe_ground(point(action, transform)),
            "advance" => {
                let a = active.as_mut().unwrap();
                validate(a, false);
                publish(a);
            }
            "finish" => {
                let mut a = active.take().unwrap();
                validate(&mut a, true);
                a.candidate = None;
                let cursor = a.intent.cursor();
                assert!(a.stroke.finish());
                publish(&mut a);
                let last = a.continuity.accepted().unwrap();
                assert!(last.exit.center.distance(cursor) <= recipe.pitch * 0.5 + VOXEL_SIZE * 0.5 + 1e-5,
                    "stopped before recorded endpoint: kind={:?} transform={transform} cursor={cursor:?} last={last:?}", recipe.kind);
                complete.push(a);
            }
            _ => {}
        }
    }
    complete
}
#[test]
fn pavement_prototype_original_turns_partition_without_thin_inner_stones() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/turns.json")).unwrap();
    for kind in [PavementKind::Slabs, PavementKind::Brick] {
        for transform in 0..3 {
            let a = replay(
                fixture["streams"][0]["actions"].as_array().unwrap(),
                PavementRecipe::new(kind, 0.12, 0.05, 42),
                transform,
            );
            assert_eq!(a.len(), 2);
        }
    }
}
#[test]
fn pavement_prototype_inward_and_second_turn_use_the_tuned_range() {
    for source in [
        include_str!("fixtures/inward.json"),
        include_str!("fixtures/second-turn.json"),
    ] {
        let fixture: Value = serde_json::from_str(source).unwrap();
        for kind in [PavementKind::Slabs, PavementKind::Brick] {
            for transform in 0..3 {
                let a = replay(
                    fixture["actions"].as_array().unwrap(),
                    PavementRecipe::new(kind, 0.12, 0.05, 42).with_smoothing(true, 2.),
                    transform,
                );
                assert_eq!(a.len(), 1);
            }
        }
    }
}
#[test]
fn pavement_prototype_long_outer_cells_split_without_losing_inner_lanes() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/arrangement.json")).unwrap();
    let a = replay(
        fixture["actions"].as_array().unwrap(),
        PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42),
        0,
    );
    assert_eq!(a.len(), 1);
    let longest = a[0]
        .coverage
        .published_pieces()
        .iter()
        .flat_map(|p| (0..4).map(move |i| p.footprint[i].distance(p.footprint[(i + 1) % 4])))
        .fold(0., f32::max);
    assert!(longest < 0.08, "long outer stone {longest}");
}
