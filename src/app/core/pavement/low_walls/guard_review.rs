//! Opt-in native regression: a live obstacle above the frozen ground must reject
//! the entire proposal, and a subsequent clear target must remain usable.
use super::*;
use crate::app::path_band::CellSolid;

impl App {
    pub(super) fn review_low_wall_guards(&mut self) -> Result<()> {
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        let start = self
            .query_terrain_ray_cpu(Vec3::new(0.15, 2.5, 0.78), Vec3::NEG_Y)
            .filter(|hit| is_ground(hit.voxel_type))
            .context("wall guard fixture soil")?
            .position;
        let ground = self.contree_builder.cpu_ray_query_snapshot();
        let offset = UVec3::new(20, 0, 170);
        let dim = UVec3::new(170, self.world_chunk_dim.y * 256, 70);
        let original = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        let footprint = [
            glam::Vec2::new(0.38, 0.75),
            glam::Vec2::new(0.44, 0.75),
            glam::Vec2::new(0.44, 0.81),
            glam::Vec2::new(0.38, 0.81),
        ];
        // Floating only for this test: no ground ray can be mistaken for the
        // upper-body protection decision. Restore the original atlas afterwards.
        let solid = CellSolid::new(footprint, [start.y; 4], 0.07, 0.10, 1.);
        let mesh = grounded_cell(
            PathCell {
                center: Vec3::new(0.41, start.y, 0.78),
                forward: Vec3::X,
                size: Vec3::new(0.06, 0.10, 0.06),
                seed: 0,
                footprint,
            },
            Vec3::ZERO,
            self.world_chunk_dim.as_vec3(),
            solid.triangles(),
        )?
        .context("wall guard fixture bounds")?;
        let bound =
            self.plain_builder
                .voxelize_model(&mesh.triangles, mesh.base, VOXEL_TYPE_ROCK)?;
        // Deliberately keep the frozen ground. The authoritative live atlas must
        // detect even an obstacle published after the gesture began.
        self.publish_visible_terrain(
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                bound,
            )])?
            .context("wall guard publication")?,
        )?;
        self.observe_player_terrain_publication_for_connectivity(bound);
        let protected = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        for kind in [LowWallKind::Earth, LowWallKind::Stone, LowWallKind::Brick] {
            self.begin_path_on_ground(
                LowWallRecipe::new(kind, 0.06, 0.12, 0.05, 42),
                start,
                Vec3::X,
                ground.clone(),
            );
            let mut active = self.pavement.active.take().context("wall guard gesture")?;
            let began = active.started;
            for point in [start + Vec3::X * 0.4, start + Vec3::X * 0.48] {
                active.observe_editable_ground(point);
                self.validate_pavement_candidate(&mut active, false)?;
                ensure!(
                    active
                        .continuity
                        .blocked()
                        .is_some_and(|b| b.reason == BlockReason::Protected),
                    "upper obstacle did not reject {kind:?}: {:?}",
                    active.continuity.blocked().map(|b| b.reason)
                );
                self.publish_pavement_rows(&mut active, ROWS_PER_FRAME)?;
                ensure!(
                    active.pieces_placed == 0
                        && active.surfaces_cleared == 0
                        && active.intent.floor() == 0,
                    "protected wall candidate published a partial column"
                );
                ensure!(
                    self.plain_builder.read_chunk_atlas_region(offset, dim)? == protected,
                    "rejected wall changed authoritative atlas"
                );
            }
            active.observe_editable_ground(start + Vec3::Z * 0.2);
            self.validate_pavement_candidate(&mut active, true)?;
            ensure!(
                active.continuity.blocked().is_none()
                    && active.intent.floor() == 1
                    && active.started == began
                    && active.intent.origin() == start,
                "clear wall candidate could not replace the rejected endpoint"
            );
            self.pavement.active = Some(active);
            self.cancel_pavement_stroke();
            ensure!(
                self.plain_builder.read_chunk_atlas_region(offset, dim)? == protected,
                "cancelling an unbuilt wall changed the atlas"
            );
            log::info!("[LOW_WALL_REVIEW] kind={kind:?} upper_obstacle=protected rejected_atlas=exact clear_target=recovered same_gesture=true");
        }
        self.plain_builder
            .write_chunk_atlas_region(offset, dim, &original)?;
        self.publish_visible_terrain(
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                bound,
            )])?
            .context("wall guard restoration")?,
        )?;
        self.observe_player_terrain_publication_for_connectivity(bound);
        ensure!(
            self.plain_builder.read_chunk_atlas_region(offset, dim)? == original,
            "wall guard fixture must restore the original terrain"
        );
        Ok(())
    }
}
