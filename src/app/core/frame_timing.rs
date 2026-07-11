use super::ui_style::{GOLD_ACCENT, PANEL_DARK, SAGE_ACCENT, SHADOW_COLOR};
use egui::{Color32, RichText};
use re_flora_vkn::GpuProfilerFrameResults;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FrameCpuScope {
    ContreePoll,
    TerrainSource,
    DeferredRebuild,
    WaterCache,
    ColliderQueue,
    WaterEditSoak,
    WaterHandoff,
    Particles,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FrameCpuTimings {
    enabled: bool,
    contree_poll_ms: f32,
    terrain_source_ms: f32,
    deferred_rebuild_ms: f32,
    water_cache_ms: f32,
    collider_queue_ms: f32,
    water_edit_soak_ms: f32,
    water_handoff_ms: f32,
    particles_ms: f32,
}

impl FrameCpuTimings {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    pub(super) fn time<T>(&mut self, scope: FrameCpuScope, f: impl FnOnce() -> T) -> T {
        if !self.enabled {
            return f();
        }

        let start = Instant::now();
        let output = f();
        self.add_ms(scope, start.elapsed().as_secs_f32() * 1000.0);
        output
    }

    pub(super) fn add_ms(&mut self, scope: FrameCpuScope, elapsed_ms: f32) {
        if !self.enabled {
            return;
        }

        match scope {
            FrameCpuScope::ContreePoll => self.contree_poll_ms += elapsed_ms,
            FrameCpuScope::TerrainSource => self.terrain_source_ms += elapsed_ms,
            FrameCpuScope::DeferredRebuild => self.deferred_rebuild_ms += elapsed_ms,
            FrameCpuScope::WaterCache => self.water_cache_ms += elapsed_ms,
            FrameCpuScope::ColliderQueue => self.collider_queue_ms += elapsed_ms,
            FrameCpuScope::WaterEditSoak => self.water_edit_soak_ms += elapsed_ms,
            FrameCpuScope::WaterHandoff => self.water_handoff_ms += elapsed_ms,
            FrameCpuScope::Particles => self.particles_ms += elapsed_ms,
        }
    }

    pub(super) fn queue_work_ms(&self) -> f32 {
        self.terrain_source_ms
            + self.deferred_rebuild_ms
            + self.water_cache_ms
            + self.collider_queue_ms
    }

    fn tracked_cpu_ms(&self) -> f32 {
        self.contree_poll_ms
            + self.queue_work_ms()
            + self.water_edit_soak_ms
            + self.water_handoff_ms
            + self.particles_ms
    }

    pub(super) fn snapshot(
        &self,
        frame: u64,
        total_ms: f32,
        egui_ms: f32,
        gpu_present_ms: f32,
    ) -> FrameTimingSnapshot {
        let tracked_cpu_ms = self.tracked_cpu_ms();
        let untracked_cpu_ms = (total_ms - egui_ms - gpu_present_ms - tracked_cpu_ms).max(0.0);

        FrameTimingSnapshot {
            frame,
            total_ms,
            egui_ms,
            gpu_present_ms,
            contree_poll_ms: self.contree_poll_ms,
            terrain_source_ms: self.terrain_source_ms,
            deferred_rebuild_ms: self.deferred_rebuild_ms,
            water_cache_ms: self.water_cache_ms,
            collider_queue_ms: self.collider_queue_ms,
            water_edit_soak_ms: self.water_edit_soak_ms,
            water_handoff_ms: self.water_handoff_ms,
            particles_ms: self.particles_ms,
            tracked_cpu_ms,
            untracked_cpu_ms,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FrameTimingSnapshot {
    pub(super) frame: u64,
    pub(super) total_ms: f32,
    pub(super) egui_ms: f32,
    pub(super) gpu_present_ms: f32,
    pub(super) contree_poll_ms: f32,
    pub(super) terrain_source_ms: f32,
    pub(super) deferred_rebuild_ms: f32,
    pub(super) water_cache_ms: f32,
    pub(super) collider_queue_ms: f32,
    pub(super) water_edit_soak_ms: f32,
    pub(super) water_handoff_ms: f32,
    pub(super) particles_ms: f32,
    pub(super) tracked_cpu_ms: f32,
    pub(super) untracked_cpu_ms: f32,
}

pub(super) fn draw_frame_timing_panel(
    ctx: &egui::Context,
    timing: FrameTimingSnapshot,
    gpu_results: Option<&GpuProfilerFrameResults>,
    perf_logging: bool,
) {
    let rows = [
        ("total", timing.total_ms),
        ("egui", timing.egui_ms),
        ("gpu + present", timing.gpu_present_ms),
        ("contree poll", timing.contree_poll_ms),
        ("terrain source", timing.terrain_source_ms),
        ("deferred rebuild", timing.deferred_rebuild_ms),
        ("water cache", timing.water_cache_ms),
        ("collider queue", timing.collider_queue_ms),
        ("water edit soak", timing.water_edit_soak_ms),
        ("water handoff", timing.water_handoff_ms),
        ("particles", timing.particles_ms),
        ("tracked cpu", timing.tracked_cpu_ms),
        ("untracked cpu", timing.untracked_cpu_ms),
    ];
    egui::Area::new("frame_timing_panel".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-16.0, 16.0))
        .show(ctx, |ui| {
            let timing_frame = egui::containers::Frame {
                fill: PANEL_DARK,
                inner_margin: egui::Margin::symmetric(12, 10),
                corner_radius: egui::CornerRadius::same(0),
                shadow: egui::epaint::Shadow {
                    offset: [4, 4],
                    blur: 0,
                    spread: 0,
                    color: SHADOW_COLOR,
                },
                stroke: egui::Stroke::new(2.0, GOLD_ACCENT),
                ..Default::default()
            };

            timing_frame.show(ui, |ui| {
                ui.set_width(340.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Frame Timing")
                            .color(GOLD_ACCENT)
                            .monospace()
                            .size(13.0)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("P").color(SAGE_ACCENT).monospace().size(11.0));
                    });
                });
                ui.label(
                    RichText::new(format!(
                        "previous frame {}{}",
                        timing.frame,
                        if perf_logging { " · logging" } else { "" }
                    ))
                    .color(SAGE_ACCENT)
                    .monospace()
                    .size(10.0),
                );
                ui.add_space(4.0);

                for (label, value_ms) in rows {
                    let value_us = value_ms * 1000.0;
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{label:<18}"))
                                .color(SAGE_ACCENT)
                                .monospace()
                                .size(11.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{value_us:>8.0} us"))
                                    .color(Color32::WHITE)
                                    .monospace()
                                    .size(11.0),
                            );
                        });
                    });
                }

                if let Some(gpu_results) = gpu_results {
                    ui.add_space(4.0);
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "gpu scopes{}",
                            if gpu_results.dropped_scope_count == 0 {
                                "".to_owned()
                            } else {
                                format!(" · dropped {}", gpu_results.dropped_scope_count)
                            }
                        ))
                        .color(GOLD_ACCENT)
                        .monospace()
                        .size(11.0),
                    );
                    for scope in gpu_results.scopes.iter().take(12) {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{:<18}", scope.name))
                                    .color(SAGE_ACCENT)
                                    .monospace()
                                    .size(11.0),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("{:>8.0} us", scope.duration_us()))
                                            .color(Color32::WHITE)
                                            .monospace()
                                            .size(11.0),
                                    );
                                },
                            );
                        });
                    }
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::{FrameCpuScope, FrameCpuTimings};

    #[test]
    fn disabled_cpu_timings_still_run_without_recording() {
        let mut timings = FrameCpuTimings::new(false);
        let mut ran = false;

        let value = timings.time(FrameCpuScope::ContreePoll, || {
            ran = true;
            42
        });
        timings.add_ms(FrameCpuScope::TerrainSource, 5.0);

        let snapshot = timings.snapshot(7, 20.0, 2.0, 3.0);
        assert!(ran);
        assert_eq!(value, 42);
        assert_eq!(snapshot.frame, 7);
        assert_eq!(snapshot.tracked_cpu_ms, 0.0);
        assert_eq!(snapshot.untracked_cpu_ms, 15.0);
        assert_eq!(timings.queue_work_ms(), 0.0);
    }

    #[test]
    fn cpu_timings_accumulate_scopes_and_snapshot_totals() {
        let mut timings = FrameCpuTimings::new(true);

        timings.add_ms(FrameCpuScope::ContreePoll, 1.0);
        timings.add_ms(FrameCpuScope::TerrainSource, 2.0);
        timings.add_ms(FrameCpuScope::TerrainSource, 3.0);
        timings.add_ms(FrameCpuScope::DeferredRebuild, 4.0);
        timings.add_ms(FrameCpuScope::WaterCache, 5.0);
        timings.add_ms(FrameCpuScope::ColliderQueue, 6.0);
        timings.add_ms(FrameCpuScope::WaterEditSoak, 7.0);
        timings.add_ms(FrameCpuScope::WaterHandoff, 8.0);
        timings.add_ms(FrameCpuScope::Particles, 9.0);

        let snapshot = timings.snapshot(11, 60.0, 4.0, 5.0);
        assert_eq!(snapshot.contree_poll_ms, 1.0);
        assert_eq!(snapshot.terrain_source_ms, 5.0);
        assert_eq!(snapshot.deferred_rebuild_ms, 4.0);
        assert_eq!(snapshot.water_cache_ms, 5.0);
        assert_eq!(snapshot.collider_queue_ms, 6.0);
        assert_eq!(snapshot.water_edit_soak_ms, 7.0);
        assert_eq!(snapshot.water_handoff_ms, 8.0);
        assert_eq!(snapshot.particles_ms, 9.0);
        assert_eq!(timings.queue_work_ms(), 20.0);
        assert_eq!(snapshot.tracked_cpu_ms, 45.0);
        assert_eq!(snapshot.untracked_cpu_ms, 6.0);
    }
}
