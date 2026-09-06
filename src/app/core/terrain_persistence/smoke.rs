//! Opt-in production-path acceptance, not a normal test or gameplay behavior.
use super::*;
use crate::app::world_edits::{TerrainBrushEdit, TreeAddOptions, TreePlacement};

impl App {
    pub(in crate::app::core) fn seed_garden_snapshot_smoke(&mut self) -> Result<()> {
        if std::env::var("RE_FLORA_GARDEN_SNAPSHOT_SMOKE").as_deref() != Ok("seed") {
            return Ok(());
        }
        let previous_selection = self.player_tools.flora_paint_selection_index;
        for index in 0..4 {
            let xz = Vec2::new((50 + index * 40) as f32 / 256.0, 70.0 / 256.0);
            let center = Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y);
            self.player_tools.flora_paint_selection_index = index as usize;
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
        self.player_tools.flora_paint_selection_index = previous_selection;
        self.debug_settings.adjustables.tree_age.value = 0.43;
        self.update_all_tree_ages_from_gui()?;
        self.add_snapshot_smoke_tree()?;
        log::info!("[GARDEN_SMOKE] seeded painted grass, all three authored species, tuned and additional tree at age 0.43");
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
                self.advance_water_terrain(false);
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "garden load water publication did not settle"
                );
                std::thread::yield_now();
            }
            self.verify_live_garden_file(Path::new(&path))?;
            log::info!("[GARDEN_SMOKE] runtime replacement pass={} terrain=exact flora=exact trees=exact no_duplicates=true", pass + 1);
        }
        log::info!("[GARDEN_SMOKE] passed startup_and_repeated_runtime_load=true");
        Ok(())
    }

    fn verify_live_garden_file(&mut self, path: &Path) -> Result<()> {
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
            saved.trees == self.capture_tree_snapshot(),
            "restored tree identity/shape/age differs"
        );
        self.verify_restored_tree_rendering(&saved.trees)?;
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
        Ok(())
    }
}
