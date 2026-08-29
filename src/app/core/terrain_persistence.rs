use super::*;
use crate::terrain_persistence::{
    TerrainSnapshotMetadata, TerrainSnapshotReader, TerrainSnapshotWriter,
    DEFAULT_TERRAIN_SNAPSHOT_PATH,
};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerrainPersistenceStatus {
    Ready,
    Saving,
    Loading,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerrainSimulationGate {
    Running,
    WaterPaused,
    Frozen,
}

/// Owns terrain-persistence policy while the concrete `App` adapter supplies GPU and world access.
///
/// The interface exposes semantic readiness instead of the mutation point, fatality flag, and
/// water-resumption bookkeeping that make up the implementation.
pub(super) struct TerrainPersistenceRuntime {
    startup_reader: Option<TerrainSnapshotReader>,
    startup_load_requested: bool,
    startup_save_path: Option<String>,
    snapshot_path: String,
    status: TerrainPersistenceStatus,
    simulation_gate: TerrainSimulationGate,
}

impl TerrainPersistenceRuntime {
    pub(super) fn from_options(options: &crate::AppOptions) -> Result<Self> {
        let metadata = terrain_snapshot_metadata();
        let startup_reader = options
            .terrain_load_path
            .as_deref()
            .map(|path| -> Result<TerrainSnapshotReader> {
                TerrainSnapshotReader::validate(path, metadata)
                    .with_context(|| format!("validate terrain snapshot {path}"))?;
                let reader = TerrainSnapshotReader::open(path)
                    .with_context(|| format!("open terrain snapshot {path}"))?;
                log::info!(
                    "[TERRAIN_PERSISTENCE] startup load validated path={} chunks={} bytes={}",
                    path,
                    metadata.chunk_count()?,
                    metadata.chunk_count()? * metadata.chunk_byte_len()?
                );
                Ok(reader)
            })
            .transpose()?;

        Ok(Self {
            startup_reader,
            startup_load_requested: options.terrain_load_path.is_some(),
            startup_save_path: options.terrain_save_path.clone(),
            snapshot_path: options
                .terrain_load_path
                .clone()
                .or_else(|| options.terrain_save_path.clone())
                .unwrap_or_else(|| DEFAULT_TERRAIN_SNAPSHOT_PATH.to_owned()),
            status: TerrainPersistenceStatus::Ready,
            simulation_gate: TerrainSimulationGate::Running,
        })
    }

    pub(super) fn take_startup_reader(&mut self) -> Option<TerrainSnapshotReader> {
        self.startup_reader.take()
    }

    pub(super) fn startup_load_requested(&self) -> bool {
        self.startup_load_requested
    }

    pub(super) fn take_startup_save_path(&mut self) -> Option<String> {
        self.startup_save_path.take()
    }

    pub(super) fn snapshot_path_mut(&mut self) -> &mut String {
        &mut self.snapshot_path
    }

    pub(super) fn can_start_operation(&self) -> bool {
        self.status == TerrainPersistenceStatus::Ready
            && self.simulation_gate == TerrainSimulationGate::Running
    }

    pub(super) fn status_label(&self) -> String {
        match &self.status {
            TerrainPersistenceStatus::Ready => match self.simulation_gate {
                TerrainSimulationGate::Running => "Ready".to_owned(),
                TerrainSimulationGate::WaterPaused => {
                    "Ready (waiting for water terrain)".to_owned()
                }
                TerrainSimulationGate::Frozen => "Error: restart required".to_owned(),
            },
            TerrainPersistenceStatus::Saving => "Saving".to_owned(),
            TerrainPersistenceStatus::Loading => "Loading".to_owned(),
            TerrainPersistenceStatus::Error(error) => format!("Error: {error}"),
        }
    }

    pub(super) fn allows_world_updates(&self) -> bool {
        self.simulation_gate != TerrainSimulationGate::Frozen
    }

    pub(super) fn allows_water_simulation(&self) -> bool {
        self.simulation_gate == TerrainSimulationGate::Running
    }

    fn selected_path(&self) -> &str {
        &self.snapshot_path
    }

    fn begin_save(&mut self) -> bool {
        if !self.can_start_operation() {
            return false;
        }
        self.status = TerrainPersistenceStatus::Saving;
        true
    }

    fn finish_save(&mut self, error: Option<String>) {
        self.simulation_gate = TerrainSimulationGate::Running;
        self.status = error.map_or(
            TerrainPersistenceStatus::Ready,
            TerrainPersistenceStatus::Error,
        );
    }

    fn begin_load(&mut self) -> bool {
        if !self.can_start_operation() {
            return false;
        }
        self.status = TerrainPersistenceStatus::Loading;
        true
    }

    fn mark_water_paused(&mut self) {
        debug_assert_ne!(self.simulation_gate, TerrainSimulationGate::Frozen);
        self.simulation_gate = TerrainSimulationGate::WaterPaused;
    }

    fn finish_load(&mut self, failure: Option<(bool, String)>) {
        match failure {
            None => {
                self.status = TerrainPersistenceStatus::Ready;
                self.simulation_gate = TerrainSimulationGate::WaterPaused;
            }
            Some((mutated, error)) => {
                self.status = TerrainPersistenceStatus::Error(error);
                self.simulation_gate = if mutated {
                    TerrainSimulationGate::Frozen
                } else {
                    TerrainSimulationGate::Running
                };
            }
        }
    }

    fn observe_water_terrain_ready(&mut self, ready: bool) -> bool {
        if ready
            && self.status == TerrainPersistenceStatus::Ready
            && self.simulation_gate == TerrainSimulationGate::WaterPaused
        {
            self.simulation_gate = TerrainSimulationGate::Running;
            return true;
        }
        false
    }
}

struct TerrainLoadFailure {
    mutated: bool,
    error: anyhow::Error,
}

impl TerrainLoadFailure {
    fn before_mutation(error: anyhow::Error) -> Self {
        Self {
            mutated: false,
            error,
        }
    }

    fn after_mutation(error: anyhow::Error) -> Self {
        Self {
            mutated: true,
            error,
        }
    }
}

impl App {
    pub(super) fn perform_startup_terrain_save(&mut self, path: &Path) -> Result<()> {
        anyhow::ensure!(
            self.terrain_persistence.begin_save(),
            "terrain persistence was not ready for startup save"
        );
        let result = self.run_terrain_save(path);
        self.terrain_persistence
            .finish_save(result.as_ref().err().map(ToString::to_string));
        result
    }

    pub(super) fn perform_runtime_terrain_save(&mut self) {
        if !self.terrain_persistence.begin_save() {
            return;
        }
        let path = self.terrain_persistence.selected_path().to_owned();
        let result = self.run_terrain_save(Path::new(&path));
        let error = result.as_ref().err().map(ToString::to_string);
        self.terrain_persistence
            .finish_save(error.map(|error| format!("save failed: {error}")));
        if let Err(err) = result {
            log::error!(
                "[TERRAIN_PERSISTENCE] runtime save failed path={}: {err:#}",
                path
            );
        }
    }

    pub(super) fn perform_runtime_terrain_load(&mut self) {
        if !self.terrain_persistence.begin_load() {
            return;
        }
        let path = self.terrain_persistence.selected_path().to_owned();
        match self.run_terrain_load(Path::new(&path)) {
            Ok(()) => {
                self.terrain_persistence.finish_load(None);
                log::info!(
                    "[TERRAIN_PERSISTENCE] runtime load complete path={}; non-terrain state retained",
                    path
                );
            }
            Err(failure) => {
                log::error!(
                    "[TERRAIN_PERSISTENCE] runtime load failed path={} mutated={} error={:#}",
                    path,
                    failure.mutated,
                    failure.error
                );
                let error = format!("load failed: {}", failure.error);
                self.terrain_persistence
                    .finish_load(Some((failure.mutated, error)));
            }
        }
    }

    pub(super) fn maybe_resume_terrain_persistence_water(&mut self) {
        let water_ready = self.water_terrain_status().is_ready();
        if self
            .terrain_persistence
            .observe_water_terrain_ready(water_ready)
        {
            log::info!("[TERRAIN_PERSISTENCE] water terrain cache Ready; water simulation resumed");
        }
    }

    fn run_terrain_save(&mut self, path: &Path) -> Result<()> {
        let result = (|| {
            self.quiesce_terrain_for_snapshot()?;
            self.write_terrain_snapshot(path)
        })();
        if self.terrain_persistence.simulation_gate != TerrainSimulationGate::Frozen {
            self.terrain_persistence.simulation_gate = TerrainSimulationGate::Running;
        }
        result
    }

    fn write_terrain_snapshot(&mut self, path: &Path) -> Result<()> {
        let start = Instant::now();
        let metadata = terrain_snapshot_metadata();
        log::info!(
            "[TERRAIN_PERSISTENCE] Saving path={} chunks={} chunk_bytes={} total_bytes={}",
            path.display(),
            metadata.chunk_count()?,
            metadata.chunk_byte_len()?,
            metadata.chunk_count()? * metadata.chunk_byte_len()?
        );
        let mut writer = TerrainSnapshotWriter::create(path, metadata)?;
        for x in 0..CHUNK_DIM.x {
            for y in 0..CHUNK_DIM.y {
                for z in 0..CHUNK_DIM.z {
                    let coordinate = UVec3::new(x, y, z);
                    let bytes = self.plain_builder.read_chunk_atlas_region(
                        coordinate * VOXEL_DIM_PER_CHUNK,
                        VOXEL_DIM_PER_CHUNK,
                    )?;
                    writer.write_chunk(coordinate.to_array(), &bytes)?;
                }
            }
        }
        let summary = writer.finish()?;
        log::info!(
            "[TERRAIN_PERSISTENCE] Save complete path={} chunks={} payload_bytes={} elapsed_ms={:.2}",
            path.display(),
            summary.chunk_count,
            summary.payload_bytes,
            start.elapsed().as_secs_f64() * 1000.0
        );
        Ok(())
    }

    fn run_terrain_load(&mut self, path: &Path) -> Result<(), TerrainLoadFailure> {
        let metadata = terrain_snapshot_metadata();
        TerrainSnapshotReader::validate(path, metadata)
            .map_err(TerrainLoadFailure::before_mutation)?;
        let mut reader =
            TerrainSnapshotReader::open(path).map_err(TerrainLoadFailure::before_mutation)?;
        self.quiesce_terrain_for_snapshot()
            .map_err(TerrainLoadFailure::before_mutation)?;

        let mut mutated = false;
        let upload_result = (|| -> Result<()> {
            while let Some(chunk) = reader.read_next_chunk()? {
                mutated = true;
                let chunk_id = UVec3::from_array(chunk.coordinate);
                self.plain_builder.write_chunk_atlas_region(
                    chunk_id * VOXEL_DIM_PER_CHUNK,
                    VOXEL_DIM_PER_CHUNK,
                    &chunk.bytes,
                )?;
            }
            reader.finish()?;
            Ok(())
        })();
        if let Err(error) = upload_result {
            return Err(TerrainLoadFailure { mutated, error });
        }

        self.publish_snapshot_replacement()
            .map_err(TerrainLoadFailure::after_mutation)
    }

    fn quiesce_terrain_for_snapshot(&mut self) -> Result<()> {
        self.water_sim.pause_and_wait()?;
        self.terrain_persistence.mark_water_paused();
        self.vulkan_ctx.device().wait_idle();
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        anyhow::ensure!(
            self.contree_builder.cpu_chunk_cache_jobs_idle(),
            "Contree CPU cache did not reach Ready before snapshot access"
        );
        log::info!(
            "[TERRAIN_PERSISTENCE] visible terrain quiesced; water worker pause acknowledged"
        );
        Ok(())
    }

    fn publish_snapshot_replacement(&mut self) -> Result<()> {
        anyhow::ensure!(
            !self.terrain_persistence.allows_water_simulation(),
            "snapshot replacement requires quiesced water simulation"
        );
        self.plain_builder.mark_all_solid_workgroups_dirty();
        let change =
            VisibleTerrainChange::from_build_edits(vec![snapshot_replacement_build_edit(
                terrain_chunk_ids(),
            )])?
            .context("snapshot replacement has no visible terrain chunks")?;
        let mut publication =
            visible_terrain::VisibleTerrainPublication::snapshot_replacement(change)?;
        publication.run_to_completion(self)?;
        self.player_tools.cancel_continuous_hold();
        Ok(())
    }
}

fn terrain_snapshot_metadata() -> TerrainSnapshotMetadata {
    TerrainSnapshotMetadata::new(CHUNK_DIM.to_array(), VOXEL_DIM_PER_CHUNK.to_array())
}

fn terrain_chunk_ids() -> Vec<UVec3> {
    let mut chunk_ids = Vec::new();
    for x in 0..CHUNK_DIM.x {
        for y in 0..CHUNK_DIM.y {
            for z in 0..CHUNK_DIM.z {
                chunk_ids.push(UVec3::new(x, y, z));
            }
        }
    }
    chunk_ids
}

fn snapshot_replacement_build_edit(chunk_ids: Vec<UVec3>) -> BuildEdit {
    BuildEdit::RebuildChunksWithoutFlora(chunk_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> TerrainPersistenceRuntime {
        TerrainPersistenceRuntime {
            startup_reader: None,
            startup_load_requested: false,
            startup_save_path: None,
            snapshot_path: DEFAULT_TERRAIN_SNAPSHOT_PATH.to_owned(),
            status: TerrainPersistenceStatus::Ready,
            simulation_gate: TerrainSimulationGate::Running,
        }
    }

    #[test]
    fn successful_load_waits_for_water_without_freezing_world_updates() {
        let mut runtime = runtime();
        assert!(runtime.begin_load());
        runtime.mark_water_paused();
        runtime.finish_load(None);

        assert!(runtime.allows_world_updates());
        assert!(!runtime.allows_water_simulation());
        assert!(!runtime.can_start_operation());
        assert!(!runtime.observe_water_terrain_ready(false));
        assert!(runtime.observe_water_terrain_ready(true));
        assert!(runtime.allows_water_simulation());
        assert!(runtime.can_start_operation());
    }

    #[test]
    fn load_failure_before_mutation_resumes_but_failure_after_mutation_freezes() {
        let mut recoverable = runtime();
        assert!(recoverable.begin_load());
        recoverable.mark_water_paused();
        recoverable.finish_load(Some((false, "invalid snapshot".to_owned())));
        assert!(recoverable.allows_world_updates());
        assert!(recoverable.allows_water_simulation());

        let mut fatal = runtime();
        assert!(fatal.begin_load());
        fatal.mark_water_paused();
        fatal.finish_load(Some((true, "publication failed".to_owned())));
        assert!(!fatal.allows_world_updates());
        assert!(!fatal.allows_water_simulation());
        assert!(!fatal.can_start_operation());
    }

    #[test]
    fn runtime_snapshot_replacement_does_not_regenerate_flora() {
        let chunk_ids = vec![UVec3::ZERO, UVec3::X];
        match snapshot_replacement_build_edit(chunk_ids.clone()) {
            BuildEdit::RebuildChunksWithoutFlora(actual) => assert_eq!(actual, chunk_ids),
            _ => panic!("runtime snapshot replacement must retain the current flora state"),
        }
    }
}
