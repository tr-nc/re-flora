use glam::{UVec2, Vec2, Vec3};

fn sign_not_zero(value: f32) -> f32 {
    if value >= 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// Encodes a Y-up unit direction into the paper's `[-1, 1]` octahedral domain.
pub fn oct_encode(direction: Vec3) -> Vec2 {
    let direction = direction.normalize_or_zero();
    if direction == Vec3::ZERO {
        return Vec2::ZERO;
    }

    let mut encoded = Vec2::new(direction.x, direction.z)
        / (direction.x.abs() + direction.y.abs() + direction.z.abs());
    if direction.y < 0.0 {
        let unfolded = encoded;
        encoded.x = (1.0 - unfolded.y.abs()) * sign_not_zero(unfolded.x);
        encoded.y = (1.0 - unfolded.x.abs()) * sign_not_zero(unfolded.y);
    }
    encoded
}

/// Decodes a point in the paper's `[-1, 1]` octahedral domain into a Y-up unit direction.
pub fn oct_decode(encoded: Vec2) -> Vec3 {
    let mut direction = Vec3::new(
        encoded.x,
        1.0 - encoded.x.abs() - encoded.y.abs(),
        encoded.y,
    );
    if direction.y < 0.0 {
        let unfolded_x = direction.x;
        let unfolded_z = direction.z;
        direction.x = (1.0 - unfolded_z.abs()) * sign_not_zero(unfolded_x);
        direction.z = (1.0 - unfolded_x.abs()) * sign_not_zero(unfolded_z);
    }
    direction.normalize_or_zero()
}

/// Returns the interior texel copied into a one-texel octahedral gutter coordinate.
///
/// Both arguments use stored-tile coordinates: interior texels span `1..=interior_side`, while
/// the gutter lies at zero and `interior_side + 1`. Interior coordinates return `None`.
pub fn octahedral_gutter_source(interior_side: u32, destination: UVec2) -> Option<UVec2> {
    if interior_side == 0 {
        return None;
    }
    let last = interior_side + 1;
    if destination.x > last || destination.y > last {
        return None;
    }

    match (destination.x, destination.y) {
        (0, 0) => Some(UVec2::new(interior_side, interior_side)),
        (x, 0) if x == last => Some(UVec2::new(1, interior_side)),
        (0, y) if y == last => Some(UVec2::new(interior_side, 1)),
        (x, y) if x == last && y == last => Some(UVec2::ONE),
        (0, y) if (1..=interior_side).contains(&y) => Some(UVec2::new(1, last - y)),
        (x, y) if x == last && (1..=interior_side).contains(&y) => {
            Some(UVec2::new(interior_side, last - y))
        }
        (x, 0) if (1..=interior_side).contains(&x) => Some(UVec2::new(last - x, 1)),
        (x, y) if y == last && (1..=interior_side).contains(&x) => {
            Some(UVec2::new(last - x, interior_side))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octahedral_axes_round_trip() {
        for direction in [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            Vec3::NEG_X,
            Vec3::NEG_Y,
            Vec3::NEG_Z,
        ] {
            assert!(oct_decode(oct_encode(direction)).dot(direction) > 0.999_999);
        }
    }

    #[test]
    fn octahedral_fibonacci_directions_round_trip() {
        let golden_angle = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
        for index in 0..4_096 {
            let y = 1.0 - (2.0 * index as f32 + 1.0) / 4_096.0;
            let radius = (1.0 - y * y).sqrt();
            let azimuth = golden_angle * index as f32;
            let direction = Vec3::new(radius * azimuth.cos(), y, radius * azimuth.sin());
            assert!(oct_decode(oct_encode(direction)).dot(direction) > 0.999_99);
        }
    }

    #[test]
    fn gutter_edges_follow_octahedral_mirror_rule() {
        let side = 4;
        assert_eq!(
            octahedral_gutter_source(side, UVec2::new(0, 1)),
            Some(UVec2::new(1, 4))
        );
        assert_eq!(
            octahedral_gutter_source(side, UVec2::new(0, 4)),
            Some(UVec2::new(1, 1))
        );
        assert_eq!(
            octahedral_gutter_source(side, UVec2::new(5, 1)),
            Some(UVec2::new(4, 4))
        );
        assert_eq!(
            octahedral_gutter_source(side, UVec2::new(5, 4)),
            Some(UVec2::new(4, 1))
        );
        assert_eq!(
            octahedral_gutter_source(side, UVec2::new(1, 0)),
            Some(UVec2::new(4, 1))
        );
        assert_eq!(
            octahedral_gutter_source(side, UVec2::new(4, 0)),
            Some(UVec2::new(1, 1))
        );
        assert_eq!(
            octahedral_gutter_source(side, UVec2::new(1, 5)),
            Some(UVec2::new(4, 4))
        );
        assert_eq!(
            octahedral_gutter_source(side, UVec2::new(4, 5)),
            Some(UVec2::new(1, 4))
        );
    }

    #[test]
    fn gutter_corners_wrap_to_diagonally_opposite_interior_corners() {
        let side = 8;
        assert_eq!(
            octahedral_gutter_source(side, UVec2::ZERO),
            Some(UVec2::splat(8))
        );
        assert_eq!(
            octahedral_gutter_source(side, UVec2::new(9, 0)),
            Some(UVec2::new(1, 8))
        );
        assert_eq!(
            octahedral_gutter_source(side, UVec2::new(0, 9)),
            Some(UVec2::new(8, 1))
        );
        assert_eq!(
            octahedral_gutter_source(side, UVec2::splat(9)),
            Some(UVec2::ONE)
        );
        assert_eq!(octahedral_gutter_source(side, UVec2::new(4, 4)), None);
        assert_eq!(octahedral_gutter_source(side, UVec2::new(10, 4)), None);
    }

    #[test]
    fn synthetic_directional_field_gutter_has_expected_values() {
        let side = 8;
        let field = |coordinate: UVec2| coordinate.x as i32 + 100 * coordinate.y as i32;
        for destination in (0..side + 2).flat_map(|y| (0..side + 2).map(move |x| UVec2::new(x, y)))
        {
            if let Some(source) = octahedral_gutter_source(side, destination) {
                let copied_value = field(source);
                assert_eq!(copied_value, source.x as i32 + 100 * source.y as i32);
                assert!((1..=side).contains(&source.x));
                assert!((1..=side).contains(&source.y));
            }
        }
    }
}
