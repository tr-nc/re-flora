use super::plant_catalog::{PlantType, PlantVariety};
use super::voxel_backpack::BackpackVoxel;
use egui::style::WidgetVisuals;
use egui::Color32;

pub(crate) const CUSTOM_GUI_FONT_PATH: Option<&str> =
    Some("assets/font/PixelifySans-VariableFont_wght.ttf");
pub(crate) const CUSTOM_GUI_FONT_NAME: &str = "re-flora_gui_font";

pub(crate) const PANEL_BG: Color32 = Color32::from_rgb(35, 40, 40);
pub(crate) const PANEL_LIGHT: Color32 = Color32::from_rgb(50, 58, 58);
pub(crate) const PANEL_DARK: Color32 = Color32::from_rgb(25, 28, 28);
pub(crate) const TEXT_COLOR: Color32 = Color32::from_rgb(235, 230, 215);
pub(crate) const GOLD_ACCENT: Color32 = Color32::from_rgb(235, 165, 60);
pub(crate) const FLOWER_ACCENT: Color32 = Color32::from_rgb(190, 160, 210);
pub(crate) const SAGE_ACCENT: Color32 = Color32::from_rgb(110, 140, 120);
pub(crate) const SHADOW_COLOR: Color32 = Color32::from_rgb(75, 60, 85);

pub(crate) const GROW_SLOT_INDEX: usize = 0;
pub(crate) const SHOVEL_SLOT_INDEX: usize = 2;
pub(crate) const HOE_SLOT_INDEX: usize = 4;
pub(crate) const WATERING_SLOT_INDEX: usize = 5;
pub(crate) const SOIL_INSPECTOR_SLOT_INDEX: usize = 6;
pub(crate) const TILLER_SLOT_INDEX: usize = 7;
pub(crate) const WIND_SLOT_INDEX: usize = 8;
pub(crate) const SPRINKLER_SLOT_INDEX: usize = 10;
pub(crate) const MOWER_SLOT_INDEX: usize = 11;
pub(crate) const SOIL_DEPOSITION_SLOT_INDEX: usize = 12;
pub(crate) const STONE_SLOT_INDEX: usize = 13;
pub(crate) const PAVEMENT_SLOT_INDEX: usize = 14;
pub(crate) const WALL_SLOT_INDEX: usize = 15;
pub(crate) const SPRINKLER_PLACEABLE_SLOT_INDEX: usize = 1;

/// Visible order, independent of the stable internal tool IDs.
pub(crate) const TOOLBAR_ORDER: [usize; 13] = [
    GROW_SLOT_INDEX,
    SHOVEL_SLOT_INDEX,
    HOE_SLOT_INDEX,
    WATERING_SLOT_INDEX,
    TILLER_SLOT_INDEX,
    SOIL_DEPOSITION_SLOT_INDEX,
    SOIL_INSPECTOR_SLOT_INDEX,
    SPRINKLER_SLOT_INDEX,
    STONE_SLOT_INDEX,
    PAVEMENT_SLOT_INDEX,
    WALL_SLOT_INDEX,
    MOWER_SLOT_INDEX,
    WIND_SLOT_INDEX,
];

pub(crate) fn toolbar_number(slot: usize) -> String {
    (TOOLBAR_ORDER
        .iter()
        .position(|&id| id == slot)
        .expect("toolbar slot")
        + 1)
    .to_string()
}

pub(crate) fn toolbar_keyboard_slot(key: winit::keyboard::KeyCode) -> Option<usize> {
    use winit::keyboard::KeyCode;
    let position = match key {
        KeyCode::Digit1 => 0,
        KeyCode::Digit2 => 1,
        KeyCode::Digit3 => 2,
        KeyCode::Digit4 => 3,
        KeyCode::Digit5 => 4,
        KeyCode::Digit6 => 5,
        KeyCode::Digit7 => 6,
        KeyCode::Digit8 => 7,
        KeyCode::Digit9 => 8,
        KeyCode::Digit0 => 9,
        _ => return None,
    };
    Some(TOOLBAR_ORDER[position])
}

pub(crate) const SOIL_INSPECTOR_TOOL_ACCENT: Color32 = Color32::from_rgb(166, 132, 88);

pub(crate) struct ItemPanelSlot {
    pub index: usize,
    pub label: &'static str,
    pub number: String,
    pub enabled: bool,
}

#[derive(Default)]
pub(crate) struct ItemPanelResponse {
    pub clicked_slot: Option<usize>,
}

pub(crate) struct PlantPanelEntry {
    pub variety: PlantVariety,
    pub label: &'static str,
    pub selected: bool,
}

#[derive(Default)]
pub(crate) struct PlantPanelResponse {
    pub clicked_variety: Option<PlantVariety>,
    pub clicked_type: Option<PlantType>,
}

const TOOL_TEXT_PADDING: f32 = 8.0;
const TOOL_TEXT_GAP: f32 = 12.0;

pub(crate) fn draw_item_panel(
    ctx: &egui::Context,
    slots: &[ItemPanelSlot],
    selected_slot_idx: Option<usize>,
    interaction_enabled: bool,
) -> ItemPanelResponse {
    let mut result = ItemPanelResponse::default();
    let style = ctx.global_style();
    // Measure the same typography used below. Wrap rather than shrinking text or
    // losing tools on small windows, and don't stretch a short bar across the screen.
    let natural_width: f32 = ctx.fonts_mut(|fonts| {
        slots
            .iter()
            .map(|slot| {
                fonts
                    .layout_no_wrap(
                        slot.label.into(),
                        egui::TextStyle::Button.resolve(&style),
                        TEXT_COLOR,
                    )
                    .size()
                    .x
                    + fonts
                        .layout_no_wrap(
                            slot.number.clone(),
                            egui::TextStyle::Small.resolve(&style),
                            TEXT_COLOR,
                        )
                        .size()
                        .x
                    + 6.0
                    + TOOL_TEXT_PADDING * 2.0
            })
            .sum::<f32>()
            + TOOL_TEXT_GAP * slots.len().saturating_sub(1) as f32
    });
    let content_width = natural_width.min((ctx.content_rect().width() - 56.0).max(1.0));
    egui::Area::new("item_panel".into())
        .order(egui::Order::Foreground)
        .movable(false)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -16.0))
        .show(ctx, |ui| {
            // The toolbar is intentionally borderless, independently of panel outlines.
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(25, 28, 28, 220))
                .inner_margin(egui::Margin::symmetric(12, 6))
                .corner_radius(6)
                .show(ui, |ui| {
                    ui.set_width(content_width);
                    ui.spacing_mut().item_spacing = egui::vec2(TOOL_TEXT_GAP, 2.0);
                    ui.horizontal_wrapped(|ui| {
                        for slot in slots {
                            if draw_tool_text(
                                ui,
                                slot,
                                selected_slot_idx == Some(slot.index),
                                interaction_enabled,
                            ) {
                                result.clicked_slot = Some(slot.index);
                            }
                        }
                    });
                });
        });
    result
}

const FLORA_PAINT_PANEL_WIDTH: f32 = 190.0;
const FLORA_PAINT_BUTTON_HEIGHT: f32 = 34.0;

pub(crate) fn draw_plant_panel(
    ctx: &egui::Context,
    entries: &[PlantPanelEntry],
    plant_type: PlantType,
    interaction_enabled: bool,
) -> PlantPanelResponse {
    let mut panel_response = PlantPanelResponse::default();
    let selected_stroke = if entries.iter().any(|entry| entry.selected) {
        GOLD_ACCENT
    } else {
        SAGE_ACCENT
    };

    egui::Area::new("flora_paint_panel".into())
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::RIGHT_CENTER, egui::Vec2::new(-16.0, -18.0))
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
                stroke: panel_stroke(ctx, 2.0, selected_stroke),
                ..Default::default()
            };

            panel_frame.show(ui, |ui| {
                ui.set_width(FLORA_PAINT_PANEL_WIDTH);
                // Keep both catalogs at half the viewport height, including frame margins.
                ui.set_height(
                    (ctx.content_rect().height() * 0.5 - panel_frame.total_margin().sum().y)
                        .max(0.0),
                );
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Planting")
                            .color(GOLD_ACCENT)
                            .size(12.0)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("GROW")
                                .color(SAGE_ACCENT)
                                .monospace()
                                .strong(),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                ui.add_enabled_ui(interaction_enabled, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Type");
                        for category in PlantType::ALL {
                            if ui
                                .selectable_label(plant_type == category, category.label())
                                .clicked()
                            {
                                panel_response.clicked_type = Some(category);
                            }
                        }
                    });
                });
                ui.add_space(4.0);
                ui.label("Variety");

                // The catalog fills the remaining fixed panel space, even for one tree.
                egui::ScrollArea::vertical()
                    .id_salt(("plant_varieties", plant_type))
                    .max_height(ui.available_height().max(FLORA_PAINT_BUTTON_HEIGHT))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for entry in entries {
                            if draw_plant_panel_entry(ui, entry, interaction_enabled) {
                                panel_response.clicked_variety = Some(entry.variety);
                            }
                            ui.add_space(4.0);
                        }
                    });
            });
        });

    panel_response
}

fn draw_plant_panel_entry(
    ui: &mut egui::Ui,
    entry: &PlantPanelEntry,
    interaction_enabled: bool,
) -> bool {
    let sense = if interaction_enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let desired_size = egui::vec2(ui.available_width(), FLORA_PAINT_BUTTON_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(desired_size, sense);
    let response = response.on_hover_text(entry.label);
    let hovered = interaction_enabled && response.hovered();
    let clicked = interaction_enabled && response.clicked();
    let painter = ui.painter_at(rect);

    let fill = if entry.selected {
        Color32::from_rgb(58, 57, 49)
    } else if hovered {
        Color32::from_rgb(54, 63, 60)
    } else {
        PANEL_LIGHT
    };
    let border = if entry.selected {
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
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        entry.label,
        egui::TextStyle::Body.resolve(ui.style()),
        if entry.selected {
            GOLD_ACCENT
        } else {
            TEXT_COLOR
        },
    );

    clicked
}

const CENTER_CARD_WIDTH: f32 = 340.0;
const CENTER_CARD_HEIGHT: f32 = 220.0;
const CENTER_CARD_CAMERA_DISTANCE: f32 = 900.0;
const CENTER_CARD_MAX_YAW_RAD: f32 = 0.26;
const CENTER_CARD_MAX_PITCH_RAD: f32 = 0.20;
const CENTER_CARD_SCREEN_RESPONSE_GAIN: f32 = 1.65;

#[derive(Clone, Copy)]
struct CardTilt {
    yaw_sin: f32,
    yaw_cos: f32,
    pitch_sin: f32,
    pitch_cos: f32,
    pointer_offset: egui::Vec2,
}

fn fullscreen_pointer_offset(screen_rect: egui::Rect, pointer_pos: egui::Pos2) -> egui::Vec2 {
    let center = screen_rect.center();
    let half_width = (screen_rect.width() * 0.5).max(1.0);
    let half_height = (screen_rect.height() * 0.5).max(1.0);
    let raw_offset = egui::vec2(
        ((pointer_pos.x - center.x) / half_width).clamp(-1.0, 1.0),
        ((pointer_pos.y - center.y) / half_height).clamp(-1.0, 1.0),
    );
    let response_scale = CENTER_CARD_SCREEN_RESPONSE_GAIN.tanh();

    egui::vec2(
        (raw_offset.x * CENTER_CARD_SCREEN_RESPONSE_GAIN).tanh() / response_scale,
        (raw_offset.y * CENTER_CARD_SCREEN_RESPONSE_GAIN).tanh() / response_scale,
    )
}

impl CardTilt {
    fn from_pointer(screen_rect: egui::Rect, pointer_pos: egui::Pos2) -> Self {
        let pointer_offset = fullscreen_pointer_offset(screen_rect, pointer_pos);
        let yaw = pointer_offset.x * CENTER_CARD_MAX_YAW_RAD;
        let pitch = -pointer_offset.y * CENTER_CARD_MAX_PITCH_RAD;
        let (yaw_sin, yaw_cos) = yaw.sin_cos();
        let (pitch_sin, pitch_cos) = pitch.sin_cos();

        Self {
            yaw_sin,
            yaw_cos,
            pitch_sin,
            pitch_cos,
            pointer_offset,
        }
    }

    fn project(self, center: egui::Pos2, local: egui::Vec2, z: f32) -> egui::Pos2 {
        let rotated_x = local.x * self.yaw_cos + z * self.yaw_sin;
        let yawed_z = -local.x * self.yaw_sin + z * self.yaw_cos;
        let rotated_y = local.y * self.pitch_cos - yawed_z * self.pitch_sin;
        let rotated_z = local.y * self.pitch_sin + yawed_z * self.pitch_cos;
        let scale = CENTER_CARD_CAMERA_DISTANCE / (CENTER_CARD_CAMERA_DISTANCE - rotated_z);

        egui::pos2(center.x + rotated_x * scale, center.y + rotated_y * scale)
    }
}

pub(crate) fn draw_center_card(ctx: &egui::Context) {
    ctx.request_repaint();

    egui::Area::new("center_card".into())
        .order(egui::Order::Tooltip)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .interactable(false)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(430.0, 310.0), egui::Sense::hover());
            let card_center = rect.center();
            let screen_rect = ctx.content_rect();
            let pointer_pos = ctx
                .input(|input| input.pointer.hover_pos())
                .unwrap_or(screen_rect.center());
            let tilt = CardTilt::from_pointer(screen_rect, pointer_pos);
            let painter = ui.painter_at(rect);

            draw_tilted_center_card(ctx, &painter, card_center, tilt);
        });
}

fn draw_tilted_center_card(
    ctx: &egui::Context,
    painter: &egui::Painter,
    card_center: egui::Pos2,
    tilt: CardTilt,
) {
    let half_size = egui::vec2(CENTER_CARD_WIDTH * 0.5, CENTER_CARD_HEIGHT * 0.5);
    let card_points = projected_rect_points(
        card_center,
        tilt,
        egui::Rect::from_min_max(
            egui::pos2(-half_size.x, -half_size.y),
            egui::pos2(half_size.x, half_size.y),
        ),
        0.0,
    );
    let shadow_offset = egui::vec2(
        10.0 - tilt.pointer_offset.x * 10.0,
        14.0 - tilt.pointer_offset.y * 8.0,
    );
    let shadow_points = card_points
        .iter()
        .map(|point| *point + shadow_offset)
        .collect();
    painter.add(egui::Shape::convex_polygon(
        shadow_points,
        Color32::from_rgba_unmultiplied(35, 26, 42, 150),
        egui::Stroke::NONE,
    ));

    painter.add(egui::Shape::convex_polygon(
        card_points.clone(),
        Color32::from_rgb(38, 45, 41),
        panel_stroke(ctx, 3.0, GOLD_ACCENT),
    ));

    draw_projected_rect(
        painter,
        card_center,
        tilt,
        egui::Rect::from_min_max(
            egui::pos2(-half_size.x + 13.0, -half_size.y + 13.0),
            egui::pos2(half_size.x - 13.0, half_size.y - 13.0),
        ),
        0.0,
        Color32::TRANSPARENT,
        panel_stroke(ctx, 1.0, SAGE_ACCENT),
    );
    draw_projected_rect(
        painter,
        card_center,
        tilt,
        egui::Rect::from_min_max(
            egui::pos2(-half_size.x, -half_size.y),
            egui::pos2(half_size.x, -half_size.y + 8.0),
        ),
        0.0,
        FLOWER_ACCENT,
        egui::Stroke::NONE,
    );

    draw_center_card_sprout(painter, card_center, tilt);
    draw_center_card_text(ctx, painter, card_center, tilt);
    draw_center_card_glare(painter, card_center, tilt);
}

fn draw_center_card_text(
    ctx: &egui::Context,
    painter: &egui::Painter,
    card_center: egui::Pos2,
    tilt: CardTilt,
) {
    draw_projected_text(
        ctx,
        painter,
        card_center,
        tilt,
        egui::vec2(0.0, -38.0),
        "Seed Card",
        egui::FontId::monospace(22.0),
        GOLD_ACCENT,
    );
    draw_projected_text(
        ctx,
        painter,
        card_center,
        tilt,
        egui::vec2(0.0, -11.0),
        "a pocket note from the garden",
        egui::FontId::monospace(12.0),
        FLOWER_ACCENT,
    );

    for (line_index, line) in [
        "Re: Flora remembers the soil,",
        "the rain, and the next small thing",
        "waiting to grow.",
    ]
    .iter()
    .enumerate()
    {
        draw_projected_text(
            ctx,
            painter,
            card_center,
            tilt,
            egui::vec2(0.0, 27.0 + line_index as f32 * 17.0),
            line,
            egui::FontId::proportional(14.0),
            TEXT_COLOR,
        );
    }

    draw_projected_text(
        ctx,
        painter,
        card_center,
        tilt,
        egui::vec2(0.0, 86.0),
        "press C to tuck it away",
        egui::FontId::monospace(11.0),
        SAGE_ACCENT,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_projected_text(
    ctx: &egui::Context,
    painter: &egui::Painter,
    card_center: egui::Pos2,
    tilt: CardTilt,
    local_center: egui::Vec2,
    text: &str,
    font_id: egui::FontId,
    color: Color32,
) {
    let text_mesh = ctx.fonts_mut(|fonts| {
        let galley = fonts.layout_no_wrap(text.to_owned(), font_id, color);
        let [font_width, font_height] = fonts.font_image_size();
        let uv_normalizer = egui::vec2(
            1.0 / font_width.max(1) as f32,
            1.0 / font_height.max(1) as f32,
        );
        let galley_origin = local_center - galley.rect.center().to_vec2();
        let mut mesh = egui::Mesh::with_texture(egui::TextureId::default());

        for row in &galley.rows {
            let index_offset = mesh.vertices.len() as u32;
            mesh.indices.extend(
                row.visuals
                    .mesh
                    .indices
                    .iter()
                    .map(|index| index + index_offset),
            );
            mesh.vertices
                .extend(row.visuals.mesh.vertices.iter().map(|vertex| {
                    let local = galley_origin + row.pos.to_vec2() + vertex.pos.to_vec2();
                    egui::epaint::Vertex {
                        pos: tilt.project(card_center, local, 0.0),
                        uv: (vertex.uv.to_vec2() * uv_normalizer).to_pos2(),
                        color: vertex.color,
                    }
                }));
        }

        mesh
    });

    painter.add(egui::Shape::mesh(text_mesh));
}

fn draw_center_card_sprout(painter: &egui::Painter, card_center: egui::Pos2, tilt: CardTilt) {
    draw_projected_rect(
        painter,
        card_center,
        tilt,
        egui::Rect::from_min_max(egui::pos2(-3.0, -94.0), egui::pos2(3.0, -66.0)),
        0.0,
        SAGE_ACCENT,
        egui::Stroke::NONE,
    );
    draw_projected_rect(
        painter,
        card_center,
        tilt,
        egui::Rect::from_min_max(egui::pos2(-29.0, -104.0), egui::pos2(-5.0, -90.0)),
        0.0,
        Color32::from_rgb(104, 158, 98),
        egui::Stroke::NONE,
    );
    draw_projected_rect(
        painter,
        card_center,
        tilt,
        egui::Rect::from_min_max(egui::pos2(5.0, -108.0), egui::pos2(29.0, -94.0)),
        0.0,
        Color32::from_rgb(129, 189, 122),
        egui::Stroke::NONE,
    );
    draw_projected_rect(
        painter,
        card_center,
        tilt,
        egui::Rect::from_min_max(egui::pos2(-25.0, -67.0), egui::pos2(25.0, -59.0)),
        0.0,
        Color32::from_rgb(112, 78, 50),
        egui::Stroke::NONE,
    );
    draw_projected_rect(
        painter,
        card_center,
        tilt,
        egui::Rect::from_min_max(egui::pos2(-34.0, -113.0), egui::pos2(34.0, -55.0)),
        0.0,
        Color32::TRANSPARENT,
        egui::Stroke::new(1.0, FLOWER_ACCENT),
    );
}

fn draw_center_card_glare(painter: &egui::Painter, card_center: egui::Pos2, tilt: CardTilt) {
    let distance = tilt.pointer_offset.length().clamp(0.0, 1.0);
    let alpha = (26.0 + distance * 34.0) as u8;
    let glare_x = -70.0 + tilt.pointer_offset.x * 115.0;
    let glare_points = [
        egui::vec2(glare_x - 19.0, -95.0),
        egui::vec2(glare_x + 13.0, -95.0),
        egui::vec2(glare_x + 58.0, 95.0),
        egui::vec2(glare_x + 26.0, 95.0),
    ]
    .into_iter()
    .map(|point| tilt.project(card_center, point, 0.0))
    .collect();

    painter.add(egui::Shape::convex_polygon(
        glare_points,
        Color32::from_rgba_unmultiplied(255, 242, 172, alpha),
        egui::Stroke::NONE,
    ));
}

fn projected_rect_points(
    card_center: egui::Pos2,
    tilt: CardTilt,
    rect: egui::Rect,
    z: f32,
) -> Vec<egui::Pos2> {
    [
        egui::vec2(rect.left(), rect.top()),
        egui::vec2(rect.right(), rect.top()),
        egui::vec2(rect.right(), rect.bottom()),
        egui::vec2(rect.left(), rect.bottom()),
    ]
    .into_iter()
    .map(|point| tilt.project(card_center, point, z))
    .collect()
}

fn draw_projected_rect(
    painter: &egui::Painter,
    card_center: egui::Pos2,
    tilt: CardTilt,
    rect: egui::Rect,
    z: f32,
    fill: Color32,
    stroke: egui::Stroke,
) {
    painter.add(egui::Shape::convex_polygon(
        projected_rect_points(card_center, tilt, rect, z),
        fill,
        stroke,
    ));
}

fn draw_tool_text(
    ui: &mut egui::Ui,
    slot: &ItemPanelSlot,
    selected: bool,
    interaction_enabled: bool,
) -> bool {
    let label = ui.painter().layout_no_wrap(
        slot.label.into(),
        egui::TextStyle::Button.resolve(ui.style()),
        TEXT_COLOR,
    );
    let key = ui.painter().layout_no_wrap(
        slot.number.clone(),
        egui::TextStyle::Small.resolve(ui.style()),
        TEXT_COLOR,
    );
    let enabled = interaction_enabled && slot.enabled;
    let size = egui::vec2(
        label.size().x + key.size().x + 6.0 + TOOL_TEXT_PADDING * 2.0,
        (label.size().y + 16.0).max(34.0),
    );
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            enabled,
            selected,
            slot.label,
        )
    });
    let response = if slot.index == MOWER_SLOT_INDEX && slot.label == "Mower" {
        response.on_hover_text("Mower (slot 12)\nEdit/cursor mode only (after camera transitions finish).\nLMB: spawn at the pointer; hold and drag to guide it.\nRelease: destroy. Every press spawns a fresh mower.\nFixed top speed and turn speed; the chassis follows the slope.\nCuts plants shorter without removing them.")
    } else if slot.index == SOIL_DEPOSITION_SLOT_INDEX && slot.label == "Soil" {
        response.on_hover_text("Soil (slot 6)\nClick LMB: one adjustable batch of individual voxels. Hold: continuous batches.\nMouse wheel: Soil spread (0 = precise), not volume or Edit radius.\nUses Dirt; Infinite Materials bypasses inventory without changing it.\nAdds new soil without fertilizing or replacing existing ground.")
    } else {
        response.on_hover_text(format!("{} (slot {})", slot.label, slot.number))
    };
    let hovered = enabled && (response.hovered() || response.has_focus());
    if enabled {
        response
            .clone()
            .on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    let color = if !slot.enabled {
        TEXT_COLOR.gamma_multiply(0.35)
    } else if selected {
        GOLD_ACCENT
    } else if hovered {
        TEXT_COLOR
    } else {
        TEXT_COLOR.gamma_multiply(0.8)
    };
    let label_pos = egui::pos2(
        rect.left() + TOOL_TEXT_PADDING,
        rect.center().y - label.size().y * 0.5,
    );
    let label_width = label.size().x;
    let underline_y = label_pos.y + label.size().y + 3.0;
    ui.painter()
        .galley_with_override_text_color(label_pos, label, color);
    ui.painter().galley_with_override_text_color(
        egui::pos2(
            label_pos.x + label_width + 6.0,
            rect.center().y - key.size().y * 0.5,
        ),
        key,
        color.gamma_multiply(0.55),
    );
    if selected || hovered {
        ui.painter().line_segment(
            [
                egui::pos2(label_pos.x, underline_y),
                egui::pos2(label_pos.x + label_width, underline_y),
            ],
            egui::Stroke::new(if selected { 2.0 } else { 1.0 }, color),
        );
    }
    enabled && response.clicked()
}

pub(crate) struct VoxelPaletteEntry {
    pub voxel: BackpackVoxel,
    pub label: &'static str,
    pub count: u32,
    pub color: Color32,
    pub selected: bool,
    pub unlimited: bool,
}

#[derive(Default)]
pub(crate) struct VoxelPaletteResponse {
    pub clicked_voxel: Option<BackpackVoxel>,
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
                stroke: panel_stroke(
                    ctx,
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
                            egui::RichText::new(if entries.iter().any(|e| e.unlimited) {
                                "Unlimited materials"
                            } else {
                                "Backpack"
                            })
                            .color(GOLD_ACCENT)
                            .size(12.0)
                            .strong(),
                        );
                        ui.label(
                            egui::RichText::new(if entries.iter().any(|e| e.unlimited) {
                                "choose Edit material · Soil always uses Dirt"
                            } else {
                                "status only · no material filter"
                            })
                            .color(TEXT_COLOR.linear_multiply(0.78))
                            .size(10.0),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("ALL")
                                .color(SAGE_ACCENT)
                                .monospace()
                                .strong(),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                for entry in entries {
                    if draw_voxel_palette_entry(ui, entry, interaction_enabled) {
                        response.clicked_voxel = Some(entry.voxel);
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
        if entry.unlimited {
            "∞".to_owned()
        } else {
            format!("{:>6}", entry.count)
        },
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

/// Keep panel geometry and widget borders unchanged when hiding the outer stroke.
pub(crate) fn apply_panel_outline_preference(style: &mut egui::Style, enabled: bool) {
    style.visuals.window_stroke = egui::Stroke::new(
        1.5,
        if enabled {
            SAGE_ACCENT
        } else {
            Color32::TRANSPARENT
        },
    );
}

/// Custom panel frames follow the same policy as egui windows and popups.
pub(crate) fn panel_stroke(ctx: &egui::Context, width: f32, color: Color32) -> egui::Stroke {
    egui::Stroke::new(
        width,
        if ctx.global_style().visuals.window_stroke.color == Color32::TRANSPARENT {
            Color32::TRANSPARENT
        } else {
            color
        },
    )
}

pub(crate) fn apply_gui_style(style: &mut egui::Style) {
    style.visuals.override_text_color = Some(TEXT_COLOR);
    style.visuals.hyperlink_color = GOLD_ACCENT;
    style.interaction.selectable_labels = false;
    style.interaction.multi_widget_text_select = false;

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
        // Button frame margins are integer-rounded by egui. A fractional stroke
        // changes the rounded padding sum on hover and shifts the entire row.
        1.0,
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

#[cfg(test)]
mod text_toolbar_tests;

#[cfg(test)]
mod hover_layout_tests {
    use super::*;

    #[test]
    fn grow_panel_can_select_every_model_flower_on_a_short_window() {
        use crate::flora::species::{self, FloraPaintSelection};
        let context = egui::Context::default();
        let mut style = (*context.global_style()).clone();
        apply_gui_style(&mut style);
        context.set_global_style(style);
        let mut entries = species::PLAYER_FLORA_PAINT_SELECTIONS
            .iter()
            .copied()
            .enumerate()
            .map(|(index, selection)| PlantPanelEntry {
                variety: PlantVariety::Flora(selection),
                label: species::flora_paint_selection_label(selection),
                selected: index == 0,
            })
            .collect::<Vec<_>>();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640., 480.));
        let draw = |events: Vec<egui::Event>, entries: &[PlantPanelEntry]| {
            let mut clicked = None;
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |_| {
                    clicked =
                        draw_plant_panel(&context, entries, PlantType::Flora, true).clicked_variety;
                },
            );
            (output, clicked)
        };
        for _ in 0..3 {
            draw(vec![], &entries);
        }
        let mut selected = 0;
        for (index, selection) in species::PLAYER_FLORA_PAINT_SELECTIONS.iter().enumerate() {
            let FloraPaintSelection::Species(species) = selection else {
                continue;
            };
            if *species < species::MODEL_FLOWER_FIRST_SPECIES {
                continue;
            }
            let label = entries[index].label;
            let mut point = None;
            for _ in 0..24 {
                let (frame, _) = draw(vec![], &entries);
                point = frame.shapes.iter().find_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape {
                        let center = text.pos + text.galley.size() * 0.5;
                        if text.galley.job.text == label
                            && shape.clip_rect.contains(center)
                            && screen.contains(center)
                        {
                            return Some(center);
                        }
                    }
                    None
                });
                if point.is_some() {
                    break;
                }
                draw(
                    vec![
                        egui::Event::PointerMoved(egui::pos2(540., 240.)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0., -100.),
                            phase: egui::TouchPhase::Move,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    &entries,
                );
            }
            let point = point.unwrap_or_else(|| panic!("{label} is unreachable in the Grow panel"));
            draw(
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                &entries,
            );
            let (_, clicked) = draw(
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                &entries,
            );
            assert_eq!(
                clicked,
                Some(PlantVariety::Flora(*selection)),
                "Grow click must resolve {label} to its real planting selection"
            );
            for entry in &mut entries {
                entry.selected = entry.variety == PlantVariety::Flora(*selection);
            }
            selected += 1;
        }
        assert_eq!(selected, crate::flora::models::MODEL_COUNT);
    }

    #[test]
    fn grow_panel_height_is_half_the_viewport_for_both_catalogs() {
        let context = egui::Context::default();
        let mut style = (*context.global_style()).clone();
        apply_gui_style(&mut style);
        context.set_global_style(style);
        for height in [480.0, 720.0, 1080.0] {
            let mut heights = Vec::new();
            for plant_type in [PlantType::Flora, PlantType::Tree, PlantType::Flora] {
                let entries = plant_type
                    .varieties()
                    .into_iter()
                    .map(|variety| PlantPanelEntry {
                        variety,
                        label: variety.label(),
                        selected: false,
                    })
                    .collect::<Vec<_>>();
                for _ in 0..4 {
                    let _ = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(640.0, height),
                            )),
                            ..Default::default()
                        },
                        |_| {
                            draw_plant_panel(&context, &entries, plant_type, true);
                        },
                    );
                }
                let rect = context
                    .memory(|memory| memory.area_rect(egui::Id::new("flora_paint_panel")))
                    .unwrap();
                assert!(
                    (rect.height() - height * 0.5).abs() <= 1.0,
                    "{plant_type:?}: panel {} vs viewport {height}",
                    rect.height()
                );
                heights.push(rect.height());
            }
            assert_eq!(heights[0], heights[1]);
            assert_eq!(heights[1], heights[2]);
        }
    }

    #[test]
    fn grow_panel_type_menu_and_tree_variety_resolve_typed_selections() {
        let context = egui::Context::default();
        let mut style = (*context.global_style()).clone();
        apply_gui_style(&mut style);
        context.set_global_style(style);
        let draw = |plant_type: PlantType, events: Vec<egui::Event>, enabled: bool| {
            let entries = plant_type
                .varieties()
                .into_iter()
                .map(|variety| PlantPanelEntry {
                    variety,
                    label: variety.label(),
                    selected: false,
                })
                .collect::<Vec<_>>();
            let mut response = PlantPanelResponse::default();
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640., 480.),
                    )),
                    events,
                    ..Default::default()
                },
                |_| {
                    response = draw_plant_panel(&context, &entries, plant_type, enabled);
                },
            );
            (output, response)
        };
        let click = |plant_type, label: &str, enabled| {
            let mut frame = draw(plant_type, vec![], enabled).0;
            for _ in 0..3 {
                frame = draw(plant_type, vec![], enabled).0;
            }
            let point = frame
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == label => {
                        let point = text.pos + text.galley.size() * 0.5;
                        shape.clip_rect.contains(point).then_some(point)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing menu entry {label}"));
            draw(
                plant_type,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                enabled,
            );
            draw(
                plant_type,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                enabled,
            )
            .1
        };
        assert_eq!(
            click(PlantType::Flora, "Tree", true).clicked_type,
            Some(PlantType::Tree)
        );
        let tree = PlantType::Tree.varieties()[0];
        assert_eq!(
            click(PlantType::Tree, tree.label(), true).clicked_variety,
            Some(tree)
        );
        assert_eq!(
            click(PlantType::Tree, "Flora", true).clicked_type,
            Some(PlantType::Flora)
        );
        assert_eq!(click(PlantType::Flora, "Tree", false).clicked_type, None);
        assert_eq!(
            click(PlantType::Tree, tree.label(), false).clicked_variety,
            None
        );
    }

    #[test]
    fn button_hover_keeps_its_rect_and_following_controls_fixed() {
        let context = egui::Context::default();
        let mut style = (*context.global_style()).clone();
        apply_gui_style(&mut style);
        context.set_global_style(style);
        let mut selected = false;
        let mut rects = [egui::Rect::NOTHING; 3];
        let mut draw = |events: Vec<egui::Event>| {
            let _ = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 300.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        rects[0] = ui
                            .selectable_value(&mut selected, true, "Saved inflow")
                            .rect;
                        rects[1] = ui.button("Another control").rect;
                    });
                    rects[2] = ui.label("Following settings").rect;
                },
            );
            rects
        };
        draw(Vec::new());
        let idle = draw(Vec::new());
        for _ in 0..3 {
            let hover = draw(vec![egui::Event::PointerMoved(idle[0].center())]);
            assert_eq!(hover, idle, "hover changed button/row geometry");
        }
        for _ in 0..3 {
            let hover = draw(vec![egui::Event::PointerMoved(idle[1].center())]);
            assert_eq!(hover, idle, "button hover moved following controls");
        }
    }
}
