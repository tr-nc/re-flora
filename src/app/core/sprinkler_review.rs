//! Opt-in real-game visual/lifecycle fixture. No settings or terrain snapshots are saved.
use super::{planting::AuthoredFloraPlacementBatch, App};
use crate::app::world_edits::TerrainBrushEdit;
use anyhow::{ensure, Result};
use glam::{UVec2, Vec3};

pub(super) struct Review {
    mode: String,
    frame: u32,
    base: Option<Vec3>,
    control_step: usize,
    control_frames: u32,
    control_baseline: Option<crate::tracer::ModelSurfaceCacheOptions>,
    watering_brushes: [u32; 3],
}

impl Review {
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
            if review.mode.starts_with("mist-cliff") {
                let lo = base + Vec3::new(-0.30, -0.01, -0.20);
                let hi = base + Vec3::new(0.08, 0.32, 0.20);
                self.stamp_sprinkler_review_box(lo, hi, crate::builder::VOXEL_TYPE_ROCK)?;
                base.y = (hi.y * 256.0).ceil() / 256.0;
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
            } else if review.mode.starts_with("mist-") {
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
            {
                let x = (base.x * 256.0).floor() + 24.0;
                self.stamp_sprinkler_review_box(
                    Vec3::new(x / 256.0, base.y, base.z - 0.22),
                    Vec3::new((x + 1.0) / 256.0, base.y + 0.26, base.z + 0.22),
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
            if let Some(n) = review.mode.strip_prefix("pixels-") {
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
        let target = base
            + Vec3::Y
                * if review.mode.starts_with("mist-") {
                    0.075
                } else {
                    3. / 256.
                };
        if review.mode.ends_with("-off") {
            self.debug_settings.adjustables.sprinkler_mist_density.value = 0.0;
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
            "mist-pop-16" | "mist-pop-64" | "mist-pop-256" => Vec3::new(1.7, 1.4, 1.9),
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
        if review.mode == "lifecycle" {
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
        if review.mode == "controls" {
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
            if review.mode.starts_with("mist-")
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
                if review.mode == "mist-cliff" {
                    ensure!(below > 0, "cliff mist never settled below its source");
                }
                if review.mode == "mist-thin-wall" {
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
