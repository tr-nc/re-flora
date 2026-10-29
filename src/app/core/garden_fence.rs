//! Editable voxel posts and free-form rope links. Only post identities are
//! metadata; ropes are ordinary terrain and persist/edit independently.
use super::{player_tools::PlayerTool, App};
use crate::{
    app::world_edits::{VoxelEdit, WorldEditTransaction},
    builder::{VOXEL_TYPE_DIRT, VOXEL_TYPE_MASK, VOXEL_TYPE_OAK_WOOD, VOXEL_TYPE_ROCK},
    geom::{build_bvh, Cuboid, UAabb3},
};
use anyhow::{ensure, Result};
use glam::{IVec3, UVec3, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};

const MAX_POSTS: usize = 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Post {
    id: u32,
    base: [u32; 3],
    height: u32,
    radius: u32,
    seed: u32,
}
impl Post {
    fn base(&self) -> UVec3 {
        self.base.into()
    }
    fn center(&self, y: u32) -> IVec3 {
        let segment = (y / 8).min(7);
        let hash = self.seed.rotate_left(segment * 3);
        self.base().as_ivec3()
            + IVec3::new(
                if segment == 0 {
                    0
                } else {
                    (hash % 3) as i32 - 1
                },
                y as i32,
                if segment == 0 {
                    0
                } else {
                    ((hash >> 5) % 3) as i32 - 1
                },
            )
    }
    fn top(&self) -> UVec3 {
        self.center(self.height - 1).as_uvec3()
    }
    fn anchor(&self) -> Vec3 {
        (self.center(self.height - 6).as_vec3() + Vec3::splat(0.5)) / 256.
    }
    fn contains(&self, p: Vec3) -> bool {
        let delta = p * 256. - self.base().as_vec3();
        delta.y >= 0.
            && delta.y <= self.height as f32 + 1.
            && delta.x.abs() < self.radius as f32 + 2.
            && delta.z.abs() < self.radius as f32 + 2.
    }
    fn radius_at(&self, y: u32) -> u32 {
        if y > self.height * 2 / 3 {
            self.radius.saturating_sub(1).max(1)
        } else {
            self.radius
        }
    }
    fn cells(&self) -> BTreeSet<[u32; 3]> {
        let mut cells = BTreeSet::new();
        for y in 0..self.height {
            let r = self.radius_at(y) as i32;
            let c = self.center(y);
            for z in -r..=r {
                for x in -r..=r {
                    if x * x + z * z <= r * r {
                        cells.insert((c + IVec3::new(x, 0, z)).as_uvec3().to_array());
                    }
                }
            }
        }
        cells
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct FenceSnapshot {
    next_id: u32,
    posts: Vec<Post>,
}
impl Default for FenceSnapshot {
    fn default() -> Self {
        Self {
            next_id: 1,
            posts: Vec::new(),
        }
    }
}
impl FenceSnapshot {
    pub(super) fn validate(&self, world: UVec3) -> Result<()> {
        ensure!(
            self.posts.len() <= MAX_POSTS && self.next_id > 0,
            "invalid fence population"
        );
        let mut ids = HashSet::new();
        for p in &self.posts {
            ensure!(
                p.id > 0
                    && p.id < self.next_id
                    && ids.insert(p.id)
                    && (48..=64).contains(&p.height)
                    && (2..=5).contains(&p.radius),
                "invalid post identity/recipe"
            );
            let base = p.base();
            let margin = p.radius + 2;
            ensure!(
                base.cmpge(UVec3::new(margin, 0, margin)).all()
                    && base.cmplt(world).all()
                    && (base + UVec3::new(margin, p.height, margin))
                        .cmplt(world)
                        .all(),
                "post exceeds world"
            );
        }
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct GardenFence {
    pub(super) saved: FenceSnapshot,
    pending: Option<u32>,
    dirty: HashSet<u32>,
}
impl GardenFence {
    pub(super) fn restore(&mut self, saved: FenceSnapshot) {
        self.dirty = saved.posts.iter().map(|p| p.id).collect();
        self.saved = saved;
        self.pending = None;
    }
    pub(super) fn observe_edit(&mut self, bound: UAabb3) {
        for p in &self.saved.posts {
            let top = p.top();
            if top.cmpge(bound.min()).all() && top.cmplt(bound.max()).all() {
                self.dirty.insert(p.id);
            }
        }
    }
}
fn bounds(cells: &BTreeSet<[u32; 3]>) -> Result<UAabb3> {
    ensure!(!cells.is_empty(), "empty fence geometry");
    let lo = cells.iter().fold(UVec3::MAX, |a, p| a.min((*p).into()));
    let hi = cells.iter().fold(UVec3::ZERO, |a, p| a.max((*p).into())) + UVec3::ONE;
    Ok(UAabb3::new(lo, hi))
}
fn rope_cells(a: Vec3, b: Vec3, world: UVec3) -> Result<BTreeSet<[u32; 3]>> {
    ensure!(a.is_finite() && b.is_finite(), "invalid rope endpoint");
    ensure!(
        [a, b]
            .into_iter()
            .all(|p| p.cmpge(Vec3::ZERO).all() && (p * 256.).cmplt(world.as_vec3()).all()),
        "rope endpoint outside garden"
    );
    let length = a.distance(b) * 256.;
    ensure!(
        (4. ..1500.).contains(&length),
        "rope length outside supported garden span"
    );
    let sag = (length * 0.12).clamp(2., 32.) / 256.;
    let steps = (length * 3.).ceil() as u32;
    let mut cells = BTreeSet::new();
    let mut previous = (a * 256.).floor().as_ivec3();
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let next = ((a.lerp(b, t) - Vec3::Y * (4. * sag * t * (1. - t))) * 256.)
            .floor()
            .as_ivec3();
        // A supercover staircase keeps diagonal rope cells face-connected.
        for axis in 0..3 {
            while previous[axis] != next[axis] {
                previous[axis] += (next[axis] - previous[axis]).signum();
                ensure!(
                    previous.cmpge(IVec3::ZERO).all() && previous.cmplt(world.as_ivec3()).all(),
                    "rope leaves editable terrain"
                );
                cells.insert(previous.as_uvec3().to_array());
            }
        }
        ensure!(
            next.cmpge(IVec3::ZERO).all() && next.cmplt(world.as_ivec3()).all(),
            "rope leaves editable terrain"
        );
        cells.insert(next.as_uvec3().to_array());
    }
    Ok(cells)
}
pub(super) fn stamp(cells: &BTreeSet<[u32; 3]>, material: u32) -> Result<VoxelEdit> {
    let mut columns = BTreeMap::<(u32, u32), Vec<u32>>::new();
    for &[x, y, z] in cells {
        columns.entry((x, z)).or_default().push(y);
    }
    let mut cuboids = Vec::new();
    for ((x, z), ys) in columns {
        let mut first = ys[0];
        let mut last = first;
        for y in ys.into_iter().skip(1).chain(std::iter::once(u32::MAX)) {
            if y == last + 1 {
                last = y;
                continue;
            }
            cuboids.push(Cuboid::from_min_max(
                Vec3::new(x as f32, first as f32, z as f32),
                Vec3::new((x + 1) as f32, (last + 1) as f32, (z + 1) as f32),
            ));
            first = y;
            last = y;
        }
    }
    let aabbs: Vec<_> = cuboids.iter().map(Cuboid::aabb).collect();
    let leaves: Vec<_> = (0..cuboids.len() as u32).collect();
    let bvh_nodes = build_bvh(&aabbs, &leaves).map_err(anyhow::Error::msg)?;
    Ok(VoxelEdit::StampCuboids {
        bvh_nodes,
        cuboids,
        voxel_type: material,
        atlas_state_write: Default::default(),
    })
}
impl App {
    pub(super) fn update_garden_fence_anchors(&mut self) -> Result<()> {
        if self.garden_fence.dirty.is_empty() {
            return Ok(());
        }
        let posts: Vec<_> = self
            .garden_fence
            .saved
            .posts
            .iter()
            .filter(|p| self.garden_fence.dirty.contains(&p.id))
            .map(|p| (p.id, p.top()))
            .collect();
        let regions: Vec<_> = posts.iter().map(|(_, top)| (*top, UVec3::ONE)).collect();
        // One batched byte read per changed cap, only after an intersecting edit;
        // no full terrain scan and no GPU wait during ordinary idle frames.
        let data = self.plain_builder.read_chunk_atlas_regions(&regions)?;
        let removed: HashSet<_> = posts
            .iter()
            .zip(data)
            .filter_map(|((id, _), bytes)| {
                (u32::from(bytes[0] & VOXEL_TYPE_MASK) != VOXEL_TYPE_OAK_WOOD).then_some(*id)
            })
            .collect();
        self.garden_fence
            .saved
            .posts
            .retain(|p| !removed.contains(&p.id));
        if self
            .garden_fence
            .pending
            .is_some_and(|id| removed.contains(&id))
        {
            self.garden_fence.pending = None;
        }
        self.garden_fence.dirty.clear();
        if !removed.is_empty() {
            log::info!(
                "[GARDEN_FENCE] retired_posts={} next_id={} ropes=editable_terrain",
                removed.len(),
                self.garden_fence.saved.next_id
            );
        }
        Ok(())
    }
    fn fence_world(&self) -> UVec3 {
        self.world_chunk_dim * super::VOXEL_DIM_PER_CHUNK
    }
    fn post_is_intact(&mut self, p: &Post) -> Result<bool> {
        let bytes = self
            .plain_builder
            .read_chunk_atlas_region(p.top(), UVec3::ONE)?;
        Ok(u32::from(bytes[0] & VOXEL_TYPE_MASK) == VOXEL_TYPE_OAK_WOOD)
    }
    fn commit_fence_cells(
        &mut self,
        cells: &BTreeSet<[u32; 3]>,
        footing: Option<u32>,
    ) -> Result<()> {
        let bound = bounds(cells)?;
        let dim = bound.max() - bound.min();
        ensure!(
            bound.max().cmple(self.fence_world()).all(),
            "fence leaves editable terrain"
        );
        let before = self
            .plain_builder
            .read_chunk_atlas_region(bound.min(), dim)?;
        for cell in cells {
            let p = UVec3::from(*cell);
            let q = p - bound.min();
            let existing =
                u32::from(before[(q.x + dim.x * (q.y + dim.y * q.z)) as usize] & VOXEL_TYPE_MASK);
            let allowed = existing == 0
                || match footing {
                    Some(y) => p.y < y + 4 && matches!(existing, VOXEL_TYPE_DIRT | VOXEL_TYPE_ROCK),
                    None => existing == VOXEL_TYPE_OAK_WOOD,
                };
            ensure!(allowed, "fence obstructed at {p:?}");
        }
        self.execute_world_edit(WorldEditTransaction::terrain_change(
            vec![stamp(cells, VOXEL_TYPE_OAK_WOOD)?],
            bound,
        ))?;
        self.observe_player_terrain_publication_for_connectivity(bound);
        Ok(())
    }
    pub(super) fn place_garden_post(&mut self, hit: Vec3, thick: bool) -> Result<u32> {
        self.update_garden_fence_anchors()?;
        ensure!(
            hit.is_finite() && self.garden_fence.saved.posts.len() < MAX_POSTS,
            "invalid/full post placement"
        );
        let id = self.garden_fence.saved.next_id;
        let next = id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("post identities exhausted"))?;
        let seed = id.wrapping_mul(0x9e3779b9);
        let v = hit * 256.;
        ensure!(
            v.cmpge(Vec3::ONE).all() && v.cmplt(self.fence_world().as_vec3()).all(),
            "post contact outside garden"
        );
        let post = Post {
            id,
            base: [
                v.x.floor() as u32,
                v.y.ceil() as u32 - 1,
                v.z.floor() as u32,
            ],
            height: 48 + seed % 17,
            radius: if thick {
                4 + (seed >> 8) % 2
            } else {
                2 + (seed >> 8) % 2
            },
            seed,
        };
        let candidate = FenceSnapshot {
            next_id: next,
            posts: vec![post.clone()],
        };
        candidate.validate(self.fence_world())?;
        let cells = post.cells();
        self.commit_fence_cells(&cells, Some(post.base[1]))?;
        log::info!(
            "[GARDEN_FENCE] post={} thick={} height={} radius={} cells={} actual_terrain=true",
            id,
            thick,
            post.height,
            post.radius,
            cells.len()
        );
        self.garden_fence.saved.posts.push(post);
        self.garden_fence.saved.next_id = next;
        Ok(id)
    }
    fn link_garden_posts(&mut self, from: u32, to: u32) -> Result<usize> {
        ensure!(from != to, "rope needs two different posts");
        let a = self
            .garden_fence
            .saved
            .posts
            .iter()
            .find(|p| p.id == from)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("first post no longer exists"))?;
        let b = self
            .garden_fence
            .saved
            .posts
            .iter()
            .find(|p| p.id == to)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("second post no longer exists"))?;
        ensure!(
            self.post_is_intact(&a)? && self.post_is_intact(&b)?,
            "rope endpoint was edited away"
        );
        let cells = rope_cells(a.anchor(), b.anchor(), self.fence_world())?;
        self.commit_fence_cells(&cells, None)?;
        log::info!(
            "[GARDEN_FENCE] rope={from}->{to} cells={} actual_terrain=true",
            cells.len()
        );
        Ok(cells.len())
    }
    pub(super) fn try_garden_fence_action(&mut self) {
        if !self.terrain_edit_pointer_available()
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
        let tool = self.player_tools.selected_tool();
        let result = if tool == PlayerTool::Rope {
            let id = self
                .garden_fence
                .saved
                .posts
                .iter()
                .filter(|p| p.contains(hit.world_position))
                .min_by(|a, b| {
                    a.anchor()
                        .distance_squared(hit.world_position)
                        .total_cmp(&b.anchor().distance_squared(hit.world_position))
                })
                .map(|p| p.id);
            let Some(id) = id else {
                return;
            };
            match self.garden_fence.pending {
                None => {
                    self.garden_fence.pending = Some(id);
                    log::info!("[GARDEN_FENCE] rope_start={id}");
                    Ok(())
                }
                Some(previous) if previous == id => {
                    self.garden_fence.pending = None;
                    Ok(())
                }
                Some(previous) => self
                    .link_garden_posts(previous, id)
                    .map(|_| self.garden_fence.pending = Some(id)),
            }
        } else if matches!(tool, PlayerTool::ThinPole | PlayerTool::ThickPole) {
            let snapshot = self.contree_builder.cpu_ray_query_snapshot();
            let normal = super::stone_placement::surface_normal(
                hit.world_position,
                pick.ray.direction,
                |o, d| snapshot.query_terrain_ray_cpu(o, d).map(|h| h.position),
            );
            if normal.is_none_or(|n| n.y < 0.75) {
                return;
            }
            self.place_garden_post(hit.world_position, tool == PlayerTool::ThickPole)
                .map(|_| ())
        } else {
            return;
        };
        if let Err(e) = result {
            self.garden_fence.pending = None;
            log::warn!("[GARDEN_FENCE] action rejected: {e:#}");
        }
    }
    pub(super) fn seed_garden_fence_review(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_FENCE_REVIEW") else {
            return Ok(());
        };
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        if mode != "load" {
            for (x, z, thick) in [(0.65, 1.16, false), (0.87, 1.10, true), (1.02, 1.28, false)] {
                let y = self.query_terrain_height_cpu(glam::Vec2::new(x, z));
                self.place_garden_post(Vec3::new(x, y, z), thick)?;
            }
            self.link_garden_posts(1, 2)?;
            self.link_garden_posts(2, 3)?;
        }
        self.garden_fence.saved.validate(self.fence_world())?;
        ensure!(
            self.garden_fence.saved.posts.len() == 3,
            "fence review post count"
        );
        let posts = self.garden_fence.saved.posts.clone();
        let mut verified = 0;
        for pair in posts.windows(2) {
            let cells = rope_cells(pair[0].anchor(), pair[1].anchor(), self.fence_world())?;
            let bound = bounds(&cells)?;
            let dim = bound.max() - bound.min();
            let bytes = self
                .plain_builder
                .read_chunk_atlas_region(bound.min(), dim)?;
            for p in cells {
                let q = UVec3::from(p) - bound.min();
                ensure!(
                    u32::from(
                        bytes[(q.x + dim.x * (q.y + dim.y * q.z)) as usize] & VOXEL_TYPE_MASK
                    ) == VOXEL_TYPE_OAK_WOOD,
                    "rope missing after placement/restore"
                );
                verified += 1;
            }
        }
        if mode == "edit" {
            let top = posts[0].top();
            let cells = BTreeSet::from([top.to_array()]);
            self.execute_world_edit(WorldEditTransaction::terrain_change(
                vec![stamp(&cells, 0)?],
                UAabb3::new(top, top + UVec3::ONE),
            ))?;
            ensure!(
                self.link_garden_posts(1, 3).is_err(),
                "missing anchor accepted"
            );
            self.update_garden_fence_anchors()?;
            ensure!(
                self.garden_fence.saved.posts.len() == 2 && self.garden_fence.saved.next_id == 4,
                "deleted post identity was not retired safely"
            );
        }
        self.camera_control.apply_snapshot_mode(true);
        self.camera_control
            .set_orbit_focus(Vec3::new(0.84, 0.49, 1.19));
        self.reset_camera_movement_input();
        self.tracer
            .set_camera_pose_looking_at(Vec3::new(0.53, 0.72, 1.66), Vec3::new(0.84, 0.49, 1.19));
        log::info!("[GARDEN_FENCE_REVIEW] mode={mode} verified=true initial_posts=3 remaining_posts={} rope_cells={verified} thin_thick=true arbitrary_chain=true",self.garden_fence.saved.posts.len());
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pole_recipe_varies_and_snapshot_rejects_duplicate_ids_and_bad_geometry() {
        let p = Post {
            id: 1,
            base: [128, 80, 128],
            height: 55,
            radius: 3,
            seed: 123,
        };
        let mut q = p.clone();
        q.id = 2;
        q.seed = 456;
        q.radius = 5;
        q.height = 61;
        assert_ne!(p.cells(), q.cells());
        let saved = FenceSnapshot {
            next_id: 3,
            posts: vec![p.clone(), q],
        };
        assert!(saved.validate(UVec3::splat(512)).is_ok());
        let json = serde_json::to_vec(&saved).unwrap();
        let restored: FenceSnapshot = serde_json::from_slice(&json).unwrap();
        assert_eq!(restored.posts[0].cells(), p.cells());
        let bad = FenceSnapshot {
            next_id: 3,
            posts: vec![p.clone(), p],
        };
        assert!(bad.validate(UVec3::splat(512)).is_err());
    }
    #[test]
    fn post_invalidation_is_local_and_does_not_reuse_deleted_identity() {
        let p = Post {
            id: 7,
            base: [128, 80, 128],
            height: 55,
            radius: 3,
            seed: 42,
        };
        let mut f = GardenFence::default();
        f.saved.next_id = 8;
        f.saved.posts.push(p.clone());
        f.observe_edit(UAabb3::new(UVec3::ZERO, UVec3::splat(8)));
        assert!(f.dirty.is_empty());
        f.observe_edit(UAabb3::new(p.top(), p.top() + UVec3::ONE));
        assert_eq!(f.dirty, HashSet::from([7]));
        assert_eq!(f.saved.next_id, 8);
        let mut bad = p;
        bad.base = [u32::MAX; 3];
        assert!(FenceSnapshot {
            next_id: 8,
            posts: vec![bad]
        }
        .validate(UVec3::splat(512))
        .is_err());
    }
    #[test]
    fn rope_is_sagged_face_connected_bounded_and_direction_independent_in_endpoints() {
        let a = Vec3::new(0.3, 0.6, 0.4);
        let b = Vec3::new(1.1, 0.7, 1.2);
        let cells = rope_cells(a, b, UVec3::splat(512)).unwrap();
        assert!(cells.iter().any(|p| p[1] as f32 / 256. < a.y));
        for end in [a, b] {
            assert!(cells.contains(&(end * 256.).floor().as_uvec3().to_array()));
        }
        let first = *cells.first().unwrap();
        let mut visited = BTreeSet::from([first]);
        let mut stack = vec![IVec3::from_array(first.map(|v| v as i32))];
        while let Some(p) = stack.pop() {
            for d in [
                IVec3::X,
                IVec3::Y,
                IVec3::Z,
                -IVec3::X,
                -IVec3::Y,
                -IVec3::Z,
            ] {
                let q = p + d;
                let key = q.as_uvec3().to_array();
                if cells.contains(&key) && visited.insert(key) {
                    stack.push(q);
                }
            }
        }
        assert_eq!(visited.len(), cells.len());
        assert!(rope_cells(Vec3::ZERO, Vec3::new(1., 0., 1.), UVec3::splat(512)).is_err());
        assert!(rope_cells(a, a, UVec3::splat(512)).is_err());
    }
}
