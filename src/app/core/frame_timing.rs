use super::ui_style::{GOLD_ACCENT, PANEL_DARK, SAGE_ACCENT, SHADOW_COLOR};
use egui::{Color32, RichText};
use re_flora_vkn::GpuProfilerFrameResults;

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
