use super::*;
use crate::egui_renderer::matte_glass::{MatteFrameExt, TEXTURE_ID};

#[test]
fn adjacent_equal_material_surfaces_keep_distinct_backdrop_boundaries() {
    let ctx = egui::Context::default();
    let output = ctx.run_ui(Default::default(), |ui| {
        // Identical geometry, texture and clipping with no content between them:
        // egui normally merges these meshes unless a paint boundary survives.
        for _ in 0..3 {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(
                egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(100.0, 100.0)),
            ));
            egui::Frame::new().show_matte(&mut child, |ui| {
                ui.set_min_size(egui::vec2(100.0, 100.0));
            });
        }
    });
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    let passes = draw_passes(&primitives);
    let glass: Vec<_> = passes.iter().filter(|pass| pass.blur_before).collect();
    assert_eq!(glass.len(), 3);
    for pass in glass {
        let materials = primitives[pass.primitives.clone()]
            .iter()
            .filter(
                |p| matches!(&p.primitive, Primitive::Mesh(mesh) if mesh.texture_id == TEXTURE_ID),
            )
            .count();
        assert_eq!(materials, 1);
    }
    assert_offsets_and_coverage(&primitives, &passes);
}

#[test]
fn nested_glass_samples_after_lower_contents_and_before_its_own_contents() {
    let ctx = egui::Context::default();
    let output = ctx.run_ui(Default::default(), |ui| {
        ui.label("Scene overlay preceding any glass");
        egui::Frame::new().show_matte(ui, |ui| {
            ui.label("Lower panel content to be blurred");
            egui::Frame::new().show_matte(ui, |ui| {
                ui.label("Middle panel content to be blurred by top");
                egui::Frame::new().show_matte(ui, |ui| {
                    ui.label("Sharp upper content");
                });
            });
        });
    });
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    let passes = draw_passes(&primitives);
    assert_eq!(passes.iter().filter(|pass| pass.blur_before).count(), 3);
    assert!(!passes[0].blur_before);
    for pass in passes.iter().filter(|pass| pass.blur_before) {
        let meshes: Vec<_> = primitives[pass.primitives.clone()]
            .iter()
            .filter_map(|p| {
                if let Primitive::Mesh(mesh) = &p.primitive {
                    Some(mesh)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(meshes[0].texture_id, TEXTURE_ID);
        assert!(
            meshes
                .iter()
                .skip(1)
                .any(|mesh| mesh.texture_id != TEXTURE_ID),
            "contents must be drawn after their own glass and before the next blur"
        );
    }
    assert_offsets_and_coverage(&primitives, &passes);
}

#[test]
fn empty_or_ordinary_ui_needs_no_backdrop_refresh() {
    assert!(draw_passes(&[]).is_empty());
    let ctx = egui::Context::default();
    let output = ctx.run_ui(Default::default(), |ui| {
        ui.label("No glass");
    });
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    let passes = draw_passes(&primitives);
    assert_eq!(passes.len(), 1);
    assert!(!passes[0].blur_before);
    assert_offsets_and_coverage(&primitives, &passes);
}

#[test]
fn backdrop_footprint_uses_clipped_glass_not_its_contents() {
    let ctx = egui::Context::default();
    let output = ctx.run_ui(Default::default(), |ui| {
        ui.set_clip_rect(egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(80.0, 90.0),
        ));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
            egui::pos2(20.0, 20.0),
            egui::vec2(100.0, 100.0),
        )));
        egui::Frame::new().show_matte(&mut child, |ui| {
            ui.set_min_size(egui::vec2(100.0, 100.0));
            ui.label("Panel contents");
        });
        // This belongs to the same draw pass but does not sample its backdrop.
        ui.painter()
            .with_clip_rect(egui::Rect::EVERYTHING)
            .rect_filled(
                egui::Rect::from_min_size(egui::pos2(300.0, 300.0), egui::vec2(1000.0, 1000.0)),
                0.0,
                egui::Color32::RED,
            );
    });
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    let passes = draw_passes(&primitives);
    let pass = passes.iter().find(|p| p.blur_before).unwrap();
    let bounds = pass.backdrop_bounds(&primitives);
    assert!(bounds.is_positive());
    assert!(bounds.min.x >= 19.0 && bounds.min.y >= 19.0);
    assert_eq!(bounds.max, egui::pos2(80.0, 90.0));
    assert_offsets_and_coverage(&primitives, &passes);
}

#[test]
fn dragging_a_real_window_moves_its_unclipped_grid_anchor() {
    use crate::egui_renderer::matte_glass::MatteWindowExt;
    let ctx = egui::Context::default();
    let mut time = 0.0;
    let mut draw = |events| {
        time += 0.1;
        let output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                events,
                ..Default::default()
            },
            |_| {
                egui::Window::new("Drag glass")
                    .default_pos(egui::pos2(50.0, 50.0))
                    .resizable(false)
                    .show_matte(
                        &ctx,
                        egui::Id::new("drag_glass"),
                        egui::Frame::window(&ctx.global_style()),
                        |ui| {
                            ui.set_min_size(egui::vec2(250.0, 120.0));
                            ui.label("Sharp contents");
                        },
                    );
            },
        );
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        draw_passes(&primitives)
            .into_iter()
            .find(|p| p.blur_before)
            .map(|p| p.surface_origin)
    };
    for _ in 0..4 {
        draw(vec![]);
    }
    let before = draw(vec![]).unwrap();
    let pointer = before + egui::vec2(100.0, 12.0);
    draw(vec![
        egui::Event::PointerMoved(pointer),
        egui::Event::PointerButton {
            pos: pointer,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    let delta = egui::vec2(7.0, 5.0);
    draw(vec![egui::Event::PointerMoved(pointer + delta)]);
    let after = draw(vec![egui::Event::PointerButton {
        pos: pointer + delta,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert_eq!(after.unwrap() - before, delta);
}

fn assert_offsets_and_coverage(primitives: &[ClippedPrimitive], passes: &[DrawPass]) {
    let (mut indices, mut vertices) = (0, 0);
    for pass in passes {
        assert_eq!(pass.index_offset, indices);
        assert_eq!(pass.vertex_offset, vertices);
        for p in &primitives[pass.primitives.clone()] {
            assert!(!matte_glass::is_backdrop_boundary(&p.primitive));
            if let Primitive::Mesh(mesh) = &p.primitive {
                indices += mesh.indices.len() as u32;
                vertices += mesh.vertices.len() as i32;
            }
        }
    }
    let mesh_count = |range: &[ClippedPrimitive]| {
        range
            .iter()
            .filter(|p| matches!(p.primitive, Primitive::Mesh(_)))
            .count()
    };
    assert_eq!(
        passes
            .iter()
            .map(|pass| mesh_count(&primitives[pass.primitives.clone()]))
            .sum::<usize>(),
        mesh_count(primitives),
        "all content must be rendered exactly once"
    );
}
