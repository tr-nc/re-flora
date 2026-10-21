//! Opt-in real-game visual/lifecycle fixture. No settings or terrain snapshots are saved.
use super::{planting::AuthoredFloraPlacementBatch, soil_drying_review::DryingReview, App};
use crate::app::world_edits::TerrainBrushEdit;
use anyhow::{ensure, Result};
use glam::{UVec2, Vec2, Vec3};

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
    contact_pause: Option<Vec<u8>>,
}

impl Review {
    pub fn isolates_wetting(&self) -> bool {
        self.mode.starts_with("wet-") && self.mode != "wet-rotary"
    }

    pub fn checks_spray_contact(&self) -> bool {
        self.mode.starts_with("rotary-contact-") || self.mode == "wet-rotary"
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
        self.isolates_wetting()
            || self.checks_spray_contact()
            || self.mode == "rotary-terraces"
            || (self.isolates_drying() && self.frame > 180)
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
            "rotary-perf",
            "near",
            "rotary-near",
            "rotary-design-current",
            "rotary-design-impact",
            "rotary-design-garden",
            "rotary-design-rotor",
            "rotary-design-switch",
            "rotary-terraces",
            "rotary-contact-continuous",
            "rotary-contact-pulse",
            "rotary-contact-off",
            "rotary-contact-invisible",
            "rotary-contact-controls",
            "rotary-contact-wall",
            "rotary-contact-fast",
            "rotary-contact-particles",
            "rotary-contact-particles-only",
            "rotary-contact-particles-disabled",
            "rotary-contact-particles-wall",
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
            contact_pause: None,
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

    fn stamp_sprinkler_terraces(&mut self, base: Vec3) -> Result<Vec3> {
        use crate::app::world_edits::{VoxelEdit, WorldEditTransaction};
        use crate::geom::{build_bvh, Cuboid, UAabb3};
        let center = (base * 256.0).round();
        let mut cuboids = Vec::new();
        for z in -90..90 {
            for x in -90..90 {
                let radius = glam::Vec2::new(x as f32 + 0.5, z as f32 + 0.5).length();
                let level = (radius / 6.4).floor() as i32;
                if level >= 14 {
                    continue;
                }
                let lo = center + Vec3::new(x as f32, 0.0, z as f32);
                let hi = lo + Vec3::new(1.0, (14 - level) as f32, 1.0);
                cuboids.push(Cuboid::from_min_max(lo, hi));
            }
        }
        let boxes: Vec<_> = cuboids.iter().map(|c| c.aabb()).collect();
        let indices: Vec<_> = (0..cuboids.len() as u32).collect();
        let bvh_nodes = build_bvh(&boxes, &indices).map_err(anyhow::Error::msg)?;
        let bounds = UAabb3::new(bvh_nodes[0].aabb.min_uvec3(), bvh_nodes[0].aabb.max_uvec3());
        self.execute_world_edit(WorldEditTransaction::terrain_change(
            vec![VoxelEdit::StampCuboids {
                bvh_nodes,
                cuboids,
                voxel_type: crate::builder::VOXEL_TYPE_DIRT,
                atlas_state_write: Default::default(),
            }],
            bounds,
        ))?;
        Ok((center + Vec3::Y * 14.0) / 256.0)
    }

    fn review_sprinkler_terraces(&mut self, base: Vec3) -> Result<()> {
        let center = (base * 256.0).round().as_uvec3();
        let offset = center - glam::UVec3::new(92, 14, 92);
        let dim = glam::UVec3::new(185, 14, 185);
        let data = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        let mut wet = [0u32; 14];
        let mut cells = [0u32; 14];
        let mut surface = vec![0u8; 185 * 185];
        for z in 0..185usize {
            for x in 0..185usize {
                let r = glam::Vec2::new(x as f32 - 91.5, z as f32 - 91.5).length();
                let level = (r / 6.4).floor() as usize;
                if level >= 14 {
                    continue;
                }
                let byte = data[x + (13 - level) * 185 + z * 185 * 14];
                ensure!(
                    byte & 15 == crate::builder::VOXEL_TYPE_DIRT as u8,
                    "terrace receiver was not soil"
                );
                surface[x + z * 185] = byte;
                cells[level] += 1;
                wet[level] += u32::from((byte >> 4) & 3 > 0);
            }
        }
        log::info!("[SPRINKLER_TERRACES] wet={wet:?} cells={cells:?} real_surface_readback=true");
        if let Ok(path) = std::env::var("RE_FLORA_SPRINKLER_WET_STATS") {
            std::fs::write(
                path,
                serde_json::to_vec(&serde_json::json!({
                    "mode":"rotary-terraces","wet":wet,"cells":cells,"surface":surface,"base":base.to_array()
                }))?,
            )?;
        }
        Ok(())
    }

    fn spray_contact_review_region(base: Vec3) -> (glam::UVec3, glam::UVec3) {
        let center = (base * 256.0).round().as_uvec3();
        (
            center - glam::UVec3::new(160, 1, 160),
            glam::UVec3::new(321, 1, 321),
        )
    }

    fn review_spray_contact(&mut self, review: &Review, base: Vec3) -> Result<()> {
        let (offset, dim) = Self::spray_contact_review_region(base);
        let data = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        let mut dose = 0u64;
        let mut wet = 0;
        let mut under_source = 0;
        let mut radial_wet = [0u32; 3];
        let mut outside_walls = 0;
        for (i, byte) in data.iter().enumerate() {
            let moisture = (byte >> 4) & 3;
            let x = i % 321;
            let z = i / 321;
            if moisture > 0 {
                ensure!(
                    byte & 15 == crate::builder::VOXEL_TYPE_DIRT as u8,
                    "spray changed non-soil material"
                );
                wet += 1;
                dose += u64::from(moisture);
                let dx = x as f32 - 159.5;
                let dz = z as f32 - 159.5;
                let radius = (dx * dx + dz * dz).sqrt() / 256.0;
                under_source += usize::from(radius < 8.0 / 256.0);
                if (0.012..0.09).contains(&radius) {
                    radial_wet[0] += 1;
                }
                if (0.09..0.19).contains(&radius) {
                    radial_wet[1] += 1;
                }
                if (0.19..0.33).contains(&radius) {
                    radial_wet[2] += 1;
                }
                outside_walls += usize::from(dx.abs() > 27.0 || dz.abs() > 27.0);
            }
        }
        let contacts = self.tracer.sprinkler_water_review_contacts()?;
        let active: Vec<_> = contacts.iter().filter(|c| c.center_flux[3] > 0.0).collect();
        for c in &contacts {
            ensure!(
                c.center_flux
                    .iter()
                    .chain(c.radii.iter())
                    .chain(c.normal.iter())
                    .chain(c.projection.iter())
                    .all(|v| v.is_finite()),
                "non-finite GPU water contact"
            );
        }
        if review.mode.ends_with("-off") {
            ensure!(dose == 0, "closed valve still watered soil: {dose}");
        } else if review.mode.ends_with("-wall") {
            // Near droplets legitimately land BEFORE the wall. Far water must
            // stop there, not disappear with the inner flow or leak outside.
            ensure!(
                radial_wet[0] > 0,
                "walls also removed the unblocked near spray"
            );
            ensure!(
                outside_walls == 0,
                "spray watered outside the solid enclosure"
            );
        } else {
            ensure!(
                radial_wet.iter().all(|n| *n > 0),
                "spray missed a near/middle/far band: {radial_wet:?}"
            );
            ensure!(under_source > 0, "spray still leaves a dry inner ring");
        }
        if review.mode.ends_with("-off") {
            ensure!(active.is_empty(), "closed valve still emitted water");
        }
        log::info!("[SPRINKLER_CONTACT_REVIEW] mode={} wet_voxels={wet} total_moisture={dose} under_source={under_source} radial_wet={radial_wet:?} outside_walls={outside_walls} contacts={} density={} saved=false",
            review.mode,active.len(),self.debug_settings.adjustables.sprinkler_mist_density.value);
        if let Ok(path) = std::env::var("RE_FLORA_SPRINKLER_WET_STATS") {
            let hits:Vec<_>=active.iter().map(|c|serde_json::json!({
                "centerFlux":c.center_flux,"radii":c.radii,"tangent":c.tangent_rotary,"normal":c.normal,"projection":c.projection})).collect();
            std::fs::write(
                path,
                serde_json::to_vec(&serde_json::json!({
                    "mode":review.mode,"wetVoxels":wet,"totalMoisture":dose,"underSource":under_source,
                    "radialWet":radial_wet,"outsideWalls":outside_walls,
                    "contacts":hits,"base":base.to_array(),"offset":offset.to_array(),"dimensions":dim.to_array(),"voxels":data
                }))?,
            )?;
        }
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

    // Loaded realized garden only. No terrain stamps, flora painting or quality overrides.
    fn prepare_garden_sprinkler_perf(&mut self, review: &mut Review) -> Result<()> {
        ensure!(review.mode == "rotary-perf" && self.terrain_persistence.startup_load_requested());
        let (count, overlap, duty) = sprinkler_perf_parameters()?;
        if review.frame == 0 {
            ensure!(self.sprinklers.is_empty() && self.mute_audio_output);
            if let Ok(mode) = std::env::var("RE_FLORA_SPRINKLER_OPTICAL_DIAGNOSTIC") {
                let diagnostic = crate::tracer::SprinklerOpticalDiagnostic::parse(&mode)?;
                let gui = &self.debug_settings.adjustables;
                ensure!(
                    gui.sprinkler_mist_flow.value
                        && gui.sprinkler_mist_density.value > 0.0
                        && gui.sprinkler_mist_particles.value
                        && self.render_flags.enable_particles,
                    "optical diagnostic needs flowing mist and droplets enabled in saved settings; it only masks the consumer"
                );
                self.tracer.set_sprinkler_optical_diagnostic(diagnostic);
                log::info!("[SPRINKLER_OPTICAL_DIAGNOSTIC] channels={mode} preparation=both bounds=both irrigation=unchanged saved=false");
            }
            self.set_manual_time_of_day(0.43);
            self.debug_settings.adjustables.sprinkler_pulse_duty.value = duty;
            // Fixed 4x4 prefix, independent of population. Every source resolves its own
            // actual support column (the old flat-height fixture is not valid on hills).
            for i in 0..count {
                let spacing = if overlap { 0.0001 } else { 0.10 };
                let xz = Vec2::new(
                    0.62 + (i % 4) as f32 * spacing,
                    1.30 - (i / 4) as f32 * spacing,
                );
                let column = (xz * 256.0).floor().as_uvec2();
                let anchor = self
                    .resolve_plantable_surface_column(column)
                    .map_err(|e| anyhow::anyhow!("garden sprinkler support {i}: {e:?}"))?;
                let base = anchor.surface_position_ws();
                self.apply_sprinkler_placement(base)?;
                log::info!("[GARDEN_PERF_SOURCE] index={i} column={column:?} grounded={base:?} instance={:?}", self.sprinklers.render_instances()[i]);
            }
            log::info!("[SPRINKLER_PERF] count={count} overlap={overlap} duty={duty} garden=true quality=saved camera={:?} time=0.43 saved=false", self.tracer.camera_pose());
        }
        if review.frame == 180 {
            let lobes = if count > 0 {
                self.tracer.sprinkler_mist_review_lobes()?
            } else {
                Vec::new()
            };
            ensure!(lobes.len() == count * crate::tracer::SPRINKLER_MIST_LOBES as usize);
            for (index, instance) in lobes
                .chunks_exact(crate::tracer::SPRINKLER_MIST_LOBES as usize)
                .enumerate()
            {
                ensure!(
                    instance
                        .iter()
                        .all(|l| l.center_amplitude.iter().all(|v| v.is_finite()))
                        && instance.iter().any(|l| l.center_amplitude[3] > 0.0),
                    "garden missing GPU source {index}"
                );
            }
            if std::env::var_os("RE_FLORA_SPRINKLER_OPTICAL_DIAGNOSTIC").is_some() {
                let droplets = if count > 0 {
                    self.tracer.sprinkler_review_droplets()?
                } else {
                    Vec::new()
                };
                ensure!(droplets.len() == count * crate::tracer::SPRINKLER_DROPLETS as usize);
                ensure!(
                    droplets
                        .iter()
                        .all(|l| bytemuck::cast_slice::<_, f32>(std::slice::from_ref(l))
                            .iter()
                            .all(|v| v.is_finite())),
                    "nonfinite diagnostic droplet data"
                );
                let live = droplets
                    .iter()
                    .filter(|l| l.center_amplitude[3] > 0.0)
                    .count();
                ensure!(
                    count == 0 || live > 0,
                    "diagnostic disabled droplet preparation"
                );
                log::info!("[SPRINKLER_OPTICAL_PUBLICATION] instances={count} slots={} live={live} droplets_prepared=true readback=warmup_only", droplets.len());
            }
            let voices = self.spatial_frame.sprinkler_diagnostics();
            ensure!(
                voices.0 == count.min(16),
                "garden must run real audio: {voices:?}"
            );
            log::info!("[SPRINKLER_PERF_PUBLICATION] instances={count} all_instances_live=true audio={voices:?} readback=warmup_only");
        }
        review.frame += 1;
        Ok(())
    }

    pub(super) fn prepare_sprinkler_review(&mut self) -> Result<()> {
        if !self.terrain_persistence.allows_world_updates() {
            return Ok(());
        }
        let Some(mut review) = self.sprinkler_review.take() else {
            return Ok(());
        };
        if std::env::var_os("RE_FLORA_GARDEN_PERF").is_some() {
            self.prepare_garden_sprinkler_perf(&mut review)?;
            self.sprinkler_review = Some(review);
            return Ok(());
        }
        if review.frame == 0 {
            ensure!(
                self.sprinklers.is_empty(),
                "sprinkler review needs an empty fixture"
            );
            let anchor = self
                .resolve_plantable_surface_column(UVec2::new(196, 360))
                .map_err(|e| anyhow::anyhow!("sprinkler review surface: {e:?}"))?;
            let mut base = anchor.surface_position_ws();
            if review.checks_spray_contact() {
                // Full 90-degree cone footprint must stay inside the 512² fixture atlas.
                base.x = 0.75;
                base.z = 1.25;
            }
            self.debug_settings
                .adjustables
                .sprinkler_mist_particles
                .value = review.mode.contains("particles") && !review.mode.ends_with("disabled");
            self.debug_settings
                .adjustables
                .sprinkler_pulse_frequency
                .value = 4.0;
            self.debug_settings
                .adjustables
                .sprinkler_design_variants
                .value = false;
            if review.is_rotary() {
                let gui = &mut self.debug_settings.adjustables;
                gui.sprinkler_spray_angle.value = 40.0;
                gui.sprinkler_rotation_speed.value = 15.0;
                gui.sprinkler_pulse_duty.value = 100.0;
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
            if review.mode.starts_with("rotary-design-") {
                let gui = &mut self.debug_settings.adjustables;
                gui.sprinkler_design_variants.value =
                    review.mode != "rotary-design-current" && review.mode != "rotary-design-switch";
                gui.sprinkler_design.value = match review.mode.as_str() {
                    "rotary-design-garden" => 1,
                    "rotary-design-rotor" => 2,
                    _ => 0,
                };
                gui.sprinkler_rotation_speed.value = 0.0;
                gui.sprinkler_wetting_speed.value = 0.0;
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
            if review.checks_spray_contact() {
                self.stamp_sprinkler_review_box(
                    base - Vec3::new(0.64, 0.03, 0.64),
                    base + Vec3::new(0.64, 0.0, 0.64),
                    crate::builder::VOXEL_TYPE_DIRT,
                )?;
                let (offset, dim) = Self::spray_contact_review_region(base);
                let data = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
                let dry: Vec<_> = data
                    .iter()
                    .map(|v| v & !crate::builder::VOXEL_MOISTURE_MASK)
                    .collect();
                self.plain_builder
                    .write_chunk_atlas_region(offset, dim, &dry)?;
                let gui = &mut self.debug_settings.adjustables;
                gui.sprinkler_wetting_speed.value = 3.0;
                gui.sprinkler_mist_density.value = if review.mode.ends_with("-invisible")
                    || review.mode.ends_with("particles-only")
                {
                    0.0
                } else {
                    1.0
                };
                gui.sprinkler_pulse_duty.value = if review.mode.ends_with("-off") {
                    0.0
                } else if review.mode.ends_with("-pulse") || review.mode.ends_with("-fast") {
                    50.0
                } else {
                    100.0
                };
                if review.mode.ends_with("-fast") {
                    gui.sprinkler_rotation_speed.value = 60.0;
                    gui.sprinkler_spray_angle.value = 60.0;
                }
                if review.mode.ends_with("-wall") {
                    for axis in [Vec3::X, Vec3::Z] {
                        for sign in [-1.0, 1.0] {
                            let center = base + axis * sign * 0.10 + Vec3::Y * 0.13;
                            let half = Vec3::splat(0.13) - axis * (0.13 - 0.5 / 256.0);
                            self.stamp_sprinkler_review_box(
                                center - half,
                                center + half,
                                crate::builder::VOXEL_TYPE_ROCK,
                            )?;
                        }
                    }
                }
            }
            if review.mode == "rotary-terraces" {
                base = self.stamp_sprinkler_terraces(base)?;
                let gui = &mut self.debug_settings.adjustables;
                gui.sprinkler_wetting_speed.value = 3.0;
                gui.sprinkler_wet_patch_size.value = 2.6;
                gui.sprinkler_pulse_duty.value = 100.0;
            }
            if review.mode == "rotary-perf" {
                let (count, overlap, duty) = sprinkler_perf_parameters()?;
                base.x = 0.75;
                base.z = 0.95;
                let gui = &mut self.debug_settings.adjustables;
                gui.sprinkler_spray_angle.value = 30.0;
                gui.sprinkler_rotation_speed.value = 5.0;
                gui.sprinkler_pulse_frequency.value = 5.0;
                gui.sprinkler_pulse_duty.value = duty;
                gui.sprinkler_mist_density.value = 6.0;
                gui.sprinkler_mist_particles.value = true;
                let side = (count as f32).sqrt().ceil() as usize;
                for i in 0..count {
                    let spacing = if overlap { 0.0001 } else { 0.17 };
                    self.apply_sprinkler_placement(
                        base + Vec3::new(
                            (i % side) as f32 * spacing - 0.255,
                            0.0,
                            (i / side) as f32 * spacing - 0.255,
                        ),
                    )?;
                }
                log::info!("[SPRINKLER_PERF] count={count} overlap={overlap} angle=30 rpm=5 hz=5 duty={duty} intensity=6 droplets=true seed=fixture camera_target={base:?} camera_offset=(0.75,0.55,0.90) saved=false");
            } else {
                self.apply_sprinkler_placement(base)?;
            }
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
                || review.checks_spray_contact()
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
        if review.mode == "rotary-contact-controls" {
            // Read the actual GPU consumers before changing their next snapshot.
            if review.frame == 84 {
                let (offset, dim) = Self::spray_contact_review_region(base);
                review.contact_pause =
                    Some(self.plain_builder.read_chunk_atlas_region(offset, dim)?);
            }
            if review.frame == 120 {
                let (offset, dim) = Self::spray_contact_review_region(base);
                ensure!(
                    review.contact_pause.as_ref()
                        == Some(&self.plain_builder.read_chunk_atlas_region(offset, dim)?),
                    "zero duty changed real soil"
                );
                ensure!(
                    self.tracer
                        .sprinkler_water_review_contacts()?
                        .iter()
                        .all(|c| c.center_flux[3] == 0.0),
                    "zero duty left incoming water"
                );
                ensure!(
                    self.tracer
                        .sprinkler_mist_review_lobes()?
                        .iter()
                        .all(|l| l.center_amplitude[3] == 0.0),
                    "zero duty left visible spray"
                );
                log::info!(
                    "[SPRINKLER_CONTACT_CONTROL] stopped_air_and_water=true rotor_continues=true"
                );
            }
            let (angle, rpm, duty) = match review.frame / 40 {
                0 => (10.0, 0.0, 100.0),
                1 => (90.0, 60.0, 100.0),
                2 => (40.0, 15.0, 0.0),
                3 => (40.0, 15.0, 50.0),
                _ => (40.0, 15.0, 100.0),
            };
            let gui = &mut self.debug_settings.adjustables;
            gui.sprinkler_pulse_frequency.value = match review.frame / 40 {
                0 => 0.25,
                1 => 20.0,
                3 => 1.0,
                _ => 4.0,
            };
            gui.sprinkler_spray_angle.value = angle;
            gui.sprinkler_rotation_speed.value = rpm;
            gui.sprinkler_pulse_duty.value = duty;
            if review.frame % 40 == 0 && review.frame <= 160 {
                log::info!("[SPRINKLER_CONTACT_CONTROL] step={} angle={angle} rpm={rpm} duty={duty} saved=false",review.frame/40);
            }
        }
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
        if review.mode == "rotary-controls"
            && review.frame > 0
            && review.frame <= 160
            && review.frame % 40 == 0
        {
            let lobes = self.tracer.sprinkler_mist_review_lobes()?;
            ensure!(
                lobes.len() == crate::tracer::SPRINKLER_MIST_LOBES as usize,
                "spray sample count"
            );
            let live: Vec<_> = lobes
                .iter()
                .filter(|l| l.center_amplitude[3] > 0.0)
                .collect();
            ensure!(
                !live.is_empty() && live.iter().all(|l| l.tangent_rotary[3] > 0.5),
                "retired alternating spray became active"
            );
            log::info!("[SPRINKLER_ROTARY_REVIEW] step={} whole_assembly=true internal_poses=16 live_samples={} explicit_readback=true",
                review.frame / 40, live.len());
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
            _ if review.mode.starts_with("rotary-design-") => {
                // Same three-quarter view of the actual fixed nozzle pose for
                // every candidate; a head-on view hides the arm silhouettes.
                let phase = self.sprinklers.render_instances()[0].animation_phase;
                let pose = crate::sprinkler::RotorPose {
                    angle: (phase * 16.0).round() * std::f32::consts::TAU / 16.0,
                };
                let forward = crate::sprinkler::RotorPose { angle: 0.0 }.direction();
                let side = Vec3::new(-forward.z, 0.0, forward.x);
                let scale =
                    if std::env::var("RE_FLORA_SPRINKLER_DESIGN_VIEW").as_deref() == Ok("play") {
                        2.7
                    } else {
                        1.0
                    };
                pose.rotate((forward + side).normalize() * 0.085 + Vec3::Y * 0.035) * scale
            }
            _ if review.checks_spray_contact() => Vec3::new(0.001, 0.85, 0.001),
            _ if review.isolates_wetting() || review.isolates_drying() => {
                Vec3::new(0.001, 0.50, 0.001)
            }
            "rotary-perf" => Vec3::new(0.75, 0.55, 0.90),
            "rotary-top" => Vec3::new(0.001, 0.10, 0.001),
            "rotary-cliff" => Vec3::new(0.65, -0.09, 0.45),
            "rotary-wall" => Vec3::new(0.55, 0.05, 0.40),
            "rotary-mist-back" => -Vec3::new(sun.x, 0.0, sun.z).normalize() * 0.36 + Vec3::Y * 0.14,
            "rotary-mist-front" => Vec3::new(sun.x, 0.0, sun.z).normalize() * 0.36 + Vec3::Y * 0.14,
            "rotary-mist-top" => Vec3::new(0.001, 0.45, 0.001),
            "rotary-terraces" => Vec3::new(0.28, 0.52, 0.40),
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
        if review.mode == "rotary-design-switch" {
            const STEPS: [(bool, u32, bool); 7] = [
                (false, 0, true),
                (true, 0, true),
                (true, 1, true),
                (true, 2, true),
                (true, 2, false),
                (false, 2, true),
                (true, 0, true),
            ];
            if let Some(&(enabled, candidate, cached)) = STEPS.get(review.control_step) {
                let gui = &mut self.debug_settings.adjustables;
                gui.sprinkler_design_variants.value = enabled;
                gui.sprinkler_design.value = candidate;
                gui.sprinkler_cache_enabled.value = cached;
                let selected = crate::sprinkler::Design::comparison(enabled, candidate) as u32;
                let input =
                    super::render_frame_input::model_cache_inputs(&self.debug_settings).surfaces;
                for kind in 0..4 {
                    ensure!(
                        input.for_kind(kind) == review.control_baseline.unwrap().for_kind(kind),
                        "candidate affected other model caches"
                    );
                }
                if review.control_frames > 0
                    && self.tracer.sprinkler_cache_status() == Some((cached, 32, selected))
                {
                    review.control_frames += 1;
                    if review.control_frames >= 8 {
                        log::info!("[SPRINKLER_DESIGN_REVIEW] step={} design={selected} cached={cached} other_models_unchanged=true",review.control_step);
                        review.control_step += 1;
                        review.control_frames = 0;
                        if review.control_step == STEPS.len() {
                            log::info!("[SPRINKLER_DESIGN_REVIEW] complete steps=7 saved=false");
                        }
                    }
                } else {
                    review.control_frames = 1;
                }
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
                    && self.tracer.sprinkler_cache_status() == Some((enabled, resolution, 0))
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
        if review.mode == "rotary-perf" && review.frame == 30 {
            let count = sprinkler_perf_parameters()?.0;
            if count > 0 {
                let lobes = self.tracer.sprinkler_mist_review_lobes()?;
                ensure!(
                    lobes.len() == count * crate::tracer::SPRINKLER_MIST_LOBES as usize,
                    "performance fixture GPU publication size"
                );
                for (index, instance) in lobes
                    .chunks_exact(crate::tracer::SPRINKLER_MIST_LOBES as usize)
                    .enumerate()
                {
                    ensure!(
                        instance
                            .iter()
                            .all(|l| l.center_amplitude.iter().all(|v| v.is_finite()))
                            && instance.iter().any(|l| l.center_amplitude[3] > 0.0),
                        "performance fixture missing live GPU source for instance {index}"
                    );
                }
            }
            log::info!("[SPRINKLER_PERF_PUBLICATION] instances={count} all_instances_live=true readback=warmup_only");
        }
        if review.frame == 180 {
            if review.mode.starts_with("rotary-design-") {
                log::info!(
                    "[SPRINKLER_DESIGN_REVIEW] mode={} status={:?}",
                    review.mode,
                    self.tracer.sprinkler_cache_status()
                );
            }
            if review.mode == "rotary-terraces" {
                self.review_sprinkler_terraces(base)?;
            }
            if review.checks_spray_contact() {
                self.review_spray_contact(&review, base)?;
            }
            if review.isolates_wetting() {
                self.review_sprinkler_wetting(&mut review, base, true)?;
            }
            if (review.mode.starts_with("mist-") || review.is_rotary())
                && self.debug_settings.adjustables.sprinkler_mist_flow.value
                && (!review.is_rotary()
                    || self.debug_settings.adjustables.sprinkler_pulse_duty.value > 0.0)
                && self.debug_settings.adjustables.sprinkler_mist_density.value > 0.0
                && review.mode != "rotary-perf"
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
            if review.mode.contains("particles") {
                let drops = self.tracer.sprinkler_review_droplets()?;
                let live: Vec<_> = drops
                    .iter()
                    .filter(|p| p.center_amplitude[3] > 0.0)
                    .collect();
                let enabled = self
                    .debug_settings
                    .adjustables
                    .sprinkler_mist_particles
                    .value
                    && self.render_flags.enable_particles;
                ensure!(
                    drops.len() == crate::tracer::SPRINKLER_DROPLETS as usize,
                    "bounded droplet slots"
                );
                ensure!(
                    live.is_empty() != enabled,
                    "droplet checkbox/global particle flag not followed"
                );
                for p in &live {
                    ensure!(
                        p.center_amplitude
                            .iter()
                            .chain(&p.radii_near_geometry)
                            .all(|v| v.is_finite()),
                        "non-finite droplet"
                    );
                    ensure!(
                        p.radii_near_geometry[0] < 0.001 && p.center_amplitude[1] > base.y,
                        "droplets must be small airborne samples"
                    );
                    if review.mode.ends_with("-wall") {
                        ensure!(
                            (p.center_amplitude[0] - base.x).abs() < 27.5 / 256.0
                                && (p.center_amplitude[2] - base.z).abs() < 27.5 / 256.0,
                            "droplet escaped enclosure"
                        );
                    }
                }
                log::info!("[SPRINKLER_DROPLET_REVIEW] enabled={enabled} active={} capacity={} mist_intensity={} optical_only=true",
                    live.len(),drops.len(),self.debug_settings.adjustables.sprinkler_mist_density.value);
            }
            let water_particles = self
                .particle_snapshots
                .iter()
                .filter(|p| p.kind == crate::particles::ParticleRenderKind::WaterDroplet)
                .count();
            ensure!(
                water_particles == 0,
                "optical droplets must not create CPU irrigation particles"
            );
            if review.mode == "mist-controls" {
                ensure!(review.watering_brushes.iter().all(|n| *n > 0),
                    "watering must record real GPU brushes at zero, normal and strong visual density");
            }
            log::info!("[SPRINKLER_MIST_REVIEW] water_particles={water_particles} watering_brushes={:?} density={} particles_enabled={}",
                review.watering_brushes, self.debug_settings.adjustables.sprinkler_mist_density.value,
                self.render_flags.enable_particles);
            let expected = if review.mode == "rotary-perf" {
                sprinkler_perf_parameters()?.0
            } else if review.mode.starts_with("mist-pop-")
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

// Separate opt-in benchmark parameters: never persisted and never used by normal play.
fn sprinkler_perf_parameters() -> Result<(usize, bool, f32)> {
    parse_sprinkler_perf_parameters(&std::env::var("RE_FLORA_SPRINKLER_PERF")?)
}

fn parse_sprinkler_perf_parameters(raw: &str) -> Result<(usize, bool, f32)> {
    let parts: Vec<_> = raw.split(',').collect();
    ensure!(
        parts.len() == 3,
        "sprinkler perf expects count,spread|overlap,30|100"
    );
    let count = parts[0].parse::<usize>()?;
    ensure!([0, 1, 4, 8, 16].contains(&count), "invalid perf population");
    ensure!(
        ["spread", "overlap"].contains(&parts[1]),
        "invalid perf layout"
    );
    let duty = parts[2].parse::<f32>()?;
    ensure!(duty == 30.0 || duty == 100.0, "invalid perf duty");
    Ok((count, parts[1] == "overlap", duty))
}

#[cfg(test)]
mod perf_tests {
    use super::parse_sprinkler_perf_parameters as parse;

    #[test]
    fn perf_matrix_accepts_only_bounded_explicit_workloads() {
        for count in [0, 1, 4, 8, 16] {
            for layout in ["spread", "overlap"] {
                for duty in [30, 100] {
                    assert_eq!(
                        parse(&format!("{count},{layout},{duty}")).unwrap(),
                        (count, layout == "overlap", duty as f32)
                    );
                }
            }
        }
        for bad in [
            "",
            "4",
            "256,spread,30",
            "3,spread,30",
            "4,away,30",
            "4,spread,NaN",
            "4,spread,0",
            "4,spread,30,extra",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
