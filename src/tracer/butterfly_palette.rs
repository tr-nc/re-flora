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
            border: [48, 30, 8, 255],
            dark_shade: [176, 122, 22, 255],
            mid_shade: [232, 185, 48, 255],
            light_shade: [255, 233, 140, 255],
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
            border: [55, 24, 8, 255],
            dark_shade: [160, 68, 18, 255],
            mid_shade: [225, 118, 32, 255],
            light_shade: [255, 190, 90, 255],
        }
    }

    pub fn white() -> Self {
        Self {
            border: [36, 38, 42, 255],
            dark_shade: [130, 135, 145, 255],
            mid_shade: [205, 210, 218, 255],
            light_shade: [250, 248, 235, 255],
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
            border: [8, 18, 55, 255],
            dark_shade: [30, 70, 150, 255],
            mid_shade: [70, 130, 220, 255],
            light_shade: [160, 210, 255, 255],
        }
    }

    pub fn brown() -> Self {
        Self {
            border: [38, 22, 10, 255],
            dark_shade: [105, 62, 28, 255],
            mid_shade: [165, 105, 50, 255],
            light_shade: [225, 170, 95, 255],
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

    println!("detected color mode for {}: palette", path_str);
    println!("palette for {} ({} colors):", path_str, used_colors.len());
    for color in &used_colors {
        println!(
            "#{:02X}{:02X}{:02X}{:02X} (r={}, g={}, b={}, a={})",
            color[0], color[1], color[2], color[3], color[0], color[1], color[2], color[3]
        );
    }

    let source_roles = infer_role_order_5(&used_colors, &path_str);

    println!("source role mapping:");
    println!("  transparent: {:02X?}", source_roles[0]);
    println!("  border:      {:02X?}", source_roles[1]);
    println!("  dark_shade: {:02X?}", source_roles[2]);
    println!("  mid_shade:  {:02X?}", source_roles[3]);
    println!("  light_shade: {:02X?}", source_roles[4]);

    println!("target config mapping:");
    println!("  transparent: {:02X?}", [0, 0, 0, 0]);
    println!("  border:      {:02X?}", target_config.border);
    println!("  dark_shade: {:02X?}", target_config.dark_shade);
    println!("  mid_shade:  {:02X?}", target_config.mid_shade);
    println!("  light_shade: {:02X?}", target_config.light_shade);

    let target_roles = target_config.into_role_array();
    let remapped = remap_palette(&source_roles, &target_roles, &rgba);

    println!("palette remapped for {}", path_str);

    remapped
}
