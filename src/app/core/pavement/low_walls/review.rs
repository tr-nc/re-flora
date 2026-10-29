//! Native fixtures through current candidate validation and atlas publication.
//! Not OS-mouse, audible or performance acceptance.
use super::*;
use glam::Vec2;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
struct Witness {
    cells: Vec<([u32; 3], u8)>,
    dig_targets: Vec<([f32; 2], u32)>,
    flora_count: usize,
    ground_y: f32,
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
// Compare voxel columns to the actual sampled support mesh, at the VOXEL
// column centre. A corner-sampled sloping cap is not an unrelated centre-ray
// height, and voxel shell/rounding must not be mistaken for changing the recipe.
fn projected_span(model: &GroundedPiece, point: Vec2, layer: Option<usize>) -> Option<(f32, f32)> {
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for (i, t) in model.triangles.iter().enumerate() {
        if layer.is_some_and(|l| i / 12 != l) {
            continue;
        }
        let a = Vec3::new(t.a[0], t.a[1], t.a[2]) + model.base;
        let b = Vec3::new(t.b[0], t.b[1], t.b[2]) + model.base;
        let c = Vec3::new(t.c[0], t.c[1], t.c[2]) + model.base;
        let xz = |p: Vec3| Vec2::new(p.x, p.z);
        let u = xz(b - a);
        let v = xz(c - a);
        let d = point - xz(a);
        let det = u.perp_dot(v);
        if det.abs() < 1e-9 {
            continue;
        }
        let bu = d.perp_dot(v) / det;
        let bv = u.perp_dot(d) / det;
        if bu >= -1e-5 && bv >= -1e-5 && bu + bv <= 1. + 1e-5 {
            let y = a.y + bu * (b.y - a.y) + bv * (c.y - a.y);
            low = low.min(y);
            high = high.max(y);
        }
    }
    (low.is_finite() && high.is_finite()).then_some((low, high))
}
impl App {
    fn publish_review_wall_rows(
        &mut self,
        active: &mut ActivePavement,
        rows: &mut Vec<PathSample>,
    ) -> Result<()> {
        ensure!(active.stroke.prepare(), "wall streaming sampling");
        let queued = active.stroke.rows.clone();
        self.publish_pavement_rows(active, ROWS_PER_FRAME)?;
        rows.extend(
            queued
                .into_iter()
                .filter(|row| row.index < active.continuity.expected()),
        );
        Ok(())
    }

    pub(in crate::app::core) fn review_low_walls(&mut self) -> Result<()> {
        let Ok(mode) = std::env::var("RE_FLORA_LOW_WALL_REVIEW") else {
            return Ok(());
        };
        if mode == "loops" || mode == "loops-verify" {
            return self.review_path_loops(mode == "loops-verify");
        }
        let path = std::path::Path::new("target/low-walls/witness.json");
        if mode == "view" || mode == "verify" || mode == "dig" {
            ensure!(
                self.terrain_persistence.startup_load_requested(),
                "wall verify needs --terrain-load"
            );
            let witness: Witness = serde_json::from_slice(&std::fs::read(path)?)?;
            for (cell, expected) in &witness.cells {
                let bytes = self
                    .plain_builder
                    .read_chunk_atlas_region(UVec3::from_array(*cell), UVec3::ONE)?;
                ensure!(
                    bytes[0] == *expected,
                    "wall restart cell {cell:?}: {} != {expected}",
                    bytes[0]
                );
            }
            let count = flora_count(&self.surface_builder.capture_flora_snapshot(0)?)?;
            ensure!(
                count == witness.flora_count,
                "wall fixture flora did not persist"
            );
            log::info!(
                "[LOW_WALL_REVIEW] restart=verified cells={} flora_count={count}",
                witness.cells.len()
            );
            if mode == "dig" {
                for (xz, material) in witness.dig_targets {
                    let hit = self
                        .query_terrain_ray_cpu(Vec3::new(xz[0], 2.5, xz[1]), Vec3::NEG_Y)
                        .context("wall Dig requires restored top")?;
                    ensure!(hit.voxel_type == material, "restored wall top material");
                    let readback = self.apply_surface_terrain_removal(
                        TerrainBrushEdit {
                            start: hit.position,
                            end: hit.position,
                            radius: 0.025,
                        },
                        Some(material),
                        None,
                        None,
                    )?;
                    let removed = readback.stats.count_removed(material);
                    ensure!(removed > 0, "Dig did not edit wall material {material}");
                    log::info!("[LOW_WALL_REVIEW] material={material} dig_removed={removed}");
                }
                log::info!(
                    "[LOW_WALL_REVIEW] dig=verified three_materials=true snapshot_file=unchanged"
                );
            } else if mode == "view" {
                let target = Vec3::new(0.90, witness.ground_y + 0.08, 1.58);
                self.camera_control.apply_snapshot_mode(true);
                self.camera_control.set_orbit_focus(target);
                ensure!(
                    self.tracer
                        .set_camera_pose_looking_at(target + Vec3::new(0.70, 0.78, 0.80), target),
                    "wall review camera"
                );
                self.reset_camera_movement_input();
                log::info!("[LOW_WALL_REVIEW] view=candidates camera_transient=true");
            }
            return Ok(());
        }
        ensure!(
            mode == "seed",
            "RE_FLORA_LOW_WALL_REVIEW expects seed, verify, view, dig, loops or loops-verify"
        );
        self.review_low_wall_guards()?;
        let original = (
            self.player_tools.selected_item_panel_display_slot(),
            self.debug_settings.adjustables.low_wall_kind.value,
            self.debug_settings.adjustables.low_wall_width.value,
            self.debug_settings.adjustables.low_wall_height.value,
            self.debug_settings.adjustables.pavement_piece_length.value,
            self.debug_settings.adjustables.pavement_seed.value,
        );
        self.player_tools.clear_selected_tool();
        self.player_tools
            .select_item_panel_slot(super::super::super::ui_style::PAVEMENT_SLOT_INDEX);
        ensure!(
            matches!(self.construction_recipe(), ConstructionRecipe::Pavement(_)),
            "wall mode OFF did not keep pavement"
        );
        let mut cells = Vec::new();
        let mut dig_targets = Vec::new();
        let mut ground_y = 0.;
        // Measured initial false rejection: natural rock well BELOW the wall
        // body, not an authored block in its mesh + shell. Must stay unchanged.
        let bedrock = UVec3::new(85, 69, 305);
        let bedrock_before = self
            .plain_builder
            .read_chunk_atlas_region(bedrock, UVec3::ONE)?[0];
        ensure!(
            u32::from(bedrock_before & crate::builder::VOXEL_TYPE_MASK) == VOXEL_TYPE_ROCK,
            "wall fixture requires measured deep rock"
        );
        for (row_index, height) in [0.08, 0.16].into_iter().enumerate() {
            for (column, kind) in [LowWallKind::Earth, LowWallKind::Stone, LowWallKind::Brick]
                .into_iter()
                .enumerate()
            {
                let origin = Vec2::new(0.25 + column as f32 * 0.5, 1.20 + row_index as f32 * 0.45);
                self.contree_builder.flush_cpu_chunk_cache_jobs();
                let ground = self.contree_builder.cpu_ray_query_snapshot();
                let points: Vec<_> = (0..=90)
                    .map(|i| {
                        let t = std::f32::consts::FRAC_PI_2 * i as f32 / 90.;
                        let p = origin + Vec2::new(0.18 * t.sin(), 0.18 * (1. - t.cos()));
                        ground
                            .query_terrain_ray_cpu(Vec3::new(p.x, 2.5, p.y), Vec3::NEG_Y)
                            .filter(|h| is_ground(h.voxel_type))
                            .map(|h| h.position)
                            .context("wall fixture needs ground")
                    })
                    .collect::<Result<_>>()?;
                ground_y = points[0].y;
                let interior = origin + Vec2::new(0.05, 0.12);
                let interior_offset = UVec3::new(
                    (interior.x * 256.).floor() as u32,
                    0,
                    (interior.y * 256.).floor() as u32,
                );
                let column_dim = UVec3::new(1, self.world_chunk_dim.y * 256, 1);
                let interior_before = self
                    .plain_builder
                    .read_chunk_atlas_region(interior_offset, column_dim)?;
                if self.player_tools.selected_tool()
                    != super::super::super::player_tools::PlayerTool::Wall
                {
                    self.player_tools
                        .select_item_panel_slot(super::super::super::ui_style::WALL_SLOT_INDEX);
                }
                let s = &mut self.debug_settings.adjustables;
                s.low_wall_kind.value = column as u32;
                s.low_wall_width.value = 0.06;
                s.low_wall_height.value = height;
                s.pavement_piece_length.value = 0.05;
                s.pavement_seed.value = 42;
                let recipe = match self.construction_recipe() {
                    ConstructionRecipe::LowWall(recipe) => recipe,
                    _ => anyhow::bail!("wall mode ON did not route to walls"),
                };
                ensure!(
                    recipe.kind == kind && recipe.height == height,
                    "wall saved-field recipe mismatch"
                );
                let mut expected =
                    PathStroke::new(recipe.path_settings(), points[0], Vec3::X).unwrap();
                for &p in &points[1..] {
                    ensure!(expected.push(p), "wall expected input");
                }
                ensure!(expected.finish(), "wall expected tail");
                let expected = expected.rows.len();
                let prior_sound = self.player_tools.terrain_edit_loop_sound;
                self.begin_path_on_ground(self.construction_recipe(), points[0], Vec3::X, ground);
                let loop_id = self
                    .player_tools
                    .terrain_edit_loop_sound
                    .context("wall Dig loop")?;
                ensure!(
                    prior_sound.is_none_or(|id| id == loop_id)
                        && !self.player_tools.terrain_edit_loop_sound_muted,
                    "wall did not reuse Dig"
                );
                let mut active = self.pavement.active.take().context("wall fixture")?;
                self.debug_settings.adjustables.low_wall_height.value = 0.3;
                let mut wall_rows = Vec::new();
                for &p in &points[1..] {
                    active.observe_editable_ground(p);
                    self.validate_pavement_candidate(&mut active, false)?;
                    ensure!(
                        active.continuity.blocked().is_none(),
                        "wall candidate rejected: {:?}",
                        active.continuity.blocked().map(|b| b.reason)
                    );
                    self.publish_review_wall_rows(&mut active, &mut wall_rows)?;
                }
                ensure!(
                    !wall_rows.is_empty(),
                    "wall was deferred entirely to release"
                );
                self.validate_pavement_candidate(&mut active, true)?;
                let mut tail = active.stroke.clone();
                ensure!(tail.finish(), "wall release sampling");
                let mut active = self.finish_pavement_on_ground(active)?;
                wall_rows.extend(
                    tail.rows
                        .into_iter()
                        .filter(|row| row.index < active.continuity.expected()),
                );
                ensure!(
                    active.continuity.blocked().is_none(),
                    "wall release rejected"
                );
                ensure!(
                    wall_rows
                        .windows(2)
                        .all(|pair| pair[0].exit == pair[1].entry
                            && pair[1].index == pair[0].index + 1),
                    "wall column continuity"
                );
                ensure!(
                    wall_rows
                        .last()
                        .context("wall endpoint")?
                        .exit
                        .center
                        .distance(*points.last().unwrap())
                        < recipe.pitch + 2. * VOXEL_SIZE,
                    "wall did not reach released endpoint"
                );
                ensure!(
                    self.player_tools.terrain_edit_loop_sound_muted,
                    "wall release left shared sound active"
                );
                ensure!(
                    wall_rows.len() == expected,
                    "{kind:?} height={height} incomplete columns {} != {expected}",
                    wall_rows.len()
                );
                ensure!(
                    active.pieces_placed == expected * recipe.layers(),
                    "wall layer count incomplete"
                );
                let material = match kind {
                    LowWallKind::Earth => VOXEL_TYPE_DIRT,
                    LowWallKind::Stone => VOXEL_TYPE_ROCK,
                    LowWallKind::Brick => VOXEL_TYPE_TERRACOTTA,
                };
                let mut models = Vec::new();
                for &row in &wall_rows {
                    models.push(
                        ground_wall_cell(recipe, row, self.world_chunk_dim.as_vec3(), |p, d| {
                            active.ground.query_terrain_ray_cpu(p, d)
                        })?
                        .context("wall expected model")?,
                    );
                }
                for (&row, model) in wall_rows.iter().zip(&models) {
                    for slice in recipe.pieces(row) {
                        let p = slice.cell.point(0.5, 0.5);
                        let hit = active
                            .ground
                            .query_terrain_ray_cpu(Vec3::new(p.x, 2.5, p.y), Vec3::NEG_Y)
                            .unwrap();
                        let cell = (Vec3::new(
                            p.x,
                            hit.position.y + (slice.lower + slice.upper) * 0.5,
                            p.y,
                        ) * 256.)
                            .floor()
                            .as_uvec3();
                        let raw = self
                            .plain_builder
                            .read_chunk_atlas_region(cell, UVec3::ONE)?[0];
                        ensure!(
                            u32::from(raw & crate::builder::VOXEL_TYPE_MASK) == material,
                            "wall interior missing {kind:?} layer={} row={} cell={cell:?}",
                            slice.layer,
                            row.index
                        );
                        cells.push((cell.to_array(), raw));
                    }
                    let p = recipe.raw_cell(row).point(0.5, 0.5);
                    // Quantized column centre, not the arbitrary analytic p.
                    let column_point =
                        (p * 256.).floor() * VOXEL_SIZE + Vec2::splat(VOXEL_SIZE * 0.5);
                    let target = projected_span(model, column_point, None)
                        .context("wall cap projection")?
                        .1;
                    let offset =
                        UVec3::new((p.x * 256.).floor() as u32, 0, (p.y * 256.).floor() as u32);
                    let bytes = self
                        .plain_builder
                        .read_chunk_atlas_region(offset, column_dim)?;
                    let highest = bytes
                        .iter()
                        .rposition(|b| u32::from(*b & crate::builder::VOXEL_TYPE_MASK) == material)
                        .context("wall top")?;
                    let actual = (highest + 1) as f32 * VOXEL_SIZE;
                    ensure!(
                        (actual - target).abs() <= 2. * VOXEL_SIZE,
                        "wall voxel roof {actual} != sampled frozen-height mesh {target}"
                    );
                    cells.push((
                        UVec3::new(offset.x, highest as u32, offset.z).to_array(),
                        bytes[highest],
                    ));
                    if row.index == 2 && row_index == 1 {
                        dig_targets.push(([p.x, p.y], material));
                    }
                }
                if kind != LowWallKind::Earth {
                    let mut checked = 0;
                    for i in 0..wall_rows.len() - 1 {
                        let a = wall_rows[i];
                        let p = Vec2::new(a.exit.center.x, a.exit.center.z);
                        let column = (p * 256.).floor().as_uvec2();
                        let point = (column.as_vec2() + Vec2::splat(0.5)) * VOXEL_SIZE;
                        for layer in 0..recipe.layers() {
                            if !(a.index as usize + layer).is_multiple_of(2) {
                                continue;
                            }
                            let span = projected_span(&models[i], point, Some(layer))
                                .or_else(|| projected_span(&models[i + 1], point, Some(layer)))
                                .context("internal half-brick span")?;
                            let cell = UVec3::new(
                                column.x,
                                (((span.0 + span.1) * 0.5) * 256.).floor() as u32,
                                column.y,
                            );
                            let raw = self
                                .plain_builder
                                .read_chunk_atlas_region(cell, UVec3::ONE)?[0];
                            ensure!(
                                u32::from(raw & crate::builder::VOXEL_TYPE_MASK) == material,
                                "false internal half-brick seam at {cell:?}, layer={layer}"
                            );
                            cells.push((cell.to_array(), raw));
                            checked += 1;
                        }
                    }
                    log::info!("[LOW_WALL_REVIEW] kind={kind:?} height={height} internal_half_brick_seams=none checked={checked}");
                }
                ensure!(
                    self.plain_builder
                        .read_chunk_atlas_region(interior_offset, column_dim)?
                        == interior_before,
                    "wall unexpectedly filled inside the garden"
                );
                let region = UVec3::new(
                    ((origin.x - 0.08) * 256.).floor() as u32,
                    0,
                    ((origin.y - 0.08) * 256.).floor() as u32,
                );
                let dim = UVec3::new(90, self.world_chunk_dim.y * 256, 90);
                let before = self.plain_builder.read_chunk_atlas_region(region, dim)?;
                // Exact emitted-cell replay with the SAME frozen substrate/ownership.
                // This tests soil self-coverage, not a fresh top-material rejection.
                let placed = active.pieces_placed;
                active.stroke.rows.extend(wall_rows.iter().copied());
                self.publish_pavement_rows(&mut active, usize::MAX)?;
                ensure!(
                    active.pieces_placed == placed
                        && self.plain_builder.read_chunk_atlas_region(region, dim)? == before,
                    "same-gesture wall replay stacked"
                );
                log::info!("[LOW_WALL_REVIEW] kind={kind:?} height={height} columns={expected}/{expected} layers={} solids={} complete=true frozen_height=true same_gesture_retrace=atlas_identical inside_fill=false", recipe.layers(), active.pieces_placed);
                if kind != LowWallKind::Earth {
                    self.contree_builder.flush_cpu_chunk_cache_jobs();
                    self.begin_path_on_ground(
                        recipe,
                        points[0],
                        Vec3::X,
                        self.contree_builder.cpu_ray_query_snapshot(),
                    );
                    let mut repeat = self.pavement.active.take().unwrap();
                    for &p in &points[1..] {
                        ensure!(repeat.stroke.push(p), "wall retrace input");
                    }
                    let repeat = self.finish_pavement_on_ground(repeat)?;
                    ensure!(
                        repeat.pieces_placed == 0
                            && self.plain_builder.read_chunk_atlas_region(region, dim)? == before,
                        "fresh masonry retrace changed atlas"
                    );
                    log::info!("[LOW_WALL_REVIEW] kind={kind:?} height={height} fresh_retrace=atlas_identical");
                }
                self.begin_path_on_ground(recipe, points[0], Vec3::X, active.ground.clone());
                self.cancel_pavement_stroke();
                ensure!(
                    self.player_tools.terrain_edit_loop_sound_muted,
                    "wall cancel left Dig audible"
                );
                self.start_terrain_edit_loop_sound(points[0]);
                self.cancel_pavement_stroke();
                ensure!(
                    !self.player_tools.terrain_edit_loop_sound_muted,
                    "inactive wall cancel silenced Dig"
                );
                self.stop_terrain_edit_loop_sound();
                log::info!("[LOW_WALL_REVIEW] kind={kind:?} height={height} sound=dig_shared release=muted cancel=muted idle_cancel=preserved");
                // Fixture-only dressing, never automatic inside fill/planting.
                let anchor = self
                    .resolve_plantable_surface_column(glam::UVec2::new(
                        interior_offset.x,
                        interior_offset.z,
                    ))
                    .map_err(|e| anyhow::anyhow!("wall fixture flower: {e:?}"))?;
                let mut batch = crate::app::core::planting::AuthoredFloraPlacementBatch::new();
                ensure!(
                    self.try_place_authored_flora(
                        &mut batch,
                        crate::flora::MODEL_FLOWER_FIRST_SPECIES,
                        anchor,
                        255,
                        0,
                        42 + column as u32
                    ),
                    "wall fixture flower failed"
                );
                self.finish_authored_flora_placement(batch)?;
            }
        }
        ensure!(
            self.plain_builder
                .read_chunk_atlas_region(bedrock, UVec3::ONE)?[0]
                == bedrock_before,
            "wall changed deep bedrock"
        );
        cells.push((bedrock.to_array(), bedrock_before));
        log::info!("[LOW_WALL_REVIEW] deep_bedrock=byte_identical wall_guard=full_mesh_plus_shell");
        let count = flora_count(&self.surface_builder.capture_flora_snapshot(0)?)?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(
            path,
            serde_json::to_vec(&Witness {
                cells,
                dig_targets,
                flora_count: count,
                ground_y,
            })?,
        )?;
        self.player_tools.clear_selected_tool();
        if let Some(slot) = original.0 {
            self.player_tools.select_item_panel_slot(slot);
        }
        let s = &mut self.debug_settings.adjustables;
        s.low_wall_kind.value = original.1;
        s.low_wall_width.value = original.2;
        s.low_wall_height.value = original.3;
        s.pavement_piece_length.value = original.4;
        s.pavement_seed.value = original.5;
        log::info!("[LOW_WALL_REVIEW] seed=verified three_materials=true two_heights=true saved_recipe_route=true flora_count={count} preferences_restored=true");
        Ok(())
    }
}
