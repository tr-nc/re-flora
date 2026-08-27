use super::App;
use crate::app::world_edits::{BuildEdit, VoxelAtlasStateWrite, VoxelEdit, WorldEditPlan};
use crate::builder::{VOXEL_TYPE_EMPTY, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND};
use crate::geom::{build_bvh, Cuboid, UAabb3};
use crate::tracer::{append_box, GeometryPreviewMesh, GlassSceneQueryEventKind, TerrainRayQuery};
use crate::voxel_material::{
    canonicalize_atlas_data, material_for, DirectShadowPolicy, LocalShadowPolicy,
    VoxelMaterialMode, VoxelSurfaceClass,
};
use anyhow::{Context, Result};
use glam::{UVec3, Vec3, Vec4};

const BUILD_DELAY_SECONDS: f32 = 0.5;
const SETTLE_FRAMES: u8 = 2;

const CAMERA_POSITION: Vec3 = Vec3::new(1.25, 0.92, 1.82);
const CAMERA_TARGET: Vec3 = Vec3::new(1.25, 0.82, 0.88);
const TEST_TIME_OF_DAY: f32 = 0.455_705;

const CLEAR_MIN: UVec3 = UVec3::new(192, 64, 128);
const CLEAR_MAX: UVec3 = UVec3::new(448, 352, 448);
const FLOOR_MIN: UVec3 = UVec3::new(192, 64, 128);
const FLOOR_MAX: UVec3 = UVec3::new(448, 88, 448);
const BACK_WALL_MIN: UVec3 = UVec3::new(192, 88, 140);
const BACK_WALL_MAX: UVec3 = UVec3::new(448, 320, 164);
const GLASS_SLAB_A_MIN: UVec3 = UVec3::new(224, 96, 274);
const GLASS_SLAB_A_MAX: UVec3 = UVec3::new(320, 304, 282);
const GLASS_SLAB_B_MIN: UVec3 = UVec3::new(344, 96, 264);
const GLASS_SLAB_B_MAX: UVec3 = UVec3::new(408, 272, 296);
const REBUILD_MIN: UVec3 = UVec3::new(184, 56, 120);
const REBUILD_MAX: UVec3 = UVec3::new(456, 360, 456);
const GPU_CAPTURE_ORIGIN: Vec3 = Vec3::new(1.0, 0.75, 1.5);
const GPU_CAPTURE_DIRECTION: Vec3 = Vec3::NEG_Z;

pub(super) const STARTUP_TREE_POSITION: Vec3 = Vec3::new(1.72, 0.2, 1.72);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestScenePhase {
    Pending,
    TerrainPublished,
    Settling { frames: u8, terrain_revision: u32 },
    WaitingForProbeField { terrain_revision: u32 },
    Ready,
    Failed,
}

#[derive(Debug)]
pub(super) struct GlassVoxelTestScene {
    phase: TestScenePhase,
}

impl GlassVoxelTestScene {
    pub(super) fn new() -> Self {
        Self {
            phase: TestScenePhase::Pending,
        }
    }

    pub(super) fn is_ready(&self) -> bool {
        self.phase == TestScenePhase::Ready
    }
}

fn cuboid(min: UVec3, max: UVec3) -> Cuboid {
    Cuboid::from_min_max(min.as_vec3(), max.as_vec3())
}

fn stamp_cuboids(cuboids: Vec<Cuboid>, voxel_type: u32) -> Result<VoxelEdit> {
    let aabbs = cuboids.iter().map(Cuboid::aabb).collect::<Vec<_>>();
    let leaves = (0..cuboids.len() as u32).collect::<Vec<_>>();
    let bvh_nodes = build_bvh(&aabbs, &leaves).map_err(anyhow::Error::msg)?;
    let material = material_for(voxel_type, VoxelMaterialMode::GlassExperiment);
    Ok(VoxelEdit::StampCuboids {
        bvh_nodes,
        cuboids,
        voxel_type,
        atlas_state_write: if material.surface_class == VoxelSurfaceClass::Dielectric {
            VoxelAtlasStateWrite::Clear
        } else {
            VoxelAtlasStateWrite::MaterialDefault
        },
    })
}

fn scene_plan() -> Result<WorldEditPlan> {
    Ok(WorldEditPlan {
        voxel_edits: vec![
            stamp_cuboids(vec![cuboid(CLEAR_MIN, CLEAR_MAX)], VOXEL_TYPE_EMPTY)?,
            stamp_cuboids(vec![cuboid(FLOOR_MIN, FLOOR_MAX)], VOXEL_TYPE_ROCK)?,
            stamp_cuboids(vec![cuboid(BACK_WALL_MIN, BACK_WALL_MAX)], VOXEL_TYPE_ROCK)?,
            stamp_cuboids(
                vec![
                    cuboid(GLASS_SLAB_A_MIN, GLASS_SLAB_A_MAX),
                    cuboid(GLASS_SLAB_B_MIN, GLASS_SLAB_B_MAX),
                ],
                VOXEL_TYPE_SAND,
            )?,
        ],
        build_edits: vec![BuildEdit::RebuildMesh(UAabb3::new(
            REBUILD_MIN,
            REBUILD_MAX,
        ))],
    })
}

fn sentinel_mesh() -> GeometryPreviewMesh {
    let mut mesh = GeometryPreviewMesh::default();
    // This narrow amber bar sits in front of slab A. It is deliberately raster-only:
    // the Glass resolve must keep it opaque instead of replacing it with a refracted
    // sample from behind the GlassFront depth.
    append_box(
        &mut mesh,
        Vec3::new(1.19, 0.48, 1.22),
        Vec3::new(1.23, 1.08, 1.26),
        Vec4::new(4.0, 0.55, 0.025, 1.0),
    );
    append_box(
        &mut mesh,
        Vec3::new(0.86, 0.42, 0.66),
        Vec3::new(1.08, 1.04, 0.70),
        Vec4::new(1.0, 0.025, 0.04, 1.0),
    );
    append_box(
        &mut mesh,
        Vec3::new(1.12, 0.34, 0.65),
        Vec3::new(1.38, 1.12, 0.70),
        Vec4::new(0.02, 0.95, 0.12, 1.0),
    );
    append_box(
        &mut mesh,
        Vec3::new(1.42, 0.46, 0.64),
        Vec3::new(1.64, 0.98, 0.70),
        Vec4::new(0.025, 0.08, 1.0, 1.0),
    );
    mesh
}

fn validate_glass_policy() -> Result<()> {
    let standard_sand = material_for(VOXEL_TYPE_SAND, VoxelMaterialMode::Standard);
    anyhow::ensure!(
        standard_sand.surface_class == VoxelSurfaceClass::Opaque
            && standard_sand.soil_state_allowed
            && standard_sand.blocks_ddgi_visibility,
        "standard Sand policy changed while the Glass experiment is off"
    );
    let material = material_for(VOXEL_TYPE_SAND, VoxelMaterialMode::GlassExperiment);
    anyhow::ensure!(
        material.surface_class == VoxelSurfaceClass::Dielectric
            && material.collision_solid
            && material.water_solid
            && material.terrain_support
            && material.probe_relocation_solid
            && !material.blocks_ddgi_visibility
            && material.direct_shadow == DirectShadowPolicy::Skip
            && material.local_shadow == LocalShadowPolicy::OpticalTransmittance
            && !material.soil_state_allowed
            && material.optical.is_some(),
        "experimental Glass material policy is internally inconsistent"
    );
    Ok(())
}

impl App {
    pub(super) fn configure_glass_voxel_test_scene(&mut self) -> Result<()> {
        validate_glass_policy()?;
        self.set_manual_time_of_day(TEST_TIME_OF_DAY);
        self.debug_settings.adjustables.auto_daynight_cycle.value = false;
        self.camera_control.set_orbit_focus(CAMERA_TARGET);
        anyhow::ensure!(
            self.tracer
                .set_camera_pose_looking_at(CAMERA_POSITION, CAMERA_TARGET),
            "failed to apply deterministic Glass voxel camera pose"
        );
        self.tracer
            .upload_debug_geometry_preview(&sentinel_mesh(), Vec3::ZERO, Vec4::ONE)?;
        self.tracer.invalidate_local_direct_sun_shadow_histories();
        log::info!(
            "[GLASS_VOXEL_TEST] configured mode=experimental-sand-id-3 camera=({:.3},{:.3},{:.3}) target=({:.3},{:.3},{:.3}) persistence=disabled",
            CAMERA_POSITION.x,
            CAMERA_POSITION.y,
            CAMERA_POSITION.z,
            CAMERA_TARGET.x,
            CAMERA_TARGET.y,
            CAMERA_TARGET.z,
        );
        Ok(())
    }

    fn validate_glass_voxel_atlas_state(&mut self) -> Result<usize> {
        let bytes = self
            .plain_builder
            .read_chunk_atlas_region(CLEAR_MIN, CLEAR_MAX - CLEAR_MIN)?;
        let mut glass_count = 0usize;
        for voxel_data in bytes {
            if u32::from(voxel_data & crate::builder::VOXEL_TYPE_MASK) != VOXEL_TYPE_SAND {
                continue;
            }
            glass_count += 1;
            anyhow::ensure!(
                canonicalize_atlas_data(voxel_data, VoxelMaterialMode::GlassExperiment)
                    == voxel_data,
                "experimental Glass voxel retained soil-state bits: {voxel_data:#04x}"
            );
        }
        anyhow::ensure!(
            glass_count > 0,
            "Glass voxel test scene authored no Glass voxels"
        );
        Ok(glass_count)
    }

    fn validate_gpu_scene_query_capture(&mut self) -> Result<()> {
        let events = self
            .tracer
            .capture_glass_scene_query_events(TerrainRayQuery {
                origin: GPU_CAPTURE_ORIGIN,
                direction: GPU_CAPTURE_DIRECTION,
            })?;
        anyhow::ensure!(events.len() == 3, "expected 3 GPU events, got {events:?}");
        let expected = [
            (
                GlassSceneQueryEventKind::Interface,
                VOXEL_TYPE_EMPTY,
                VOXEL_TYPE_SAND,
                GLASS_SLAB_A_MAX.z as f32 / 256.0,
            ),
            (
                GlassSceneQueryEventKind::Interface,
                VOXEL_TYPE_SAND,
                VOXEL_TYPE_EMPTY,
                GLASS_SLAB_A_MIN.z as f32 / 256.0,
            ),
            (
                GlassSceneQueryEventKind::Opaque,
                VOXEL_TYPE_EMPTY,
                VOXEL_TYPE_ROCK,
                BACK_WALL_MAX.z as f32 / 256.0,
            ),
        ];
        for (event, (kind, from_voxel_type, to_voxel_type, position_z)) in
            events.iter().zip(expected)
        {
            anyhow::ensure!(
                event.kind == kind
                    && event.from_voxel_type == from_voxel_type
                    && event.to_voxel_type == to_voxel_type
                    && event.tied_axes == 0b100
                    && (event.position.z - position_z).abs() <= 1.0e-6,
                "GPU SceneQuery differs from the deterministic reference: {event:?} expected kind={kind:?} from={from_voxel_type} to={to_voxel_type} z={position_z}",
            );
        }
        log::info!(
            "[GLASS_VOXEL_TEST][SCENE_QUERY] gpu_reference_match=true events={} dda_steps={:?} sequence=air-glass,glass-air,air-opaque",
            events.len(),
            events.iter().map(|event| event.dda_steps).collect::<Vec<_>>(),
        );
        Ok(())
    }

    pub(super) fn process_glass_voxel_test_scene(&mut self) {
        let Some(phase) = self
            .glass_voxel_test_scene
            .as_ref()
            .map(|scene| scene.phase)
        else {
            return;
        };

        let next_phase = match phase {
            TestScenePhase::Pending => {
                let Some(render_start) = self.render_start_time else {
                    return;
                };
                if render_start.elapsed().as_secs_f32() < BUILD_DELAY_SECONDS {
                    return;
                }
                match scene_plan()
                    .context("compile deterministic Glass voxel test scene")
                    .and_then(|plan| self.execute_edit_plan(plan))
                {
                    Ok(()) => TestScenePhase::TerrainPublished,
                    Err(err) => {
                        log::error!("[GLASS_VOXEL_TEST] construction failed: {err:#}");
                        TestScenePhase::Failed
                    }
                }
            }
            TestScenePhase::TerrainPublished => {
                let glass_count = self
                    .validate_glass_voxel_atlas_state()
                    .unwrap_or_else(|err| {
                        panic!("[GLASS_VOXEL_TEST] atlas validation failed: {err:#}")
                    });
                let terrain_revision = self
                    .observe_initial_published_terrain_for_ddgi()
                    .unwrap_or_else(|err| {
                        panic!("[GLASS_VOXEL_TEST] DDGI visibility publication failed: {err:#}")
                    });
                self.validate_gpu_scene_query_capture()
                    .unwrap_or_else(|err| {
                        panic!("[GLASS_VOXEL_TEST] GPU SceneQuery validation failed: {err:#}")
                    });
                log::info!(
                    "[GLASS_VOXEL_TEST] terrain published revision={} canonical_glass_voxels={} settling_frames={}",
                    terrain_revision,
                    glass_count,
                    SETTLE_FRAMES,
                );
                TestScenePhase::Settling {
                    frames: SETTLE_FRAMES,
                    terrain_revision,
                }
            }
            TestScenePhase::Settling {
                frames,
                terrain_revision,
            } => {
                if frames > 1 {
                    TestScenePhase::Settling {
                        frames: frames - 1,
                        terrain_revision,
                    }
                } else {
                    TestScenePhase::WaitingForProbeField { terrain_revision }
                }
            }
            TestScenePhase::WaitingForProbeField { terrain_revision } => {
                if !self
                    .tracer
                    .environment_probe_terrain_revision_ready(terrain_revision)
                {
                    return;
                }
                let glass_debug = self
                    .tracer
                    .capture_glass_debug_summary()
                    .unwrap_or_else(|err| {
                        panic!("[GLASS_VOXEL_TEST] debug readback failed: {err:#}")
                    });
                assert!(
                    glass_debug.raster_screen_hit_pixels > 0,
                    "[GLASS_VOXEL_TEST] Glass never resolved raster geometry behind it"
                );
                assert!(
                    glass_debug.foreground_pixels > 0,
                    "[GLASS_VOXEL_TEST] foreground raster sentinel was not preserved"
                );
                assert_eq!(
                    glass_debug.exhaustion_pixels, 0,
                    "[GLASS_VOXEL_TEST] authored scene exhausted a Glass path budget"
                );
                log::info!(
                    "[GLASS_VOXEL_TEST][FRAME] extent={}x{} glass_pixels={} foreground_pixels={} screen_hit_pixels={} raster_screen_hit_pixels={} fallback_pixels={} query_budget_fallback_pixels={} exhaustion_pixels={} scene_queries_max={} interfaces_max={} dda_steps_median={} dda_steps_p95={} dda_steps_max={}",
                    glass_debug.extent.width,
                    glass_debug.extent.height,
                    glass_debug.glass_pixels,
                    glass_debug.foreground_pixels,
                    glass_debug.screen_hit_pixels,
                    glass_debug.raster_screen_hit_pixels,
                    glass_debug.fallback_pixels,
                    glass_debug.query_budget_fallback_pixels,
                    glass_debug.exhaustion_pixels,
                    glass_debug.scene_queries_max,
                    glass_debug.interfaces_max,
                    glass_debug.dda_steps_median,
                    glass_debug.dda_steps_p95,
                    glass_debug.dda_steps_max,
                );
                log::info!(
                    "[GLASS_VOXEL_TEST] ready revision={} acceptance=canonical-id3-isolated-scene",
                    terrain_revision,
                );
                TestScenePhase::Ready
            }
            TestScenePhase::Ready | TestScenePhase::Failed => return,
        };

        self.glass_voxel_test_scene
            .as_mut()
            .expect("Glass voxel test scene state disappeared")
            .phase = next_phase;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::world_edits::{VoxelAtlasStateWrite, VoxelEdit};
    use crate::builder::VOXEL_TYPE_SAND;

    #[test]
    fn scene_plan_authors_only_canonical_sand_for_experimental_glass() {
        let plan = scene_plan().unwrap();
        let glass_edits = plan
            .voxel_edits
            .iter()
            .filter_map(|edit| match edit {
                VoxelEdit::StampCuboids {
                    voxel_type,
                    atlas_state_write,
                    ..
                } if *voxel_type == VOXEL_TYPE_SAND => Some(*atlas_state_write),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(!glass_edits.is_empty());
        assert!(glass_edits
            .iter()
            .all(|policy| *policy == VoxelAtlasStateWrite::Clear));
    }
}
