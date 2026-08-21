use super::*;
use glam::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainGradeField {
    pub center_voxels: Vec2,
    pub half_extent_voxels: Vec2,
    pub target_height_voxels: f32,
    pub feather_voxels: f32,
}

impl TerrainGradeField {
    fn dispatch_bound(self, atlas_dim: UVec3) -> Result<UAabb3> {
        if !self.center_voxels.is_finite()
            || !self.half_extent_voxels.is_finite()
            || self.half_extent_voxels.min_element() <= 0.0
            || !self.target_height_voxels.is_finite()
            || !self.feather_voxels.is_finite()
            || self.feather_voxels <= 0.0
            || self.feather_voxels > self.half_extent_voxels.min_element()
        {
            return Err(anyhow::anyhow!("invalid terrain grade field: {self:?}"));
        }

        let min_xz = (self.center_voxels - self.half_extent_voxels)
            .floor()
            .as_ivec2();
        let max_xz = (self.center_voxels + self.half_extent_voxels)
            .ceil()
            .as_ivec2();
        let atlas_max = atlas_dim.as_ivec3();
        let min = IVec3::new(min_xz.x, 0, min_xz.y).clamp(IVec3::ZERO, atlas_max);
        let max = IVec3::new(max_xz.x, atlas_max.y, max_xz.y).clamp(IVec3::ZERO, atlas_max);
        if any_ivec3_less_equal(max, min) {
            return Err(anyhow::anyhow!(
                "terrain grade field is outside atlas: min={min:?} max={max:?} atlas={atlas_dim:?}"
            ));
        }

        Ok(UAabb3::new(min.as_uvec3(), max.as_uvec3()))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TerrainGradePushConstants {
    offset: [u32; 4],
    dim: [u32; 4],
    center_half_extent: [f32; 4],
    height_feather: [f32; 4],
}

impl PlainBuilder {
    pub fn grade_terrain(&mut self, field: TerrainGradeField) -> Result<UAabb3> {
        let bound = field.dispatch_bound(chunk_atlas_dim(&self.resources))?;
        let offset = bound.min();
        let dim = bound.max() - offset;
        let push_constants = TerrainGradePushConstants {
            offset: [offset.x, offset.y, offset.z, 0],
            dim: [dim.x, dim.y, dim.z, 0],
            center_half_extent: [
                field.center_voxels.x,
                field.center_voxels.y,
                field.half_extent_voxels.x,
                field.half_extent_voxels.y,
            ],
            height_feather: [field.target_height_voxels, field.feather_voxels, 0.0, 0.0],
        };

        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.terrain_grade_ppl.record(
                    cmdbuf,
                    Extent3D::new(dim.x, dim.y, dim.z),
                    Some(bytemuck::bytes_of(&push_constants)),
                );
            },
        );

        log::info!(
            "[TERRAIN_GRADE] graded canonical terrain offset={offset:?} dim={dim:?} center={:?} half_extent={:?} target_height={:.1} feather={:.1}",
            field.center_voxels,
            field.half_extent_voxels,
            field.target_height_voxels,
            field.feather_voxels,
        );
        Ok(bound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_field() -> TerrainGradeField {
        TerrainGradeField {
            center_voxels: Vec2::new(160.0, 390.0),
            half_extent_voxels: Vec2::new(100.0, 90.0),
            target_height_voxels: 100.0,
            feather_voxels: 20.0,
        }
    }

    #[test]
    fn dispatch_bound_covers_the_full_rounded_grade_footprint() {
        let bound = test_field().dispatch_bound(UVec3::splat(512)).unwrap();
        assert_eq!(bound.min(), UVec3::new(60, 0, 300));
        assert_eq!(bound.max(), UVec3::new(260, 512, 480));
    }

    #[test]
    fn feather_must_fit_inside_the_grade_footprint() {
        let mut field = test_field();
        field.feather_voxels = 101.0;
        assert!(field.dispatch_bound(UVec3::splat(512)).is_err());
    }
}
