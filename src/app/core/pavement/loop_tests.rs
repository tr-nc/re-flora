use super::*;

fn begin(recipe: ConstructionRecipe, start: Vec3) -> ActivePavement {
    let mut a = tests::pending_pavement_runtime().active.unwrap();
    a.recipe = recipe;
    a.stroke = recipe.stroke(start, Vec3::Z).unwrap();
    a.intent = PathIntent::new(start);
    a
}
fn cells(recipe: ConstructionRecipe, row: PathSample) -> Vec<PathCell> {
    match recipe {
        ConstructionRecipe::LowWall(wall) => vec![wall.raw_cell(row)],
        _ => recipe.pieces(row),
    }
}
fn validate(a: &mut ActivePavement, release: bool) {
    let Some(candidate) = &a.candidate else {
        return;
    };
    assert!(
        a.continuity.blocked().is_none(),
        "{:?}",
        a.continuity.blocked().map(|b| b.reason)
    );
    let mut staged = PavementCoverage::default();
    for row in candidate.stroke.pending_rows() {
        assert!(
            row.valid && !a.recipe.pieces(row).is_empty(),
            "invalid row {}",
            row.index
        );
        for cell in cells(a.recipe, row) {
            assert!(
                a.recipe.accepts(&a.coverage, cell),
                "published overlap row {} closing={} recipe={:?} cursor={:?} target={} last={:?}",
                row.index,
                candidate.closes_loop(),
                a.recipe,
                a.intent.cursor(),
                a.loop_target().is_some(),
                a.continuity.accepted().map(|r| r.exit)
            );
            assert!(
                a.recipe.accepts(&staged, cell),
                "staged overlap row {}",
                row.index
            );
            staged.record(cell);
        }
    }
    a.candidate.as_mut().unwrap().checked = true;
    if release || (!candidate_closes(a) && a.intent.spaced(a.stroke.config.pitch)) {
        a.fix_candidate();
    }
}
#[test]
fn construction_loops_accept_a_corner_start_without_cutting_or_overlapping_the_first_course() {
    for kind in [LowWallKind::Earth, LowWallKind::Stone, LowWallKind::Brick] {
        for width in [0.05, 0.06, 0.15] {
            for sign in [-1., 1.] {
                let recipe = LowWallRecipe::new(kind, width, 0.12, 0.05, 13).into();
                let origin = Vec3::ONE;
                let corners = [
                    Vec3::ZERO,
                    Vec3::X * 0.8,
                    Vec3::new(0.8, 0., 0.7),
                    Vec3::Z * 0.7,
                    Vec3::ZERO,
                ];
                let mut a = begin(recipe, origin);
                let mut rows = Vec::new();
                for pair in corners.windows(2) {
                    for i in 1..=60 {
                        let p = pair[0].lerp(pair[1], i as f32 / 60.);
                        a.observe_ground(origin + Vec3::new(p.x * sign, p.x * 0.03, p.z));
                        validate(&mut a, false);
                        publish(&mut a, &mut rows);
                    }
                }
                assert!(candidate_closes(&a), "{kind:?} width={width} sign={sign}");
                validate(&mut a, true);
                assert!(a.stroke.finish());
                publish(&mut a, &mut rows);
                assert_eq!(rows.last().unwrap().exit, rows[0].entry);
            }
        }
    }
}

#[test]
fn construction_loop_closes_a_recorded_native_soil_corner() {
    let points: Vec<[f32; 3]> =
        serde_json::from_str(include_str!("fixtures/loop-corner.json")).unwrap();
    let mut a = begin(
        LowWallRecipe::new(LowWallKind::Stone, 0.06, 0.12, 0.05, 42).into(),
        Vec3::from_array(points[0]),
    );
    let mut rows = Vec::new();
    for p in &points[1..] {
        a.observe_ground(Vec3::from_array(*p));
        validate(&mut a, false);
        publish(&mut a, &mut rows);
    }
    assert!(candidate_closes(&a));
    validate(&mut a, true);
    assert!(a.stroke.finish());
    publish(&mut a, &mut rows);
    assert_eq!(rows.last().unwrap().exit, rows[0].entry);
}

fn candidate_closes(a: &ActivePavement) -> bool {
    a.candidate.as_ref().is_some_and(Candidate::closes_loop)
}
fn publish(a: &mut ActivePavement, rows: &mut Vec<PathSample>) {
    if candidate_closes(a) {
        return;
    }
    assert!(a.stroke.prepare());
    while let Some(row) = a.continuity.next(&mut a.stroke.rows) {
        if let Some(last) = a.continuity.accepted() {
            assert_eq!(row.entry, last.exit);
        }
        for cell in cells(a.recipe, row) {
            assert!(a.recipe.accepts(&a.coverage, cell));
            a.coverage.record(cell);
        }
        a.continuity.accept(row);
        a.intent.published(row.exit.center);
        rows.push(row);
    }
}
fn recipes() -> Vec<ConstructionRecipe> {
    let mut recipes: Vec<_> = [LowWallKind::Earth, LowWallKind::Stone, LowWallKind::Brick]
        .into_iter()
        .map(|k| LowWallRecipe::new(k, 0.06, 0.12, 0.05, 42).into())
        .collect();
    for kind in [PavementKind::Slabs, PavementKind::Brick] {
        for smooth in [false, true] {
            recipes.push(
                PavementRecipe::new(kind, 0.12, 0.05, 42)
                    .with_smoothing(smooth, 2.)
                    .into(),
            );
        }
    }
    recipes
}

#[test]
fn construction_loops_close_exact_sections_and_masonry_bonds_in_both_directions() {
    for recipe in recipes() {
        for sign in [-1., 1.] {
            for steps in [64, 160] {
                let radius = 0.65;
                let mut a = begin(recipe, Vec3::ONE + Vec3::X * radius);
                let mut rows = Vec::new();
                for i in 1..=steps {
                    let t = sign * std::f32::consts::TAU * i as f32 / steps as f32;
                    a.observe_editable_ground(Vec3::ONE + Vec3::new(t.cos(), 0., t.sin()) * radius);
                    validate(&mut a, false);
                    publish(&mut a, &mut rows);
                }
                assert!(candidate_closes(&a), "{} steps={steps}", recipe.name());
                let before = a.coverage.published_pieces().len();
                publish(&mut a, &mut rows);
                assert_eq!(
                    a.coverage.published_pieces().len(),
                    before,
                    "snap must be preview-only until release"
                );
                validate(&mut a, true);
                assert!(a.stroke.finish());
                publish(&mut a, &mut rows);
                let first = rows[0];
                let last = *rows.last().unwrap();
                assert!(last.closes_loop);
                assert_eq!(last.exit, first.entry);
                assert_eq!(
                    rows.len() % 2,
                    0,
                    "closed bond cannot swap parity at its seam"
                );
                assert!(a.stroke.finish());
                assert!(
                    a.stroke.rows.is_empty(),
                    "finishing twice cannot duplicate the join"
                );
                if let ConstructionRecipe::LowWall(wall) = recipe {
                    let start = wall.pieces(first);
                    let end = wall.pieces(last);
                    for (layer, (a, b)) in start.iter().zip(&end).enumerate() {
                        let seam = first.entry.center;
                        let forward = Vec3::new(-first.entry.across.z, 0., first.entry.across.x);
                        let signed =
                            |p: glam::Vec2| (Vec3::new(p.x, seam.y, p.y) - seam).dot(forward);
                        if layer % 2 == 1 || wall.kind == LowWallKind::Earth {
                            assert!(signed(a.cell.footprint[0]).abs() < 1e-5);
                            assert!(signed(b.cell.footprint[3]).abs() < 1e-5);
                        } else {
                            assert!(
                                (signed(a.cell.footprint[0]) + signed(b.cell.footprint[3])).abs()
                                    < 1e-5,
                                "same physical head-gap identity on both sides"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn construction_loop_snap_is_reversible_and_does_not_capture_short_strokes() {
    let recipe: ConstructionRecipe =
        LowWallRecipe::new(LowWallKind::Stone, 0.06, 0.12, 0.05, 42).into();
    let start = Vec3::ONE + Vec3::X * 0.4;
    let mut a = begin(recipe, start);
    a.observe_ground(start + Vec3::Z * 0.01);
    assert!(!candidate_closes(&a));
    let mut rows = Vec::new();
    for i in 1..=120 {
        let t = std::f32::consts::TAU * i as f32 / 120.;
        a.observe_ground(Vec3::ONE + Vec3::new(t.cos(), 0., t.sin()) * 0.4);
        validate(&mut a, false);
        publish(&mut a, &mut rows);
    }
    assert!(candidate_closes(&a));
    let checkpoint = a.continuity.expected();
    // Move back onto a point outside the capture zone, not a second loop.
    a.observe_ground(Vec3::ONE + Vec3::new(5.7_f32.cos(), 0., 5.7_f32.sin()) * 0.4);
    assert!(!candidate_closes(&a));
    assert_eq!(a.continuity.expected(), checkpoint);
    a.observe_ground(start);
    assert!(candidate_closes(&a));
    assert_eq!(a.continuity.expected(), checkpoint);
}
