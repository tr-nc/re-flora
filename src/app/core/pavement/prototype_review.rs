//! Opt-in release app review: actual atlas, flora, preflight and visible terrain
//! publication. Normal unit tests do not create a GPU, window or audio device.
use super::*;
use glam::Vec2;

#[derive(serde::Serialize, serde::Deserialize)]
struct Witness {
    offset: [u32; 3],
    dim: [u32; 3],
    crc: u32,
    flora_count: usize,
    authored: Vec<String>,
}
const WITNESS: &str = "target/pavement-prototype-native/witness.json";
fn authored_witness(snapshot: &crate::builder::FloraSnapshot) -> Result<Vec<String>> {
    let value = serde_json::to_value(snapshot)?;
    let mut roots = Vec::new();
    for chunk in value["chunks"].as_array().context("flora chunks")? {
        for species in chunk["species"].as_array().context("flora species")? {
            for root in species["authored"].as_array().context("authored roots")? {
                roots.push(format!(
                    "{}:{}:{}",
                    chunk["coordinate"], species["key"], root
                ));
            }
        }
    }
    roots.sort();
    Ok(roots)
}
fn flora_count(snapshot: &crate::builder::FloraSnapshot) -> Result<usize> {
    let value = serde_json::to_value(snapshot)?;
    Ok(value["chunks"]
        .as_array()
        .context("flora chunks")?
        .iter()
        .flat_map(|c| c["species"].as_array().unwrap())
        .map(|s| s["instances"].as_array().unwrap().len())
        .sum())
}
impl App {
    pub(super) fn verify_pavement_prototype(&mut self) -> Result<()> {
        let witness: Witness = serde_json::from_slice(&std::fs::read(WITNESS)?)?;
        let bytes = self.plain_builder.read_chunk_atlas_region(
            UVec3::from_array(witness.offset),
            UVec3::from_array(witness.dim),
        )?;
        ensure!(
            crc32fast::hash(&bytes) == witness.crc,
            "prototype terrain save/load mismatch"
        );
        let flora = self.surface_builder.capture_flora_snapshot(0)?;
        ensure!(
            flora_count(&flora)? == witness.flora_count
                && authored_witness(&flora)? == witness.authored,
            "prototype flora save/load mismatch"
        );
        log::info!("[PAVEMENT_PROTOTYPE_REVIEW] restart=verified atlas_crc={} flora_count={} authored_identity_and_seed=true",witness.crc,witness.flora_count);
        Ok(())
    }
    fn prototype_target(&mut self, a: &mut ActivePavement, point: Vec3) -> Result<()> {
        a.observe_editable_ground(point);
        self.validate_pavement_candidate(a, false)?;
        ensure!(
            a.continuity.blocked().is_none(),
            "prototype native target {point:?} rejected: {:?}",
            a.continuity.blocked().map(|b| b.reason)
        );
        self.publish_pavement_rows(a, ROWS_PER_FRAME)
    }
    pub(super) fn review_pavement_prototype(&mut self) -> Result<()> {
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        let ground = self.contree_builder.cpu_ray_query_snapshot();
        let sample = |p: Vec2| {
            ground
                .query_terrain_ray_cpu(Vec3::new(p.x, 2.5, p.y), Vec3::NEG_Y)
                .map(|h| h.position)
                .context("prototype review soil")
        };
        // A completed path is part of the before-image of BOTH later gestures.
        let old_start = sample(Vec2::new(0.3, 1.6))?;
        let old_recipe = PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 7);
        self.begin_path_on_ground(old_recipe, old_start, Vec3::X, ground.clone());
        let mut old = self
            .pavement
            .active
            .take()
            .context("completed path fixture")?;
        self.prototype_target(&mut old, old_start + Vec3::X * 0.5)?;
        let old = self.finish_pavement_on_ground(old)?;
        ensure!(old.pieces_placed > 0, "completed path must exist");
        for (kind, z) in [(PavementKind::Slabs, 0.4), (PavementKind::Brick, 1.2)] {
            let start = sample(Vec2::new(0.3, z))?;
            let plant_column = glam::UVec2::new((0.7 * 256.) as u32, (z * 256.) as u32);
            let anchor = self
                .resolve_plantable_surface_column(plant_column)
                .map_err(|e| anyhow::anyhow!("prototype plant: {e:?}"))?;
            let mut batch = super::super::planting::AuthoredFloraPlacementBatch::new();
            ensure!(
                self.try_place_authored_flora(
                    &mut batch,
                    crate::flora::MODEL_FLOWER_FIRST_SPECIES,
                    anchor,
                    255,
                    0,
                    123
                ),
                "prototype restore witness plant"
            );
            self.finish_authored_flora_placement(batch)?;
            let flora_before = self.surface_builder.capture_flora_snapshot(0)?;
            let floor = (start.y * 256.).floor() as u32;
            let offset = UVec3::new(32, floor.saturating_sub(16), 32);
            let dim = UVec3::new(448, 32, 448);
            let before = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
            let recipe = PavementRecipe::new(kind, 0.12, 0.05, 42);
            self.begin_path_on_ground(recipe, start, Vec3::X, ground.clone());
            let mut a = self.pavement.active.take().context("retract gesture")?;
            for i in 1..=45 {
                self.prototype_target(&mut a, start + Vec3::X * (i as f32 * 0.015))?;
            }
            let placed = a.pieces_placed;
            ensure!(
                placed > 0,
                "streaming stones must be published before withdrawal"
            );
            ensure!(
                flora_count(&self.surface_builder.capture_flora_snapshot(0)?)?
                    < flora_count(&flora_before)?,
                "live paving must remove witness flora"
            );
            // A rejected state must not latch out the same backdrag operation.
            a.block_here(BlockReason::Ground);
            let requested = start + Vec3::new(0.3, 0., 0.02);
            self.prototype_target(&mut a, requested)?;
            ensure!(
                a.pieces_placed < placed,
                "backdrag did not reclaim published stones"
            );
            let edited = a.intent.cursor();
            let retained = a.pieces_placed;
            for _ in 0..4 {
                self.prototype_target(&mut a, requested)?;
            }
            ensure!(
                a.intent.cursor() == edited && a.pieces_placed == retained,
                "stationary raw pointer regrew a projected tail"
            );
            // Continue a new branch, then withdraw it and the original prefix.
            for i in 1..=20 {
                self.prototype_target(&mut a, edited + Vec3::new(0.3, 0., 0.2) * (i as f32 / 20.))?;
            }
            for i in (0..20).rev() {
                self.prototype_target(&mut a, edited + Vec3::new(0.3, 0., 0.2) * (i as f32 / 20.))?;
            }
            for i in (0..15).rev() {
                self.prototype_target(&mut a, start + Vec3::X * (i as f32 * 0.02))?;
            }
            ensure!(
                a.withdrawn(),
                "stroke must reach its origin without an implicit click"
            );
            let a = self.finish_pavement_on_ground(a)?;
            ensure!(
                a.pieces_placed == 0 && a.edits.owned_voxels().is_empty(),
                "withdrawn stroke retained owned stones"
            );
            ensure!(
                self.plain_builder.read_chunk_atlas_region(offset, dim)? == before,
                "withdrawal changed soil or completed paving"
            );
            ensure!(
                flora_count(&self.surface_builder.capture_flora_snapshot(0)?)?
                    == flora_count(&flora_before)?,
                "withdrawal did not restore exact removed flora count"
            );
            // All original authored identities and seeds must remain present.
            let before_json = serde_json::to_value(&flora_before)?;
            let after_json = serde_json::to_value(self.surface_builder.capture_flora_snapshot(0)?)?;
            for chunk in before_json["chunks"].as_array().unwrap() {
                let restored = after_json["chunks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["coordinate"] == chunk["coordinate"])
                    .context("restored flora chunk")?;
                for (species, index) in chunk["species"].as_array().unwrap().iter().zip(0..) {
                    for identity in species["authored"].as_array().unwrap() {
                        ensure!(
                            restored["species"][index]["authored"]
                                .as_array()
                                .unwrap()
                                .contains(identity),
                            "authored flora identity/seed lost on withdrawal"
                        );
                    }
                }
            }
            log::info!("[PAVEMENT_PROTOTYPE_REVIEW] kind={kind:?} published_retracted=true rejected_tail=true stationary=true branch=true empty_click_suppressed=true soil_and_completed_path=atlas_identical flora_count_and_identity=restored");
        }
        // Backdrag followed immediately by cancel, before any redraw/preflight.
        let start = sample(Vec2::new(0.25, 0.78))?;
        let offset = UVec3::new(32, (start.y * 256.).floor() as u32 - 16, 32);
        let dim = UVec3::new(448, 32, 448);
        let before = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        self.begin_path_on_ground(
            PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42),
            start,
            Vec3::X,
            ground.clone(),
        );
        let mut a = self.pavement.active.take().context("cancel retraction")?;
        for i in 1..=30 {
            self.prototype_target(&mut a, start + Vec3::X * (i as f32 * 0.015))?;
        }
        ensure!(a.pieces_placed > 0, "cancel fixture must publish stones");
        a.observe_editable_ground(start);
        self.pavement.active = Some(a);
        self.cancel_pavement_stroke();
        ensure!(
            self.pavement.active.is_none() && self.player_tools.terrain_edit_loop_sound_muted,
            "cancel lifecycle"
        );
        ensure!(
            self.plain_builder.read_chunk_atlas_region(offset, dim)? == before,
            "cancel before redraw left obsolete stones"
        );
        log::info!("[PAVEMENT_PROTOTYPE_REVIEW] backdrag_then_cancel=atlas_identical redraw_not_required=true sound=muted");
        // Same minimal sharp-turn witness as the approved HTML, with real soil.
        let points = [
            Vec2::new(0.1, 0.1),
            Vec2::new(0.1, 0.5),
            Vec2::new(0.11, 0.595),
            Vec2::new(0.55, 0.374),
        ];
        let points = points.map(sample).into_iter().collect::<Result<Vec<_>>>()?;
        let recipe = PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42)
            .with_smoothing(true, 2.)
            .with_retraction(false, 2.);
        self.begin_path_on_ground(recipe, points[0], Vec3::Z, ground);
        let mut a = self
            .pavement
            .active
            .take()
            .context("inward native gesture")?;
        for &point in &points[1..] {
            self.prototype_target(&mut a, point)?;
        }
        let a = self.finish_pavement_on_ground(a)?;
        ensure!(
            a.continuity.blocked().is_none() && a.pieces_placed > 0,
            "inward smoothing native publication"
        );
        log::info!("[PAVEMENT_PROTOTYPE_REVIEW] smoothing_range=2 sharp_corner=published terrain_preflight=true");
        let bytes = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        let flora = self.surface_builder.capture_flora_snapshot(0)?;
        let witness = Witness {
            offset: offset.to_array(),
            dim: dim.to_array(),
            crc: crc32fast::hash(&bytes),
            flora_count: flora_count(&flora)?,
            authored: authored_witness(&flora)?,
        };
        std::fs::create_dir_all("target/pavement-prototype-native")?;
        std::fs::write(WITNESS, serde_json::to_vec_pretty(&witness)?)?;
        Ok(())
    }
}
