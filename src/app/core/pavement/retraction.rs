//! Current-gesture backdrag, with checkpoints and exact terrain/flora receipts.
//! Completed paths have no receipts here and cannot be erased by this gesture.
use super::edit_history::RetractionTarget;
use super::*;
use crate::builder::FloraRemovalReceipt;

#[derive(Default)]
pub(super) struct Backdrag {
    pointer: Option<Vec3>,
    checkpoints: Vec<Checkpoint>,
    rollback_to: Option<usize>,
    retracted: bool,
}
struct Checkpoint {
    point: Vec3,
    stroke: PathStroke,
    continuity: CourseContinuity,
    intent: (usize, usize),
    coverage: usize,
    previous_surface: Option<Vec3>,
    pieces: usize,
    surfaces: usize,
    edits: usize,
}
impl ActivePavement {
    pub(super) fn remember_checkpoint(&mut self) {
        if !self.recipe.retract_stroke
            || self.recipe.kind == PavementKind::Dirt
            || self.continuity.blocked().is_some()
            || self.backdrag.rollback_to.is_some()
        {
            return;
        }
        let point = self.intent.anchor();
        let last = self.backdrag.checkpoints.last();
        if last.is_some_and(|last| {
            last.point == point && last.continuity.expected() == self.continuity.expected()
        }) {
            return;
        }
        let checkpoint = Checkpoint {
            point,
            stroke: self.stroke.clone(),
            continuity: self.continuity.clone(),
            intent: self.intent.checkpoint(),
            coverage: self.coverage.published_pieces().len(),
            previous_surface: self.previous_surface,
            pieces: self.pieces_placed,
            surfaces: self.surfaces_cleared,
            edits: self.edits.checkpoint(),
        };
        if last.is_some_and(|last| last.point == point) {
            *self.backdrag.checkpoints.last_mut().unwrap() = checkpoint;
        } else {
            self.backdrag.checkpoints.push(checkpoint);
        }
    }
    pub(super) fn retracting(&self) -> bool {
        self.backdrag.retracted
    }
    pub(super) fn withdrawn(&self) -> bool {
        self.backdrag.retracted
            && self.intent.floor() == 0
            && self.intent.cursor() == self.intent.origin()
    }
    pub(super) fn observe_editable_ground(&mut self, point: Vec3) {
        if !self.recipe.retract_stroke || self.recipe.kind == PavementKind::Dirt {
            self.observe_ground(point);
            return;
        }
        if !point.is_finite() {
            return;
        }
        self.remember_checkpoint();
        let pointer = self.backdrag.pointer.unwrap_or(self.intent.cursor());
        let motion = point - pointer;
        let travel = glam::Vec2::new(motion.x, motion.z).length();
        if travel < 1e-10 {
            return;
        }
        self.backdrag.pointer = Some(point);
        let previous = self.intent.cursor();
        let mut points: Vec<_> = self.backdrag.checkpoints.iter().map(|c| c.point).collect();
        if points.last() != Some(&previous) {
            points.push(previous);
        }
        let capture = self.recipe.width * 0.25 * self.recipe.retract_scale;
        let mut remaining = 0.;
        let mut hit = None;
        for i in (0..points.len().saturating_sub(1)).rev() {
            let edge = points[i + 1] - points[i];
            let edge2 = glam::Vec2::new(edge.x, edge.z);
            let extent = edge2.length();
            if extent < 1e-10 {
                continue;
            }
            let delta = point - points[i];
            let raw = glam::Vec2::new(delta.x, delta.z).dot(edge2) / edge2.length_squared();
            let t = raw.clamp(0., 1.);
            let projected = points[i].lerp(points[i + 1], t);
            let target = if i == 0 && raw < 0. {
                points[i].lerp(points[i + 1], raw)
            } else {
                projected
            };
            let removed = remaining + (1. - t) * extent;
            if removed > travel + 2. * capture {
                break;
            }
            if removed > 1e-8
                && glam::Vec2::new(motion.x, motion.z).dot(edge2) < -1e-10
                && glam::Vec2::new(point.x - target.x, point.z - target.z).length() <= capture
            {
                hit = Some((i, projected));
                break;
            }
            remaining += extent;
        }
        let Some((i, projected)) = hit else {
            self.backdrag.retracted = false;
            self.observe_ground(point);
            return;
        };
        let checkpoint = &self.backdrag.checkpoints[i];
        let keep = checkpoint.edits;
        self.stroke = checkpoint.stroke.clone();
        self.continuity = checkpoint.continuity.clone();
        self.continuity.resume();
        self.intent
            .restore_prefix(checkpoint.intent, checkpoint.point);
        self.coverage.truncate(checkpoint.coverage);
        self.previous_surface = checkpoint.previous_surface;
        self.pieces_placed = checkpoint.pieces;
        self.surfaces_cleared = checkpoint.surfaces;
        self.candidate = None;
        self.backdrag.checkpoints.truncate(i + 1);
        self.backdrag.rollback_to =
            Some(self.backdrag.rollback_to.map_or(keep, |old| old.min(keep)));
        self.backdrag.retracted = true;
        if projected != self.intent.cursor() {
            self.observe_ground(projected);
        }
    }
}
impl App {
    pub(super) fn apply_pending_pavement_retraction(
        &mut self,
        active: &mut ActivePavement,
    ) -> Result<()> {
        let Some(keep) = active.backdrag.rollback_to else {
            return Ok(());
        };
        let removed = active.edits.retract(keep, self)?;
        active.backdrag.rollback_to = None;
        log::info!("[PAVEMENT] retract courses={removed} retained_pieces={} terrain_before_images=true flora_restored=true current_gesture_only=true",active.pieces_placed);
        Ok(())
    }
}

impl RetractionTarget<FloraRemovalReceipt> for App {
    fn read_voxels(&mut self, offset: UVec3, dim: UVec3) -> Result<Vec<u8>> {
        self.plain_builder.read_chunk_atlas_region(offset, dim)
    }
    fn write_voxels(&mut self, offset: UVec3, dim: UVec3, bytes: &[u8]) -> Result<()> {
        self.plain_builder
            .write_chunk_atlas_region(offset, dim, bytes)
    }
    fn publish_voxels(&mut self, bound: UAabb3) -> Result<()> {
        self.publish_visible_terrain(
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                bound,
            )])?
            .context("pavement withdrawal must publish terrain")?,
        )?;
        self.observe_player_terrain_publication_for_connectivity(bound);
        Ok(())
    }
    fn restore_flora(&mut self, receipt: &FloraRemovalReceipt) -> Result<()> {
        self.surface_builder.restore_removed_flora(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::super::prototype_tests::{begin, publish, validate};
    use super::*;
    fn advance(a: &mut ActivePavement) {
        if let Some(keep) = a.backdrag.rollback_to.take() {
            // This fixture publishes geometry only, so it has no world receipts.
            assert_eq!(keep, a.edits.checkpoint());
        }
        if a.withdrawn() {
            return;
        }
        validate(a, a.retracting());
        publish(a);
        a.pieces_placed = a.coverage.published_pieces().len();
        a.remember_checkpoint();
    }
    #[test]
    fn pavement_prototype_recorded_backdrag_reclaims_baked_and_unbaked_tail() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/retraction.json")).unwrap();
        for kind in [PavementKind::Slabs, PavementKind::Brick] {
            for transform in 0..3 {
                let mut a = None;
                for action in fixture["actions"].as_array().unwrap() {
                    let point = || {
                        let p = action["point"].as_array().unwrap();
                        let (x, z) = (p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32);
                        Vec3::ONE
                            + match transform {
                                1 => Vec3::new(-z, 0., x),
                                2 => Vec3::new(-x, 0., z),
                                _ => Vec3::new(x, 0., z),
                            }
                    };
                    match action["type"].as_str().unwrap() {
                        "begin" => {
                            a = Some(begin(point(), PavementRecipe::new(kind, 0.12, 0.05, 42)))
                        }
                        "observe" => a.as_mut().unwrap().observe_editable_ground(point()),
                        "advance" => advance(a.as_mut().unwrap()),
                        "finish" => {
                            let a = a.as_mut().unwrap();
                            if let Some(keep) = a.backdrag.rollback_to.take() {
                                assert_eq!(keep, a.edits.checkpoint());
                            }
                            validate(a, true);
                            a.candidate = None;
                            assert!(a.stroke.finish());
                            publish(a);
                            let exit = a.continuity.accepted().unwrap().exit.center;
                            assert!(
                                exit.distance(a.intent.cursor())
                                    <= a.recipe.pitch * 0.5 + VOXEL_SIZE * 0.5 + 1e-5
                            );
                            assert!(
                                a.coverage.published_pieces().len() < 30,
                                "old long tail must not survive"
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    #[test]
    fn pavement_backdrag_handles_rejected_tail_stationary_pointer_origin_and_branch() {
        for direction in [Vec3::X, Vec3::Z, Vec3::NEG_X, Vec3::NEG_Z] {
            let mut a = begin(
                Vec3::ONE,
                PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42),
            );
            for i in 1..=30 {
                a.observe_editable_ground(Vec3::ONE + direction * (i as f32 * 0.02));
                advance(&mut a);
            }
            let before = a.coverage.published_pieces().len();
            assert!(before > 0);
            a.block_here(BlockReason::Ground);
            let sideways = Vec3::new(direction.z, 0., -direction.x);
            let raw = Vec3::ONE + direction * 0.2 + sideways * 0.025;
            a.observe_editable_ground(raw);
            advance(&mut a);
            assert!(a.coverage.published_pieces().len() < before);
            let edited = a.intent.cursor();
            let count = a.coverage.published_pieces().len();
            for _ in 0..4 {
                a.observe_editable_ground(raw);
                advance(&mut a);
                assert_eq!(a.intent.cursor(), edited);
                assert_eq!(a.coverage.published_pieces().len(), count);
            }
            for i in (0..10).rev() {
                a.observe_editable_ground(Vec3::ONE + direction * (i as f32 * 0.02));
                advance(&mut a);
            }
            assert!(a.withdrawn());
            assert!(a.coverage.published_pieces().is_empty());
            assert!(a.stroke.rows.is_empty());
            a.observe_editable_ground(Vec3::ONE + sideways * 0.25);
            advance(&mut a);
            assert!(!a.withdrawn());
            assert_eq!(a.intent.origin(), Vec3::ONE);
        }
    }
}
