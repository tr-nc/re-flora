//! One opt-in playable wall vine, independent of tree/grass ownership and snapshot history.
use super::App;
use crate::app::world_edits::{VoxelEdit, WorldEditTransaction};
use crate::builder::{
    ContreeCpuVoxelBlock, ContreeCpuVoxelBlockExport, ContreeCpuVoxelSourceSnapshot,
};
use crate::builder::{VOXEL_TYPE_EMPTY, VOXEL_TYPE_LIMESTONE};
use crate::climbing_plants::{fixtures::Fixture, Plant, SearchDirection, Terrain};
use crate::geom::{build_bvh, Cuboid, UAabb3};
use crate::tracer::DynamicFruitRenderInstance;
use anyhow::Result;
use glam::{IVec3, Mat3, Quat, UVec3, Vec3};
use std::cell::Cell;
use std::sync::Arc;
use std::time::Instant;

mod review;
mod site;
mod tick;
use site::Site;
use tick::{Action, Cadence, Event, Search, Snapshot, Tuning, VineTick};

const PLAYABLE_VINE_SEED: u64 = 3500;

#[derive(Default)]
pub(super) struct ClimbingPlants {
    vine: Option<VineTick>,
    created: bool,
    awaiting_seed: bool,
    fixture: Fixture,
    last_selection: Option<(Fixture, u64)>,
    site: Option<Site>,
    seed: u64,
    direction: SearchDirection,
    pub reset_requested: bool,
    pub focus_requested: bool,
    pub refill_tip_requested: bool,
    waiting_for_terrain: bool,
    last_action: &'static str,
    instances: Vec<DynamicFruitRenderInstance>,
    collision_cache: CollisionPatchCache,
    review: review::Review,
}
impl ClimbingPlants {
    fn observe_selection(&mut self, fixture: Fixture, seed: u64) {
        let selection = (fixture, seed);
        if self
            .last_selection
            .replace(selection)
            .is_some_and(|old| old != selection)
        {
            self.reset_requested = true;
        }
    }

    fn plant(&self) -> Option<&Plant> {
        self.vine.as_ref().map(VineTick::plant)
    }

    pub(super) fn draw_actions(&mut self, ui: &mut egui::Ui) {
        let mut actions = Vec::new();
        ui.small("3 / Dig: remove the backing wall with LMB. The first unsupported step cuts off its whole branch above it, even if still attached higher up. Shift + wheel: brush size.");
        ui.horizontal(|ui| {
            if self.site.is_none() {
                if ui.button("Create vine wall and focus").clicked() {
                    self.reset_requested = true;
                }
            } else if ui.button("Restart wall and vine").clicked() {
                self.reset_requested = true;
            }
            if ui
                .add_enabled(
                    self.vine.is_some() || self.site.is_some(),
                    egui::Button::new("Focus vine"),
                )
                .clicked()
            {
                self.focus_requested = true;
            }
        });
        ui.small("Grow → Climbing Vine plants one session vine at the clicked terrain face. Test terrain restarts the demo vine; search controls change live.");
        if let Some(site) = self.site {
            let (min, max) = site.bounds();
            ui.small(format!(
                "Patch voxels: {:?}..{:?}",
                min.to_array(),
                max.to_array()
            ));
        }
        if self.waiting_for_terrain {
            ui.label("Waiting for current terrain collision data; simulation is held safely.");
        }
        if let Some(vine) = &self.vine {
            let plant = vine.plant();
            let attached = plant.anchors.iter().filter(|a| a.attached).count();
            let flexible = plant.nodes.iter().filter(|node| !node.fixed).count();
            ui.small(format!(
                "{} recent nodes (lighter green); continuous bending through compliant attachments",
                flexible
            ));
            ui.label(format!(
                "{} stem nodes · {} tips · {} attachments · live length {:.0}/{:.0} voxels",
                plant.nodes.len(),
                plant.tips.len(),
                attached,
                plant.live_arc(),
                plant.max_live_arc()
            ));
            if !plant.root_connected() {
                ui.label("Root disconnected: regrowth stopped. Reset to restore the root.");
            } else if plant.nodes.len() >= 512 || plant.live_arc() + 2.0 > plant.max_live_arc() {
                ui.label("Live stem limit reached. Pruning frees room to grow again.");
            } else if vine.growth_blocked() {
                ui.label("Searching for reachable support; large unsupported gaps stop extension.");
            } else {
                ui.label("Root connected. A cut leaves a new exploratory tip on the lower stem.");
            }
            ui.add_enabled_ui(!self.waiting_for_terrain, |ui| {
                ui.horizontal(|ui| {
                    if ui.add_enabled(plant.anchors.iter().any(|a| a.attached && a.node != 0), egui::Button::new("Prune highest attachment"))
                        .on_hover_text("Remove this attachment's stem and all descendants. The wall is unchanged; the cut can regrow.").clicked() {
                        actions.push(Action::PruneHighest);
                    }
                    if ui.add_enabled(plant.nodes.len() > 1, egui::Button::new("Prune back to root")).clicked() {
                        actions.push(Action::PruneToRoot);
                    }
                });
                if ui.add_enabled(plant.root_connected(), egui::Button::new("Disconnect root"))
                    .on_hover_text("Stop regrowth until reset. This mode prunes unsupported stems instead of simulating falling remnants.").clicked() {
                    actions.push(Action::DisconnectRoot);
                }
                ui.collapsing("Blocked-tip test", |ui| {
                    ui.small("Inserts real limestone through a tip. Blocked stem is pruned; the surviving shoot searches for a safe route.");
                    if ui.button("Refill terrain through tip").clicked() {
                        self.refill_tip_requested = true;
                    }
                });
            });
        }
        if let Some(vine) = &mut self.vine {
            for action in actions {
                vine.request(action);
            }
        }
        if !self.last_action.is_empty() {
            ui.label(self.last_action);
        }
    }

    pub fn has_history(&self) -> bool {
        self.vine.is_some()
    }
    pub fn observe_edit(&mut self, bound: UAabb3) {
        if let Some(vine) = &mut self.vine {
            vine.observe_edit(bound);
        }
    }
    fn observe_tick(&mut self, report: &tick::Report) {
        self.waiting_for_terrain = report.waiting_for_terrain;
        for event in &report.events {
            match event {
                Event::PrunedHighest(pruned) => {
                    self.last_action =
                        "Pruned the highest attachment and its upper branch; the cut can regrow.";
                    log::info!(
                        "[CLIMBING] manually pruned nodes={} buds={}",
                        pruned.removed,
                        pruned.buds
                    );
                }
                Event::PrunedToRoot(pruned) => {
                    self.last_action = "Pruned back to the root. Suitable wall allows new growth.";
                    log::info!(
                        "[CLIMBING] pruned to root nodes={} buds={}",
                        pruned.removed,
                        pruned.buds
                    );
                }
                Event::DisconnectedRoot => {
                    self.last_action =
                        "Disconnected root; reset the wall and vine to restore growth.";
                    log::info!("[CLIMBING] root disconnected; regrowth stopped until reset");
                }
                Event::SupportPruned {
                    pruned,
                    remaining_nodes,
                } => {
                    self.last_action =
                        "Missing or blocked wall: upper branches pruned. The rooted stump can explore again.";
                    log::info!(
                        "[CLIMBING] support lost: pruned_nodes={} buds={} remaining_nodes={remaining_nodes}",
                        pruned.removed,
                        pruned.buds,
                    );
                }
            }
        }
    }

    /// World replacement drops history and prevents silently authoring a new wall on load.
    pub fn replace_world(&mut self) {
        *self = Self {
            created: true,
            ..Self::default()
        };
    }
}
/// The cache owns one immutable export; dependency readiness is checked on every use.
#[derive(Default)]
struct CollisionPatchCache {
    block: Option<Arc<ContreeCpuVoxelBlock>>,
}
impl CollisionPatchCache {
    fn query(
        &mut self,
        source: &ContreeCpuVoxelSourceSnapshot,
        min: UVec3,
        max: UVec3,
    ) -> Result<Option<Arc<ContreeCpuVoxelBlock>>> {
        if let Some(block) = &self.block {
            if min.cmpge(block.voxel_min).all()
                && max.cmple(block.voxel_min + block.dim).all()
                && block
                    .source_dependencies
                    .iter()
                    .all(|d| source.is_chunk_voxel_cache_ready(*d))
            {
                return Ok(Some(Arc::clone(block)));
            }
        }
        self.block = None;
        // Bounds arrive clamped to the 256-voxel chunk grid. A 16-voxel envelope
        // amortizes tip exploration without caching an entire world.
        let min = min / 16 * 16;
        let max = (max + UVec3::splat(15)) / 16 * 16;
        let ContreeCpuVoxelBlockExport::Ready(block) = source.export_voxel_block(min, max - min)?
        else {
            return Ok(None);
        };
        let block = Arc::new(block);
        self.block = Some(Arc::clone(&block));
        Ok(Some(block))
    }
}

struct Patch {
    block: Arc<ContreeCpuVoxelBlock>,
    fresh: bool,
    queries: Option<Cell<u64>>,
}
impl Terrain for Patch {
    fn voxel(&self, cell: IVec3) -> Option<u8> {
        if let Some(queries) = &self.queries {
            queries.set(queries.get() + 1);
        }
        if cell.cmplt(IVec3::ZERO).any() {
            return None;
        }
        let cell = cell.as_uvec3();
        if cell.cmplt(self.block.voxel_min).any() {
            return None;
        }
        let c = cell - self.block.voxel_min;
        let d = self.block.dim;
        if c.cmpge(d).any() {
            return None;
        }
        Some(self.block.voxel_types[((c.z * d.y + c.y) * d.x + c.x) as usize])
    }
    fn current(&self) -> bool {
        self.fresh
    }
}

/// Resolve the entrance face of the clicked solid voxel, including side faces of posts.
/// The root sits just outside the face; no fixture terrain is authored for player planting.
fn root_at_surface(
    terrain: &impl Terrain,
    hit: Vec3,
    ray_direction: Vec3,
) -> Option<(Vec3, Vec3, IVec3, u8)> {
    let direction = ray_direction.normalize_or_zero();
    if direction == Vec3::ZERO || !hit.is_finite() {
        return None;
    }
    let point = hit * 256.0;
    let cell = (point + direction * 0.1).floor().as_ivec3();
    let material = terrain.voxel(cell)?;
    if material == VOXEL_TYPE_EMPTY as u8 {
        return None;
    }
    let axis = (0..3)
        .filter(|&axis| direction[axis].abs() > 1e-5)
        .min_by(|&a, &b| {
            let entrance = |axis: usize| {
                let face = cell[axis] as f32 + f32::from(direction[axis] < 0.0);
                (point[axis] - face).abs() / direction[axis].abs()
            };
            entrance(a).total_cmp(&entrance(b))
        })?;
    let mut normal = Vec3::ZERO;
    normal[axis] = -direction[axis].signum();
    let face = cell[axis] as f32 + f32::from(normal[axis] > 0.0);
    let mut position = point;
    position[axis] = face;
    position += normal * 0.83;
    (crate::climbing_plants::clear_segment(terrain, position, position, 0.65) == Some(true))
        .then_some((position, normal, cell, material))
}

fn stamp_boxes(boxes: &[(UVec3, UVec3)], material: u32) -> Result<VoxelEdit> {
    let cuboids: Vec<_> = boxes
        .iter()
        .map(|(min, max)| Cuboid::from_min_max(min.as_vec3(), max.as_vec3()))
        .collect();
    let bounds: Vec<_> = cuboids.iter().map(Cuboid::aabb).collect();
    let ids: Vec<_> = (0..cuboids.len() as u32).collect();
    Ok(VoxelEdit::StampCuboids {
        bvh_nodes: build_bvh(&bounds, &ids).map_err(anyhow::Error::msg)?,
        cuboids,
        voxel_type: material,
        atlas_state_write: Default::default(),
    })
}
fn wall_edit(min: UVec3, max: UVec3, material: u32) -> Result<WorldEditTransaction> {
    Ok(WorldEditTransaction::terrain_change(
        vec![stamp_boxes(&[(min, max)], material)?],
        UAabb3::new(min, max),
    ))
}
fn fixture_edit(fixture: Fixture, site: Site) -> Result<WorldEditTransaction> {
    let (min, max) = site.bounds();
    let boxes: Vec<_> = fixture
        .boxes()
        .into_iter()
        .map(|b| site.voxel_box(b))
        .collect();
    let mut edits = vec![
        stamp_boxes(&[(min, max)], VOXEL_TYPE_EMPTY)?,
        stamp_boxes(&boxes, VOXEL_TYPE_LIMESTONE)?,
    ];
    if let Some(hole) = fixture.hole() {
        edits.push(stamp_boxes(&[site.voxel_box(hole)], VOXEL_TYPE_EMPTY)?);
    }
    Ok(WorldEditTransaction::terrain_change(
        edits,
        UAabb3::new(min, max),
    ))
}

impl App {
    /// Grow-tool placement replaces the single session vine, without changing the clicked terrain.
    pub(super) fn plant_climbing_at_surface(&mut self, hit: Vec3, direction: Vec3) -> Result<bool> {
        let world_dim = super::CHUNK_DIM * super::VOXEL_DIM_PER_CHUNK;
        let point = hit * 256.0;
        if !point.is_finite()
            || point.cmplt(Vec3::splat(4.0)).any()
            || point.cmpge((world_dim - UVec3::splat(4)).as_vec3()).any()
        {
            return Ok(false);
        }
        let min = (point - Vec3::splat(4.0)).floor().as_uvec3();
        let max = (point + Vec3::splat(5.0)).ceil().as_uvec3();
        let source = self.contree_builder.cpu_voxel_source_snapshot();
        let Some(block) = self
            .climbing_plants
            .collision_cache
            .query(&source, min, max)?
        else {
            self.climbing_plants.waiting_for_terrain = true;
            return Ok(false);
        };
        let latest = self.contree_builder.cpu_voxel_source_snapshot();
        if !block
            .source_dependencies
            .iter()
            .all(|d| latest.is_chunk_voxel_cache_ready(*d))
        {
            self.climbing_plants.waiting_for_terrain = true;
            return Ok(false);
        }
        let patch = Patch {
            block,
            fresh: true,
            queries: None,
        };
        let Some((position, normal, cell, material)) = root_at_surface(&patch, hit, direction)
        else {
            self.climbing_plants.last_action =
                "Choose a clear, solid terrain face for the vine root.";
            return Ok(false);
        };
        self.climbing_plants.vine = Some(VineTick::new(
            Plant::seed(position, normal, cell, material, PLAYABLE_VINE_SEED)
                .with_search_direction(SearchDirection::Counterclockwise),
        ));
        self.climbing_plants.created = true;
        self.climbing_plants.awaiting_seed = false;
        self.climbing_plants.reset_requested = false;
        self.climbing_plants.site = None;
        self.climbing_plants.seed = PLAYABLE_VINE_SEED;
        self.climbing_plants.direction = SearchDirection::Counterclockwise;
        self.climbing_plants.last_selection = Some((
            Fixture::from_index(self.debug_settings.adjustables.climbing_fixture.value),
            PLAYABLE_VINE_SEED,
        ));
        self.climbing_plants.waiting_for_terrain = false;
        self.climbing_plants.last_action =
            "Planted vine at the selected surface (one session vine at a time).";
        log::info!("[CLIMBING] Grow planted vine at {position:?} on {cell:?} material={material}");
        Ok(true)
    }

    pub(super) fn update_climbing_plants(&mut self, steps: u32, tick_seconds: f32) -> Result<()> {
        // Explicit test scenes own their terrain and camera. The automatic interactive
        // demo must not author another fixture or steal their capture viewpoint.
        if self
            .launch_owners
            .test_scene_frame_plan()
            .owns_capture_scene()
        {
            return Ok(());
        }
        let review_mode = std::env::var("RE_FLORA_CLIMBING_REVIEW").ok();
        let review = review_mode.is_some();
        let overhang_review = review_mode.as_deref() == Some("overhang");
        let review_fixture = if overhang_review {
            Some(Fixture::Inward)
        } else {
            review_mode.as_deref().and_then(Fixture::parse)
        };
        let selected_fixture = if review {
            review_fixture.unwrap_or_default()
        } else {
            Fixture::from_index(self.debug_settings.adjustables.climbing_fixture.value)
        };
        let selected_seed = if review && !overhang_review {
            42
        } else {
            PLAYABLE_VINE_SEED
        };
        // The player-facing phenotype always searches counterclockwise. Native
        // fixture review still exercises the opposite code-configured direction.
        let selected_direction = if review && !overhang_review {
            SearchDirection::Clockwise
        } else {
            SearchDirection::Counterclockwise
        };
        self.climbing_plants
            .observe_selection(selected_fixture, selected_seed);
        if !self.terrain_persistence.allows_world_updates() {
            return Ok(());
        }
        if self.climbing_plants.reset_requested || !self.climbing_plants.created {
            let fixture = selected_fixture;
            let site = if let Some(site) = self.climbing_plants.site {
                site
            } else {
                let source = self.contree_builder.cpu_voxel_source_snapshot();
                let world_dim = super::CHUNK_DIM * super::VOXEL_DIM_PER_CHUNK;
                let ContreeCpuVoxelBlockExport::Ready(block) =
                    source.export_voxel_block(Site::COLUMN, UVec3::new(1, world_dim.y, 1))?
                else {
                    self.climbing_plants.waiting_for_terrain = true;
                    return Ok(());
                };
                let latest = self.contree_builder.cpu_voxel_source_snapshot();
                let fresh = block
                    .source_dependencies
                    .iter()
                    .all(|d| latest.is_chunk_voxel_cache_ready(*d));
                let patch = Patch {
                    block: Arc::new(block),
                    fresh,
                    queries: None,
                };
                let Some(site) = Site::find(&patch, world_dim) else {
                    self.climbing_plants.last_action =
                        "Waiting for current soil/sand/rock with enough headroom at the test site.";
                    return Ok(());
                };
                site
            };
            self.climbing_plants = ClimbingPlants {
                created: true,
                awaiting_seed: true,
                fixture,
                last_selection: Some((fixture, selected_seed)),
                site: Some(site),
                seed: selected_seed,
                direction: selected_direction,
                focus_requested: true,
                review: review::Review::for_fixture(review_fixture, site)
                    .with_overhang(overhang_review),
                ..Default::default()
            };
            self.tracer.show_climbing_plant_geometry(&[])?;
            self.execute_world_edit(fixture_edit(fixture, site)?)?;
            log::info!(
                "[CLIMBING] authored editable {} fixture; history is session-only",
                fixture.name()
            );
        }
        if !self.climbing_plants.awaiting_seed && self.climbing_plants.vine.is_none() {
            return Ok(()); // loading a world must not silently seed a new vine
        }
        let site = self.climbing_plants.site;
        if self.climbing_plants.focus_requested {
            self.climbing_plants.focus_requested = false;
            let target = self.climbing_plants.plant().map_or_else(
                || {
                    site.expect("fixture seed has a grounded site")
                        .point(Vec3::new(
                            256.,
                            244.,
                            if self.climbing_plants.fixture == Fixture::Slope {
                                330.
                            } else {
                                308.
                            },
                        ))
                        / 256.
                },
                |plant| {
                    let (min, max) = plant.nodes.iter().fold(
                        (plant.nodes[0].position, plant.nodes[0].position),
                        |(min, max), node| (min.min(node.position), max.max(node.position)),
                    );
                    (min + max) * 0.5 / 256.
                },
            );
            self.camera_control.apply_snapshot_mode(true);
            self.camera_control.set_orbit_focus(target);
            self.tracer.set_camera_pose_looking_at(
                target
                    + if overhang_review {
                        Vec3::new(0.5, 0.15, 0.65)
                    } else {
                        Vec3::new(0., 0.02, 0.65)
                    },
                target,
            );
            self.reset_camera_movement_input();
        }
        if std::mem::take(&mut self.climbing_plants.refill_tip_requested) {
            if let Some(plant) = self.climbing_plants.plant() {
                let tip = plant.nodes.last().unwrap().position;
                let cell = tip.floor().as_uvec3();
                let min = UVec3::new(cell.x.saturating_sub(1), cell.y.saturating_sub(2), cell.z);
                let max = UVec3::new(cell.x + 2, cell.y + 3, (tip.z + plant.radius).ceil() as u32)
                    .min(super::CHUNK_DIM * super::VOXEL_DIM_PER_CHUNK);
                anyhow::ensure!(
                    max.cmpgt(min).all(),
                    "vine tip is outside editable refill bounds"
                );
                self.climbing_plants.last_action =
                    "Inserted limestone through a tip; the blocked stem will be pruned.";
                self.execute_world_edit(wall_edit(min, max, VOXEL_TYPE_LIMESTONE)?)?;
                log::info!("[CLIMBING] real refill through tip bounds={min:?}..{max:?}");
            }
        }
        // Opt-in CPU scopes cover the vine update, excluding fixture edits and camera actions.
        let profile_start = self.perf_logging.then(Instant::now);
        let elapsed_us = || profile_start.map_or(0, |start| start.elapsed().as_micros());
        let source = self.contree_builder.cpu_voxel_source_snapshot();

        let (min, max) = if let Some(plant) = self.climbing_plants.plant() {
            let mut min = plant.nodes[0].position;
            let mut max = min;
            for node in &plant.nodes {
                min = min.min(node.position);
                max = max.max(node.position);
            }
            for anchor in &plant.anchors {
                min = min.min(anchor.cell.as_vec3());
                max = max.max(anchor.cell.as_vec3());
            }
            (
                (min - Vec3::splat(8.)).max(Vec3::ZERO).floor().as_uvec3(),
                (max + Vec3::splat(9.))
                    .ceil()
                    .as_uvec3()
                    .min(super::CHUNK_DIM * super::VOXEL_DIM_PER_CHUNK),
            )
        } else {
            let (position, _, _) = site
                .expect("fixture seed has a grounded site")
                .seed(self.climbing_plants.fixture);
            (
                (position - Vec3::splat(8.0)).floor().as_uvec3(),
                (position + Vec3::splat(9.0)).ceil().as_uvec3(),
            )
        };
        let patch = self
            .climbing_plants
            .collision_cache
            .query(&source, min, max)?
            .map(|block| {
                // Publication readiness is distinct from revision equality. Pass
                // both facts to the tick owner, including unavailable/stale exports.
                let latest = self.contree_builder.cpu_voxel_source_snapshot();
                let fresh = block
                    .source_dependencies
                    .iter()
                    .all(|d| latest.is_chunk_voxel_cache_ready(*d));
                Patch {
                    block,
                    fresh,
                    queries: self.perf_logging.then(|| Cell::new(0)),
                }
            });
        let export_us = elapsed_us();
        if self.climbing_plants.awaiting_seed {
            self.climbing_plants.waiting_for_terrain = true;
            let Some(patch) = patch.as_ref().filter(|patch| patch.current()) else {
                return Ok(());
            };
            self.climbing_plants.waiting_for_terrain = false;
            let (position, normal, cell) = site
                .expect("fixture seed has a grounded site")
                .seed(self.climbing_plants.fixture);
            if patch.voxel(cell) == Some(VOXEL_TYPE_LIMESTONE as u8)
                && crate::climbing_plants::clear_segment(patch, position, position, 0.65)
                    == Some(true)
            {
                self.climbing_plants.vine = Some(VineTick::new(
                    Plant::seed(
                        position,
                        normal,
                        cell,
                        VOXEL_TYPE_LIMESTONE as u8,
                        self.climbing_plants.seed,
                    )
                    .with_search_direction(self.climbing_plants.direction),
                ));
                self.climbing_plants.awaiting_seed = false;
                log::info!(
                    "[CLIMBING] seed={} direction={:?} position={position:?} fixture={} dependencies={}",
                    self.climbing_plants.seed,
                    self.climbing_plants.direction,
                    self.climbing_plants.fixture.name(),
                    patch.block.source_dependencies.len()
                );
            }
        }
        let Some(vine) = &mut self.climbing_plants.vine else {
            return Ok(());
        };
        let spacing = if overhang_review {
            10.0
        } else if review {
            16.0
        } else {
            self.debug_settings.adjustables.climbing_spacing.value
        };
        let flexibility = if overhang_review {
            2.0
        } else if review {
            1.0
        } else {
            self.debug_settings.adjustables.climbing_flexibility.value
        };
        let cadence = if review {
            Cadence::Review {
                growing: self.climbing_plants.review.growing(),
            }
        } else {
            Cadence::Play {
                dt: steps as f32 * tick_seconds,
                growth_per_second: self.debug_settings.adjustables.climbing_speed.value,
            }
        };
        let report = vine.advance(
            patch.as_ref().map(|patch| Snapshot {
                terrain: patch,
                dependencies: &patch.block.source_dependencies,
            }),
            cadence,
            Tuning {
                spacing,
                flexibility,
                search: (!review).then_some(Search {
                    turn: self.debug_settings.adjustables.climbing_search_turn.value,
                    reach: self.debug_settings.adjustables.climbing_search_reach.value,
                    rate: self.debug_settings.adjustables.climbing_search_rate.value,
                }),
            },
            self.perf_logging,
        );
        let pose_end_us = elapsed_us();
        self.climbing_plants.observe_tick(&report);
        if !report.publish {
            return Ok(());
        }
        let patch = patch
            .as_ref()
            .expect("a published tick has a terrain export");
        let plant = self.climbing_plants.vine.as_ref().unwrap().plant();
        if report.nodes_before_motion / 16 != plant.nodes.len() / 16 {
            log::info!(
                "[CLIMBING] growth nodes={} attached={} tips={} finite={}",
                plant.nodes.len(),
                plant.anchors.iter().filter(|a| a.attached).count(),
                plant.tips.len(),
                plant.nodes.iter().all(|n| n.position.is_finite())
            );
        }
        let mut instances = std::mem::take(&mut self.climbing_plants.instances);
        instances.clear();
        instances.reserve(plant.nodes.len() * 2);
        for node in &plant.nodes {
            // Stable IDs keep unaffected leaves in place when live indices compact.
            let id = node.id;
            if let Some(parent) = node.parent {
                let start = plant.nodes[parent].position;
                let delta = node.position - start;
                let up = delta.normalize_or_zero();
                let side = up
                    .cross(node.normal)
                    .try_normalize()
                    .unwrap_or_else(|| up.any_orthonormal_vector());
                let rotation = Quat::from_mat3(&Mat3::from_cols(side, up, side.cross(up)));
                instances.push(block_instance(
                    (start + node.position) * 0.5,
                    rotation,
                    Vec3::new(0.9, delta.length(), 0.9),
                    if node.fixed {
                        Vec3::new(0.22, 0.32, 0.07)
                    } else {
                        Vec3::new(0.32, 0.52, 0.09)
                    },
                ));
                if id % 3 == 0 {
                    let side = if id % 2 == 0 { -1. } else { 1. };
                    instances.push(block_instance(
                        node.position + rotation * Vec3::new(side * 1.8, 0., 0.3),
                        rotation * Quat::from_rotation_z(side * 0.6),
                        Vec3::new(3.8, 2.5, 0.55),
                        Vec3::new(0.12 + (id % 5) as f32 * 0.015, 0.38, 0.09),
                    ));
                }
            }
        }
        {
            // Short attachment organs, not extra main shoots. Their footprint is
            // the established support cell; the compliant stem may sit off it.
            for a in plant.anchors.iter().skip(1) {
                let surface = a.surface_position();
                let tangent = a.normal.any_orthonormal_vector();
                for offset in [-0.25, 0.25] {
                    let start = plant.nodes[a.node].position + tangent * offset;
                    let end = surface + tangent * (offset * 0.5);
                    let delta = end - start;
                    if let Some(direction) = delta.try_normalize() {
                        instances.push(block_instance(
                            (start + end) * 0.5,
                            Quat::from_rotation_arc(Vec3::Y, direction),
                            Vec3::new(0.18, delta.length(), 0.18),
                            Vec3::new(0.3, 0.24, 0.1),
                        ));
                    }
                }
            }
        }
        if self.debug_settings.adjustables.climbing_show_anchors.value {
            for a in &plant.anchors {
                instances.push(block_instance(
                    plant.nodes[a.node].position + a.normal,
                    Quat::IDENTITY,
                    Vec3::splat(1.6),
                    if a.attached {
                        Vec3::new(1., 0.8, 0.1)
                    } else {
                        Vec3::new(1., 0.1, 0.1)
                    },
                ));
            }
        }
        if self.debug_settings.adjustables.climbing_show_anchors.value {
            for node in plant.regrowth_nodes() {
                instances.push(block_instance(
                    node.position + node.normal * 2.0,
                    Quat::IDENTITY,
                    Vec3::splat(1.8),
                    Vec3::new(1.0, 0.4, 0.05),
                ));
            }
        }
        if self.debug_settings.adjustables.climbing_show_anchors.value {
            for position in plant.search_probes() {
                instances.push(block_instance(
                    position,
                    Quat::IDENTITY,
                    Vec3::splat(1.0),
                    Vec3::new(0.1, 0.65, 1.0),
                ));
            }
        }
        let review_edit = if review {
            self.climbing_plants.review.advance(plant, patch)?
        } else {
            None
        };
        self.tracer.show_climbing_plant_geometry(&instances)?;
        self.climbing_plants.instances = instances;
        if self.perf_logging {
            let total_us = elapsed_us();
            let phase = if review {
                self.climbing_plants.review.phase()
            } else {
                "play"
            };
            let plant = self.climbing_plants.plant().unwrap();
            log::info!(
                "[CLIMBING][PERF] phase={phase} tick={} nodes={} attached={} quanta={} motion_steps={} queries={} export_us={export_us} prune_us={} growth_us={} pose_us={} render_us={} total_us={total_us}",
                self.climbing_plants.review.ticks,
                plant.nodes.len(),
                plant.anchors.iter().filter(|a| a.attached).count(),
                report.growth_attempts,
                report.motion_steps,
                patch.queries.as_ref().map_or(0, Cell::get),
                report.timings.revalidate_us,
                report.timings.growth_us,
                report.timings.pose_us,
                total_us - pose_end_us,
            );
        }
        if let Some(transaction) = review_edit {
            self.execute_world_edit(transaction)?;
        }
        Ok(())
    }
}
fn block_instance(
    position: Vec3,
    rotation: Quat,
    dimensions: Vec3,
    color: Vec3,
) -> DynamicFruitRenderInstance {
    let mut instance = DynamicFruitRenderInstance::new(position / 256., rotation, 1.);
    instance.dimensions = dimensions / 256.;
    instance.color = color;
    instance
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::test_cpu_voxel_source_snapshot;

    struct Wall;
    impl Terrain for Wall {
        fn voxel(&self, cell: IVec3) -> Option<u8> {
            Some(u8::from(cell.z == 0))
        }
        fn current(&self) -> bool {
            true
        }
    }
    #[test]
    fn grow_root_uses_clicked_ground_and_pole_faces_without_modifying_terrain() {
        struct Scene(Fixture);
        impl Terrain for Scene {
            fn voxel(&self, cell: IVec3) -> Option<u8> {
                Some(if self.0.solid(cell) {
                    VOXEL_TYPE_LIMESTONE as u8
                } else {
                    0
                })
            }
            fn current(&self) -> bool {
                true
            }
        }
        for (fixture, hit, direction, expected_normal, expected_cell) in [
            (
                Fixture::Ground,
                Vec3::new(255.5, 192.0, 314.5),
                -Vec3::Y,
                Vec3::Y,
                IVec3::new(255, 191, 314),
            ),
            (
                Fixture::Pole,
                Vec3::new(255.5, 210.5, 316.0),
                -Vec3::Z,
                Vec3::Z,
                IVec3::new(255, 210, 315),
            ),
            (
                Fixture::Pole,
                Vec3::new(266.0, 210.5, 306.5),
                -Vec3::X,
                Vec3::X,
                IVec3::new(265, 210, 306),
            ),
        ] {
            let terrain = Scene(fixture);
            let (position, normal, cell, material) =
                root_at_surface(&terrain, hit / 256.0, direction).unwrap();
            assert_eq!(
                (normal, cell, material),
                (expected_normal, expected_cell, VOXEL_TYPE_LIMESTONE as u8)
            );
            let mut plant = Plant::seed(position, normal, cell, material, PLAYABLE_VINE_SEED);
            assert_eq!(plant.revalidate(&terrain).unwrap().removed, 0);
            assert!(plant.root_connected());
        }
        // A hit point that does not resolve to a solid support must not replace the vine.
        assert!(root_at_surface(
            &Scene(Fixture::Pole),
            Vec3::new(280.0, 210.5, 316.0) / 256.0,
            -Vec3::Z,
        )
        .is_none());
    }

    fn test_plant() -> Plant {
        Plant::seed(
            Vec3::new(20.5, 4.5, 1.8),
            Vec3::Z,
            IVec3::new(20, 4, 0),
            1,
            42,
        )
    }
    #[test]
    fn action_buttons_restart_immediately_and_gate_unavailable_simulation() {
        fn text_position(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + egui::vec2(4.0, 5.0))
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| text_position(s, label)),
                _ => None,
            }
        }
        fn click(runtime: &mut ClimbingPlants, context: &egui::Context, label: &str) {
            let mut draw = |events| {
                context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 1200.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| runtime.draw_actions(ui),
                )
            };
            draw(Vec::new());
            let output = draw(Vec::new());
            let pos = output
                .shapes
                .iter()
                .find_map(|s| text_position(&s.shape, label))
                .expect(label);
            for pressed in [true, false] {
                draw(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ]);
            }
        }
        fn advance(runtime: &mut ClimbingPlants, cadence: Cadence) -> tick::Report {
            let report = runtime.vine.as_mut().unwrap().advance(
                Some(Snapshot {
                    terrain: &Wall,
                    dependencies: &[],
                }),
                cadence,
                Tuning {
                    spacing: 16.0,
                    flexibility: 1.0,
                    search: None,
                },
                false,
            );
            runtime.observe_tick(&report);
            report
        }
        let mut runtime = ClimbingPlants {
            created: true,
            vine: Some(VineTick::new(test_plant())),
            site: Some(Site::default()),
            ..Default::default()
        };
        for _ in 0..40 {
            advance(&mut runtime, Cadence::Review { growing: true });
        }
        let hold = Cadence::Play {
            dt: 0.0,
            growth_per_second: 10.0,
        };
        let context = egui::Context::default();
        click(&mut runtime, &context, "Restart wall and vine");
        assert!(
            runtime.reset_requested,
            "restart still requires a confirmation click"
        );
        runtime.reset_requested = false;
        runtime.waiting_for_terrain = true;
        click(&mut runtime, &context, "Prune highest attachment");
        assert!(advance(&mut runtime, hold).events.is_empty());
        click(&mut runtime, &context, "Prune highest attachment");
        assert!(matches!(
            advance(&mut runtime, hold).events.as_slice(),
            [Event::PrunedHighest(_)]
        ));
        click(&mut runtime, &context, "Prune back to root");
        assert!(matches!(
            advance(&mut runtime, hold).events.as_slice(),
            [Event::PrunedToRoot(_)]
        ));
        assert_eq!(runtime.plant().unwrap().nodes.len(), 1);
        click(&mut runtime, &context, "Disconnect root");
        assert_eq!(
            advance(&mut runtime, hold).events,
            vec![Event::DisconnectedRoot]
        );
        assert!(!runtime.plant().unwrap().root_connected());
        click(&mut runtime, &context, "Restart wall and vine");
        assert!(runtime.reset_requested);
    }

    #[test]
    fn terrain_choice_immediately_requests_one_rebuild_without_reset_or_confirm() {
        let mut runtime = ClimbingPlants::default();
        runtime.observe_selection(Fixture::Flat, 42);
        assert!(!runtime.reset_requested);
        runtime.observe_selection(Fixture::Hole, 42);
        assert!(
            runtime.reset_requested,
            "changing Test terrain did not apply it"
        );
        runtime.reset_requested = false;
        runtime.observe_selection(Fixture::Hole, 42);
        assert!(
            !runtime.reset_requested,
            "unchanged settings rebuilt the scene again"
        );
        runtime.observe_selection(Fixture::Hole, 43);
        assert!(runtime.reset_requested, "changing the seed did not restart");
    }

    #[test]
    fn collision_cache_reuses_only_covered_current_ready_exports() {
        let a = UVec3::ZERO;
        let b = UVec3::X;
        let grid = UVec3::new(2, 1, 1);
        let dim = UVec3::splat(256);
        let min = UVec3::new(254, 2, 2);
        let max = UVec3::new(258, 8, 8);
        let ready = test_cpu_voxel_source_snapshot(grid, dim, &[], &[], &[(a, 1), (b, 1)], &[]);
        let mut cache = CollisionPatchCache::default();
        let first = cache.query(&ready, min, max).unwrap().unwrap();
        let second = cache.query(&ready, min + UVec3::ONE, max).unwrap().unwrap();
        assert!(
            Arc::ptr_eq(&first, &second),
            "unchanged terrain was re-exported"
        );
        // Both sides of a chunk seam are dependencies, including known-empty chunks.
        let changed = test_cpu_voxel_source_snapshot(grid, dim, &[], &[], &[(a, 1), (b, 2)], &[]);
        let third = cache.query(&changed, min, max).unwrap().unwrap();
        assert!(!Arc::ptr_eq(&first, &third), "cross-chunk revision ignored");
        // Identical revision, but a newly present chunk is not yet decoded.
        let pending = test_cpu_voxel_source_snapshot(grid, dim, &[b], &[], &[(a, 1), (b, 2)], &[b]);
        // Model a previously ready export with exactly the pending source's identities.
        // Readiness must be checked even when presence AND revision still match.
        let mut previously_ready = (*third).clone();
        for dependency in &mut previously_ready.source_dependencies {
            *dependency = pending
                .chunk_source_dependency(dependency.chunk_idx)
                .unwrap();
        }
        cache.block = Some(Arc::new(previously_ready));
        assert!(cache.query(&pending, min, max).unwrap().is_none());
        assert!(
            cache.block.is_none(),
            "pending terrain retained a usable old export"
        );
        let restored = cache.query(&changed, min, max).unwrap().unwrap();
        assert!(!Arc::ptr_eq(&third, &restored));
        let expanded = cache
            .query(&changed, min, max + UVec3::splat(32))
            .unwrap()
            .unwrap();
        assert!(
            !Arc::ptr_eq(&restored, &expanded),
            "out-of-bounds export reused"
        );
        let local = cache
            .query(&ready, UVec3::splat(2), UVec3::splat(8))
            .unwrap()
            .unwrap();
        let unrelated = cache
            .query(&changed, UVec3::splat(2), UVec3::splat(8))
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(&local, &unrelated));
    }

    #[test]
    fn world_replacement_drops_the_vine_without_silently_reseeding() {
        let mut runtime = ClimbingPlants {
            vine: Some(VineTick::new(test_plant())),
            created: true,
            ..Default::default()
        };
        assert!(runtime.has_history());
        runtime.replace_world();
        assert!(!runtime.has_history());
        assert!(runtime.plant().is_none());
        assert!(runtime.created); // load must not silently respawn the authored wall
        assert!(!runtime.awaiting_seed); // nor silently grow a new vine in existing terrain
        assert!(runtime.collision_cache.block.is_none());
    }

    #[test]
    fn cross_chunk_export_dependencies_reject_changed_and_pending_sources() {
        let a = UVec3::ZERO;
        let b = UVec3::X;
        let grid = UVec3::new(2, 1, 1);
        let dim = UVec3::splat(256);
        let source = test_cpu_voxel_source_snapshot(grid, dim, &[], &[], &[(a, 1), (b, 1)], &[]);
        let ContreeCpuVoxelBlockExport::Ready(block) = source
            .export_voxel_block(UVec3::new(255, 1, 1), UVec3::new(2, 2, 2))
            .unwrap()
        else {
            panic!("absent chunks are authoritative empty");
        };
        assert_eq!(block.source_dependencies.len(), 2);
        assert!(block
            .source_dependencies
            .iter()
            .all(|d| source.is_chunk_voxel_cache_ready(*d)));
        let changed = test_cpu_voxel_source_snapshot(grid, dim, &[], &[], &[(a, 1), (b, 2)], &[]);
        assert!(!block
            .source_dependencies
            .iter()
            .all(|d| changed.is_chunk_voxel_cache_ready(*d)));
        let pending = test_cpu_voxel_source_snapshot(grid, dim, &[b], &[], &[(a, 1), (b, 1)], &[b]);
        assert!(!block
            .source_dependencies
            .iter()
            .all(|d| pending.is_chunk_voxel_cache_ready(*d)));
        assert!(matches!(
            pending
                .export_voxel_block(UVec3::new(255, 1, 1), UVec3::new(2, 2, 2))
                .unwrap(),
            ContreeCpuVoxelBlockExport::NotReady(_)
        ));
        let patch = Patch {
            block: Arc::new(block),
            fresh: true,
            queries: None,
        };
        assert_eq!(patch.voxel(IVec3::new(255, 1, 1)), Some(0));
        assert_eq!(patch.voxel(IVec3::new(256, 1, 1)), Some(0));
        assert_eq!(patch.voxel(IVec3::new(257, 1, 1)), None);
    }
}
