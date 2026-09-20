//! Stable butterfly base-color presets shared by ecology and the live wing mesh.
//! No sprite palette roles, outline colors, or image remapping remain.
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

    /// Existing garden mix; purple/red remain available through their stable IDs.
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

    /// Preserve the accepted mesh renderer's seven base colors exactly.
    pub fn base_color_srgb(self) -> [u8; 3] {
        match self {
            Self::Yellow => [231, 220, 118],
            Self::Purple => [145, 82, 190],
            Self::Orange => [202, 133, 60],
            Self::White => [231, 229, 207],
            Self::Red => [205, 48, 54],
            Self::Blue => [112, 153, 201],
            Self::Brown => [141, 116, 79],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garden_mix_and_all_seven_mesh_colors_stay_unchanged() {
        let mut counts = [0; 7];
        for roll in 0..ButterflyPalettePreset::GARDEN_ROLL_COUNT {
            counts[ButterflyPalettePreset::from_garden_roll(roll) as usize] += 1;
        }
        assert_eq!(counts, [25, 0, 20, 35, 0, 5, 15]);
        let colors = (0..ButterflyPalettePreset::COUNT)
            .map(|id| ButterflyPalettePreset::from_index(id).base_color_srgb())
            .collect::<Vec<_>>();
        assert_eq!(
            colors,
            [
                [231, 220, 118],
                [145, 82, 190],
                [202, 133, 60],
                [231, 229, 207],
                [205, 48, 54],
                [112, 153, 201],
                [141, 116, 79]
            ]
        );
    }
}
