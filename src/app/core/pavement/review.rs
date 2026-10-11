//! Explicit hidden Release fixture. It uses production sampling, stamps, publication,
//! flora removal and snapshot persistence; it is not an OS mouse/visual approval test.
use super::*;
use glam::{UVec3, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Witness {
    cells: Vec<([u32; 3], u8)>,
    flora_count: usize,
}
fn flora_count(flora: &crate::builder::FloraSnapshot) -> Result<usize> {
    let value = serde_json::to_value(flora)?;
    Ok(value["chunks"]
        .as_array()
        .context("flora chunks")?
        .iter()
        .flat_map(|c| c["species"].as_array().unwrap())
        .map(|s| s["instances"].as_array().unwrap().len())
        .sum())
}
fn column_tops(bytes: &[u8], dim: UVec3) -> Vec<Option<(u32, u8)>> {
    let mut columns = Vec::new();
    for z in 0..dim.z {
        for x in 0..dim.x {
            columns.push((0..dim.y).rev().find_map(|y| {
                let material =
                    bytes[((z * dim.y + y) * dim.x + x) as usize] & crate::builder::VOXEL_TYPE_MASK;
                (material != 0).then_some((y, material))
            }));
        }
    }
    columns
}
impl App {
    /// Independent golden quarter-circles: validate every lane's real atlas
    /// material, not just CPU acceptance or a rendering-only outline.
    fn review_pavement_bends(&mut self, cells: &mut Vec<([u32; 3], u8)>) -> Result<()> {
        for (kind, sign, origin) in [
            (PavementKind::Slabs, 1., Vec2::new(0.28, 1.55)),
            (PavementKind::Brick, 1., Vec2::new(0.70, 1.55)),
            (PavementKind::Slabs, -1., Vec2::new(1.12, 1.73)),
            (PavementKind::Brick, -1., Vec2::new(1.54, 1.73)),
        ] {
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            let ground = self.contree_builder.cpu_ray_query_snapshot();
            let recipe = PavementRecipe::new(kind, 0.12, 0.05, 42);
            let points: Vec<_> = (0..=180)
                .map(|i| {
                    let t = std::f32::consts::FRAC_PI_2 * i as f32 / 180.;
                    let p = origin + Vec2::new(0.18 * t.sin(), sign * 0.18 * (1. - t.cos()));
                    ground
                        .query_terrain_ray_cpu(Vec3::new(p.x, 2.5, p.y), Vec3::NEG_Y)
                        .map(|h| h.position)
                        .context("native bend requires ground")
                })
                .collect::<Result<_>>()?;
            let mut expected = PathStroke::new(recipe.path_settings(), points[0], Vec3::X).unwrap();
            for &p in &points[1..] {
                ensure!(expected.push(p), "bend oracle overflow");
            }
            ensure!(expected.finish(), "bend oracle tail overflow");
            let expected: Vec<_> = expected
                .rows
                .iter()
                .flat_map(|r| recipe.pieces(*r))
                .collect();
            ensure!(
                expected.len() == if kind == PavementKind::Slabs { 18 } else { 15 },
                "bend oracle must have six complete courses"
            );
            self.begin_path_on_ground(recipe, points[0], Vec3::X, ground);
            let mut active = self.pavement.active.take().unwrap();
            for &p in &points[1..] {
                ensure!(active.stroke.push(p), "native bend sampling");
                self.publish_pavement_rows(&mut active, ROWS_PER_FRAME)?;
            }
            let active = self.finish_pavement_on_ground(active)?;
            ensure!(
                active.pieces_placed == expected.len(),
                "native bend {kind:?} sign={sign}: {} / {} pieces",
                active.pieces_placed,
                expected.len()
            );
            let material = if kind == PavementKind::Slabs {
                VOXEL_TYPE_ROCK
            } else {
                VOXEL_TYPE_TERRACOTTA
            };
            for (index, piece) in expected.iter().enumerate() {
                for (u, v) in [(0.25, 0.25), (0.5, 0.5), (0.75, 0.75)] {
                    let point = piece.point(u, v);
                    let offset = UVec3::new(
                        (point.x * 256.).floor() as u32,
                        0,
                        (point.y * 256.).floor() as u32,
                    );
                    let dim = UVec3::new(1, self.plain_builder.voxel_dimensions().y, 1);
                    let bytes = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
                    let (y, value) = bytes
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, value)| **value & crate::builder::VOXEL_TYPE_MASK != 0)
                        .context("native bend tile vanished")?;
                    ensure!(
                        u32::from(value & crate::builder::VOXEL_TYPE_MASK) == material,
                        "native bend {kind:?} sign={sign} tile={index} uv={u},{v} material={value}"
                    );
                    cells.push(((offset + UVec3::Y * y as u32).to_array(), *value));
                }
            }
            log::info!("[PAVEMENT_BEND_REVIEW] kind={kind:?} sign={sign} courses=6 tiles={} complete=true atlas_material={material}",expected.len());
            // Same frozen guide must still reject old-world columns on retrace.
            let (foot_min, foot_max) = expected.iter().flat_map(|p| p.footprint).fold(
                (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
                |(min, max), p| (min.min(p), max.max(p)),
            );
            let lo = ((foot_min - Vec2::splat(2. * VOXEL_SIZE)) * 256.)
                .floor()
                .as_uvec2();
            let hi = ((foot_max + Vec2::splat(2. * VOXEL_SIZE)) * 256.)
                .ceil()
                .as_uvec2();
            let min = UVec3::new(lo.x, 0, lo.y);
            let dim = UVec3::new(
                hi.x - lo.x,
                self.plain_builder.voxel_dimensions().y,
                hi.y - lo.y,
            );
            let before = self.plain_builder.read_chunk_atlas_region(min, dim)?;
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            let live = self.contree_builder.cpu_ray_query_snapshot();
            self.begin_path_on_ground(recipe, points[0], Vec3::X, live);
            let mut repeat = self.pavement.active.take().unwrap();
            for &p in &points[1..] {
                ensure!(repeat.stroke.push(p), "native bend retrace sampling");
            }
            let repeat = self.finish_pavement_on_ground(repeat)?;
            ensure!(
                repeat.pieces_placed == 0,
                "native bend retrace must not place anything"
            );
            let after = self.plain_builder.read_chunk_atlas_region(min, dim)?;
            ensure!(before == after, "native bent retrace changed atlas");
            log::info!("[PAVEMENT_BEND_REVIEW] kind={kind:?} sign={sign} retrace=atlas_identical");
        }
        Ok(())
    }
    pub(in crate::app::core) fn review_pavement(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_PAVEMENT_REVIEW") else {
            return Ok(());
        };
        if mode == "pointer" || mode == "turn" || mode == "recover" {
            return self.begin_pavement_pointer_review();
        }
        let path = std::path::Path::new("target/pavement/witness.json");
        if mode == "verify" || mode == "view" {
            ensure!(
                self.terrain_persistence.startup_load_requested(),
                "pavement verify needs --terrain-load"
            );
            let witness: Witness = serde_json::from_slice(&std::fs::read(path)?)?;
            for (cell, expected) in &witness.cells {
                let bytes = self
                    .plain_builder
                    .read_chunk_atlas_region(UVec3::from_array(*cell), UVec3::ONE)?;
                ensure!(
                    bytes[0] == *expected,
                    "pavement restart cell {cell:?}: {} != {expected}",
                    bytes[0]
                );
            }
            let count = flora_count(&self.surface_builder.capture_flora_snapshot(
                self.time_info.time_since_start_duration().as_millis() as u32,
            )?)?;
            ensure!(
                count == witness.flora_count,
                "pavement flora removal did not persist"
            );
            log::info!(
                "[PAVEMENT_REVIEW] restart=verified cells={} flora_count={count}",
                witness.cells.len()
            );
            if mode == "view" {
                let hit = self
                    .query_terrain_ray_cpu(Vec3::new(1., 2.5, 1.65), Vec3::NEG_Y)
                    .context("pavement bend review camera requires ground")?;
                let target = Vec3::new(1., hit.position.y + 0.015, 1.65);
                self.camera_control.apply_snapshot_mode(true);
                self.camera_control.set_orbit_focus(target);
                ensure!(
                    self.tracer
                        .set_camera_pose_looking_at(target + Vec3::new(0., 0.78, 0.46), target),
                    "pavement bend review camera"
                );
                self.reset_camera_movement_input();
                log::info!("[PAVEMENT_REVIEW] view=bends camera_transient=true target={target:?}");
            }
        } else {
            ensure!(
                mode == "seed",
                "RE_FLORA_PAVEMENT_REVIEW expects seed, pointer, turn, recover, verify or view (--terrain-load)"
            );
            let mut cells = Vec::new();
            for (kind, points) in [
                (
                    PavementKind::Slabs,
                    vec![
                        Vec2::new(0.35, 0.45),
                        Vec2::new(0.75, 0.45),
                        Vec2::new(0.9, 0.51),
                        Vec2::new(0.98, 0.66),
                        Vec2::new(0.98, 0.95),
                    ],
                ),
                (
                    PavementKind::Brick,
                    vec![
                        Vec2::new(1.3, 0.45),
                        Vec2::new(1.3, 0.95),
                        Vec2::new(1.5, 1.15),
                    ],
                ),
                (
                    PavementKind::Dirt,
                    vec![
                        Vec2::new(0.35, 1.),
                        Vec2::new(0.65, 1.25),
                        Vec2::new(0.82, 1.25),
                    ],
                ),
            ] {
                self.contree_builder.flush_cpu_chunk_cache_jobs();
                let ground = self.contree_builder.cpu_ray_query_snapshot();
                let points: Vec<_> = points
                    .iter()
                    .map(|p| {
                        ground
                            .query_terrain_ray_cpu(Vec3::new(p.x, 2.5, p.y), Vec3::NEG_Y)
                            .map(|h| h.position)
                            .context("pavement fixture requires soil")
                    })
                    .collect::<Result<_>>()?;
                if kind == PavementKind::Dirt {
                    // Don't rely on random startup vegetation for this assertion.
                    let column = glam::UVec2::new(
                        (points[0].x * 256.).floor() as u32,
                        (points[0].z * 256.).floor() as u32,
                    );
                    let anchor = self
                        .resolve_plantable_surface_column(column)
                        .map_err(|e| anyhow::anyhow!("dirt fixture anchor: {e:?}"))?;
                    let mut batch = super::super::planting::AuthoredFloraPlacementBatch::new();
                    ensure!(
                        self.try_place_authored_flora(
                            &mut batch,
                            crate::flora::MODEL_FLOWER_FIRST_SPECIES,
                            anchor,
                            255,
                            0,
                            42
                        ),
                        "dirt fixture plant failed"
                    );
                    self.finish_authored_flora_placement(batch)?;
                }
                let flora_before = flora_count(&self.surface_builder.capture_flora_snapshot(0)?)?;
                // Read a local atlas block around the dirt trail: surface treatment
                // must not raise terrain or stamp an isotropic pile of soil disks.
                let dirt_min = UVec3::new(64, 0, 230);
                let dirt_dim = UVec3::new(166, 256, 116);
                let dirt_before = if kind == PavementKind::Dirt {
                    self.plain_builder
                        .read_chunk_atlas_region(dirt_min, dirt_dim)?
                } else {
                    Vec::new()
                };
                let previous_loop = self.player_tools.terrain_edit_loop_sound;
                self.begin_path_on_ground(
                    PavementRecipe::new(kind, 0.12, 0.05, 42),
                    points[0],
                    Vec3::Z,
                    ground,
                );
                let loop_id = self
                    .player_tools
                    .terrain_edit_loop_sound
                    .context("pavement must start Dig's shared loop")?;
                ensure!(
                    !self.player_tools.terrain_edit_loop_sound_muted,
                    "pavement loop stayed silent"
                );
                ensure!(
                    previous_loop.is_none_or(|id| id == loop_id),
                    "pavement duplicated Dig's emitter"
                );
                let mut active = self.pavement.active.take().context("fixture stroke")?;
                let start = Instant::now();
                for point in &points[1..] {
                    ensure!(active.stroke.push(*point), "fixture sampling rejected");
                    // Same bounded publication unit used by redraws.
                    self.publish_pavement_rows(&mut active, ROWS_PER_FRAME)?;
                }
                let active = self.finish_pavement_on_ground(active)?;
                ensure!(
                    self.player_tools.terrain_edit_loop_sound_muted,
                    "release left pavement audible"
                );
                let after = flora_count(&self.surface_builder.capture_flora_snapshot(0)?)?;
                if kind == PavementKind::Dirt {
                    ensure!(
                        self.plain_builder
                            .read_chunk_atlas_region(dirt_min, dirt_dim)?
                            == dirt_before,
                        "dirt path modified terrain height/material"
                    );
                    ensure!(
                        after < flora_before,
                        "dirt path did not clear any vegetation"
                    );
                    log::info!(
                        "[PAVEMENT_REVIEW] kind=Dirt atlas_unchanged=true flora_removed={}",
                        flora_before - after
                    );
                } else {
                    ensure!(
                        active.pieces_placed > 12,
                        "too few fixture pavement pieces: {}",
                        active.pieces_placed
                    );
                    let expected = if kind == PavementKind::Brick {
                        VOXEL_TYPE_TERRACOTTA
                    } else {
                        VOXEL_TYPE_ROCK
                    };
                    for piece in active.coverage.published_pieces() {
                        let cell = ((piece.center + Vec3::Y * piece.size.y * 0.5) * 256.)
                            .floor()
                            .as_uvec3();
                        let raw = self
                            .plain_builder
                            .read_chunk_atlas_region(cell, UVec3::ONE)?;
                        ensure!(
                            u32::from(raw[0]) & crate::builder::VOXEL_TYPE_MASK as u32 == expected,
                            "pavement interior material absent at {cell:?}"
                        );
                        cells.push((cell.to_array(), raw[0]));
                    }
                    log::info!("[PAVEMENT_REVIEW] kind={kind:?} pieces={} interior_material={expected} elapsed_ms={:.2}", active.pieces_placed, start.elapsed().as_secs_f64() * 1000.);
                    // Repeat on a fresh snapshot. Conservative stroke-local exclusion
                    // can leave a small turn gap that a later gesture may fill; that
                    // is not stacking. Compare the actual top of EVERY previously
                    // paved voxel column, not just the number of new pieces.
                    let min = points
                        .iter()
                        .fold(Vec3::splat(f32::INFINITY), |a, p| a.min(*p))
                        - Vec3::splat(0.12);
                    let max = points
                        .iter()
                        .fold(Vec3::splat(f32::NEG_INFINITY), |a, p| a.max(*p))
                        + Vec3::splat(0.12);
                    let offset = UVec3::new(
                        (min.x * 256.).floor() as u32,
                        0,
                        (min.z * 256.).floor() as u32,
                    );
                    let dim = UVec3::new(
                        (max.x * 256.).ceil() as u32 - offset.x,
                        self.world_chunk_dim.y * 256,
                        (max.z * 256.).ceil() as u32 - offset.z,
                    );
                    let before = column_tops(
                        &self.plain_builder.read_chunk_atlas_region(offset, dim)?,
                        dim,
                    );
                    self.contree_builder.flush_cpu_chunk_cache_jobs();
                    self.begin_path_on_ground(
                        active.recipe,
                        points[0],
                        Vec3::Z,
                        self.contree_builder.cpu_ray_query_snapshot(),
                    );
                    let mut repeat = self.pavement.active.take().unwrap();
                    for point in &points[1..] {
                        ensure!(repeat.stroke.push(*point), "repeat sampling");
                    }
                    let repeat = self.finish_pavement_on_ground(repeat)?;
                    ensure!(
                        self.player_tools.terrain_edit_loop_sound_muted,
                        "repeat release left pavement audible"
                    );
                    let after = column_tops(
                        &self.plain_builder.read_chunk_atlas_region(offset, dim)?,
                        dim,
                    );
                    let mut checked = 0;
                    for (a, b) in before.iter().zip(&after) {
                        if a.is_some_and(|(_, material)| u32::from(material) == expected) {
                            ensure!(a == b, "retracing raised or changed an existing paved column: {a:?} -> {b:?}");
                            checked += 1;
                        }
                    }
                    ensure!(checked > 100, "no existing paving columns checked");
                    log::info!("[PAVEMENT_REVIEW] kind={kind:?} retrace=no_extra_layer checked_columns={checked} gap_fill_pieces={}", repeat.pieces_placed);
                }
                // All GUI/camera/focus/ground-loss exits use this cancellation owner.
                self.begin_path_on_ground(active.recipe, points[0], Vec3::Z, active.ground.clone());
                ensure!(
                    self.player_tools.terrain_edit_loop_sound == Some(loop_id)
                        && !self.player_tools.terrain_edit_loop_sound_muted,
                    "pavement did not resume Dig's cached loop"
                );
                self.cancel_pavement_stroke();
                ensure!(
                    self.pavement.active.is_none()
                        && self.player_tools.terrain_edit_loop_sound_muted,
                    "cancel left pavement audible"
                );
                // An inactive pavement cancellation must not interrupt Dig itself.
                self.start_terrain_edit_loop_sound(points[0]);
                self.cancel_pavement_stroke();
                ensure!(
                    !self.player_tools.terrain_edit_loop_sound_muted,
                    "idle pavement muted another tool"
                );
                self.stop_terrain_edit_loop_sound();
                log::info!("[PAVEMENT_REVIEW] kind={kind:?} sound=dig_shared release=muted cancel=muted idle_cancel=preserved");
            }
            std::fs::create_dir_all(path.parent().unwrap())?;
            self.review_pavement_bends(&mut cells)?;
            let flora_count = flora_count(&self.surface_builder.capture_flora_snapshot(0)?)?;
            std::fs::write(path, serde_json::to_vec(&Witness { cells, flora_count })?)?;
            log::info!(
                "[PAVEMENT_REVIEW] seed=verified three_surfaces=true witness={}",
                path.display()
            );
        }
        // Inspection only, not saved. Normal launches keep the authored camera.
        if mode != "view" {
            self.tracer.set_camera_pose_looking_at(
                Vec3::new(0.94, 1.42, 1.75),
                Vec3::new(0.94, 0.64, 0.8),
            );
            self.sync_orbit_focus_from_current_view();
        }
        self.select_item_panel_slot(super::super::ui_style::PAVEMENT_SLOT_INDEX);
        Ok(())
    }
}
