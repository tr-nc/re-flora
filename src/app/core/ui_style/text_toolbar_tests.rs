use super::*;

fn slots() -> Vec<ItemPanelSlot> {
    [
        "Grow",
        "Edit",
        "Trim",
        "Water",
        "Till",
        "Soil",
        "Inspector",
        "Spray",
        "Stone",
        "Pave",
        "Wall",
        "Mower",
        "Wind",
        "Lantern",
        "Thin pole",
        "Thick pole",
        "Rope",
        "Plow",
        "Shovel",
        "Harvest",
        "Vacuum",
        "Bee house",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, label)| ItemPanelSlot {
        index: TOOLBAR_ORDER[index],
        label,
        number: toolbar_number(TOOLBAR_ORDER[index]),
        enabled: true,
    })
    .collect()
}

fn frame(
    ctx: &egui::Context,
    slots: &[ItemPanelSlot],
    width: f32,
    selected: Option<usize>,
    enabled: bool,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, ItemPanelResponse) {
    let mut response = ItemPanelResponse::default();
    let output = ctx.run_ui(
        egui::RawInput {
            time: Some(ctx.input(|input| input.time) + 0.05),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 600.),
            )),
            events,
            ..Default::default()
        },
        |_| response = draw_item_panel(ctx, slots, selected, enabled),
    );
    (output, response)
}

fn label_rect(output: &egui::FullOutput, label: &str) -> egui::Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.galley.rect.translate(text.pos.to_vec2()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing tool label {label}"))
}

fn underline_count(output: &egui::FullOutput) -> usize {
    output
        .shapes
        .iter()
        .filter(|shape| matches!(shape.shape, egui::Shape::LineSegment { .. }))
        .count()
}

#[test]
fn panel_outlines_toggle_only_frames_and_preserve_layout_and_controls() {
    let ctx = egui::Context::default();
    let mut style = (*ctx.global_style()).clone();
    apply_gui_style(&mut style);
    let widgets = style.visuals.widgets.clone();
    let selection = style.visuals.selection;
    ctx.set_global_style(style);
    let draw = |enabled| {
        ctx.global_style_mut(|style| apply_panel_outline_preference(style, enabled));
        assert_eq!(ctx.global_style().visuals.widgets, widgets);
        assert_eq!(ctx.global_style().visuals.selection, selection);
        let mut output = None;
        for _ in 0..4 {
            output = Some(ctx.run_ui(
                egui::RawInput {
                    time: Some(ctx.input(|i| i.time) + 1.0),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600., 1000.),
                    )),
                    ..Default::default()
                },
                |_| {
                    draw_plant_panel(&ctx, &[], PlantType::Flora, true);
                    draw_voxel_palette(
                        &ctx,
                        &[VoxelPaletteEntry {
                            voxel: BackpackVoxel::Dirt,
                            label: "Dirt",
                            count: 3,
                            color: GOLD_ACCENT,
                            selected: true,
                            unlimited: false,
                        }],
                        true,
                    );
                    egui::Window::new("ordinary egui window").show(&ctx, |ui| {
                        ui.allocate_space(egui::vec2(200., 160.));
                    });
                },
            ));
        }
        fn collect(shape: egui::Shape, rects: &mut Vec<egui::epaint::RectShape>) {
            match shape {
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, rects);
                    }
                }
                egui::Shape::Rect(rect)
                    if rect.rect.height() > 60.
                        && (rect.fill == PANEL_BG
                            || (rect.fill == Color32::TRANSPARENT && rect.stroke.width > 0.0)) =>
                {
                    rects.push(rect)
                }
                _ => {}
            }
        }
        let mut rectangles = Vec::new();
        for shape in output.unwrap().shapes {
            collect(shape.shape, &mut rectangles);
        }
        rectangles
    };
    let on = draw(true);
    let off = draw(false);
    let restored = draw(true);
    assert_eq!(on.len(), 3);
    assert_eq!(on.len(), off.len());
    for ((on, off), restored) in on.iter().zip(&off).zip(&restored) {
        assert_eq!(on.rect, off.rect);
        assert_eq!(on.rect, restored.rect);
        assert_eq!(on.fill, off.fill);
        assert_eq!(on.stroke.width, off.stroke.width);
        assert!(!on.stroke.is_empty());
        assert!(off.stroke.is_empty());
        assert_eq!(on.stroke, restored.stroke);
    }
}

#[test]
fn matte_theme_keeps_selected_and_hovered_labels_readable() {
    fn luminance(color: Color32) -> f32 {
        let linear = |channel: u8| {
            let value = channel as f32 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
    }
    let mut style = egui::Style::default();
    apply_gui_style(&mut style);
    for background in [
        panel_entry_fill(true, false),
        panel_entry_fill(false, true),
        style.visuals.widgets.active.bg_fill,
    ] {
        for foreground in [TEXT_COLOR, GOLD_ACCENT] {
            let contrast = (luminance(background) + 0.05) / (luminance(foreground) + 0.05);
            assert!(
                contrast >= 4.5,
                "selected/hovered control contrast: {contrast}"
            );
        }
    }
}

#[test]
fn toolbar_numbers_and_numeric_shortcuts_follow_visible_order() {
    use winit::keyboard::KeyCode;
    let slots = slots();
    let keys = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
        KeyCode::Digit0,
    ];
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &slots, 1440., None, true, vec![]);
    }
    let (output, _) = frame(&ctx, &slots, 1440., None, true, vec![]);
    for (position, slot) in slots.iter().enumerate() {
        let number = (position + 1).to_string();
        assert_eq!(slot.number, number);
        assert!(label_rect(&output, &number).is_positive());
        if let Some(&key) = keys.get(position) {
            assert_eq!(toolbar_keyboard_slot(key), Some(slot.index));
        }
    }
    for key in [
        KeyCode::KeyB,
        KeyCode::KeyX,
        KeyCode::KeyN,
        KeyCode::KeyV,
        KeyCode::KeyJ,
        KeyCode::KeyL,
    ] {
        assert_eq!(toolbar_keyboard_slot(key), None);
    }
}

#[test]
fn text_toolbar_wraps_without_icons_boxes_or_lost_tools() {
    for width in [320., 640., 1440.] {
        let ctx = egui::Context::default();
        ctx.global_style_mut(apply_gui_style);
        let slots = slots();
        for _ in 0..3 {
            frame(&ctx, &slots, width, None, true, vec![]);
        }
        let (output, _) = frame(&ctx, &slots, width, None, true, vec![]);
        let panel = ctx.memory(|memory| memory.area_rect("item_panel")).unwrap();
        assert!(panel.left() >= 0. && panel.right() <= width, "{panel:?}");
        assert_eq!(underline_count(&output), 0);
        let mut labels = Vec::new();
        for slot in &slots {
            let label = label_rect(&output, slot.label);
            assert!(
                panel.contains_rect(label),
                "{}: {label:?} outside {panel:?}",
                slot.label
            );
            for previous in &labels {
                assert!(!label.intersects(*previous));
            }
            labels.push(label);
        }
        // One explicitly tagged material surface; no per-tool/keycap boxes or icons.
        fn check_surface(shape: &egui::Shape, panel: egui::Rect) -> usize {
            match shape {
                egui::Shape::Vec(shapes) => {
                    shapes.iter().map(|shape| check_surface(shape, panel)).sum()
                }
                egui::Shape::Rect(rect) => {
                    assert!(rect.stroke.is_empty());
                    assert_eq!(rect.fill, Color32::TRANSPARENT);
                    0
                }
                egui::Shape::Mesh(mesh) => {
                    assert_eq!(
                        mesh.texture_id,
                        crate::egui_renderer::matte_glass::TEXTURE_ID
                    );
                    assert_eq!(mesh.calc_bounds(), panel);
                    1
                }
                _ => 0,
            }
        }
        let materials: usize = output
            .shapes
            .iter()
            .map(|shape| check_surface(&shape.shape, panel))
            .sum();
        assert_eq!(materials, 1);
    }
}

#[test]
fn hover_and_selection_underline_text_without_moving_it() {
    let ctx = egui::Context::default();
    let slots = slots();
    for _ in 0..3 {
        frame(&ctx, &slots, 1440., None, true, vec![]);
    }
    let (idle, _) = frame(&ctx, &slots, 1440., None, true, vec![]);
    let label = label_rect(&idle, "Grow");
    let (hovered, _) = frame(
        &ctx,
        &slots,
        1440.,
        None,
        true,
        vec![egui::Event::PointerMoved(label.center())],
    );
    assert_eq!(underline_count(&hovered), 1);
    assert_eq!(label_rect(&hovered, "Grow"), label);
    let (selected, _) = frame(
        &ctx,
        &slots,
        1440.,
        Some(0),
        true,
        vec![egui::Event::PointerGone],
    );
    assert_eq!(underline_count(&selected), 1);
    assert_eq!(label_rect(&selected, "Grow"), label);
    assert!(selected.shapes.iter().any(|s| matches!(s.shape, egui::Shape::LineSegment { stroke, .. } if stroke.color == GOLD_ACCENT)));
}

#[test]
fn text_toolbar_clicks_keep_slot_ids_and_respect_disabled_input() {
    for interaction in [false, true] {
        for available in [false, true] {
            let ctx = egui::Context::default();
            let mut slots = slots();
            slots[1].index = 42;
            slots[1].enabled = available;
            for _ in 0..3 {
                frame(&ctx, &slots, 640., None, interaction, vec![]);
            }
            let (output, _) = frame(&ctx, &slots, 640., None, interaction, vec![]);
            let pos = label_rect(&output, "Edit").center();
            for pressed in [true, false] {
                let (_, response) = frame(
                    &ctx,
                    &slots,
                    640.,
                    None,
                    interaction,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
                assert_eq!(
                    response.clicked_slot,
                    (!pressed && interaction && available).then_some(42)
                );
            }
        }
    }
}
