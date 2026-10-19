//! Opt-in real-game visual/lifecycle fixture. No settings or terrain snapshots are saved.
use super::{planting::AuthoredFloraPlacementBatch, soil_drying_review::DryingReview, App};
use crate::app::world_edits::TerrainBrushEdit;
use anyhow::{ensure, Result};
use glam::{UVec2, Vec3};

#[derive(Default)]
struct WettingSamples {
    histogram: [u32; 4],
    edges: u32,
    boundaries: u32,
    trials: u32,
    means: Vec<f64>,
    footprint_histogram: [u32; 4],
    footprint_means: Vec<f64>,
    outside_sector_moisture: u64,
}

pub(super) struct Review {
    mode: String,
    frame: u32,
    base: Option<Vec3>,
    control_step: usize,
    control_frames: u32,
    control_baseline: Option<crate::tracer::ModelSurfaceCacheOptions>,
    watering_brushes: [u32; 3],
    wetting_samples: WettingSamples,
    drying: Option<DryingReview>,
}

impl Review {
    pub fn isolates_wetting(&self) -> bool {
        self.mode.starts_with("wet-")
    }

    pub fn is_rotary(&self) -> bool {
        self.mode.starts_with("rotary-") || self.mode == "wet-rotary"
    }

    pub fn wetting_axis(&self) -> Vec3 {
        if self.mode == "wet-edge-x" {
            Vec3::X
        } else {
            Vec3::Z
        }
    }

    pub fn isolates_drying(&self) -> bool {
        self.mode.starts_with("dry-")
    }

    pub fn freezes_drying(&self) -> bool {
        self.isolates_wetting() || (self.isolates_drying() && self.frame > 180)
    }

    fn wetting_ensemble(&self) -> bool {
        self.isolates_wetting() && self.mode != "wet-controls" && self.mode != "wet-brush"
    }

    fn wetting_steps(&self) -> u32 {
        match self.mode.as_str() {
            "wet-30fps" => 6,
            "wet-120fps" => 24,
            _ => 12,
        }
    }

    pub fn wetting_dt(&self, real_dt: f32) -> f32 {
        if !self.isolates_wetting() {
            return real_dt;
        }
        let steps = self.wetting_steps();
        let frame = if self.mode == "wet-controls" {
            self.frame % 40
        } else {
            self.frame
        };
        if frame > 0 && frame <= steps * if self.wetting_ensemble() { 7 } else { 1 } {
            0.2 / steps as f32
        } else {
            0.0
        }
    }

    pub fn observe_watering(&mut self, brushes: usize, density: f32) {
        let slot = if density == 0.0 {
            0
        } else if density >= 3.0 {
            2
        } else {
            1
        };
        self.watering_brushes[slot] += brushes as u32;
    }

    pub fn from_env() -> Result<Option<Self>> {
        let Ok(mode) = std::env::var("RE_FLORA_SPRINKLER_REVIEW") else {
            return Ok(None);
        };
        const MODES: &[&str] = &[
            "near",
            "rotary-near",
            "rotary-top",
            "rotary-pixels-8",
            "rotary-pixels-16",
            "rotary-pixels-32",
            "rotary-controls",
            "rotary-mist",
            "rotary-mist-back",
            "rotary-mist-front",
            "rotary-mist-top",
            "rotary-mist-off",
            "rotary-mist-legacy",
            "rotary-lifecycle",
            "rotary-wall",
            "rotary-cliff",
            "wet-rotary",
            "garden",
            "orbit",
            "lifecycle",
            "pixels-8",
            "pixels-16",
            "pixels-32",
            "controls",
            "mist-back",
            "mist-front",
            "mist-off",
            "mist-inside",
            "mist-controls",
            "mist-wall",
            "mist-night",
            "mist-level",
            "mist-level-off",
            "mist-front-off",
            "mist-cliff",
            "mist-cliff-off",
            "mist-cliff-legacy",
            "mist-flow",
            "mist-flow-off",
            "mist-flow-legacy",
            "mist-flow-top",
            "mist-flow-controls",
            "mist-thin-wall",
            "mist-thin-wall-off",
            "mist-corner",
            "mist-corner-off",
            "mist-roof",
            "mist-roof-off",
            "mist-edit",
            "mist-pop-16",
            "mist-pop-64",
            "mist-pop-256",
            "mist-overlap-256",
            "wet-2",
            "wet-4",
            "wet-12",
            "wet-24",
            "wet-edge-x",
            "wet-edge-z",
            "wet-30fps",
            "wet-120fps",
            "wet-zero",
            "wet-double",
            "wet-controls",
            "wet-mist",
            "wet-brush",
            "dry-zero",
            "dry-slow",
            "dry-normal",
            "dry-fast",
            "dry-controls",
            "dry-watering",
        ];
        ensure!(
            MODES.contains(&mode.as_str()),
            "RE_FLORA_SPRINKLER_REVIEW must be one of {}",
            MODES.join(", ")
        );
        Ok(Some(Self {
            mode,
            frame: 0,
            base: None,
            control_step: 0,
            control_frames: 0,
            control_baseline: None,
            watering_brushes: [0; 3],
            wetting_samples: WettingSamples::default(),
            drying: None,
        }))
    }
}

impl App {
    fn stamp_sprinkler_review_box(&mut self, lo: Vec3, hi: Vec3, voxel_type: u32) -> Result<()> {
        use crate::app::world_edits::{VoxelEdit, WorldEditTransaction};
        use crate::geom::{build_bvh, Cuboid, UAabb3};
        let cuboids = vec![Cuboid::from_min_max(
            (lo * 256.0).floor(),
            (hi * 256.0).ceil(),
        )];
        let bvh_nodes = build_bvh(&[cuboids[0].aabb()], &[0]).map_err(anyhow::Error::msg)?;
        let bounds = UAabb3::new(bvh_nodes[0].aabb.min_uvec3(), bvh_nodes[0].aabb.max_uvec3());
        self.execute_world_edit(WorldEditTransaction::terrain_change(
            vec![VoxelEdit::StampCuboids {
                bvh_nodes,
                cuboids,
                voxel_type,
                atlas_state_write: Default::default(),
            }],
            bounds,
        ))?;
        Ok(())
    }

    fn review_sprinkler_wetting(
        &mut self,
        review: &mut Review,
        base: Vec3,
        save: bool,
    ) -> Result<()> {
        use glam::UVec3;
        let center = (base * 256.0).round().as_uvec3();
        let offset = UVec3::new(center.x - 80, center.y - 1, center.z - 80);
        let dim = UVec3::new(161, 1, 161);
        let data = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        let mut histogram = [0u32; 4];
        let mut edges = 0u32;
        let mut boundaries = 0u32;
        let mut footprint_histogram = [0u32; 4];
        let mut outside_sector_moisture = 0u64;
        let axis = review.wetting_axis();
        let scale = self
            .debug_settings
            .adjustables
            .sprinkler_wet_patch_size
            .value
            .max(2.0);
        let mut eligible = vec![false; data.len()];
        for z in 0..161usize {
            for x in 0..161usize {
                let index = z * 161 + x;
                let byte = data[index];
                let ty = byte & 15;
                let moisture = (byte >> 4) & 3;
                if ty != crate::builder::VOXEL_TYPE_DIRT as u8
                    && ty != crate::builder::VOXEL_TYPE_SAND as u8
                {
                    ensure!(moisture == 0, "wetting modified non-soil material");
                    continue;
                }
                let dx = offset.x as f32 + x as f32 + 0.5 - base.x * 256.0;
                let dz = offset.z as f32 + z as f32 + 0.5 - base.z * 256.0;
                let radius = (dx * dx + dz * dz).sqrt();
                let along = (dx * axis.x + dz * axis.z).abs();
                let across = (dx * axis.z - dz * axis.x).abs();
                let inside = radius < 76.8
                    && (radius < 0.001 || along / radius >= std::f32::consts::FRAC_1_SQRT_2);
                if review.mode != "wet-brush"
                    && (radius >= 76.8
                        || (!review.is_rotary() && across - along > 2.0 * scale + 0.002))
                {
                    ensure!(
                        moisture == 0,
                        "wetting escaped range or finite patch support"
                    );
                }
                if radius < 76.8 {
                    footprint_histogram[moisture as usize] += 1;
                    if !inside {
                        outside_sector_moisture += u64::from(moisture);
                    }
                }
                if inside && radius > 8.0 && radius < 60.0 && along / radius > 0.75 {
                    eligible[index] = true;
                    histogram[moisture as usize] += 1;
                }
            }
        }
        for z in 0..160usize {
            for x in 0..160usize {
                let i = z * 161 + x;
                for j in [i + 1, i + 161] {
                    if eligible[i] && eligible[j] {
                        edges += 1;
                        boundaries += u32::from(((data[i] >> 4) & 3) != ((data[j] >> 4) & 3));
                    }
                }
            }
        }
        let samples = &mut review.wetting_samples;
        if !save || samples.trials == 0 {
            let count = histogram.iter().sum::<u32>().max(1);
            let dose = histogram
                .iter()
                .enumerate()
                .map(|(i, n)| i as u32 * n)
                .sum::<u32>();
            samples.means.push(f64::from(dose) / f64::from(count));
            let footprint_count = footprint_histogram.iter().sum::<u32>().max(1);
            let footprint_dose = footprint_histogram
                .iter()
                .enumerate()
                .map(|(i, n)| i as u32 * n)
                .sum::<u32>();
            samples
                .footprint_means
                .push(f64::from(footprint_dose) / f64::from(footprint_count));
            for (total, part) in samples
                .footprint_histogram
                .iter_mut()
                .zip(footprint_histogram)
            {
                *total += part;
            }
            samples.outside_sector_moisture += outside_sector_moisture;
            for (total, part) in samples.histogram.iter_mut().zip(histogram) {
                *total += part;
            }
            samples.edges += edges;
            samples.boundaries += boundaries;
            samples.trials += 1;
        }
        if !save {
            if samples.trials < 7 {
                // Explicit review only: reset moisture, preserving all type/state
                // bits and geometry. A single correlated realization is NOT a
                // meaningful conservation test; compare independent soil trials.
                let dry: Vec<u8> = data
                    .iter()
                    .map(|b| b & !crate::builder::VOXEL_MOISTURE_MASK)
                    .collect();
                self.plain_builder
                    .write_chunk_atlas_region(offset, dim, &dry)?;
            }
            return Ok(());
        }
        let histogram = samples.histogram;
        let edges = samples.edges;
        let boundaries = samples.boundaries;
        let trials = samples.trials;
        let dose = histogram
            .iter()
            .enumerate()
            .map(|(i, n)| i as u32 * n)
            .sum::<u32>();
        let count = histogram.iter().sum::<u32>();
        let mean = dose as f32 / count.max(1) as f32;
        let disagreement = boundaries as f32 / edges.max(1) as f32;
        let footprint_count = samples.footprint_histogram.iter().sum::<u32>().max(1);
        let footprint_dose = samples
            .footprint_histogram
            .iter()
            .enumerate()
            .map(|(i, n)| i as u32 * n)
            .sum::<u32>();
        let footprint_mean = f64::from(footprint_dose) / f64::from(footprint_count);
        if review.mode.starts_with("wet-edge-") {
            ensure!(
                samples.outside_sector_moisture > 0,
                "real wet patches are still hard-clipped at the 90-degree sector edge"
            );
        }
        if review.mode == "wet-zero" {
            ensure!(
                footprint_dose == 0,
                "zero wetting speed still wrote moisture"
            );
        } else if review.mode != "wet-brush" {
            ensure!(dose > 0, "wetting produced no real soil state");
        }
        log::info!("[SPRINKLER_WETTING_REVIEW] mode={} trials={trials} receivers={count} histogram={histogram:?} mean={mean:.6} adjacent_disagreement={disagreement:.6} footprint_mean={footprint_mean:.6} outside_sector_moisture={} saved=false",review.mode,samples.outside_sector_moisture);
        if let Ok(path) = std::env::var("RE_FLORA_SPRINKLER_WET_STATS") {
            let payload = serde_json::json!({"mode":review.mode,"trials":trials,"trialMeans":samples.means,"receivers":count,"histogram":histogram,"mean":mean,"adjacentDisagreement":disagreement,"footprintMean":footprint_mean,"footprintTrialMeans":samples.footprint_means,"outsideSectorMoisture":samples.outside_sector_moisture,"offset":offset.to_array(),"dimensions":dim.to_array(),"voxels":data});
            std::fs::write(path, serde_json::to_vec(&payload)?)?;
        }
        Ok(())
    }

    pub(super) fn prepare_sprinkler_review(&mut self) -> Result<()> {
        if !self.terrain_persistence.allows_world_updates() {
            return Ok(());
        }
        let Some(mut review) = self.sprinkler_review.take() else {
            return Ok(());
        };
        if review.frame == 0 {
            ensure!(
                self.sprinklers.is_empty(),
                "sprinkler review needs an empty fixture"
            );
            let anchor = self
                .resolve_plantable_surface_column(UVec2::new(196, 360))
                .map_err(|e| anyhow::anyhow!("sprinkler review surface: {e:?}"))?;
            let mut base = anchor.surface_position_ws();
            self.debug_settings.adjustables.sprinkler_rotary.value = review.is_rotary();
            if review.is_rotary() {
                let gui = &mut self.debug_settings.adjustables;
                gui.sprinkler_cache_enabled.value = true;
                gui.sprinkler_pixel_resolution.value = 32;
                gui.sprinkler_mist_flow.value = !review.mode.ends_with("-legacy");
                gui.sprinkler_mist_density.value = if review.mode.contains("mist")
                    || matches!(
                        review.mode.as_str(),
                        "rotary-controls" | "rotary-wall" | "rotary-cliff"
                    ) {
                    1.0
                } else {
                    0.0
                };
            }
            if review.mode.starts_with("mist-cliff") || review.mode == "rotary-cliff" {
                let lo = base + Vec3::new(-0.30, -0.01, -0.20);
                let hi = base + Vec3::new(0.08, 0.32, 0.20);
                self.stamp_sprinkler_review_box(lo, hi, crate::builder::VOXEL_TYPE_ROCK)?;
                base.y = (hi.y * 256.0).ceil() / 256.0;
            }
            if review.mode.starts_with("mist-pop-") {
                // Keep the complete population and its mist margins inside the voxel domain.
                base.x = 0.60;
                base.z = 0.60;
            }
            if review.isolates_wetting() || review.isolates_drying() {
                self.stamp_sprinkler_review_box(
                    base - Vec3::new(0.34, 0.03, 0.34),
                    base + Vec3::new(0.34, 0.0, 0.34),
                    crate::builder::VOXEL_TYPE_DIRT,
                )?;
                // A non-absorbing stripe inside the footprint guards material/state ownership.
                self.stamp_sprinkler_review_box(
                    base + Vec3::new(0.10, -0.03, -0.30),
                    base + Vec3::new(0.11, 0.0, 0.30),
                    crate::builder::VOXEL_TYPE_ROCK,
                )?;
                let gui = &mut self.debug_settings.adjustables;
                if review.isolates_drying() {
                    let watering = review.mode == "dry-watering";
                    gui.sprinkler_wetting_speed.value = if watering { 1.0 } else { 0.0 };
                    gui.sprinkler_mist_density.value = if watering { 1.0 } else { 0.0 };
                    gui.soil_drying_speed.value = match review.mode.as_str() {
                        "dry-zero" | "dry-watering" => 0.0,
                        "dry-slow" => 0.2,
                        "dry-fast" => 3.0,
                        _ => 1.0,
                    };
                    review.drying = Some(DryingReview::new(
                        &mut self.plain_builder,
                        base,
                        if watering { 0 } else { 3 },
                    )?);
                } else {
                    gui.sprinkler_mist_density.value =
                        if review.mode == "wet-mist" { 1.0 } else { 0.0 };
                    gui.sprinkler_wet_patch_size.value = match review.mode.as_str() {
                        "wet-2" => 2.0,
                        "wet-4" => 4.0,
                        "wet-24" => 24.0,
                        "wet-edge-x" | "wet-edge-z" => 32.0,
                        _ => 12.0,
                    };
                    gui.sprinkler_wetting_speed.value = match review.mode.as_str() {
                        "wet-zero" | "wet-brush" => 0.0,
                        "wet-double" => 2.0,
                        _ => 1.0,
                    };
                }
            }
            self.apply_sprinkler_placement(base)?;
            if review.mode.starts_with("mist-pop-") || review.mode == "mist-overlap-256" {
                let count = review.mode.rsplit('-').next().unwrap().parse::<usize>()?;
                let side = (count as f32).sqrt().ceil() as usize;
                for i in 1..count {
                    let spacing = if review.mode.starts_with("mist-overlap") {
                        0.0001
                    } else {
                        0.17
                    };
                    self.apply_sprinkler_placement(
                        base + Vec3::new(
                            (i % side) as f32 * spacing,
                            0.0,
                            (i / side) as f32 * spacing,
                        ),
                    )?;
                }
            }
            let mut plants = AuthoredFloraPlacementBatch::new();
            for (index, column) in [
                UVec2::new(180, 346),
                UVec2::new(211, 345),
                UVec2::new(216, 367),
                UVec2::new(176, 365),
            ]
            .into_iter()
            .enumerate()
            {
                let anchor = self
                    .resolve_plantable_surface_column(column)
                    .map_err(|e| anyhow::anyhow!("sprinkler review flower: {e:?}"))?;
                ensure!(
                    self.try_place_authored_flora(
                        &mut plants,
                        crate::flora::MODEL_FLOWER_FIRST_SPECIES + index as u32,
                        anchor,
                        255,
                        0,
                        index as u32 * 137
                    ),
                    "sprinkler review planting"
                );
            }
            self.finish_authored_flora_placement(plants)?;
            review.base = Some(base);
            self.set_manual_time_of_day(if review.mode == "mist-night" {
                0.0
            } else if review.mode.starts_with("mist-level") {
                0.43
            } else if review.mode.starts_with("mist-")
                || matches!(
                    review.mode.as_str(),
                    "rotary-mist-back" | "rotary-mist-front" | "rotary-mist-top"
                )
            {
                0.30
            } else {
                0.43
            });
            if review.mode.starts_with("mist-") {
                self.debug_settings.adjustables.sprinkler_mist_density.value = 1.0;
                self.debug_settings.adjustables.sprinkler_mist_flow.value =
                    !review.mode.ends_with("-legacy");
            }
            if review.mode.starts_with("mist-thin-wall")
                || review.mode.starts_with("mist-corner")
                || review.mode == "mist-edit"
                || review.mode == "rotary-wall"
            {
                let span = if review.is_rotary() { 0.5 } else { 0.22 };
                let x = (base.x * 256.0).floor() + 24.0;
                self.stamp_sprinkler_review_box(
                    Vec3::new(x / 256.0, base.y, base.z - span),
                    Vec3::new((x + 1.0) / 256.0, base.y + 0.26, base.z + span),
                    crate::builder::VOXEL_TYPE_ROCK,
                )?;
            }
            if review.mode.starts_with("mist-corner") {
                let z = (base.z * 256.0).floor() + 24.0;
                self.stamp_sprinkler_review_box(
                    Vec3::new(base.x - 0.22, base.y, z / 256.0),
                    Vec3::new(base.x + 0.22, base.y + 0.26, (z + 1.0) / 256.0),
                    crate::builder::VOXEL_TYPE_ROCK,
                )?;
            }
            if review.mode.starts_with("mist-roof") {
                let y = (base.y * 256.0).floor() + 14.0;
                self.stamp_sprinkler_review_box(
                    Vec3::new(base.x - 0.25, y / 256.0, base.z - 0.25),
                    Vec3::new(base.x + 0.25, (y + 1.0) / 256.0, base.z + 0.25),
                    crate::builder::VOXEL_TYPE_ROCK,
                )?;
            }
            if review.mode == "mist-wall" {
                self.apply_cube_placement(crate::app::world_edits::CubePlacementEdit {
                    center: base + Vec3::new(-0.29, 0.10, -0.207),
                    size: 0.20,
                    rotation: glam::Quat::IDENTITY,
                    voxel_type: crate::builder::VOXEL_TYPE_ROCK,
                })?;
            }
            self.debug_settings.adjustables.auto_daynight_cycle.value = false;
            if let Some(n) = review
                .mode
                .strip_prefix("pixels-")
                .or_else(|| review.mode.strip_prefix("rotary-pixels-"))
            {
                self.debug_settings
                    .adjustables
                    .sprinkler_cache_enabled
                    .value = true;
                self.debug_settings
                    .adjustables
                    .sprinkler_pixel_resolution
                    .value = n.parse()?;
            }
            review.control_baseline =
                Some(super::render_frame_input::model_cache_inputs(&self.debug_settings).surfaces);
            log::info!("[SPRINKLER_REVIEW] mode={} base={base:?} placement=production raster=true saved=false", review.mode);
        }
        if review.frame == 60 {
            if let Some(resize) = &mut self.resize_lifecycle_test {
                // Startup resizes can finish before the first live consumer.
                // Replay only after hardware, mist and their descriptors have drawn.
                resize.requested = 0;
                resize.next_request_frame = 0;
                resize.complete = false;
                log::info!("[SPRINKLER_MIST_RESIZE] after_submitted_frames=60");
            }
        }
        let base = review.base.unwrap();
        if let Some(drying) = &mut review.drying {
            drying.checkpoint(
                &mut self.plain_builder,
                &review.mode,
                review.frame,
                self.debug_settings.adjustables.soil_drying_speed.value,
            )?;
            if review.mode == "dry-controls" {
                let speed = match review.frame / 60 {
                    0 => 1.0,
                    1 => 0.0,
                    2 => 3.0,
                    _ => 0.0,
                };
                self.debug_settings.adjustables.soil_drying_speed.value = speed;
                if review.frame % 60 == 0 && review.frame <= 180 {
                    log::info!(
                        "[SOIL_DRYING_CONTROL] step={} speed={speed:.2} saved=false",
                        review.frame / 60
                    );
                }
            }
        }
        if review.wetting_ensemble()
            && review.frame > 0
            && review.frame <= 7 * review.wetting_steps()
            && review.frame % review.wetting_steps() == 0
        {
            self.review_sprinkler_wetting(&mut review, base, false)?;
        }
        if review.mode == "wet-controls" {
            let (size, speed) = match review.frame / 40 {
                0 => (2.0, 1.0),
                1 => (4.0, 1.0),
                2 => (12.0, 1.0),
                3 => (24.0, 1.5),
                _ => (24.0, 0.0),
            };
            let gui = &mut self.debug_settings.adjustables;
            gui.sprinkler_wet_patch_size.value = size;
            gui.sprinkler_wetting_speed.value = speed;
            if review.frame % 40 == 0 && review.frame <= 160 {
                log::info!(
                    "[SPRINKLER_WETTING_CONTROL] step={} scale={size} speed={speed} saved=false",
                    review.frame / 40
                );
            }
        }
        if review.mode == "wet-brush" && review.frame == 1 {
            self.add_watering_brush_moisture(TerrainBrushEdit {
                start: base,
                end: base + Vec3::Z * 0.08,
                radius: 0.07,
            })?;
            self.mix_tiller_brush_soil(TerrainBrushEdit {
                start: base + Vec3::Z * 0.15,
                end: base + Vec3::Z * 0.17,
                radius: 0.04,
            })?;
        }
        let mut target = base
            + Vec3::Y
                * if review.mode.starts_with("mist-") {
                    0.075
                } else {
                    3. / 256.
                };
        if review.mode.starts_with("mist-pop-") {
            let count = review.mode.rsplit('-').next().unwrap().parse::<usize>()?;
            let span = ((count as f32).sqrt().ceil() - 1.0) * 0.17;
            target += Vec3::new(span * 0.5, 0.0, span * 0.5);
        }
        if review.mode.ends_with("-off") {
            self.debug_settings.adjustables.sprinkler_mist_density.value = 0.0;
        }
        if review.mode == "rotary-controls" {
            if review.frame > 0 && review.frame <= 160 && review.frame % 40 == 0 {
                let previous = self.debug_settings.adjustables.sprinkler_rotary.value;
                let lobes = self.tracer.sprinkler_mist_review_lobes()?;
                ensure!(lobes.len() == 48, "rotary review lobe ABI");
                let live: Vec<_> = lobes
                    .iter()
                    .filter(|l| l.center_amplitude[3] > 0.0)
                    .collect();
                ensure!(!live.is_empty(), "GPU mist mode has no live spray");
                ensure!(
                    live.iter().all(|l| (l.tangent_rotary[3] > 0.5) == previous),
                    "GPU mist mode did not follow the live checkbox"
                );
                log::info!("[SPRINKLER_ROTARY_REVIEW] gpu_previous={previous} oriented_single_outlet={previous} live_samples={} explicit_readback=true", live.len());
            }
            let enabled = (review.frame / 40) % 2 == 0;
            self.debug_settings.adjustables.sprinkler_rotary.value = enabled;
            if review.frame % 40 == 0 && review.frame <= 160 {
                log::info!(
                    "[SPRINKLER_ROTARY_REVIEW] step={} enabled={enabled} saved=false",
                    review.frame / 40
                );
            }
        }
        if review.mode == "mist-flow-controls" {
            let flow = (review.frame / 40) % 2 == 0;
            self.debug_settings.adjustables.sprinkler_mist_flow.value = flow;
            if review.frame % 40 == 0 && review.frame <= 160 {
                log::info!(
                    "[SPRINKLER_FLOW_REVIEW] step={} flow={flow} density=1 saved=false",
                    review.frame / 40
                );
            }
        }
        if review.mode == "mist-edit" && review.frame == 90 {
            let x = (base.x * 256.0).floor() + 24.0;
            self.stamp_sprinkler_review_box(
                Vec3::new(x / 256.0, base.y, base.z - 0.22),
                Vec3::new((x + 1.0) / 256.0, base.y + 0.26, base.z + 0.22),
                crate::builder::VOXEL_TYPE_EMPTY,
            )?;
            log::info!("[SPRINKLER_FLOW_REVIEW] terrain_edit=removed_wall frame=90");
        }
        if review.mode == "mist-controls" {
            let density = match review.frame / 40 {
                0 => 1.0,
                1 => 0.0,
                2 => 3.0,
                _ => 1.0,
            };
            self.debug_settings.adjustables.sprinkler_mist_density.value = density;
            if review.frame % 40 == 0 && review.frame <= 160 {
                log::info!(
                    "[SPRINKLER_MIST_REVIEW] density={density} watering_sources={}",
                    self.sprinklers.moisture_sources().len()
                );
            }
        }
        // Align the backlit fixture with the actual low sun, not a guessed azimuth.
        let (altitude, azimuth) = Self::calculate_sun_position(
            0.30,
            self.debug_settings.adjustables.latitude.value,
            self.debug_settings.adjustables.season.value,
        );
        let sun = crate::util::get_sun_dir(altitude.asin().to_degrees(), azimuth * 360.0);
        let offset = match review.mode.as_str() {
            _ if review.isolates_wetting() || review.isolates_drying() => {
                Vec3::new(0.001, 0.50, 0.001)
            }
            "rotary-top" => Vec3::new(0.001, 0.10, 0.001),
            "rotary-cliff" => Vec3::new(0.65, -0.09, 0.45),
            "rotary-wall" => Vec3::new(0.55, 0.05, 0.40),
            "rotary-mist-back" => -Vec3::new(sun.x, 0.0, sun.z).normalize() * 0.36 + Vec3::Y * 0.14,
            "rotary-mist-front" => Vec3::new(sun.x, 0.0, sun.z).normalize() * 0.36 + Vec3::Y * 0.14,
            "rotary-mist-top" => Vec3::new(0.001, 0.45, 0.001),
            _ if review.mode.contains("rotary-mist") => Vec3::new(0.50, 0.10, 0.42),
            "garden" => Vec3::new(0.20, 0.19, 0.32),
            "mist-back" | "mist-off" | "mist-controls" | "mist-night" => -sun * 0.36,
            "mist-wall" => Vec3::new(-0.55, 0.07, -0.392),
            "mist-level" | "mist-level-off" => Vec3::new(-0.30, -0.020, -0.214),
            "mist-front" | "mist-front-off" => sun * 0.36,
            "mist-flow" | "mist-flow-off" | "mist-flow-legacy" | "mist-flow-controls" => {
                Vec3::new(0.50, 0.10, 0.42)
            }
            "mist-flow-top" => Vec3::new(0.001, 0.55, 0.001),
            "mist-cliff" | "mist-cliff-off" | "mist-cliff-legacy" => Vec3::new(0.65, -0.09, 0.45),
            "mist-thin-wall" | "mist-thin-wall-off" | "mist-edit" | "mist-corner"
            | "mist-corner-off" => Vec3::new(0.55, 0.05, 0.40),
            "mist-roof" | "mist-roof-off" => Vec3::new(0.55, -0.04, 0.40),
            "mist-pop-16" | "mist-pop-64" | "mist-pop-256" => {
                let count = review.mode.rsplit('-').next().unwrap().parse::<usize>()?;
                let span = ((count as f32).sqrt().ceil() - 1.0) * 0.17;
                Vec3::new(0.75, 0.55, 0.90) * (span / 0.90).max(0.8)
            }
            "mist-overlap-256" => Vec3::new(0.50, 0.10, 0.42),
            "mist-inside" => Vec3::new(-0.015, 0.015, -0.025),
            "orbit" => {
                let angle = review.frame as f32 * 0.012;
                Vec3::new(angle.sin() * 0.12, 0.065, angle.cos() * 0.12)
            }
            _ => Vec3::new(0.055, 0.045, 0.080),
        };
        self.camera_control.apply_snapshot_mode(true);
        self.camera_control.set_orbit_focus(target);
        ensure!(
            self.tracer
                .set_camera_pose_looking_at(target + offset, target),
            "sprinkler review camera"
        );
        self.reset_camera_movement_input();
        if matches!(review.mode.as_str(), "lifecycle" | "rotary-lifecycle") {
            if review.frame == 40 {
                self.apply_sprinkler_placement(base + Vec3::X * 0.035)?;
                self.apply_sprinkler_placement(base - Vec3::X * 0.035)?;
                ensure!(
                    self.sprinklers.len() == 3 && self.tracer.sprinkler_instance_count() == 3,
                    "sprinkler instances must publish together with gameplay records"
                );
            }
            if review.frame == 120 {
                ensure!(
                    self.remove_sprinklers_in_brush(TerrainBrushEdit {
                        start: base,
                        end: base,
                        radius: 0.06,
                    })? == 3,
                    "sprinkler brush removal"
                );
                ensure!(
                    self.sprinklers.is_empty() && self.tracer.sprinkler_instance_count() == 0,
                    "removal must retire hardware, optical volumes and watering sources"
                );
            }
            if review.frame == 150 {
                self.apply_sprinkler_placement(base)?;
            }
        }
        if matches!(review.mode.as_str(), "controls" | "rotary-controls") {
            const STEPS: [(bool, u32); 7] = [
                (true, 8),
                (true, 16),
                (true, 32),
                (false, 32),
                (true, 24),
                (true, 8),
                (true, 32),
            ];
            if let Some(&(enabled, resolution)) = STEPS.get(review.control_step) {
                let gui = &mut self.debug_settings.adjustables;
                gui.sprinkler_cache_enabled.value = enabled;
                gui.sprinkler_pixel_resolution.value = resolution;
                let inputs =
                    super::render_frame_input::model_cache_inputs(&self.debug_settings).surfaces;
                for kind in 0..4 {
                    ensure!(
                        inputs.for_kind(kind) == review.control_baseline.unwrap().for_kind(kind),
                        "sprinkler control changed another model's sampling"
                    );
                }
                if review.control_frames > 0
                    && self.tracer.sprinkler_cache_status() == Some((enabled, resolution))
                {
                    review.control_frames += 1;
                    if review.control_frames >= 12 {
                        log::info!("[SPRINKLER_CACHE_REVIEW] step={} cached={enabled} resolution={resolution} other_models_unchanged=true",
                            review.control_step);
                        review.control_step += 1;
                        review.control_frames = 0;
                        if review.control_step == STEPS.len() {
                            log::info!("[SPRINKLER_CACHE_REVIEW] complete steps=7 saved=false");
                        }
                    }
                } else {
                    review.control_frames = 1;
                }
            }
        }
        if review.frame == 180 {
            if review.isolates_wetting() {
                self.review_sprinkler_wetting(&mut review, base, true)?;
            }
            if (review.mode.starts_with("mist-") || review.is_rotary())
                && self.debug_settings.adjustables.sprinkler_mist_flow.value
                && self.debug_settings.adjustables.sprinkler_mist_density.value > 0.0
                && !review.mode.starts_with("mist-pop-")
                && !review.mode.starts_with("mist-overlap-")
            {
                let lobes = self.tracer.sprinkler_mist_review_lobes()?;
                let mut active = 0;
                let mut below = 0;
                let mut beyond_wall = 0;
                let mut beyond_corner = 0;
                let mut above_roof = 0;
                let wall_z = ((base.z * 256.0).floor() + 25.0) / 256.0;
                let roof_y = ((base.y * 256.0).floor() + 14.0) / 256.0;
                let wall_x = ((base.x * 256.0).floor() + 25.0) / 256.0;
                for lobe in lobes {
                    ensure!(
                        lobe.center_amplitude.iter().all(|v| v.is_finite()),
                        "non-finite mist field"
                    );
                    if lobe.center_amplitude[3] <= 0.0 {
                        continue;
                    }
                    active += 1;
                    ensure!(
                        lobe.radii_near_geometry[..3]
                            .iter()
                            .all(|v| *v > 0.0 && v.is_finite()),
                        "invalid mist support"
                    );
                    below += usize::from(lobe.center_amplitude[1] < base.y - 0.03);
                    beyond_wall += usize::from(lobe.center_amplitude[0] > wall_x + 0.005);
                    beyond_corner += usize::from(
                        lobe.center_amplitude[0] > wall_x + 0.005
                            || lobe.center_amplitude[2] > wall_z + 0.005,
                    );
                    above_roof += usize::from(lobe.center_amplitude[1] > roof_y);
                }
                ensure!(
                    active > 0,
                    "terrain-aware mist produced no live continuous density"
                );
                if matches!(review.mode.as_str(), "mist-cliff" | "rotary-cliff") {
                    ensure!(below > 0, "cliff mist never settled below its source");
                }
                if matches!(review.mode.as_str(), "mist-thin-wall" | "rotary-wall") {
                    ensure!(beyond_wall == 0, "mist tunnelled through a one-voxel wall");
                }
                if review.mode == "mist-corner" {
                    ensure!(
                        beyond_corner == 0,
                        "mist escaped through the solid wall corner"
                    );
                }
                if review.mode == "mist-roof" {
                    ensure!(above_roof == 0, "mist crossed a one-voxel overhead slab");
                }
                if review.mode == "mist-edit" {
                    ensure!(
                        beyond_wall > 0,
                        "mist retained stale terrain after wall removal"
                    );
                }
                log::info!("[SPRINKLER_FLOW_REVIEW] gpu_lobes={active} below_source={below} beyond_wall={beyond_wall} beyond_corner={beyond_corner} above_roof={above_roof} readback=explicit_review_only");
            }
            let water_particles = self
                .particle_snapshots
                .iter()
                .filter(|p| p.kind == crate::particles::ParticleRenderKind::WaterDroplet)
                .count();
            ensure!(
                water_particles == 0,
                "mist must not spawn visible water particles"
            );
            if review.mode == "mist-controls" {
                ensure!(review.watering_brushes.iter().all(|n| *n > 0),
                    "watering must record real GPU brushes at zero, normal and strong visual density");
            }
            log::info!("[SPRINKLER_MIST_REVIEW] water_particles={water_particles} watering_brushes={:?} density={} particles_enabled={}",
                review.watering_brushes, self.debug_settings.adjustables.sprinkler_mist_density.value,
                self.render_flags.enable_particles);
            let expected = if review.mode.starts_with("mist-pop-")
                || review.mode.starts_with("mist-overlap-")
            {
                review.mode.rsplit('-').next().unwrap().parse::<usize>()?
            } else {
                1
            };
            ensure!(
                self.sprinklers.len() == expected
                    && self.tracer.sprinkler_instance_count() == expected as u32,
                "sprinkler review final publication"
            );
            log::info!(
                "[SPRINKLER_REVIEW] passed mode={} frames=180 instances={expected} saved=false",
                review.mode
            );
        }
        review.frame += 1;
        self.sprinkler_review = Some(review);
        Ok(())
    }
}
