//! Opt-in real atlas preflight fixture; no normal garden changes.
use super::*;
use crate::app::path_band::CellSolid;

impl App {
    pub(in crate::app::core::pavement) fn review_guarded_construction_candidates(
        &mut self,
    ) -> Result<()> {
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        let start = self
            .query_terrain_ray_cpu(Vec3::new(0.15, 2.5, 0.78), Vec3::NEG_Y)
            .filter(|h| is_ground(h.voxel_type))
            .context("guard fixture soil")?
            .position;
        // Freeze ground as at gesture start. An obstacle published afterwards
        // must still be caught by the live atlas no-stacking guard.
        let ground = self.contree_builder.cpu_ray_query_snapshot();
        let footprint = [
            glam::Vec2::new(0.38, 0.75),
            glam::Vec2::new(0.44, 0.75),
            glam::Vec2::new(0.44, 0.81),
            glam::Vec2::new(0.38, 0.81),
        ];
        let top = start.y + 0.006;
        let cell = PathCell {
            center: Vec3::new(0.41, top, 0.78),
            forward: Vec3::X,
            size: Vec3::new(0.06, 0.03, 0.06),
            seed: 0,
            footprint,
        };
        let solid = CellSolid::new(footprint, [top; 4], 0., 0.03, 1.);
        let mesh = grounded_cell(
            cell,
            Vec3::ZERO,
            self.world_chunk_dim.as_vec3(),
            solid.triangles(),
        )?
        .context("guard fixture bounds")?;
        let obstacle =
            self.plain_builder
                .voxelize_model(&mesh.triangles, mesh.base, VOXEL_TYPE_ROCK)?;
        self.publish_visible_terrain(
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                obstacle,
            )])?
            .context("guard fixture publication")?,
        )?;
        self.observe_player_terrain_publication_for_connectivity(obstacle);
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        self.begin_path_on_ground(
            PavementRecipe::new(PavementKind::Brick, 0.06, 0.05, 42),
            start,
            Vec3::X,
            ground,
        );
        let mut active = self.pavement.active.take().context("guard fixture path")?;
        let began = active.started;
        let origin = active.intent.origin();
        let lo = UVec3::new(20, 0, 170);
        let dim = UVec3::new(
            170,
            self.world_chunk_dim.y * crate::app::core::VOXEL_DIM_PER_CHUNK.y,
            70,
        );
        let before = crc32fast::hash(&self.plain_builder.read_chunk_atlas_region(lo, dim)?);
        for p in [start + Vec3::X * 0.4, start + Vec3::X * 0.48] {
            active.observe_ground(p);
            ensure!(
                active.continuity.blocked().is_none(),
                "obstacle fixture must be geometrically valid"
            );
            self.validate_pavement_candidate(&mut active, false)?;
            ensure!(
                active
                    .continuity
                    .blocked()
                    .is_some_and(|b| b.reason == BlockReason::Protected),
                "authoritative no-stacking protection was bypassed"
            );
            ensure!(active.intent.floor() == 0, "protected endpoint was fixed");
            self.publish_pavement_rows(&mut active, ROWS_PER_FRAME)?;
            ensure!(
                active.surfaces_cleared == 0 && active.pieces_placed == 0,
                "protected candidate wrote courses"
            );
            ensure!(
                crc32fast::hash(&self.plain_builder.read_chunk_atlas_region(lo, dim)?) == before,
                "candidate rejection changed atlas bytes"
            );
        }
        active.observe_ground(start + Vec3::Z * 0.2);
        self.validate_pavement_candidate(&mut active, true)?;
        ensure!(
            active.continuity.blocked().is_none() && active.intent.floor() == 1,
            "new clear target still required visiting origin/retracing"
        );
        ensure!(
            active.started == began && active.intent.origin() == origin,
            "candidate retry began a new path"
        );
        let finished = self.finish_pavement_on_ground(active)?;
        ensure!(
            finished.surfaces_cleared >= 3,
            "clear candidate did not actually publish"
        );
        ensure!(
            self.player_tools.terrain_edit_loop_sound_muted,
            "guard fixture release sound"
        );
        // Exercise the production proposal/adoption seam for surface-only Dirt
        // as well; the older paving fixture intentionally drives the compiler directly.
        let dirt_start = Vec3::new(0.1, start.y, 0.78);
        self.begin_path_on_ground(
            PavementRecipe::new(PavementKind::Dirt, 0.12, 0.05, 42),
            dirt_start,
            Vec3::NEG_Z,
            self.contree_builder.cpu_ray_query_snapshot(),
        );
        let mut dirt = self.pavement.active.take().context("Dirt candidate")?;
        let before_dirt = crc32fast::hash(&self.plain_builder.read_chunk_atlas_region(lo, dim)?);
        for k in 1..=4 {
            dirt.observe_ground(dirt_start + Vec3::NEG_Z * (0.04 * k as f32));
            self.validate_pavement_candidate(&mut dirt, false)?;
            ensure!(
                dirt.continuity.blocked().is_none(),
                "Dirt candidate preflight rejected soil"
            );
            self.publish_pavement_rows(&mut dirt, ROWS_PER_FRAME)?;
        }
        let dirt = self.finish_pavement_on_ground(dirt)?;
        ensure!(
            dirt.pieces_placed == 0 && dirt.surfaces_cleared >= 5,
            "Dirt candidate raised terrain or lost distance sampling"
        );
        ensure!(
            crc32fast::hash(&self.plain_builder.read_chunk_atlas_region(lo, dim)?) == before_dirt,
            "surface-only Dirt candidate changed atlas bytes"
        );
        log::info!("[PAVEMENT_CANDIDATE_REVIEW] dirt_candidate=published surface_sampling=retained atlas=unchanged");
        log::info!("[PAVEMENT_CANDIDATE_REVIEW] protected_targets=discarded live_atlas_guard=retained rejected_atlas=exact clear_target=published fixed_history=valid_only no_retreat=true same_gesture=true");
        Ok(())
    }
}
