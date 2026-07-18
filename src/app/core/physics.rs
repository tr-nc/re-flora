use crate::builder::{ContreeBuilder, ContreeCpuVoxelBlock, ContreeCpuVoxelBlockExport};
use crate::tracer::{voxel_apple_offsets, Tracer};
use anyhow::Context;
use glam::{IVec3, UVec3, Vec3};
use re_flora_physics::{
    BrickOccupancy, CollisionWorld, DynamicBodyDesc, DynamicBodyId, DynamicColliderShape,
    StaticVoxelBrickId, STATIC_VOXEL_BRICK_DIM,
};
use std::collections::HashSet;
use std::time::Instant;

const STARTUP_TERRAIN_BRICK_ID: StaticVoxelBrickId = StaticVoxelBrickId(IVec3::new(8, 3, 8));
const STARTUP_TERRAIN_BRICK_MIN: UVec3 = UVec3::new(256, 96, 256);
const VOXELS_PER_WORLD_UNIT: f32 = 256.0;
// Keep the probe inside the only terrain-collision brick currently imported, but place it on the
// camera-facing side of the startup tree so the trunk does not hide the four-voxel fruit.
const COLLISION_PROBE_SPAWN_VOXELS: Vec3 = Vec3::new(276.0, 152.0, 280.0);
const COLLISION_PROBE_GRAVITY_VOXELS: Vec3 = Vec3::new(0.0, -9.8 * VOXELS_PER_WORLD_UNIT, 0.0);

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
    collision_probe_body: Option<DynamicBodyId>,
    collision_probe_mesh_uploaded: bool,
}

impl TerrainPhysics {
    pub(super) fn new() -> Self {
        let mut collision_world = CollisionWorld::new();
        collision_world
            .set_gravity(COLLISION_PROBE_GRAVITY_VOXELS)
            .expect("collision probe gravity constant must be valid");
        Self {
            collision_world,
            startup_brick_state: StartupTerrainBrickState::Pending,
            collision_probe_body: None,
            collision_probe_mesh_uploaded: false,
        }
    }

    pub(super) fn collision_probe_ready(&self) -> bool {
        self.startup_brick_state == StartupTerrainBrickState::Imported
    }

    pub(super) fn collision_probe_active(&self) -> bool {
        self.collision_probe_body.is_some()
    }

    pub(super) fn collision_probe_status(&self) -> String {
        match self.startup_brick_state {
            StartupTerrainBrickState::Pending => "Terrain collider loading...".to_owned(),
            StartupTerrainBrickState::Failed => "Terrain collider failed to load".to_owned(),
            StartupTerrainBrickState::Imported => {
                let Some(id) = self.collision_probe_body else {
                    return "Ready".to_owned();
                };
                let Some(state) = self.collision_world.dynamic_body_state(id) else {
                    return "Probe body unavailable".to_owned();
                };
                let position = state.position / VOXELS_PER_WORLD_UNIT;
                let speed = state.linear_velocity.length() / VOXELS_PER_WORLD_UNIT;
                let motion = if state.sleeping { "resting" } else { "moving" };
                format!(
                    "{motion} at ({:.3}, {:.3}, {:.3})\nspeed {:.2} world units/s",
                    position.x, position.y, position.z, speed
                )
            }
        }
    }

    pub(super) fn drop_collision_probe(&mut self, tracer: &mut Tracer) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.collision_probe_ready(),
            "terrain collision brick is not ready"
        );

        self.clear_collision_probe(tracer);
        if !self.collision_probe_mesh_uploaded {
            tracer
                .upload_collision_probe_geometry()
                .context("uploading collision probe geometry")?;
            self.collision_probe_mesh_uploaded = true;
        }

        let collider_points = collision_probe_convex_points();
        let collider_point_count = collider_points.len();
        let mut desc = DynamicBodyDesc::sphere(COLLISION_PROBE_SPAWN_VOXELS, 2.0);
        desc.collider = DynamicColliderShape::ConvexHull {
            points: collider_points,
        };
        desc.linear_velocity = Vec3::new(7.0, 0.0, 2.0);
        desc.angular_velocity = Vec3::new(3.2, 1.7, -4.0);
        desc.friction = 0.85;
        desc.restitution = 0.08;
        desc.linear_damping = 0.08;
        desc.angular_damping = 0.12;
        desc.ccd_enabled = true;

        let body = self
            .collision_world
            .spawn_dynamic_body(desc)
            .context("spawning collision probe body")?;
        let state = self
            .collision_world
            .dynamic_body_state(body)
            .context("reading newly spawned collision probe body")?;
        if let Err(err) = tracer
            .show_collision_probe_geometry(state.position / VOXELS_PER_WORLD_UNIT, state.rotation)
        {
            self.collision_world.remove_dynamic_body(body);
            return Err(err).context("showing collision probe geometry");
        }
        self.collision_probe_body = Some(body);
        log::info!(
            "[COLLISION][PROBE] dropped body={} spawn_voxels={:?} convex_points={}",
            body.get(),
            COLLISION_PROBE_SPAWN_VOXELS,
            collider_point_count,
        );
        Ok(())
    }

    pub(super) fn clear_collision_probe(&mut self, tracer: &mut Tracer) {
        if let Some(body) = self.collision_probe_body.take() {
            self.collision_world.remove_dynamic_body(body);
        }
        tracer.clear_collision_probe_geometry();
    }

    pub(super) fn advance_collision_probe(
        &mut self,
        frame_delta_time: f32,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        let Some(body) = self.collision_probe_body else {
            return Ok(());
        };
        let step = self.collision_world.advance(frame_delta_time);
        if step.dropped_seconds > 0.0 {
            log::warn!(
                "[COLLISION][PROBE] physics hitch dropped {:.3} ms",
                step.dropped_seconds * 1_000.0
            );
        }
        let Some(state) = self.collision_world.dynamic_body_state(body) else {
            self.collision_probe_body = None;
            tracer.clear_collision_probe_geometry();
            anyhow::bail!("collision probe body disappeared from physics world");
        };
        tracer
            .show_collision_probe_geometry(state.position / VOXELS_PER_WORLD_UNIT, state.rotation)
            .context("updating collision probe geometry")
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

fn collision_probe_convex_points() -> Vec<Vec3> {
    let mut corners = HashSet::new();
    for voxel in voxel_apple_offsets() {
        for x in 0..=1 {
            for y in 0..=1 {
                for z in 0..=1 {
                    corners.insert(voxel + IVec3::new(x, y, z));
                }
            }
        }
    }
    let mut corners = corners.into_iter().collect::<Vec<_>>();
    corners.sort_unstable_by_key(|corner| (corner.x, corner.y, corner.z));
    corners.into_iter().map(IVec3::as_vec3).collect()
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
    fn collision_probe_convex_points_cover_visible_apple_bounds() {
        let points = collision_probe_convex_points();
        let min = points.iter().copied().reduce(Vec3::min).unwrap();
        let max = points.iter().copied().reduce(Vec3::max).unwrap();

        assert!(points.len() > voxel_apple_offsets().len());
        assert_eq!(min, Vec3::splat(-2.0));
        assert_eq!(max, Vec3::splat(2.0));
    }

    #[test]
    fn collision_probe_spawns_inside_imported_brick_xz_bounds() {
        let probe_radius = Vec3::splat(2.0);
        let brick_min = STARTUP_TERRAIN_BRICK_MIN.as_vec3();
        let brick_max = brick_min + Vec3::splat(STATIC_VOXEL_BRICK_DIM as f32);

        assert!((COLLISION_PROBE_SPAWN_VOXELS - probe_radius).x >= brick_min.x);
        assert!((COLLISION_PROBE_SPAWN_VOXELS - probe_radius).z >= brick_min.z);
        assert!((COLLISION_PROBE_SPAWN_VOXELS + probe_radius).x <= brick_max.x);
        assert!((COLLISION_PROBE_SPAWN_VOXELS + probe_radius).z <= brick_max.z);
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
