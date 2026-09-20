//! Authored per-voxel terrain appearance. No voxel/save data or raster-flora state.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainMaterialParams {
    pub enabled: bool,
    pub soil_strength: f32,
    pub rock_strength: f32,
    pub color_band: f32,
    pub seed: u32,
}

impl Default for TerrainMaterialParams {
    fn default() -> Self {
        Self {
            enabled: false,
            soil_strength: 0.35,
            rock_strength: 0.32,
            color_band: 0.25,
            seed: 17,
        }
    }
}

/// Matches both reflected palette uniforms; no additional descriptor is needed.
pub(crate) struct TerrainMaterialGpuParams {
    pub variation: [f32; 4],
    pub seed: u32,
    pub enabled: u32,
}

impl TerrainMaterialParams {
    pub(crate) fn normalized(self) -> Self {
        fn finite_range(value: f32, fallback: f32, max: f32) -> f32 {
            if value.is_finite() {
                value.clamp(0.0, max)
            } else {
                fallback
            }
        }
        let defaults = Self::default();
        Self {
            soil_strength: finite_range(self.soil_strength, defaults.soil_strength, 0.75),
            rock_strength: finite_range(self.rock_strength, defaults.rock_strength, 0.75),
            color_band: finite_range(self.color_band, defaults.color_band, 1.0),
            ..self
        }
    }

    pub(crate) fn gpu(self) -> TerrainMaterialGpuParams {
        let value = self.normalized();
        TerrainMaterialGpuParams {
            variation: [
                value.soil_strength,
                value.rock_strength,
                value.color_band,
                0.0,
            ],
            seed: value.seed,
            enabled: u32::from(value.enabled),
        }
    }

    pub(crate) fn identity(self) -> [u32; 5] {
        let value = self.normalized();
        [
            u32::from(value.enabled),
            value.soil_strength.to_bits(),
            value.rock_strength.to_bits(),
            value.color_band.to_bits(),
            value.seed,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_inputs_cannot_create_nonfinite_shader_values() {
        let params = TerrainMaterialParams {
            soil_strength: -1.0,
            rock_strength: f32::INFINITY,
            color_band: 999.0,
            ..TerrainMaterialParams::default()
        }
        .normalized();
        assert_eq!(params.soil_strength, 0.0);
        assert_eq!(params.rock_strength, 0.32);
        assert_eq!(params.color_band, 1.0);
        assert!(params.gpu().variation.into_iter().all(f32::is_finite));
    }

    #[test]
    fn uniform_encoding_keeps_strength_band_seed_and_toggle() {
        let params = TerrainMaterialParams {
            enabled: true,
            seed: u32::MAX,
            ..TerrainMaterialParams::default()
        };
        let gpu = params.gpu();
        assert_eq!(gpu.variation, [0.35, 0.32, 0.25, 0.0]);
        assert_eq!(gpu.seed, u32::MAX);
        assert_eq!(gpu.enabled, 1);
        assert_eq!(TerrainMaterialParams::default().gpu().enabled, 0);
    }
}
