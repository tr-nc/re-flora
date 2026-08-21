use super::*;
use glam::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainHillField {
    pub center_voxels: Vec2,
    pub base_height_voxels: f32,
    pub radii_voxels: Vec2,
    pub rise_voxels: f32,
    pub maximum_inflation_voxels: f32,
    pub noise_amplitude_voxels: f32,
    pub noise_frequency_world: f32,
    pub noise_seed: u32,
}

impl TerrainHillField {
    const DISPATCH_RADIUS_SCALE: f32 = 1.5;

    fn dispatch_bound(self, atlas_dim: UVec3) -> Result<UAabb3> {
        if !self.center_voxels.is_finite()
            || !self.base_height_voxels.is_finite()
            || !self.radii_voxels.is_finite()
            || self.radii_voxels.min_element() <= 0.0
            || !self.rise_voxels.is_finite()
            || self.rise_voxels <= 0.0
            || !self.maximum_inflation_voxels.is_finite()
            || self.maximum_inflation_voxels < 0.0
            || !self.noise_amplitude_voxels.is_finite()
            || self.noise_amplitude_voxels < 0.0
            || !self.noise_frequency_world.is_finite()
            || self.noise_frequency_world <= 0.0
        {
            return Err(anyhow::anyhow!("invalid terrain hill field: {self:?}"));
        }

        let support_radii = self.radii_voxels * Self::DISPATCH_RADIUS_SCALE;
        let maximum_height = self.base_height_voxels
            + self.rise_voxels
            + self.noise_amplitude_voxels
            + self.maximum_inflation_voxels;
        let min = Vec3::new(
            self.center_voxels.x - support_radii.x,
            0.0,
            self.center_voxels.y - support_radii.y,
        )
        .floor()
        .as_ivec3();
        let max_exclusive = Vec3::new(
            self.center_voxels.x + support_radii.x,
            maximum_height + 1.0,
            self.center_voxels.y + support_radii.y,
        )
        .ceil()
        .as_ivec3();
        let atlas_max = atlas_dim.as_ivec3();
        let clipped_min = min.clamp(IVec3::ZERO, atlas_max);
        let clipped_max = max_exclusive.clamp(IVec3::ZERO, atlas_max);
        if any_ivec3_less_equal(clipped_max, clipped_min) {
            return Err(anyhow::anyhow!(
                "terrain hill field is outside atlas: min={min:?} max={max_exclusive:?} atlas={atlas_dim:?}"
            ));
        }

        Ok(UAabb3::new(clipped_min.as_uvec3(), clipped_max.as_uvec3()))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TerrainHillBlendPushConstants {
    offset: [u32; 4],
    dim: [u32; 4],
    center_base_blend: [f32; 4],
    radii_rise_noise: [f32; 4],
    noise_params: [f32; 4],
    options: [u32; 4],
}

impl PlainBuilder {
    pub fn blend_terrain_hill(&mut self, hill: TerrainHillField) -> Result<UAabb3> {
        let bound = hill.dispatch_bound(chunk_atlas_dim(&self.resources))?;
        let offset = bound.min();
        let dim = bound.max() - offset;
        let push_constants = TerrainHillBlendPushConstants {
            offset: [offset.x, offset.y, offset.z, 0],
            dim: [dim.x, dim.y, dim.z, 0],
            center_base_blend: [
                hill.center_voxels.x,
                hill.center_voxels.y,
                hill.base_height_voxels,
                hill.maximum_inflation_voxels,
            ],
            radii_rise_noise: [
                hill.radii_voxels.x,
                hill.radii_voxels.y,
                hill.rise_voxels,
                hill.noise_amplitude_voxels,
            ],
            noise_params: [
                hill.noise_frequency_world,
                TerrainHillField::DISPATCH_RADIUS_SCALE,
                0.0,
                0.0,
            ],
            options: [hill.noise_seed, 0, 0, 0],
        };

        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.terrain_hill_blend_ppl.record(
                    cmdbuf,
                    Extent3D::new(dim.x, dim.y, dim.z),
                    Some(bytemuck::bytes_of(&push_constants)),
                );
            },
        );

        log::info!(
            "[TERRAIN_HILL] blended canonical hill offset={offset:?} dim={dim:?} center={:?} base_y={:.1} radii={:?} rise={:.1} maximum_inflation={:.1} noise_amplitude={:.1}",
            hill.center_voxels,
            hill.base_height_voxels,
            hill.radii_voxels,
            hill.rise_voxels,
            hill.maximum_inflation_voxels,
            hill.noise_amplitude_voxels,
        );
        Ok(bound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hill() -> TerrainHillField {
        TerrainHillField {
            center_voxels: Vec2::new(160.0, 300.0),
            base_height_voxels: 100.0,
            radii_voxels: Vec2::new(80.0, 100.0),
            rise_voxels: 90.0,
            maximum_inflation_voxels: 8.0,
            noise_amplitude_voxels: 4.0,
            noise_frequency_world: 3.0,
            noise_seed: 1,
        }
    }

    #[test]
    fn dispatch_bound_contains_scaled_hill_support_and_maximum_height() {
        let bound = test_hill().dispatch_bound(UVec3::splat(512)).unwrap();
        assert_eq!(bound.min(), UVec3::new(40, 0, 150));
        assert_eq!(bound.max(), UVec3::new(280, 203, 450));
    }

    #[test]
    fn invalid_hill_dimensions_are_rejected() {
        let mut hill = test_hill();
        hill.radii_voxels.x = 0.0;
        assert!(hill.dispatch_bound(UVec3::splat(512)).is_err());
    }
}
