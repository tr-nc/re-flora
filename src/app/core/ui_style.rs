use super::ActiveVoxelType;
use egui::style::WidgetVisuals;
use egui::{Color32, TextureHandle};

pub(crate) const CUSTOM_GUI_FONT_PATH: Option<&str> =
    Some("assets/font/PixelifySans-VariableFont_wght.ttf");
pub(crate) const CUSTOM_GUI_FONT_NAME: &str = "verdarium_gui_font";

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
pub(crate) const ITEM_PANEL_SMOOTH_ICON_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/11_Wooden_Hoe.PNG";
pub(crate) const ITEM_PANEL_SMOOTH_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/26_Copper_Hoe.PNG";
pub(crate) const ITEM_PANEL_HOE_ICON_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/28_Copper_Sickle.PNG";
pub(crate) const ITEM_PANEL_HOE_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/28_Copper_Sickle.PNG";
pub(crate) const ITEM_PANEL_WATER_ICON_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/15_Wooden_Watering_Can.PNG";
pub(crate) const ITEM_PANEL_WATER_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/30_Copper_Watering_Can.PNG";
pub(crate) const ITEM_PANEL_SLOT_COUNT: usize = 5;
pub(crate) const SHOVEL_SLOT_INDEX: usize = 0;
pub(crate) const SMOOTH_SLOT_INDEX: usize = 1;
pub(crate) const STAFF_SLOT_INDEX: usize = 2;
pub(crate) const HOE_SLOT_INDEX: usize = 3;
pub(crate) const WATER_SLOT_INDEX: usize = 4;

pub(crate) const SHOVEL_TOOL_ACCENT: Color32 = Color32::from_rgb(178, 124, 80);
pub(crate) const SMOOTH_TOOL_ACCENT: Color32 = Color32::from_rgb(190, 156, 106);
pub(crate) const STAFF_TOOL_ACCENT: Color32 = Color32::from_rgb(129, 189, 122);
pub(crate) const HOE_TOOL_ACCENT: Color32 = Color32::from_rgb(219, 128, 152);
pub(crate) const WATER_TOOL_ACCENT: Color32 = Color32::from_rgb(96, 171, 218);

pub(crate) struct ItemPanelSlot<'a> {
    pub index: usize,
    pub label: &'static str,
    pub key_hint: &'static str,
    pub icon: Option<&'a TextureHandle>,
    pub accent: Color32,
    pub enabled: bool,
}

#[derive(Default)]
pub(crate) struct ItemPanelResponse {
    pub clicked_slot: Option<usize>,
}

#[derive(Clone, Copy)]
struct ItemPanelTheme {
    slot_size: egui::Vec2,
    icon_size: egui::Vec2,
    slot_gap: f32,
    tray_padding: egui::Vec2,
    keycap_size: egui::Vec2,
}

impl Default for ItemPanelTheme {
    fn default() -> Self {
        Self {
            slot_size: egui::Vec2::new(66.0, 62.0),
            icon_size: egui::Vec2::new(31.0, 31.0),
            slot_gap: 7.0,
            tray_padding: egui::Vec2::new(12.0, 10.0),
            keycap_size: egui::Vec2::new(16.0, 14.0),
        }
    }
}

pub(crate) fn draw_item_panel(
    ctx: &egui::Context,
    slots: &[ItemPanelSlot<'_>],
    selected_slot_idx: usize,
    interaction_enabled: bool,
) -> ItemPanelResponse {
    let theme = ItemPanelTheme::default();
    let mut panel_response = ItemPanelResponse::default();

    egui::Area::new("item_panel".into())
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::Vec2::new(0.0, -16.0))
        .show(ctx, |ui| {
            let panel_frame = egui::containers::Frame {
                fill: PANEL_DARK,
                inner_margin: egui::Margin {
                    left: theme.tray_padding.x as i8,
                    right: theme.tray_padding.x as i8,
                    top: theme.tray_padding.y as i8,
                    bottom: (theme.tray_padding.y + 3.0) as i8,
                },
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
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.slot_gap;
                    for slot in slots {
                        let clicked = draw_item_panel_slot(
                            ui,
                            slot,
                            slot.index == selected_slot_idx,
                            interaction_enabled && slot.enabled,
                            theme,
                        );
                        if clicked {
                            panel_response.clicked_slot = Some(slot.index);
                        }
                    }
                });
            });
        });

    panel_response
}

fn draw_item_panel_slot(
    ui: &mut egui::Ui,
    slot: &ItemPanelSlot<'_>,
    selected: bool,
    interaction_enabled: bool,
    theme: ItemPanelTheme,
) -> bool {
    let sense = if interaction_enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(theme.slot_size, sense);
    let response = response.on_hover_text(format!("{} [{}]", slot.label, slot.key_hint));
    let hovered = interaction_enabled && response.hovered();
    let clicked = interaction_enabled && response.clicked();
    let painter = ui.painter_at(rect);

    let fill = if selected {
        Color32::from_rgb(58, 57, 49)
    } else if hovered {
        Color32::from_rgb(54, 63, 60)
    } else {
        PANEL_LIGHT
    };
    let accent = if slot.enabled {
        slot.accent
    } else {
        SAGE_ACCENT.linear_multiply(0.45)
    };
    let border = if selected {
        egui::Stroke::new(2.0, GOLD_ACCENT)
    } else if hovered {
        egui::Stroke::new(1.5, FLOWER_ACCENT)
    } else {
        egui::Stroke::new(1.0, SAGE_ACCENT)
    };

    painter.rect_filled(rect, egui::CornerRadius::same(0), fill);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(0),
        border,
        egui::StrokeKind::Inside,
    );

    let accent_rect = egui::Rect::from_min_max(
        rect.left_top(),
        egui::pos2(rect.right(), rect.top() + if selected { 4.0 } else { 2.0 }),
    );
    painter.rect_filled(accent_rect, egui::CornerRadius::same(0), accent);

    let keycap_rect = egui::Rect::from_min_size(rect.min + egui::vec2(5.0, 7.0), theme.keycap_size);
    painter.rect_filled(keycap_rect, egui::CornerRadius::same(0), PANEL_DARK);
    painter.rect_stroke(
        keycap_rect,
        egui::CornerRadius::same(0),
        egui::Stroke::new(1.0, accent),
        egui::StrokeKind::Inside,
    );
    painter.text(
        keycap_rect.center(),
        egui::Align2::CENTER_CENTER,
        slot.key_hint,
        egui::TextStyle::Small.resolve(ui.style()),
        TEXT_COLOR,
    );

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.center().y - 1.0),
        theme.icon_size,
    );
    if let Some(icon) = slot.icon {
        egui::Image::new(icon)
            .fit_to_exact_size(theme.icon_size)
            .paint_at(ui, icon_rect);
    } else {
        painter.line_segment(
            [icon_rect.left_top(), icon_rect.right_bottom()],
            egui::Stroke::new(1.0, accent),
        );
        painter.line_segment(
            [icon_rect.right_top(), icon_rect.left_bottom()],
            egui::Stroke::new(1.0, accent),
        );
    }

    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 6.0),
        egui::Align2::CENTER_BOTTOM,
        slot.label,
        egui::TextStyle::Small.resolve(ui.style()),
        if selected { GOLD_ACCENT } else { TEXT_COLOR },
    );

    if selected {
        let notch = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.bottom() + 2.0),
            egui::vec2(24.0, 3.0),
        );
        painter.rect_filled(notch, egui::CornerRadius::same(0), GOLD_ACCENT);
    }

    clicked
}

pub(crate) struct VoxelPaletteEntry {
    pub voxel_type: ActiveVoxelType,
    pub label: &'static str,
    pub count: u32,
    pub color: Color32,
    pub selected: bool,
}

#[derive(Default)]
pub(crate) struct VoxelPaletteResponse {
    pub clicked_voxel_type: Option<ActiveVoxelType>,
    pub panel_center: Option<egui::Pos2>,
}

pub(crate) fn draw_voxel_palette(
    ctx: &egui::Context,
    entries: &[VoxelPaletteEntry],
    interaction_enabled: bool,
) -> VoxelPaletteResponse {
    let mut response = VoxelPaletteResponse::default();

    let area = egui::Area::new("voxel_palette".into())
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-16.0, 16.0))
        .show(ctx, |ui| {
            let panel_frame = egui::containers::Frame {
                fill: PANEL_DARK,
                inner_margin: egui::Margin::symmetric(12, 10),
                corner_radius: egui::CornerRadius::same(0),
                shadow: egui::epaint::Shadow {
                    offset: [4, 4],
                    blur: 0,
                    spread: 0,
                    color: SHADOW_COLOR,
                },
                stroke: egui::Stroke::new(
                    2.0,
                    entries
                        .iter()
                        .find(|entry| entry.selected)
                        .map(|entry| entry.color)
                        .unwrap_or(SAGE_ACCENT),
                ),
                ..Default::default()
            };

            panel_frame.show(ui, |ui| {
                ui.set_width(312.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("Build Material")
                                .color(GOLD_ACCENT)
                                .size(12.0)
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new("Click to switch voxel type")
                                .color(TEXT_COLOR.linear_multiply(0.78))
                                .size(10.0),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(active) = entries.iter().find(|entry| entry.selected) {
                            ui.label(
                                egui::RichText::new(active.label)
                                    .color(active.color)
                                    .monospace()
                                    .strong(),
                            );
                        }
                    });
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                for entry in entries {
                    if draw_voxel_palette_entry(ui, entry, interaction_enabled) {
                        response.clicked_voxel_type = Some(entry.voxel_type);
                    }
                    ui.add_space(4.0);
                }
            });
        });

    response.panel_center = Some(area.response.rect.center());
    response
}

fn draw_voxel_palette_entry(
    ui: &mut egui::Ui,
    entry: &VoxelPaletteEntry,
    interaction_enabled: bool,
) -> bool {
    let sense = if interaction_enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let desired_size = egui::vec2(ui.available_width(), 34.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, sense);
    let response = response.on_hover_text(entry.label);
    let hovered = interaction_enabled && response.hovered();
    let clicked = interaction_enabled && response.clicked();
    let painter = ui.painter_at(rect);

    let bg = if entry.selected {
        Color32::from_rgb(58, 57, 49)
    } else if hovered {
        Color32::from_rgb(54, 63, 60)
    } else {
        PANEL_LIGHT
    };
    let border = if entry.selected {
        egui::Stroke::new(2.0, entry.color)
    } else if hovered {
        egui::Stroke::new(1.5, FLOWER_ACCENT)
    } else {
        egui::Stroke::new(1.0, SAGE_ACCENT)
    };

    painter.rect_filled(rect, egui::CornerRadius::same(0), bg);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(0),
        border,
        egui::StrokeKind::Inside,
    );

    let swatch_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(6.0, 6.0),
        rect.min + egui::vec2(26.0, rect.height() - 6.0),
    );
    painter.rect_filled(swatch_rect, egui::CornerRadius::same(0), entry.color);
    painter.rect_stroke(
        swatch_rect,
        egui::CornerRadius::same(0),
        egui::Stroke::new(1.0, PANEL_DARK),
        egui::StrokeKind::Inside,
    );

    let label_pos = egui::pos2(swatch_rect.right() + 10.0, rect.center().y - 8.0);
    painter.text(
        label_pos,
        egui::Align2::LEFT_TOP,
        entry.label,
        egui::TextStyle::Body.resolve(ui.style()),
        if entry.selected {
            GOLD_ACCENT
        } else {
            TEXT_COLOR
        },
    );

    painter.text(
        egui::pos2(rect.right() - 10.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        format!("{:>6}", entry.count),
        egui::TextStyle::Monospace.resolve(ui.style()),
        TEXT_COLOR,
    );

    if entry.selected {
        let marker_rect = egui::Rect::from_min_max(
            egui::pos2(rect.right() - 4.0, rect.top() + 4.0),
            egui::pos2(rect.right(), rect.bottom() - 4.0),
        );
        painter.rect_filled(marker_rect, egui::CornerRadius::same(0), GOLD_ACCENT);
    }

    clicked
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
