//! Shared apple size policy and opt-in surface experiments.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Appearance {
    pub directional_lighting: bool,
    pub light_bands: bool,
    pub color_patches: bool,
    pub color_stripes: bool,
    pub sun_gloss: bool,
    pub environment_fill: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            directional_lighting: false,
            light_bands: false,
            color_patches: false,
            color_stripes: false,
            sun_gloss: false,
            environment_fill: 1.0,
        }
    }
}

impl Appearance {
    pub fn shader_flags(self) -> u32 {
        u32::from(self.directional_lighting)
            | (u32::from(self.directional_lighting && self.light_bands) << 1)
            | (u32::from(self.color_patches) << 2)
            | (u32::from(self.color_stripes) << 3)
            | (u32::from(self.sun_gloss) << 4)
    }

    pub fn normalized_environment_fill(self) -> f32 {
        if self.environment_fill.is_finite() {
            self.environment_fill.clamp(0.0, 1.5)
        } else {
            1.0
        }
    }
}
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
    fn experiments_default_to_original_and_flags_match_shader_contract() {
        let mut appearance = Appearance::default();
        assert_eq!(appearance.shader_flags(), 0);
        assert_eq!(appearance.normalized_environment_fill(), 1.);
        appearance.light_bands = true;
        assert_eq!(
            appearance.shader_flags(),
            0,
            "light bands require directional sunlight"
        );
        appearance.directional_lighting = true;
        assert_eq!(appearance.shader_flags(), 3);
        appearance.color_patches = true;
        appearance.color_stripes = true;
        appearance.sun_gloss = true;
        assert_eq!(appearance.shader_flags(), 31);
        appearance.environment_fill = f32::NAN;
        assert_eq!(appearance.normalized_environment_fill(), 1.);
        appearance.environment_fill = -1.;
        assert_eq!(appearance.normalized_environment_fill(), 0.);
        let shader = include_str!("../../shader/slang/apple_appearance.slang");
        for (name, value) in [
            ("DIRECTIONAL_LIGHTING", 1),
            ("LIGHT_BANDS", 2),
            ("COLOR_PATCHES", 4),
            ("COLOR_STRIPES", 8),
            ("SUN_GLOSS", 16),
        ] {
            assert!(shader.contains(&format!("APPLE_{name} = {value}u;")));
        }
        assert!(shader.contains("float facing=saturate(dot(n,sunDirection));"));
        assert!(shader.contains("environmentLight*=gui.apple_environment_fill"));
        assert!(shader.contains("sunTransmittance*specular*saturate(facing*4.0)"));
    }

    #[test]
    fn native_and_cached_apples_use_the_same_surface_relighting() {
        let mesh = include_str!("../../shader/slang/model_mesh.slang");
        assert!(
            mesh.contains("shadeAppleSurface("),
            "apple surfaces need their own switchable normal-based relighting"
        );
        assert!(
            mesh.contains("cached.localPosition"),
            "peel coordinates must come from the baked canonical surface, not the camera quad"
        );
        assert!(
            mesh.contains("cached.position,cached.normal"),
            "cached depth position and normal must feed live lighting"
        );
    }

    #[test]
    fn size_scale_is_positive_bounded_and_has_a_safe_default() {
        for (value, expected) in [(0., 0.25), (0.7, 0.7), (1., 1.), (3., 2.), (f32::NAN, 1.)] {
            assert_eq!(normalize_size_scale(value), expected);
        }
    }
}
