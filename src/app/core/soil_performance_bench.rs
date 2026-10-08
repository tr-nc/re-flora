//! Opt-in release app replay: real atlas writes, Visible Terrain Publication and rendered frames.
//! Fixture construction is unmeasured; normal startup and player settings are never changed.
use super::player_tools::ContinuousTerrainToolAction;
use super::*;
use crate::app::world_edits::TerrainBrushEdit;
use crate::builder::{VOXEL_TYPE_DIRT, VOXEL_TYPE_MASK};
use crate::terrain_deposition::{deposit_in_bounds, DepositionRequest, MAX_DEPOSITION_BATCH};
use std::path::PathBuf;

pub(super) struct SoilPerformanceBench {
    output: PathBuf,
    fixture_voxels: u32,
    frames: u32,
    warmup: u32,
    frame: u32,
    prepared: bool,
    idle: bool,
    center: Vec3,
    peak: u32,
    events: u32,
    added: u32,
    seed: u32,
    frame_ms: Vec<f64>,
    event_ms: Vec<f64>,
    max_p95_ms: Option<f64>,
    fixture_hash: u64,
    hold_seconds: Option<u32>,
    hold_start: Option<Instant>,
}

impl SoilPerformanceBench {
    pub(super) fn from_env() -> Result<Option<Self>> {
        let Some(output) = std::env::var_os("RE_FLORA_SOIL_PERF_REPORT") else {
            return Ok(None);
        };
        let setting = |name: &str, default: u32| -> Result<u32> {
            Ok(std::env::var(name)
                .ok()
                .map(|s| s.parse())
                .transpose()?
                .unwrap_or(default))
        };
        let frames = setting("RE_FLORA_SOIL_PERF_FRAMES", 240)?;
        anyhow::ensure!(
            (30..=10_000).contains(&frames),
            "soil benchmark frames must be 30..10000"
        );
        let hold_seconds: Option<u32> = std::env::var("RE_FLORA_SOIL_PERF_HOLD_SECONDS")
            .ok()
            .map(|s| s.parse())
            .transpose()?;
        anyhow::ensure!(
            hold_seconds.is_none_or(|s| (1..=30).contains(&s)),
            "native soil hold must be 1..30 seconds"
        );
        Ok(Some(Self {
            output: output.into(),
            fixture_voxels: setting("RE_FLORA_SOIL_PERF_FIXTURE", 524_288)?,
            frames,
            warmup: setting("RE_FLORA_SOIL_PERF_WARMUP", 60)?,
            frame: 0,
            prepared: false,
            idle: std::env::var_os("RE_FLORA_SOIL_PERF_IDLE").is_some(),
            center: Vec3::ZERO,
            peak: 0,
            events: 0,
            added: 0,
            seed: 0,
            frame_ms: Vec::new(),
            event_ms: Vec::new(),
            fixture_hash: 0,
            hold_seconds,
            hold_start: None,
            max_p95_ms: std::env::var("RE_FLORA_SOIL_PERF_MAX_P95_MS")
                .ok()
                .map(|s| s.parse())
                .transpose()?,
        }))
    }
}

fn atlas_hash(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn stats(samples: &[f64]) -> serde_json::Value {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    if sorted.is_empty() {
        return serde_json::json!({"count": 0});
    }
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    serde_json::json!({
        "count": sorted.len(), "mean_ms": mean,
        "median_ms": sorted[sorted.len()/2],
        "p95_ms": sorted[((sorted.len() as f64 * 0.95).ceil() as usize - 1).min(sorted.len()-1)],
        "max_ms": sorted[sorted.len()-1], "equivalent_fps": 1000. / mean,
    })
}

impl App {
    fn prepare_soil_performance_fixture(&mut self, bench: &mut SoilPerformanceBench) -> Result<()> {
        anyhow::ensure!(
            self.rooftop_scene.is_some(),
            "soil benchmark requires --rooftop-poc (unsaved isolated atlas)"
        );
        anyhow::ensure!(
            self.infinite_materials_enabled(),
            "soil benchmark requires saved Infinite Materials enabled"
        );
        let offset = rooftop_scene::SOIL_MIN;
        let dim = rooftop_scene::SOIL_MAX - offset;
        let legal = UAabb3::new(offset, offset + dim);
        let mut data = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        anyhow::ensure!(
            data.iter().all(|v| v & VOXEL_TYPE_MASK == 0),
            "soil benchmark requires an empty rooftop soil atlas"
        );
        self.debug_settings
            .adjustables
            .soil_deposition_fast_path
            .value = std::env::var_os("RE_FLORA_SOIL_PERF_FAST").is_some();
        bench.center = (offset + dim / 2).as_vec3();
        bench.center.y = offset.y as f32;
        let start = Instant::now();
        let cache = PathBuf::from(format!(
            "target/soil-perf/fixtures/reference-v1-{}-{}-{}-{}.bin",
            bench.fixture_voxels,
            self.debug_settings
                .adjustables
                .soil_deposition_spread_radius
                .value
                .to_bits(),
            self.debug_settings
                .adjustables
                .soil_deposition_clump_radius
                .value,
            self.debug_settings
                .adjustables
                .soil_deposition_repose_slope
                .value
                .to_bits()
        ));
        if cache.exists() {
            let cached = std::fs::read(&cache)?;
            let expected = std::fs::read_to_string(cache.with_extension("hash"))?;
            anyhow::ensure!(
                cached.len() == data.len()
                    && format!("{:016x}", atlas_hash(&cached)) == expected.trim(),
                "soil benchmark fixture cache is corrupt; remove {} and regenerate",
                cache.display()
            );
            data = cached;
        } else {
            let mut remaining = bench.fixture_voxels;
            let mut seed = 1;
            while remaining > 0 {
                let volume = remaining.min(MAX_DEPOSITION_BATCH);
                let result = deposit_in_bounds(
                    &mut data,
                    offset,
                    dim,
                    Some(offset.y),
                    DepositionRequest {
                        start: bench.center,
                        end: bench.center,
                        radius: self
                            .debug_settings
                            .adjustables
                            .soil_deposition_spread_radius
                            .value,
                        volume,
                        clump_radius: self
                            .debug_settings
                            .adjustables
                            .soil_deposition_clump_radius
                            .value,
                        repose_slope: self
                            .debug_settings
                            .adjustables
                            .soil_deposition_repose_slope
                            .value,
                        seed,
                    },
                    legal,
                );
                anyhow::ensure!(
                    result.added == volume && !result.needs_more_space,
                    "soil fixture could not settle all requested voxels"
                );
                remaining -= volume;
                seed += 1;
            }
            std::fs::create_dir_all(cache.parent().unwrap())?;
            std::fs::write(&cache, &data)?;
            std::fs::write(
                cache.with_extension("hash"),
                format!("{:016x}", atlas_hash(&data)),
            )?;
        }
        anyhow::ensure!(
            data.iter()
                .filter(|v| **v & VOXEL_TYPE_MASK == VOXEL_TYPE_DIRT as u8)
                .count()
                == bench.fixture_voxels as usize,
            "soil reference fixture has wrong material volume"
        );
        bench.fixture_hash = atlas_hash(&data);
        bench.peak = offset.y;
        for z in 0..dim.z {
            for y in 0..dim.y {
                for x in 0..dim.x {
                    if data[((z * dim.y + y) * dim.x + x) as usize] & VOXEL_TYPE_MASK != 0 {
                        bench.peak = bench.peak.max(offset.y + y);
                    }
                }
            }
        }
        self.plain_builder
            .write_chunk_atlas_region(offset, dim, &data)?;
        self.plain_builder.mark_all_solid_workgroups_dirty();
        self.publish_visible_terrain(VisibleTerrainChange::preserving_flora(
            UAabb3::new(offset, offset + dim - UVec3::ONE),
            world_ops::FloraBrushEdit {
                start: bench.center / 256.,
                end: bench.center / 256.,
                radius: 0.,
                tick: 0,
                spawn_time_ms: 0,
            },
            true,
        ))?;
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        let focus = Vec3::new(bench.center.x, bench.peak as f32, bench.center.z) / 256.;
        self.tracer
            .set_camera_pose_looking_at(focus + Vec3::new(0., 2.1, 0.25), focus);
        self.camera_control.set_orbit_focus(focus);
        bench.prepared = true;
        log::info!("[SOIL_PERF] fixture={} peak={} prepare_ms={:.2} batch={} rate={} spread={} clump={} slope={} idle={} warmup={} frames={} replay=one_batch_every_three_rendered_frames", bench.fixture_voxels, bench.peak, start.elapsed().as_secs_f64()*1000., self.debug_settings.adjustables.soil_deposition_batch_size.value, self.debug_settings.adjustables.soil_deposition_rate.value, self.debug_settings.adjustables.soil_deposition_spread_radius.value, self.debug_settings.adjustables.soil_deposition_clump_radius.value, self.debug_settings.adjustables.soil_deposition_repose_slope.value, bench.idle, bench.warmup, bench.frames);
        Ok(())
    }

    pub(super) fn advance_soil_performance_bench(&mut self) -> Result<()> {
        let Some(mut bench) = self.soil_performance_bench.take() else {
            return Ok(());
        };
        let result = (|| {
            if !bench.prepared {
                self.prepare_soil_performance_fixture(&mut bench)?;
            }
            if bench.frame >= bench.warmup && bench.hold_seconds.is_some() && !bench.idle {
                if bench.hold_start.is_none() {
                    self.select_item_panel_slot(ui_style::SOIL_DEPOSITION_SLOT_INDEX);
                    let extent = self.window_state.window_extent();
                    self.cursor_position_physical = Some(Vec2::new(
                        extent.width as f32 * 0.5,
                        extent.height as f32 * 0.5,
                    ));
                    self.set_tool_mouse_button_state(MouseButton::Left, ElementState::Pressed);
                    anyhow::ensure!(
                        matches!(
                            self.player_tools.begin_pointer_action(MouseButton::Left),
                            Some(PlayerToolPointerAction::Continuous(
                                ContinuousTerrainToolAction::SoilDeposit
                            ))
                        ),
                        "soil benchmark LMB did not resolve to deposition"
                    );
                    bench.hold_start = Some(Instant::now());
                }
            } else if bench.frame >= bench.warmup
                && (bench.frame - bench.warmup).is_multiple_of(3)
                && !bench.idle
            {
                bench.seed += 1;
                let center =
                    Vec3::new(bench.center.x, bench.peak as f32 + 1., bench.center.z) / 256.;
                let edit = TerrainBrushEdit {
                    start: center,
                    end: center,
                    radius: self.soil_deposition_radius(),
                };
                let volume = self
                    .debug_settings
                    .adjustables
                    .soil_deposition_batch_size
                    .value;
                let start = Instant::now();
                let result = self.apply_soil_deposition(edit, volume, bench.seed)?;
                let elapsed = start.elapsed().as_secs_f64() * 1000.;
                anyhow::ensure!(
                    result.added == volume,
                    "soil replay did not add all requested material: {} / {}",
                    result.added,
                    volume
                );
                if let Some(changed) = result.changed {
                    bench.peak = bench.peak.max(changed.max().y);
                }
                bench.events += 1;
                bench.added += result.added;
                bench.event_ms.push(elapsed);
                log::info!(
                    "[SOIL_PERF] event={} frame={} volume={} peak={} total_ms={:.3}",
                    bench.events,
                    bench.frame,
                    result.added,
                    bench.peak,
                    elapsed
                );
            }
            Ok(())
        })();
        self.soil_performance_bench = Some(bench);
        result
    }

    pub(super) fn note_soil_performance_hold(
        &mut self,
        result: &crate::terrain_deposition::DepositionResult,
        requested: u32,
        elapsed_ms: f64,
    ) {
        let Some(bench) = self.soil_performance_bench.as_mut() else {
            return;
        };
        if bench.hold_start.is_none() || requested == 0 {
            return;
        }
        assert_eq!(
            result.added, requested,
            "native soil benchmark rejected requested material"
        );
        bench.events += 1;
        bench.added += result.added;
        bench.event_ms.push(elapsed_ms);
        if let Some(changed) = result.changed {
            bench.peak = bench.peak.max(changed.max().y);
        }
    }

    pub(super) fn finish_soil_performance_bench_frame(
        &mut self,
        timing: FrameTimingSnapshot,
    ) -> Result<bool> {
        let Some(mut bench) = self.soil_performance_bench.take() else {
            return Ok(false);
        };
        if !bench.prepared {
            self.soil_performance_bench = Some(bench);
            return Ok(false);
        }
        if bench.frame >= bench.warmup {
            bench.frame_ms.push(f64::from(timing.total_ms));
        }
        bench.frame += 1;
        let completed = if let Some(seconds) = bench.hold_seconds {
            bench
                .hold_start
                .is_some_and(|start| start.elapsed() >= Duration::from_secs(u64::from(seconds)))
        } else {
            bench.frame >= bench.warmup + bench.frames
        };
        if !completed {
            self.soil_performance_bench = Some(bench);
            return Ok(false);
        }
        let hold_elapsed = bench.hold_start.map(|start| start.elapsed().as_secs_f64());
        if bench.hold_seconds.is_some() {
            self.set_tool_mouse_button_state(MouseButton::Left, ElementState::Released);
            self.refresh_terrain_edit_hold_from_mouse_buttons();
            anyhow::ensure!(
                bench.events > 10 && bench.added > 0,
                "native benchmark did not exercise LMB deposition"
            );
        }
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        let offset = rooftop_scene::SOIL_MIN;
        let dim = rooftop_scene::SOIL_MAX - offset;
        let data = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        let count = data
            .iter()
            .filter(|v| **v & VOXEL_TYPE_MASK == VOXEL_TYPE_DIRT as u8)
            .count();
        anyhow::ensure!(
            count == (bench.fixture_voxels + bench.added) as usize,
            "soil benchmark volume is not conserved"
        );
        let hash = atlas_hash(&data);
        let frame_stats = stats(&bench.frame_ms);
        let event_stats = stats(&bench.event_ms);
        let report = serde_json::json!({
            "fixture_voxels": bench.fixture_voxels, "added": bench.added, "atlas_voxels": count,
            "atlas_hash": format!("{hash:016x}"), "peak_y": bench.peak, "idle": bench.idle,
            "fast_path": self.debug_settings.adjustables.soil_deposition_fast_path.value,
            "batch": self.debug_settings.adjustables.soil_deposition_batch_size.value,
            "rate": self.debug_settings.adjustables.soil_deposition_rate.value,
            "spread": self.debug_settings.adjustables.soil_deposition_spread_radius.value,
            "clump": self.debug_settings.adjustables.soil_deposition_clump_radius.value,
            "slope": self.debug_settings.adjustables.soil_deposition_repose_slope.value,
            "fixture_hash": format!("{:016x}", bench.fixture_hash),
            "hold_seconds": bench.hold_seconds,
            "hold_elapsed_seconds": hold_elapsed,
            "replay": if bench.hold_seconds.is_some() { "native LMB hold using hover, saved rate, real clock, production stroke/flow and synchronous publication" } else { "one batch every three rendered frames; fixed work, not wall-clock holding FPS" },
            "frame": frame_stats, "event": event_stats,
            "frame_ms": bench.frame_ms, "event_ms": bench.event_ms,
        });
        if let Some(parent) = bench.output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&bench.output, serde_json::to_string_pretty(&report)?)?;
        log::info!("[SOIL_PERF] summary fixture={} events={} added={} hash={:016x} frame={} event={} output={}", bench.fixture_voxels, bench.events, bench.added, hash, frame_stats, event_stats, bench.output.display());
        if let Some(limit) = bench.max_p95_ms {
            anyhow::ensure!(
                frame_stats["p95_ms"].as_f64().unwrap_or(f64::INFINITY) <= limit,
                "soil frame p95 {}ms exceeds requested repro gate {limit}ms",
                frame_stats["p95_ms"]
            );
        }
        Ok(true)
    }
}
