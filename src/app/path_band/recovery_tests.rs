use super::*;

fn settings(sampling: PathSampling) -> PathSettings {
    PathSettings {
        width: 0.06,
        pitch: 0.05,
        sampling,
    }
}

#[test]
fn recovery_uses_the_accepted_exit_not_the_furthest_generated_course() {
    let config = settings(PathSampling::Courses);
    let mut stroke = PathStroke::new(config, Vec3::ONE, Vec3::X).unwrap();
    assert!(stroke.push(Vec3::ONE + Vec3::X * 0.8));
    assert!(stroke.prepare());
    let accepted = stroke.rows[2];
    assert!(stroke.rows.back().unwrap().index > accepted.index + 4);
    let anchor = accepted.exit.center;
    assert!(stroke.replace_tail(Some(accepted), &[anchor, anchor + Vec3::X * 0.4]));
    assert!(stroke.finish());
    let next = stroke.rows[0];
    assert_eq!(next.index, accepted.index + 1);
    assert_eq!(next.entry, accepted.exit);
    assert!((next.distance - accepted.distance - config.pitch).abs() < 1e-6);
    assert!(stroke.rows.iter().all(|r| r.valid));
    assert!(stroke
        .rows
        .iter()
        .zip(stroke.rows.iter().skip(1))
        .all(|(a, b)| a.exit == b.entry && b.index == a.index + 1));
}

#[test]
fn returning_to_an_accepted_end_does_not_build_an_extra_stationary_course() {
    let config = settings(PathSampling::Courses);
    let mut stroke = PathStroke::new(config, Vec3::ONE, Vec3::X).unwrap();
    stroke.push(Vec3::ONE + Vec3::X * 0.5);
    stroke.prepare();
    let accepted = stroke.rows[3];
    for _ in 0..4 {
        assert!(stroke.replace_tail(Some(accepted), &[accepted.exit.center]));
        assert!(stroke.pending_rows().is_empty());
        assert!(stroke.finish());
        assert!(stroke.rows.is_empty());
    }
}

#[test]
fn curved_accepted_exit_preserves_its_actual_cross_section_and_tangent() {
    let config = settings(PathSampling::Courses);
    let mut stroke = PathStroke::new(config, Vec3::ONE, Vec3::X).unwrap();
    stroke.push(Vec3::ONE + Vec3::X * 0.4);
    stroke.push(Vec3::ONE + Vec3::new(0.4, 0., 0.4));
    stroke.prepare();
    let accepted = *stroke
        .rows
        .iter()
        .find(|r| r.valid && r.forward.z > 0.4 && r.forward.x > 0.4)
        .unwrap();
    let anchor = accepted.exit.center;
    let tangent = Vec3::new(-accepted.exit.across.z, 0., accepted.exit.across.x);
    assert!(stroke.replace_tail(Some(accepted), &[anchor, anchor + tangent * 0.4]));
    stroke.finish();
    assert_eq!(stroke.rows[0].entry, accepted.exit);
    assert!(stroke.rows.iter().all(|r| r.valid));
}

#[test]
fn recovery_does_not_treat_the_frozen_entry_as_a_free_sharp_new_start() {
    let config = settings(PathSampling::Courses);
    let accepted = PathSample::straight(Vec3::ONE, Vec3::X, 0.3, 6, config.pitch);
    let mut stroke = PathStroke::new(config, Vec3::ONE, Vec3::X).unwrap();
    let anchor = accepted.exit.center;
    stroke.replace_tail(Some(accepted), &[anchor, anchor + Vec3::Z * 0.4]);
    stroke.finish();
    assert_eq!(stroke.rows[0].entry, accepted.exit);
    assert!(
        !stroke.rows[0].valid,
        "a return cannot bypass the same minimum path-band radius"
    );
}

#[test]
fn dirt_recovery_continues_distance_sampling_without_repeating_the_accepted_dab() {
    let config = settings(PathSampling::Surface);
    let mut stroke = PathStroke::new(config, Vec3::ONE, Vec3::X).unwrap();
    stroke.push(Vec3::ONE + Vec3::X * 0.5);
    let accepted = stroke.rows[2];
    let anchor = accepted.center;
    stroke.replace_tail(Some(accepted), &[anchor, anchor + Vec3::X * 0.13]);
    stroke.finish();
    assert_eq!(stroke.rows[0].index, accepted.index + 1);
    assert!(stroke.rows.iter().all(|r| r.center.distance(anchor) > 1e-6));
    assert!((stroke.rows[0].distance - accepted.distance - config.pitch).abs() < 1e-6);
    assert!(
        stroke
            .rows
            .back()
            .unwrap()
            .center
            .distance(anchor + Vec3::X * 0.13)
            < 1e-5
    );
}
