//! Loading-only presentation. Flower pixels are authored once in model space;
//! their colored quads rotate together, never re-sampled onto a screen-space grid.
use egui::{epaint::Mesh, Color32, Context, FontId, LayerId, Pos2, Rect, Vec2};
use std::time::Instant;

const GREENS: [Color32; 2] = [Color32::from_rgb(41, 79, 64), Color32::from_rgb(48, 87, 70)];
const STEPS: usize = 24;
const STEP_SECONDS: f64 = 0.8 / 0.75;

pub(crate) struct Splash {
    flowers: [Mesh; 2],
    started: Option<Instant>,
}

impl Default for Splash {
    fn default() -> Self {
        Self {
            flowers: std::array::from_fn(flower_mesh),
            started: None,
        }
    }
}

impl Splash {
    pub(super) fn show(&mut self, ctx: &Context, progress: f32) {
        let seconds = self
            .started
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f64();
        let rect = ctx.viewport_rect();
        let painter = ctx.layer_painter(LayerId::background());
        painter.rect_filled(rect, 0.0, GREENS[0]);
        let cell = if rect.width() < 650.0 { 112.0 } else { 144.0 };
        let columns = (rect.width() / cell).ceil() as usize + 2;
        let rows = (rect.height() / cell).ceil() as usize + 2;
        let origin = rect.center() - Vec2::new(columns as f32, rows as f32) * (cell / 2.0);
        let flowers: [Mesh; 2] = std::array::from_fn(|kind| {
            let mut mesh = self.flowers[kind].clone();
            for vertex in &mut mesh.vertices {
                vertex.pos *= 2.0;
            }
            mesh.rotate(
                egui::emath::Rot2::from_angle(
                    rotation_step(kind, seconds) as f32 * std::f32::consts::TAU / STEPS as f32,
                ),
                Pos2::ZERO,
            );
            mesh
        });
        let mut field = Mesh::default();
        for row in 0..rows {
            for column in 0..columns {
                let kind = (row + column) % 2;
                let tile = Rect::from_min_size(
                    origin + Vec2::new(column as f32, row as f32) * cell,
                    Vec2::splat(cell),
                );
                if !tile.intersects(rect) {
                    continue;
                }
                painter.rect_filled(tile, 0.0, GREENS[kind]);
                append_at(&mut field, &flowers[kind], tile.center());
            }
        }
        painter.add(egui::Shape::mesh(field));

        // Reuse the game's Pixelify Sans font. No subtitle or loading-status text.
        let title = painter.layout_no_wrap(
            "re: flora".to_owned(),
            FontId::proportional((rect.width() * 0.05).clamp(40.0, 72.0)),
            GREENS[0],
        );
        let title_center = rect.min + Vec2::new(rect.width() * 0.5, rect.height() * 0.365);
        let padding = if rect.width() < 650.0 {
            Vec2::new(24.0, 16.0)
        } else {
            Vec2::new(30.0, 19.0)
        };
        painter.rect_filled(
            Rect::from_center_size(title_center, title.size() + padding * 2.0),
            0.0,
            Color32::WHITE,
        );
        painter.galley(title_center - title.size() * 0.5, title, GREENS[0]);

        let bar_width = (rect.width() * 0.19).clamp(160.0, 260.0).min(rect.width());
        let bar = Rect::from_min_size(
            rect.min + Vec2::new((rect.width() - bar_width) * 0.5, rect.height() * 0.86),
            Vec2::new(bar_width, 3.0),
        );
        painter.rect_filled(bar, 0.0, Color32::from_rgb(83, 117, 91));
        painter.rect_filled(
            Rect::from_min_size(
                bar.min,
                Vec2::new(bar.width() * progress.clamp(0.0, 1.0), bar.height()),
            ),
            0.0,
            Color32::from_rgb(238, 229, 173),
        );
    }
}

fn rotation_step(kind: usize, seconds: f64) -> usize {
    let step = (seconds / STEP_SECONDS + kind as f64 * 0.5).floor() as usize % STEPS;
    if kind == 0 {
        step
    } else {
        (STEPS - step) % STEPS
    }
}

fn append_at(target: &mut Mesh, source: &Mesh, center: Pos2) {
    let offset = target.vertices.len() as u32;
    target
        .indices
        .extend(source.indices.iter().map(|index| offset + index));
    target
        .vertices
        .extend(source.vertices.iter().map(|vertex| egui::epaint::Vertex {
            pos: center + vertex.pos.to_vec2(),
            ..*vertex
        }));
}

/// Build colored unit-square texels instead of filtered textures. This preserves
/// the local pixel lattice at every rotation without changing the GUI sampler.
fn flower_mesh(kind: usize) -> Mesh {
    let (petals, breadth, edge, petal, light, center, pollen) = if kind == 0 {
        (5, 3.5, 0xcfaa48, 0xf1ce62, 0xffe59a, 0x9d773a, 0xd4a44d)
    } else {
        (7, 2.3, 0xc8d5bb, 0xeeefdb, 0xfffdf1, 0xd1aa45, 0xf4d66e)
    };
    let rgb = |hex: u32| Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
    let mut pixels = [None; 32 * 32];
    for i in 0..petals {
        let angle = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / petals as f64;
        let (sin, cos) = angle.sin_cos();
        let length = 4.6 + 0.3 * (i as f64 * 2.0).sin();
        for y in 3..29 {
            for x in 3..29 {
                let dx = x as f64 - 15.5;
                let dy = y as f64 - 15.5;
                let u = dx * cos + dy * sin;
                let v = -dx * sin + dy * cos;
                let distance = ((u - 6.4) / length).powi(2) + (v / breadth).powi(2);
                if distance <= 1.0 {
                    pixels[y * 32 + x] = Some(rgb(if distance > 0.77 {
                        edge
                    } else if v < -0.3 {
                        light
                    } else {
                        petal
                    }));
                }
            }
        }
    }
    for y in 12..20 {
        for x in 12..20 {
            let dx = x as f64 - 15.5;
            let dy = y as f64 - 15.5;
            if dx * dx + dy * dy <= 10.0 {
                pixels[y * 32 + x] = Some(rgb(if dx + dy < -1.0 { pollen } else { center }));
            }
        }
    }
    let mut mesh = Mesh::default();
    for (index, color) in pixels.into_iter().enumerate() {
        if let Some(color) = color {
            mesh.add_colored_rect(
                Rect::from_min_size(
                    egui::pos2((index % 32) as f32 - 16.0, (index / 32) as f32 - 16.0),
                    Vec2::splat(1.0),
                ),
                color,
            );
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_holds_steps_and_loops_in_opposite_directions() {
        for kind in 0..2 {
            assert_eq!(rotation_step(kind, 0.1), rotation_step(kind, 0.3));
            assert_eq!(
                rotation_step(kind, 0.1),
                rotation_step(kind, 0.1 + STEP_SECONDS * STEPS as f64)
            );
        }
        assert_eq!(rotation_step(0, 1.2), 1);
        assert_eq!(rotation_step(1, 1.2), 23);
    }

    #[test]
    fn local_pixels_remain_rigid_colored_quads_after_rotation() {
        for kind in 0..2 {
            let original = flower_mesh(kind);
            assert!(!original.is_empty());
            assert_eq!(original.vertices.len() % 4, 0);
            let mut rotated = original.clone();
            rotated.rotate(
                egui::emath::Rot2::from_angle(std::f32::consts::PI / 12.0),
                Pos2::ZERO,
            );
            assert_eq!(original.indices, rotated.indices);
            for (before, after) in original.vertices.iter().zip(&rotated.vertices) {
                assert_eq!(before.color, after.color);
                assert!(
                    (before.pos.to_vec2().length() - after.pos.to_vec2().length()).abs() < 0.00001
                );
            }
            for (before, after) in original
                .vertices
                .chunks_exact(4)
                .zip(rotated.vertices.chunks_exact(4))
            {
                for i in 0..4 {
                    assert!(
                        (before[i].pos.distance(before[(i + 1) % 4].pos)
                            - after[i].pos.distance(after[(i + 1) % 4].pos))
                        .abs()
                            < 0.00001
                    );
                }
            }
        }
    }

    #[test]
    fn batching_keeps_flower_indices_and_local_colors_valid() {
        let mut field = Mesh::default();
        for kind in 0..2 {
            let flower = flower_mesh(kind);
            append_at(&mut field, &flower, egui::pos2(100.0 * kind as f32, 50.0));
        }
        assert!(field.is_valid());
        assert_eq!(
            field.vertices.len(),
            flower_mesh(0).vertices.len() + flower_mesh(1).vertices.len()
        );
        assert_ne!(
            flower_mesh(0).vertices[0].color,
            flower_mesh(1).vertices[0].color
        );
    }
}
