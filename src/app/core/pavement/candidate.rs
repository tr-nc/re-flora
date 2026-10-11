//! Shared planning preflight: current proposals and actual whole-course publication
//! use identical support, projected footprint, clearance and atlas protection.
use super::*;
mod review;

pub(super) struct Candidate {
    pub stroke: PathStroke,
    pub endpoint: Vec3,
    pub expected: u32,
    pub checked: bool,
}
#[derive(Default)]
pub(super) struct StagedCoverage {
    paving: PavementCoverage,
}
pub(super) struct PreparedStamp {
    pub grounded: GroundedPiece,
    pub readback: PathStampReadback,
    pub material: u32,
    pub count: usize,
}
pub(super) enum CoursePreparation {
    Ready(Vec<PreparedStamp>),
    Rejected(BlockReason),
}

impl App {
    pub(super) fn validate_pavement_candidate(
        &mut self,
        active: &mut ActivePavement,
        force: bool,
    ) -> Result<()> {
        if active
            .candidate
            .as_ref()
            .is_some_and(|c| c.expected != active.continuity.expected())
        {
            // Publications can advance between a sub-spacing preview and release.
            // Never adopt its stale queue or generated checkpoint.
            active.propose_current();
        }
        let Some(candidate) = active.candidate.as_ref() else {
            return Ok(());
        };
        if active.continuity.blocked().is_some() {
            return Ok(());
        }
        if !candidate.checked {
            let rows = if active.stroke.config.sampling == PathSampling::Surface {
                // Surface sampling emits during push; include its latest endpoint
                // by finishing a private copy, never the actual world compiler.
                let mut end = candidate.stroke.clone();
                if !end.finish() {
                    active.block_here(BlockReason::Sampling);
                    return Ok(());
                }
                end.rows.into_iter().collect()
            } else {
                candidate.stroke.pending_rows()
            };
            let mut staged = StagedCoverage::default();
            let mut expected = active.continuity.expected();
            for row in rows {
                if row.index != expected {
                    active
                        .continuity
                        .block(row, BlockReason::Gap, &mut Default::default());
                    return Ok(());
                }
                expected = expected.wrapping_add(1);
                match self.prepare_pavement_course(active, row, &mut staged)? {
                    CoursePreparation::Ready(_) => {}
                    CoursePreparation::Rejected(reason) => {
                        active
                            .continuity
                            .block(row, reason, &mut Default::default());
                        return Ok(());
                    }
                }
            }
            active.candidate.as_mut().unwrap().checked = true;
        }
        // Keep live previews between spatially spaced fixed endpoints. On release,
        // the last valid small residual is approved too.
        if force || active.intent.spaced(active.stroke.config.pitch) {
            active.fix_candidate();
        }
        Ok(())
    }

    pub(super) fn prepare_pavement_course(
        &mut self,
        active: &ActivePavement,
        row: PathSample,
        staged: &mut StagedCoverage,
    ) -> Result<CoursePreparation> {
        use CoursePreparation::{Ready, Rejected};
        if !row.valid {
            return Ok(Rejected(BlockReason::Turn));
        }
        let paving = active.recipe;
        match paving.kind {
            PavementKind::Dirt => {
                if !active
                    .ground
                    .query_terrain_ray_cpu(row.center + Vec3::Y * 4. * VOXEL_SIZE, Vec3::NEG_Y)
                    .is_some_and(|h| {
                        is_ground(h.voxel_type) && h.position.distance(row.center) < 8. * VOXEL_SIZE
                    })
                {
                    return Ok(Rejected(BlockReason::Ground));
                }
                Ok(Ready(Vec::new()))
            }
            PavementKind::Slabs | PavementKind::Brick => {
                let pieces = paving.pieces(row);
                if pieces.is_empty() {
                    return Ok(Rejected(BlockReason::Turn));
                }
                let mut prepared = Vec::with_capacity(pieces.len());
                for piece in pieces {
                    if !active.coverage.accepts(piece) || !staged.paving.accepts(piece) {
                        return Ok(Rejected(BlockReason::Overlap));
                    }
                    let Some(grounded) = ground_piece(
                        piece,
                        paving.kind,
                        self.world_chunk_dim.as_vec3(),
                        |p, d| active.ground.query_terrain_ray_cpu(p, d),
                    )?
                    else {
                        return Ok(Rejected(BlockReason::Ground));
                    };
                    let Some(readback) = self.prepare_path_cell_stamp(active, &grounded)? else {
                        return Ok(Rejected(BlockReason::Protected));
                    };
                    staged.paving.record(grounded.piece);
                    prepared.push(PreparedStamp {
                        grounded,
                        readback,
                        count: 1,
                        material: if paving.kind == PavementKind::Brick {
                            VOXEL_TYPE_TERRACOTTA
                        } else {
                            VOXEL_TYPE_ROCK
                        },
                    });
                }
                Ok(Ready(prepared))
            }
        }
    }
}
