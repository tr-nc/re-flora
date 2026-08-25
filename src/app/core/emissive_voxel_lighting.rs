use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{mpsc, Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::Instant,
};

use anyhow::{anyhow, ensure, Result};
use glam::{UVec3, Vec3};

use crate::{
    builder::{
        ContreeCpuSparseVoxelExport, ContreeCpuVoxelBlock, ContreeCpuVoxelBlockExport,
        ContreeCpuVoxelSourceDependency, ContreeCpuVoxelSourceSnapshot, VOXEL_TYPE_EMISSIVE,
    },
    geom::UAabb3,
    lighting::{
        EmissiveVoxelEmitter, EmissiveVoxelProvider, LocalLightProviderSnapshot,
        LocalLightRegistry, EMISSIVE_VOXEL_CLUSTER_DIM, EMISSIVE_VOXEL_COLOR_SRGB,
        EMISSIVE_VOXEL_SURFACE_RADIANCE,
    },
};

const EMISSIVE_VOXEL_LOCAL_SCAN_MAX_CELLS_PER_FRAME: usize = 16;
const EMISSIVE_VOXEL_LOCAL_SCAN_TIME_BUDGET_MS: f64 = 0.25;
const EMISSIVE_VOXEL_LIGHT_RANGE_WORLD: f32 = 0.35;

fn should_start_next_local_cell(completed_cells: usize, elapsed_ms: f64) -> bool {
    completed_cells == 0
        || (completed_cells < EMISSIVE_VOXEL_LOCAL_SCAN_MAX_CELLS_PER_FRAME
            && elapsed_ms < EMISSIVE_VOXEL_LOCAL_SCAN_TIME_BUDGET_MS)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmissiveVoxelScanReason {
    RuntimeEdit,
    ConservativeChunk,
}

trait EmissiveVoxelSource {
    fn chunk_dependency(&self, chunk_idx: UVec3) -> Option<ContreeCpuVoxelSourceDependency>;
    fn export_cell(&self, bound: UAabb3) -> Result<ContreeCpuVoxelBlockExport>;
}

impl EmissiveVoxelSource for ContreeCpuVoxelSourceSnapshot {
    fn chunk_dependency(&self, chunk_idx: UVec3) -> Option<ContreeCpuVoxelSourceDependency> {
        self.chunk_source_dependency(chunk_idx)
    }

    fn export_cell(&self, bound: UAabb3) -> Result<ContreeCpuVoxelBlockExport> {
        self.export_voxel_block(bound.min(), bound.max() - bound.min())
            .map_err(|err| anyhow!("failed to export emissive voxel cell {bound:?}: {err}"))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct EmissiveVoxelLightingAdvance {
    full_chunks_scheduled: usize,
    full_cells_scheduled: usize,
    local_scanned_cells: usize,
    full_scanned_cells: usize,
    local_emissive_voxels: usize,
    full_emissive_voxels: usize,
    local_provider_publications: usize,
    full_provider_publications: usize,
    full_chunk_completions: usize,
    full_worker_submissions: usize,
    full_worker_coalesced: usize,
    full_worker_stale_results: usize,
    full_worker_not_ready: usize,
    discarded_full_stagings: usize,
    stale_dependency_retries: usize,
    not_ready_retries: usize,
    max_local_dirty_to_publication_frames: Option<u64>,
    max_full_scan_frames: Option<u64>,
    local_cpu_ms: f64,
    full_cpu_ms: f64,
    completed_full_cpu_ms: f64,
    full_worker_ms: f64,
    full_publication_cpu_ms: f64,
    cpu_ms: f64,
    backlog: EmissiveVoxelScanBacklog,
}

#[derive(Clone, Debug)]
struct LocalEditMetrics {
    first_queued_frame: u64,
    change_count: usize,
    requested_cells: usize,
    scanned_cells: usize,
    emissive_voxels: usize,
    provider_publications: usize,
    cpu_ms: f64,
}

#[derive(Clone, Copy, Debug)]
struct CompletedFullChunk {
    chunk_idx: UVec3,
    dependency: ContreeCpuVoxelSourceDependency,
    scanned_cells: usize,
    emitter_voxels: usize,
    scan_frames: u64,
    cpu_ms: f64,
    publication_cpu_ms: f64,
    provider_changed: bool,
}

#[derive(Clone, Debug)]
struct FullChunkStaging {
    dependency: ContreeCpuVoxelSourceDependency,
    queued_frame: u64,
    scanned_cells: BTreeSet<GridCoord>,
    emitters: BTreeMap<GridCoord, EmissiveVoxelEmitter>,
    cpu_ms: f64,
}

#[derive(Clone)]
struct SparseFullChunkRequest {
    source: Arc<ContreeCpuVoxelSourceSnapshot>,
    dependency: ContreeCpuVoxelSourceDependency,
    queued_frame: u64,
}

struct SparseFullChunkResult {
    dependency: ContreeCpuVoxelSourceDependency,
    queued_frame: u64,
    outcome: std::result::Result<ContreeCpuSparseVoxelExport, String>,
    worker_ms: f64,
}

#[derive(Default)]
struct SparseFullChunkMailbox {
    stopping: bool,
    pending: BTreeMap<GridCoord, SparseFullChunkRequest>,
    order: VecDeque<GridCoord>,
}

struct SparseFullChunkScanner {
    mailbox: Arc<(Mutex<SparseFullChunkMailbox>, Condvar)>,
    result_rx: mpsc::Receiver<SparseFullChunkResult>,
    outstanding: BTreeMap<GridCoord, ContreeCpuVoxelSourceDependency>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SparseFullChunkRequestDisposition {
    Queued,
    Coalesced,
    AlreadyOutstanding,
}

impl SparseFullChunkScanner {
    fn new() -> Result<Self> {
        let mailbox = Arc::new((
            Mutex::new(SparseFullChunkMailbox::default()),
            Condvar::new(),
        ));
        let worker_mailbox = Arc::clone(&mailbox);
        let (result_tx, result_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("emissive-voxel-scan".to_owned())
            .spawn(move || sparse_full_chunk_worker(worker_mailbox, result_tx))
            .map_err(|err| anyhow!("failed to spawn emissive voxel scan worker: {err}"))?;
        Ok(Self {
            mailbox,
            result_rx,
            outstanding: BTreeMap::new(),
            worker: Some(worker),
        })
    }

    /// One pending request per chunk: a newer immutable snapshot replaces an older queued one,
    /// while insertion order remains round-robin across chunks.
    fn request(&mut self, request: SparseFullChunkRequest) -> SparseFullChunkRequestDisposition {
        let chunk = GridCoord::new(request.dependency.chunk_idx);
        if self.outstanding.get(&chunk) == Some(&request.dependency) {
            return SparseFullChunkRequestDisposition::AlreadyOutstanding;
        }
        let (lock, wake) = &*self.mailbox;
        let mut mailbox = lock.lock().expect("emissive voxel mailbox poisoned");
        let coalesced = mailbox.pending.insert(chunk, request).is_some();
        if !coalesced {
            mailbox.order.push_back(chunk);
        }
        self.outstanding
            .insert(chunk, mailbox.pending[&chunk].dependency);
        wake.notify_one();
        if coalesced {
            SparseFullChunkRequestDisposition::Coalesced
        } else {
            SparseFullChunkRequestDisposition::Queued
        }
    }

    fn poll_result(&mut self) -> Option<SparseFullChunkResult> {
        let result = self.result_rx.try_recv().ok()?;
        let chunk = GridCoord::new(result.dependency.chunk_idx);
        if self.outstanding.get(&chunk) == Some(&result.dependency) {
            self.outstanding.remove(&chunk);
        }
        Some(result)
    }

    fn cancel_chunk(&mut self, chunk: UVec3) {
        let chunk = GridCoord::new(chunk);
        let (lock, _) = &*self.mailbox;
        let mut mailbox = lock.lock().expect("emissive voxel mailbox poisoned");
        mailbox.pending.remove(&chunk);
        mailbox.order.retain(|queued| *queued != chunk);
        self.outstanding.remove(&chunk);
    }

    fn shutdown(&mut self) {
        let Some(worker) = self.worker.take() else {
            return;
        };
        let (lock, wake) = &*self.mailbox;
        {
            let mut mailbox = lock.lock().expect("emissive voxel mailbox poisoned");
            mailbox.stopping = true;
            mailbox.pending.clear();
            mailbox.order.clear();
        }
        self.outstanding.clear();
        wake.notify_one();
        if worker.join().is_err() {
            log::warn!("[LOCAL_LIGHT][VOXEL_PROVIDER] sparse scan worker panicked at shutdown");
        }
    }
}

impl Drop for SparseFullChunkScanner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn sparse_full_chunk_worker(
    mailbox: Arc<(Mutex<SparseFullChunkMailbox>, Condvar)>,
    result_tx: mpsc::Sender<SparseFullChunkResult>,
) {
    loop {
        let request = {
            let (lock, wake) = &*mailbox;
            let mut mailbox = lock.lock().expect("emissive voxel mailbox poisoned");
            while !mailbox.stopping && mailbox.order.is_empty() {
                mailbox = wake
                    .wait(mailbox)
                    .expect("emissive voxel mailbox poisoned while waiting");
            }
            if mailbox.stopping {
                return;
            }
            let chunk = mailbox
                .order
                .pop_front()
                .expect("non-empty order must contain one chunk");
            mailbox
                .pending
                .remove(&chunk)
                .expect("ordered sparse scan request must exist")
        };
        let started_at = Instant::now();
        let outcome = request
            .source
            .export_chunk_voxels_matching_type(request.dependency.chunk_idx, VOXEL_TYPE_EMISSIVE)
            .map_err(|err| err.to_string());
        let result = SparseFullChunkResult {
            dependency: request.dependency,
            queued_frame: request.queued_frame,
            outcome,
            worker_ms: started_at.elapsed().as_secs_f64() * 1_000.0,
        };
        if result_tx.send(result).is_err() {
            return;
        }
    }
}

pub(super) struct EmissiveVoxelLightingRuntime {
    chunk_dim: UVec3,
    voxels_per_world_unit: Vec3,
    scheduler: EmissiveVoxelScanScheduler,
    provider: EmissiveVoxelProvider,
    cell_budget: usize,
    full_staging: BTreeMap<GridCoord, FullChunkStaging>,
    sparse_full_scanner: Option<SparseFullChunkScanner>,
    required_sparse_full: BTreeMap<GridCoord, PendingCell>,
    local_edit_metrics: Option<LocalEditMetrics>,
    pending_registry_snapshot: Option<LocalLightProviderSnapshot>,
}

impl EmissiveVoxelLightingRuntime {
    pub(super) fn new(chunk_dim: UVec3, voxel_dim_per_chunk: UVec3) -> Result<Self> {
        let mut runtime = Self::new_with_grid(
            chunk_dim,
            voxel_dim_per_chunk,
            EMISSIVE_VOXEL_CLUSTER_DIM,
            EMISSIVE_VOXEL_LOCAL_SCAN_MAX_CELLS_PER_FRAME,
        )?;
        runtime.scheduler.sparse_full_requests = true;
        runtime.sparse_full_scanner = Some(SparseFullChunkScanner::new()?);
        Ok(runtime)
    }

    fn new_with_grid(
        chunk_dim: UVec3,
        voxel_dim_per_chunk: UVec3,
        cell_dim: UVec3,
        cell_budget: usize,
    ) -> Result<Self> {
        ensure!(
            cell_budget > 0,
            "emissive voxel cell budget must be non-zero"
        );
        let voxels_per_world_unit = voxel_dim_per_chunk.as_vec3();
        Ok(Self {
            chunk_dim,
            voxels_per_world_unit,
            scheduler: EmissiveVoxelScanScheduler::new(chunk_dim, voxel_dim_per_chunk, cell_dim)
                .map_err(|err| anyhow!("invalid emissive voxel scan grid: {err:?}"))?,
            provider: EmissiveVoxelProvider::new(voxels_per_world_unit)
                .map_err(|err| anyhow!("invalid emissive voxel provider scale: {err:?}"))?,
            cell_budget,
            full_staging: BTreeMap::new(),
            sparse_full_scanner: None,
            required_sparse_full: BTreeMap::new(),
            local_edit_metrics: None,
            pending_registry_snapshot: None,
        })
    }

    pub(super) fn mark_trusted_change(&mut self, bound: UAabb3, frame: u64) -> Result<usize> {
        let touched = self
            .scheduler
            .mark_trusted_change(bound, frame)
            .map_err(|err| anyhow!("invalid emissive voxel terrain change: {err:?}"))?;
        // Whole-chunk/world replacements are conservative full scans, not runtime-edit latency.
        if touched > 0 && touched < self.scheduler.cells_per_chunk {
            let metrics = self.local_edit_metrics.get_or_insert(LocalEditMetrics {
                first_queued_frame: frame,
                change_count: 0,
                requested_cells: 0,
                scanned_cells: 0,
                emissive_voxels: 0,
                provider_publications: 0,
                cpu_ms: 0.0,
            });
            metrics.first_queued_frame = metrics.first_queued_frame.min(frame);
            metrics.change_count += 1;
            metrics.requested_cells += touched;
        }
        Ok(touched)
    }

    pub(super) fn shutdown(&mut self) {
        if let Some(scanner) = self.sparse_full_scanner.as_mut() {
            scanner.shutdown();
        }
    }

    pub(super) fn advance(
        &mut self,
        source: &ContreeCpuVoxelSourceSnapshot,
        registry: &mut LocalLightRegistry,
        frame: u64,
    ) -> Result<EmissiveVoxelLightingAdvance> {
        let source = Arc::new(source.clone());
        self.advance_from_mode(source.as_ref(), registry, frame, Some(source.clone()))
    }

    #[cfg(test)]
    fn advance_from(
        &mut self,
        source: &impl EmissiveVoxelSource,
        registry: &mut LocalLightRegistry,
        frame: u64,
    ) -> Result<EmissiveVoxelLightingAdvance> {
        self.advance_from_mode(source, registry, frame, None)
    }

    fn advance_from_mode(
        &mut self,
        source: &impl EmissiveVoxelSource,
        registry: &mut LocalLightRegistry,
        frame: u64,
        sparse_source: Option<Arc<ContreeCpuVoxelSourceSnapshot>>,
    ) -> Result<EmissiveVoxelLightingAdvance> {
        let started_at = Instant::now();
        let mut result = EmissiveVoxelLightingAdvance::default();
        let mut provider_changed = false;
        let mut completed_full = Vec::new();
        if let Some(snapshot) = self.pending_registry_snapshot.clone() {
            registry
                .reconcile(snapshot)
                .map_err(|err| anyhow!("failed to retry emissive provider publication: {err:?}"))?;
            self.pending_registry_snapshot = None;
        }
        for z in 0..self.chunk_dim.z {
            for y in 0..self.chunk_dim.y {
                for x in 0..self.chunk_dim.x {
                    let chunk_idx = UVec3::new(x, y, z);
                    if let Some(dependency) = source.chunk_dependency(chunk_idx) {
                        let sparse_required = self
                            .required_sparse_full
                            .get(&GridCoord::new(chunk_idx))
                            .copied();
                        let state = &self.scheduler.chunks[&GridCoord::new(chunk_idx)];
                        let dependency_changed = state.observed != Some(dependency);
                        let schedules_full = state.observed != Some(dependency)
                            && dependency.source_revision.is_some()
                            && dependency.is_present
                            && (state.awaiting_dependency.is_empty()
                                || state.awaiting_dependency.len()
                                    == self.scheduler.cells_per_chunk
                                || state.full.is_some()
                                || sparse_required.is_some());
                        self.scheduler
                            .observe_dependency(dependency, frame)
                            .map_err(|err| {
                                anyhow!("failed to observe emissive voxel source: {err:?}")
                            })?;
                        if schedules_full && sparse_source.is_some() {
                            result.full_chunks_scheduled += 1;
                        } else if schedules_full {
                            result.full_cells_scheduled += self.scheduler.cells_per_chunk;
                        }
                        if dependency_changed && dependency.is_present {
                            if let Some(previous) = sparse_required {
                                self.scheduler
                                    .require_full_chunk_preserving_priority(
                                        dependency,
                                        previous.queued_frame.min(frame),
                                    )
                                    .map_err(|err| {
                                        anyhow!(
                                            "failed to keep sparse full scan exact across revision: {err:?}"
                                        )
                                    })?;
                            }
                        }
                        if dependency_changed
                            && dependency.source_revision.is_some()
                            && !dependency.is_present
                        {
                            self.required_sparse_full.remove(&GridCoord::new(chunk_idx));
                            if let Some(scanner) = self.sparse_full_scanner.as_mut() {
                                scanner.cancel_chunk(chunk_idx);
                            }
                            self.full_staging.remove(&GridCoord::new(chunk_idx));
                            let chunk_bound =
                                chunk_voxel_bound(chunk_idx, self.scheduler.voxel_dim_per_chunk);
                            let change = self
                                .provider
                                .replace_region(chunk_bound, std::iter::empty())
                                .map_err(|err| {
                                    anyhow!(
                                        "failed to transactionally clear absent emissive chunk {chunk_bound:?}: {err:?}"
                                    )
                                })?;
                            result.full_chunk_completions += 1;
                            if change.changed {
                                provider_changed = true;
                                result.full_provider_publications += 1;
                            }
                            completed_full.push(CompletedFullChunk {
                                chunk_idx,
                                dependency,
                                scanned_cells: 0,
                                emitter_voxels: 0,
                                scan_frames: 0,
                                cpu_ms: 0.0,
                                publication_cpu_ms: 0.0,
                                provider_changed: change.changed,
                            });
                        }
                    }
                }
            }
        }
        if let Some(sparse_source) = sparse_source.as_ref() {
            let requests = self.scheduler.drain_full_chunk_requests();
            for request in requests {
                let chunk = GridCoord::new(request.chunk_idx);
                self.required_sparse_full
                    .entry(chunk)
                    .and_modify(|pending| {
                        pending.dependency = request.dependency;
                        pending.queued_frame = pending.queued_frame.min(request.queued_frame);
                    })
                    .or_insert(PendingCell {
                        dependency: request.dependency,
                        queued_frame: request.queued_frame,
                    });
                let queued_frame = self.required_sparse_full[&chunk].queued_frame;
                let disposition = self
                    .sparse_full_scanner
                    .as_mut()
                    .expect("production sparse source requires its worker")
                    .request(SparseFullChunkRequest {
                        source: Arc::clone(sparse_source),
                        dependency: request.dependency,
                        queued_frame,
                    });
                match disposition {
                    SparseFullChunkRequestDisposition::Queued => {
                        result.full_worker_submissions += 1;
                    }
                    SparseFullChunkRequestDisposition::Coalesced => {
                        result.full_worker_submissions += 1;
                        result.full_worker_coalesced += 1;
                    }
                    SparseFullChunkRequestDisposition::AlreadyOutstanding => {}
                }
            }

            if let Some(worker_result) = self
                .sparse_full_scanner
                .as_mut()
                .expect("production sparse source requires its worker")
                .poll_result()
            {
                result.full_worker_ms += worker_result.worker_ms;
                let chunk = GridCoord::new(worker_result.dependency.chunk_idx);
                let still_required = self.required_sparse_full.get(&chunk).copied();
                let live_dependency = source.chunk_dependency(worker_result.dependency.chunk_idx);
                if still_required
                    .is_none_or(|pending| pending.dependency != worker_result.dependency)
                    || live_dependency != Some(worker_result.dependency)
                {
                    result.full_worker_stale_results += 1;
                } else {
                    match worker_result.outcome {
                        Ok(ContreeCpuSparseVoxelExport::NotReady { dependency }) => {
                            ensure!(
                                dependency == worker_result.dependency,
                                "sparse worker returned mixed not-ready dependency: requested={:?} returned={dependency:?}",
                                worker_result.dependency
                            );
                            result.full_worker_not_ready += 1;
                        }
                        Ok(ContreeCpuSparseVoxelExport::Ready {
                            dependency,
                            world_voxels,
                        }) => {
                            ensure!(
                                dependency == worker_result.dependency,
                                "sparse worker returned mixed dependency: requested={:?} returned={dependency:?}",
                                worker_result.dependency
                            );
                            let publish_started_at = Instant::now();
                            let emitter = emissive_voxel_emitter(self.voxels_per_world_unit)?;
                            let emitter_voxels = world_voxels.len();
                            let chunk_bound = chunk_voxel_bound(
                                dependency.chunk_idx,
                                self.scheduler.voxel_dim_per_chunk,
                            );
                            let change = self
                                .provider
                                .replace_region(
                                    chunk_bound,
                                    world_voxels
                                        .into_iter()
                                        .map(|world_voxel| (world_voxel, emitter)),
                                )
                                .map_err(|err| {
                                    anyhow!(
                                        "failed to publish sparse emissive chunk {chunk_bound:?}: {err:?}"
                                    )
                                })?;
                            let publication_cpu_ms =
                                publish_started_at.elapsed().as_secs_f64() * 1_000.0;
                            result.full_publication_cpu_ms += publication_cpu_ms;
                            result.full_chunk_completions += 1;
                            result.completed_full_cpu_ms += worker_result.worker_ms;
                            result.max_full_scan_frames =
                                Some(frame.saturating_sub(worker_result.queued_frame));
                            if change.changed {
                                provider_changed = true;
                                result.full_provider_publications += 1;
                            }
                            self.required_sparse_full.remove(&chunk);
                            completed_full.push(CompletedFullChunk {
                                chunk_idx: dependency.chunk_idx,
                                dependency,
                                scanned_cells: 0,
                                emitter_voxels,
                                scan_frames: frame.saturating_sub(worker_result.queued_frame),
                                cpu_ms: worker_result.worker_ms,
                                publication_cpu_ms,
                                provider_changed: change.changed,
                            });
                        }
                        Err(err) => {
                            return Err(anyhow!(
                                "sparse emissive chunk scan failed for {:?}: {err}",
                                worker_result.dependency
                            ));
                        }
                    }
                }
            }

            // NotReady keeps the authoritative requirement but releases the completed worker
            // request. Re-submit only when this frame's latest immutable snapshot reports the
            // exact dependency ready; scanner-side identity dedupe prevents busy duplicates.
            let ready_required = self
                .required_sparse_full
                .values()
                .copied()
                .filter(|pending| sparse_source.is_chunk_voxel_cache_ready(pending.dependency))
                .collect::<Vec<_>>();
            for pending in ready_required {
                let disposition = self
                    .sparse_full_scanner
                    .as_mut()
                    .expect("production sparse source requires its worker")
                    .request(SparseFullChunkRequest {
                        source: Arc::clone(sparse_source),
                        dependency: pending.dependency,
                        queued_frame: pending.queued_frame,
                    });
                match disposition {
                    SparseFullChunkRequestDisposition::Queued => {
                        result.full_worker_submissions += 1;
                    }
                    SparseFullChunkRequestDisposition::Coalesced => {
                        result.full_worker_submissions += 1;
                        result.full_worker_coalesced += 1;
                    }
                    SparseFullChunkRequestDisposition::AlreadyOutstanding => {}
                }
            }
        }
        let stale_staging = self
            .full_staging
            .iter()
            .filter_map(|(chunk, staging)| {
                let observed = self.scheduler.chunks.get(chunk)?.observed;
                (observed != Some(staging.dependency)).then_some(*chunk)
            })
            .collect::<Vec<_>>();
        for chunk in stale_staging {
            self.full_staging.remove(&chunk);
            result.discarded_full_stagings += 1;
        }

        let production_local_scan = sparse_source.is_some();
        let local_scan_started_at = Instant::now();
        let mut local_cells_started = 0;
        let mut fallback_work = (!production_local_scan)
            .then(|| self.scheduler.take_work(self.cell_budget).into_iter());
        loop {
            // A 16^3 cell export is the smallest immutable/transactional unit and cannot be
            // preempted. The budget is therefore a soft bound with at most one-cell overshoot.
            // Always completing one cell guarantees progress even on a slow frame.
            if production_local_scan
                && !should_start_next_local_cell(
                    local_cells_started,
                    local_scan_started_at.elapsed().as_secs_f64() * 1_000.0,
                )
            {
                break;
            }
            let work = if production_local_scan {
                if local_cells_started >= self.cell_budget {
                    break;
                }
                self.scheduler.pop_priority()
            } else {
                fallback_work.as_mut().and_then(Iterator::next)
            };
            let Some(work) = work else {
                break;
            };
            if production_local_scan {
                local_cells_started += 1;
            }
            let work_started_at = Instant::now();
            let export = match source.export_cell(work.cell_bound) {
                Ok(export) => export,
                Err(err) => {
                    if provider_changed {
                        self.pending_registry_snapshot = Some(self.provider.snapshot());
                    }
                    self.scheduler.retry(work);
                    return Err(err);
                }
            };
            let block = match export {
                ContreeCpuVoxelBlockExport::Ready(block) => block,
                ContreeCpuVoxelBlockExport::NotReady(_) => {
                    self.scheduler.retry(work);
                    result.not_ready_retries += 1;
                    continue;
                }
            };
            if block.source_dependencies.as_slice() != [work.dependency] {
                self.scheduler.retry(work);
                result.stale_dependency_retries += 1;
                continue;
            }
            let emitters = match emissive_emitters_from_block(&block, self.voxels_per_world_unit) {
                Ok(emitters) => emitters,
                Err(err) => {
                    if provider_changed {
                        self.pending_registry_snapshot = Some(self.provider.snapshot());
                    }
                    self.scheduler.retry(work);
                    return Err(err);
                }
            };
            match work.reason {
                EmissiveVoxelScanReason::RuntimeEdit => {
                    let emitter_count = emitters.len();
                    result.local_scanned_cells += 1;
                    result.local_emissive_voxels += emitter_count;
                    let change = match self.provider.replace_region(work.cell_bound, emitters) {
                        Ok(change) => change,
                        Err(err) => {
                            if provider_changed {
                                self.pending_registry_snapshot = Some(self.provider.snapshot());
                            }
                            self.scheduler.retry(work);
                            return Err(anyhow!(
                                "failed to transactionally publish emissive cell {:?}: {err:?}",
                                work.cell_bound
                            ));
                        }
                    };
                    if change.changed {
                        provider_changed = true;
                        result.local_provider_publications += 1;
                    }
                    let latency = frame.saturating_sub(work.queued_frame);
                    result.max_local_dirty_to_publication_frames = Some(
                        result
                            .max_local_dirty_to_publication_frames
                            .map_or(latency, |current| current.max(latency)),
                    );
                    let elapsed_ms = work_started_at.elapsed().as_secs_f64() * 1_000.0;
                    result.local_cpu_ms += elapsed_ms;
                    if let Some(metrics) = self.local_edit_metrics.as_mut() {
                        metrics.scanned_cells += 1;
                        metrics.emissive_voxels += emitter_count;
                        metrics.provider_publications += usize::from(change.changed);
                        metrics.cpu_ms += elapsed_ms;
                    }
                }
                EmissiveVoxelScanReason::ConservativeChunk => {
                    result.full_scanned_cells += 1;
                    result.full_emissive_voxels += emitters.len();
                    let cell = GridCoord::new(work.cell_bound.min() / self.scheduler.cell_dim);
                    let staging = self
                        .full_staging
                        .entry(GridCoord::new(work.chunk_idx))
                        .or_insert_with(|| FullChunkStaging {
                            dependency: work.dependency,
                            queued_frame: work.queued_frame,
                            scanned_cells: BTreeSet::new(),
                            emitters: BTreeMap::new(),
                            cpu_ms: 0.0,
                        });
                    debug_assert_eq!(staging.dependency, work.dependency);
                    staging.queued_frame = staging.queued_frame.min(work.queued_frame);
                    staging.scanned_cells.insert(cell);
                    for (voxel, emitter) in emitters {
                        staging.emitters.insert(GridCoord::new(voxel), emitter);
                    }
                    let elapsed_ms = work_started_at.elapsed().as_secs_f64() * 1_000.0;
                    staging.cpu_ms += elapsed_ms;
                    result.full_cpu_ms += elapsed_ms;
                }
            }
        }

        let completed_full_chunks = self
            .full_staging
            .iter()
            .filter_map(|(chunk, staging)| {
                let scheduler_done = self.scheduler.chunks.get(chunk).is_some_and(|state| {
                    state.observed == Some(staging.dependency) && state.full.is_none()
                });
                (scheduler_done && staging.scanned_cells.len() == self.scheduler.cells_per_chunk)
                    .then_some(*chunk)
            })
            .collect::<Vec<_>>();
        for chunk in completed_full_chunks {
            let staging = self
                .full_staging
                .remove(&chunk)
                .expect("selected full staging must still exist");
            let live_dependency = source.chunk_dependency(chunk.get());
            if live_dependency != Some(staging.dependency) {
                let restart_dependency = live_dependency.unwrap_or(staging.dependency);
                self.scheduler
                    .restart_full_chunk(restart_dependency, frame)
                    .map_err(|err| anyhow!("failed to restart stale full scan: {err:?}"))?;
                result.discarded_full_stagings += 1;
                continue;
            }
            let chunk_bound = chunk_voxel_bound(chunk.get(), self.scheduler.voxel_dim_per_chunk);
            let emitter_voxels = staging.emitters.len();
            let change = self
                .provider
                .replace_region(
                    chunk_bound,
                    staging
                        .emitters
                        .into_iter()
                        .map(|(voxel, emitter)| (voxel.get(), emitter)),
                )
                .map_err(|err| {
                    self.scheduler
                        .restart_full_chunk(staging.dependency, frame)
                        .expect("existing full chunk dependency must remain schedulable");
                    anyhow!(
                        "failed to transactionally publish emissive chunk {chunk_bound:?}: {err:?}"
                    )
                })?;
            result.full_chunk_completions += 1;
            result.completed_full_cpu_ms += staging.cpu_ms;
            result.max_full_scan_frames = Some(
                result
                    .max_full_scan_frames
                    .map_or(frame.saturating_sub(staging.queued_frame), |current| {
                        current.max(frame.saturating_sub(staging.queued_frame))
                    }),
            );
            if change.changed {
                provider_changed = true;
                result.full_provider_publications += 1;
            }
            completed_full.push(CompletedFullChunk {
                chunk_idx: chunk.get(),
                dependency: staging.dependency,
                scanned_cells: self.scheduler.cells_per_chunk,
                emitter_voxels,
                scan_frames: frame.saturating_sub(staging.queued_frame),
                cpu_ms: staging.cpu_ms,
                publication_cpu_ms: 0.0,
                provider_changed: change.changed,
            });
        }
        if provider_changed {
            let snapshot = self.provider.snapshot();
            self.pending_registry_snapshot = Some(snapshot.clone());
            registry.reconcile(snapshot).map_err(|err| {
                anyhow!("failed to reconcile emissive provider snapshot: {err:?}")
            })?;
            self.pending_registry_snapshot = None;
        }
        result.backlog = self.scheduler.backlog();
        result.backlog.full_chunks += self.required_sparse_full.len();
        result.cpu_ms = started_at.elapsed().as_secs_f64() * 1_000.0;
        let provider_snapshot = self.provider.snapshot();
        if result.full_chunks_scheduled > 0
            || result.full_cells_scheduled > 0
            || result.discarded_full_stagings > 0
        {
            log::info!(
                "[LOCAL_LIGHT][VOXEL_PROVIDER_SCHEDULE] full_chunks_scheduled={} full_cells_scheduled={} worker_submissions={} worker_coalesced={} discarded_stagings={} local_max_cells={} local_soft_budget_ms={:.3} backlog_full_chunks={} backlog_full_cells={} backlog_priority_cells={} awaiting_dependency_cells={}",
                result.full_chunks_scheduled,
                result.full_cells_scheduled,
                result.full_worker_submissions,
                result.full_worker_coalesced,
                result.discarded_full_stagings,
                self.cell_budget,
                EMISSIVE_VOXEL_LOCAL_SCAN_TIME_BUDGET_MS,
                result.backlog.full_chunks,
                result.backlog.full_cells,
                result.backlog.priority_cells,
                result.backlog.awaiting_dependency_cells,
            );
        }
        for completed in completed_full {
            log::info!(
                "[LOCAL_LIGHT][VOXEL_PROVIDER_FULL] chunk={:?} source_revision={:?} is_present={} dense_cells={} emitter_voxels={} scan_frames={} worker_or_scan_cpu_ms={:.3} publication_cpu_ms={:.3} provider_changed={} provider_source_revision={} provider_source_count={} registry_revision={} registry_source_revision={} backlog_full_chunks={} backlog_full_cells={} backlog_priority_cells={} discarded_stagings={} stale_worker_results={} worker_not_ready={}",
                completed.chunk_idx,
                completed.dependency.source_revision,
                completed.dependency.is_present,
                completed.scanned_cells,
                completed.emitter_voxels,
                completed.scan_frames,
                completed.cpu_ms,
                completed.publication_cpu_ms,
                completed.provider_changed,
                provider_snapshot.source_revision(),
                provider_snapshot.sources().len(),
                registry.registry_revision(),
                registry.snapshot().source_revision(),
                result.backlog.full_chunks,
                result.backlog.full_cells,
                result.backlog.priority_cells,
                result.discarded_full_stagings,
                result.full_worker_stale_results,
                result.full_worker_not_ready,
            );
        }
        if result.backlog.priority_cells == 0
            && result.backlog.awaiting_dependency_cells == 0
            && self
                .local_edit_metrics
                .as_ref()
                .is_some_and(|metrics| metrics.scanned_cells > 0)
        {
            let completed = self
                .local_edit_metrics
                .take()
                .expect("checked local edit metrics must exist");
            log::info!(
                "[LOCAL_LIGHT][VOXEL_PROVIDER_EDIT] change_count={} requested_cells={} scanned_cells={} emitter_voxels={} edit_to_publication_frames={} scan_cpu_ms={:.3} provider_publications={} provider_source_revision={} provider_source_count={} registry_revision={} registry_source_revision={} backlog_full_chunks={} backlog_full_cells={} stale_dependency_retries={}",
                completed.change_count,
                completed.requested_cells,
                completed.scanned_cells,
                completed.emissive_voxels,
                frame.saturating_sub(completed.first_queued_frame),
                completed.cpu_ms,
                completed.provider_publications,
                provider_snapshot.source_revision(),
                provider_snapshot.sources().len(),
                registry.registry_revision(),
                registry.snapshot().source_revision(),
                result.backlog.full_chunks,
                result.backlog.full_cells,
                result.stale_dependency_retries,
            );
        }
        Ok(result)
    }
}

fn emissive_emitters_from_block(
    block: &ContreeCpuVoxelBlock,
    voxels_per_world_unit: Vec3,
) -> Result<Vec<(UVec3, EmissiveVoxelEmitter)>> {
    ensure!(
        voxels_per_world_unit.is_finite() && voxels_per_world_unit.min_element() > 0.0,
        "emissive voxel scale must be finite and positive"
    );
    let voxel_count = u64::from(block.dim.x)
        .checked_mul(u64::from(block.dim.y))
        .and_then(|count| count.checked_mul(u64::from(block.dim.z)))
        .and_then(|count| usize::try_from(count).ok())
        .ok_or_else(|| anyhow!("emissive voxel block size overflow: {:?}", block.dim))?;
    ensure!(
        block.voxel_types.len() == voxel_count,
        "emissive voxel block length mismatch: dim={:?} expected={} actual={}",
        block.dim,
        voxel_count,
        block.voxel_types.len()
    );

    let emitter = emissive_voxel_emitter(voxels_per_world_unit)?;
    let mut emitters = Vec::new();
    for (index, voxel_type) in block.voxel_types.iter().copied().enumerate() {
        if u32::from(voxel_type) != VOXEL_TYPE_EMISSIVE {
            continue;
        }
        let index = index as u32;
        let x = index % block.dim.x;
        let yz = index / block.dim.x;
        let y = yz % block.dim.y;
        let z = yz / block.dim.y;
        emitters.push((block.voxel_min + UVec3::new(x, y, z), emitter));
    }
    Ok(emitters)
}

fn emissive_voxel_emitter(voxels_per_world_unit: Vec3) -> Result<EmissiveVoxelEmitter> {
    ensure!(
        voxels_per_world_unit.is_finite() && voxels_per_world_unit.min_element() > 0.0,
        "emissive voxel scale must be finite and positive"
    );
    let voxel_size_world = Vec3::ONE / voxels_per_world_unit;
    // For an isotropic point approximation of a Lambertian cuboid, the mean projected area is
    // one quarter of surface area. Intensity therefore remains in world-unit radiometric scale.
    let average_projected_area = (voxel_size_world.x * voxel_size_world.y
        + voxel_size_world.y * voxel_size_world.z
        + voxel_size_world.z * voxel_size_world.x)
        * 0.5;
    let voxel_half_diagonal_world = (voxel_size_world * 0.5).length();
    EmissiveVoxelEmitter::new(
        EMISSIVE_VOXEL_COLOR_SRGB,
        EMISSIVE_VOXEL_SURFACE_RADIANCE * average_projected_area,
        (voxel_size_world.min_element() * 0.001).max(f32::EPSILON),
        EMISSIVE_VOXEL_LIGHT_RANGE_WORLD.max(voxel_half_diagonal_world),
    )
    .map_err(|err| anyhow!("failed to construct emissive voxel emitter: {err:?}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EmissiveVoxelScanWork {
    chunk_idx: UVec3,
    cell_bound: UAabb3,
    dependency: ContreeCpuVoxelSourceDependency,
    queued_frame: u64,
    reason: EmissiveVoxelScanReason,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EmissiveVoxelScanBacklog {
    priority_cells: usize,
    full_chunks: usize,
    full_cells: usize,
    awaiting_dependency_cells: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmissiveVoxelScanSchedulerError {
    InvalidGrid,
    InvalidTrustedBound(UAabb3),
    UnknownChunk(UVec3),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GridCoord([u32; 3]);

impl GridCoord {
    fn new(value: UVec3) -> Self {
        Self(value.to_array())
    }

    fn get(self) -> UVec3 {
        UVec3::from_array(self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingCell {
    dependency: ContreeCpuVoxelSourceDependency,
    queued_frame: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingFullChunk {
    chunk_idx: UVec3,
    dependency: ContreeCpuVoxelSourceDependency,
    queued_frame: u64,
}

#[derive(Clone, Debug)]
struct FullChunkScan {
    dependency: ContreeCpuVoxelSourceDependency,
    queued_frame: u64,
    cells: BTreeSet<GridCoord>,
}

#[derive(Clone, Debug, Default)]
struct ChunkScanState {
    observed: Option<ContreeCpuVoxelSourceDependency>,
    awaiting_dependency: BTreeMap<GridCoord, u64>,
    priority: BTreeMap<GridCoord, PendingCell>,
    full: Option<FullChunkScan>,
}

/// Plans bounded, revision-matched emitter scans without owning Contree or the light provider.
/// Runtime edits stay high priority while conservative startup/load scans advance round-robin.
#[derive(Clone, Debug)]
struct EmissiveVoxelScanScheduler {
    chunk_dim: UVec3,
    voxel_dim_per_chunk: UVec3,
    cell_dim: UVec3,
    world_voxel_dim: UVec3,
    cells_per_chunk: usize,
    chunks: BTreeMap<GridCoord, ChunkScanState>,
    full_chunk_cursor: usize,
    full_turn_when_single_budget: bool,
    sparse_full_requests: bool,
}

impl EmissiveVoxelScanScheduler {
    fn new(
        chunk_dim: UVec3,
        voxel_dim_per_chunk: UVec3,
        cell_dim: UVec3,
    ) -> Result<Self, EmissiveVoxelScanSchedulerError> {
        if chunk_dim.cmpeq(UVec3::ZERO).any()
            || voxel_dim_per_chunk.cmpeq(UVec3::ZERO).any()
            || cell_dim.cmpeq(UVec3::ZERO).any()
            || (voxel_dim_per_chunk % cell_dim).cmpne(UVec3::ZERO).any()
        {
            return Err(EmissiveVoxelScanSchedulerError::InvalidGrid);
        }
        let Some(world_voxel_dim) = chunk_dim.checked_mul(voxel_dim_per_chunk) else {
            return Err(EmissiveVoxelScanSchedulerError::InvalidGrid);
        };
        let cells_per_axis = voxel_dim_per_chunk / cell_dim;
        let Some(cells_per_chunk) = usize::try_from(
            u64::from(cells_per_axis.x)
                .checked_mul(u64::from(cells_per_axis.y))
                .and_then(|count| count.checked_mul(u64::from(cells_per_axis.z)))
                .ok_or(EmissiveVoxelScanSchedulerError::InvalidGrid)?,
        )
        .ok() else {
            return Err(EmissiveVoxelScanSchedulerError::InvalidGrid);
        };
        let mut chunks = BTreeMap::new();
        for z in 0..chunk_dim.z {
            for y in 0..chunk_dim.y {
                for x in 0..chunk_dim.x {
                    chunks.insert(
                        GridCoord::new(UVec3::new(x, y, z)),
                        ChunkScanState::default(),
                    );
                }
            }
        }
        Ok(Self {
            chunk_dim,
            voxel_dim_per_chunk,
            cell_dim,
            world_voxel_dim,
            cells_per_chunk,
            chunks,
            full_chunk_cursor: 0,
            full_turn_when_single_budget: false,
            sparse_full_requests: false,
        })
    }

    /// Records a trustworthy half-open terrain-edit bound. It remains pending until a new Contree
    /// source dependency is published, so scans can never read the previous source revision.
    fn mark_trusted_change(
        &mut self,
        bound: UAabb3,
        frame: u64,
    ) -> Result<usize, EmissiveVoxelScanSchedulerError> {
        if bound.min().cmpge(bound.max()).any() || bound.max().cmpgt(self.world_voxel_dim).any() {
            return Err(EmissiveVoxelScanSchedulerError::InvalidTrustedBound(bound));
        }
        let mut touched = 0;
        for cell in grid_cells_intersecting(bound, self.cell_dim) {
            let chunk_idx = (cell.get() * self.cell_dim) / self.voxel_dim_per_chunk;
            let state = self
                .chunks
                .get_mut(&GridCoord::new(chunk_idx))
                .expect("validated world cell must belong to one configured chunk");
            match state.awaiting_dependency.entry(cell) {
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    *entry.get_mut() = (*entry.get()).min(frame);
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(frame);
                    touched += 1;
                }
            }
        }
        Ok(touched)
    }

    /// Observes a published immutable Contree dependency. A revision without a matching trusted
    /// bound is treated as a load/rebuild and conservatively schedules the whole chunk.
    fn observe_dependency(
        &mut self,
        dependency: ContreeCpuVoxelSourceDependency,
        frame: u64,
    ) -> Result<(), EmissiveVoxelScanSchedulerError> {
        if dependency.chunk_idx.cmpge(self.chunk_dim).any() {
            return Err(EmissiveVoxelScanSchedulerError::UnknownChunk(
                dependency.chunk_idx,
            ));
        }
        if dependency.source_revision.is_none() {
            return Ok(());
        }
        let chunk = GridCoord::new(dependency.chunk_idx);
        let state = self
            .chunks
            .get_mut(&chunk)
            .expect("validated chunk must have scheduler state");
        if state.observed == Some(dependency) {
            return Ok(());
        }
        state.observed = Some(dependency);

        // Absence is authoritative data, not a cache miss. There are no voxels to sample and the
        // runtime can atomically clear this chunk's provider region as soon as this dependency is
        // observed.
        if !dependency.is_present {
            state.awaiting_dependency.clear();
            state.priority.clear();
            state.full = None;
            return Ok(());
        }

        if state.awaiting_dependency.is_empty() {
            state.priority.clear();
            state.full = Some(FullChunkScan {
                dependency,
                queued_frame: frame,
                cells: full_scan_cells(
                    self.sparse_full_requests,
                    dependency.chunk_idx,
                    self.voxel_dim_per_chunk,
                    self.cell_dim,
                ),
            });
            return Ok(());
        }

        let awaiting = std::mem::take(&mut state.awaiting_dependency);
        if awaiting.len() == self.cells_per_chunk {
            let queued_frame = awaiting.values().copied().min().unwrap_or(frame);
            state.priority.clear();
            state.full = Some(FullChunkScan {
                dependency,
                queued_frame,
                cells: full_scan_cells(
                    self.sparse_full_requests,
                    dependency.chunk_idx,
                    self.voxel_dim_per_chunk,
                    self.cell_dim,
                ),
            });
            return Ok(());
        }

        for pending in state.priority.values_mut() {
            pending.dependency = dependency;
        }
        // A conservative full scan is exact-dependency staging. Even a trustworthy local edit
        // therefore restarts an already-required full scan, while its bounded cells remain in the
        // priority queue and can publish immediately. If no full scan exists, the edit does not
        // create one.
        let restarted_full_queued_frame =
            state.full.as_ref().map(|full| full.queued_frame.min(frame));
        if let Some(queued_frame) = restarted_full_queued_frame {
            state.full = Some(FullChunkScan {
                dependency,
                queued_frame,
                cells: full_scan_cells(
                    self.sparse_full_requests,
                    dependency.chunk_idx,
                    self.voxel_dim_per_chunk,
                    self.cell_dim,
                ),
            });
        }
        for (cell, queued_frame) in awaiting {
            state
                .priority
                .entry(cell)
                .and_modify(|pending| {
                    pending.dependency = dependency;
                    pending.queued_frame = pending.queued_frame.min(queued_frame);
                })
                .or_insert(PendingCell {
                    dependency,
                    queued_frame,
                });
        }
        if !self.sparse_full_requests
            && state
                .full
                .as_ref()
                .is_some_and(|full| full.cells.is_empty())
        {
            state.full = None;
        }
        Ok(())
    }

    fn backlog(&self) -> EmissiveVoxelScanBacklog {
        EmissiveVoxelScanBacklog {
            priority_cells: self.chunks.values().map(|state| state.priority.len()).sum(),
            full_chunks: self
                .chunks
                .values()
                .filter(|state| state.full.is_some())
                .count(),
            full_cells: self
                .chunks
                .values()
                .filter_map(|state| state.full.as_ref())
                .map(|full| full.cells.len())
                .sum(),
            awaiting_dependency_cells: self
                .chunks
                .values()
                .map(|state| state.awaiting_dependency.len())
                .sum(),
        }
    }

    fn restart_full_chunk(
        &mut self,
        dependency: ContreeCpuVoxelSourceDependency,
        frame: u64,
    ) -> Result<(), EmissiveVoxelScanSchedulerError> {
        if dependency.chunk_idx.cmpge(self.chunk_dim).any() || dependency.source_revision.is_none()
        {
            return Err(EmissiveVoxelScanSchedulerError::UnknownChunk(
                dependency.chunk_idx,
            ));
        }
        let state = self
            .chunks
            .get_mut(&GridCoord::new(dependency.chunk_idx))
            .expect("validated chunk must have scheduler state");
        state.observed = Some(dependency);
        state.priority.clear();
        state.full = Some(FullChunkScan {
            dependency,
            queued_frame: frame,
            cells: full_scan_cells(
                self.sparse_full_requests,
                dependency.chunk_idx,
                self.voxel_dim_per_chunk,
                self.cell_dim,
            ),
        });
        Ok(())
    }

    fn require_full_chunk_preserving_priority(
        &mut self,
        dependency: ContreeCpuVoxelSourceDependency,
        queued_frame: u64,
    ) -> Result<(), EmissiveVoxelScanSchedulerError> {
        if dependency.chunk_idx.cmpge(self.chunk_dim).any()
            || dependency.source_revision.is_none()
            || !dependency.is_present
        {
            return Err(EmissiveVoxelScanSchedulerError::UnknownChunk(
                dependency.chunk_idx,
            ));
        }
        let state = self
            .chunks
            .get_mut(&GridCoord::new(dependency.chunk_idx))
            .expect("validated chunk must have scheduler state");
        state.full = Some(FullChunkScan {
            dependency,
            queued_frame,
            cells: full_scan_cells(
                self.sparse_full_requests,
                dependency.chunk_idx,
                self.voxel_dim_per_chunk,
                self.cell_dim,
            ),
        });
        Ok(())
    }

    fn drain_full_chunk_requests(&mut self) -> Vec<PendingFullChunk> {
        self.chunks
            .iter_mut()
            .filter_map(|(chunk, state)| {
                state.full.take().map(|full| PendingFullChunk {
                    chunk_idx: chunk.get(),
                    dependency: full.dependency,
                    queued_frame: full.queued_frame,
                })
            })
            .collect()
    }

    fn take_work(&mut self, budget: usize) -> Vec<EmissiveVoxelScanWork> {
        let mut work = Vec::with_capacity(budget);
        let mut guaranteed_full_taken = false;
        while work.len() < budget {
            let backlog = self.backlog();
            let has_priority = backlog.priority_cells > 0;
            let has_full = backlog.full_cells > 0;
            if !has_priority && !has_full {
                break;
            }

            let select_full = if has_priority && has_full {
                if budget == 1 {
                    let select_full = self.full_turn_when_single_budget;
                    self.full_turn_when_single_budget = !self.full_turn_when_single_budget;
                    select_full
                } else {
                    !guaranteed_full_taken && work.len() + 1 == budget
                }
            } else {
                has_full
            };
            let next = if select_full {
                guaranteed_full_taken = true;
                self.pop_full()
            } else {
                self.pop_priority()
            };
            if let Some(next) = next {
                work.push(next);
            } else if let Some(next) = self.pop_full().or_else(|| self.pop_priority()) {
                work.push(next);
            } else {
                break;
            }
        }
        work
    }

    /// Returns failed work only if its immutable dependency is still current. An old read or
    /// failure can therefore never overwrite a task already promoted to a newer source revision.
    fn retry(&mut self, work: EmissiveVoxelScanWork) -> bool {
        let Some(state) = self.chunks.get_mut(&GridCoord::new(work.chunk_idx)) else {
            return false;
        };
        if state.observed != Some(work.dependency) {
            return false;
        }
        let cell = GridCoord::new(work.cell_bound.min() / self.cell_dim);
        match work.reason {
            EmissiveVoxelScanReason::RuntimeEdit => {
                if let Some(full) = state.full.as_mut() {
                    full.cells.remove(&cell);
                }
                state.priority.entry(cell).or_insert(PendingCell {
                    dependency: work.dependency,
                    queued_frame: work.queued_frame,
                });
            }
            EmissiveVoxelScanReason::ConservativeChunk => {
                state
                    .full
                    .get_or_insert_with(|| FullChunkScan {
                        dependency: work.dependency,
                        queued_frame: work.queued_frame,
                        cells: BTreeSet::new(),
                    })
                    .cells
                    .insert(cell);
            }
        }
        true
    }

    fn pop_priority(&mut self) -> Option<EmissiveVoxelScanWork> {
        let selected = self
            .chunks
            .iter()
            .filter_map(|(chunk, state)| {
                state
                    .priority
                    .iter()
                    .next()
                    .map(|(cell, pending)| (pending.queued_frame, *chunk, *cell))
            })
            .min()?;
        let (_, chunk, cell) = selected;
        let pending = self.chunks.get_mut(&chunk)?.priority.remove(&cell)?;
        Some(self.work(
            chunk,
            cell,
            pending.dependency,
            pending.queued_frame,
            EmissiveVoxelScanReason::RuntimeEdit,
        ))
    }

    fn pop_full(&mut self) -> Option<EmissiveVoxelScanWork> {
        let chunk_count = self.chunks.len();
        for offset in 0..chunk_count {
            let index = (self.full_chunk_cursor + offset) % chunk_count;
            let chunk = GridCoord::new(chunk_from_linear(index, self.chunk_dim));
            let state = self.chunks.get_mut(&chunk)?;
            let Some(full) = state.full.as_mut() else {
                continue;
            };
            let Some(cell) = full.cells.pop_first() else {
                state.full = None;
                continue;
            };
            let dependency = full.dependency;
            let queued_frame = full.queued_frame;
            if full.cells.is_empty() {
                state.full = None;
            }
            self.full_chunk_cursor = (index + 1) % chunk_count;
            return Some(self.work(
                chunk,
                cell,
                dependency,
                queued_frame,
                EmissiveVoxelScanReason::ConservativeChunk,
            ));
        }
        None
    }

    fn work(
        &self,
        chunk: GridCoord,
        cell: GridCoord,
        dependency: ContreeCpuVoxelSourceDependency,
        queued_frame: u64,
        reason: EmissiveVoxelScanReason,
    ) -> EmissiveVoxelScanWork {
        let min = cell.get() * self.cell_dim;
        EmissiveVoxelScanWork {
            chunk_idx: chunk.get(),
            cell_bound: UAabb3::new(min, min + self.cell_dim),
            dependency,
            queued_frame,
            reason,
        }
    }
}

fn grid_cells_intersecting(bound: UAabb3, cell_dim: UVec3) -> impl Iterator<Item = GridCoord> {
    let first = bound.min() / cell_dim;
    let last = (bound.max() - UVec3::ONE) / cell_dim;
    (first.z..=last.z).flat_map(move |z| {
        (first.y..=last.y)
            .flat_map(move |y| (first.x..=last.x).map(move |x| GridCoord::new(UVec3::new(x, y, z))))
    })
}

fn cells_in_chunk(
    chunk_idx: UVec3,
    voxel_dim_per_chunk: UVec3,
    cell_dim: UVec3,
) -> BTreeSet<GridCoord> {
    let min = chunk_idx * voxel_dim_per_chunk;
    let max = min + voxel_dim_per_chunk;
    grid_cells_intersecting(UAabb3::new(min, max), cell_dim).collect()
}

fn full_scan_cells(
    sparse_full_requests: bool,
    chunk_idx: UVec3,
    voxel_dim_per_chunk: UVec3,
    cell_dim: UVec3,
) -> BTreeSet<GridCoord> {
    if sparse_full_requests {
        BTreeSet::new()
    } else {
        cells_in_chunk(chunk_idx, voxel_dim_per_chunk, cell_dim)
    }
}

fn chunk_voxel_bound(chunk_idx: UVec3, voxel_dim_per_chunk: UVec3) -> UAabb3 {
    let min = chunk_idx * voxel_dim_per_chunk;
    UAabb3::new(min, min + voxel_dim_per_chunk)
}

fn chunk_from_linear(index: usize, chunk_dim: UVec3) -> UVec3 {
    let index = index as u32;
    let x = index % chunk_dim.x;
    let yz = index / chunk_dim.x;
    let z = yz % chunk_dim.z;
    let y = yz / chunk_dim.z;
    UVec3::new(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        builder::{
            test_cpu_voxel_source_snapshot, ContreeCpuVoxelBlock, ContreeCpuVoxelBlockExport,
            ContreeCpuVoxelSourceDependency, ContreeCpuVoxelSourceSnapshot, VOXEL_TYPE_EMISSIVE,
        },
        lighting::LocalLightRegistry,
    };
    use glam::{UVec3, Vec3};
    use re_flora_terrain_collider::{ContreeCpuChunkCache, ContreeCpuNode};
    use std::{cell::Cell, collections::BTreeMap, sync::Arc};

    fn scheduler() -> EmissiveVoxelScanScheduler {
        EmissiveVoxelScanScheduler::new(UVec3::ONE, UVec3::splat(256), UVec3::splat(16)).unwrap()
    }

    #[test]
    fn local_scan_soft_budget_guarantees_one_cell_then_stops_before_more_work() {
        assert!(should_start_next_local_cell(0, 100.0));
        assert!(should_start_next_local_cell(
            1,
            EMISSIVE_VOXEL_LOCAL_SCAN_TIME_BUDGET_MS - 0.001
        ));
        assert!(!should_start_next_local_cell(
            1,
            EMISSIVE_VOXEL_LOCAL_SCAN_TIME_BUDGET_MS
        ));
        assert!(!should_start_next_local_cell(
            EMISSIVE_VOXEL_LOCAL_SCAN_MAX_CELLS_PER_FRAME,
            0.0
        ));
    }

    fn dependency(revision: u64) -> ContreeCpuVoxelSourceDependency {
        dependency_for(UVec3::ZERO, revision)
    }

    fn dependency_for(chunk_idx: UVec3, revision: u64) -> ContreeCpuVoxelSourceDependency {
        ContreeCpuVoxelSourceDependency {
            chunk_idx,
            source_revision: Some(revision),
            is_present: true,
        }
    }

    fn sparse_snapshot(
        revision: u64,
        emitter_voxel: Option<UVec3>,
        unfinished: bool,
    ) -> ContreeCpuVoxelSourceSnapshot {
        let chunk = UVec3::ZERO;
        let caches = emitter_voxel
            .into_iter()
            .map(|voxel| {
                let child_idx = voxel.x + voxel.z * 4 + voxel.y * 16;
                let (child_mask_lo, child_mask_hi) = if child_idx < 32 {
                    (1 << child_idx, 0)
                } else {
                    (0, 1 << (child_idx - 32))
                };
                (
                    chunk,
                    Arc::new(ContreeCpuChunkCache {
                        chunk_idx: chunk,
                        nodes: vec![ContreeCpuNode {
                            packed_0: 1,
                            child_mask_lo,
                            child_mask_hi,
                        }],
                        leaves: vec![VOXEL_TYPE_EMISSIVE],
                    }),
                )
            })
            .collect::<Vec<_>>();
        let unfinished_chunks = unfinished.then_some(chunk).into_iter().collect::<Vec<_>>();
        test_cpu_voxel_source_snapshot(
            UVec3::ONE,
            UVec3::splat(4),
            &[chunk],
            caches.as_slice(),
            &[(chunk, revision)],
            &unfinished_chunks,
        )
    }

    fn async_runtime() -> EmissiveVoxelLightingRuntime {
        let mut runtime = EmissiveVoxelLightingRuntime::new_with_grid(
            UVec3::ONE,
            UVec3::splat(4),
            UVec3::splat(4),
            1,
        )
        .unwrap();
        runtime.sparse_full_scanner = Some(SparseFullChunkScanner::new().unwrap());
        runtime
    }

    #[test]
    fn initial_dependency_without_trusted_bound_schedules_one_progressive_chunk_scan() {
        let mut scheduler = scheduler();

        scheduler.observe_dependency(dependency(1), 0).unwrap();

        assert_eq!(scheduler.backlog().full_cells, 4_096);
        assert_eq!(scheduler.backlog().priority_cells, 0);
        let work = scheduler.take_work(2);
        assert_eq!(work.len(), 2);
        assert!(work
            .iter()
            .all(|work| work.reason == EmissiveVoxelScanReason::ConservativeChunk));
        assert!(work
            .iter()
            .all(|work| work.dependency.source_revision == Some(1)));
    }

    #[test]
    fn trusted_local_edit_schedules_only_its_intersecting_cell() {
        let mut scheduler = scheduler();
        scheduler.observe_dependency(dependency(1), 0).unwrap();
        assert_eq!(scheduler.take_work(4_096).len(), 4_096);
        let edited = crate::geom::UAabb3::new(UVec3::new(33, 2, 3), UVec3::new(34, 3, 4));

        assert_eq!(scheduler.mark_trusted_change(edited, 10).unwrap(), 1);
        scheduler.observe_dependency(dependency(2), 12).unwrap();

        assert_eq!(scheduler.backlog().priority_cells, 1);
        assert_eq!(scheduler.backlog().full_cells, 0);
        let work = scheduler.take_work(64);
        assert_eq!(work.len(), 1);
        assert_eq!(work[0].reason, EmissiveVoxelScanReason::RuntimeEdit);
        assert_eq!(work[0].cell_bound.min(), UVec3::new(32, 0, 0));
        assert_eq!(work[0].cell_bound.max(), UVec3::new(48, 16, 16));
        assert_eq!(work[0].dependency.source_revision, Some(2));
        assert_eq!(work[0].queued_frame, 10);
    }

    #[test]
    fn local_edit_preempts_initial_backlog_but_full_scan_still_advances() {
        let mut scheduler = scheduler();
        scheduler.observe_dependency(dependency(1), 0).unwrap();
        assert_eq!(
            scheduler.take_work(1)[0].reason,
            EmissiveVoxelScanReason::ConservativeChunk
        );
        let edited = crate::geom::UAabb3::new(UVec3::splat(240), UVec3::splat(241));
        scheduler.mark_trusted_change(edited, 3).unwrap();
        scheduler.observe_dependency(dependency(2), 4).unwrap();

        let priority = scheduler.take_work(1);
        assert_eq!(priority[0].reason, EmissiveVoxelScanReason::RuntimeEdit);
        assert_eq!(priority[0].dependency.source_revision, Some(2));
        let fair_progress = scheduler.take_work(1);
        assert_eq!(
            fair_progress[0].reason,
            EmissiveVoxelScanReason::ConservativeChunk
        );
        assert_eq!(fair_progress[0].dependency.source_revision, Some(2));
    }

    #[test]
    fn dependency_change_without_trusted_bound_conservatively_rescans_whole_chunk() {
        let mut scheduler = scheduler();
        scheduler.observe_dependency(dependency(1), 0).unwrap();
        assert_eq!(scheduler.take_work(4_096).len(), 4_096);

        scheduler.observe_dependency(dependency(2), 20).unwrap();

        assert_eq!(scheduler.backlog().full_cells, 4_096);
        assert_eq!(scheduler.backlog().priority_cells, 0);
    }

    #[test]
    fn stale_retry_cannot_replace_newer_priority_work() {
        let mut scheduler = scheduler();
        scheduler
            .mark_trusted_change(crate::geom::UAabb3::new(UVec3::ZERO, UVec3::ONE), 1)
            .unwrap();
        scheduler.observe_dependency(dependency(1), 2).unwrap();
        let stale = scheduler.take_work(1).pop().unwrap();
        scheduler
            .mark_trusted_change(crate::geom::UAabb3::new(UVec3::ZERO, UVec3::ONE), 3)
            .unwrap();
        scheduler.observe_dependency(dependency(2), 4).unwrap();

        assert!(!scheduler.retry(stale));
        let current = scheduler.take_work(1).pop().unwrap();
        assert_eq!(current.dependency.source_revision, Some(2));
        assert_eq!(current.queued_frame, 3);
    }

    #[test]
    fn conservative_backlog_round_robins_chunks_and_full_bound_uses_full_queue() {
        let mut scheduler = EmissiveVoxelScanScheduler::new(
            UVec3::new(2, 1, 1),
            UVec3::splat(32),
            UVec3::splat(16),
        )
        .unwrap();
        scheduler
            .observe_dependency(dependency_for(UVec3::ZERO, 1), 0)
            .unwrap();
        scheduler
            .observe_dependency(dependency_for(UVec3::X, 1), 0)
            .unwrap();

        let first = scheduler.take_work(4);
        assert_eq!(
            first.iter().map(|work| work.chunk_idx).collect::<Vec<_>>(),
            vec![UVec3::ZERO, UVec3::X, UVec3::ZERO, UVec3::X]
        );

        let whole_second_chunk =
            crate::geom::UAabb3::new(UVec3::new(32, 0, 0), UVec3::new(64, 32, 32));
        assert_eq!(
            scheduler
                .mark_trusted_change(whole_second_chunk, 10)
                .unwrap(),
            8
        );
        scheduler
            .observe_dependency(dependency_for(UVec3::X, 2), 11)
            .unwrap();
        assert_eq!(
            scheduler.chunks[&GridCoord::new(UVec3::X)].priority.len(),
            0
        );
        assert_eq!(
            scheduler.chunks[&GridCoord::new(UVec3::X)]
                .full
                .as_ref()
                .unwrap()
                .cells
                .len(),
            8
        );
    }

    #[derive(Clone)]
    struct FakeVoxelSource {
        dependency: ContreeCpuVoxelSourceDependency,
        cells: BTreeMap<[u32; 3], Vec<u8>>,
        cell_dim: UVec3,
        mismatch_revision: Option<u64>,
        export_calls: Cell<usize>,
    }

    impl FakeVoxelSource {
        fn new(revision: u64, cell_dim: UVec3) -> Self {
            Self {
                dependency: dependency(revision),
                cells: BTreeMap::new(),
                cell_dim,
                mismatch_revision: None,
                export_calls: Cell::new(0),
            }
        }

        fn set_cell(&mut self, min: UVec3, voxel_types: Vec<u8>) {
            self.cells.insert(min.to_array(), voxel_types);
        }
    }

    impl EmissiveVoxelSource for FakeVoxelSource {
        fn chunk_dependency(&self, _chunk_idx: UVec3) -> Option<ContreeCpuVoxelSourceDependency> {
            Some(self.dependency)
        }

        fn export_cell(&self, bound: UAabb3) -> anyhow::Result<ContreeCpuVoxelBlockExport> {
            self.export_calls.set(self.export_calls.get() + 1);
            let mut dependency = self.dependency;
            if let Some(revision) = self.mismatch_revision {
                dependency.source_revision = Some(revision);
            }
            let voxel_count = (self.cell_dim.x * self.cell_dim.y * self.cell_dim.z) as usize;
            Ok(ContreeCpuVoxelBlockExport::Ready(ContreeCpuVoxelBlock {
                voxel_min: bound.min(),
                dim: self.cell_dim,
                voxel_dim_per_chunk: UVec3::splat(32),
                voxel_types: self
                    .cells
                    .get(&bound.min().to_array())
                    .cloned()
                    .unwrap_or_else(|| vec![0; voxel_count]),
                source_dependencies: vec![dependency],
            }))
        }
    }

    #[test]
    fn x_fastest_contree_block_maps_emissive_voxels_to_world_unit_emitters() {
        let block = ContreeCpuVoxelBlock {
            voxel_min: UVec3::new(32, 64, 96),
            dim: UVec3::new(2, 2, 1),
            voxel_dim_per_chunk: UVec3::splat(256),
            voxel_types: vec![0, VOXEL_TYPE_EMISSIVE as u8, VOXEL_TYPE_EMISSIVE as u8, 0],
            source_dependencies: vec![dependency(7)],
        };

        let emitters = emissive_emitters_from_block(&block, UVec3::splat(256).as_vec3()).unwrap();

        assert_eq!(
            emitters.iter().map(|(voxel, _)| *voxel).collect::<Vec<_>>(),
            vec![UVec3::new(33, 64, 96), UVec3::new(32, 65, 96)]
        );
        let voxel_size = Vec3::splat(1.0 / 256.0);
        let average_projected_area = (voxel_size.x * voxel_size.y
            + voxel_size.y * voxel_size.z
            + voxel_size.z * voxel_size.x)
            * 0.5;
        assert_eq!(
            emitters[0].1.intensity,
            crate::lighting::EMISSIVE_VOXEL_SURFACE_RADIANCE * average_projected_area
        );
        assert_eq!(
            emitters[0].1.color,
            crate::lighting::EMISSIVE_VOXEL_COLOR_SRGB
        );
    }

    #[test]
    fn runtime_reconciles_add_and_remove_without_stale_light() {
        let cell_dim = UVec3::splat(16);
        let mut runtime =
            EmissiveVoxelLightingRuntime::new_with_grid(UVec3::ONE, UVec3::splat(32), cell_dim, 8)
                .unwrap();
        let mut source = FakeVoxelSource::new(1, cell_dim);
        let mut first_cell = vec![0; 16 * 16 * 16];
        first_cell[1] = VOXEL_TYPE_EMISSIVE as u8;
        source.set_cell(UVec3::ZERO, first_cell);
        let mut registry = LocalLightRegistry::default();

        let initial = runtime.advance_from(&source, &mut registry, 1).unwrap();
        assert_eq!(initial.full_scanned_cells, 8);
        assert_eq!(initial.full_emissive_voxels, 1);
        assert_eq!(initial.full_provider_publications, 1);
        assert_eq!(runtime.provider.voxel_count(), 1);
        assert_eq!(registry.snapshot().lights().len(), 1);

        runtime
            .mark_trusted_change(UAabb3::new(UVec3::ZERO, UVec3::ONE), 2)
            .unwrap();
        source.dependency.source_revision = Some(2);
        source.set_cell(UVec3::ZERO, vec![0; 16 * 16 * 16]);
        let removed = runtime.advance_from(&source, &mut registry, 3).unwrap();

        assert_eq!(removed.local_scanned_cells, 1);
        assert_eq!(removed.local_emissive_voxels, 0);
        assert_eq!(removed.local_provider_publications, 1);
        assert_eq!(runtime.provider.voxel_count(), 0);
        assert!(registry.snapshot().lights().is_empty());
        assert_eq!(removed.max_local_dirty_to_publication_frames, Some(1));
    }

    #[test]
    fn mismatched_snapshot_dependency_is_retried_without_publication() {
        let cell_dim = UVec3::splat(16);
        let mut runtime =
            EmissiveVoxelLightingRuntime::new_with_grid(UVec3::ONE, UVec3::splat(32), cell_dim, 1)
                .unwrap();
        let mut source = FakeVoxelSource::new(1, cell_dim);
        source.mismatch_revision = Some(2);
        let mut registry = LocalLightRegistry::default();

        let advance = runtime.advance_from(&source, &mut registry, 1).unwrap();

        assert_eq!(advance.full_scanned_cells, 0);
        assert_eq!(advance.stale_dependency_retries, 1);
        assert_eq!(runtime.scheduler.backlog().full_cells, 8);
        assert!(registry.snapshot().lights().is_empty());
    }

    #[test]
    fn conservative_chunk_half_scan_does_not_publish_and_completion_publishes_once() {
        let cell_dim = UVec3::splat(16);
        let mut runtime =
            EmissiveVoxelLightingRuntime::new_with_grid(UVec3::ONE, UVec3::splat(32), cell_dim, 4)
                .unwrap();
        let mut source = FakeVoxelSource::new(1, cell_dim);
        let mut cell = vec![0; 16 * 16 * 16];
        cell[0] = VOXEL_TYPE_EMISSIVE as u8;
        source.set_cell(UVec3::ZERO, cell);
        let mut registry = LocalLightRegistry::default();

        let half = runtime.advance_from(&source, &mut registry, 1).unwrap();
        assert_eq!(half.full_scanned_cells, 4);
        assert_eq!(half.full_chunk_completions, 0);
        assert_eq!(half.full_provider_publications, 0);
        assert_eq!(runtime.provider.voxel_count(), 0);
        assert!(registry.snapshot().lights().is_empty());

        let complete = runtime.advance_from(&source, &mut registry, 2).unwrap();
        assert_eq!(complete.full_scanned_cells, 4);
        assert_eq!(complete.full_chunk_completions, 1);
        assert_eq!(complete.full_provider_publications, 1);
        assert_eq!(runtime.provider.voxel_count(), 1);
        assert_eq!(registry.snapshot().lights().len(), 1);
    }

    #[test]
    fn conservative_chunk_revision_change_discards_partial_staging_without_mixing() {
        let cell_dim = UVec3::splat(16);
        let mut runtime =
            EmissiveVoxelLightingRuntime::new_with_grid(UVec3::ONE, UVec3::splat(32), cell_dim, 4)
                .unwrap();
        let mut source = FakeVoxelSource::new(1, cell_dim);
        let mut old_cell = vec![0; 16 * 16 * 16];
        old_cell[0] = VOXEL_TYPE_EMISSIVE as u8;
        source.set_cell(UVec3::ZERO, old_cell);
        let mut registry = LocalLightRegistry::default();
        runtime.advance_from(&source, &mut registry, 1).unwrap();
        assert!(registry.snapshot().lights().is_empty());

        source.dependency.source_revision = Some(2);
        source.cells.clear();
        let mut new_cell = vec![0; 16 * 16 * 16];
        new_cell[0] = VOXEL_TYPE_EMISSIVE as u8;
        source.set_cell(UVec3::new(16, 16, 16), new_cell);
        let restarted = runtime.advance_from(&source, &mut registry, 2).unwrap();
        assert_eq!(restarted.discarded_full_stagings, 1);
        assert_eq!(restarted.full_chunk_completions, 0);
        assert!(registry.snapshot().lights().is_empty());

        let complete = runtime.advance_from(&source, &mut registry, 3).unwrap();
        assert_eq!(complete.full_chunk_completions, 1);
        assert_eq!(runtime.provider.voxel_count(), 1);
        assert!(runtime
            .provider
            .snapshot()
            .sources()
            .iter()
            .any(|source| source.key()
                == crate::lighting::EmissiveVoxelProvider::source_key_for_voxel(UVec3::new(
                    16, 16, 16
                ))));
        assert!(!runtime
            .provider
            .snapshot()
            .sources()
            .iter()
            .any(|source| source.key()
                == crate::lighting::EmissiveVoxelProvider::source_key_for_voxel(UVec3::ZERO)));
    }

    #[test]
    fn trusted_edit_publishes_its_cell_while_exact_full_staging_restarts() {
        let cell_dim = UVec3::splat(16);
        let mut runtime =
            EmissiveVoxelLightingRuntime::new_with_grid(UVec3::ONE, UVec3::splat(32), cell_dim, 4)
                .unwrap();
        let mut source = FakeVoxelSource::new(1, cell_dim);
        let mut registry = LocalLightRegistry::default();
        runtime.advance_from(&source, &mut registry, 1).unwrap();
        assert!(registry.snapshot().lights().is_empty());

        let edited = UAabb3::new(UVec3::ZERO, UVec3::ONE);
        runtime.mark_trusted_change(edited, 2).unwrap();
        source.dependency.source_revision = Some(2);
        let mut edited_cell = vec![0; 16 * 16 * 16];
        edited_cell[0] = VOXEL_TYPE_EMISSIVE as u8;
        source.set_cell(UVec3::ZERO, edited_cell);

        let advanced = runtime.advance_from(&source, &mut registry, 3).unwrap();

        assert_eq!(advanced.discarded_full_stagings, 1);
        assert_eq!(advanced.local_scanned_cells, 1);
        assert_eq!(advanced.local_provider_publications, 1);
        assert_eq!(advanced.full_chunk_completions, 0);
        assert_eq!(advanced.max_local_dirty_to_publication_frames, Some(1));
        assert_eq!(runtime.provider.voxel_count(), 1);
        assert_eq!(registry.snapshot().lights().len(), 1);
        assert_eq!(runtime.full_staging.len(), 1);
        assert!(runtime
            .full_staging
            .values()
            .all(|staging| staging.dependency.source_revision == Some(2)));
    }

    #[test]
    fn removed_chunk_is_cleared_by_one_atomic_full_publication() {
        let cell_dim = UVec3::splat(16);
        let mut runtime =
            EmissiveVoxelLightingRuntime::new_with_grid(UVec3::ONE, UVec3::splat(32), cell_dim, 8)
                .unwrap();
        let mut source = FakeVoxelSource::new(1, cell_dim);
        let mut cell = vec![0; 16 * 16 * 16];
        cell[0] = VOXEL_TYPE_EMISSIVE as u8;
        source.set_cell(UVec3::ZERO, cell);
        let mut registry = LocalLightRegistry::default();
        runtime.advance_from(&source, &mut registry, 1).unwrap();
        assert_eq!(registry.snapshot().lights().len(), 1);

        source.dependency.source_revision = Some(2);
        source.dependency.is_present = false;
        source.cells.clear();
        let export_calls_before_removal = source.export_calls.get();
        let removed = runtime.advance_from(&source, &mut registry, 2).unwrap();

        assert_eq!(removed.full_chunk_completions, 1);
        assert_eq!(removed.full_scanned_cells, 0);
        assert_eq!(removed.full_provider_publications, 1);
        assert_eq!(removed.local_provider_publications, 0);
        assert_eq!(source.export_calls.get(), export_calls_before_removal);
        assert_eq!(runtime.scheduler.backlog().full_cells, 0);
        assert_eq!(runtime.provider.voxel_count(), 0);
        assert!(registry.snapshot().lights().is_empty());
    }

    #[test]
    fn initially_absent_chunk_is_authoritative_empty_without_scanning_cells() {
        let cell_dim = UVec3::splat(16);
        let mut runtime =
            EmissiveVoxelLightingRuntime::new_with_grid(UVec3::ONE, UVec3::splat(32), cell_dim, 8)
                .unwrap();
        let mut source = FakeVoxelSource::new(1, cell_dim);
        source.dependency.is_present = false;
        let mut registry = LocalLightRegistry::default();

        let first = runtime.advance_from(&source, &mut registry, 1).unwrap();
        let unchanged = runtime.advance_from(&source, &mut registry, 2).unwrap();

        assert_eq!(first.full_chunk_completions, 1);
        assert_eq!(first.full_scanned_cells, 0);
        assert_eq!(first.full_provider_publications, 0);
        assert_eq!(unchanged.full_chunk_completions, 0);
        assert_eq!(source.export_calls.get(), 0);
        assert_eq!(
            runtime.scheduler.backlog(),
            EmissiveVoxelScanBacklog::default()
        );
    }

    #[test]
    fn sparse_worker_not_ready_waits_without_busy_duplicates_then_publishes_once() {
        let not_ready = sparse_snapshot(1, None, true);
        let ready = sparse_snapshot(1, Some(UVec3::new(3, 2, 1)), false);
        let mut runtime = async_runtime();
        let mut registry = LocalLightRegistry::default();
        let mut frame = 1;
        let first = runtime.advance(&not_ready, &mut registry, frame).unwrap();
        assert_eq!(first.full_worker_submissions, 1);

        let mut saw_not_ready = false;
        for _ in 0..1_000 {
            frame += 1;
            let advance = runtime.advance(&not_ready, &mut registry, frame).unwrap();
            assert_eq!(advance.full_worker_submissions, 0);
            assert_eq!(advance.full_chunk_completions, 0);
            if advance.full_worker_not_ready == 1 {
                saw_not_ready = true;
                break;
            }
            std::thread::yield_now();
        }
        assert!(
            saw_not_ready,
            "worker must return the explicit not-ready result"
        );
        frame += 1;
        let still_waiting = runtime.advance(&not_ready, &mut registry, frame).unwrap();
        assert_eq!(still_waiting.full_worker_submissions, 0);

        let mut submissions_after_ready = 0;
        let mut publications = 0;
        for _ in 0..1_000 {
            frame += 1;
            let advance = runtime.advance(&ready, &mut registry, frame).unwrap();
            submissions_after_ready += advance.full_worker_submissions;
            publications += advance.full_provider_publications;
            if advance.full_chunk_completions == 1 {
                break;
            }
            std::thread::yield_now();
        }
        assert_eq!(submissions_after_ready, 1);
        assert_eq!(publications, 1);
        assert_eq!(registry.snapshot().lights().len(), 1);

        frame += 1;
        let stable = runtime.advance(&ready, &mut registry, frame).unwrap();
        assert_eq!(stable.full_worker_submissions, 0);
        assert_eq!(stable.full_chunk_completions, 0);
        assert_eq!(stable.full_provider_publications, 0);
    }

    #[test]
    fn stale_not_ready_result_cannot_requeue_after_live_revision_changes() {
        let old_not_ready = sparse_snapshot(1, None, true);
        let current = sparse_snapshot(2, Some(UVec3::new(1, 1, 1)), false);
        let mut runtime = async_runtime();
        let mut registry = LocalLightRegistry::default();
        let mut frame = 1;
        let first = runtime
            .advance(&old_not_ready, &mut registry, frame)
            .unwrap();
        assert_eq!(first.full_worker_submissions, 1);

        let mut total_submissions = 1;
        let mut total_publications = 0;
        for _ in 0..1_000 {
            frame += 1;
            let advance = runtime.advance(&current, &mut registry, frame).unwrap();
            total_submissions += advance.full_worker_submissions;
            total_publications += advance.full_provider_publications;
            if advance.full_chunk_completions == 1 {
                break;
            }
            std::thread::yield_now();
        }

        assert_eq!(total_submissions, 2, "one request per dependency");
        assert_eq!(total_publications, 1);
        assert_eq!(registry.snapshot().lights().len(), 1);
        assert_eq!(
            runtime
                .required_sparse_full
                .get(&GridCoord::new(UVec3::ZERO)),
            None
        );
    }
}
