use super::*;
use crate::terrain_persistence::TerrainSnapshotReader;
use crate::terrain_persistence::{TerrainSnapshotMetadata, TerrainSnapshotWriter};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TerrainPersistenceStatus {
    Ready,
    Saving,
    Loading,
    Error(String),
}

impl TerrainPersistenceStatus {
    pub(super) fn label(&self) -> String {
        match self {
            Self::Ready => "Ready".to_owned(),
            Self::Saving => "Saving".to_owned(),
            Self::Loading => "Loading".to_owned(),
            Self::Error(error) => format!("Error: {error}"),
        }
    }

    pub(super) fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LoadingPhase {
    Terrain,
    Building,
    Colliders,
}

pub(super) struct LoadingState {
    pub(super) chunk_indices: Vec<UVec3>,
    pub(super) terrain_snapshot_reader: Option<TerrainSnapshotReader>,
    pub(super) current: usize,
    pub(super) step_label: String,
    pub(super) phase: LoadingPhase,
    pub(super) collider_total: usize,
}

impl LoadingState {
    fn total(&self) -> usize {
        match self.phase {
            LoadingPhase::Terrain | LoadingPhase::Building => self.chunk_indices.len(),
            LoadingPhase::Colliders => self.collider_total,
        }
    }

    fn progress_fraction(&self) -> f32 {
        if self.chunk_indices.is_empty() {
            return 1.0;
        }

        let total = self.chunk_indices.len() as f32;
        match self.phase {
            LoadingPhase::Terrain => (self.current as f32 / total) * 0.25,
            LoadingPhase::Building => 0.25 + (self.current as f32 / total) * 0.25,
            LoadingPhase::Colliders => {
                let collider_total = self.collider_total.max(1) as f32;
                0.5 + (self.current as f32 / collider_total) * 0.5
            }
        }
    }

    fn is_done(&self) -> bool {
        self.phase == LoadingPhase::Colliders
            && self.collider_total > 0
            && self.current >= self.collider_total
    }
}

impl App {
    pub(super) fn process_loading_step(&mut self) {
        let mut should_apply_debug_startup_materials = false;
        let loading = match &mut self.loading_state {
            Some(loading) => loading,
            None => return,
        };

        if loading.is_done() {
            return;
        }

        let total = loading.total();
        let current = loading.current + 1;

        match loading.phase {
            LoadingPhase::Terrain => {
                let (chunk_id, snapshot_bytes) =
                    if let Some(reader) = loading.terrain_snapshot_reader.as_mut() {
                        let chunk = reader
                            .read_next_chunk()
                            .unwrap_or_else(|err| {
                                panic!("terrain snapshot upload read failed: {err:#}")
                            })
                            .unwrap_or_else(|| {
                                panic!("terrain snapshot ended before all chunks were uploaded")
                            });
                        let chunk_id = UVec3::from_array(chunk.coordinate);
                        (chunk_id, Some(chunk.bytes))
                    } else {
                        (loading.chunk_indices[loading.current], None)
                    };
                let atlas_offset = chunk_id * VOXEL_DIM_PER_CHUNK;
                loading.step_label = format!("Terrain {}/{}", current, total);

                let result = match snapshot_bytes {
                    Some(bytes) => self.plain_builder.write_chunk_atlas_region(
                        atlas_offset,
                        VOXEL_DIM_PER_CHUNK,
                        &bytes,
                    ),
                    None => self
                        .plain_builder
                        .chunk_init(atlas_offset, VOXEL_DIM_PER_CHUNK),
                };
                if let Err(err) = result {
                    panic!("terrain snapshot/procedural atlas initialization failed for {chunk_id:?}: {err:#}");
                }

                loading.current += 1;
                if loading.current >= total {
                    if let Some(reader) = loading.terrain_snapshot_reader.as_mut() {
                        reader.finish().unwrap_or_else(|err| {
                            panic!("terrain snapshot final validation failed: {err:#}")
                        });
                        self.plain_builder.mark_all_solid_workgroups_dirty();
                    } else {
                        should_apply_debug_startup_materials = true;
                    }
                    loading.current = 0;
                    loading.phase = LoadingPhase::Building;
                }
            }
            LoadingPhase::Building => {
                let chunk_id = loading.chunk_indices[loading.current];
                let atlas_offset = chunk_id * VOXEL_DIM_PER_CHUNK;
                loading.step_label = format!("Building {}/{}", current, total);

                let active_voxel_len = match self.surface_builder.build_surface(chunk_id, false) {
                    Ok(active_voxel_len) => active_voxel_len,
                    Err(err) => {
                        log::error!("build_surface failed for {chunk_id:?}: {err}");
                        loading.current += 1;
                        return;
                    }
                };

                let scene_offsets = if active_voxel_len == 0 {
                    self.contree_builder
                        .clear_empty_surface_chunk(atlas_offset)
                        .scene_offsets
                } else {
                    match self.contree_builder.build_and_alloc(atlas_offset) {
                        Ok(scene_offsets) => scene_offsets,
                        Err(err) => {
                            log::error!("build_and_alloc failed for {chunk_id:?}: {err}");
                            loading.current += 1;
                            return;
                        }
                    }
                };

                match scene_offsets {
                    Some((node_buffer_offset, leaf_buffer_offset)) => {
                        if let Err(err) = self.scene_accel_builder.update_scene_tex(
                            chunk_id,
                            Some((node_buffer_offset, leaf_buffer_offset)),
                        ) {
                            log::error!("update_scene_tex failed for {chunk_id:?}: {err}");
                        }
                    }
                    None => {
                        if let Err(err) = self.scene_accel_builder.update_scene_tex(chunk_id, None)
                        {
                            log::error!("clear_scene_tex failed for {chunk_id:?}: {err}");
                        }
                    }
                }

                loading.current += 1;
                if loading.current >= total {
                    match self
                        .terrain_physics
                        .begin_world_terrain_collider_import(CHUNK_DIM * VOXEL_DIM_PER_CHUNK)
                    {
                        Ok(collider_total) => {
                            loading.current = 0;
                            loading.collider_total = collider_total;
                            loading.phase = LoadingPhase::Colliders;
                            loading.step_label = format!("Colliders 0/{collider_total}");
                        }
                        Err(err) => {
                            log::error!("Failed to start global terrain collider import: {err:#}");
                            loading.current = 1;
                            loading.collider_total = 1;
                            loading.phase = LoadingPhase::Colliders;
                        }
                    }
                }
            }
            LoadingPhase::Colliders => {
                match self
                    .terrain_physics
                    .process_world_terrain_collider_import(&self.contree_builder)
                {
                    Ok((completed, total)) => {
                        loading.current = completed;
                        loading.collider_total = total;
                        loading.step_label = format!("Colliders {completed}/{total}");
                    }
                    Err(err) => {
                        log::error!("Failed to import global terrain colliders: {err:#}");
                        loading.current = loading.collider_total;
                    }
                }
            }
        }

        if should_apply_debug_startup_materials {
            if let Err(err) = self.apply_debug_startup_materials() {
                log::error!("Failed to apply debug startup materials: {err}");
            }
        }
    }

    pub(super) fn render_loading_frame(&mut self) {
        let loading = match &self.loading_state {
            Some(loading) => loading,
            None => return,
        };

        let progress = loading.progress_fraction();
        let step_label = loading.step_label.clone();
        let is_done = loading.is_done();

        self.egui_renderer
            .update(&self.window_state.window(), |ctx| {
                #[allow(deprecated)]
                egui::CentralPanel::default()
                    .frame(egui::containers::Frame {
                        fill: Color32::from_rgb(20, 20, 25),
                        ..Default::default()
                    })
                    .show(ctx, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(ui.available_height() * 0.3);

                            ui.label(
                                RichText::new("Re: Flora")
                                    .size(36.0)
                                    .color(Color32::from_rgb(200, 180, 140)),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("Loading world...")
                                    .size(18.0)
                                    .color(Color32::from_rgb(160, 160, 170)),
                            );
                            ui.add_space(24.0);

                            let bar_width = ui.available_width().min(400.0);
                            let progress = if is_done { 1.0 } else { progress };
                            let bar_height = 24.0;
                            let (rect, _) = ui.allocate_at_least(
                                egui::vec2(bar_width, bar_height),
                                egui::Sense::hover(),
                            );

                            let painter = ui.painter();
                            painter.rect_filled(rect, 2.0, Color32::from_rgb(40, 40, 50));

                            let fill_width = rect.width() * progress;
                            let fill_rect = egui::Rect::from_min_max(
                                rect.min,
                                egui::pos2(rect.min.x + fill_width, rect.max.y),
                            );
                            painter.rect_filled(fill_rect, 2.0, Color32::from_rgb(100, 140, 80));

                            let pct_text = format!("{}%", (progress * 100.0) as u32);
                            let font = egui::FontId::proportional(14.0);
                            let shadow_galley = painter.layout_no_wrap(
                                pct_text.clone(),
                                font.clone(),
                                Color32::from_black_alpha(120),
                            );
                            let galley = painter.layout_no_wrap(pct_text, font, Color32::WHITE);
                            let text_pos = egui::pos2(
                                rect.center().x - galley.size().x / 2.0,
                                rect.center().y - galley.size().y / 2.0,
                            );
                            painter.galley(
                                egui::pos2(text_pos.x + 1.0, text_pos.y + 1.0),
                                shadow_galley,
                                Color32::from_black_alpha(120),
                            );
                            painter.galley(text_pos, galley, Color32::WHITE);

                            ui.add_space(12.0);

                            let status = if is_done {
                                "Finalizing...".to_owned()
                            } else {
                                step_label.clone()
                            };
                            ui.label(
                                RichText::new(status)
                                    .size(14.0)
                                    .color(Color32::from_rgb(130, 130, 140)),
                            );
                        });
                    });
            });

        self.schedule_tracer_frame_retirements();
        let frame = match self.frame_manager.begin_frame(&mut self.swapchain) {
            Ok(frame) => frame,
            Err(SwapchainFrameError::OutOfDate) => {
                self.is_resize_pending = true;
                return;
            }
            Err(error) => panic!("Error while acquiring next image. Cause: {}", error),
        };
        let frame_slot = frame.frame_slot();
        self.collect_gpu_profiler_frame(frame_slot);
        let device = self.vulkan_ctx.device();
        let cmdbuf = frame.command_buffer();
        let image_idx = frame.image_index();

        cmdbuf.begin(false);
        cmdbuf.begin_resource_state_transaction();
        if let Some(profiler) = self.gpu_profiler.as_mut() {
            profiler.begin_frame(frame_slot, cmdbuf);
        }
        let frame_gpu_scope = self.gpu_profiler.as_mut().and_then(|profiler| {
            profiler.begin_scope(
                frame_slot,
                cmdbuf,
                "frame.render",
                PipelineStage::ALL_COMMANDS,
            )
        });

        let render_area = self.window_state.window_extent();

        self.swapchain
            .record_prepare_image_for_render_pass(cmdbuf, image_idx);

        self.egui_renderer.prepare_command_buffer(device, cmdbuf);
        self.swapchain
            .record_begin_render_pass_cmdbuf(cmdbuf, image_idx, render_area);

        let egui_gpu_scope = self.gpu_profiler.as_mut().and_then(|profiler| {
            profiler.begin_scope(
                frame_slot,
                cmdbuf,
                "egui.render",
                PipelineStage::ALL_COMMANDS,
            )
        });
        self.egui_renderer
            .record_command_buffer(device, cmdbuf, render_area);
        if let Some(scope) = egui_gpu_scope {
            if let Some(profiler) = self.gpu_profiler.as_mut() {
                profiler.end_scope(frame_slot, cmdbuf, scope, PipelineStage::ALL_COMMANDS);
            }
        }

        cmdbuf.end_render_pass();

        if let Some(scope) = frame_gpu_scope {
            if let Some(profiler) = self.gpu_profiler.as_mut() {
                profiler.end_scope(frame_slot, cmdbuf, scope, PipelineStage::ALL_COMMANDS);
            }
        }

        cmdbuf.end();

        let present_result =
            self.frame_manager
                .submit_and_present(&self.vulkan_ctx, &mut self.swapchain, &frame);
        match present_result {
            Ok(is_suboptimal) if is_suboptimal => {
                self.is_resize_pending = true;
            }
            Err(SwapchainFrameError::OutOfDate) => {
                self.is_resize_pending = true;
            }
            Err(error) => panic!("Failed to present queue. Cause: {}", error),
            _ => {}
        }

        if is_done {
            self.loading_state = None;
            self.finalize_loading();
        }
    }

    pub(super) fn finalize_loading(&mut self) {
        self.vulkan_ctx.device().wait_idle();
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        if !self.contree_builder.cpu_chunk_cache_jobs_idle() {
            panic!("[TERRAIN_PERSISTENCE] Contree CPU cache was not ready at startup");
        }
        BENCH.lock().unwrap().summary();

        self.ensure_butterfly_emitter();

        if self.terrain_load_path.is_none() {
            if let Err(err) = self.plant_startup_tuned_tree() {
                log::error!("Failed to plant startup tuning tree: {}", err);
            }
        } else {
            log::info!(
                "[TERRAIN_PERSISTENCE] startup snapshot loaded; procedural tuning-tree stamp suppressed"
            );
        }

        if let Some(path) = self.terrain_save_path.clone() {
            self.save_terrain_snapshot(Path::new(&path))
                .unwrap_or_else(|err| panic!("[TERRAIN_PERSISTENCE] CLI save failed: {err:#}"));
        }

        if let Err(err) = self.spatial_sound_manager.start() {
            log::error!("Failed to start audio engine: {}", err);
        }

        self.enqueue_startup_water_terrain_collider_rebuilds();
        if self.environment_lighting_test_scene.is_none()
            && self.hybrid_transparency_test_scene.is_none()
        {
            self.observe_initial_published_terrain_for_ddgi()
                .unwrap_or_else(|err| {
                    panic!("[DDGI] initial exact voxel visibility publication failed: {err:#}")
                });
        }
        self.time_info.reset_frame_delta();
        self.render_start_time = Some(Instant::now());
    }

    pub(super) fn save_terrain_snapshot(&mut self, path: &Path) -> Result<()> {
        let start = Instant::now();
        let metadata =
            TerrainSnapshotMetadata::new(CHUNK_DIM.to_array(), VOXEL_DIM_PER_CHUNK.to_array());
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

    pub(super) fn perform_runtime_terrain_save(&mut self) {
        if !self.terrain_persistence_status.is_ready() {
            return;
        }
        self.terrain_persistence_status = TerrainPersistenceStatus::Saving;
        let path = self.terrain_snapshot_path.clone();
        let result = (|| {
            self.vulkan_ctx.device().wait_idle();
            self.contree_builder.flush_cpu_chunk_cache_jobs();
            anyhow::ensure!(
                self.contree_builder.cpu_chunk_cache_jobs_idle(),
                "Contree CPU cache did not reach Ready before save"
            );
            self.save_terrain_snapshot(Path::new(&path))
        })();
        match result {
            Ok(()) => {
                self.terrain_persistence_status = TerrainPersistenceStatus::Ready;
            }
            Err(err) => {
                log::error!(
                    "[TERRAIN_PERSISTENCE] runtime save failed path={}: {err:#}",
                    path
                );
                self.terrain_persistence_status =
                    TerrainPersistenceStatus::Error(format!("save failed: {err}"));
            }
        }
    }

    pub(super) fn perform_runtime_terrain_load(&mut self) {
        if !self.terrain_persistence_status.is_ready() {
            return;
        }
        self.terrain_persistence_status = TerrainPersistenceStatus::Loading;
        let path = self.terrain_snapshot_path.clone();
        let result = self.load_terrain_snapshot(Path::new(&path));
        match result {
            Ok(()) => {
                self.terrain_persistence_status = TerrainPersistenceStatus::Ready;
                log::info!(
                    "[TERRAIN_PERSISTENCE] runtime load complete path={}; non-terrain state retained",
                    path
                );
            }
            Err((mutated, err)) => {
                log::error!(
                    "[TERRAIN_PERSISTENCE] runtime load failed path={} mutated={} error={err:#}",
                    path,
                    mutated
                );
                if mutated {
                    self.terrain_persistence_fatal = true;
                }
                self.terrain_persistence_status =
                    TerrainPersistenceStatus::Error(format!("load failed: {err}"));
            }
        }
    }

    fn load_terrain_snapshot(&mut self, path: &Path) -> Result<(), (bool, anyhow::Error)> {
        let metadata =
            TerrainSnapshotMetadata::new(CHUNK_DIM.to_array(), VOXEL_DIM_PER_CHUNK.to_array());
        TerrainSnapshotReader::validate(path, metadata).map_err(|err| (false, err))?;
        let mut reader = TerrainSnapshotReader::open(path).map_err(|err| (false, err))?;
        self.vulkan_ctx.device().wait_idle();

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
            self.plain_builder.mark_all_solid_workgroups_dirty();
            Ok(())
        })();
        if let Err(err) = upload_result {
            return Err((mutated, err));
        }

        let chunk_ids = terrain_chunk_ids();
        if !self.enqueue_deferred_chunk_rebuilds(&chunk_ids) {
            return Err((
                true,
                anyhow::anyhow!("visible terrain rebuild failed after snapshot upload"),
            ));
        }
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        if !self.contree_builder.cpu_chunk_cache_jobs_idle() {
            return Err((
                true,
                anyhow::anyhow!("Contree CPU cache did not reach Ready after snapshot rebuild"),
            ));
        }
        self.observe_published_terrain_edit_for_ddgi(UAabb3::new(
            UVec3::ZERO,
            CHUNK_DIM * VOXEL_DIM_PER_CHUNK,
        ))
        .map_err(|err| (true, err))?;
        self.terrain_physics
            .begin_world_terrain_collider_import(CHUNK_DIM * VOXEL_DIM_PER_CHUNK)
            .map_err(|err| (true, err))?;
        loop {
            let (completed, total) = self
                .terrain_physics
                .process_world_terrain_collider_import(&self.contree_builder)
                .map_err(|err| (true, err))?;
            if completed >= total {
                break;
            }
        }
        self.terrain_persistence_water_paused = true;
        self.enqueue_startup_water_terrain_collider_rebuilds();
        self.request_vsm_history_reset();
        self.player_tools.shovel_dig_held = false;
        Ok(())
    }

    pub(super) fn maybe_resume_terrain_persistence_water(&mut self) {
        if !self.terrain_persistence_water_paused
            || !self.water_terrain_initialized
            || !self.deferred_terrain_sdf_source_refreshes.is_idle()
            || !self.deferred_terrain_sdf_collider_rebuilds.is_idle()
            || !self.deferred_water_terrain_cache_rebuilds.is_idle()
        {
            return;
        }
        self.terrain_persistence_water_paused = false;
        log::info!("[TERRAIN_PERSISTENCE] water terrain cache Ready; water simulation resumed");
    }

    #[allow(dead_code)]
    fn validate_startup_terrain_query(&mut self) {
        let rays = [
            TerrainRayQuery {
                origin: Vec3::new(0.5, 1.0, 0.5),
                direction: Vec3::new(0.0, -1.0, 0.0),
            },
            TerrainRayQuery {
                origin: Vec3::new(1.5, 1.0, 1.5),
                direction: Vec3::new(0.0, -1.0, 0.0),
            },
            TerrainRayQuery {
                origin: Vec3::new(2.5, 1.0, 3.5),
                direction: Vec3::new(0.0, -1.0, 0.0),
            },
            TerrainRayQuery {
                origin: Vec3::new(4.5, 1.0, 4.5),
                direction: Vec3::new(0.0, -1.0, 0.0),
            },
        ];

        for ray in rays {
            let cpu_start = Instant::now();
            let cpu_hit = self
                .contree_builder
                .query_terrain_ray_cpu(ray.origin, ray.direction);
            let cpu_elapsed = cpu_start.elapsed();

            let gpu_start = Instant::now();
            let gpu_hit = self.tracer.query_terrain_ray_with_validity(ray);
            let gpu_elapsed = gpu_start.elapsed();

            let format_cpu = |hit: Option<crate::builder::ContreeCpuRayHit>| match hit {
                Some(hit) => format!(
                    "hit ({:.3}, {:.3}, {:.3}) type {}",
                    hit.position.x, hit.position.y, hit.position.z, hit.voxel_type
                ),
                None => "miss".to_owned(),
            };
            let gpu_position = match &gpu_hit {
                Ok(sample) if sample.is_valid => Some(sample.position),
                _ => None,
            };
            let format_gpu =
                |result: &anyhow::Result<crate::tracer::TerrainRayHitSample>| match result {
                    Ok(sample) if sample.is_valid => format!(
                        "hit ({:.3}, {:.3}, {:.3})",
                        sample.position.x, sample.position.y, sample.position.z
                    ),
                    Ok(_) => "miss".to_owned(),
                    Err(err) => format!("error: {err}"),
                };
            let position_delta = match (cpu_hit, gpu_position) {
                (Some(cpu_hit), Some(gpu_pos)) => {
                    format!("{:.6}", cpu_hit.position.distance(gpu_pos))
                }
                _ => "n/a".to_owned(),
            };

            log::info!(
                "Terrain query validation for origin ({:.3}, {:.3}, {:.3}) dir ({:.3}, {:.3}, {:.3}): cached_chunks={}, CPU={} in {:?}, GPU={} in {:?}, delta={}",
                ray.origin.x,
                ray.origin.y,
                ray.origin.z,
                ray.direction.x,
                ray.direction.y,
                ray.direction.z,
                self.contree_builder.cpu_cached_chunk_count(),
                format_cpu(cpu_hit),
                cpu_elapsed,
                format_gpu(&gpu_hit),
                gpu_elapsed,
                position_delta,
            );
        }
    }
}
