//! Input/terrain adapter for the distance-sampled pavement module.
use super::{player_tools::PlayerTool, App, BuildEdit, VisibleTerrainChange};
use crate::{
    app::{
        low_walls::{LowWallKind, LowWallRecipe},
        path_band::{PathCell, PathIntent, PathSample, PathSampling, PathStroke},
        pavement::{PavementCoverage, PavementKind, PavementRecipe, VOXEL_SIZE},
        world_edits::TerrainBrushEdit,
    },
    builder::{
        ContreeCpuRayQuerySnapshot, ModelTriangleGpu, VOXEL_TYPE_DIRT, VOXEL_TYPE_ROCK,
        VOXEL_TYPE_SAND, VOXEL_TYPE_TERRACOTTA,
    },
    geom::UAabb3,
    stone_models::{generate, StoneKind, StoneSpec},
};
use anyhow::{ensure, Context, Result};
use glam::{Mat3, Quat, UVec3, Vec3};
use std::{collections::HashSet, time::Instant};

mod continuity;
mod loop_review;
#[cfg(test)]
mod loop_tests;
use continuity::{BlockReason, CourseContinuity};
mod candidate;
use candidate::{Candidate, CoursePreparation, StagedCoverage};
mod low_walls;
mod perf;
mod pointer_review;
mod recipe;
use recipe::ConstructionRecipe;
mod prototype_review;
mod retraction;
mod review;
use retraction::Backdrag;
mod edit_history;
use edit_history::EditHistory;

const ROWS_PER_FRAME: usize = 4;
const MIN_GROUND_NORMAL_Y: f32 = 0.65;

#[derive(Default)]
pub(super) struct PavementRuntime {
    active: Option<ActivePavement>,
    pointer_review: Option<pointer_review::PointerReview>,
}
struct ActivePavement {
    recipe: ConstructionRecipe,
    stroke: PathStroke,
    coverage: PavementCoverage,
    previous_surface: Option<Vec3>,
    continuity: CourseContinuity,
    intent: PathIntent,
    candidate: Option<Candidate>,
    // Never ray-cast onto voxels that this same gesture just authored.
    ground: ContreeCpuRayQuerySnapshot,
    pieces_placed: usize,
    surfaces_cleared: usize,
    backdrag: Backdrag,
    // Owns exact voxel/flora receipts and protection facts for this gesture only.
    edits: EditHistory,
    started: Instant,
}
#[derive(Clone, Copy)]
pub(super) struct PendingPathCell {
    pub cell: PathCell,
    pub valid: bool,
}

pub(super) struct PathIntentPreview {
    points: Vec<Vec3>,
    floor: usize,
    anchor: Vec3,
    invalid: bool,
    loop_target: Option<Vec3>,
}

impl ActivePavement {
    fn loop_target(&self) -> Option<PathSample> {
        if self.recipe.is_dirt() || self.retracting() {
            return None;
        }
        let first = self.continuity.first()?;
        let last = self.continuity.accepted()?;
        let settings = self.recipe.path_settings();
        let capture = (settings.width * 0.75).max(settings.pitch);
        (last.end_distance - first.distance > 6. * settings.width
            && self.intent.cursor().distance(self.intent.origin()) <= capture)
            .then_some(first)
    }

    fn block_here(&mut self, reason: BlockReason) {
        let row = self
            .stroke
            .pending_rows()
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                PathSample::straight(
                    self.intent.anchor(),
                    Vec3::X,
                    0.,
                    self.continuity.expected(),
                    self.stroke.config.pitch,
                )
            });
        // Candidate feedback must not discard approved compiler work.
        self.continuity.block(row, reason, &mut Default::default());
    }

    fn observe_ground(&mut self, point: Vec3) {
        if !point.is_finite() {
            return;
        }
        let changed = self.intent.observe(point);
        if !changed && (self.candidate.is_some() || self.continuity.blocked().is_none()) {
            return;
        }
        self.propose_current();
    }

    fn propose_current(&mut self) {
        let mut stroke = self.stroke.clone();
        stroke.replan_unpublished(self.continuity.accepted());
        let mut endpoint = self.intent.cursor();
        let join = self.loop_target().and_then(|first| {
            self.stroke
                .close_loop(self.continuity.accepted()?, first, |row| {
                    let pieces = self.recipe.pieces(row);
                    !pieces.is_empty()
                        && match self.recipe {
                            ConstructionRecipe::LowWall(wall) => {
                                self.recipe.accepts(&self.coverage, wall.raw_cell(row))
                            }
                            _ => pieces
                                .into_iter()
                                .all(|cell| self.recipe.accepts(&self.coverage, cell)),
                        }
                })
        });
        let pushed = if let Some(join) = join {
            endpoint = join.rows.back().unwrap().exit.center;
            stroke = join;
            true
        } else {
            stroke.push(endpoint)
        };
        let invalid = stroke.pending_rows().into_iter().find(|r| !r.valid);
        if !pushed || !self.intent.can_fix() {
            self.block_here(BlockReason::Sampling);
        } else if let Some(row) = invalid {
            self.continuity
                .block(row, BlockReason::Turn, &mut Default::default());
        } else {
            self.continuity.resume();
        }
        self.candidate = Some(Candidate {
            stroke,
            endpoint,
            expected: self.continuity.expected(),
            checked: self.continuity.blocked().is_some(),
        });
    }

    fn fix_candidate(&mut self) {
        let candidate = self.candidate.take().expect("approved candidate");
        self.stroke = candidate.stroke;
        self.intent.fix(candidate.endpoint);
    }

    fn discard_unpublished(&mut self) {
        self.intent.discard_unpublished();
        let anchor = self.intent.anchor();
        // Preserve actual published exit/ID, never generated/queued checkpoints.
        self.stroke
            .replace_tail(self.continuity.accepted(), &[anchor]);
        self.candidate = None;
    }
}

impl ConstructionRecipe {
    fn preview_candidate(self, row: PathSample, valid: bool) -> Vec<PendingPathCell> {
        // Never submit folded invalid polygons to the painter. A red straight
        // cell represents the rejected attempt, not an alternative bakeable route.
        let row = if valid {
            row
        } else {
            let forward = Vec3::new(row.forward.x, 0., row.forward.z).normalize_or_zero();
            let forward = if forward.is_finite() && forward != Vec3::ZERO {
                forward
            } else {
                Vec3::X
            };
            PathSample::straight(
                row.center,
                forward,
                row.distance,
                row.index,
                self.path_settings().pitch,
            )
        };
        self.pieces(row)
            .into_iter()
            .map(|cell| PendingPathCell { cell, valid })
            .collect()
    }
}

impl PavementRuntime {
    pub(super) fn pending_preview(&self) -> Vec<PendingPathCell> {
        let Some(a) = &self.active else {
            return Vec::new();
        };
        if a.withdrawn() {
            return Vec::new();
        }
        // Only the CURRENT proposal inherits the approved guide; it is never
        // used as the compiler for the next pointer observation.
        let mut valid = a.continuity.blocked().is_none();
        a.candidate
            .as_ref()
            .map_or(&a.stroke, |c| &c.stroke)
            .pending_rows()
            .into_iter()
            .flat_map(|row| {
                valid &= row.valid;
                a.recipe.preview_candidate(row, valid)
            })
            .collect()
    }

    pub(super) fn intent_preview(&self, preview: &[PendingPathCell]) -> Option<PathIntentPreview> {
        let a = self.active.as_ref()?;
        Some(PathIntentPreview {
            points: a.intent.points(),
            floor: a.intent.floor(),
            anchor: a.intent.anchor(),
            invalid: a.continuity.blocked().is_some() || preview.iter().any(|p| !p.valid),
            loop_target: a
                .candidate
                .as_ref()
                .filter(|c| c.closes_loop())
                .map(|c| c.endpoint),
        })
    }

    pub(super) fn feedback(&self, preview: &[PendingPathCell]) -> Option<&'static str> {
        let a = self.active.as_ref()?;
        a.continuity.blocked().map(|b| b.reason.message()).or_else(|| {
            preview.iter().any(|p| !p.valid).then_some("This target cannot be built. Keep holding and move to a valid target; rejected points are ignored.")
        })
    }

    fn invalid_tail(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|a| a.continuity.blocked().is_some())
    }
    fn cancel(&mut self) -> bool {
        let Some(active) = self.active.take() else {
            return false;
        };
        log::info!("[PAVEMENT] interrupted kind={} placed={} pending_discarded={}; published terrain retained", active.recipe.name(), active.pieces_placed, active.stroke.rows.len());
        true
    }
}

fn is_ground(material: u32) -> bool {
    matches!(material, VOXEL_TYPE_DIRT | VOXEL_TYPE_SAND)
}

/// Local +Z follows the *projected path tangent*, +Y follows the ground normal.
fn surface_frame(normal: Vec3, forward: Vec3) -> Option<Quat> {
    if !normal.is_finite() || !forward.is_finite() {
        return None;
    }
    let up = normal.normalize_or_zero();
    if up.y < MIN_GROUND_NORMAL_Y {
        return None;
    }
    let along = (forward - up * forward.dot(up)).normalize_or_zero();
    if along == Vec3::ZERO {
        return None;
    }
    Some(Quat::from_mat3(&Mat3::from_cols(
        up.cross(along),
        up,
        along,
    )))
}

struct GroundedPiece {
    piece: PathCell,
    triangles: Vec<ModelTriangleGpu>,
    base: Vec3,
    footprint: Vec<glam::Vec2>,
    min: Vec3,
    max: Vec3,
}

// A protected atlas region. Undo data is captured immediately before each stamp,
// because adjacent stones in this same course may share voxelized boundaries.
struct PathStampReadback {
    offset: UVec3,
    dim: UVec3,
}

fn project_path_point(
    position: Vec3,
    matrix: glam::Mat4,
    extent: glam::Vec2,
) -> Option<egui::Pos2> {
    let clip = matrix * position.extend(1.);
    if clip.w <= 0.0001 || !clip.is_finite() {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    // Camera projection already flips Y for Vulkan's top-left viewport.
    Some(egui::pos2(
        (ndc.x * 0.5 + 0.5) * extent.x,
        (ndc.y * 0.5 + 0.5) * extent.y,
    ))
}

pub(super) fn draw_intent_preview(
    ctx: &egui::Context,
    preview: &Option<PathIntentPreview>,
    matrix: glam::Mat4,
    extent: glam::Vec2,
) {
    let Some(intent) = preview else {
        return;
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        "construction_intent".into(),
    ));
    let project = |p| project_path_point(p + Vec3::Y * (2. * VOXEL_SIZE), matrix, extent);
    let gold = egui::Color32::from_rgba_unmultiplied(242, 223, 155, 190);
    let red = egui::Color32::from_rgba_unmultiplied(255, 93, 78, 225);
    for (i, points) in intent.points.windows(2).enumerate() {
        if let (Some(a), Some(b)) = (project(points[0]), project(points[1])) {
            painter.line_segment(
                [a, b],
                egui::Stroke::new(
                    1.5,
                    if intent.invalid && i >= intent.floor {
                        red
                    } else {
                        gold
                    },
                ),
            );
        }
    }
    // Only approved endpoints and the latest cursor proposal are displayed.
    // These guides are transient feedback, never editable world objects.
    for (i, point) in intent.points.iter().enumerate().step_by(8) {
        if let Some(p) = project(*point) {
            painter.circle_filled(
                p,
                1.7,
                if intent.invalid && i > intent.floor {
                    red
                } else {
                    gold
                },
            );
        }
    }
    if let Some(p) = intent.points.first().and_then(|p| project(*p)) {
        painter.circle_stroke(p, 4., egui::Stroke::new(1.5, gold));
    }
    if let Some(p) = intent.loop_target.and_then(project) {
        painter.circle_stroke(
            p,
            9.,
            egui::Stroke::new(
                2.5,
                if intent.invalid {
                    red
                } else {
                    egui::Color32::from_rgb(122, 230, 157)
                },
            ),
        );
    }
    if let Some(p) = project(intent.anchor) {
        painter.circle_filled(p, 3., egui::Color32::from_rgb(122, 230, 157));
        if intent.invalid {
            painter.circle_stroke(
                p,
                6.,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(122, 230, 157)),
            );
        }
    }
}

pub(super) fn draw_pending_preview(
    ctx: &egui::Context,
    pieces: &[PendingPathCell],
    matrix: glam::Mat4,
    extent: glam::Vec2,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        "pavement_tail".into(),
    ));
    let project = |position| project_path_point(position, matrix, extent);
    for preview in pieces {
        let piece = &preview.cell;
        let (fill, outline, riser) = if preview.valid {
            (
                egui::Color32::from_rgba_unmultiplied(190, 176, 139, 50),
                egui::Color32::from_rgba_unmultiplied(242, 223, 155, 150),
                egui::Color32::from_rgba_unmultiplied(242, 223, 155, 100),
            )
        } else {
            (
                egui::Color32::from_rgba_unmultiplied(242, 72, 63, 70),
                egui::Color32::from_rgba_unmultiplied(255, 93, 78, 225),
                egui::Color32::from_rgba_unmultiplied(255, 93, 78, 180),
            )
        };
        let corners: Option<Vec<egui::Pos2>> = piece
            .footprint
            .iter()
            .map(|p| {
                project(Vec3::new(
                    p.x,
                    piece.center.y + piece.size.y + VOXEL_SIZE,
                    p.y,
                ))
            })
            .collect();
        if let Some(corners) = corners {
            // Tall pending cells show their rise as well as the top planform.
            // Original low paver outlines are unchanged.
            if piece.size.y > 3. * VOXEL_SIZE {
                for (i, p) in piece.footprint.iter().enumerate() {
                    if let Some(bottom) = project(Vec3::new(p.x, piece.center.y + VOXEL_SIZE, p.y))
                    {
                        painter.line_segment([bottom, corners[i]], egui::Stroke::new(1., riser));
                    }
                }
            }
            painter.add(egui::Shape::convex_polygon(
                corners,
                fill,
                egui::Stroke::new(1., outline),
            ));
        }
    }
}

fn projected_hull(mut points: Vec<glam::Vec2>) -> Vec<glam::Vec2> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    let mut lower: Vec<glam::Vec2> = Vec::new();
    let mut upper: Vec<glam::Vec2> = Vec::new();
    for &p in &points {
        while lower.len() >= 2 {
            let n = lower.len();
            if (lower[n - 1] - lower[n - 2]).perp_dot(p - lower[n - 1]) > 1e-10 {
                break;
            }
            lower.pop();
        }
        lower.push(p);
    }
    for &p in points.iter().rev() {
        while upper.len() >= 2 {
            let n = upper.len();
            if (upper[n - 1] - upper[n - 2]).perp_dot(p - upper[n - 1]) > 1e-10 {
                break;
            }
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn atlas_blocks_piece(
    piece: &GroundedPiece,
    offset: UVec3,
    dim: UVec3,
    bytes: &[u8],
    owned: &HashSet<UVec3>,
) -> bool {
    for z in 0..dim.z {
        for x in 0..dim.x {
            let point = glam::Vec2::new(
                offset.x as f32 + x as f32 + 0.5,
                offset.z as f32 + z as f32 + 0.5,
            ) * VOXEL_SIZE;
            // Actual projected geometry, including the shader's surface shell.
            // A former rigid OBB would reject valid neighbours on the inner bend.
            if !crate::app::pavement::contains_polygon(&piece.footprint, point, VOXEL_SIZE * 0.75) {
                continue;
            }
            for y in 0..dim.y {
                let material = u32::from(
                    bytes[((z * dim.y + y) * dim.x + x) as usize] & crate::builder::VOXEL_TYPE_MASK,
                );
                if material != crate::builder::VOXEL_TYPE_EMPTY
                    && !is_ground(material)
                    && !owned.contains(&(offset + UVec3::new(x, y, z)))
                {
                    log::debug!(
                        "[PATH_ATLAS] protected voxel={:?} material={material} candidate={:?}",
                        offset + UVec3::new(x, y, z),
                        piece.piece.center
                    );
                    return true;
                }
            }
        }
    }
    false
}
/// Keep paving's shallow-substrate no-stacking protection unchanged.
fn path_protection_depth(recipe: ConstructionRecipe, cell: PathCell) -> f32 {
    match recipe {
        ConstructionRecipe::Pavement(_) => cell.size.x.max(cell.size.z) + 4. * VOXEL_SIZE,
        // Walls protect the whole extruded body plus its voxel shell, not deep
        // natural rock which is outside the region they can possibly write.
        ConstructionRecipe::LowWall(_) => 0.,
    }
}
struct GroundSupport {
    point: Vec3,
    normal: Vec3,
}
impl GroundSupport {
    fn plane(&self, point: glam::Vec2) -> Vec3 {
        let d = point - glam::Vec2::new(self.point.x, self.point.z);
        Vec3::new(
            d.x,
            -(self.normal.x * d.x + self.normal.z * d.y) / self.normal.y,
            d.y,
        )
    }
}
fn ground_support(
    piece: PathCell,
    ray: impl Fn(Vec3, Vec3) -> Option<crate::builder::ContreeCpuRayHit>,
) -> Option<GroundSupport> {
    let (foot_min, foot_max) = piece.footprint.iter().fold(
        (
            glam::Vec2::splat(f32::INFINITY),
            glam::Vec2::splat(f32::NEG_INFINITY),
        ),
        |(min, max), &p| (min.min(p), max.max(p)),
    );
    let reach = (foot_max - foot_min).max_element() + 4. * VOXEL_SIZE;
    let hit = ray(piece.center + Vec3::Y * reach, Vec3::NEG_Y)
        .filter(|h| is_ground(h.voxel_type) && (h.position.y - piece.center.y).abs() < reach)?;
    let normal = super::stone_placement::surface_normal(hit.position, Vec3::NEG_Y, |p, d| {
        ray(p, d).map(|h| h.position)
    })?;
    let rotation = surface_frame(normal, piece.forward)?;
    let normal = rotation * Vec3::Y;
    let plane = |point: glam::Vec2| {
        let delta = point - glam::Vec2::new(hit.position.x, hit.position.z);
        Vec3::new(
            delta.x,
            -(normal.x * delta.x + normal.z * delta.y) / normal.y,
            delta.y,
        )
    };
    // Check actual cell support, not a rectangle that includes a neighbour's tile.
    let steps = ((foot_max - foot_min) / VOXEL_SIZE)
        .ceil()
        .as_uvec2()
        .max(glam::UVec2::ONE);
    let supports = piece.footprint.iter().copied().chain(
        (0..=steps.x)
            .flat_map(|x| {
                (0..=steps.y).map(move |z| {
                    foot_min
                        + (foot_max - foot_min)
                            * glam::Vec2::new(x as f32 / steps.x as f32, z as f32 / steps.y as f32)
                })
            })
            .filter(|p| crate::app::path_band::contains_polygon(&piece.footprint, *p, 0.)),
    );
    for support in supports {
        let support = hit.position + plane(support);
        let actual = ray(support + Vec3::Y * reach, Vec3::NEG_Y);
        if actual.is_none_or(|h| {
            !is_ground(h.voxel_type)
                || (h.position.y - support.y).abs()
                    > piece.size.x.min(piece.size.z) * 0.3 + 2. * VOXEL_SIZE
        }) {
            log::debug!(
                "[PATH_GROUND] unsupported center={:?} expected={support:?} actual={actual:?}",
                piece.center
            );
            return None;
        }
    }

    Some(GroundSupport {
        point: hit.position,
        normal,
    })
}
fn ground_piece(
    piece: PathCell,
    kind: PavementKind,
    world_size: Vec3,
    ray: impl Fn(Vec3, Vec3) -> Option<crate::builder::ContreeCpuRayHit>,
) -> Result<Option<GroundedPiece>> {
    let Some(support) = ground_support(piece, &ray) else {
        return Ok(None);
    };
    // Keep the closed, chipped stone topology, but map every shared vertex into
    // the inset course cell. Vertical extrusion keeps the joint's XZ footprint.
    let scale = piece.size.x.max(piece.size.z) / 0.3;
    let mut spec = StoneSpec::new(StoneKind::Slab, piece.seed);
    spec.size = piece.size / scale;
    spec.variation = if kind == PavementKind::Slabs {
        0.35
    } else {
        0.
    };
    spec.slab.edge_cut = if kind == PavementKind::Slabs {
        0.16
    } else {
        0.06
    };
    let mesh = generate(spec)?;
    ensure!(
        (mesh.spec.size * scale - piece.size).length() < 1e-5,
        "pavement dimensions exceeded stone generator range"
    );
    let base = support.point - Vec3::Y * VOXEL_SIZE.min(piece.size.y * 0.25);

    let piece = PathCell {
        center: support.point,
        ..piece
    };
    grounded_cell(
        piece,
        base,
        world_size,
        mesh.triangles().map(|points| {
            points.map(|point| {
                let local = point * scale;
                let foot = piece.point(
                    (local.x / piece.size.x + 0.5).clamp(0., 1.),
                    (local.z / piece.size.z + 0.5).clamp(0., 1.),
                );
                support.plane(foot) + Vec3::Y * local.y
            })
        }),
    )
}
/// One checked mesh boundary for all path constructions, before any atlas write.
fn grounded_cell(
    piece: PathCell,
    base: Vec3,
    world_size: Vec3,
    triangles: impl Iterator<Item = [Vec3; 3]>,
) -> Result<Option<GroundedPiece>> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut projected = Vec::new();
    let mut gpu = Vec::new();
    for points in triangles {
        ensure!(
            points.iter().all(|p| p.is_finite())
                && (points[1] - points[0])
                    .cross(points[2] - points[0])
                    .length_squared()
                    > 1e-16,
            "invalid path-cell triangle"
        );
        for point in points {
            let world = base + point;
            min = min.min(world);
            max = max.max(world);
            projected.push(glam::Vec2::new(world.x, world.z));
        }
        gpu.push(ModelTriangleGpu {
            a: points[0].extend(0.).to_array(),
            b: points[1].extend(0.).to_array(),
            c: points[2].extend(0.).to_array(),
        });
    }
    if gpu.is_empty()
        || !(min - Vec3::splat(VOXEL_SIZE)).cmpge(Vec3::ZERO).all()
        || !(max + Vec3::splat(VOXEL_SIZE)).cmplt(world_size).all()
    {
        return Ok(None);
    }
    let footprint = projected_hull(projected);
    ensure!(footprint.len() >= 3, "path cell has no footprint");
    Ok(Some(GroundedPiece {
        piece,
        triangles: gpu,
        base,
        footprint,
        min,
        max,
    }))
}

impl App {
    pub(super) fn cancel_pavement_stroke(&mut self) {
        // Cursor events can precede the next redraw. Complete an already edited
        // backdrag before cancelling, rather than retaining its obsolete stones.
        if let Some(mut active) = self.pavement.active.take() {
            if let Err(error) = self.apply_pending_pavement_retraction(&mut active) {
                log::error!("[PAVEMENT] cancellation withdrawal failed: {error:#}");
            }
            self.pavement.active = Some(active);
        }
        // The loop is shared with Dig; idle pavement must not mute another tool.
        if self.pavement.cancel() {
            self.stop_terrain_edit_loop_sound();
        }
    }

    pub(super) fn pavement_recipe(&self) -> PavementRecipe {
        let settings = &self.debug_settings.adjustables;
        PavementRecipe::new(
            PavementKind::from_choice(settings.pavement_kind.value),
            settings.pavement_width.value,
            settings.pavement_piece_length.value,
            settings.pavement_seed.value,
        )
        .with_smoothing(
            settings.pavement_smooth_curves.value,
            settings.pavement_smooth_scale.value,
        )
        .with_retraction(
            settings.pavement_retract_stroke.value,
            settings.pavement_retract_scale.value,
        )
    }

    fn construction_recipe(&self) -> ConstructionRecipe {
        let s = &self.debug_settings.adjustables;
        if self.player_tools.selected_tool() == PlayerTool::Wall {
            LowWallRecipe::new(
                LowWallKind::from_choice(s.low_wall_kind.value),
                s.low_wall_width.value,
                s.low_wall_height.value,
                s.pavement_piece_length.value,
                s.pavement_seed.value,
            )
            .into()
        } else {
            self.pavement_recipe().into()
        }
    }

    pub(super) fn pavement_tool_label(&self) -> String {
        let recipe = self
            .pavement
            .active
            .as_ref()
            .map_or_else(|| self.construction_recipe(), |active| active.recipe);
        match recipe {
            ConstructionRecipe::Pavement(recipe) => match recipe.kind {
                PavementKind::Slabs => "Pavement: stone slabs".into(),
                PavementKind::Brick => "Pavement: brick".into(),
                PavementKind::Dirt => "Pavement: dirt".into(),
            },
            ConstructionRecipe::LowWall(recipe) => format!(
                "Low wall: {} ({:.2} high)",
                recipe.kind.name(),
                recipe.height
            ),
        }
    }

    pub(super) fn pavement_tool_instructions(&self) -> &'static str {
        if self.pavement.active.as_ref().is_some_and(|a| {
            a.continuity.blocked().is_none()
                && a.candidate
                    .as_ref()
                    .is_some_and(|c| c.closes_loop() && c.checked)
        }) {
            return "Release to close the loop\nMove away to keep drawing an open path";
        }
        let recipe = self
            .pavement
            .active
            .as_ref()
            .map_or_else(|| self.construction_recipe(), |active| active.recipe);
        if matches!(recipe, ConstructionRecipe::LowWall(_)) {
            "Left-drag on soil; return to the start to close\nTab: material   Shift + wheel: thickness\nCtrl + wheel: height   R: settings"
        } else if recipe.is_dirt() {
            "Left-drag on soil to clear vegetation\nG: camera mode   Tab: surface\nShift + wheel: width   R: settings"
        } else if recipe.retraction_scale().is_some() {
            "Return to the start to close; backdrag retracts\nG: camera mode   Tab: surface\nShift + wheel: width   R: settings"
        } else {
            "Left-drag on soil; return to the start to close\nG: camera mode   Tab: surface\nShift + wheel: width   R: settings"
        }
    }

    pub(super) fn cycle_pavement_kind(&mut self) {
        self.cancel_pavement_stroke();
        let settings = &mut self.debug_settings.adjustables;
        let value = if self.player_tools.selected_tool() == PlayerTool::Wall {
            &mut settings.low_wall_kind.value
        } else {
            &mut settings.pavement_kind.value
        };
        *value = (*value + 1) % 3;
        self.play_item_panel_scroll_sound();
    }

    pub(super) fn adjust_pavement_width(&mut self, lines: f32) {
        if !lines.is_finite() {
            return;
        }
        let settings = &mut self.debug_settings.adjustables;
        let (width, maximum) = if self.player_tools.selected_tool() == PlayerTool::Wall {
            (&mut settings.low_wall_width.value, 0.15)
        } else {
            (&mut settings.pavement_width.value, 0.3)
        };
        *width = (*width + lines * 0.01).clamp(0.05, maximum);
    }

    pub(super) fn adjust_low_wall_height(&mut self, lines: f32) {
        if lines.is_finite() {
            let height = &mut self.debug_settings.adjustables.low_wall_height.value;
            *height = (*height + lines * 0.01).clamp(0.02, 0.3);
        }
    }

    pub(super) fn pavement_preview_width(&self) -> f32 {
        self.pavement.active.as_ref().map_or_else(
            || self.construction_recipe().path_settings().width,
            |active| active.stroke.config.width,
        )
    }

    fn pavement_pointer_hit(
        &self,
        origin: Vec3,
        direction: Vec3,
    ) -> Option<crate::builder::ContreeCpuRayHit> {
        let live = self.query_terrain_ray_cpu(origin, direction);
        if let Some(active) = &self.pavement.active {
            // Respect live raster-tree occlusion, while keeping new pavement and
            // asynchronous terrain-cache refreshes out of the ground reference.
            if live.is_some_and(|h| {
                matches!(
                    h.voxel_type,
                    crate::builder::VOXEL_TYPE_CHERRY_WOOD | crate::builder::VOXEL_TYPE_OAK_WOOD
                )
            }) {
                return None;
            }
            active.ground.query_terrain_ray_cpu(origin, direction)
        } else {
            live
        }
    }

    pub(super) fn pavement_hover(&self) -> Option<super::input::TerrainEditHover> {
        if !self.terrain_edit_pointer_available() || self.rooftop_scene.is_some() {
            return None;
        }
        let (origin, direction) = self.terrain_edit_ray()?;
        let hit = self.pavement_pointer_hit(origin, direction)?;
        if hit.position.distance(origin) > super::SHOVEL_RAY_QUERY_DISTANCE {
            return None;
        }
        let width = self.pavement_preview_width();
        Some(super::input::TerrainEditHover {
            center: hit.position,
            is_editable: !self.pavement.invalid_tail()
                && is_ground(hit.voxel_type)
                && hit.position.x > width * 0.5
                && hit.position.z > width * 0.5
                && hit.position.x + width * 0.5 < self.world_chunk_dim.x as f32
                && hit.position.z + width * 0.5 < self.world_chunk_dim.z as f32,
        })
    }

    pub(super) fn begin_pavement_stroke(&mut self) {
        // Duplicate down events cannot restart a still-held gesture.
        if self.pavement.active.is_some() {
            return;
        }
        if !self.terrain_edit_pointer_available()
            || self.rooftop_scene.is_some()
            || !self.terrain_persistence.can_start_operation()
        {
            return;
        }
        let Some((origin, direction)) = self.terrain_edit_ray() else {
            return;
        };
        let ground = self.contree_builder.cpu_ray_query_snapshot();
        let Some(hit) = self.pavement_pointer_hit(origin, direction).filter(|h| {
            is_ground(h.voxel_type)
                && h.position.distance(origin) <= super::SHOVEL_RAY_QUERY_DISTANCE
        }) else {
            return;
        };
        self.begin_path_on_ground(
            self.construction_recipe(),
            hit.position,
            self.tracer.camera_front(),
            ground,
        );
    }

    fn begin_path_on_ground(
        &mut self,
        recipe: impl Into<ConstructionRecipe>,
        start: Vec3,
        heading: Vec3,
        ground: ContreeCpuRayQuerySnapshot,
    ) {
        let recipe = recipe.into();
        if let Some(stroke) = recipe.stroke(start, heading) {
            self.pavement.active = Some(ActivePavement {
                recipe,
                stroke,
                coverage: PavementCoverage::default(),
                previous_surface: None,
                continuity: CourseContinuity::default(),
                intent: PathIntent::new(start),
                candidate: None,
                ground,
                pieces_placed: 0,
                surfaces_cleared: 0,
                backdrag: Backdrag::default(),
                edits: EditHistory::new(recipe.retraction_scale().is_some()),
                started: Instant::now(),
            });
            self.start_terrain_edit_loop_sound(start);
        }
    }

    /// Called for cursor events, not only redraws: turns between frames are retained.
    pub(super) fn sample_pavement_pointer(&mut self) {
        if self.pavement.active.is_none() {
            return;
        }
        if !self.player_tools.selected_tool().is_construction()
            || !self.terrain_edit_pointer_available()
            || !self.terrain_persistence.can_start_operation()
        {
            self.cancel_pavement_stroke();
            return;
        }
        let point = self.terrain_edit_ray().and_then(|(origin, direction)| {
            self.pavement_pointer_hit(origin, direction)
                .filter(|h| h.position.distance(origin) <= super::SHOVEL_RAY_QUERY_DISTANCE)
                .map(|h| h.position)
        });
        let active = self.pavement.active.as_mut().unwrap();
        let Some(point) = point else {
            active.candidate = None;
            active.block_here(BlockReason::Ground);
            self.stop_terrain_edit_loop_sound();
            return;
        };
        active.observe_editable_ground(point);
        // Sound starts only after real preflight, not predicted buildability.
        if self.pavement.invalid_tail() {
            self.stop_terrain_edit_loop_sound();
        }
    }

    pub(super) fn advance_pavement_stroke(&mut self) {
        // Also samples when the editing camera moves while the mouse is still.
        self.sample_pavement_pointer();
        let Some(mut active) = self.pavement.active.take() else {
            return;
        };
        let performance = perf::Step::begin(&active, "advance");
        if let Err(error) = self
            .validate_pavement_candidate(&mut active, false)
            .and_then(|()| self.publish_pavement_rows(&mut active, ROWS_PER_FRAME))
        {
            log::error!("[PAVEMENT] publication failed: {error:#}");
            self.stop_terrain_edit_loop_sound();
            return;
        }
        let moved = active.intent.take_motion();
        if active.continuity.blocked().is_some() || !moved {
            self.stop_terrain_edit_loop_sound();
        } else {
            self.start_terrain_edit_loop_sound(active.intent.cursor());
        }
        if let Some(performance) = performance {
            performance.finish(&active);
        }
        self.pavement.active = Some(active);
    }

    pub(super) fn finish_pavement_stroke(&mut self) {
        self.sample_pavement_pointer();
        let Some(active) = self.pavement.active.take() else {
            return;
        };
        if let Err(error) = self.finish_pavement_on_ground(active) {
            log::error!("[PAVEMENT] release publication failed: {error:#}");
        }
    }

    fn finish_pavement_on_ground(&mut self, mut active: ActivePavement) -> Result<ActivePavement> {
        let performance = perf::Step::begin(&active, "release");
        if active.withdrawn() {
            let result = self.apply_pending_pavement_retraction(&mut active);
            self.stop_terrain_edit_loop_sound();
            result?;
            log::info!("[PAVEMENT] withdrawn pieces=0 empty_click_suppressed=true");
            if let Some(performance) = performance {
                performance.finish(&active);
            }
            return Ok(active);
        }
        let result = self
            .validate_pavement_candidate(&mut active, true)
            .and_then(|()| {
                // Ignore rejected current target; finish the previously approved guide.
                active.candidate = None;
                active.continuity.resume();
                if active.stroke.finish() {
                    self.publish_pavement_rows(&mut active, usize::MAX)
                } else {
                    active.block_here(BlockReason::Sampling);
                    Ok(())
                }
            });
        // A failed final publication must also silence the shared emitter.
        self.stop_terrain_edit_loop_sound();
        result?;
        log::info!("[PAVEMENT] finished kind={} pieces={} surface_rows={} closed={} gesture_ms={:.2} persistence=terrain-and-flora", active.recipe.name(), active.pieces_placed, active.surfaces_cleared, active.continuity.accepted().is_some_and(|row| row.closes_loop), active.started.elapsed().as_secs_f64() * 1000.);
        if let Some(performance) = performance {
            performance.finish(&active);
        }
        Ok(active)
    }

    fn path_stamp_readback(
        &self,
        recipe: ConstructionRecipe,
        grounded: &GroundedPiece,
    ) -> PathStampReadback {
        // Contree is a visible thin shell: soil may occlude older
        // paving in it. Test the authoritative atlas columns as well
        // so joining/retracing never builds a layer over buried tiles.
        let margin = Vec3::splat(VOXEL_SIZE);
        let extra_depth = path_protection_depth(recipe, grounded.piece);
        let offset = ((grounded.min - margin - Vec3::Y * extra_depth) * 256.)
            .floor()
            .max(Vec3::ZERO)
            .as_uvec3();
        let end = ((grounded.max + margin) * 256.)
            .ceil()
            .as_uvec3()
            .min(self.world_chunk_dim * super::VOXEL_DIM_PER_CHUNK);
        PathStampReadback {
            offset,
            dim: end - offset,
        }
    }

    fn stamp_prepared_path_cell(
        &mut self,
        active: &mut ActivePavement,
        grounded: &GroundedPiece,
        readback: PathStampReadback,
        material: u32,
        bound: &mut Option<UAabb3>,
    ) -> Result<()> {
        let PathStampReadback { offset, dim } = readback;
        // Preflight snapshots for a whole row may precede another stone's stamp.
        // The undo owner must capture the immediate authoritative before-image.
        let bytes = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        let written =
            self.plain_builder
                .voxelize_model(&grounded.triangles, grounded.base, material)?;
        *bound = Some(bound.map_or(written, |bound| bound.union_with(&written)));
        let after = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        active
            .edits
            .record_stamp(offset, dim, &bytes, &after, material)?;

        Ok(())
    }
    fn publish_pavement_rows(&mut self, active: &mut ActivePavement, limit: usize) -> Result<()> {
        self.apply_pending_pavement_retraction(active)?;
        if active.withdrawn() {
            return Ok(());
        }
        if active.continuity.blocked().is_some() {
            self.stop_terrain_edit_loop_sound();
            return Ok(());
        }
        // A snapped loop is a reversible proposal until release. Do not publish
        // either its connector or the old open tail while the user is deciding.
        if active
            .candidate
            .as_ref()
            .is_some_and(Candidate::closes_loop)
        {
            return Ok(());
        }
        if !active.stroke.prepare() {
            active.block_here(BlockReason::Sampling);
            self.stop_terrain_edit_loop_sound();
            return Ok(());
        }
        let recipe = active.recipe;
        let mut bound: Option<UAabb3> = None;
        let mut cleared = Vec::new();
        let mut placed = 0;
        // Even if a later upload fails, publish the preceding successful atlas writes.
        let write_result: Result<()> = (|| {
            for _ in 0..limit {
                let Some(row) = active.continuity.next(&mut active.stroke.rows) else {
                    break;
                };
                // Identical whole-course protection to the current candidate.
                let prepared = match self.prepare_pavement_course(
                    active,
                    row,
                    &mut StagedCoverage::default(),
                )? {
                    CoursePreparation::Ready(stamps) => stamps,
                    CoursePreparation::Rejected(reason) => {
                        active
                            .continuity
                            .block(row, reason, &mut active.stroke.rows);
                        break;
                    }
                };
                let receipt_index = active.edits.begin_course();
                for stamp in prepared {
                    self.stamp_prepared_path_cell(
                        active,
                        &stamp.grounded,
                        stamp.readback,
                        stamp.material,
                        &mut bound,
                    )?;
                    active.coverage.record(stamp.grounded.piece);
                    placed += stamp.count;
                }
                active.continuity.accept(row);
                if let Some(review) = self.pavement.pointer_review.as_mut() {
                    review.record_published(row)?;
                }
                active.intent.published(
                    if active.stroke.config.sampling == PathSampling::Surface {
                        row.center
                    } else {
                        row.exit.center
                    },
                );
                cleared.push((
                    TerrainBrushEdit {
                        start: active.previous_surface.unwrap_or(row.center),
                        end: row.center,
                        radius: recipe.path_settings().width * 0.5,
                    },
                    receipt_index,
                ));
                active.previous_surface = Some(row.center);
            }
            Ok(())
        })();
        if active.continuity.blocked().is_some() {
            active.discard_unpublished();
            self.stop_terrain_edit_loop_sound();
        }
        if let Some(bound) = bound {
            self.publish_visible_terrain(
                VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                    bound,
                )])?
                .context("pavement stamp has no visible region")?,
            )?;
            self.observe_player_terrain_publication_for_connectivity(bound);
        }
        let surface_rows = cleared.len();
        for (edit, receipt_index) in cleared {
            if let Some(index) = receipt_index {
                let flora = self.apply_reversible_surface_flora_removal(edit)?;
                active.edits.record_flora(index, flora);
            } else {
                self.apply_surface_flora_removal(edit)?;
            }
        }
        active.pieces_placed += placed;
        active.surfaces_cleared += surface_rows;
        if placed > 0 || surface_rows > 0 {
            log::info!("[PAVEMENT] publish kind={} pieces={placed} surface_rows={surface_rows} path_space=true frozen_ground=true queued={}", recipe.name(), active.stroke.rows.len());
        }
        write_result?;
        active.remember_checkpoint();
        Ok(())
    }
}

#[cfg(test)]
mod candidate_tests;
#[cfg(test)]
mod prototype_tests;

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn pending_pavement_runtime() -> PavementRuntime {
        let recipe = PavementRecipe::new(PavementKind::Slabs, 0.06, 0.05, 42);
        PavementRuntime {
            active: Some(ActivePavement {
                recipe: recipe.into(),
                stroke: recipe.stroke(Vec3::ONE, Vec3::X).unwrap(),
                coverage: PavementCoverage::default(),
                previous_surface: None,
                continuity: CourseContinuity::default(),
                intent: PathIntent::new(Vec3::ONE),
                candidate: None,
                ground: crate::builder::test_cpu_voxel_source_snapshot(
                    UVec3::ONE,
                    UVec3::splat(256),
                    &[],
                    &[],
                    &[],
                    &[],
                ),
                pieces_placed: 0,
                surfaces_cleared: 0,
                backdrag: Backdrag::default(),
                edits: EditHistory::new(recipe.retract_stroke),
                started: Instant::now(),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn low_wall_pending_preview_has_all_layers_and_rejected_targets_remain_replaceable() {
        for kind in [LowWallKind::Earth, LowWallKind::Stone, LowWallKind::Brick] {
            let wall = LowWallRecipe::new(kind, 0.06, 0.16, 0.05, 42);
            let mut runtime = pending_pavement_runtime();
            let active = runtime.active.as_mut().unwrap();
            active.recipe = wall.into();
            active.stroke = active.recipe.stroke(Vec3::ONE, Vec3::X).unwrap();
            let preview = runtime.pending_preview();
            assert_eq!(preview.len(), wall.layers());
            assert!(preview.iter().all(|p| p.valid));
            let highest = preview
                .iter()
                .map(|p| p.cell.center.y + p.cell.size.y)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!((highest - 1.16).abs() < 1e-6);
            let active = runtime.active.as_mut().unwrap();
            active.observe_editable_ground(Vec3::ONE + Vec3::X * 0.3);
            active.fix_candidate();
            let origin = active.intent.origin();
            let approved = active.stroke.pending_rows();
            active.observe_editable_ground(Vec3::ONE - Vec3::X * 0.2);
            assert!(active.continuity.blocked().is_some());
            assert_eq!(active.stroke.pending_rows(), approved);
            assert!(runtime.pending_preview().iter().all(|p| !p.valid));
            let active = runtime.active.as_mut().unwrap();
            active.observe_editable_ground(Vec3::ONE + Vec3::X * 0.6);
            assert!(active.continuity.blocked().is_none());
            assert_eq!(active.intent.origin(), origin);
            assert!(runtime.pending_preview().iter().all(|p| p.valid));
        }
    }

    #[test]
    fn rejected_candidate_is_red_finite_and_does_not_change_the_approved_guide() {
        let mut runtime = pending_pavement_runtime();
        let a = runtime.active.as_mut().unwrap();
        a.observe_ground(Vec3::ONE + Vec3::X * 0.3);
        a.fix_candidate(); // Geometry seam test; native tests add real atlas/support checks.
        let approved = a.stroke.pending_rows();
        for point in [
            Vec3::ONE + Vec3::new(0.112, 0., 0.0684),
            Vec3::ONE + Vec3::new(-0.2, 0., 0.05),
        ] {
            runtime.active.as_mut().unwrap().observe_ground(point);
            assert_eq!(
                runtime.active.as_ref().unwrap().stroke.pending_rows(),
                approved
            );
            let preview = runtime.pending_preview();
            assert!(!preview.is_empty());
            assert!(preview.iter().all(|p| !p.valid));
            assert!(preview
                .iter()
                .all(|p| p.cell.footprint.iter().all(|v| v.is_finite())));
        }
    }

    #[test]
    fn current_rejected_preview_still_inherits_the_same_start_not_a_pointer_local_patch() {
        let mut runtime = pending_pavement_runtime();
        let a = runtime.active.as_mut().unwrap();
        a.observe_ground(Vec3::ONE + Vec3::X * 0.3);
        a.fix_candidate();
        let first_course_len = a.recipe.pieces(a.stroke.pending_rows()[0]).len();
        assert!(
            first_course_len > 1,
            "fixture must contain multiple paving lanes"
        );
        let target = Vec3::ONE + Vec3::new(-0.2, 0., 0.05);
        a.observe_ground(target);
        let preview = runtime.pending_preview();
        assert!(preview.len() >= first_course_len);
        assert!(preview.iter().all(|p| !p.valid));
        // Individual pavers are offset into lanes; the whole first course,
        // not its first tile, must remain centered on the original press.
        let center = preview
            .iter()
            .take(first_course_len)
            .map(|p| p.cell.center)
            .sum::<Vec3>()
            / first_course_len as f32;
        assert!(((center - Vec3::ONE) * Vec3::new(1., 0., 1.)).length() < 1e-5);
        assert!(((center - target) * Vec3::new(1., 0., 1.)).length() > 0.1);
    }

    #[test]
    fn only_fixed_endpoints_and_current_candidate_are_actually_painted() {
        let mut runtime = pending_pavement_runtime();
        let a = runtime.active.as_mut().unwrap();
        let approved = Vec3::ONE + Vec3::X * 0.3;
        a.observe_ground(approved);
        a.fix_candidate();
        let rejected = Vec3::ONE + Vec3::new(-0.2, 0., 0.05);
        a.observe_ground(Vec3::ONE + Vec3::new(-0.3, 0., 0.05));
        a.observe_ground(rejected); // Previous rejected target must disappear.
        let preview = runtime.intent_preview(&runtime.pending_preview()).unwrap();
        assert_eq!(preview.points, [Vec3::ONE, approved, rejected]);
        assert_eq!(preview.anchor, approved);
        assert_eq!(preview.floor, 1);
        assert!(preview.invalid);
        let preview = Some(preview);
        let matrix = glam::Mat4::orthographic_rh(-2., 2., -2., 2., 0.1, 10.)
            * glam::Mat4::look_at_rh(Vec3::new(2., 3., 4.), Vec3::ONE, Vec3::Y);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            draw_intent_preview(ui.ctx(), &preview, matrix, glam::Vec2::splat(800.));
        });
        assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::LineSegment { stroke, .. } if stroke.color == egui::Color32::from_rgba_unmultiplied(255, 93, 78, 225))));
        assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Circle(c) if c.fill == egui::Color32::from_rgb(122, 230, 157))));
    }

    #[test]
    fn valid_candidate_replaces_failure_without_retreat_or_new_gesture() {
        let mut runtime = pending_pavement_runtime();
        let a = runtime.active.as_mut().unwrap();
        a.observe_ground(Vec3::ONE + Vec3::X * 0.3);
        let failed = PathSample::straight(Vec3::ONE + Vec3::X * 0.3, Vec3::NEG_X, 0.3, 6, 0.05);
        a.continuity
            .block(failed, BlockReason::Turn, &mut a.stroke.rows);
        let origin = a.intent.origin();
        let began = a.started;
        a.observe_ground(Vec3::ONE + Vec3::X * 0.65); // Never visits the green endpoint.
        assert!(
            a.continuity.blocked().is_none(),
            "valid current candidate still requires retracing rejected history"
        );
        assert_eq!(a.intent.origin(), origin);
        assert_eq!(a.started, began);
        assert!(runtime.pending_preview().iter().all(|p| p.valid));
    }

    #[test]
    fn invalid_pending_cell_is_actually_painted_red() {
        let runtime = pending_pavement_runtime();
        let row = runtime.active.as_ref().unwrap().stroke.pending_rows()[0];
        let cell = runtime.active.as_ref().unwrap().recipe.pieces(row)[0];
        let matrix = glam::Mat4::orthographic_rh(-2., 2., -2., 2., 0.1, 10.)
            * glam::Mat4::look_at_rh(Vec3::new(2., 3., 4.), Vec3::ONE, Vec3::Y);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            draw_pending_preview(
                ui.ctx(),
                &[PendingPathCell { cell, valid: false }],
                matrix,
                glam::Vec2::splat(800.),
            );
        });
        assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Path(p) if p.fill == egui::Color32::from_rgba_unmultiplied(242, 72, 63, 70))), "renderer must use red, not the normal gold preview");
    }

    #[test]
    fn pointer_terrain_step_keeps_a_complete_pending_course() {
        // Captured from Winit cursor picking, not hand-authored ground-path input.
        // Crossing a one-voxel soil riser shifts the oblique ray's Z intersection.
        let points = [
            Vec3::new(0.6503109, 0.39062488, 1.200211),
            Vec3::new(0.6602107, 0.39062488, 1.200211),
            Vec3::new(0.67008054, 0.39453113, 1.2054193),
        ];
        let recipes = [
            PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42),
            PavementRecipe::new(PavementKind::Brick, 0.12, 0.05, 42),
        ];
        for recipe in recipes {
            let mut stroke =
                PathStroke::new(recipe.path_settings(), points[0], Vec3::NEG_Z).unwrap();
            for point in &points[1..] {
                assert!(stroke.push(*point));
                assert!(stroke.prepare());
                let rows = stroke.pending_rows();
                assert!(!rows.is_empty());
                assert!(
                    rows.iter()
                        .all(|r| r.valid && !recipe.pieces(*r).is_empty()),
                    "{} lost its preview on a voxel soil step: {rows:?}",
                    ConstructionRecipe::from(recipe).name()
                );
            }
            assert!(stroke.finish());
            assert!(stroke.rows.iter().all(|r| r.valid));
        }
    }

    #[test]
    fn pending_cell_risers_share_the_top_vulkan_screen_projection() {
        let extent = glam::Vec2::new(500., 400.);
        let matrix = crate::gameplay::camera::Camera::calculate_proj_mat(55., 1.25, 0.01, 5.)
            * glam::Mat4::look_at_rh(Vec3::new(1., 1., 2.), Vec3::new(1., 0.3, 1.), Vec3::Y);
        let piece = PathCell::straight(
            Vec3::new(0.65, 0.3, 1.2),
            Vec3::X,
            Vec3::new(0.06, 0.12, 0.05),
            42,
        );
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            draw_pending_preview(
                ui.ctx(),
                &[PendingPathCell {
                    cell: piece,
                    valid: true,
                }],
                matrix,
                extent,
            );
        });
        let risers: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match s.shape {
                egui::Shape::LineSegment { points, .. } => Some(points),
                _ => None,
            })
            .collect();
        assert_eq!(risers.len(), 4);
        let project = |p: Vec3| {
            let clip = matrix * p.extend(1.);
            let ndc = clip.truncate() / clip.w;
            egui::pos2(
                (0.5 + ndc.x * 0.5) * extent.x,
                (0.5 + ndc.y * 0.5) * extent.y,
            )
        };
        for (i, p) in piece.footprint.iter().enumerate() {
            let bottom = project(Vec3::new(p.x, piece.center.y + VOXEL_SIZE, p.y));
            let top = project(Vec3::new(
                p.x,
                piece.center.y + piece.size.y + VOXEL_SIZE,
                p.y,
            ));
            assert!(
                risers[i][0].distance(bottom) < 1e-4,
                "cell base is reflected across screen centre"
            );
            assert!(risers[i][1].distance(top) < 1e-4);
            assert!(
                top.y < bottom.y,
                "world-up cell must rise toward the top of the screen"
            );
        }
    }

    #[test]
    fn pavement_cancellation_reports_sound_ownership_only_once() {
        for kind in [PavementKind::Slabs, PavementKind::Brick, PavementKind::Dirt] {
            let mut stroke = PathStroke::new(
                PavementRecipe::new(kind, 0.12, 0.05, 42).path_settings(),
                Vec3::ONE,
                Vec3::X,
            )
            .unwrap();
            assert!(stroke.push(Vec3::ONE + Vec3::X * 0.2));
            assert!(stroke.finish());
            assert!(!stroke.rows.is_empty());
            let mut runtime = PavementRuntime {
                active: Some(ActivePavement {
                    recipe: PavementRecipe::new(kind, 0.12, 0.05, 42).into(),
                    stroke,
                    coverage: PavementCoverage::default(),
                    previous_surface: None,
                    continuity: CourseContinuity::default(),
                    intent: PathIntent::new(Vec3::ONE),
                    candidate: None,
                    ground: crate::builder::test_cpu_voxel_source_snapshot(
                        UVec3::ONE,
                        UVec3::splat(256),
                        &[],
                        &[],
                        &[],
                        &[],
                    ),
                    pieces_placed: 0,
                    surfaces_cleared: 0,
                    backdrag: Backdrag::default(),
                    edits: EditHistory::new(kind != PavementKind::Dirt),
                    started: Instant::now(),
                }),
                ..Default::default()
            };
            assert!(runtime.cancel(), "an active gesture must silence its loop");
            assert!(runtime.active.is_none(), "cancel must discard queued work");
            assert!(
                !runtime.cancel(),
                "idle pavement must not silence Dig's loop"
            );
        }
    }

    #[test]
    fn surface_frame_preserves_both_the_slope_normal_and_projected_path_tangent() {
        for normal in [Vec3::Y, Vec3::new(-0.3, 1., 0.2).normalize()] {
            for forward in [Vec3::X, Vec3::Z, Vec3::new(1., 0., 1.).normalize()] {
                let rotation = surface_frame(normal, forward).unwrap();
                assert!((rotation * Vec3::Y).distance(normal) < 1e-5);
                assert!(
                    (rotation * Vec3::Z)
                        .distance((forward - normal * forward.dot(normal)).normalize())
                        < 1e-5
                );
            }
        }
        assert!(surface_frame(Vec3::X, Vec3::Z).is_none());
        assert!(surface_frame(Vec3::Y, Vec3::Y).is_none());
        assert!(surface_frame(Vec3::NAN, Vec3::Z).is_none());
    }
    #[test]
    fn every_saved_width_length_extreme_and_half_brick_preserves_its_physical_dimensions() {
        for kind in [PavementKind::Slabs, PavementKind::Brick] {
            for width in [0.05, 0.12, 0.3] {
                for length in [0.025, 0.05, 0.1] {
                    let recipe = PavementRecipe::new(kind, width, length, 42);
                    for index in [0, 1] {
                        let row = crate::app::pavement::PathSample::straight(
                            Vec3::ONE,
                            Vec3::Z,
                            0.,
                            index,
                            recipe.pitch,
                        );
                        for piece in recipe.pieces(row) {
                            let grounded = ground_piece(piece, kind, Vec3::splat(2.), |p, d| {
                                Some(crate::builder::ContreeCpuRayHit {
                                    position: p + d * ((1. - p.y) / d.y),
                                    voxel_type: VOXEL_TYPE_DIRT,
                                })
                            })
                            .unwrap();
                            assert!(
                                grounded.is_some(),
                                "kind={kind:?} width={width} length={length} index={index}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn authoritative_columns_reject_occluded_old_tiles_but_ignore_neighbor_columns() {
        let piece = PathCell::straight(Vec3::ONE, Vec3::Z, Vec3::new(0.04, 0.008, 0.04), 42);
        let grounded = ground_piece(piece, PavementKind::Slabs, Vec3::splat(2.), |p, d| {
            Some(crate::builder::ContreeCpuRayHit {
                position: p + d * ((1. - p.y) / d.y),
                voxel_type: VOXEL_TYPE_DIRT,
            })
        })
        .unwrap()
        .unwrap();
        let offset = UVec3::new(256, 246, 256);
        let dim = UVec3::new(1, 16, 1);
        let mut bytes = vec![VOXEL_TYPE_DIRT as u8; 16];
        assert!(!atlas_blocks_piece(
            &grounded,
            offset,
            dim,
            &bytes,
            &HashSet::new()
        ));
        bytes[0] = VOXEL_TYPE_ROCK as u8;
        assert!(
            atlas_blocks_piece(&grounded, offset, dim, &bytes, &HashSet::new()),
            "buried paving must not be mistaken for plain soil"
        );
        assert!(!atlas_blocks_piece(
            &grounded,
            offset + UVec3::X * 32,
            dim,
            &bytes,
            &HashSet::new()
        ));
        let owned = HashSet::from([offset]);
        assert!(
            !atlas_blocks_piece(&grounded, offset, dim, &bytes, &owned),
            "the current gesture may join its own edges"
        );
        bytes[1] = VOXEL_TYPE_ROCK as u8;
        assert!(
            atlas_blocks_piece(&grounded, offset, dim, &bytes, &owned),
            "ownership must be exact, not a broad footprint exemption"
        );
    }

    #[test]
    fn tilted_tile_top_and_surface_shell_are_part_of_the_column_footprint() {
        let normal = Vec3::new(-0.4, 1., 0.).normalize();
        let piece = PathCell::straight(Vec3::ONE, Vec3::Z, Vec3::new(0.04, 0.008, 0.04), 42);
        let grounded = ground_piece(piece, PavementKind::Slabs, Vec3::splat(2.), |p, d| {
            Some(crate::builder::ContreeCpuRayHit {
                position: p + d * ((Vec3::ONE - p).dot(normal) / d.dot(normal)),
                voxel_type: VOXEL_TYPE_DIRT,
            })
        })
        .unwrap()
        .unwrap();
        assert!(atlas_blocks_piece(
            &grounded,
            UVec3::new(250, 246, 256),
            UVec3::ONE,
            &[VOXEL_TYPE_ROCK as u8],
            &HashSet::new()
        ));
    }

    #[test]
    fn an_old_stone_between_center_and_corners_is_not_paved_over() {
        let piece = PathCell::straight(Vec3::ONE, Vec3::Z, Vec3::new(0.04, 0.008, 0.04), 42);
        let grounded = ground_piece(piece, PavementKind::Slabs, Vec3::splat(2.), |p, d| {
            Some(crate::builder::ContreeCpuRayHit {
                position: p + d * ((1. - p.y) / d.y),
                voxel_type: if p.x > 1.009 && p.x < 1.015 {
                    VOXEL_TYPE_ROCK
                } else {
                    VOXEL_TYPE_DIRT
                },
            })
        })
        .unwrap();
        assert!(grounded.is_none());
    }

    #[test]
    fn bent_meshes_are_closed_outward_solids_on_flat_and_sloping_ground() {
        for kind in [PavementKind::Slabs, PavementKind::Brick] {
            for sign in [-1., 1.] {
                let recipe = PavementRecipe::new(kind, 0.12, 0.05, 42);
                let mut stroke =
                    PathStroke::new(recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
                for i in 1..=90 {
                    let t = std::f32::consts::FRAC_PI_2 * i as f32 / 90.;
                    assert!(stroke.push(
                        Vec3::ONE + Vec3::new(0.18 * t.sin(), 0., sign * 0.18 * (1. - t.cos()))
                    ));
                }
                assert!(stroke.finish());
                let mut pieces = 0;
                for row in stroke.rows {
                    for piece in recipe.pieces(row) {
                        for normal in [Vec3::Y, Vec3::new(-0.3, 1., 0.2).normalize()] {
                            let mut piece = piece;
                            piece.center.y = 1.
                                - (normal.x * (piece.center.x - 1.)
                                    + normal.z * (piece.center.z - 1.))
                                    / normal.y;
                            let grounded = ground_piece(piece, kind, Vec3::splat(2.), |p, d| {
                                Some(crate::builder::ContreeCpuRayHit {
                                    position: p + d * ((Vec3::ONE - p).dot(normal) / d.dot(normal)),
                                    voxel_type: VOXEL_TYPE_DIRT,
                                })
                            })
                            .unwrap()
                            .unwrap();
                            let mut edges = std::collections::BTreeMap::new();
                            let mut volume = 0.;
                            for triangle in &grounded.triangles {
                                let points = [triangle.a, triangle.b, triangle.c]
                                    .map(|p| Vec3::from_slice(&p));
                                volume += points[0].dot(points[1].cross(points[2])) / 6.;
                                for i in 0..3 {
                                    let a = points[i].to_array().map(f32::to_bits);
                                    let b = points[(i + 1) % 3].to_array().map(f32::to_bits);
                                    let (key, direction) =
                                        if a < b { ((a, b), 1) } else { ((b, a), -1) };
                                    let value = edges.entry(key).or_insert((0, 0));
                                    value.0 += 1;
                                    value.1 += direction;
                                }
                            }
                            assert!(volume > 1e-7, "folded or inward solid");
                            assert!(
                                edges
                                    .values()
                                    .all(|&(count, direction)| count == 2 && direction == 0),
                                "every deformed edge must have one face in each direction"
                            );
                            assert!(grounded.footprint.iter().all(|p| {
                                crate::app::pavement::contains_polygon(&piece.footprint, *p, 1e-6)
                            }));
                        }
                        pieces += 1;
                    }
                }
                assert_eq!(pieces, if kind == PavementKind::Slabs { 18 } else { 15 });
            }
        }
    }
    #[test]
    fn grounded_tiles_are_small_closed_stones_and_reject_cliffs_edges_and_existing_paving() {
        let piece = PathCell::straight(Vec3::ONE, Vec3::Z, Vec3::new(0.04, 0.008, 0.04), 42);
        let ray = |p: Vec3, d: Vec3| {
            Some(crate::builder::ContreeCpuRayHit {
                position: p + d * ((1. - p.y) / d.y),
                voxel_type: VOXEL_TYPE_DIRT,
            })
        };
        let tile = ground_piece(piece, PavementKind::Slabs, Vec3::splat(2.), ray)
            .unwrap()
            .unwrap();
        assert!(!tile.triangles.is_empty());
        assert!((tile.base.y - (1. - 0.002)).abs() < 1e-5);
        assert!(ground_piece(
            PathCell::straight(Vec3::new(0., 1., 1.), Vec3::Z, piece.size, piece.seed),
            PavementKind::Brick,
            Vec3::splat(2.),
            ray
        )
        .unwrap()
        .is_none());
        assert!(
            ground_piece(piece, PavementKind::Brick, Vec3::splat(2.), |p, d| ray(
                p, d
            )
            .map(|h| crate::builder::ContreeCpuRayHit {
                voxel_type: VOXEL_TYPE_ROCK,
                ..h
            }))
            .unwrap()
            .is_none()
        );
        assert!(
            ground_piece(piece, PavementKind::Brick, Vec3::splat(2.), |p, d| (p.x
                <= 1.001)
                .then(|| ray(p, d))
                .flatten())
            .unwrap()
            .is_none()
        );
    }
}
