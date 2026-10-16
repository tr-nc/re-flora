//! Opt-in real atlas/terrain fixture for reversible closure and saved rings.
//! Uses production candidate/publication APIs, never writes GUI preferences.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Witness {
    cells: Vec<([u32; 3], u8)>,
    interiors: Vec<([u32; 3], Vec<u8>)>,
    ground_y: f32,
}
impl App {
    pub(super) fn review_path_loops(&mut self, reload: bool) -> Result<()> {
        let directory = std::path::Path::new("target/path-loops");
        std::fs::create_dir_all(directory)?;
        let path = directory.join("witness.json");
        let height = self.world_chunk_dim.y * 256;
        let column = UVec3::new(1, height, 1);
        let mut witness = if reload {
            ensure!(
                self.terrain_persistence.startup_load_requested(),
                "loop verification needs --terrain-load"
            );
            serde_json::from_slice::<Witness>(&std::fs::read(&path)?)?
        } else {
            Witness {
                cells: vec![],
                interiors: vec![],
                ground_y: 0.,
            }
        };
        if !reload {
            for (slot, kind) in [LowWallKind::Earth, LowWallKind::Stone, LowWallKind::Brick]
                .into_iter()
                .enumerate()
            {
                self.contree_builder.flush_cpu_chunk_cache_jobs();
                let ground = self.contree_builder.cpu_ray_query_snapshot();
                let center = Vec3::new(0.35 + slot as f32 * 0.65, 0., 1.6);
                let radius = 0.22;
                let mut points = Vec::new();
                for i in 0..=128 {
                    let t = i as f32 / 128.;
                    let delta = if kind == LowWallKind::Stone {
                        // A start at a rectangle corner, not only easy circles.
                        let corners = [
                            Vec3::new(-radius, 0., -radius),
                            Vec3::new(radius, 0., -radius),
                            Vec3::new(radius, 0., radius),
                            Vec3::new(-radius, 0., radius),
                            Vec3::new(-radius, 0., -radius),
                        ];
                        let edge = ((t * 4.) as usize).min(3);
                        corners[edge].lerp(corners[edge + 1], t * 4. - edge as f32)
                    } else {
                        let angle = t * std::f32::consts::TAU;
                        Vec3::new(angle.cos(), 0., angle.sin()) * radius
                    };
                    let p = center + delta;
                    points.push(
                        ground
                            .query_terrain_ray_cpu(Vec3::new(p.x, 2.5, p.z), Vec3::NEG_Y)
                            .filter(|h| is_ground(h.voxel_type))
                            .with_context(|| {
                                format!("loop fixture requires soil: kind={kind:?} point={p:?}")
                            })?
                            .position,
                    );
                }
                std::fs::write(
                    directory.join(format!("{kind:?}-points.json")),
                    serde_json::to_vec(&points.iter().map(|p| p.to_array()).collect::<Vec<_>>())?,
                )?;
                witness.ground_y = points[0].y;
                let interior = UVec3::new((center.x * 256.) as u32, 0, (center.z * 256.) as u32);
                let before = self
                    .plain_builder
                    .read_chunk_atlas_region(interior, column)?;
                let wall = LowWallRecipe::new(kind, 0.06, 0.12, 0.05, 42);
                self.begin_path_on_ground(wall, points[0], Vec3::Z, ground);
                let mut active = self.pavement.active.take().context("loop gesture")?;
                let mut rows = Vec::new();
                for &point in &points[1..] {
                    active.observe_editable_ground(point);
                    self.validate_pavement_candidate(&mut active, false)?;
                    ensure!(
                        active.continuity.blocked().is_none(),
                        "loop {kind:?} rejected: {:?} point={point:?} closure={} first={:?} last={:?}",
                        active.continuity.blocked().map(|b| b.reason),
                        active.candidate.as_ref().is_some_and(Candidate::closes_loop),
                        active.continuity.first(), active.continuity.accepted()
                    );
                    if !active
                        .candidate
                        .as_ref()
                        .is_some_and(Candidate::closes_loop)
                    {
                        ensure!(active.stroke.prepare(), "loop streaming");
                        let queued = active.stroke.rows.clone();
                        self.publish_pavement_rows(&mut active, ROWS_PER_FRAME)?;
                        rows.extend(
                            queued
                                .into_iter()
                                .filter(|r| r.index < active.continuity.expected()),
                        );
                    }
                }
                ensure!(
                    active
                        .candidate
                        .as_ref()
                        .is_some_and(|c| c.closes_loop() && c.checked),
                    "loop {kind:?} did not snap"
                );
                let count = active.continuity.expected();
                let offset = UVec3::new(
                    ((center.x - radius - 0.1) * 256.).floor() as u32,
                    0,
                    ((center.z - radius - 0.1) * 256.).floor() as u32,
                );
                let dim = UVec3::new(
                    ((radius + 0.1) * 512.).ceil() as u32,
                    height,
                    ((radius + 0.1) * 512.).ceil() as u32,
                );
                let preview_before = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
                self.publish_pavement_rows(&mut active, ROWS_PER_FRAME)?;
                ensure!(
                    active.continuity.expected() == count,
                    "loop preview published early"
                );
                ensure!(
                    self.plain_builder.read_chunk_atlas_region(offset, dim)? == preview_before,
                    "loop preview changed atlas"
                );
                let join = active.candidate.as_ref().unwrap().stroke.rows.clone();
                let active = self.finish_pavement_on_ground(active)?;
                ensure!(
                    active.continuity.blocked().is_none(),
                    "loop release blocked"
                );
                rows.extend(
                    join.into_iter()
                        .filter(|r| r.index < active.continuity.expected()),
                );
                let first = rows[0];
                let last = *rows.last().context("loop rows")?;
                ensure!(
                    last.closes_loop && last.exit == first.entry && rows.len() % 2 == 0,
                    "loop seam or bond mismatch"
                );
                ensure!(
                    rows.windows(2)
                        .all(|p| p[0].exit == p[1].entry && p[1].index == p[0].index + 1),
                    "loop skipped a course"
                );
                let material = match kind {
                    LowWallKind::Earth => VOXEL_TYPE_DIRT,
                    LowWallKind::Stone => VOXEL_TYPE_ROCK,
                    LowWallKind::Brick => VOXEL_TYPE_TERRACOTTA,
                };
                for row in rows {
                    for slice in wall.pieces(row) {
                        let p = slice.cell.point(0.5, 0.5);
                        let floor = active
                            .ground
                            .query_terrain_ray_cpu(Vec3::new(p.x, 2.5, p.y), Vec3::NEG_Y)
                            .context("wall floor")?
                            .position
                            .y;
                        let cell = (Vec3::new(p.x, floor + (slice.lower + slice.upper) * 0.5, p.y)
                            * 256.)
                            .floor()
                            .as_uvec3();
                        let byte = self
                            .plain_builder
                            .read_chunk_atlas_region(cell, UVec3::ONE)?[0];
                        ensure!(
                            u32::from(byte & crate::builder::VOXEL_TYPE_MASK) == material,
                            "loop missing {kind:?} row={} layer={}",
                            row.index,
                            slice.layer
                        );
                        witness.cells.push((cell.to_array(), byte));
                    }
                }
                ensure!(
                    self.plain_builder
                        .read_chunk_atlas_region(interior, column)?
                        == before,
                    "loop filled or removed interior soil"
                );
                witness.interiors.push((interior.to_array(), before));
                ensure!(
                    self.player_tools.terrain_edit_loop_sound_muted,
                    "closed loop left sound playing"
                );
                log::info!("[PATH_LOOP_REVIEW] kind={kind:?} closed=true exact_seam=true even_bond=true preview_atlas=unchanged interior=unchanged rows={} release_sound=muted",active.continuity.expected());
            }
            std::fs::write(&path, serde_json::to_vec_pretty(&witness)?)?;
        } else {
            for (cell, byte) in &witness.cells {
                ensure!(
                    self.plain_builder
                        .read_chunk_atlas_region(UVec3::from_array(*cell), UVec3::ONE)?[0]
                        == *byte,
                    "loop restart voxel mismatch"
                );
            }
            for (cell, bytes) in &witness.interiors {
                ensure!(
                    self.plain_builder
                        .read_chunk_atlas_region(UVec3::from_array(*cell), column)?
                        == *bytes,
                    "loop restart interior mismatch"
                );
            }
            log::info!(
                "[PATH_LOOP_REVIEW] restart=verified cells={} interiors=unchanged",
                witness.cells.len()
            );
        }
        let target = Vec3::new(1., witness.ground_y + 0.06, 1.4);
        self.camera_control.apply_snapshot_mode(true);
        self.camera_control.set_orbit_focus(target);
        ensure!(
            self.tracer
                .set_camera_pose_looking_at(target + Vec3::new(0.15, 0.95, 1.0), target),
            "loop camera"
        );
        self.reset_camera_movement_input();
        Ok(())
    }
}
