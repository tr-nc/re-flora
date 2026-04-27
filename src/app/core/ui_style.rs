use egui::style::WidgetVisuals;
use egui::{Color32, TextureHandle};

pub(crate) const CUSTOM_GUI_FONT_PATH: Option<&str> = Some(
    "assets/font/ark-pixel-font-12px-monospaced-ttf-v2025.10.20/ark-pixel-12px-monospaced-zh_cn.ttf",
);
pub(crate) const CUSTOM_GUI_FONT_NAME: &str = "re_flora_gui_font";

pub(crate) const PANEL_BG: Color32 = Color32::from_rgb(35, 40, 40);
pub(crate) const PANEL_LIGHT: Color32 = Color32::from_rgb(50, 58, 58);
pub(crate) const PANEL_DARK: Color32 = Color32::from_rgb(25, 28, 28);
pub(crate) const TEXT_COLOR: Color32 = Color32::from_rgb(235, 230, 215);
pub(crate) const GOLD_ACCENT: Color32 = Color32::from_rgb(235, 165, 60);
pub(crate) const FLOWER_ACCENT: Color32 = Color32::from_rgb(190, 160, 210);
pub(crate) const SAGE_ACCENT: Color32 = Color32::from_rgb(110, 140, 120);
pub(crate) const SHADOW_COLOR: Color32 = Color32::from_rgb(75, 60, 85);

pub(crate) const ITEM_PANEL_SHOVEL_ICON_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/10_Wooden_Shovel.PNG";
pub(crate) const ITEM_PANEL_SHOVEL_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/10_Wooden_Shovel.PNG";
pub(crate) const ITEM_PANEL_STAFF_ICON_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/4_Wooden_Staff.PNG";
pub(crate) const ITEM_PANEL_STAFF_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/4_Wooden_Staff.PNG";
pub(crate) const ITEM_PANEL_HOE_ICON_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/28_Copper_Sickle.PNG";
pub(crate) const ITEM_PANEL_HOE_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/28_Copper_Sickle.PNG";
pub(crate) const ITEM_PANEL_SLOT_COUNT: usize = 3;
pub(crate) const SHOVEL_SLOT_INDEX: usize = 0;
pub(crate) const STAFF_SLOT_INDEX: usize = 1;
pub(crate) const HOE_SLOT_INDEX: usize = 2;
pub(crate) const MAX_VOXEL_STORAGE_PER_TYPE: u32 = 200_000;

pub(crate) fn draw_item_panel(
    ctx: &egui::Context,
    item_panel_shovel_icon: Option<&TextureHandle>,
    item_panel_staff_icon: Option<&TextureHandle>,
    item_panel_hoe_icon: Option<&TextureHandle>,
    selected_slot_idx: usize,
) {
    egui::Area::new("item_panel".into())
        .anchor(egui::Align2::CENTER_BOTTOM, egui::Vec2::new(0.0, -16.0))
        .show(ctx, |ui| {
            let panel_frame = egui::containers::Frame {
                fill: PANEL_DARK,
                inner_margin: egui::Margin::symmetric(10, 8),
                corner_radius: egui::CornerRadius::same(0),
                shadow: egui::epaint::Shadow {
                    offset: [4, 4],
                    blur: 0,
                    spread: 0,
                    color: SHADOW_COLOR,
                },
                stroke: egui::Stroke::new(2.0, FLOWER_ACCENT),
                ..Default::default()
            };

            panel_frame.show(ui, |ui| {
                let slot_size = egui::Vec2::new(52.0, 52.0);
                let icon_size = egui::Vec2::new(32.0, 32.0);

                egui::Grid::new("item_panel_slots")
                    .num_columns(ITEM_PANEL_SLOT_COUNT)
                    .spacing(egui::Vec2::new(6.0, 0.0))
                    .show(ui, |ui| {
                        for slot_idx in 0..ITEM_PANEL_SLOT_COUNT {
                            let is_selected = slot_idx == selected_slot_idx;
                            let slot_frame = egui::containers::Frame {
                                fill: PANEL_LIGHT,
                                inner_margin: egui::Margin::same(6),
                                corner_radius: egui::CornerRadius::same(0),
                                stroke: if is_selected {
                                    egui::Stroke::new(1.5, GOLD_ACCENT)
                                } else {
                                    egui::Stroke::new(1.5, SAGE_ACCENT)
                                },
                                ..Default::default()
                            };

                            slot_frame.show(ui, |ui| {
                                ui.set_min_size(slot_size);
                                ui.with_layout(
                                    egui::Layout::centered_and_justified(
                                        egui::Direction::LeftToRight,
                                    ),
                                    |ui| {
                                        if slot_idx == SHOVEL_SLOT_INDEX {
                                            if let Some(icon) = item_panel_shovel_icon {
                                                ui.add(
                                                    egui::Image::new(icon)
                                                        .fit_to_exact_size(icon_size),
                                                );
                                            }
                                        }
                                        if slot_idx == STAFF_SLOT_INDEX {
                                            if let Some(icon) = item_panel_staff_icon {
                                                ui.add(
                                                    egui::Image::new(icon)
                                                        .fit_to_exact_size(icon_size),
                                                );
                                            }
                                        }
                                        if slot_idx == HOE_SLOT_INDEX {
                                            if let Some(icon) = item_panel_hoe_icon {
                                                ui.add(
                                                    egui::Image::new(icon)
                                                        .fit_to_exact_size(icon_size),
                                                );
                                            }
                                        }
                                    },
                                );
                            });
                        }
                        ui.end_row();
                    });
            });
        });
}

pub(crate) fn draw_backpack_summary(
    ctx: &egui::Context,
    dirt_count: u32,
    sand_count: u32,
    cherry_wood_count: u32,
    oak_wood_count: u32,
    rock_count: u32,
) -> egui::Pos2 {
    egui::Area::new("backpack_summary".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-16.0, 80.0))
        .show(ctx, |ui| {
            let panel_frame = egui::containers::Frame {
                fill: PANEL_DARK,
                inner_margin: egui::Margin::symmetric(10, 8),
                corner_radius: egui::CornerRadius::same(0),
                shadow: egui::epaint::Shadow {
                    offset: [4, 4],
                    blur: 0,
                    spread: 0,
                    color: SHADOW_COLOR,
                },
                stroke: egui::Stroke::new(2.0, SAGE_ACCENT),
                ..Default::default()
            };

            panel_frame.show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Voxel Inventory")
                        .color(GOLD_ACCENT)
                        .size(12.0)
                        .strong(),
                );
                ui.separator();

                draw_voxel_count_progress(ui, "Dirt", dirt_count, Color32::from_rgb(178, 124, 80));
                draw_voxel_count_progress(ui, "Sand", sand_count, Color32::from_rgb(229, 204, 126));
                draw_voxel_count_progress(
                    ui,
                    "Cherry wood",
                    cherry_wood_count,
                    Color32::from_rgb(219, 128, 152),
                );
                draw_voxel_count_progress(
                    ui,
                    "Oak wood",
                    oak_wood_count,
                    Color32::from_rgb(159, 110, 70),
                );
                draw_voxel_count_progress(ui, "Rock", rock_count, Color32::from_rgb(168, 176, 190));
            });
        })
        .response
        .rect
        .center()
}

fn draw_voxel_count_progress(ui: &mut egui::Ui, label: &str, count: u32, fill: Color32) {
    let clamped = count.min(MAX_VOXEL_STORAGE_PER_TYPE);
    let ratio = clamped as f32 / MAX_VOXEL_STORAGE_PER_TYPE as f32;
    ui.horizontal(|ui| {
        ui.add_sized(
            egui::vec2(86.0, 16.0),
            egui::Label::new(egui::RichText::new(label).monospace()),
        );

        let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(210.0, 16.0), egui::Sense::hover());
        let painter = ui.painter_at(bar_rect);

        painter.rect_filled(bar_rect, egui::CornerRadius::same(0), PANEL_LIGHT);

        let fill_width = (bar_rect.width() * ratio).floor();
        if fill_width > 0.0 {
            let fill_rect = egui::Rect::from_min_max(
                bar_rect.min,
                egui::pos2(bar_rect.min.x + fill_width, bar_rect.max.y),
            );
            painter.rect_filled(fill_rect, egui::CornerRadius::same(0), fill);
        }

        painter.rect_stroke(
            bar_rect,
            egui::CornerRadius::same(0),
            egui::Stroke::new(1.0, SAGE_ACCENT),
            egui::StrokeKind::Inside,
        );

        painter.text(
            bar_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{count}"),
            egui::TextStyle::Monospace.resolve(ui.style()),
            TEXT_COLOR,
        );
    });
}

pub(crate) fn draw_active_voxel_display(
    ctx: &egui::Context,
    active_voxel_label: &str,
    active_voxel_color: Color32,
) {
    egui::Area::new("active_voxel_display".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-16.0, 16.0))
        .show(ctx, |ui| {
            let panel_frame = egui::containers::Frame {
                fill: PANEL_DARK,
                inner_margin: egui::Margin::symmetric(10, 8),
                corner_radius: egui::CornerRadius::same(0),
                shadow: egui::epaint::Shadow {
                    offset: [4, 4],
                    blur: 0,
                    spread: 0,
                    color: SHADOW_COLOR,
                },
                stroke: egui::Stroke::new(2.0, active_voxel_color),
                ..Default::default()
            };

            panel_frame.show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Active Voxel")
                        .color(GOLD_ACCENT)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(active_voxel_label)
                        .color(active_voxel_color)
                        .monospace()
                        .strong(),
                );
            });
        });
}

pub(crate) fn apply_gui_style(style: &mut egui::Style) {
    style.visuals.override_text_color = Some(TEXT_COLOR);
    style.visuals.hyperlink_color = GOLD_ACCENT;

    style.visuals.selection.bg_fill = FLOWER_ACCENT.linear_multiply(0.4);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, GOLD_ACCENT);

    style.visuals.window_fill = PANEL_BG;
    style.visuals.panel_fill = PANEL_BG;

    style.visuals.extreme_bg_color = PANEL_DARK;
    style.visuals.code_bg_color = PANEL_DARK;
    style.visuals.text_edit_bg_color = Some(PANEL_DARK);
    style.visuals.faint_bg_color = PANEL_DARK;

    style.visuals.window_corner_radius = egui::CornerRadius::same(0);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(0);

    style.visuals.window_stroke = egui::Stroke::new(1.5, SAGE_ACCENT);

    style.visuals.popup_shadow = egui::epaint::Shadow {
        offset: [4, 4],
        blur: 10,
        spread: 0,
        color: SHADOW_COLOR,
    };
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: [6, 6],
        blur: 12,
        spread: 0,
        color: SHADOW_COLOR,
    };

    style.visuals.window_highlight_topmost = false;
    style.visuals.button_frame = true;
    style.visuals.collapsing_header_frame = true;
    style.visuals.slider_trailing_fill = true;

    style.visuals.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.6 };

    style.spacing.item_spacing = egui::Vec2::new(10.0, 8.0);
    style.spacing.button_padding = egui::Vec2::new(10.0, 6.0);
    style.spacing.window_margin = egui::Margin::symmetric(14, 14);
    style.spacing.menu_margin = egui::Margin::symmetric(10, 8);
    style.spacing.indent = 20.0;
    style.spacing.interact_size = egui::Vec2::new(40.0, 24.0);
    style.spacing.slider_width = 200.0;
    style.spacing.icon_spacing = 8.0;

    style.spacing.scroll.floating = true;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating_width = 4.0;
    style.spacing.scroll.foreground_color = true;
    style.spacing.scroll.dormant_background_opacity = 0.0;
    style.spacing.scroll.active_background_opacity = 0.4;
    style.spacing.scroll.interact_background_opacity = 0.6;
    style.spacing.scroll.dormant_handle_opacity = 0.6;
    style.spacing.scroll.active_handle_opacity = 0.9;
    style.spacing.scroll.interact_handle_opacity = 1.0;

    style.visuals.widgets.noninteractive = widget_visuals(
        Color32::TRANSPARENT,
        Color32::TRANSPARENT,
        SAGE_ACCENT,
        TEXT_COLOR,
        1.0,
    );

    style.visuals.widgets.inactive = widget_visuals(
        PANEL_LIGHT,
        PANEL_LIGHT,
        Color32::TRANSPARENT,
        TEXT_COLOR,
        0.0,
    );

    style.visuals.widgets.hovered = widget_visuals(
        Color32::from_rgb(65, 75, 75),
        Color32::from_rgb(65, 75, 75),
        FLOWER_ACCENT,
        GOLD_ACCENT,
        1.5,
    );

    style.visuals.widgets.active = widget_visuals(
        GOLD_ACCENT,
        GOLD_ACCENT,
        GOLD_ACCENT,
        Color32::from_rgb(30, 35, 30),
        1.0,
    );

    style.visuals.widgets.open =
        widget_visuals(PANEL_LIGHT, PANEL_LIGHT, GOLD_ACCENT, TEXT_COLOR, 1.5);
}

fn widget_visuals(
    bg_fill: Color32,
    weak_bg_fill: Color32,
    stroke_color: Color32,
    text_color: Color32,
    stroke_width: f32,
) -> WidgetVisuals {
    WidgetVisuals {
        bg_fill,
        weak_bg_fill,
        bg_stroke: egui::Stroke::new(stroke_width, stroke_color),
        corner_radius: egui::CornerRadius::same(4),
        fg_stroke: egui::Stroke::new(1.5, text_color),
        expansion: 0.0,
    }
}
