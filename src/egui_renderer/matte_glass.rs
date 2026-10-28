//! Explicit scene-backed UI surfaces. No fill-color detection and no scene feedback.
use egui::{
    epaint::Vertex, Color32, Frame, Id, InnerResponse, LayerId, Order, Rect, Shape, TextureId, Ui,
};

/// Reserved by this renderer; ordinary egui textures remain on the normal paint path.
pub const TEXTURE_ID: TextureId = TextureId::User(0x7266_6d61_7474_6501);

/// One logical pixel, like the approved browser recipe (scales with UI DPI).
const PIXEL: f32 = 1.0;

fn surface(rect: Rect, opacity: f32) -> Shape {
    if !rect.is_positive() || opacity <= 0.0 {
        return Shape::Noop;
    }
    // Five rectangular strips make crisp two-step corners without a rounded bevel.
    // Non-overlapping geometry matters during window fades.
    let step = PIXEL.min(rect.width() / 4.0).min(rect.height() / 4.0);
    let strips = [
        Rect::from_min_max(
            rect.min + egui::vec2(2.0 * step, 0.0),
            egui::pos2(rect.right() - 2.0 * step, rect.bottom()),
        ),
        Rect::from_min_max(
            rect.min + egui::vec2(step, step),
            egui::pos2(rect.left() + 2.0 * step, rect.bottom() - step),
        ),
        Rect::from_min_max(
            egui::pos2(rect.right() - 2.0 * step, rect.top() + step),
            rect.max - egui::vec2(step, step),
        ),
        Rect::from_min_max(
            rect.min + egui::vec2(0.0, 2.0 * step),
            egui::pos2(rect.left() + step, rect.bottom() - 2.0 * step),
        ),
        Rect::from_min_max(
            egui::pos2(rect.right() - step, rect.top() + 2.0 * step),
            rect.max - egui::vec2(0.0, 2.0 * step),
        ),
    ];
    let mut mesh = egui::Mesh::with_texture(TEXTURE_ID);
    let color = Color32::WHITE.linear_multiply(opacity);
    for strip in strips {
        let base = mesh.vertices.len() as u32;
        for pos in [
            strip.left_top(),
            strip.right_top(),
            strip.right_bottom(),
            strip.left_bottom(),
        ] {
            mesh.vertices.push(Vertex {
                pos,
                uv: (pos - rect.min).to_pos2(),
                color,
            });
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Shape::mesh(mesh)
}

fn clear_body(mut frame: Frame) -> Frame {
    frame.fill = Color32::TRANSPARENT;
    frame.shadow = Default::default();
    frame.corner_radius = egui::CornerRadius::ZERO;
    frame
}

pub trait MatteFrameExt {
    fn show_matte<R>(self, ui: &mut Ui, contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R>;
}

impl MatteFrameExt for Frame {
    fn show_matte<R>(self, ui: &mut Ui, contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let background = ui.painter().add(Shape::Noop);
        let mut prepared = clear_body(self).begin(ui);
        let inner = contents(&mut prepared.content_ui);
        let rect = prepared.frame.widget_rect(prepared.content_ui.min_rect());
        // Painter::set applies this UI's fade/disabled opacity to the mesh as usual.
        if ui.is_rect_visible(rect) {
            ui.painter().set(background, surface(rect, 1.0));
        }
        let response = prepared.end(ui);
        InnerResponse::new(inner, response)
    }
}

pub trait MatteWindowExt {
    fn show_matte<R>(
        self,
        ctx: &egui::Context,
        id: Id,
        frame: Frame,
        contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<Option<R>>>;
}

impl MatteWindowExt for egui::Window<'_> {
    fn show_matte<R>(
        self,
        ctx: &egui::Context,
        id: Id,
        frame: Frame,
        contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<Option<R>>> {
        // Window does not expose Frame::begin. Adapt only this explicitly opted-in
        // window's frame, in its own layer/range. The frame's real paint slot and
        // faded alpha preserve title ordering and collapsed-window fade behavior.
        let layer = LayerId::new(Order::Middle, id);
        let start = ctx.layer_painter(layer).add(Shape::Noop);
        let mut native_frame = clear_body(frame);
        native_frame.fill = Color32::WHITE;
        let result = self
            .id(id)
            .order(Order::Middle)
            .frame(native_frame)
            .show(ctx, contents);
        if let Some(shown) = &result {
            let bounds = shown.response.rect - frame.outer_margin;
            ctx.graphics_mut(|graphics| {
                let list = graphics.entry(layer);
                let index =
                    list.all_entries()
                        .enumerate()
                        .skip(start.0 + 1)
                        .find_map(|(index, entry)| match &entry.shape {
                            Shape::Rect(rect) if rect.rect == bounds => {
                                Some(egui::layers::ShapeIdx(index))
                            }
                            _ => None,
                        });
                if let Some(index) = index {
                    list.mutate_shape(index, |entry| {
                        if let Shape::Rect(rect) = &entry.shape {
                            let material = surface(rect.rect, rect.fill.a() as f32 / 255.0);
                            let mut rim = rect.clone();
                            rim.fill = Color32::TRANSPARENT;
                            entry.shape = Shape::Vec(vec![material, Shape::Rect(rim)]);
                        }
                    });
                }
            });
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_material(shape: &Shape) -> bool {
        match shape {
            Shape::Mesh(mesh) => mesh.texture_id == TEXTURE_ID,
            Shape::Vec(shapes) => shapes.iter().any(has_material),
            _ => false,
        }
    }

    #[test]
    fn material_is_explicit_and_has_pixel_steps_without_overlapping_triangles() {
        let rect = Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 60.0));
        let Shape::Mesh(mesh) = surface(rect, 1.0) else {
            panic!("missing material mesh")
        };
        assert_eq!(mesh.texture_id, TEXTURE_ID);
        let area: f32 = mesh
            .indices
            .chunks_exact(3)
            .map(|triangle| {
                let a = mesh.vertices[triangle[0] as usize].pos;
                let b = mesh.vertices[triangle[1] as usize].pos;
                let c = mesh.vertices[triangle[2] as usize].pos;
                ((b - a).rot90().dot(c - a)).abs() * 0.5
            })
            .sum();
        assert_eq!(area, rect.area() - 12.0);
        for vertex in &mesh.vertices {
            assert!(rect.contains(vertex.pos));
            assert_eq!(vertex.uv, (vertex.pos - rect.min).to_pos2());
        }
    }

    #[test]
    fn material_frame_precedes_text_and_keeps_content_layout() {
        let ctx = egui::Context::default();
        let mut layouts = Vec::new();
        for glass in [false, true] {
            let output = ctx.run_ui(Default::default(), |ui| {
                let frame = Frame::new().inner_margin(12);
                let contents = |ui: &mut Ui| ui.label("Sharp text").rect;
                let shown = if glass {
                    frame.show_matte(ui, contents)
                } else {
                    frame.show(ui, contents)
                };
                layouts.push((shown.response.rect, shown.inner));
            });
            if glass {
                let material = output
                    .shapes
                    .iter()
                    .position(|s| has_material(&s.shape))
                    .unwrap();
                let text = output
                    .shapes
                    .iter()
                    .position(|s| matches!(&s.shape, Shape::Text(_)))
                    .unwrap();
                assert!(material < text);
            }
        }
        assert_eq!(layouts[0], layouts[1]);
    }

    #[test]
    fn collapsed_windows_keep_the_material_without_running_contents() {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.75);
        let mut final_output = None;
        for frame in 0..4 {
            final_output = Some(ctx.run_ui(
                egui::RawInput {
                    time: Some(frame as f64),
                    ..Default::default()
                },
                |_| {
                    let shown = egui::Window::new("Collapsed glass")
                        .default_open(false)
                        .fixed_pos(egui::pos2(30.0 + frame as f32 * 10.0, 50.0))
                        .show_matte(
                            &ctx,
                            Id::new("collapsed_glass"),
                            Frame::window(&ctx.global_style()),
                            |_| panic!("collapsed contents must not run"),
                        )
                        .unwrap();
                    assert!(shown.inner.is_none());
                },
            ));
        }
        assert!(final_output
            .unwrap()
            .shapes
            .iter()
            .any(|shape| has_material(&shape.shape)));
    }

    #[test]
    fn window_surface_precedes_title_and_disappears_when_closed() {
        let ctx = egui::Context::default();
        let id = Id::new("glass_window_test");
        for (frame, open) in [true, true, true, false, false].into_iter().enumerate() {
            let mut open = open;
            let output = ctx.run_ui(
                egui::RawInput {
                    time: Some(ctx.input(|i| i.time) + 1.0),
                    ..Default::default()
                },
                |_| {
                    egui::Window::new("Glass title").open(&mut open).show_matte(
                        &ctx,
                        id,
                        Frame::window(&ctx.global_style()),
                        |ui| {
                            ui.label("Contents");
                        },
                    );
                },
            );
            let material = output.shapes.iter().position(|s| has_material(&s.shape));
            if open {
                if let Some(title) = output.shapes.iter().position(
                    |s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == "Glass title"),
                ) {
                    assert!(material.unwrap() < title);
                }
            } else if frame == 4 {
                assert!(material.is_none());
            }
        }
    }
}
