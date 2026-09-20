//! Authored per-voxel terrain appearance. No voxel/save data or raster-flora state.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainMaterialParams {
    pub soil_strength: f32,
    pub rock_strength: f32,
    pub seed: u32,
}

impl Default for TerrainMaterialParams {
    fn default() -> Self {
        Self {
            soil_strength: 0.135,
            rock_strength: 0.32,
            seed: 17,
        }
    }
}

/// Matches both reflected palette uniforms; no additional descriptor is needed.
pub(crate) struct TerrainMaterialGpuParams {
    pub variation: [f32; 2],
    pub seed: u32,
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
            ..self
        }
    }

    pub(crate) fn gpu(self) -> TerrainMaterialGpuParams {
        let value = self.normalized();
        TerrainMaterialGpuParams {
            variation: [value.soil_strength, value.rock_strength],
            seed: value.seed,
        }
    }

    pub(crate) fn identity(self) -> [u32; 3] {
        let value = self.normalized();
        [
            value.soil_strength.to_bits(),
            value.rock_strength.to_bits(),
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
            ..TerrainMaterialParams::default()
        }
        .normalized();
        assert_eq!(params.soil_strength, 0.0);
        assert_eq!(params.rock_strength, 0.32);
        assert!(params.gpu().variation.into_iter().all(f32::is_finite));
    }

    #[test]
    fn uniform_encoding_keeps_strength_and_seed() {
        let params = TerrainMaterialParams {
            seed: u32::MAX,
            ..TerrainMaterialParams::default()
        };
        let gpu = params.gpu();
        assert_eq!(gpu.variation, [0.135, 0.32]);
        assert_eq!(gpu.seed, u32::MAX);
    }
}
