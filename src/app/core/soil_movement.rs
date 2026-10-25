//! Application boundary for conservative soil tools. No material supply/debug
//! option can create soil in a movement transaction.
use super::player_tools::{ContinuousTerrainToolAction, PlayerTool};
use super::*;
use crate::app::world_edits::{TerrainBrushEdit, VoxelEdit};
use crate::builder::VOXEL_TYPE_DIRT;

impl App {
    pub(super) fn try_plow(&mut self, now: Instant) {
        let action = ContinuousTerrainToolAction::Plow;
        if !self.terrain_edit_pointer_available()
            || self.player_tools.selected_tool() != PlayerTool::Plow
        {
            self.player_tools.interrupt_stroke(action);
            return;
        }
        let Some(pick) = self.pointer_picking.active else {
            return;
        };
        let Some(hit) = pick.hit.filter(|h| {
            h.tree_primitive.is_none()
                && h.voxel_type == VOXEL_TYPE_DIRT
                && h.distance <= SHOVEL_RAY_QUERY_DISTANCE
        }) else {
            self.player_tools.defer_stroke(action, now);
            return;
        };
        let center = hit.world_position;
        if self
            .player_tools
            .previous_stroke_center(action)
            .is_some_and(|p| {
                let d = center - p;
                d.x * d.x + d.z * d.z < (1. / 256_f32).powi(2)
            })
        {
            self.player_tools.record_stroke_dab(action, now, center);
            return;
        }
        let mut edit = self.player_tools.stroke_edit_with_radius(
            action,
            center,
            (self.player_tools.terrain_edit_radius * 256.).clamp(2., 6.) / 256.,
        );
        if edit.start.distance(edit.end) > 32. / 256. {
            edit.start = edit.end;
        }
        if let Err(e) = self.apply_conservative_plow(edit) {
            log::error!("[SOIL_MOVE] plow failed: {e:#}");
            self.player_tools.interrupt_stroke(action);
            return;
        }
        self.player_tools.record_stroke_dab(action, now, center);
    }

    pub(super) fn apply_conservative_plow(&mut self, edit: TerrainBrushEdit) -> Result<u32> {
        let started = Instant::now();
        let start = edit.start * 256.;
        let end = edit.end * 256.;
        let width = edit.radius * 256.;
        if !start.is_finite()
            || !end.is_finite()
            || !(2. ..=6.).contains(&width)
            || start.distance(end) > 32.
        {
            return Ok(0);
        }
        let legal = if self.rooftop_scene.is_some() {
            UAabb3::new(rooftop_scene::SOIL_MIN, rooftop_scene::SOIL_MAX)
        } else {
            INITIAL_EDITABLE_TERRAIN_BOUNDS.voxel_bounds(VOXEL_DIM_PER_CHUNK)
        };
        if start.cmplt(legal.min().as_vec3()).any()
            || end.cmplt(legal.min().as_vec3()).any()
            || start.cmpge(legal.max().as_vec3()).any()
            || end.cmpge(legal.max().as_vec3()).any()
        {
            return Ok(0);
        }
        let halo = Vec3::new(width + 16., 12., width + 16.);
        let min = (start.min(end) - halo)
            .floor()
            .max(legal.min().as_vec3())
            .as_uvec3();
        let max = (start.max(end) + Vec3::new(halo.x, 20., halo.z))
            .ceil()
            .min(legal.max().as_vec3())
            .as_uvec3();
        if min.cmpge(max).any() {
            return Ok(0);
        }
        let bytes = self.plain_builder.read_chunk_atlas_region(min, max - min)?;
        let plan = crate::soil_movement::plow(
            &bytes,
            UAabb3::new(min, max),
            start,
            end,
            width,
            self.debug_settings
                .adjustables
                .soil_deposition_repose_slope
                .value,
        );
        let Some(plan) = plan else {
            return Ok(0);
        };
        let moved = plan.moved;
        // Debit and credit are in the SAME bounded atlas upload. Even a later
        // render/physics publication failure cannot destroy one side of the move.
        self.plain_builder
            .write_chunk_atlas_region(plan.origin, plan.dim, &plan.data)?;
        self.plain_builder.mark_all_solid_workgroups_dirty();
        self.surface_builder
            .remove_authored_flora_buried_by_voxels(&plan.deposited)?;
        let bound = UAabb3::new(
            plan.origin.saturating_sub(UVec3::ONE),
            (plan.origin + plan.dim).min(self.world_chunk_dim * VOXEL_DIM_PER_CHUNK - UVec3::ONE),
        );
        self.publish_visible_terrain(
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                bound,
            )])?
            .context("soil move publication")?,
        )?;
        log::info!("[SOIL_MOVE] plow moved={moved} removed={moved} deposited={moved} read_bytes={} elapsed_ms={:.3} cohesive_existing_terrain=true",bytes.len(),started.elapsed().as_secs_f64()*1000.);
        Ok(moved)
    }

    pub(super) fn seed_soil_movement_review(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_SOIL_MOVE_REVIEW") else {
            return Ok(());
        };
        let origin = UVec3::new(160, 112, 256);
        let dim = UVec3::new(96, 48, 96);
        let count = |data: &[u8]| data.iter().filter(|&&v| v & 15 == 2).count();
        if mode != "load" {
            let mut data = vec![0; dim.element_product() as usize];
            for z in 0..dim.z {
                for y in 0..12 {
                    for x in 0..dim.x {
                        data[((z * dim.y + y) * dim.x + x) as usize] = 2;
                    }
                }
            }
            self.execute_world_edit(WorldEditTransaction::terrain_change(
                vec![VoxelEdit::RestoreAtlasRegion {
                    origin,
                    dim,
                    data: data.into(),
                }],
                UAabb3::new(origin, origin + dim - UVec3::ONE),
            ))?;
            let before = count(&self.plain_builder.read_chunk_atlas_region(origin, dim)?);
            let mut moved = 0;
            for (a, b) in [(178., 202.), (202., 226.)] {
                moved += self.apply_conservative_plow(TerrainBrushEdit {
                    start: Vec3::new(a, 124., 300.) / 256.,
                    end: Vec3::new(b, 124., 300.) / 256.,
                    radius: 6. / 256.,
                })?;
            }
            let after = count(&self.plain_builder.read_chunk_atlas_region(origin, dim)?);
            anyhow::ensure!(
                before == after && moved > 100,
                "plow failed soil conservation {before}->{after}, moved={moved}"
            );
            log::info!("[SOIL_MOVE_REVIEW] native=true before={before} after={after} moved={moved} conserved=true");
        } else {
            let after = count(&self.plain_builder.read_chunk_atlas_region(origin, dim)?);
            anyhow::ensure!(
                after == 96 * 12 * 96,
                "loaded plow soil count changed: {after}"
            );
            log::info!("[SOIL_MOVE_REVIEW] load=true soil={after} conserved=true");
        }
        self.camera_control.apply_snapshot_mode(true);
        let target = Vec3::new(0.79, 0.48, 1.17);
        self.camera_control.set_orbit_focus(target);
        self.tracer
            .set_camera_pose_looking_at(Vec3::new(0.64, 0.83, 1.54), target);
        self.reset_camera_movement_input();
        Ok(())
    }
}
