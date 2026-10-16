//! Pure interaction/compiler seams. Native reviews add actual support/atlas validation.
use super::*;
fn active() -> ActivePavement {
    super::tests::pending_pavement_runtime().active.unwrap()
}
fn approve_geometry(a: &mut ActivePavement, p: Vec3) {
    a.observe_ground(p);
    assert!(a.continuity.blocked().is_none());
    a.fix_candidate();
}

// Exercise the real interaction seam, including the complete provisional tail,
// at every mouse target. Native reviews add terrain support and GPU protection.
fn approve_complete_geometry(a: &mut ActivePavement, target: Vec3) {
    a.observe_ground(target);
    assert!(
        a.continuity.blocked().is_none(),
        "legal target {target:?} rejected: {:?}",
        a.continuity.blocked().map(|b| b.reason)
    );
    let Some(candidate) = a.candidate.as_ref() else {
        return;
    };
    let mut staged = PavementCoverage::default();
    for row in candidate.stroke.pending_rows() {
        assert!(row.valid, "invalid row {} at {target:?}", row.index);
        let pieces = a.recipe.pieces(row);
        assert!(!pieces.is_empty(), "empty row {} at {target:?}", row.index);
        for piece in pieces {
            assert!(a.coverage.accepts(piece) && staged.accepts(piece));
            staged.record(piece);
        }
    }
    if a.intent.spaced(a.stroke.config.pitch) {
        a.fix_candidate();
    }
    assert!(a.stroke.prepare());
    while let Some(row) = a.continuity.next(&mut a.stroke.rows) {
        for piece in a.recipe.pieces(row) {
            assert!(a.coverage.accepts(piece));
            a.coverage.record(piece);
        }
        a.continuity.accept(row);
    }
}

#[test]
fn three_lane_compressed_bends_accept_every_streaming_target() {
    for sign in [-1., 1.] {
        for steps in [18, 45, 180] {
            let mut a = active();
            a.recipe = PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42).into();
            a.stroke = PathStroke::new(a.recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
            for i in 1..=steps {
                let angle = std::f32::consts::FRAC_PI_2 * i as f32 / steps as f32;
                let p =
                    Vec3::ONE + Vec3::new(0.10 * angle.sin(), 0., sign * 0.10 * (1. - angle.cos()));
                approve_complete_geometry(&mut a, p);
            }
            if a.candidate.is_some() {
                a.fix_candidate(); // Release approves the last sub-spacing residual.
            }
            assert!(a.stroke.finish());
            assert!(a.stroke.rows.iter().all(|r| r.valid));
            assert_eq!(a.stroke.rows.len(), 4);
            assert!(
                a.stroke.rows.iter().all(|r| a.recipe.pieces(*r).len() >= 3),
                "every course retains all three lanes; outer lanes may be subdivided"
            );
        }
    }
}

#[test]
fn compressed_bend_after_publication_keeps_the_actual_prefix_and_exit() {
    for sign in [-1., 1.] {
        let mut a = active();
        a.recipe = PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42).into();
        a.stroke = PathStroke::new(a.recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
        for i in 1..=50 {
            approve_complete_geometry(&mut a, Vec3::ONE + Vec3::X * (i as f32 * 0.01));
        }
        let accepted = a
            .continuity
            .accepted()
            .expect("must publish before turning");
        let original = a.coverage.published_pieces().to_vec();
        let corner = Vec3::ONE + Vec3::X * 0.5;
        for i in 1..=90 {
            let angle = std::f32::consts::FRAC_PI_2 * i as f32 / 90.;
            approve_complete_geometry(
                &mut a,
                corner + Vec3::new(0.10 * angle.sin(), 0., sign * 0.10 * (1. - angle.cos())),
            );
            for (a, b) in original.iter().zip(a.coverage.published_pieces()) {
                assert_eq!(a.footprint, b.footprint);
                assert_eq!(a.seed, b.seed);
            }
        }
        if a.candidate.is_some() {
            a.fix_candidate();
        }
        assert!(a.stroke.finish());
        let mut previous = a.continuity.accepted().unwrap().exit;
        while let Some(row) = a.continuity.next(&mut a.stroke.rows) {
            assert_eq!(row.entry, previous);
            assert!(row.index > accepted.index);
            let pieces = a.recipe.pieces(row);
            assert!(!pieces.is_empty());
            for piece in pieces {
                assert!(a.coverage.accepts(piece));
                a.coverage.record(piece);
            }
            previous = row.exit;
            a.continuity.accept(row);
        }
        assert!(a.continuity.blocked().is_none());
    }
}

#[test]
fn short_turning_tail_accepts_targets_before_a_full_corner_can_fit() {
    for sign in [-1., 1.] {
        let mut a = active();
        a.recipe = PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42).into();
        a.stroke = PathStroke::new(a.recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
        let corner = Vec3::ONE + Vec3::X * 0.3;
        approve_complete_geometry(&mut a, corner);
        for i in 1..=40 {
            approve_complete_geometry(&mut a, corner + Vec3::Z * (sign * i as f32 * 0.005));
        }
    }
}

#[test]
fn discarded_targets_cannot_change_a_later_legal_candidate_or_its_seeds() {
    let mut clean = active();
    let mut noisy = active();
    let approved = Vec3::ONE + Vec3::X * 0.3;
    approve_geometry(&mut clean, approved);
    approve_geometry(&mut noisy, approved);
    for p in [
        Vec3::new(-0.3, 0., 0.01),
        Vec3::new(-0.2, 0., 0.1),
        Vec3::new(0.112, 0., 0.0684),
    ] {
        noisy.observe_ground(Vec3::ONE + p);
        assert!(noisy.continuity.blocked().is_some());
        assert_eq!(noisy.intent.floor(), 1);
        assert_eq!(noisy.stroke.pending_rows(), clean.stroke.pending_rows());
    }
    let legal = Vec3::ONE + Vec3::X * 0.65;
    clean.observe_ground(legal);
    noisy.observe_ground(legal); // No visit to the approved endpoint.
    let a = &clean.candidate.as_ref().unwrap().stroke;
    let b = &noisy.candidate.as_ref().unwrap().stroke;
    assert_eq!(a.pending_rows(), b.pending_rows());
    for (a, b) in a.pending_rows().into_iter().zip(b.pending_rows()) {
        let a = clean.recipe.pieces(a);
        let b = noisy.recipe.pieces(b);
        assert_eq!(a.len(), b.len());
        for (a, b) in a.into_iter().zip(b) {
            assert_eq!(a.footprint, b.footprint);
            assert_eq!(a.seed, b.seed);
        }
    }
}
#[test]
fn short_picked_risers_fit_without_projecting_away_the_cursor() {
    for delta in [
        Vec3::new(0.01, 0., 0.005),
        Vec3::new(0.01, 0., 0.009),
        Vec3::new(-0.001, 0., 0.005),
    ] {
        let mut a = active();
        a.recipe = PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42).into();
        a.stroke = PathStroke::new(a.recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
        let approved = Vec3::ONE + Vec3::X * 0.3;
        approve_geometry(&mut a, approved);
        let raw = approved + delta;
        approve_complete_geometry(&mut a, raw);
        let candidate = a.candidate.as_ref().unwrap();
        assert_eq!(candidate.endpoint, raw);
        assert!(candidate.stroke.pending_rows().iter().all(|r| r.valid));
        assert_eq!(a.intent.anchor(), approved);
    }
}
#[test]
fn stationary_approved_cursor_does_not_replan_or_repeat_motion() {
    let mut a = active();
    let p = Vec3::ONE + Vec3::X * 0.3;
    approve_geometry(&mut a, p);
    assert!(a.intent.take_motion());
    for _ in 0..10 {
        a.observe_ground(p);
        assert!(a.candidate.is_none());
        assert!(!a.intent.take_motion());
    }
}
#[test]
fn enormous_rejected_target_does_not_poison_the_next_finite_candidate() {
    let mut a = active();
    approve_geometry(&mut a, Vec3::ONE + Vec3::X * 0.3);
    let approved = a.stroke.pending_rows();
    a.observe_ground(Vec3::splat(f32::MAX));
    assert_eq!(
        a.continuity.blocked().unwrap().reason,
        BlockReason::Sampling
    );
    assert_eq!(a.stroke.pending_rows(), approved);
    a.observe_ground(Vec3::ONE + Vec3::X * 0.6);
    assert!(a.continuity.blocked().is_none());
    assert_eq!(a.intent.floor(), 1);
}
#[test]
fn release_geometry_ignores_invalid_current_target_and_keeps_last_approved_end() {
    let mut a = active();
    approve_geometry(&mut a, Vec3::ONE + Vec3::X * 0.3);
    a.observe_ground(Vec3::ONE + Vec3::new(-0.2, 0., 0.05));
    assert!(a.continuity.blocked().is_some());
    let mut finish = a.stroke.clone();
    assert!(finish.finish());
    assert!(finish.rows.iter().all(|r| r.valid));
    assert!(finish
        .rows
        .iter()
        .all(|r| r.center.z == 1. && r.center.x <= 1.301));
}
