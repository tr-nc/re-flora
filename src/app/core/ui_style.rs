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

pub(crate) const ITEM_PANEL_SHOVEL_ICON_PATH: &str = "assets/texture/tool_icons/dig.png";
pub(crate) const ITEM_PANEL_SHOVEL_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/10_Wooden_Shovel.PNG";
pub(crate) const ITEM_PANEL_STAFF_ICON_PATH: &str = "assets/texture/tool_icons/grow.png";
pub(crate) const ITEM_PANEL_STAFF_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/4_Wooden_Staff.PNG";
pub(crate) const ITEM_PANEL_SMOOTH_ICON_PATH: &str = "assets/texture/tool_icons/smooth.png";
pub(crate) const ITEM_PANEL_SMOOTH_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/26_Copper_Hoe.PNG";
pub(crate) const ITEM_PANEL_HOE_ICON_PATH: &str = "assets/texture/tool_icons/trim.png";
pub(crate) const ITEM_PANEL_HOE_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/28_Copper_Sickle.PNG";
pub(crate) const ITEM_PANEL_WATER_ICON_PATH: &str = "assets/texture/tool_icons/water.png";
pub(crate) const ITEM_PANEL_WATER_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/30_Copper_Watering_Can.PNG";
pub(crate) const ITEM_PANEL_SPRINKLER_ICON_PATH: &str = "assets/texture/tool_icons/sprinkler.png";
pub(crate) const ITEM_PANEL_SOIL_INSPECTOR_ICON_PATH: &str =
    "assets/texture/tool_icons/inspector.png";
pub(crate) const ITEM_PANEL_SOIL_INSPECTOR_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/24_Copper_Pickaxe.PNG";
pub(crate) const ITEM_PANEL_FERTILIZER_ICON_PATH: &str = "assets/texture/tool_icons/fertilizer.png";
pub(crate) const ITEM_PANEL_FERTILIZER_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/46_Gold_Bar.PNG";
pub(crate) const ITEM_PANEL_TILLER_ICON_PATH: &str = "assets/texture/tool_icons/till.png";
pub(crate) const ITEM_PANEL_TILLER_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/11_Wooden_Hoe.PNG";
pub(crate) const ITEM_PANEL_TREE_ICON_PATH: &str = "assets/texture/tool_icons/tree.png";
pub(crate) const ITEM_PANEL_TREE_ICON_FALLBACK_PATH: &str =
    "assets/texture/Pixel_Farming_Tools_IconSet_16px/Individuals/8_Wooden_Axe.PNG";
pub(crate) const ITEM_PANEL_SLOT_COUNT: usize = 11;
pub(crate) const HAND_SLOT_INDEX: usize = 0;
pub(crate) const STAFF_SLOT_INDEX: usize = 1;
pub(crate) const SHOVEL_SLOT_INDEX: usize = 2;
pub(crate) const SMOOTH_SLOT_INDEX: usize = 3;
pub(crate) const HOE_SLOT_INDEX: usize = 4;
pub(crate) const WATERING_SLOT_INDEX: usize = 5;
pub(crate) const SOIL_INSPECTOR_SLOT_INDEX: usize = 6;
pub(crate) const FERTILIZER_SLOT_INDEX: usize = 7;
pub(crate) const TILLER_SLOT_INDEX: usize = 8;
pub(crate) const PLACE_TOOL_SLOT_INDEX: usize = 9;
pub(crate) const TREE_SLOT_INDEX: usize = PLACE_TOOL_SLOT_INDEX;
pub(crate) const SPRINKLER_SLOT_INDEX: usize = 10;
pub(crate) const PLACEABLE_PANEL_SLOT_COUNT: usize = 2;
pub(crate) const TREE_PLACEABLE_SLOT_INDEX: usize = 0;
pub(crate) const SPRINKLER_PLACEABLE_SLOT_INDEX: usize = 1;

pub(crate) const SHOVEL_TOOL_ACCENT: Color32 = Color32::from_rgb(178, 124, 80);
pub(crate) const SMOOTH_TOOL_ACCENT: Color32 = Color32::from_rgb(190, 156, 106);
pub(crate) const STAFF_TOOL_ACCENT: Color32 = Color32::from_rgb(129, 189, 122);
pub(crate) const HOE_TOOL_ACCENT: Color32 = Color32::from_rgb(219, 128, 152);
pub(crate) const TREE_TOOL_ACCENT: Color32 = Color32::from_rgb(82, 154, 90);
pub(crate) const WATER_TOOL_ACCENT: Color32 = Color32::from_rgb(96, 171, 218);
pub(crate) const SOIL_INSPECTOR_TOOL_ACCENT: Color32 = Color32::from_rgb(166, 132, 88);
pub(crate) const FERTILIZER_TOOL_ACCENT: Color32 = Color32::from_rgb(220, 96, 54);
pub(crate) const TILLER_TOOL_ACCENT: Color32 = Color32::from_rgb(151, 113, 76);

pub(crate) struct ToolPanelSlot<'a> {
    pub index: usize,
    pub label: &'static str,
    pub key_hint: &'static str,
    pub category: Option<&'static str>,
    pub icon: Option<&'a TextureHandle>,
    pub accent: Color32,
    pub enabled: bool,
}

pub(crate) type ItemPanelSlot<'a> = ToolPanelSlot<'a>;

#[derive(Default)]
pub(crate) struct ToolPanelResponse {
    pub clicked_slot: Option<usize>,
}

pub(crate) type ItemPanelResponse = ToolPanelResponse;

pub(crate) struct FloraPaintPanelEntry {
    pub index: usize,
    pub label: &'static str,
    pub selected: bool,
}

#[derive(Default)]
pub(crate) struct FloraPaintPanelResponse {
    pub clicked_selection_index: Option<usize>,
}

#[derive(Clone, Copy)]
enum ToolPanelOrientation {
    Horizontal,
}

#[derive(Clone, Copy)]
struct ToolPanelHeader {
    active: bool,
    active_text: &'static str,
    inactive_text: &'static str,
}

impl ToolPanelHeader {
    fn text(self) -> &'static str {
        if self.active {
            self.active_text
        } else {
            self.inactive_text
        }
    }

    fn color(self) -> Color32 {
        if self.active {
            GOLD_ACCENT
        } else {
            SAGE_ACCENT
        }
    }
}

#[derive(Clone, Copy)]
struct ToolPanelTheme {
    slot_size: egui::Vec2,
    icon_size: egui::Vec2,
    slot_gap: f32,
    tray_padding: egui::Vec2,
    bottom_padding_extra: f32,
    keycap_size: egui::Vec2,
    shadow_offset: [i8; 2],
}

impl ToolPanelTheme {
    const fn new(
        slot_size: egui::Vec2,
        icon_size: egui::Vec2,
        slot_gap: f32,
        tray_padding: egui::Vec2,
        bottom_padding_extra: f32,
        keycap_size: egui::Vec2,
        shadow_offset: [i8; 2],
    ) -> Self {
        Self {
            slot_size,
            icon_size,
            slot_gap,
            tray_padding,
            bottom_padding_extra,
            keycap_size,
            shadow_offset,
        }
    }

    fn tool_bar() -> Self {
        Self::new(
            egui::Vec2::new(60.0, 60.0),
            egui::Vec2::new(28.0, 28.0),
            5.0,
            egui::Vec2::new(12.0, 10.0),
            3.0,
            egui::Vec2::new(16.0, 14.0),
            [4, 4],
        )
    }
}

struct ToolPanelSpec {
    area_id: &'static str,
    anchor: egui::Align2,
    offset: egui::Vec2,
    orientation: ToolPanelOrientation,
    theme: ToolPanelTheme,
    fill: Color32,
    stroke: egui::Stroke,
    header: Option<ToolPanelHeader>,
}

pub(crate) fn draw_item_panel(
    ctx: &egui::Context,
    slots: &[ItemPanelSlot<'_>],
    selected_slot_idx: Option<usize>,
    interaction_enabled: bool,
) -> ItemPanelResponse {
    draw_tool_panel(
        ctx,
        slots,
        selected_slot_idx,
        interaction_enabled,
        ToolPanelSpec {
            area_id: "item_panel",
            anchor: egui::Align2::CENTER_BOTTOM,
            offset: egui::Vec2::new(0.0, -16.0),
            orientation: ToolPanelOrientation::Horizontal,
            theme: ToolPanelTheme::tool_bar(),
            fill: PANEL_DARK,
            stroke: egui::Stroke::new(2.0, FLOWER_ACCENT),
            header: None,
        },
    )
}

const FLORA_PAINT_PANEL_WIDTH: f32 = 190.0;
const FLORA_PAINT_BUTTON_HEIGHT: f32 = 34.0;

pub(crate) fn draw_flora_paint_panel(
    ctx: &egui::Context,
    entries: &[FloraPaintPanelEntry],
    interaction_enabled: bool,
) -> FloraPaintPanelResponse {
    let mut panel_response = FloraPaintPanelResponse::default();
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
                stroke: egui::Stroke::new(2.0, selected_stroke),
                ..Default::default()
            };

            panel_frame.show(ui, |ui| {
                ui.set_width(FLORA_PAINT_PANEL_WIDTH);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Plant Brush")
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

                for entry in entries {
                    if draw_flora_paint_panel_entry(ui, entry, interaction_enabled) {
                        panel_response.clicked_selection_index = Some(entry.index);
                    }
                    ui.add_space(4.0);
                }
            });
        });

    panel_response
}

fn draw_flora_paint_panel_entry(
    ui: &mut egui::Ui,
    entry: &FloraPaintPanelEntry,
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
        egui::Stroke::new(3.0, GOLD_ACCENT),
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
        egui::Stroke::new(1.0, SAGE_ACCENT),
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
        "Verdarium remembers the soil,",
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

fn draw_tool_panel(
    ctx: &egui::Context,
    slots: &[ToolPanelSlot<'_>],
    selected_slot_idx: Option<usize>,
    interaction_enabled: bool,
    spec: ToolPanelSpec,
) -> ToolPanelResponse {
    let mut panel_response = ToolPanelResponse::default();
    let theme = spec.theme;

    egui::Area::new(spec.area_id.into())
        .order(egui::Order::Foreground)
        .anchor(spec.anchor, spec.offset)
        .show(ctx, |ui| {
            let panel_frame = egui::containers::Frame {
                fill: spec.fill,
                inner_margin: egui::Margin {
                    left: theme.tray_padding.x as i8,
                    right: theme.tray_padding.x as i8,
                    top: theme.tray_padding.y as i8,
                    bottom: (theme.tray_padding.y + theme.bottom_padding_extra) as i8,
                },
                corner_radius: egui::CornerRadius::same(0),
                shadow: egui::epaint::Shadow {
                    offset: theme.shadow_offset,
                    blur: 0,
                    spread: 0,
                    color: SHADOW_COLOR,
                },
                stroke: spec.stroke,
                ..Default::default()
            };

            panel_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    draw_tool_panel_contents(
                        ui,
                        slots,
                        selected_slot_idx,
                        interaction_enabled,
                        theme,
                        spec.header,
                        spec.orientation,
                        &mut panel_response,
                    );
                });
            });
        });

    panel_response
}

#[allow(clippy::too_many_arguments)]
fn draw_tool_panel_contents(
    ui: &mut egui::Ui,
    slots: &[ToolPanelSlot<'_>],
    selected_slot_idx: Option<usize>,
    interaction_enabled: bool,
    theme: ToolPanelTheme,
    header: Option<ToolPanelHeader>,
    orientation: ToolPanelOrientation,
    panel_response: &mut ToolPanelResponse,
) {
    if let Some(header) = header {
        ui.label(
            egui::RichText::new(header.text())
                .color(header.color())
                .monospace()
                .size(11.0),
        );
        match orientation {
            ToolPanelOrientation::Horizontal => ui.add_space(3.0),
        }
    }

    match orientation {
        ToolPanelOrientation::Horizontal => ui.spacing_mut().item_spacing.x = theme.slot_gap,
    }

    let mut previous_category = None;
    for slot in slots {
        if slot.category != previous_category {
            if previous_category.is_some() {
                match orientation {
                    ToolPanelOrientation::Horizontal => ui.add_space(5.0),
                }
            }
            previous_category = slot.category;
        }

        let clicked = draw_tool_panel_slot(
            ui,
            slot,
            selected_slot_idx == Some(slot.index),
            interaction_enabled && slot.enabled,
            theme,
        );
        if clicked {
            panel_response.clicked_slot = Some(slot.index);
        }
    }
}

fn tool_panel_category_fill(category: Option<&'static str>) -> Color32 {
    match category {
        Some("FREE") => Color32::from_rgb(43, 52, 49),
        Some("TOOLS") => Color32::from_rgb(54, 49, 42),
        Some("CARE") => Color32::from_rgb(42, 55, 52),
        Some("SCAN") => Color32::from_rgb(53, 50, 62),
        Some("ITEMS") => Color32::from_rgb(50, 45, 40),
        _ => PANEL_LIGHT,
    }
}

fn draw_tool_panel_slot(
    ui: &mut egui::Ui,
    slot: &ToolPanelSlot<'_>,
    selected: bool,
    interaction_enabled: bool,
    theme: ToolPanelTheme,
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

    let category_fill = tool_panel_category_fill(slot.category);
    let fill = if selected {
        category_fill.linear_multiply(1.22)
    } else if hovered {
        category_fill.linear_multiply(1.12)
    } else {
        category_fill
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
        let palm = egui::Rect::from_center_size(
            egui::pos2(icon_rect.center().x, icon_rect.center().y + 5.0),
            egui::vec2(15.0, 13.0),
        );
        painter.rect_stroke(
            palm,
            egui::CornerRadius::same(3),
            egui::Stroke::new(1.3, accent),
            egui::StrokeKind::Inside,
        );
        for x_offset in [-6.0, -2.0, 2.0, 6.0] {
            let x = icon_rect.center().x + x_offset;
            painter.line_segment(
                [
                    egui::pos2(x, icon_rect.center().y + 1.0),
                    egui::pos2(x, icon_rect.top() + 4.0),
                ],
                egui::Stroke::new(1.3, accent),
            );
        }
        painter.line_segment(
            [
                egui::pos2(palm.left() + 1.0, palm.center().y),
                egui::pos2(icon_rect.left() + 3.0, icon_rect.center().y + 1.0),
            ],
            egui::Stroke::new(1.3, accent),
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
                            egui::RichText::new("Backpack")
                                .color(GOLD_ACCENT)
                                .size(12.0)
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new("status only · no material filter")
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
