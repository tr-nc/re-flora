use super::App;
use crate::builder::{
    VOXEL_TYPE_CHERRY_WOOD, VOXEL_TYPE_DIRT, VOXEL_TYPE_OAK_WOOD, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND,
    VOXEL_TYPE_STUCCO,
};
use crate::gameplay::camera::{FootstepEvent, FootstepSurface};
use glam::Vec3;

const SURFACE_PROBE_OFFSET_WORLD: f32 = 2.0 / 256.0;
const SURFACE_PROBE_MAX_DISTANCE_WORLD: f32 = 4.0 / 256.0;

fn classify_footstep_surface(voxel_type: Option<u32>) -> FootstepSurface {
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
                event.surface = classify_footstep_surface(voxel_type);
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

    #[test]
    fn voxel_surface_classification_is_gameplay_owned_and_explicit() {
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_DIRT)),
            FootstepSurface::Dirt
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_SAND)),
            FootstepSurface::Sand
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_ROCK)),
            FootstepSurface::Stone
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_STUCCO)),
            FootstepSurface::Stucco
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_CHERRY_WOOD)),
            FootstepSurface::Wood
        );
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_OAK_WOOD)),
            FootstepSurface::Wood
        );
    }

    #[test]
    fn missing_empty_and_unrecognized_voxels_remain_unknown() {
        assert_eq!(classify_footstep_surface(None), FootstepSurface::Unknown);
        assert_eq!(
            classify_footstep_surface(Some(VOXEL_TYPE_EMPTY)),
            FootstepSurface::Unknown
        );
        assert_eq!(
            classify_footstep_surface(Some(999)),
            FootstepSurface::Unknown
        );
    }
}
