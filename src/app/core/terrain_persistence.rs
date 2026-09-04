use super::*;
use crate::terrain_persistence::{
    TerrainSnapshotMetadata, TerrainSnapshotReader, TerrainSnapshotWriter,
    DEFAULT_TERRAIN_SNAPSHOT_PATH,
};
use std::path::Path;

const GLASS_EXPERIMENT_PERSISTENCE_DISABLED_REASON: &str =
    "Glass voxel experiment cannot be persisted";

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerrainPersistenceStatus {
    Ready,
    Saving,
    Loading,
    PublishedAwaitingDependents,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerrainSimulationGate {
    Running,
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
    disabled_reason: Option<&'static str>,
    status: TerrainPersistenceStatus,
    simulation_gate: TerrainSimulationGate,
}

impl TerrainPersistenceRuntime {
    pub(super) fn from_plan(
        options: &crate::TerrainPersistencePlan,
        glass_experiment_enabled: bool,
    ) -> Result<Self> {
        let metadata = terrain_snapshot_metadata();
        let startup_reader = options
            .load_path
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
            startup_load_requested: options.load_path.is_some(),
            startup_save_path: options.save_path.clone(),
            snapshot_path: options
                .load_path
                .clone()
                .or_else(|| options.save_path.clone())
                .unwrap_or_else(|| DEFAULT_TERRAIN_SNAPSHOT_PATH.to_owned()),
            disabled_reason: glass_experiment_enabled
                .then_some(GLASS_EXPERIMENT_PERSISTENCE_DISABLED_REASON),
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
        self.disabled_reason.is_none()
            && self.status == TerrainPersistenceStatus::Ready
            && self.simulation_gate == TerrainSimulationGate::Running
    }

    pub(super) fn status_label(&self) -> String {
        if let Some(reason) = self.disabled_reason {
            return format!("Disabled: {reason}");
        }
        match &self.status {
            TerrainPersistenceStatus::Ready => match self.simulation_gate {
                TerrainSimulationGate::Running => "Ready".to_owned(),
                TerrainSimulationGate::Frozen => "Error: restart required".to_owned(),
            },
            TerrainPersistenceStatus::Saving => "Saving".to_owned(),
            TerrainPersistenceStatus::Loading => "Loading".to_owned(),
            TerrainPersistenceStatus::PublishedAwaitingDependents => {
                "Ready (waiting for water terrain)".to_owned()
            }
            TerrainPersistenceStatus::Error(error) => format!("Error: {error}"),
        }
    }

    pub(super) fn allows_world_updates(&self) -> bool {
        self.simulation_gate != TerrainSimulationGate::Frozen
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

    fn finish_load(&mut self, failure: Option<(bool, String)>) {
        match failure {
            None => {
                self.status = TerrainPersistenceStatus::PublishedAwaitingDependents;
                self.simulation_gate = TerrainSimulationGate::Running;
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

    pub(in crate::app::core) fn complete_published_load(
        &mut self,
        _event: water::WaterPublicationResumed,
    ) {
        assert_eq!(
            self.status,
            TerrainPersistenceStatus::PublishedAwaitingDependents,
            "water resumed without a published terrain load awaiting dependents"
        );
        self.status = TerrainPersistenceStatus::Ready;
    }

    #[cfg(test)]
    pub(in crate::app::core) fn published_awaiting_dependents_for_test() -> Self {
        Self {
            startup_reader: None,
            startup_load_requested: false,
            startup_save_path: None,
            snapshot_path: DEFAULT_TERRAIN_SNAPSHOT_PATH.to_owned(),
            disabled_reason: None,
            status: TerrainPersistenceStatus::PublishedAwaitingDependents,
            simulation_gate: TerrainSimulationGate::Running,
        }
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

    fn run_terrain_save(&mut self, path: &Path) -> Result<()> {
        let result = (|| {
            self.quiesce_terrain_for_snapshot()?;
            self.write_terrain_snapshot(path)
        })();
        if self.water.phase() == water::WaterPhase::Quiesced {
            let resumed = self.water.resume_after_snapshot_read();
            debug_assert!(resumed, "snapshot save must release its water quiescence");
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
        if let Err(error) = self.quiesce_terrain_for_snapshot() {
            self.water.resume_after_snapshot_read();
            return Err(TerrainLoadFailure::before_mutation(error));
        }

        let mut mutated = false;
        let upload_result = (|| -> Result<()> {
            while let Some(chunk) = reader.read_next_chunk()? {
                let chunk_id = UVec3::from_array(chunk.coordinate);
                if !mutated {
                    if let Err(error) = self.water.snapshot_mutation_started() {
                        self.water.resume_after_snapshot_read();
                        return Err(error);
                    }
                    mutated = true;
                }
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
            if mutated {
                self.water.retain_quiescence_after_publication_failure();
            } else {
                self.water.resume_after_snapshot_read();
            }
            return Err(TerrainLoadFailure { mutated, error });
        }

        self.publish_snapshot_replacement().map_err(|error| {
            self.water.retain_quiescence_after_publication_failure();
            TerrainLoadFailure::after_mutation(error)
        })
    }

    fn quiesce_terrain_for_snapshot(&mut self) -> Result<()> {
        self.water.quiesce_for_snapshot()?;
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
            self.water.phase() == water::WaterPhase::Quiesced,
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
            disabled_reason: None,
            status: TerrainPersistenceStatus::Ready,
            simulation_gate: TerrainSimulationGate::Running,
        }
    }

    #[test]
    fn glass_voxel_experiment_disables_runtime_persistence_without_freezing_the_world() {
        let options = crate::TerrainPersistencePlan::default();
        let mut runtime = TerrainPersistenceRuntime::from_plan(&options, true).unwrap();

        assert!(!runtime.can_start_operation());
        assert!(!runtime.begin_save());
        assert!(runtime.allows_world_updates());
        assert_eq!(runtime.simulation_gate, TerrainSimulationGate::Running);
        assert_eq!(
            runtime.status_label(),
            "Disabled: Glass voxel experiment cannot be persisted",
        );
    }

    #[test]
    fn successful_load_waits_for_water_without_freezing_world_updates() {
        let mut runtime = runtime();
        assert!(runtime.begin_load());
        runtime.finish_load(None);

        assert!(runtime.allows_world_updates());
        assert!(!runtime.can_start_operation());
        assert_eq!(
            runtime.status,
            TerrainPersistenceStatus::PublishedAwaitingDependents
        );
    }

    #[test]
    fn load_failure_before_mutation_resumes_but_failure_after_mutation_freezes() {
        let mut recoverable = runtime();
        assert!(recoverable.begin_load());
        recoverable.finish_load(Some((false, "invalid snapshot".to_owned())));
        assert!(recoverable.allows_world_updates());

        let mut fatal = runtime();
        assert!(fatal.begin_load());
        fatal.finish_load(Some((true, "publication failed".to_owned())));
        assert!(!fatal.allows_world_updates());
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
