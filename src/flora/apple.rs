//! Shared size policy for attached/fallen apple models, shadows and collision.
pub fn normalize_size_scale(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.25, 2.0)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attached_color_and_shadow_share_the_size_scale() {
        let color = include_str!("../../shader/slang/apple_pixel_tree_pose.slang");
        let shadow = include_str!("../../shader/slang/leaves_shadow.vert.slang");
        assert!(color.contains("output.scale=") && color.contains("*gui_input.apple_size_scale;"));
        assert!(shadow.contains("applePreviewPosition(input.packed_data)*"));
        assert!(shadow.contains("*gui_input.apple_size_scale;"));
        // Both native and pre-cache color consumers use this same physical pose.
        for shader in [
            include_str!("../../shader/slang/apple_mesh_tree.vert.slang"),
            include_str!("../../shader/slang/apple_pixel_tree.vert.slang"),
        ] {
            assert!(shader.contains("appleTreePose("));
        }
    }

    #[test]
    fn size_scale_is_positive_bounded_and_has_a_safe_default() {
        for (value, expected) in [(0., 0.25), (0.7, 0.7), (1., 1.), (3., 2.), (f32::NAN, 1.)] {
            assert_eq!(normalize_size_scale(value), expected);
        }
    }
}
