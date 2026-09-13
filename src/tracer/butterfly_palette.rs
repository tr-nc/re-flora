use crate::tracer::palette_remap::{
    collect_used_colors, detect_png_color_mode, infer_role_order_5, remap_palette, PaletteColor,
};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ButterflyPaletteRole {
    Transparent,
    Border,
    DarkShade,
    MidShade,
    LightShade,
}

impl ButterflyPaletteRole {
    #[allow(dead_code)]
    pub const ROLE_ORDER: [ButterflyPaletteRole; 5] = [
        ButterflyPaletteRole::Transparent,
        ButterflyPaletteRole::Border,
        ButterflyPaletteRole::DarkShade,
        ButterflyPaletteRole::MidShade,
        ButterflyPaletteRole::LightShade,
    ];
}

#[derive(Debug, Clone, Copy)]
pub struct ButterflyPaletteConfig {
    pub border: PaletteColor,
    pub dark_shade: PaletteColor,
    pub mid_shade: PaletteColor,
    pub light_shade: PaletteColor,
}

impl ButterflyPaletteConfig {
    pub fn into_role_array(self) -> [PaletteColor; 5] {
        [
            [0, 0, 0, 0],
            self.border,
            self.dark_shade,
            self.mid_shade,
            self.light_shade,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ButterflyPalettePreset {
    Yellow = 0,
    Purple = 1,
    Orange = 2,
    White = 3,
    Red = 4,
    Blue = 5,
    Brown = 6,
}

impl ButterflyPalettePreset {
    pub const COUNT: u32 = 7;
    pub const GARDEN_ROLL_COUNT: u32 = 100;

    /// Art-directed starter mix, not a measured ecological distribution.
    /// Keep texture IDs stable; purple/red remain available to existing IDs.
    pub fn from_garden_roll(roll: u32) -> Self {
        match roll {
            0..35 => Self::White,
            35..60 => Self::Yellow,
            60..80 => Self::Orange,
            80..95 => Self::Brown,
            95..100 => Self::Blue,
            _ => panic!("butterfly garden roll out of range: {roll}"),
        }
    }

    pub fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Yellow,
            1 => Self::Purple,
            2 => Self::Orange,
            3 => Self::White,
            4 => Self::Red,
            5 => Self::Blue,
            6 => Self::Brown,
            _ => Self::Yellow,
        }
    }

    pub fn config(&self) -> ButterflyPaletteConfig {
        match self {
            Self::Yellow => ButterflyPaletteConfig::yellow(),
            Self::Purple => ButterflyPaletteConfig::purple(),
            Self::Orange => ButterflyPaletteConfig::orange(),
            Self::White => ButterflyPaletteConfig::white(),
            Self::Red => ButterflyPaletteConfig::red(),
            Self::Blue => ButterflyPaletteConfig::blue(),
            Self::Brown => ButterflyPaletteConfig::brown(),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Purple => "purple",
            Self::Orange => "orange",
            Self::White => "white",
            Self::Red => "red",
            Self::Blue => "blue",
            Self::Brown => "brown",
        }
    }
}

impl ButterflyPaletteConfig {
    pub fn yellow() -> Self {
        Self {
            border: [49, 46, 28, 255],
            dark_shade: [155, 149, 76, 255],
            mid_shade: [231, 220, 118, 255],
            light_shade: [251, 244, 181, 255],
        }
    }

    pub fn purple() -> Self {
        Self {
            border: [30, 12, 45, 255],
            dark_shade: [85, 40, 125, 255],
            mid_shade: [145, 82, 190, 255],
            light_shade: [220, 175, 245, 255],
        }
    }

    pub fn orange() -> Self {
        Self {
            border: [46, 31, 22, 255],
            dark_shade: [118, 69, 37, 255],
            mid_shade: [202, 133, 60, 255],
            light_shade: [238, 191, 115, 255],
        }
    }

    pub fn white() -> Self {
        Self {
            border: [48, 47, 41, 255],
            dark_shade: [154, 153, 137, 255],
            mid_shade: [231, 229, 207, 255],
            light_shade: [251, 249, 235, 255],
        }
    }

    pub fn red() -> Self {
        Self {
            border: [50, 8, 10, 255],
            dark_shade: [135, 24, 28, 255],
            mid_shade: [205, 48, 54, 255],
            light_shade: [255, 132, 120, 255],
        }
    }

    pub fn blue() -> Self {
        Self {
            border: [40, 42, 49, 255],
            dark_shade: [60, 87, 131, 255],
            mid_shade: [112, 153, 201, 255],
            light_shade: [213, 224, 233, 255],
        }
    }

    pub fn brown() -> Self {
        Self {
            border: [40, 32, 25, 255],
            dark_shade: [84, 66, 46, 255],
            mid_shade: [141, 116, 79, 255],
            light_shade: [207, 187, 143, 255],
        }
    }
}

pub fn load_butterfly_and_remap(
    path: &Path,
    target_config: &ButterflyPaletteConfig,
) -> image::RgbaImage {
    let path_str = path.to_string_lossy().to_string();

    let color_mode = detect_png_color_mode(path);
    assert!(
        color_mode == Some("palette"),
        "Butterfly atlas '{}' must be in indexed palette mode, got: {:?}",
        path_str,
        color_mode
    );

    let img = image::open(path)
        .unwrap_or_else(|e| panic!("Failed to open butterfly atlas '{}': {}", path_str, e));

    let rgba = img.to_rgba8();
    let used_colors = collect_used_colors(&rgba);
    let source_roles = infer_role_order_5(&used_colors, &path_str);

    let target_roles = target_config.into_role_array();
    remap_palette(&source_roles, &target_roles, &rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garden_palette_rolls_have_exact_authored_weights_and_stable_ids() {
        let mut counts = [0; ButterflyPalettePreset::COUNT as usize];
        for roll in 0..ButterflyPalettePreset::GARDEN_ROLL_COUNT {
            let preset = ButterflyPalettePreset::from_garden_roll(roll);
            assert_eq!(ButterflyPalettePreset::from_index(preset as u32), preset);
            counts[preset as usize] += 1;
        }
        assert_eq!(counts, [25, 0, 20, 35, 0, 5, 15]);
    }

    #[test]
    fn garden_palette_preserves_transparency_and_ordered_shading_roles() {
        for index in 0..ButterflyPalettePreset::COUNT {
            let colors = ButterflyPalettePreset::from_index(index)
                .config()
                .into_role_array();
            assert_eq!(colors[0], [0; 4]);
            let mut previous_luminance = -1.0;
            for color in &colors[1..] {
                assert_eq!(color[3], 255);
                let luminance =
                    0.2126 * color[0] as f32 + 0.7152 * color[1] as f32 + 0.0722 * color[2] as f32;
                assert!(luminance > previous_luminance);
                previous_luminance = luminance;
            }
        }
    }
}
