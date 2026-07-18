use crate::builder::{ContreeBuilder, ContreeCpuVoxelBlock, ContreeCpuVoxelBlockExport};
use glam::{IVec3, UVec3};
use re_flora_physics::{
    BrickOccupancy, CollisionWorld, StaticVoxelBrickId, STATIC_VOXEL_BRICK_DIM,
};
use std::time::Instant;

const STARTUP_TERRAIN_BRICK_ID: StaticVoxelBrickId = StaticVoxelBrickId(IVec3::new(8, 3, 8));
const STARTUP_TERRAIN_BRICK_MIN: UVec3 = UVec3::new(256, 96, 256);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StartupTerrainBrickState {
    #[default]
    Pending,
    Imported,
    Failed,
}

pub(super) struct TerrainPhysics {
    collision_world: CollisionWorld,
    startup_brick_state: StartupTerrainBrickState,
}

impl TerrainPhysics {
    pub(super) fn new() -> Self {
        Self {
            collision_world: CollisionWorld::new(),
            startup_brick_state: StartupTerrainBrickState::Pending,
        }
    }

    pub(super) fn try_import_startup_terrain_brick(&mut self, contree_builder: &ContreeBuilder) {
        if self.startup_brick_state != StartupTerrainBrickState::Pending {
            return;
        }

        let total_start = Instant::now();
        let export_start = Instant::now();
        let snapshot = contree_builder.cpu_voxel_source_snapshot();
        let export = snapshot.export_voxel_block(
            STARTUP_TERRAIN_BRICK_MIN,
            UVec3::splat(STATIC_VOXEL_BRICK_DIM),
        );
        let export_elapsed = export_start.elapsed();

        let block = match export {
            Ok(ContreeCpuVoxelBlockExport::Ready(block)) => block,
            Ok(ContreeCpuVoxelBlockExport::NotReady(_)) => return,
            Err(err) => {
                log::error!(
                    "[COLLISION][TERRAIN_BRICK] export failed id={:?} min={:?} dim={} error={err}",
                    STARTUP_TERRAIN_BRICK_ID,
                    STARTUP_TERRAIN_BRICK_MIN,
                    STATIC_VOXEL_BRICK_DIM,
                );
                self.startup_brick_state = StartupTerrainBrickState::Failed;
                return;
            }
        };

        let revision = match single_source_revision(&block) {
            Ok(revision) => revision,
            Err(err) => {
                log::error!(
                    "[COLLISION][TERRAIN_BRICK] source dependency validation failed id={:?} min={:?} dim={} dependencies={:?} error={err}",
                    STARTUP_TERRAIN_BRICK_ID,
                    STARTUP_TERRAIN_BRICK_MIN,
                    STATIC_VOXEL_BRICK_DIM,
                    block.source_dependencies,
                );
                self.startup_brick_state = StartupTerrainBrickState::Failed;
                return;
            }
        };

        let occupancy_start = Instant::now();
        let occupancy = match BrickOccupancy::from_x_fastest_voxel_types(&block.voxel_types) {
            Ok(occupancy) => occupancy,
            Err(err) => {
                log::error!(
                    "[COLLISION][TERRAIN_BRICK] occupancy build failed id={:?} min={:?} dim={} error={err}",
                    STARTUP_TERRAIN_BRICK_ID,
                    STARTUP_TERRAIN_BRICK_MIN,
                    STATIC_VOXEL_BRICK_DIM,
                );
                self.startup_brick_state = StartupTerrainBrickState::Failed;
                return;
            }
        };
        let occupancy_elapsed = occupancy_start.elapsed();
        let solid_count = occupancy.filled_count();

        let physics_start = Instant::now();
        let update = self.collision_world.upsert_static_voxel_brick(
            STARTUP_TERRAIN_BRICK_ID,
            revision,
            occupancy,
        );
        let physics_elapsed = physics_start.elapsed();

        self.startup_brick_state = StartupTerrainBrickState::Imported;
        log::info!(
            "[COLLISION][TERRAIN_BRICK] imported id={:?} min={:?} dim={} dependencies={:?} solids={} update={:?} export_ms={:.3} occupancy_ms={:.3} physics_ms={:.3} total_ms={:.3}",
            STARTUP_TERRAIN_BRICK_ID,
            STARTUP_TERRAIN_BRICK_MIN,
            STATIC_VOXEL_BRICK_DIM,
            block.source_dependencies,
            solid_count,
            update,
            export_elapsed.as_secs_f64() * 1_000.0,
            occupancy_elapsed.as_secs_f64() * 1_000.0,
            physics_elapsed.as_secs_f64() * 1_000.0,
            total_start.elapsed().as_secs_f64() * 1_000.0,
        );
    }
}

fn single_source_revision(block: &ContreeCpuVoxelBlock) -> Result<u64, &'static str> {
    let [dependency] = block.source_dependencies.as_slice() else {
        return Err("expected exactly one source dependency");
    };
    if !dependency.is_present {
        return Err("source chunk is not present");
    }
    dependency
        .source_revision
        .ok_or("source chunk has no revision")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::ContreeCpuVoxelSourceDependency;

    fn block_with_dependencies(
        source_dependencies: Vec<ContreeCpuVoxelSourceDependency>,
    ) -> ContreeCpuVoxelBlock {
        ContreeCpuVoxelBlock {
            voxel_min: STARTUP_TERRAIN_BRICK_MIN,
            dim: UVec3::splat(STATIC_VOXEL_BRICK_DIM),
            voxel_dim_per_chunk: UVec3::splat(256),
            voxel_types: vec![0; STATIC_VOXEL_BRICK_DIM.pow(3) as usize],
            source_dependencies,
        }
    }

    #[test]
    fn startup_brick_bounds_match_its_physics_id() {
        assert_eq!(
            STARTUP_TERRAIN_BRICK_ID.0.as_uvec3() * STATIC_VOXEL_BRICK_DIM,
            STARTUP_TERRAIN_BRICK_MIN
        );
    }

    #[test]
    fn accepts_one_present_source_revision() {
        let block = block_with_dependencies(vec![ContreeCpuVoxelSourceDependency {
            chunk_idx: UVec3::new(1, 0, 1),
            source_revision: Some(42),
            is_present: true,
        }]);

        assert_eq!(single_source_revision(&block), Ok(42));
    }

    #[test]
    fn rejects_missing_or_ambiguous_source_dependencies() {
        let missing_revision = block_with_dependencies(vec![ContreeCpuVoxelSourceDependency {
            chunk_idx: UVec3::new(1, 0, 1),
            source_revision: None,
            is_present: true,
        }]);
        assert_eq!(
            single_source_revision(&missing_revision),
            Err("source chunk has no revision")
        );

        let no_dependencies = block_with_dependencies(Vec::new());
        assert_eq!(
            single_source_revision(&no_dependencies),
            Err("expected exactly one source dependency")
        );
    }
}
