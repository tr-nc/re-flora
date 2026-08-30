use super::*;
use crate::terrain_persistence::TerrainSnapshotReader;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LoadingPhase {
    Terrain,
    Building,
    Colliders,
}

pub(super) struct LoadingState {
    pub(super) chunk_indices: Vec<UVec3>,
    pub(super) terrain_snapshot_reader: Option<TerrainSnapshotReader>,
    pub(super) visible_terrain_publication: Option<visible_terrain::VisibleTerrainPublication>,
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
        let mut should_apply_water_experience_terrain = false;
        let mut should_apply_house_scene = false;
        let water_experience_requested = self.water_experience_scene.is_some();
        let house_scene_requested = self.house_scene_requested;
        let phase = match self.loading_state.as_ref() {
            Some(loading) if !loading.is_done() => loading.phase,
            _ => return,
        };

        if matches!(phase, LoadingPhase::Building | LoadingPhase::Colliders) {
            self.advance_loading_visible_terrain();
            return;
        }

        let loading = match &mut self.loading_state {
            Some(loading) => loading,
            None => return,
        };

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
                        should_apply_water_experience_terrain = water_experience_requested;
                        should_apply_house_scene = house_scene_requested;
                    }
                    loading.current = 0;
                    loading.phase = LoadingPhase::Building;
                }
            }
            LoadingPhase::Building | LoadingPhase::Colliders => unreachable!(),
        }

        if should_apply_water_experience_terrain {
            self.apply_water_experience_terrain()
                .unwrap_or_else(|err| panic!("[WATER_EXPERIENCE] terrain setup failed: {err:#}"));
        }
        if should_apply_house_scene {
            self.apply_house_scene()
                .unwrap_or_else(|err| panic!("[HOUSE_SCENE] terrain setup failed: {err:#}"));
        }
    }

    fn advance_loading_visible_terrain(&mut self) {
        let mut publication = self
            .loading_state
            .as_mut()
            .expect("loading state disappeared before terrain publication")
            .visible_terrain_publication
            .take()
            .expect("loading terrain publication is already being advanced");
        let progress = publication
            .advance(self)
            .unwrap_or_else(|err| panic!("startup Visible Terrain Publication failed: {err:#}"));
        let loading = self
            .loading_state
            .as_mut()
            .expect("loading state disappeared after terrain publication advance");
        match progress {
            visible_terrain::VisibleTerrainPublicationProgress::Preparing {
                prepared_chunks,
                total_chunks,
            } => {
                loading.phase = LoadingPhase::Building;
                loading.current = prepared_chunks;
                loading.step_label = format!("Building {prepared_chunks}/{total_chunks}");
            }
            visible_terrain::VisibleTerrainPublicationProgress::ImportingColliders {
                completed,
                total,
            } => {
                loading.phase = LoadingPhase::Colliders;
                loading.current = completed;
                loading.collider_total = total;
                loading.step_label = format!("Colliders {completed}/{total}");
            }
            visible_terrain::VisibleTerrainPublicationProgress::AwaitingStartupSettlement => {
                loading.phase = LoadingPhase::Colliders;
                loading.current = loading.collider_total;
                loading.step_label = format!(
                    "Colliders {}/{}",
                    loading.collider_total, loading.collider_total
                );
            }
            visible_terrain::VisibleTerrainPublicationProgress::Complete(_) => {
                unreachable!("startup publication completes only after loading settlement")
            }
        }
        loading.visible_terrain_publication = Some(publication);
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

        let frame = match self.frame_manager.begin_frame(&mut self.swapchain) {
            Ok(frame) => frame,
            Err(SwapchainFrameError::OutOfDate) => {
                self.queue_current_frame_extent();
                return;
            }
            Err(error) => panic!("Error while acquiring next image. Cause: {}", error),
        };
        let frame_slot = frame.frame_slot();
        self.collect_gpu_profiler_frame(frame_slot);
        let device = self.vulkan_ctx.device();
        let cmdbuf = frame.command_buffer();
        assert_eq!(
            frame.frame_extent_generation(),
            self.swapchain.frame_extent_generation(),
            "loading frame extent generation is not the active swapchain generation"
        );
        self.tracer
            .assert_frame_extent_generation(frame.frame_extent_generation());
        let render_area = frame.extent();

        cmdbuf.begin(false);
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

        self.swapchain
            .record_prepare_image_for_render_pass(cmdbuf, &frame);

        self.egui_renderer.prepare_command_buffer(device, cmdbuf);
        self.swapchain
            .record_begin_render_pass_cmdbuf(cmdbuf, &frame);

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
                self.queue_current_frame_extent();
            }
            Err(SwapchainFrameError::OutOfDate) => {
                self.queue_current_frame_extent();
            }
            Err(error) => panic!("Failed to present queue. Cause: {}", error),
            _ => {}
        }

        if is_done {
            let mut publication = self
                .loading_state
                .take()
                .and_then(|mut loading| loading.visible_terrain_publication.take())
                .expect("completed loading must retain its Visible Terrain Publication");
            self.finalize_loading(&mut publication);
        }
    }

    pub(super) fn abort_loading_visible_terrain_publication(&mut self) {
        if let Some(loading) = self.loading_state.as_mut() {
            if let Some(publication) = loading.visible_terrain_publication.as_mut() {
                publication.abort(&mut self.contree_builder);
            }
        }
    }

    pub(super) fn finalize_loading(
        &mut self,
        publication: &mut visible_terrain::VisibleTerrainPublication,
    ) {
        self.vulkan_ctx.device().wait_idle();
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        if !self.contree_builder.cpu_chunk_cache_jobs_idle() {
            panic!("[TERRAIN_PERSISTENCE] Contree CPU cache was not ready at startup");
        }
        BENCH.lock().unwrap().summary();

        self.ensure_butterfly_emitter();

        if self.water_experience_scene.is_some() {
            log::info!(
                "[WATER_EXPERIENCE] procedural tuning tree suppressed for an unobstructed basin"
            );
        } else if self.house_scene_requested {
            log::info!("[HOUSE_SCENE] procedural tuning tree suppressed around the house");
        } else if !self.terrain_persistence.startup_load_requested() {
            if let Err(err) = self.plant_startup_tuned_tree() {
                log::error!("Failed to plant startup tuning tree: {}", err);
            }
        } else {
            log::info!(
                "[TERRAIN_PERSISTENCE] startup snapshot loaded; procedural tuning-tree stamp suppressed"
            );
        }

        if self
            .denoiser_bench
            .as_ref()
            .is_some_and(DenoiserBench::is_foliage_shadow)
            || self.lighting_mode_acceptance.is_active()
        {
            self.configure_foliage_shadow_bench_receiver()
                .unwrap_or_else(|err| {
                    panic!("[FOLIAGE_SHADOW_BENCH] receiver setup failed: {err:#}")
                });
        }

        if let Some(path) = self.terrain_persistence.take_startup_save_path() {
            self.perform_startup_terrain_save(Path::new(&path))
                .unwrap_or_else(|err| panic!("[TERRAIN_PERSISTENCE] CLI save failed: {err:#}"));
        }

        publication.complete_startup(self).unwrap_or_else(|err| {
            panic!("startup Visible Terrain Publication completion failed: {err:#}")
        });
        self.time_info.reset_frame_delta();
        self.render_start_time = Some(Instant::now());
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
