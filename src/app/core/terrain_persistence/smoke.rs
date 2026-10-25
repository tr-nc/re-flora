//! Opt-in production-path acceptance, not a normal test or gameplay behavior.
use super::*;
use crate::app::world_edits::{TerrainBrushEdit, TreeAddOptions, TreePlacement};

impl App {
    pub(in crate::app::core) fn seed_grass_soil_cover_review(&mut self) -> Result<()> {
        if std::env::var("RE_FLORA_GRASS_SOIL_COVER_REVIEW").as_deref() != Ok("visual") {
            return Ok(());
        }
        let previous = self.player_tools.planting.flora_index;
        self.player_tools.planting.flora_index = 0; // ordinary Grass Mix
        let xz = Vec2::new(0.82, 1.22);
        let center = Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y);
        for serial in 1..=8 {
            self.apply_surface_flora_regeneration(
                TerrainBrushEdit {
                    start: center,
                    end: center,
                    radius: 0.10,
                },
                serial,
                true,
            )?;
        }
        self.player_tools.planting.flora_index = previous;
        let chunks: Vec<_> = self
            .surface_builder
            .resources
            .instances
            .chunk_flora_instances
            .iter()
            .map(|(_, r)| r.chunk_id)
            .collect();
        for chunk in chunks {
            self.surface_builder
                .update_flora_growth_for_chunk(chunk, 255)?;
        }
        self.surface_builder
            .refresh_grass_soil_cover(&mut self.plain_builder)?;
        let roots: usize = self
            .surface_builder
            .resources
            .instances
            .chunk_flora_instances
            .iter()
            .map(|(_, r)| r.hydration[..2].iter().map(Vec::len).sum::<usize>())
            .sum();
        anyhow::ensure!(
            roots > 0,
            "visual grass-soil fixture did not plant ordinary grass"
        );
        log::info!("[SOIL_COVER_REVIEW] visual roots={roots} center={center:?}");
        Ok(())
    }

    pub(in crate::app::core) fn seed_garden_snapshot_smoke(&mut self) -> Result<()> {
        if std::env::var("RE_FLORA_GARDEN_SNAPSHOT_SMOKE").as_deref() != Ok("seed") {
            return Ok(());
        }
        let previous_selection = self.player_tools.planting.flora_index;
        for index in 0..crate::flora::species::PLAYER_FLORA_PAINT_SELECTIONS.len() as u32 {
            // Vines have a separate, session-only Grow placement path and are
            // not members of the persistent FloraSnapshot species store.
            if !matches!(
                crate::flora::species::PLAYER_FLORA_PAINT_SELECTIONS[index as usize],
                crate::flora::species::FloraPaintSelection::Species(_)
            ) {
                continue;
            }
            // Keep every registered species inside the world as the catalog grows.
            let xz = Vec2::new(
                (50 + (index % 8) * 40) as f32 / 256.0,
                (70 + (index / 8) * 70) as f32 / 256.0,
            );
            let center = Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y);
            self.player_tools.planting.flora_index = index as usize;
            self.apply_surface_flora_regeneration(
                TerrainBrushEdit {
                    start: center,
                    end: center,
                    radius: TERRAIN_EDIT_DEFAULT_RADIUS,
                },
                index + 1,
                true,
            )?;
        }
        self.player_tools.planting.flora_index = previous_selection;
        self.debug_settings.adjustables.tree_age.value = 0.43;
        self.update_all_tree_ages_from_gui()?;
        self.add_snapshot_smoke_tree()?;
        log::info!("[GARDEN_SMOKE] seeded painted grass, all registered authored species, tuned and additional tree at age 0.43");
        Ok(())
    }

    fn add_snapshot_smoke_tree(&mut self) -> Result<()> {
        let xz = Vec2::new(1.3, 1.2);
        let position = Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y);
        let mut desc = TreeDesc::default();
        desc.branching.seed = 923;
        desc.branching.iterations = 3;
        self.add_tree(
            desc,
            TreePlacement::World(position),
            TreeAddOptions {
                assign_new_id: true,
            },
        )
    }

    pub(in crate::app::core) fn verify_garden_snapshot_smoke(&mut self) -> Result<()> {
        if std::env::var_os("RE_FLORA_PLANT_HYDRATION_REVIEW").is_some() {
            let old_selection = self.player_tools.planting.flora_index;
            self.player_tools.planting.flora_index = 0;
            let xz = Vec2::new(0.3, 0.3);
            let center = Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y);
            self.apply_surface_flora_regeneration(
                TerrainBrushEdit {
                    start: center,
                    end: center,
                    radius: 0.08,
                },
                42,
                true,
            )?;
            self.player_tools.planting.flora_index = old_selection;
            self.surface_builder
                .verify_hydration_runtime(&mut self.plain_builder)?;
        }
        if std::env::var("RE_FLORA_STONE_PLACEMENT_REVIEW").as_deref() == Ok("verify") {
            anyhow::ensure!(
                self.terrain_persistence.startup_load_requested(),
                "stone verify requires --terrain-load"
            );
            let path = self.terrain_persistence.selected_path().to_owned();
            let terrain = self.verify_live_garden_file(Path::new(&path))?;
            for (x, expected) in [
                (0.6, crate::builder::VOXEL_TYPE_ROCK),
                (1.3, crate::builder::VOXEL_TYPE_ROCK),
            ] {
                let hit = self
                    .query_terrain_ray_cpu(Vec3::new(x, 2.5, 0.6), Vec3::NEG_Y)
                    .context("restored stone requires a terrain ray hit")?;
                anyhow::ensure!(
                    hit.voxel_type == expected,
                    "restored stone surface has wrong material"
                );
                let readback = self.apply_surface_terrain_removal(
                    TerrainBrushEdit {
                        start: hit.position,
                        end: hit.position,
                        radius: 0.025,
                    },
                    Some(expected),
                    None,
                    None,
                )?;
                anyhow::ensure!(
                    readback.stats.count_removed(expected) > 0,
                    "Dig must edit restored stone voxels"
                );
                log::info!(
                    "[STONE_PLACEMENT_REVIEW] material={expected} dig_removed={}",
                    readback.stats.count_removed(expected)
                );
            }
            log::info!("[STONE_PLACEMENT_REVIEW] restart=verified terrain_bytes={terrain} rock_and_slab=true dig=verified snapshot_file=unchanged");
            return Ok(());
        }
        if let Some(output) = std::env::var_os("RE_FLORA_TERRAIN_BENCH") {
            return self.benchmark_garden_snapshots(Path::new(&output));
        }
        if std::env::var("RE_FLORA_GARDEN_SNAPSHOT_SMOKE").as_deref() != Ok("verify") {
            return Ok(());
        }
        anyhow::ensure!(
            self.terrain_persistence.startup_load_requested(),
            "garden verify requires --terrain-load"
        );
        let path = self.terrain_persistence.selected_path().to_owned();
        self.verify_live_garden_file(Path::new(&path))?;
        for pass in 0..2 {
            // A real intervening edit must disappear after Load, including wood and tree IDs.
            self.add_snapshot_smoke_tree()?;
            self.perform_runtime_terrain_load();
            anyhow::ensure!(
                self.terrain_persistence.status
                    == TerrainPersistenceStatus::PublishedAwaitingDependents,
                "runtime garden load failed: {}",
                self.terrain_persistence.status_label()
            );
            let deadline = Instant::now() + std::time::Duration::from_secs(20);
            while !self.terrain_persistence.can_start_operation() {
                self.advance_snapshot_smoke_dependents();
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "garden load water publication did not settle: {:?}",
                    self.water.terrain_status()
                );
                std::thread::yield_now();
            }
            anyhow::ensure!(
                self.water.is_running()
                    && self.water.terrain_status().is_ready()
                    && self.terrain_persistence.allows_world_updates(),
                "runtime garden load must resume real water and world updates"
            );
            let terrain = self.verify_live_garden_file(Path::new(&path))?;
            log::info!("[GARDEN_SMOKE] runtime replacement pass={} terrain={terrain} flora=exact plant_water=exact trees=exact no_duplicates=true water=Running terrain=Ready world_updates=true", pass + 1);
        }
        log::info!("[GARDEN_SMOKE] passed startup_and_repeated_runtime_load=true");
        Ok(())
    }

    fn advance_snapshot_smoke_dependents(&mut self) {
        // This synchronous fixture blocks normal frames. Drive their real dependency order:
        // migrations can enqueue fresh CPU-cache work after the full snapshot collider import.
        self.contree_builder
            .poll_cpu_chunk_cache_jobs(self.tracer.camera_position(), VOXEL_DIM_PER_CHUNK);
        self.terrain_physics
            .process_terrain_collider_updates(&self.contree_builder);
        self.advance_water_terrain(false);
    }

    fn benchmark_garden_snapshots(&mut self, output: &Path) -> Result<()> {
        anyhow::ensure!(
            self.terrain_persistence.startup_load_requested(),
            "terrain benchmark requires --terrain-load"
        );
        let source = self.terrain_persistence.selected_path().to_owned();
        anyhow::ensure!(
            Path::new(&source) != output,
            "benchmark output must differ from fixture"
        );
        for pass in 0..4 {
            *self.terrain_persistence.snapshot_path_mut() = output.to_string_lossy().into_owned();
            let start = Instant::now();
            self.perform_runtime_terrain_save();
            let save_ms = start.elapsed().as_secs_f64() * 1000.;
            anyhow::ensure!(
                self.terrain_persistence.status == TerrainPersistenceStatus::Ready,
                "benchmark save failed"
            );
            self.verify_live_garden_file(output)?;
            *self.terrain_persistence.snapshot_path_mut() = source.clone();
            let start = Instant::now();
            self.perform_runtime_terrain_load();
            anyhow::ensure!(
                self.terrain_persistence.awaits_dependents(),
                "benchmark load failed"
            );
            let deadline = Instant::now() + std::time::Duration::from_secs(60);
            while !self.terrain_persistence.can_start_operation() {
                self.advance_snapshot_smoke_dependents();
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "benchmark water publication did not settle"
                );
                std::thread::yield_now();
            }
            let load_ms = start.elapsed().as_secs_f64() * 1000.;
            let terrain = self.verify_live_garden_file(Path::new(&source))?;
            log::info!("[TERRAIN_BENCH] pass={pass} warmup={} save_ms={save_ms:.2} load_ready_ms={load_ms:.2} terrain={terrain} flora=exact trees=exact", pass == 0);
        }
        Ok(())
    }

    fn verify_live_garden_file(&mut self, path: &Path) -> Result<&'static str> {
        self.vulkan_ctx.device().wait_idle();
        let mut reader = TerrainSnapshotReader::open(path)?;
        let saved = GardenSnapshot::decode(reader.garden_data())?;
        let actual = self
            .surface_builder
            .capture_flora_snapshot(self.time_info.time_since_start_duration().as_millis() as u32)?;
        anyhow::ensure!(
            saved.flora.same_layout_and_growth(&actual),
            "restored flora layout/growth/identity differs"
        );
        anyhow::ensure!(
            saved.flora.same_water_storage(&actual),
            "restored plant hydration or root water reserve differs"
        );
        anyhow::ensure!(
            saved.trees.current_species() == self.capture_tree_snapshot(),
            "restored tree identity/shape/age differs"
        );
        self.verify_restored_tree_rendering(&saved.trees)?;
        if saved.trees.has_retired_trees() {
            self.verify_retired_tree_terrain(&saved.trees, &mut reader)?;
            return Ok("migrated");
        }
        while let Some(chunk) = reader.read_next_chunk()? {
            let bytes = self.plain_builder.read_chunk_atlas_region(
                UVec3::from_array(chunk.coordinate) * VOXEL_DIM_PER_CHUNK,
                VOXEL_DIM_PER_CHUNK,
            )?;
            anyhow::ensure!(
                bytes == chunk.bytes,
                "loaded terrain bytes differ in chunk {:?}",
                chunk.coordinate
            );
        }
        Ok("exact")
    }
}
