use super::*;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) enum ChunkRebuildRequest {
    #[default]
    Normal,
    PreserveFlora(world_ops::FloraBrushEdit),
}

pub(super) struct TerrainChunkRebuildInFlight {
    chunk_id: UVec3,
    revision: u64,
    started_at: Instant,
    main_thread_ms: f64,
    flora_edit: Option<world_ops::FloraBrushEdit>,
    stage: TerrainChunkRebuildStage,
}

enum TerrainChunkRebuildStage {
    Surface {
        job: SurfaceBuildJob,
    },
    ContreeReady {
        active_voxel_len: u32,
        surface_total_ms: f64,
    },
    Contree {
        active_voxel_len: u32,
        surface_total_ms: f64,
        job: ContreeBuildJob,
    },
    Scene {
        active_voxel_len: u32,
        surface_total_ms: f64,
        contree_total_ms: f64,
        job: SceneTexUpdateJob,
    },
}

impl App {
    pub(super) fn deferred_chunk_rebuilds_idle(&self) -> bool {
        self.deferred_chunk_rebuilds.is_idle() && self.terrain_chunk_rebuild_inflight.is_none()
    }

    pub(super) fn finish_visible_rebuilds_for_persistence(&mut self) -> bool {
        while !self.deferred_chunk_rebuilds.is_idle()
            || self.terrain_chunk_rebuild_inflight.is_some()
        {
            if self.terrain_chunk_rebuild_inflight.is_none() {
                self.process_deferred_chunk_rebuild();
            }
            if self.terrain_chunk_rebuild_inflight.is_some()
                && !self.finish_deferred_chunk_rebuild_blocking()
            {
                return false;
            }
        }
        true
    }

    /// Consumes the one currently submitted rebuild stage without progressing
    /// to a later Surface, Contree, or scene-texture stage. Queued requests are
    /// intentionally abandoned when the application is exiting.
    pub(super) fn discard_deferred_chunk_rebuild_for_shutdown(&mut self) -> Result<()> {
        let Some(inflight) = self.terrain_chunk_rebuild_inflight.take() else {
            return Ok(());
        };
        let chunk_id = inflight.chunk_id;
        let revision = inflight.revision;
        match inflight.stage {
            TerrainChunkRebuildStage::Surface { job } => {
                self.surface_builder.discard_build_surface(job)?;
            }
            TerrainChunkRebuildStage::ContreeReady { .. } => {}
            TerrainChunkRebuildStage::Contree { job, .. } => {
                self.contree_builder
                    .discard_build_and_alloc_for_shutdown(job)?;
            }
            TerrainChunkRebuildStage::Scene { job, .. } => {
                self.scene_accel_builder.discard_update_scene_tex(job)?;
            }
        }
        self.deferred_chunk_rebuilds.complete(chunk_id, revision);
        log::info!(
            "[SHUTDOWN][DEFERRED_REBUILD] discarded active chunk {:?} revision {} without follow-up stage",
            chunk_id,
            revision,
        );
        Ok(())
    }

    pub(super) fn process_deferred_chunk_rebuild(&mut self) {
        if self.terrain_chunk_rebuild_inflight.is_some() {
            self.progress_deferred_chunk_rebuild();
            return;
        }

        self.try_start_deferred_chunk_rebuild();
    }

    pub(super) fn finish_deferred_chunk_rebuild_blocking(&mut self) -> bool {
        while self.terrain_chunk_rebuild_inflight.is_some() {
            let wait_result = match self
                .terrain_chunk_rebuild_inflight
                .as_ref()
                .map(|inflight| &inflight.stage)
            {
                Some(TerrainChunkRebuildStage::Surface { job }) => {
                    self.surface_builder.wait_build_surface(job)
                }
                Some(TerrainChunkRebuildStage::ContreeReady { .. }) => Ok(()),
                Some(TerrainChunkRebuildStage::Contree { job, .. }) => {
                    self.contree_builder.wait_build_and_alloc(job)
                }
                Some(TerrainChunkRebuildStage::Scene { job, .. }) => {
                    self.scene_accel_builder.wait_update_scene_tex(job)
                }
                None => Ok(()),
            };

            if let Err(err) = wait_result {
                log::error!(
                    "[QUEUE][DEFERRED_REBUILD] failed while draining async rebuild before visible sync rebuild: {}",
                    err,
                );
                return false;
            }

            self.progress_deferred_chunk_rebuild();
        }
        true
    }

    pub(super) fn deferred_surface_rebuild_inflight(&self) -> bool {
        matches!(
            self.terrain_chunk_rebuild_inflight
                .as_ref()
                .map(|job| &job.stage),
            Some(TerrainChunkRebuildStage::Surface { .. })
        )
    }

    fn try_start_deferred_chunk_rebuild(&mut self) {
        let player_pos = self.tracer.camera_position();
        let Some(work) = self
            .deferred_chunk_rebuilds
            .pop(ChunkPopMode::NearestWithAging {
                focus: player_pos,
                chunk_extent: VOXEL_DIM_PER_CHUNK,
            })
        else {
            return;
        };

        match work.payload {
            ChunkRebuildRequest::Normal => {
                self.start_async_surface_chunk_rebuild(work.chunk_id, work.revision, true, None);
            }
            ChunkRebuildRequest::PreserveFlora(flora_edit) => {
                self.start_async_surface_chunk_rebuild(
                    work.chunk_id,
                    work.revision,
                    false,
                    Some(flora_edit),
                );
            }
        }
    }

    fn start_async_surface_chunk_rebuild(
        &mut self,
        chunk_id: UVec3,
        revision: u64,
        place_flora: bool,
        flora_edit: Option<world_ops::FloraBrushEdit>,
    ) {
        let submit_start = Instant::now();
        match self
            .surface_builder
            .submit_build_surface(chunk_id, place_flora)
        {
            Ok(job) => {
                let submit_ms = submit_start.elapsed().as_secs_f64() * 1000.0;
                log::debug!(
                    "[QUEUE][DEFERRED_REBUILD] submit surface chunk {:?} revision {} submit_ms={:.2} pending={} active={}",
                    chunk_id,
                    revision,
                    submit_ms,
                    self.deferred_chunk_rebuilds.len(),
                    self.deferred_chunk_rebuilds.active_len(),
                );
                self.terrain_chunk_rebuild_inflight = Some(TerrainChunkRebuildInFlight {
                    chunk_id,
                    revision,
                    started_at: Instant::now(),
                    main_thread_ms: submit_ms,
                    flora_edit,
                    stage: TerrainChunkRebuildStage::Surface { job },
                });
            }
            Err(err) => {
                let elapsed_ms = submit_start.elapsed().as_secs_f32() * 1000.0;
                self.deferred_chunk_rebuilds.complete(chunk_id, revision);
                log::error!(
                    "[PERF][DEFERRED_REBUILD] chunk {:?} failed surface submit after {:.2}ms remaining {} revision {}: {}",
                    chunk_id,
                    elapsed_ms,
                    self.deferred_chunk_rebuilds.len(),
                    revision,
                    err,
                );
            }
        }
    }

    fn progress_deferred_chunk_rebuild(&mut self) {
        let Some(inflight) = self.terrain_chunk_rebuild_inflight.take() else {
            return;
        };
        let TerrainChunkRebuildInFlight {
            chunk_id,
            revision,
            started_at,
            main_thread_ms,
            flora_edit,
            stage,
        } = inflight;
        let inflight = TerrainChunkRebuildInFlight {
            chunk_id,
            revision,
            started_at,
            main_thread_ms,
            flora_edit,
            stage: TerrainChunkRebuildStage::ContreeReady {
                active_voxel_len: 0,
                surface_total_ms: 0.0,
            },
        };

        match stage {
            TerrainChunkRebuildStage::Surface { job } => {
                self.progress_deferred_chunk_surface_rebuild(inflight, job);
            }
            TerrainChunkRebuildStage::ContreeReady {
                active_voxel_len,
                surface_total_ms,
            } => {
                self.submit_deferred_chunk_contree_rebuild(
                    inflight,
                    active_voxel_len,
                    surface_total_ms,
                );
            }
            TerrainChunkRebuildStage::Contree {
                active_voxel_len,
                surface_total_ms,
                job,
            } => {
                self.finish_deferred_chunk_contree_rebuild(
                    inflight,
                    active_voxel_len,
                    surface_total_ms,
                    job,
                );
            }
            TerrainChunkRebuildStage::Scene {
                active_voxel_len,
                surface_total_ms,
                contree_total_ms,
                job,
            } => {
                self.finish_deferred_chunk_scene_update(
                    inflight,
                    active_voxel_len,
                    surface_total_ms,
                    contree_total_ms,
                    job,
                );
            }
        }
    }

    fn progress_deferred_chunk_surface_rebuild(
        &mut self,
        mut inflight: TerrainChunkRebuildInFlight,
        job: SurfaceBuildJob,
    ) {
        let ready = match self.surface_builder.build_surface_ready(&job) {
            Ok(ready) => ready,
            Err(err) => {
                log::error!(
                    "[QUEUE][DEFERRED_REBUILD] failed to poll surface chunk {:?} revision {}: {}",
                    inflight.chunk_id,
                    inflight.revision,
                    err,
                );
                inflight.stage = TerrainChunkRebuildStage::Surface { job };
                self.terrain_chunk_rebuild_inflight = Some(inflight);
                return;
            }
        };

        if !ready {
            inflight.stage = TerrainChunkRebuildStage::Surface { job };
            self.terrain_chunk_rebuild_inflight = Some(inflight);
            return;
        }

        if !self
            .deferred_chunk_rebuilds
            .is_latest_revision(inflight.chunk_id, inflight.revision)
        {
            log::debug!(
                "[QUEUE][DEFERRED_REBUILD] discard stale surface chunk {:?} revision {} wall_ms={:.2} pending={} active={}",
                inflight.chunk_id,
                inflight.revision,
                inflight.started_at.elapsed().as_secs_f32() * 1000.0,
                self.deferred_chunk_rebuilds.len(),
                self.deferred_chunk_rebuilds.active_len(),
            );
            self.deferred_chunk_rebuilds
                .complete(inflight.chunk_id, inflight.revision);
            return;
        }

        let finish_start = Instant::now();
        match self.surface_builder.finish_build_surface(job) {
            Ok(surface) => {
                let finish_ms = finish_start.elapsed().as_secs_f64() * 1000.0;
                inflight.main_thread_ms += finish_ms;
                let surface_total_ms = surface.total_ms;
                let mut preserve_flora_ms = 0.0;
                if let Some(flora_edit) = inflight.flora_edit {
                    let flora_start = Instant::now();
                    match self.surface_builder.edit_flora_instances_for_brush(
                        inflight.chunk_id,
                        flora_edit.start,
                        flora_edit.end,
                        flora_edit.radius,
                        flora_edit.tick,
                        flora_edit.spawn_time_ms,
                    ) {
                        Ok(()) => {
                            preserve_flora_ms = flora_start.elapsed().as_secs_f64() * 1000.0;
                            inflight.main_thread_ms += preserve_flora_ms;
                        }
                        Err(err) => {
                            let flora_ms = flora_start.elapsed().as_secs_f64() * 1000.0;
                            inflight.main_thread_ms += flora_ms;
                            self.deferred_chunk_rebuilds
                                .complete(inflight.chunk_id, inflight.revision);
                            log::error!(
                                "[PERF][DEFERRED_REBUILD] chunk {:?} failed preserve-flora edit after {:.2}ms wall {:.2}ms remaining {} revision {}: {}",
                                inflight.chunk_id,
                                inflight.main_thread_ms,
                                inflight.started_at.elapsed().as_secs_f64() * 1000.0,
                                self.deferred_chunk_rebuilds.len(),
                                inflight.revision,
                                err,
                            );
                            return;
                        }
                    }
                }
                log::log!(
                    if self.perf_logging {
                        log::Level::Info
                    } else {
                        log::Level::Debug
                    },
                    "[PERF][DEFERRED_REBUILD_PHASE] chunk {:?} revision {} phase surface_finish main_thread {:.2}ms surface_total {:.2}ms active_voxels {} flora {:.2}ms preserve_flora {:.2}ms place_flora {}",
                    inflight.chunk_id,
                    inflight.revision,
                    finish_ms,
                    surface_total_ms,
                    surface.active_voxel_len,
                    surface.flora_ms,
                    preserve_flora_ms,
                    surface.place_flora,
                );
                inflight.stage = TerrainChunkRebuildStage::ContreeReady {
                    active_voxel_len: surface.active_voxel_len,
                    surface_total_ms,
                };
                self.terrain_chunk_rebuild_inflight = Some(inflight);
            }
            Err(err) => {
                let finish_ms = finish_start.elapsed().as_secs_f32() * 1000.0;
                self.deferred_chunk_rebuilds
                    .complete(inflight.chunk_id, inflight.revision);
                log::error!(
                    "[PERF][DEFERRED_REBUILD] chunk {:?} failed surface finish after {:.2}ms remaining {} revision {}: {}",
                    inflight.chunk_id,
                    finish_ms,
                    self.deferred_chunk_rebuilds.len(),
                    inflight.revision,
                    err,
                );
            }
        }
    }

    fn submit_deferred_chunk_contree_rebuild(
        &mut self,
        mut inflight: TerrainChunkRebuildInFlight,
        active_voxel_len: u32,
        surface_total_ms: f64,
    ) {
        if !self
            .deferred_chunk_rebuilds
            .is_latest_revision(inflight.chunk_id, inflight.revision)
        {
            log::debug!(
                "[QUEUE][DEFERRED_REBUILD] discard stale contree-ready chunk {:?} revision {} wall_ms={:.2} pending={} active={}",
                inflight.chunk_id,
                inflight.revision,
                inflight.started_at.elapsed().as_secs_f32() * 1000.0,
                self.deferred_chunk_rebuilds.len(),
                self.deferred_chunk_rebuilds.active_len(),
            );
            self.deferred_chunk_rebuilds
                .complete(inflight.chunk_id, inflight.revision);
            return;
        }

        if active_voxel_len == 0 {
            let clear_start = Instant::now();
            let contree = self
                .contree_builder
                .clear_empty_surface_chunk(inflight.chunk_id * VOXEL_DIM_PER_CHUNK);
            let clear_ms = clear_start.elapsed().as_secs_f64() * 1000.0;
            inflight.main_thread_ms += clear_ms;

            let scene_submit_start = Instant::now();
            match self
                .scene_accel_builder
                .submit_update_scene_tex(contree.chunk_idx, contree.scene_offsets)
            {
                Ok(scene_job) => {
                    let scene_submit_ms = scene_submit_start.elapsed().as_secs_f64() * 1000.0;
                    inflight.main_thread_ms += scene_submit_ms;
                    log::log!(
                        if self.perf_logging {
                            log::Level::Info
                        } else {
                            log::Level::Debug
                        },
                        "[PERF][DEFERRED_REBUILD_PHASE] chunk {:?} revision {} phase contree_skip main_thread {:.2}ms contree_total {:.2}ms scene_submit {:.2}ms active_voxels {}",
                        inflight.chunk_id,
                        inflight.revision,
                        clear_ms,
                        contree.total_ms,
                        scene_submit_ms,
                        active_voxel_len,
                    );
                    inflight.stage = TerrainChunkRebuildStage::Scene {
                        active_voxel_len,
                        surface_total_ms,
                        contree_total_ms: contree.total_ms,
                        job: scene_job,
                    };
                    self.terrain_chunk_rebuild_inflight = Some(inflight);
                }
                Err(err) => {
                    let scene_submit_ms = scene_submit_start.elapsed().as_secs_f64() * 1000.0;
                    inflight.main_thread_ms += scene_submit_ms;
                    self.deferred_chunk_rebuilds
                        .complete(inflight.chunk_id, inflight.revision);
                    log::error!(
                        "[PERF][DEFERRED_REBUILD] chunk {:?} failed scene submit after contree skip {:.2}ms wall {:.2}ms remaining {} revision {}: {}",
                        inflight.chunk_id,
                        inflight.main_thread_ms,
                        inflight.started_at.elapsed().as_secs_f64() * 1000.0,
                        self.deferred_chunk_rebuilds.len(),
                        inflight.revision,
                        err,
                    );
                }
            }
            return;
        }

        let submit_start = Instant::now();
        match self
            .contree_builder
            .submit_build_and_alloc(inflight.chunk_id * VOXEL_DIM_PER_CHUNK)
        {
            Ok(job) => {
                let submit_ms = submit_start.elapsed().as_secs_f64() * 1000.0;
                inflight.main_thread_ms += submit_ms;
                log::log!(
                    if self.perf_logging {
                        log::Level::Info
                    } else {
                        log::Level::Debug
                    },
                    "[PERF][DEFERRED_REBUILD_PHASE] chunk {:?} revision {} phase contree_submit main_thread {:.2}ms active_voxels {}",
                    inflight.chunk_id,
                    inflight.revision,
                    submit_ms,
                    active_voxel_len,
                );
                inflight.stage = TerrainChunkRebuildStage::Contree {
                    active_voxel_len,
                    surface_total_ms,
                    job,
                };
                self.terrain_chunk_rebuild_inflight = Some(inflight);
            }
            Err(err) => {
                let submit_ms = submit_start.elapsed().as_secs_f64() * 1000.0;
                inflight.main_thread_ms += submit_ms;
                self.deferred_chunk_rebuilds
                    .complete(inflight.chunk_id, inflight.revision);
                log::error!(
                    "[PERF][DEFERRED_REBUILD] chunk {:?} failed contree submit after {:.2}ms wall {:.2}ms remaining {} revision {}: {}",
                    inflight.chunk_id,
                    inflight.main_thread_ms,
                    inflight.started_at.elapsed().as_secs_f64() * 1000.0,
                    self.deferred_chunk_rebuilds.len(),
                    inflight.revision,
                    err,
                );
            }
        }
    }

    fn finish_deferred_chunk_contree_rebuild(
        &mut self,
        mut inflight: TerrainChunkRebuildInFlight,
        active_voxel_len: u32,
        surface_total_ms: f64,
        job: ContreeBuildJob,
    ) {
        let ready = match self.contree_builder.build_and_alloc_ready(&job) {
            Ok(ready) => ready,
            Err(err) => {
                log::error!(
                    "[QUEUE][DEFERRED_REBUILD] failed to poll contree chunk {:?} revision {}: {}",
                    inflight.chunk_id,
                    inflight.revision,
                    err,
                );
                inflight.stage = TerrainChunkRebuildStage::Contree {
                    active_voxel_len,
                    surface_total_ms,
                    job,
                };
                self.terrain_chunk_rebuild_inflight = Some(inflight);
                return;
            }
        };

        if !ready {
            inflight.stage = TerrainChunkRebuildStage::Contree {
                active_voxel_len,
                surface_total_ms,
                job,
            };
            self.terrain_chunk_rebuild_inflight = Some(inflight);
            return;
        }

        if !self
            .deferred_chunk_rebuilds
            .is_latest_revision(inflight.chunk_id, inflight.revision)
        {
            log::debug!(
                "[QUEUE][DEFERRED_REBUILD] discard stale contree chunk {:?} revision {} wall_ms={:.2} pending={} active={}",
                inflight.chunk_id,
                inflight.revision,
                inflight.started_at.elapsed().as_secs_f32() * 1000.0,
                self.deferred_chunk_rebuilds.len(),
                self.deferred_chunk_rebuilds.active_len(),
            );
            self.contree_builder.discard_build_and_alloc(job);
            self.deferred_chunk_rebuilds
                .complete(inflight.chunk_id, inflight.revision);
            return;
        }

        let finish_start = Instant::now();
        let contree = match self.contree_builder.finish_build_and_alloc(job) {
            Ok(contree) => contree,
            Err(err) => {
                let finish_ms = finish_start.elapsed().as_secs_f64() * 1000.0;
                inflight.main_thread_ms += finish_ms;
                self.deferred_chunk_rebuilds
                    .complete(inflight.chunk_id, inflight.revision);
                log::error!(
                    "[PERF][DEFERRED_REBUILD] chunk {:?} failed contree finish after {:.2}ms wall {:.2}ms remaining {} revision {}: {}",
                    inflight.chunk_id,
                    inflight.main_thread_ms,
                    inflight.started_at.elapsed().as_secs_f64() * 1000.0,
                    self.deferred_chunk_rebuilds.len(),
                    inflight.revision,
                    err,
                );
                return;
            }
        };
        let contree_finish_ms = finish_start.elapsed().as_secs_f64() * 1000.0;
        inflight.main_thread_ms += contree_finish_ms;

        let scene_data = contree.scene_offsets;
        let scene_submit_start = Instant::now();
        match self
            .scene_accel_builder
            .submit_update_scene_tex(contree.chunk_idx, scene_data)
        {
            Ok(scene_job) => {
                let scene_submit_ms = scene_submit_start.elapsed().as_secs_f64() * 1000.0;
                inflight.main_thread_ms += scene_submit_ms;
                log::log!(
                    if self.perf_logging {
                        log::Level::Info
                    } else {
                        log::Level::Debug
                    },
                    "[PERF][DEFERRED_REBUILD_PHASE] chunk {:?} revision {} phase contree_finish main_thread {:.2}ms contree_total {:.2}ms scene_submit {:.2}ms active_voxels {}",
                    inflight.chunk_id,
                    inflight.revision,
                    contree_finish_ms,
                    contree.total_ms,
                    scene_submit_ms,
                    active_voxel_len,
                );
                inflight.stage = TerrainChunkRebuildStage::Scene {
                    active_voxel_len,
                    surface_total_ms,
                    contree_total_ms: contree.total_ms,
                    job: scene_job,
                };
                self.terrain_chunk_rebuild_inflight = Some(inflight);
            }
            Err(err) => {
                let scene_submit_ms = scene_submit_start.elapsed().as_secs_f64() * 1000.0;
                inflight.main_thread_ms += scene_submit_ms;
                self.deferred_chunk_rebuilds
                    .complete(inflight.chunk_id, inflight.revision);
                log::error!(
                    "[PERF][DEFERRED_REBUILD] chunk {:?} failed scene submit after {:.2}ms wall {:.2}ms remaining {} revision {}: {}",
                    inflight.chunk_id,
                    inflight.main_thread_ms,
                    inflight.started_at.elapsed().as_secs_f64() * 1000.0,
                    self.deferred_chunk_rebuilds.len(),
                    inflight.revision,
                    err,
                );
            }
        }
    }

    fn finish_deferred_chunk_scene_update(
        &mut self,
        mut inflight: TerrainChunkRebuildInFlight,
        active_voxel_len: u32,
        surface_total_ms: f64,
        contree_total_ms: f64,
        job: SceneTexUpdateJob,
    ) {
        let ready = match self.scene_accel_builder.update_scene_tex_ready(&job) {
            Ok(ready) => ready,
            Err(err) => {
                log::error!(
                    "[QUEUE][DEFERRED_REBUILD] failed to poll scene update chunk {:?} revision {}: {}",
                    inflight.chunk_id,
                    inflight.revision,
                    err,
                );
                inflight.stage = TerrainChunkRebuildStage::Scene {
                    active_voxel_len,
                    surface_total_ms,
                    contree_total_ms,
                    job,
                };
                self.terrain_chunk_rebuild_inflight = Some(inflight);
                return;
            }
        };

        if !ready {
            inflight.stage = TerrainChunkRebuildStage::Scene {
                active_voxel_len,
                surface_total_ms,
                contree_total_ms,
                job,
            };
            self.terrain_chunk_rebuild_inflight = Some(inflight);
            return;
        }

        let finish_start = Instant::now();
        let scene = match self.scene_accel_builder.finish_update_scene_tex(job) {
            Ok(scene) => scene,
            Err(err) => {
                let finish_ms = finish_start.elapsed().as_secs_f64() * 1000.0;
                inflight.main_thread_ms += finish_ms;
                self.deferred_chunk_rebuilds
                    .complete(inflight.chunk_id, inflight.revision);
                log::error!(
                    "[QUEUE][DEFERRED_REBUILD] failed to finish scene update chunk {:?} revision {}: {}",
                    inflight.chunk_id,
                    inflight.revision,
                    err,
                );
                return;
            }
        };
        let finish_ms = finish_start.elapsed().as_secs_f64() * 1000.0;
        inflight.main_thread_ms += finish_ms;
        let is_latest = self
            .deferred_chunk_rebuilds
            .is_latest_revision(inflight.chunk_id, inflight.revision);
        self.deferred_chunk_rebuilds
            .complete(inflight.chunk_id, inflight.revision);
        if is_latest {
            // Deferred rebuild requests no longer retain their original voxel AABB. Fall back to
            // the rebuilt terrain chunk only on successful publication; precise synchronous edit
            // paths enqueue their original bound instead.
            self.terrain_physics
                .mark_terrain_chunks_dirty(&[inflight.chunk_id], VOXEL_DIM_PER_CHUNK);
            self.request_vsm_history_reset();
        }

        log::log!(
            if self.perf_logging {
                log::Level::Info
            } else {
                log::Level::Debug
            },
            "[PERF][DEFERRED_REBUILD] chunk {:?} total {:.2}ms wall {:.2}ms surface_total {:.2}ms contree_total {:.2}ms scene_total {:.2}ms scene_finish {:.2}ms active_voxels {} remaining {} revision {} latest={} preserve_flora={}",
            inflight.chunk_id,
            inflight.main_thread_ms,
            inflight.started_at.elapsed().as_secs_f64() * 1000.0,
            surface_total_ms,
            contree_total_ms,
            scene.total_ms,
            finish_ms,
            active_voxel_len,
            self.deferred_chunk_rebuilds.len(),
            inflight.revision,
            is_latest,
            inflight.flora_edit.is_some(),
        );
    }
}
