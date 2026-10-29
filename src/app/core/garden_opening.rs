//! One-time adapter from the pure opening recipe into ordinary editing transactions.
mod audit;
mod grass_review;
#[cfg(test)]
mod tree_settings_tests;
use super::{planting::AuthoredFloraPlacementBatch, App, VOXEL_DIM_PER_CHUNK};
use crate::{
    app::{gui_config::DebugSettings, world_edits::TerrainBrushEdit},
    gameplay::CameraPose,
    garden_layout as layout,
    tree_gen::{TreeDesc, TreeSpecies},
};
use glam::{Vec2, Vec3};

pub(super) use crate::garden_layout::is_review_preset;

fn opening_tree_description(settings: &DebugSettings, seed: u32, index: usize) -> TreeDesc {
    super::vegetation::planting_description(
        settings,
        TreeSpecies::AppleTree,
        layout::tree_individual(seed, index),
    )
}

impl App {
    pub(super) fn establish_garden_opening(&mut self, seed: u32) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.terrain_persistence.startup_load_requested(),
            "loaded world must never be regenerated"
        );
        let world = self.world_chunk_dim * VOXEL_DIM_PER_CHUNK;
        let size = Vec2::new(world.x as f32, world.z as f32) / 256.;
        // Use the player's world-space Grass Mix distribution, not a separate
        // all-short meadow that would leave a species seam when extended.
        let center = Vec3::new(size.x * 0.5, 0.53, size.y * 0.5);
        self.seed_opening_grass(
            TerrainBrushEdit {
                start: center,
                end: center,
                radius: size.length() * 0.49,
            },
            seed,
        )?;
        self.review_opening_grass_distribution()?;
        for (i, p) in [
            Vec2::new(0.21, 0.40),
            Vec2::new(0.77, 0.46),
            Vec2::new(0.57, 0.24),
        ]
        .iter()
        .enumerate()
        {
            let xz = *p * size;
            let center = Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y);
            self.seed_opening_grass(
                TerrainBrushEdit {
                    start: center,
                    end: center,
                    radius: 0.17,
                },
                layout::hash(seed ^ i as u32),
            )?;
        }
        // Sample the same saved recipe/maturity as Grow, without charging inventory.
        // Once published, each tree owns its description and optional growth clock.
        let instant_mature = self
            .debug_settings
            .adjustables
            .tree_planting_instant_mature
            .value;
        let mut placed_trees = 0;
        for (i, p) in layout::TREE_CENTERS.iter().enumerate() {
            let xz = *p * size;
            let y = self.query_terrain_height_cpu(xz);
            let desc = opening_tree_description(&self.debug_settings, seed, i);
            let radius = layout::tree_radius_voxels(&desc);
            let height = crate::tree_gen::Tree::new(desc.clone()).height_voxels();
            log::info!("[GARDEN_OPENING] tree={i} species={} seed={} size={:.3} crown_radius_voxels={radius:.3} height_voxels={height:.3} position={:?} apple_probability={} appearance={:?} instant_mature={instant_mature}",
                desc.species.label(), desc.branching.seed, desc.size,
                Vec3::new(xz.x, y, xz.y), desc.fruit_spawn_probability, desc.appearance);
            match self.plant_tree_with_maturity(desc, Vec3::new(xz.x, y, xz.y), instant_mature) {
                Ok(id) => {
                    placed_trees += 1;
                    log::info!("[GARDEN_OPENING] planted_tree={i} id={id}");
                }
                // User-authored trees can exceed the world or intersect terrain. Apply
                // Grow's validation, not a hidden scale cap or a startup crash.
                Err(error) if error.is::<super::vegetation::TreePlantingRejection>() => {
                    log::warn!("[GARDEN_OPENING] skipped_tree={i} reason={error:#}");
                }
                Err(error) => return Err(error),
            }
        }
        // Actual mow/removal paths, not a shader-only exclusion: persists in snapshots,
        // and subsequent player edits/planting remain free to replace these patches.
        for segment in layout::ROUTE.windows(2) {
            let ground = |p: Vec2, app: &Self| {
                let xz = p * size;
                Vec3::new(xz.x, app.query_terrain_height_cpu(xz), xz.y)
            };
            self.apply_surface_flora_removal(TerrainBrushEdit {
                start: ground(segment[0], self),
                end: ground(segment[1], self),
                radius: 0.085,
            })?;
        }
        for (p, radius) in [(layout::ARRIVAL, 0.18), (layout::POCKET, 0.14)] {
            let xz = p * size;
            let center = Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y);
            self.apply_flora_trim_path(TerrainBrushEdit {
                start: center,
                end: center,
                radius: radius + 0.045,
            })?;
            self.apply_surface_flora_removal(TerrainBrushEdit {
                start: center,
                end: center,
                radius,
            })?;
        }
        let flowers = layout::flowers(seed, glam::UVec2::new(world.x, world.z), |column| {
            self.resolve_plantable_surface_column(column).is_ok()
        });
        let mut batch = AuthoredFloraPlacementBatch::new();
        let species = layout::FLOWER_KEYS.map(|key| {
            crate::flora::species::species()
                .iter()
                .position(|s| s.key == key)
                .expect("opening species must exist in the ordinary catalog") as u32
        });
        let time = self.time_info.time_since_start_duration().as_millis() as u32;
        let mut placed = 0;
        let mut fingerprint = seed;
        for flower in &flowers {
            let anchor = self
                .resolve_plantable_surface_column(flower.column)
                .map_err(|e| anyhow::anyhow!("garden substrate changed: {e:?}"))?;
            if self.try_place_authored_flora(
                &mut batch,
                species[flower.color],
                anchor,
                flower.growth,
                time,
                flower.seed,
            ) {
                placed += 1;
                fingerprint = layout::hash(
                    fingerprint ^ flower.seed ^ flower.column.x ^ (flower.column.y << 16),
                );
            }
        }
        self.finish_authored_flora_placement(batch)?;
        log::info!("[GARDEN_OPENING] version={} seed={seed} trees={placed_trees} flowers={placed} candidates={} attempt_cap={} fingerprint={fingerprint:08x} editable=true",
            layout::VERSION, flowers.len(), layout::MAX_ATTEMPTS);
        Ok(())
    }

    pub(super) fn apply_garden_opening_camera(&mut self) {
        let preset = self.launch_owners.snapshot_name().map(str::to_owned);
        let explicit_review = preset.as_deref().is_some_and(is_review_preset);
        if !explicit_review && (preset.is_some() || self.fresh_garden_seed.is_none()) {
            return;
        }
        let world = self.world_chunk_dim * VOXEL_DIM_PER_CHUNK;
        let size = Vec2::new(world.x as f32, world.z as f32) / 256.;
        let (p, target, overhead) = match preset.as_deref() {
            Some("garden-overview") => (Vec2::new(0.85, 1.10), Vec2::new(0.46, 0.43), true),
            Some("garden-reverse") => (layout::POCKET, layout::ARRIVAL, false),
            Some("garden-corridor") => (layout::ROUTE[1], layout::POCKET, false),
            _ => (layout::ARRIVAL, Vec2::new(0.52, 0.39), false),
        };
        let xz = p * size;
        let y = if overhead {
            1.55
        } else {
            self.query_terrain_height_cpu(xz) + 0.08
        };
        let direction = target * size - xz;
        self.tracer.apply_camera_pose(CameraPose {
            position: Vec3::new(xz.x, y, xz.y),
            yaw_deg: direction.x.atan2(-direction.y).to_degrees(),
            pitch_deg: if overhead { -39. } else { -8. },
            fov_deg: 60.,
        });
        self.camera_control.apply_snapshot_mode(overhead);
        log::info!(
            "[GARDEN_OPENING] camera={} position={:?} walk={}",
            preset.as_deref().unwrap_or("fresh-arrival"),
            self.tracer.camera_position(),
            !overhead
        );
    }
}
