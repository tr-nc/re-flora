use super::App;
use crate::builder::{
    VOXEL_TYPE_CHERRY_WOOD, VOXEL_TYPE_DIRT, VOXEL_TYPE_OAK_WOOD, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND,
    VOXEL_TYPE_STUCCO,
};
use crate::gameplay::camera::{FootstepEvent, FootstepSurface};
use crate::voxel_material::VoxelMaterialMode;
use glam::Vec3;

const SURFACE_PROBE_OFFSET_WORLD: f32 = 2.0 / 256.0;
const SURFACE_PROBE_MAX_DISTANCE_WORLD: f32 = 4.0 / 256.0;

fn classify_footstep_surface(
    voxel_type: Option<u32>,
    material_mode: VoxelMaterialMode,
) -> FootstepSurface {
    if voxel_type == Some(VOXEL_TYPE_SAND) && material_mode == VoxelMaterialMode::GlassExperiment {
        return FootstepSurface::Glass;
    }
    match voxel_type {
        Some(VOXEL_TYPE_DIRT) => FootstepSurface::Dirt,
        Some(VOXEL_TYPE_SAND) => FootstepSurface::Sand,
        Some(VOXEL_TYPE_ROCK) => FootstepSurface::Stone,
        Some(VOXEL_TYPE_CHERRY_WOOD | VOXEL_TYPE_OAK_WOOD) => FootstepSurface::Wood,
        Some(VOXEL_TYPE_STUCCO) => FootstepSurface::Stucco,
        _ => FootstepSurface::Unknown,
    }
}

impl App {
    pub(super) fn resolve_local_footstep_events(
        &self,
        events: Vec<FootstepEvent>,
    ) -> Vec<FootstepEvent> {
        events
            .into_iter()
            .map(|mut event| {
                let probe_origin = event.contact_world + Vec3::Y * SURFACE_PROBE_OFFSET_WORLD;
                let voxel_type = self
                    .query_terrain_ray_cpu(probe_origin, Vec3::NEG_Y)
                    .filter(|hit| {
                        probe_origin.distance(hit.position) <= SURFACE_PROBE_MAX_DISTANCE_WORLD
                    })
                    .map(|hit| hit.voxel_type);
                event.surface = classify_footstep_surface(voxel_type, self.voxel_material_mode());
                event
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::classify_footstep_surface;
    use crate::builder::{
        VOXEL_TYPE_CHERRY_WOOD, VOXEL_TYPE_DIRT, VOXEL_TYPE_EMPTY, VOXEL_TYPE_OAK_WOOD,
        VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND, VOXEL_TYPE_STUCCO,
    };
    use crate::gameplay::camera::FootstepSurface;
    use crate::voxel_material::VoxelMaterialMode;

    #[test]
    fn voxel_surface_classification_is_gameplay_owned_and_explicit() {
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_DIRT), VoxelMaterialMode::Standard),
            FootstepSurface::Dirt
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_SAND), VoxelMaterialMode::Standard),
            FootstepSurface::Sand
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_ROCK), VoxelMaterialMode::Standard),
            FootstepSurface::Stone
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_STUCCO), VoxelMaterialMode::Standard),
            FootstepSurface::Stucco
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_CHERRY_WOOD), VoxelMaterialMode::Standard,),
            FootstepSurface::Wood
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_OAK_WOOD), VoxelMaterialMode::Standard,),
            FootstepSurface::Wood
        );
    }

    #[test]
    fn missing_empty_and_unrecognized_voxels_remain_unknown() {
        assert_eq!(
            classify_footstep_surface(None, VoxelMaterialMode::Standard),
            FootstepSurface::Unknown
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_EMPTY), VoxelMaterialMode::Standard),
            FootstepSurface::Unknown
        );
        assert_eq!(
            classify_footstep_surface(Some(999), VoxelMaterialMode::Standard),
            FootstepSurface::Unknown
        );
    }

    #[test]
    fn glass_experiment_preserves_sand_off_and_names_glass_on() {
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_SAND), VoxelMaterialMode::Standard),
            FootstepSurface::Sand,
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_SAND), VoxelMaterialMode::GlassExperiment,),
            FootstepSurface::Glass,
        );
    }
}
