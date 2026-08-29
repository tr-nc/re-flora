use super::*;
use std::collections::HashSet;

#[derive(Clone, Debug)]
enum VisibleTerrainRebuild {
    BuildEdits(Vec<BuildEdit>),
    PreserveFlora {
        bound: UAabb3,
        flora_edit: world_ops::FloraBrushEdit,
    },
}

/// A semantic request to make one authoritative terrain change fully visible.
///
/// The fields stay private so callers cannot select builder stages or downstream observers.
#[derive(Clone, Debug)]
pub(super) struct VisibleTerrainChange {
    rebuild: VisibleTerrainRebuild,
    affected_voxels: UAabb3,
    terrain_changed: bool,
}

impl VisibleTerrainChange {
    pub(super) fn from_build_edits(build_edits: Vec<BuildEdit>) -> Result<Option<Self>> {
        let mut affected_voxels: Option<UAabb3> = None;
        for edit in &build_edits {
            let edit_bound = build_edit_bound(edit)?;
            affected_voxels = Some(match affected_voxels {
                Some(bound) => bound.union_with(&edit_bound),
                None => edit_bound,
            });
        }
        let Some(affected_voxels) = affected_voxels else {
            return Ok(None);
        };
        Ok(Some(Self {
            rebuild: VisibleTerrainRebuild::BuildEdits(build_edits),
            affected_voxels,
            terrain_changed: true,
        }))
    }

    pub(super) fn tree_chunks(chunk_ids: Vec<UVec3>) -> Result<Self> {
        Self::from_build_edits(vec![BuildEdit::RebuildChunksWithoutFlora(chunk_ids)])?
            .context("tree publication requires at least one affected chunk")
    }

    pub(super) fn preserving_flora(
        bound: UAabb3,
        flora_edit: world_ops::FloraBrushEdit,
        terrain_changed: bool,
    ) -> Self {
        Self {
            rebuild: VisibleTerrainRebuild::PreserveFlora { bound, flora_edit },
            affected_voxels: bound,
            terrain_changed,
        }
    }

    fn affected_chunks(&self) -> Result<Vec<UVec3>> {
        let mut chunk_ids = match &self.rebuild {
            VisibleTerrainRebuild::BuildEdits(build_edits) => {
                let mut chunk_ids = Vec::new();
                for edit in build_edits {
                    chunk_ids.extend(build_edit_chunks(edit)?);
                }
                chunk_ids
            }
            VisibleTerrainRebuild::PreserveFlora { bound, .. } => {
                world_ops::affected_chunk_indices_for_bound(*bound, VOXEL_DIM_PER_CHUNK)
            }
        };
        let mut seen = HashSet::new();
        chunk_ids.retain(|chunk_id| seen.insert(*chunk_id));
        anyhow::ensure!(
            !chunk_ids.is_empty(),
            "visible terrain publication requires at least one affected chunk"
        );
        Ok(chunk_ids)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisibleTerrainPublicationKind {
    Edit,
    Startup { reconcile_loaded_terrain: bool },
    SnapshotReplacement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisibleTerrainPublicationState {
    Physical,
    EditObservers,
    LoadedConnectivity,
    BeginWorldColliders,
    ImportWorldColliders,
    AwaitingStartupSettlement,
    StartupObservers,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VisibleTerrainCompletion {
    chunks: usize,
    visible_revision: u32,
    changed_revision: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VisibleTerrainPublicationProgress {
    Preparing {
        prepared_chunks: usize,
        total_chunks: usize,
    },
    ImportingColliders {
        completed: usize,
        total: usize,
    },
    AwaitingStartupSettlement,
    Complete(VisibleTerrainCompletion),
}

pub(super) trait VisibleTerrainPublicationHost {
    fn advance_physical(
        &mut self,
        publication: &mut physical_visible_terrain::PhysicalTerrainPublication,
    ) -> Result<physical_visible_terrain::PhysicalTerrainPublicationProgress>;
    fn emissive_observe_change(&mut self, affected_voxels: UAabb3) -> Result<()>;
    fn invalidate_direct_sun_shadows(&mut self);
    fn visible_terrain_revision(&self) -> u32;
    fn mark_terrain_colliders_dirty(&mut self, affected_voxels: UAabb3);
    fn ddgi_observe_visible_terrain(
        &mut self,
        revision: u32,
        affected_voxels: UAabb3,
    ) -> Result<()>;
    fn commit_visible_terrain_revision(&mut self, revision: u32);
    fn reconcile_loaded_terrain(&mut self) -> Result<()>;
    fn prepare_snapshot_world_collider_import(&mut self) -> Result<()>;
    fn begin_world_collider_import(&mut self) -> Result<usize>;
    fn advance_world_collider_import(&mut self) -> Result<(usize, usize)>;
    fn enqueue_startup_water_terrain(&mut self);
    fn observe_initial_terrain_for_ddgi(&mut self) -> Result<u32>;
}

/// Owns one complete Visible Terrain Publication across synchronous edits and incremental loading.
///
/// Physical rebuilding, semantic completion, and observer ordering stay behind this interface.
/// `PhysicalTerrainPublication` remains the concrete deep implementation of the physical phase.
pub(super) struct VisibleTerrainPublication {
    kind: VisibleTerrainPublicationKind,
    physical: Vec<physical_visible_terrain::PhysicalTerrainPublication>,
    physical_index: usize,
    physically_published_chunks: usize,
    chunk_count: usize,
    affected_voxels: UAabb3,
    terrain_changed: bool,
    changed_revision: Option<u32>,
    collider_total: usize,
    state: VisibleTerrainPublicationState,
    completion: Option<VisibleTerrainCompletion>,
    started_at: Instant,
}

impl VisibleTerrainPublication {
    fn edit(change: VisibleTerrainChange) -> Result<Self> {
        Self::from_change(change, VisibleTerrainPublicationKind::Edit)
    }

    pub(super) fn snapshot_replacement(change: VisibleTerrainChange) -> Result<Self> {
        Self::from_change(change, VisibleTerrainPublicationKind::SnapshotReplacement)
    }

    fn from_change(
        change: VisibleTerrainChange,
        kind: VisibleTerrainPublicationKind,
    ) -> Result<Self> {
        let chunk_ids = change.affected_chunks()?;
        let physical = match change.rebuild {
            VisibleTerrainRebuild::BuildEdits(build_edits) => build_edits
                .into_iter()
                .map(|edit| {
                    physical_visible_terrain::PhysicalTerrainPublication::from_build_edit(
                        edit,
                        VOXEL_DIM_PER_CHUNK,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
            VisibleTerrainRebuild::PreserveFlora { bound, flora_edit } => {
                vec![
                    physical_visible_terrain::PhysicalTerrainPublication::preserving_flora(
                        bound,
                        flora_edit,
                        VOXEL_DIM_PER_CHUNK,
                    )?,
                ]
            }
        };
        Ok(Self::new(
            kind,
            physical,
            chunk_ids.len(),
            change.affected_voxels,
            change.terrain_changed,
        ))
    }

    pub(super) fn startup(chunk_ids: Vec<UVec3>, loaded_snapshot: bool) -> Result<Self> {
        let affected_voxels = chunk_bound(&chunk_ids)?;
        let chunk_count = chunk_ids.len();
        let physical = vec![
            physical_visible_terrain::PhysicalTerrainPublication::loading(
                chunk_ids,
                VOXEL_DIM_PER_CHUNK,
            )?,
        ];
        Ok(Self::new(
            VisibleTerrainPublicationKind::Startup {
                reconcile_loaded_terrain: loaded_snapshot,
            },
            physical,
            chunk_count,
            affected_voxels,
            false,
        ))
    }

    fn new(
        kind: VisibleTerrainPublicationKind,
        physical: Vec<physical_visible_terrain::PhysicalTerrainPublication>,
        chunk_count: usize,
        affected_voxels: UAabb3,
        terrain_changed: bool,
    ) -> Self {
        debug_assert!(!physical.is_empty());
        Self {
            kind,
            physical,
            physical_index: 0,
            physically_published_chunks: 0,
            chunk_count,
            affected_voxels,
            terrain_changed,
            changed_revision: None,
            collider_total: 0,
            state: VisibleTerrainPublicationState::Physical,
            completion: None,
            started_at: Instant::now(),
        }
    }

    pub(super) fn advance(
        &mut self,
        host: &mut impl VisibleTerrainPublicationHost,
    ) -> Result<VisibleTerrainPublicationProgress> {
        if self.state == VisibleTerrainPublicationState::Failed {
            anyhow::bail!("visible terrain publication is terminally failed");
        }
        let result = self.advance_inner(host);
        if result.is_err() {
            self.state = VisibleTerrainPublicationState::Failed;
        }
        result
    }

    fn advance_inner(
        &mut self,
        host: &mut impl VisibleTerrainPublicationHost,
    ) -> Result<VisibleTerrainPublicationProgress> {
        loop {
            match self.state {
                VisibleTerrainPublicationState::Physical => {
                    let publication = &mut self.physical[self.physical_index];
                    match host.advance_physical(publication)? {
                        physical_visible_terrain::PhysicalTerrainPublicationProgress::Preparing {
                            prepared_chunks,
                            ..
                        } => {
                            return Ok(VisibleTerrainPublicationProgress::Preparing {
                                prepared_chunks: self.physically_published_chunks + prepared_chunks,
                                total_chunks: self.chunk_count,
                            });
                        }
                        physical_visible_terrain::PhysicalTerrainPublicationProgress::Published {
                            chunks,
                        } => {
                            self.physically_published_chunks += chunks;
                            self.physical_index += 1;
                            if self.physical_index < self.physical.len() {
                                return Ok(VisibleTerrainPublicationProgress::Preparing {
                                    prepared_chunks: self.physically_published_chunks,
                                    total_chunks: self.chunk_count,
                                });
                            }
                            self.state = match self.kind {
                                VisibleTerrainPublicationKind::Edit
                                | VisibleTerrainPublicationKind::SnapshotReplacement => {
                                    VisibleTerrainPublicationState::EditObservers
                                }
                                VisibleTerrainPublicationKind::Startup {
                                    reconcile_loaded_terrain: true,
                                } => VisibleTerrainPublicationState::LoadedConnectivity,
                                VisibleTerrainPublicationKind::Startup {
                                    reconcile_loaded_terrain: false,
                                } => VisibleTerrainPublicationState::BeginWorldColliders,
                            };
                        }
                    }
                }
                VisibleTerrainPublicationState::EditObservers => {
                    self.publish_edit_observers(host)?;
                    self.state = match self.kind {
                        VisibleTerrainPublicationKind::Edit => {
                            VisibleTerrainPublicationState::Complete
                        }
                        VisibleTerrainPublicationKind::SnapshotReplacement => {
                            VisibleTerrainPublicationState::LoadedConnectivity
                        }
                        VisibleTerrainPublicationKind::Startup { .. } => {
                            unreachable!("startup publication does not run edit observers")
                        }
                    };
                }
                VisibleTerrainPublicationState::LoadedConnectivity => {
                    host.reconcile_loaded_terrain()?;
                    if self.kind == VisibleTerrainPublicationKind::SnapshotReplacement {
                        host.prepare_snapshot_world_collider_import()?;
                    }
                    self.state = VisibleTerrainPublicationState::BeginWorldColliders;
                }
                VisibleTerrainPublicationState::BeginWorldColliders => {
                    self.collider_total = host.begin_world_collider_import()?;
                    self.state = VisibleTerrainPublicationState::ImportWorldColliders;
                    return Ok(VisibleTerrainPublicationProgress::ImportingColliders {
                        completed: 0,
                        total: self.collider_total,
                    });
                }
                VisibleTerrainPublicationState::ImportWorldColliders => {
                    let (completed, total) = host.advance_world_collider_import()?;
                    anyhow::ensure!(
                        total == self.collider_total,
                        "world collider import total changed from {} to {total}",
                        self.collider_total
                    );
                    if completed < total {
                        return Ok(VisibleTerrainPublicationProgress::ImportingColliders {
                            completed,
                            total,
                        });
                    }
                    self.state = match self.kind {
                        VisibleTerrainPublicationKind::Startup { .. } => {
                            VisibleTerrainPublicationState::AwaitingStartupSettlement
                        }
                        VisibleTerrainPublicationKind::SnapshotReplacement => {
                            VisibleTerrainPublicationState::StartupObservers
                        }
                        VisibleTerrainPublicationKind::Edit => {
                            unreachable!("edit publication does not import world colliders")
                        }
                    };
                }
                VisibleTerrainPublicationState::AwaitingStartupSettlement => {
                    return Ok(VisibleTerrainPublicationProgress::AwaitingStartupSettlement);
                }
                VisibleTerrainPublicationState::StartupObservers => {
                    host.enqueue_startup_water_terrain();
                    if matches!(self.kind, VisibleTerrainPublicationKind::Startup { .. }) {
                        host.observe_initial_terrain_for_ddgi()?;
                    }
                    self.state = VisibleTerrainPublicationState::Complete;
                }
                VisibleTerrainPublicationState::Complete => {
                    return Ok(VisibleTerrainPublicationProgress::Complete(
                        self.completion(host.visible_terrain_revision()),
                    ));
                }
                VisibleTerrainPublicationState::Failed => unreachable!(),
            }
        }
    }

    fn publish_edit_observers(
        &mut self,
        host: &mut impl VisibleTerrainPublicationHost,
    ) -> Result<()> {
        if self.terrain_changed {
            host.emissive_observe_change(self.affected_voxels)?;
        }
        host.invalidate_direct_sun_shadows();
        let revision =
            next_visible_terrain_revision(host.visible_terrain_revision(), self.terrain_changed);
        if let Some(revision) = revision {
            host.mark_terrain_colliders_dirty(self.affected_voxels);
            host.ddgi_observe_visible_terrain(revision, self.affected_voxels)?;
            host.commit_visible_terrain_revision(revision);
        }
        self.changed_revision = revision;
        Ok(())
    }

    pub(super) fn complete_startup(
        &mut self,
        host: &mut impl VisibleTerrainPublicationHost,
    ) -> Result<VisibleTerrainCompletion> {
        anyhow::ensure!(
            self.state == VisibleTerrainPublicationState::AwaitingStartupSettlement,
            "startup terrain can settle only after physical and collider publication"
        );
        self.state = VisibleTerrainPublicationState::StartupObservers;
        match self.advance(host)? {
            VisibleTerrainPublicationProgress::Complete(completion) => Ok(completion),
            _ => unreachable!("startup observers complete synchronously"),
        }
    }

    pub(super) fn run_to_completion(
        &mut self,
        host: &mut impl VisibleTerrainPublicationHost,
    ) -> Result<VisibleTerrainCompletion> {
        loop {
            match self.advance(host)? {
                VisibleTerrainPublicationProgress::Preparing { .. }
                | VisibleTerrainPublicationProgress::ImportingColliders { .. } => {}
                VisibleTerrainPublicationProgress::AwaitingStartupSettlement => {
                    return self.complete_startup(host);
                }
                VisibleTerrainPublicationProgress::Complete(completion) => return Ok(completion),
            }
        }
    }

    pub(super) fn abort(&mut self, contree_builder: &mut ContreeBuilder) {
        if self.state == VisibleTerrainPublicationState::Complete {
            return;
        }
        for publication in &mut self.physical[self.physical_index..] {
            publication.abort(contree_builder);
        }
        self.state = VisibleTerrainPublicationState::Failed;
    }

    fn completion(&mut self, visible_revision: u32) -> VisibleTerrainCompletion {
        if let Some(completion) = self.completion {
            return completion;
        }
        let completion = VisibleTerrainCompletion {
            chunks: self.chunk_count,
            visible_revision,
            changed_revision: self.changed_revision,
        };
        self.completion = Some(completion);
        log::info!(
            "[PERF][VISIBLE_TERRAIN_PUBLICATION] chunks={} terrain_changed={} revision={:?} elapsed_ms={:.2}",
            completion.chunks,
            self.terrain_changed,
            completion.changed_revision,
            self.started_at.elapsed().as_secs_f64() * 1000.0,
        );
        completion
    }
}

impl VisibleTerrainPublicationHost for App {
    fn advance_physical(
        &mut self,
        publication: &mut physical_visible_terrain::PhysicalTerrainPublication,
    ) -> Result<physical_visible_terrain::PhysicalTerrainPublicationProgress> {
        publication.advance(physical_visible_terrain::PhysicalTerrainBuilders::new(
            &mut self.surface_builder,
            &mut self.contree_builder,
            &mut self.scene_accel_builder,
        ))
    }

    fn emissive_observe_change(&mut self, affected_voxels: UAabb3) -> Result<()> {
        if let Some(runtime) = self.emissive_voxel_lighting.as_mut() {
            runtime
                .mark_trusted_change(affected_voxels, self.time_info.total_frame_count())
                .map(|_| ())
        } else {
            Ok(())
        }
    }

    fn invalidate_direct_sun_shadows(&mut self) {
        self.tracer.invalidate_local_direct_sun_shadow_histories();
    }

    fn visible_terrain_revision(&self) -> u32 {
        self.visible_terrain_revision
    }

    fn mark_terrain_colliders_dirty(&mut self, affected_voxels: UAabb3) {
        self.terrain_physics
            .mark_terrain_voxels_dirty(affected_voxels);
    }

    fn ddgi_observe_visible_terrain(
        &mut self,
        revision: u32,
        affected_voxels: UAabb3,
    ) -> Result<()> {
        self.tracer
            .observe_published_environment_probe_terrain(revision, affected_voxels)
    }

    fn commit_visible_terrain_revision(&mut self, revision: u32) {
        self.visible_terrain_revision = revision;
    }

    fn reconcile_loaded_terrain(&mut self) -> Result<()> {
        self.reconcile_loaded_terrain_publication()
    }

    fn prepare_snapshot_world_collider_import(&mut self) -> Result<()> {
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        anyhow::ensure!(
            self.contree_builder.cpu_chunk_cache_jobs_idle(),
            "Contree CPU cache did not reach Ready after snapshot publication"
        );
        Ok(())
    }

    fn begin_world_collider_import(&mut self) -> Result<usize> {
        self.terrain_physics
            .begin_world_terrain_collider_import(CHUNK_DIM * VOXEL_DIM_PER_CHUNK)
    }

    fn advance_world_collider_import(&mut self) -> Result<(usize, usize)> {
        self.terrain_physics
            .process_world_terrain_collider_import(&self.contree_builder)
    }

    fn enqueue_startup_water_terrain(&mut self) {
        self.enqueue_startup_water_terrain_collider_rebuilds();
    }

    fn observe_initial_terrain_for_ddgi(&mut self) -> Result<u32> {
        if self.environment_lighting_test_scene.is_none()
            && self.hybrid_transparency_test_scene.is_none()
        {
            self.observe_initial_published_terrain_for_ddgi()
        } else {
            Ok(self.visible_terrain_revision)
        }
    }
}

impl App {
    pub(super) fn publish_visible_terrain(&mut self, change: VisibleTerrainChange) -> Result<()> {
        let mut publication = VisibleTerrainPublication::edit(change)?;
        publication.run_to_completion(self).unwrap_or_else(|err| {
            panic!(
                "Visible Terrain Publication failed after entering non-rollbackable state: {err:#}"
            )
        });
        Ok(())
    }
}

fn chunk_bound(chunk_ids: &[UVec3]) -> Result<UAabb3> {
    let min_chunk = chunk_ids
        .iter()
        .copied()
        .reduce(UVec3::min)
        .context("visible terrain publication requires at least one affected chunk")?;
    let max_chunk = chunk_ids
        .iter()
        .copied()
        .reduce(UVec3::max)
        .expect("a minimum chunk implies a maximum chunk");
    Ok(UAabb3::new(
        min_chunk * VOXEL_DIM_PER_CHUNK,
        (max_chunk + UVec3::ONE) * VOXEL_DIM_PER_CHUNK,
    ))
}

fn build_edit_chunks(edit: &BuildEdit) -> Result<Vec<UVec3>> {
    let chunk_ids = match edit {
        BuildEdit::RebuildMesh(bound) | BuildEdit::RebuildMeshWithoutFlora(bound) => {
            world_ops::affected_chunk_indices_for_bound(*bound, VOXEL_DIM_PER_CHUNK)
        }
        BuildEdit::RebuildChunks(chunk_ids) | BuildEdit::RebuildChunksWithoutFlora(chunk_ids) => {
            chunk_ids.clone()
        }
    };
    anyhow::ensure!(
        !chunk_ids.is_empty(),
        "visible terrain build edit requires at least one affected chunk"
    );
    Ok(chunk_ids)
}

fn build_edit_bound(edit: &BuildEdit) -> Result<UAabb3> {
    match edit {
        BuildEdit::RebuildMesh(bound) | BuildEdit::RebuildMeshWithoutFlora(bound) => Ok(*bound),
        BuildEdit::RebuildChunks(chunk_ids) | BuildEdit::RebuildChunksWithoutFlora(chunk_ids) => {
            chunk_bound(chunk_ids)
        }
    }
}

fn next_visible_terrain_revision(current: u32, terrain_changed: bool) -> Option<u32> {
    terrain_changed.then(|| current.wrapping_add(1).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Event {
        PhysicalPreparing,
        PhysicalComplete,
        Emissive,
        Shadow,
        CollidersDirty,
        Ddgi(u32),
        Revision(u32),
        Connectivity,
        ChildPublicationComplete,
        SnapshotCollidersReady,
        ColliderBegin,
        ColliderComplete,
        Water,
        InitialDdgi,
    }

    struct RecordingHost {
        physical: VecDeque<physical_visible_terrain::PhysicalTerrainPublicationProgress>,
        events: Vec<Event>,
        revision: u32,
        connectivity_child: bool,
    }

    impl RecordingHost {
        fn new(
            physical: impl IntoIterator<
                Item = physical_visible_terrain::PhysicalTerrainPublicationProgress,
            >,
        ) -> Self {
            Self {
                physical: physical.into_iter().collect(),
                events: Vec::new(),
                revision: 7,
                connectivity_child: false,
            }
        }

        fn with_connectivity_child(mut self) -> Self {
            self.connectivity_child = true;
            self
        }
    }

    impl VisibleTerrainPublicationHost for RecordingHost {
        fn advance_physical(
            &mut self,
            _publication: &mut physical_visible_terrain::PhysicalTerrainPublication,
        ) -> Result<physical_visible_terrain::PhysicalTerrainPublicationProgress> {
            let progress = self
                .physical
                .pop_front()
                .expect("test physical progress exhausted");
            self.events.push(match progress {
                physical_visible_terrain::PhysicalTerrainPublicationProgress::Preparing {
                    ..
                } => Event::PhysicalPreparing,
                physical_visible_terrain::PhysicalTerrainPublicationProgress::Published {
                    ..
                } => Event::PhysicalComplete,
            });
            Ok(progress)
        }

        fn emissive_observe_change(&mut self, _affected_voxels: UAabb3) -> Result<()> {
            self.events.push(Event::Emissive);
            Ok(())
        }

        fn invalidate_direct_sun_shadows(&mut self) {
            self.events.push(Event::Shadow);
        }

        fn visible_terrain_revision(&self) -> u32 {
            self.revision
        }

        fn mark_terrain_colliders_dirty(&mut self, _affected_voxels: UAabb3) {
            self.events.push(Event::CollidersDirty);
        }

        fn ddgi_observe_visible_terrain(
            &mut self,
            revision: u32,
            _affected_voxels: UAabb3,
        ) -> Result<()> {
            self.events.push(Event::Ddgi(revision));
            Ok(())
        }

        fn commit_visible_terrain_revision(&mut self, revision: u32) {
            self.events.push(Event::Revision(revision));
            self.revision = revision;
        }

        fn reconcile_loaded_terrain(&mut self) -> Result<()> {
            self.events.push(Event::Connectivity);
            if self.connectivity_child {
                self.events.push(Event::ChildPublicationComplete);
            }
            Ok(())
        }

        fn prepare_snapshot_world_collider_import(&mut self) -> Result<()> {
            self.events.push(Event::SnapshotCollidersReady);
            Ok(())
        }

        fn begin_world_collider_import(&mut self) -> Result<usize> {
            self.events.push(Event::ColliderBegin);
            Ok(1)
        }

        fn advance_world_collider_import(&mut self) -> Result<(usize, usize)> {
            self.events.push(Event::ColliderComplete);
            Ok((1, 1))
        }

        fn enqueue_startup_water_terrain(&mut self) {
            self.events.push(Event::Water);
        }

        fn observe_initial_terrain_for_ddgi(&mut self) -> Result<u32> {
            self.events.push(Event::InitialDdgi);
            Ok(self.revision)
        }
    }

    fn published(chunks: usize) -> physical_visible_terrain::PhysicalTerrainPublicationProgress {
        physical_visible_terrain::PhysicalTerrainPublicationProgress::Published { chunks }
    }

    #[test]
    fn build_batch_has_one_combined_publication_bound_and_unique_chunks() {
        let first = UAabb3::new(UVec3::splat(8), UVec3::splat(16));
        let change = VisibleTerrainChange::from_build_edits(vec![
            BuildEdit::RebuildMesh(first),
            BuildEdit::RebuildChunks(vec![UVec3::new(1, 0, 0), UVec3::new(1, 0, 0)]),
        ])
        .unwrap()
        .unwrap();

        assert_eq!(
            change.affected_voxels,
            first.union_with(&UAabb3::new(
                UVec3::new(1, 0, 0) * VOXEL_DIM_PER_CHUNK,
                UVec3::new(2, 1, 1) * VOXEL_DIM_PER_CHUNK,
            ))
        );
        let chunks = change.affected_chunks().unwrap();
        assert_eq!(
            chunks
                .iter()
                .filter(|&&id| id == UVec3::new(1, 0, 0))
                .count(),
            1
        );
    }

    #[test]
    fn synchronous_completion_follows_physical_and_every_observer() {
        let change = VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildChunks(vec![
            UVec3::ZERO,
        ])])
        .unwrap()
        .unwrap();
        let mut publication = VisibleTerrainPublication::edit(change).unwrap();
        let mut host = RecordingHost::new([published(1)]);

        let completion = publication.run_to_completion(&mut host).unwrap();

        assert_eq!(completion.changed_revision, Some(8));
        assert_eq!(completion.visible_revision, 8);
        assert_eq!(
            host.events,
            vec![
                Event::PhysicalComplete,
                Event::Emissive,
                Event::Shadow,
                Event::CollidersDirty,
                Event::Ddgi(8),
                Event::Revision(8),
            ]
        );
    }

    #[test]
    fn loaded_startup_reconciles_after_physical_and_before_collider_import() {
        let mut publication = VisibleTerrainPublication::startup(vec![UVec3::ZERO], true).unwrap();
        let mut host = RecordingHost::new([published(1)]);

        assert_eq!(
            publication.advance(&mut host).unwrap(),
            VisibleTerrainPublicationProgress::ImportingColliders {
                completed: 0,
                total: 1,
            }
        );
        assert_eq!(
            host.events,
            vec![
                Event::PhysicalComplete,
                Event::Connectivity,
                Event::ColliderBegin,
            ]
        );
        assert_eq!(
            publication.advance(&mut host).unwrap(),
            VisibleTerrainPublicationProgress::AwaitingStartupSettlement
        );
        let completion = publication.complete_startup(&mut host).unwrap();
        assert_eq!(completion.visible_revision, 7);
        assert_eq!(
            host.events,
            vec![
                Event::PhysicalComplete,
                Event::Connectivity,
                Event::ColliderBegin,
                Event::ColliderComplete,
                Event::Water,
                Event::InitialDdgi,
            ]
        );
    }

    #[test]
    fn loaded_connectivity_child_completes_before_collider_import() {
        let mut publication = VisibleTerrainPublication::startup(vec![UVec3::ZERO], true).unwrap();
        let mut host = RecordingHost::new([published(1)]).with_connectivity_child();

        publication.advance(&mut host).unwrap();

        assert_eq!(
            host.events,
            vec![
                Event::PhysicalComplete,
                Event::Connectivity,
                Event::ChildPublicationComplete,
                Event::ColliderBegin,
            ]
        );
    }

    #[test]
    fn snapshot_replacement_has_one_ordered_semantic_completion() {
        let change = VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildChunks(vec![
            UVec3::ZERO,
        ])])
        .unwrap()
        .unwrap();
        let mut publication = VisibleTerrainPublication::snapshot_replacement(change).unwrap();
        let mut host = RecordingHost::new([published(1)]);

        let completion = publication.run_to_completion(&mut host).unwrap();

        assert_eq!(completion.changed_revision, Some(8));
        assert_eq!(
            host.events,
            vec![
                Event::PhysicalComplete,
                Event::Emissive,
                Event::Shadow,
                Event::CollidersDirty,
                Event::Ddgi(8),
                Event::Revision(8),
                Event::Connectivity,
                Event::SnapshotCollidersReady,
                Event::ColliderBegin,
                Event::ColliderComplete,
                Event::Water,
            ]
        );

        assert_eq!(
            publication.advance(&mut host).unwrap(),
            VisibleTerrainPublicationProgress::Complete(completion)
        );
        assert_eq!(host.events.len(), 11);
    }

    #[test]
    fn procedural_startup_skips_loaded_connectivity() {
        let mut publication = VisibleTerrainPublication::startup(vec![UVec3::ZERO], false).unwrap();
        let mut host = RecordingHost::new([published(1)]);

        publication.advance(&mut host).unwrap();

        assert_eq!(
            host.events,
            vec![Event::PhysicalComplete, Event::ColliderBegin]
        );
    }

    #[test]
    fn physical_progress_is_not_semantic_completion() {
        let mut publication =
            VisibleTerrainPublication::startup(vec![UVec3::ZERO, UVec3::X], false).unwrap();
        let mut host = RecordingHost::new([
            physical_visible_terrain::PhysicalTerrainPublicationProgress::Preparing {
                prepared_chunks: 1,
                total_chunks: 2,
            },
            published(2),
        ]);

        assert_eq!(
            publication.advance(&mut host).unwrap(),
            VisibleTerrainPublicationProgress::Preparing {
                prepared_chunks: 1,
                total_chunks: 2,
            }
        );
        assert_eq!(host.events, vec![Event::PhysicalPreparing]);
        assert!(publication.completion.is_none());
    }

    #[test]
    fn terrain_revision_advances_only_for_a_complete_terrain_change() {
        assert_eq!(next_visible_terrain_revision(7, true), Some(8));
        assert_eq!(next_visible_terrain_revision(u32::MAX, true), Some(1));
        assert_eq!(next_visible_terrain_revision(7, false), None);
    }

    #[test]
    fn empty_build_batch_is_not_a_publication() {
        assert!(VisibleTerrainChange::from_build_edits(Vec::new())
            .unwrap()
            .is_none());
        assert!(
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildChunks(Vec::new())])
                .is_err()
        );
    }
}
