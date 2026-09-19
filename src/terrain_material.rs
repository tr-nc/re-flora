//! Authored, world-space terrain appearance. No voxel/save data or raster-flora state.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainMaterialParams {
    pub enabled: bool,
    pub soil_scale_voxels: f32,
    pub soil_strength: f32,
    pub rock_scale_voxels: f32,
    pub rock_strength: f32,
    pub rock_layer_tilt_degrees: f32,
    pub color_band: f32,
    pub seed: u32,
}

impl Default for TerrainMaterialParams {
    fn default() -> Self {
        Self {
            enabled: false,
            soil_scale_voxels: 16.0,
            soil_strength: 0.35,
            rock_scale_voxels: 12.0,
            rock_strength: 0.32,
            rock_layer_tilt_degrees: 15.0,
            color_band: 0.25,
            seed: 17,
        }
    }
}

/// Flat fields match both reflected palette uniforms; no additional descriptor is needed.
pub(crate) struct TerrainMaterialGpuParams {
    pub soil: [f32; 4],
    pub rock: [f32; 4],
    pub seed: u32,
    pub enabled: u32,
}

impl TerrainMaterialParams {
    pub(crate) fn normalized(self) -> Self {
        fn finite_range(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        }
        let defaults = Self::default();
        Self {
            // At least eight voxels per feature: this first stage is deliberately macro-only.
            soil_scale_voxels: finite_range(
                self.soil_scale_voxels,
                defaults.soil_scale_voxels,
                8.0,
                128.0,
            ),
            soil_strength: finite_range(self.soil_strength, defaults.soil_strength, 0.0, 0.75),
            rock_scale_voxels: finite_range(
                self.rock_scale_voxels,
                defaults.rock_scale_voxels,
                8.0,
                128.0,
            ),
            rock_strength: finite_range(self.rock_strength, defaults.rock_strength, 0.0, 0.75),
            rock_layer_tilt_degrees: finite_range(
                self.rock_layer_tilt_degrees,
                defaults.rock_layer_tilt_degrees,
                -75.0,
                75.0,
            ),
            color_band: finite_range(self.color_band, defaults.color_band, 0.0, 1.0),
            ..self
        }
    }

    pub(crate) fn gpu(self) -> TerrainMaterialGpuParams {
        let value = self.normalized();
        let (sin_tilt, cos_tilt) = value.rock_layer_tilt_degrees.to_radians().sin_cos();
        TerrainMaterialGpuParams {
            soil: [
                value.soil_scale_voxels,
                value.soil_strength,
                value.color_band,
                0.0,
            ],
            rock: [
                value.rock_scale_voxels,
                value.rock_strength,
                sin_tilt,
                cos_tilt,
            ],
            seed: value.seed,
            enabled: u32::from(value.enabled),
        }
    }

    pub(crate) fn identity(self) -> [u32; 8] {
        let value = self.normalized();
        [
            u32::from(value.enabled),
            value.soil_scale_voxels.to_bits(),
            value.soil_strength.to_bits(),
            value.rock_scale_voxels.to_bits(),
            value.rock_strength.to_bits(),
            value.rock_layer_tilt_degrees.to_bits(),
            value.color_band.to_bits(),
            value.seed,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_inputs_cannot_create_high_frequency_or_nonfinite_shader_values() {
        let params = TerrainMaterialParams {
            soil_scale_voxels: 0.0,
            rock_scale_voxels: f32::NAN,
            soil_strength: -1.0,
            rock_strength: f32::INFINITY,
            rock_layer_tilt_degrees: 999.0,
            color_band: 999.0,
            ..TerrainMaterialParams::default()
        }
        .normalized();
        assert_eq!(params.soil_scale_voxels, 8.0);
        assert_eq!(params.rock_scale_voxels, 12.0);
        assert_eq!(params.soil_strength, 0.0);
        assert_eq!(params.rock_strength, 0.32);
        assert_eq!(params.rock_layer_tilt_degrees, 75.0);
        assert_eq!(params.color_band, 1.0);
        let gpu = params.gpu();
        assert!(gpu.soil.into_iter().chain(gpu.rock).all(f32::is_finite));
    }

    #[test]
    fn uniform_encoding_keeps_world_scale_ramp_seed_and_layer_normal() {
        let params = TerrainMaterialParams {
            seed: u32::MAX,
            ..TerrainMaterialParams::default()
        };
        let gpu = params.gpu();
        assert_eq!(gpu.soil, [16.0, 0.35, 0.25, 0.0]);
        assert_eq!(gpu.rock[..2], [12.0, 0.32]);
        assert!((gpu.rock[2].powi(2) + gpu.rock[3].powi(2) - 1.0).abs() < 1e-6);
        assert_eq!(gpu.seed, u32::MAX);
        assert_eq!(gpu.enabled, 0);
    }
}
