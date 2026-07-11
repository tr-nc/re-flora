use glam::Vec3;

/// Builds a deterministic orthonormal basis perpendicular to `dir` without axis-threshold flips.
///
/// The returned vectors are both perpendicular to `dir` and to each other. This uses the
/// branch-free Duff et al. form of Frisvad's basis construction so a smoothly changing direction
/// yields smoothly changing basis vectors.
pub fn stable_perpendicular_basis(dir: Vec3) -> (Vec3, Vec3) {
    let normal = dir.normalize_or_zero();
    if normal == Vec3::ZERO {
        return (Vec3::X, Vec3::Z);
    }

    let sign = if normal.z >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (sign + normal.z);
    let b = normal.x * normal.y * a;
    let tangent = Vec3::new(
        1.0 + sign * normal.x * normal.x * a,
        sign * b,
        -sign * normal.x,
    )
    .normalize_or_zero();
    let bitangent = Vec3::new(b, sign + normal.y * normal.y * a, -normal.y).normalize_or_zero();

    (tangent, bitangent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_perpendicular_basis_is_orthonormal() {
        let dir = Vec3::new(0.31, 0.92, -0.24);
        let normal = dir.normalize();
        let (tangent, bitangent) = stable_perpendicular_basis(dir);

        assert!((tangent.length() - 1.0).abs() < 1.0e-5);
        assert!((bitangent.length() - 1.0).abs() < 1.0e-5);
        assert!(tangent.dot(normal).abs() < 1.0e-5);
        assert!(bitangent.dot(normal).abs() < 1.0e-5);
        assert!(tangent.dot(bitangent).abs() < 1.0e-5);
    }

    #[test]
    fn stable_perpendicular_basis_does_not_flip_at_vertical_thresholds() {
        let below = Vec3::new((1.0_f32 - 0.899 * 0.899).sqrt(), 0.899, 0.0);
        let above = Vec3::new((1.0_f32 - 0.901 * 0.901).sqrt(), 0.901, 0.0);
        let (below_tangent, below_bitangent) = stable_perpendicular_basis(below);
        let (above_tangent, above_bitangent) = stable_perpendicular_basis(above);

        assert!(below_tangent.dot(above_tangent) > 0.99);
        assert!(below_bitangent.dot(above_bitangent) > 0.99);
    }
}
