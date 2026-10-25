//! Player-placeable stone lanterns are ordinary editable terrain, not raster
//! props. The existing visible-terrain owner publishes collision, flora, shadows,
//! emissive voxels and persistence together.
use super::{player_tools::PlayerTool, App};
use crate::{
    app::world_edits::{VoxelEdit, WorldEditTransaction},
    builder::{VOXEL_TYPE_DIRT, VOXEL_TYPE_EMISSIVE, VOXEL_TYPE_MASK, VOXEL_TYPE_ROCK},
    geom::{build_bvh, Cuboid, UAabb3},
};
use anyhow::{ensure, Context, Result};
use glam::{IVec3, UVec3, Vec3};

const FOOTING_HEIGHT: i32 = 6;

#[derive(Clone, Copy)]
struct Part {
    lo: IVec3,
    hi: IVec3,
    material: u32,
}
fn parts() -> Vec<Part> {
    let mut p = Vec::new();
    let mut stone = |lo, hi| {
        p.push(Part {
            lo: IVec3::from_array(lo),
            hi: IVec3::from_array(hi),
            material: VOXEL_TYPE_ROCK,
        })
    };
    // Broad foot, tapering plinth and slender pedestal.
    stone([-12, 0, -12], [13, 3, 13]);
    stone([-9, 3, -9], [10, FOOTING_HEIGHT, 10]);
    stone([-4, FOOTING_HEIGHT, -4], [5, 21, 5]);
    stone([-8, 21, -8], [9, 24, 9]);
    // Four genuinely open windows around a glowing core, not painted faces.
    for x in [-8, 6] {
        for z in [-8, 6] {
            stone([x, 24, z], [x + 3, 36, z + 3]);
        }
    }
    stone([-9, 36, -9], [10, 38, 10]);
    stone([-1, 24, -1], [2, 26, 2]);
    // Pagoda-like stepped overhang and finial.
    for (y, r) in [(38, 14), (40, 11), (42, 8), (44, 5)] {
        stone([-r, y, -r], [r + 1, y + 2, r + 1]);
    }
    stone([-2, 46, -2], [3, 49, 3]);
    p.push(Part {
        lo: IVec3::new(-2, 26, -2),
        hi: IVec3::new(3, 32, 3),
        material: VOXEL_TYPE_EMISSIVE,
    });
    p
}
struct Plan {
    base: IVec3,
    bound: UAabb3,
    parts: Vec<Part>,
}
impl Plan {
    fn new(hit: Vec3, world_dim: UVec3) -> Result<Self> {
        ensure!(hit.is_finite(), "invalid lantern contact");
        let v = hit * 256.;
        let base = IVec3::new(
            v.x.floor() as i32,
            v.y.ceil() as i32 - 1,
            v.z.floor() as i32,
        );
        let parts = parts();
        let lo = parts.iter().fold(IVec3::MAX, |a, p| a.min(base + p.lo));
        let hi = parts.iter().fold(IVec3::MIN, |a, p| a.max(base + p.hi));
        ensure!(
            lo.cmpge(IVec3::ZERO).all() && hi.cmple(world_dim.as_ivec3()).all(),
            "lantern would extend beyond editable terrain"
        );
        Ok(Self {
            base,
            bound: UAabb3::new(lo.as_uvec3(), hi.as_uvec3()),
            parts,
        })
    }
    fn material_at(&self, world: IVec3) -> u32 {
        let p = world - self.base;
        self.parts
            .iter()
            .rev()
            .find(|part| p.cmpge(part.lo).all() && p.cmplt(part.hi).all())
            .map_or(0, |p| p.material)
    }
    fn edits(&self) -> Result<Vec<VoxelEdit>> {
        [VOXEL_TYPE_ROCK, VOXEL_TYPE_EMISSIVE]
            .into_iter()
            .map(|material| {
                let cuboids: Vec<_> = self
                    .parts
                    .iter()
                    .filter(|p| p.material == material)
                    .map(|p| {
                        Cuboid::from_min_max(
                            (self.base + p.lo).as_vec3(),
                            (self.base + p.hi).as_vec3(),
                        )
                    })
                    .collect();
                let aabbs: Vec<_> = cuboids.iter().map(Cuboid::aabb).collect();
                let leaves: Vec<_> = (0..cuboids.len() as u32).collect();
                let bvh_nodes = build_bvh(&aabbs, &leaves).map_err(anyhow::Error::msg)?;
                Ok(VoxelEdit::StampCuboids {
                    bvh_nodes,
                    cuboids,
                    voxel_type: material,
                    atlas_state_write: Default::default(),
                })
            })
            .collect()
    }
    fn validate_clearance(&self, bytes: &[u8]) -> Result<()> {
        let dim = self.bound.max() - self.bound.min();
        ensure!(
            bytes.len() == dim.element_product() as usize,
            "invalid lantern clearance read"
        );
        for z in 0..dim.z {
            for y in 0..dim.y {
                for x in 0..dim.x {
                    let p = self.bound.min() + UVec3::new(x, y, z);
                    let existing =
                        u32::from(bytes[(x + dim.x * (y + dim.y * z)) as usize] & VOXEL_TYPE_MASK);
                    // The two base courses may embed in a gently uneven soil/rock
                    // surface. Nothing above it (including windows) is overwritten.
                    let footing = p.y < (self.base.y + FOOTING_HEIGHT) as u32
                        && matches!(existing, VOXEL_TYPE_DIRT | VOXEL_TYPE_ROCK);
                    let material = self.material_at(p.as_ivec3());
                    let local = p.as_ivec3() - self.base;
                    let window =
                        (24..36).contains(&local.y) && local.x.abs() <= 8 && local.z.abs() <= 8;
                    if existing != 0 && !footing && (material != 0 || window) {
                        anyhow::bail!("lantern needs a clear upright space at {p:?}");
                    }
                    if existing != 0 && material != 0 {
                        ensure!(
                            existing == VOXEL_TYPE_DIRT || existing == VOXEL_TYPE_ROCK,
                            "lantern foot would overwrite another material"
                        );
                    }
                }
            }
        }
        Ok(())
    }
}
impl App {
    pub(super) fn try_lantern_placement(&mut self) {
        if self.player_tools.selected_tool() != PlayerTool::Lantern
            || !self.terrain_edit_pointer_available()
            || !self.terrain_persistence.can_start_operation()
            || self.rooftop_scene.is_some()
        {
            return;
        }
        let Some(pick) = self.pointer_picking.active else {
            return;
        };
        let Some(hit) = pick.hit else {
            return;
        };
        if hit.tree_primitive.is_some()
            || !(0.0..=super::SHOVEL_RAY_QUERY_DISTANCE).contains(&hit.distance)
        {
            return;
        }
        let snapshot = self.contree_builder.cpu_ray_query_snapshot();
        let Some(normal) = super::stone_placement::surface_normal(
            hit.world_position,
            pick.ray.direction,
            |o, d| snapshot.query_terrain_ray_cpu(o, d).map(|h| h.position),
        ) else {
            return;
        };
        if normal.y < 0.8 {
            log::info!("[STONE_LANTERN] rejected: upright ground required");
            return;
        }
        if let Err(e) = self.place_stone_lantern(hit.world_position) {
            log::warn!("[STONE_LANTERN] placement rejected: {e:#}");
        }
    }
    fn place_stone_lantern(&mut self, hit: Vec3) -> Result<Plan> {
        let plan = Plan::new(hit, self.world_chunk_dim * super::VOXEL_DIM_PER_CHUNK)?;
        let dim = plan.bound.max() - plan.bound.min();
        let before = self
            .plain_builder
            .read_chunk_atlas_region(plan.bound.min(), dim)?;
        plan.validate_clearance(&before)?;
        self.execute_world_edit(WorldEditTransaction::terrain_change(
            plan.edits()?,
            plan.bound,
        ))?;
        self.observe_player_terrain_publication_for_connectivity(plan.bound);
        log::info!("[STONE_LANTERN] placed base={:?} parts={} actual_terrain=true collision=terrain_pipeline emissive_provider=terrain persistence=atlas",plan.base,plan.parts.len());
        Ok(plan)
    }
    pub(super) fn seed_stone_lantern_review(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_LANTERN_REVIEW") else {
            return Ok(());
        };
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        ensure!(
            self.contree_builder.cpu_chunk_cache_jobs_idle(),
            "lantern review needs settled CPU terrain"
        );
        let xz = glam::Vec2::new(0.82, 1.22);
        let y = self.query_terrain_height_cpu(xz);
        let plan = if mode == "load" {
            // The fixture's center ray now hits the persisted roof, not its soil.
            Plan::new(
                Vec3::new(xz.x, y - 48. / 256., xz.y),
                self.world_chunk_dim * super::VOXEL_DIM_PER_CHUNK,
            )?
        } else {
            self.place_stone_lantern(Vec3::new(xz.x, y, xz.y))?
        };
        let dim = plan.bound.max() - plan.bound.min();
        let bytes = self
            .plain_builder
            .read_chunk_atlas_region(plan.bound.min(), dim)?;
        let mut stone = 0;
        let mut light = 0;
        let mut windows = 0;
        for z in 0..dim.z {
            for y in 0..dim.y {
                for x in 0..dim.x {
                    let world = plan.bound.min() + UVec3::new(x, y, z);
                    let expected = plan.material_at(world.as_ivec3());
                    let actual =
                        u32::from(bytes[(x + dim.x * (y + dim.y * z)) as usize] & VOXEL_TYPE_MASK);
                    if expected != 0 {
                        ensure!(
                            actual == expected,
                            "lantern atlas material mismatch at {world:?}"
                        );
                    }
                    if expected == VOXEL_TYPE_ROCK {
                        stone += 1;
                    }
                    if expected == VOXEL_TYPE_EMISSIVE {
                        light += 1;
                    }
                    let local = world.as_ivec3() - plan.base;
                    if local == IVec3::new(0, 29, -8) {
                        ensure!(actual == 0, "lantern window closed");
                        windows += 1;
                    }
                }
            }
        }
        ensure!(
            stone > 1000 && light == 150 && windows == 1,
            "lantern native acceptance failed"
        );
        // CPU collision/query caches publish asynchronously after the visible
        // edit. Only this opt-in acceptance fixture waits for their readiness.
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        ensure!(
            self.contree_builder.cpu_chunk_cache_jobs_idle(),
            "lantern CPU terrain did not settle"
        );
        let roof_ray = Vec3::new(xz.x, (plan.base.y + 60) as f32 / 256., xz.y);
        let roof = self
            .contree_builder
            .query_terrain_ray_cpu(roof_ray, Vec3::NEG_Y)
            .context("lantern physical ray missed")?;
        ensure!(
            (roof.position.y - (plan.base.y + 49) as f32 / 256.).abs() < 2. / 256.,
            "lantern physical roof ray mismatch: actual={:?} expected_y={}",
            roof.position,
            (plan.base.y + 49) as f32 / 256.
        );
        if mode == "edit" {
            let lo = (plan.base + IVec3::new(-2, 26, -2)).as_uvec3();
            let hi = (plan.base + IVec3::new(3, 32, 3)).as_uvec3();
            let cuboid = Cuboid::from_min_max(lo.as_vec3(), hi.as_vec3());
            let bvh_nodes = build_bvh(&[cuboid.aabb()], &[0]).map_err(anyhow::Error::msg)?;
            self.execute_world_edit(WorldEditTransaction::terrain_change(
                vec![VoxelEdit::StampCuboids {
                    bvh_nodes,
                    cuboids: vec![cuboid],
                    voxel_type: 0,
                    atlas_state_write: Default::default(),
                }],
                UAabb3::new(lo, hi),
            ))?;
            let removed = self.plain_builder.read_chunk_atlas_region(lo, hi - lo)?;
            ensure!(
                removed.iter().all(|b| b & VOXEL_TYPE_MASK == 0),
                "lantern light core was not editable"
            );
            log::info!("[STONE_LANTERN_REVIEW] core_removed=150 ordinary_terrain_edit=true");
        }
        log::info!("[STONE_LANTERN_REVIEW] verified=true mode={mode} stone={stone} emissive={light} open_windows=true physical_roof_ray=true");
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lantern_has_a_stone_pedestal_roof_and_open_windows_around_a_luminous_core() {
        let p = Plan::new(Vec3::ONE, UVec3::splat(512)).unwrap();
        for offset in [
            IVec3::new(0, 1, 0),
            IVec3::new(0, 12, 0),
            IVec3::new(0, 38, 0),
            IVec3::new(0, 47, 0),
        ] {
            assert_eq!(p.material_at(p.base + offset), VOXEL_TYPE_ROCK);
        }
        for offset in [
            IVec3::new(0, 29, -8),
            IVec3::new(0, 29, 8),
            IVec3::new(-8, 29, 0),
            IVec3::new(8, 29, 0),
        ] {
            assert_eq!(p.material_at(p.base + offset), 0);
        }
        assert_eq!(
            p.material_at(p.base + IVec3::new(0, 29, 0)),
            VOXEL_TYPE_EMISSIVE
        );
        assert!(Plan::new(Vec3::ZERO, UVec3::splat(512)).is_err());
        assert!(Plan::new(Vec3::splat(2.), UVec3::splat(512)).is_err());
    }
    #[test]
    fn obstruction_rejects_before_any_edit_and_soil_foot_embedding_is_allowed() {
        let p = Plan::new(Vec3::ONE, UVec3::splat(512)).unwrap();
        let d = p.bound.max() - p.bound.min();
        let mut bytes = vec![0; d.element_product() as usize];
        let index = |v: IVec3| {
            let q = (p.base + v).as_uvec3() - p.bound.min();
            (q.x + d.x * (q.y + d.y * q.z)) as usize
        };
        bytes[index(IVec3::ZERO)] = VOXEL_TYPE_DIRT as u8;
        assert!(p.validate_clearance(&bytes).is_ok());
        bytes[index(IVec3::new(0, 29, 0))] = VOXEL_TYPE_ROCK as u8;
        assert!(p.validate_clearance(&bytes).is_err());
    }
}
