use super::*;
use crate::app::core::ui_style::{GOLD_ACCENT, PANEL_DARK};

pub(in crate::app::core) fn shop_open_after_key(
    key: winit::keyboard::PhysicalKey,
    pressed: bool,
    repeat: bool,
    open: bool,
    world_keys: bool,
) -> Option<bool> {
    use winit::keyboard::KeyCode;
    if !pressed || repeat {
        return None;
    }
    if open && key == KeyCode::Escape {
        Some(false)
    } else if key == KeyCode::KeyB && (open || world_keys) {
        Some(!open)
    } else {
        None
    }
}

pub(in crate::app::core) enum ShopAction {
    Buy(String),
    SellApples(u32),
}

pub(in crate::app::core) fn draw_garden_shop(
    ctx: &egui::Context,
    economy: &GardenEconomy,
    open: &mut bool,
    feedback: Option<&str>,
    planting_summary: Option<&str>,
    infinite: bool,
    interaction_enabled: bool,
) -> Option<ShopAction> {
    let mut action = None;
    let wallet = format!(
        "{} coins · {} apple{}",
        economy.coins,
        economy.apples,
        if economy.apples == 1 { "" } else { "s" }
    );
    egui::Area::new("garden_wallet".into())
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(16., 16.))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(PANEL_DARK)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&wallet).color(GOLD_ACCENT));
                        if ui
                            .add_enabled(interaction_enabled, egui::Button::new("Shop · B"))
                            .clicked()
                        {
                            *open = !*open;
                        }
                    });
                    if let Some(summary) = planting_summary {
                        ui.label(summary);
                    }
                    if !*open {
                        if let Some(text) = feedback {
                            ui.label(text);
                        }
                    }
                });
        });
    egui::Window::new("Garden shop")
        .open(open)
        .default_pos(egui::pos2(30., 105.))
        .default_width(440.)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading(&wallet);
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Apples sell for {} coins each",
                    GardenEconomy::apple_price()
                ));
                if ui
                    .add_enabled(economy.apples > 0, egui::Button::new("Sell all apples"))
                    .clicked()
                {
                    action = Some(ShopAction::SellApples(economy.apples));
                }
            });
            if infinite {
                ui.label("Debug: unlimited planting. Purchases still spend real coins.");
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(475.)
                .show(ui, |ui| {
                    egui::Grid::new("garden_shop_products")
                        .striped(true)
                        .spacing(egui::vec2(18., 8.))
                        .show(ui, |ui| {
                            for p in catalog() {
                                ui.label(&p.label);
                                ui.label(format!("{} owned", economy.count(&p.id)));
                                if ui
                                    .add_enabled(
                                        economy.coins >= p.price,
                                        egui::Button::new(format!("Buy · {} coins", p.price)),
                                    )
                                    .clicked()
                                {
                                    action = Some(ShopAction::Buy(p.id.clone()));
                                }
                                ui.end_row();
                            }
                        });
                });
            ui.separator();
            ui.label("Use Grow to plant. Seedlings are used before seeds.");
            ui.label("Harvest / E picks one ripe apple. Hold Vacuum to collect nearby apples and plants.");
            ui.label("Collected flowers return as seedlings. B / Esc closes this shop.");
            if let Some(text) = feedback {
                ui.label(text);
            }
        });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shop_keys_close_the_modal_even_when_world_shortcuts_are_blocked() {
        use winit::keyboard::KeyCode;
        assert_eq!(
            shop_open_after_key(KeyCode::KeyB.into(), true, false, false, true),
            Some(true)
        );
        assert_eq!(
            shop_open_after_key(KeyCode::KeyB.into(), true, false, true, false),
            Some(false)
        );
        assert_eq!(
            shop_open_after_key(KeyCode::Escape.into(), true, false, true, false),
            Some(false)
        );
        assert_eq!(
            shop_open_after_key(KeyCode::Escape.into(), true, false, false, true),
            None
        );
        assert_eq!(
            shop_open_after_key(KeyCode::KeyB.into(), true, true, false, true),
            None
        );
        assert_eq!(
            shop_open_after_key(KeyCode::KeyB.into(), true, false, false, false),
            None
        );
    }
    fn frame(
        ctx: &egui::Context,
        economy: &GardenEconomy,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Option<ShopAction>) {
        let mut open = true;
        let mut action = None;
        let output = ctx.run_ui(
            egui::RawInput {
                time: Some(ctx.input(|i| i.time) + 0.05),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 1000.),
                )),
                events,
                ..Default::default()
            },
            |_| {
                action = draw_garden_shop(ctx, economy, &mut open, None, None, false, true);
            },
        );
        (output, action)
    }
    #[test]
    fn real_shop_buttons_emit_only_affordable_purchase_intent() {
        for funded in [true, false] {
            let ctx = egui::Context::default();
            let mut economy = GardenEconomy::default();
            if !funded {
                economy.coins = 0;
            }
            for _ in 0..3 {
                frame(&ctx, &economy, vec![]);
            }
            let (output, _) = frame(&ctx, &economy, vec![]);
            let label = format!("Buy · {} coins", PRICES.grass_packet_price);
            let point = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.galley.rect.translate(text.pos.to_vec2()).center())
                    }
                    _ => None,
                })
                .expect("visible grass purchase button");
            frame(
                &ctx,
                &economy,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            );
            let (_, action) = frame(
                &ctx,
                &economy,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
            if funded {
                let Some(ShopAction::Buy(id)) = action else {
                    panic!("affordable button did not emit purchase");
                };
                assert_eq!(id, "seeds/grass_mix");
                economy.buy(&id).unwrap();
                assert_eq!(economy.count(&id), PRICES.grass_packet_size);
            } else {
                assert!(action.is_none());
            }
        }
    }
}
