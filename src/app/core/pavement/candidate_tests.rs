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
fn short_picked_riser_can_use_only_a_forward_two_voxel_input_proposal() {
    for (delta, assisted) in [
        (Vec3::new(0.01, 0., 0.005), true),
        (Vec3::new(0.01, 0., 0.009), false),
        (Vec3::new(-0.001, 0., 0.005), false),
    ] {
        let mut a = active();
        a.recipe = PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42);
        a.stroke = PathStroke::new(a.recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
        let approved = Vec3::ONE + Vec3::X * 0.3;
        approve_geometry(&mut a, approved);
        let raw = approved + delta;
        let mut unassisted = a.stroke.clone();
        assert!(unassisted.push(raw));
        assert!(
            unassisted.pending_rows().iter().any(|r| !r.valid),
            "fixture must trigger assistance"
        );
        a.observe_ground(raw);
        let candidate = a.candidate.as_ref().unwrap();
        assert_eq!(a.continuity.blocked().is_none(), assisted);
        assert!((candidate.endpoint - raw).length() <= 2. * VOXEL_SIZE + 1e-6);
        if assisted {
            assert!((candidate.endpoint - (approved + Vec3::X * 0.01)).length() < 1e-6);
            assert!(candidate.stroke.pending_rows().iter().all(|r| r.valid));
        } else {
            assert_eq!(candidate.endpoint, raw);
        }
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
