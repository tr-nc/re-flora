use super::{App, CHUNK_DIM, VOXEL_DIM_PER_CHUNK};
use crate::builder::{ContreeCpuRayHit, VOXEL_TYPE_DIRT};
use glam::{UVec2, UVec3, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PlantableSurfaceAnchor {
    surface_position_ws: Vec3,
    base_world_vox: UVec3,
}

impl PlantableSurfaceAnchor {
    pub(super) fn base_world_vox(self) -> UVec3 {
        self.base_world_vox
    }

    pub(super) fn base_center_vox(self) -> Vec3 {
        self.base_world_vox.as_vec3() + Vec3::splat(0.5)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PlantingRejection {
    OutsideWorld,
    NoSurface,
    UnsupportedSubstrate { voxel_type: u32 },
}

#[derive(Default)]
pub(super) struct AuthoredFloraPlacementBatch {
    dirty_chunks: Vec<UVec3>,
    spawn_time_ms: u32,
}

impl AuthoredFloraPlacementBatch {
    pub(super) fn new() -> Self {
        Self::default()
    }
}

impl App {
    pub(super) fn resolve_plantable_surface_column(
        &self,
        column_world_vox: UVec2,
    ) -> Result<PlantableSurfaceAnchor, PlantingRejection> {
        let world_dim_vox = CHUNK_DIM * VOXEL_DIM_PER_CHUNK;
        if column_world_vox.x >= world_dim_vox.x || column_world_vox.y >= world_dim_vox.z {
            return Err(PlantingRejection::OutsideWorld);
        }

        let position_ws = Vec3::new(
            (column_world_vox.x as f32 + 0.5) / VOXEL_DIM_PER_CHUNK.x as f32,
            CHUNK_DIM.y as f32 + 1.0,
            (column_world_vox.y as f32 + 0.5) / VOXEL_DIM_PER_CHUNK.z as f32,
        );
        let hit = self
            .query_terrain_ray_cpu(position_ws, Vec3::NEG_Y)
            .ok_or(PlantingRejection::NoSurface)?;

        classify_plantable_surface_hit(column_world_vox, hit, world_dim_vox)
    }

    pub(super) fn try_place_authored_flora(
        &mut self,
        batch: &mut AuthoredFloraPlacementBatch,
        species_index: u32,
        anchor: PlantableSurfaceAnchor,
        growth_progress: u32,
        spawn_start_ms: u32,
        seed: u32,
    ) -> bool {
        batch.spawn_time_ms = spawn_start_ms;
        self.surface_builder
            .try_insert_authored_flora_instance_unchecked(
                species_index,
                anchor.base_world_vox(),
                growth_progress,
                spawn_start_ms,
                seed,
                &mut batch.dirty_chunks,
            )
    }

    pub(super) fn finish_authored_flora_placement(
        &mut self,
        batch: AuthoredFloraPlacementBatch,
    ) -> anyhow::Result<()> {
        self.surface_builder
            .sync_authored_flora_dirty_chunks(&batch.dirty_chunks, batch.spawn_time_ms)
    }
}

fn is_plantable_surface_voxel_type(voxel_type: u32) -> bool {
    voxel_type == VOXEL_TYPE_DIRT
}

fn classify_plantable_surface_hit(
    column_world_vox: UVec2,
    hit: ContreeCpuRayHit,
    world_dim_vox: UVec3,
) -> Result<PlantableSurfaceAnchor, PlantingRejection> {
    if !is_plantable_surface_voxel_type(hit.voxel_type) {
        return Err(PlantingRejection::UnsupportedSubstrate {
            voxel_type: hit.voxel_type,
        });
    }
    if !hit.position.is_finite() || hit.position.y < 0.0 {
        return Err(PlantingRejection::OutsideWorld);
    }

    let base_y = (hit.position.y * VOXEL_DIM_PER_CHUNK.y as f32).floor() as u32;
    if base_y >= world_dim_vox.y {
        return Err(PlantingRejection::OutsideWorld);
    }

    Ok(PlantableSurfaceAnchor {
        surface_position_ws: hit.position,
        base_world_vox: UVec3::new(column_world_vox.x, base_y, column_world_vox.y),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{
        VOXEL_TYPE_CHERRY_WOOD, VOXEL_TYPE_OAK_WOOD, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND,
        VOXEL_TYPE_STUCCO,
    };

    const WORLD_DIM_VOX: UVec3 = UVec3::new(512, 512, 512);

    fn hit(voxel_type: u32, y: f32) -> ContreeCpuRayHit {
        ContreeCpuRayHit {
            position: Vec3::new(0.5, y, 0.5),
            voxel_type,
        }
    }

    #[test]
    fn dirt_hit_resolves_canonical_base_voxel() {
        let anchor = classify_plantable_surface_hit(
            UVec2::new(12, 34),
            hit(VOXEL_TYPE_DIRT, 100.75 / 256.0),
            WORLD_DIM_VOX,
        )
        .unwrap();

        assert_eq!(anchor.base_world_vox(), UVec3::new(12, 100, 34));
        assert_eq!(anchor.base_center_vox(), Vec3::new(12.5, 100.5, 34.5));
    }

    #[test]
    fn non_dirt_hits_are_rejected() {
        for voxel_type in [
            VOXEL_TYPE_SAND,
            VOXEL_TYPE_ROCK,
            VOXEL_TYPE_CHERRY_WOOD,
            VOXEL_TYPE_OAK_WOOD,
            VOXEL_TYPE_STUCCO,
        ] {
            assert_eq!(
                classify_plantable_surface_hit(
                    UVec2::new(12, 34),
                    hit(voxel_type, 0.5),
                    WORLD_DIM_VOX,
                ),
                Err(PlantingRejection::UnsupportedSubstrate { voxel_type })
            );
        }
    }

    #[test]
    fn manual_planting_requires_dirt_while_surface_flora_allows_stucco() {
        assert!(is_plantable_surface_voxel_type(VOXEL_TYPE_DIRT));
        let shader_policy = include_str!("../../../shader/slang/flora_surface_planting.slang");
        assert!(shader_policy
            .contains("return voxelType == VOXEL_TYPE_DIRT || voxelType == VOXEL_TYPE_STUCCO;"));
    }

    #[test]
    fn hits_outside_vertical_world_bounds_are_rejected() {
        assert_eq!(
            classify_plantable_surface_hit(
                UVec2::new(12, 34),
                hit(VOXEL_TYPE_DIRT, 2.0),
                WORLD_DIM_VOX,
            ),
            Err(PlantingRejection::OutsideWorld)
        );
    }
}
