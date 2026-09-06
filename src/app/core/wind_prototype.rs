//! PROTOTYPE adapter: pointer input and overlays only. Finite wind events and
//! evolution belong to wind_field; deleting this demo must not rewrite that model.
use super::{player_tools::PlayerTool, App};
use crate::wind_field::{Gust, GustSettings, WindField, MAX_GUSTS};
use egui::{Color32, Pos2, Stroke};
use glam::{Mat4, Vec2, Vec3};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

struct Drag {
    origin: Vec3,
    start_screen: Vec2,
    endpoint: Vec3,
}

fn gust_display_center(gust: &Gust, time: f32) -> Vec3 {
    gust.center(time)
}

impl Drag {
    fn preview(&self, settings: GustSettings, multiplier: f32, radial: bool, time: f32) -> Gust {
        let delta = self.endpoint - self.origin;
        let planar = Vec2::new(delta.x, delta.z);
        let speed = if radial {
            settings.speed
        } else {
            planar.length() * 256.
        } * multiplier;
        Gust {
            origin: self.origin,
            direction: planar.normalize_or_zero(),
            settings: GustSettings { speed, ..settings },
            radial,
            start: time,
        }
    }

    fn release(&self, field: &mut WindField, multiplier: f32, radial: bool) -> bool {
        let preview = self.preview(field.manual_gust, multiplier, radial, field.time());
        field.release_with_settings(preview.origin, preview.direction, radial, preview.settings)
    }
}

fn draw_gust_controls(ui: &mut egui::Ui, settings: &mut GustSettings, radial: bool) {
    ui.add(egui::Slider::new(&mut settings.strength, 0. ..=8.).text("Gust strength"));
    if !radial {
        ui.add(egui::Slider::new(&mut settings.width, 8. ..=512.).text("Width (voxels)"));
    }
    ui.add(
        egui::Slider::new(&mut settings.depth, 4. ..=256.).text(if radial {
            "Ring thickness"
        } else {
            "Depth (voxels)"
        }),
    );
    ui.add(egui::Slider::new(&mut settings.softness, 0.05..=1.).text("Edge softness"));
    ui.add(egui::Slider::new(&mut settings.duration, 0.5..=8.).text("Gust lifetime (s)"));
    if radial {
        ui.add(egui::Slider::new(&mut settings.speed, 10. ..=180.).text("Ring base speed"));
    }
}

pub(super) struct WindPrototype {
    pub field: WindField,
    radial: bool,
    speed_multiplier: f32,
    drag: Option<Drag>,
    status: String,
    scripted: bool,
    script_started: bool,
    script_completed: bool,
}

impl WindPrototype {
    pub fn from_environment() -> Option<Self> {
        std::env::var_os("RE_FLORA_WIND_PROTOTYPE").map(|_| Self {
            field: WindField::default(),
            radial: false,
            speed_multiplier: 1.,
            drag: None,
            status: "Drag on terrain, release to send a gust.".into(),
            scripted: std::env::var_os("RE_FLORA_WIND_PROTOTYPE_SMOKE").is_some(),
            script_started: false,
            script_completed: false,
        })
    }

    pub fn cancel(&mut self) {
        if self.drag.take().is_some() {
            self.status = "Gust preview cancelled.".into();
        }
    }

    pub fn advance(&mut self, time: f32) {
        self.field.advance(time);
        if self.scripted && !self.script_started {
            self.script_started = true;
            self.field.release(Vec3::new(0.7, 0.5, 1.), Vec2::X, false);
            self.field.release(Vec3::new(1., 0.5, 1.), Vec2::ZERO, true);
        }
        if self.scripted && !self.script_completed && self.field.time() > 4. {
            assert!(self.field.gusts.is_empty(), "expired manual gusts leaked");
            self.script_completed = true;
            log::info!("[WIND_PROTOTYPE] smoke=passed directional_and_radial_expired=true");
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, matrix: Mat4, extent: Vec2, wind_selected: bool) {
        if !wind_selected {
            self.cancel();
        }
        egui::Window::new("Background wind — PROTOTYPE")
            .default_pos(Pos2::new(18., 70.))
            .default_width(290.)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Temporary controls — never saved to GUI config");
                ui.horizontal_wrapped(|ui| {
                    for (mode, name) in [(0, "A Original"), (1, "B Turning"), (2, "C Local detail")]
                    {
                        ui.selectable_value(&mut self.field.mode, mode, name);
                    }
                });
                ui.small("Background only. The Wind item works in every mode.");
                if self.field.mode == 0 {
                    ui.label("Original uses your saved wind sources.");
                }
                ui.add_enabled_ui(self.field.mode > 0, |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.field.heading_degrees, 0. ..=360.)
                            .text("Main direction"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.field.strength, 0. ..=5.)
                            .text("Mean wind strength"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.field.wander_degrees, 0. ..=90.)
                            .text("Direction wander"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.field.wander_period, 4. ..=60.)
                            .text("Wander period (s)"),
                    );
                    let direction = self.field.direction();
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(260., 45.), egui::Sense::hover());
                    let center = rect.center();
                    ui.painter()
                        .circle_stroke(center, 18., Stroke::new(1., Color32::GRAY));
                    ui.painter().arrow(
                        center,
                        egui::vec2(direction.x, direction.y) * 22.,
                        Stroke::new(2., Color32::LIGHT_GREEN),
                    );
                    ui.label("Compass: +X right, +Z down (world plane)");
                });
                ui.separator();
                ui.collapsing("Local detail / transport", |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.field.propagation_speed, 0. ..=150.)
                            .text("Pattern travel speed"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.field.detail_strength, 0. ..=2.)
                            .text("Local disturbance"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.field.detail_scale, 10. ..=180.)
                            .text("Pattern size (voxels)"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.field.evolution_rate, 0. ..=2.)
                            .text("Pattern evolution"),
                    );
                });
                ui.label(format!(
                    "Wind time {:.1}s | Active gusts {}/{}",
                    self.field.time(),
                    self.field.gusts.len(),
                    MAX_GUSTS
                ));
                ui.small("Select Wind in the bottom toolbar (9) to release local wind.");
                ui.small(
                    "Wind input only; plant inertia continues. Audio / free particles unchanged.",
                );
            });
        if wind_selected {
            egui::Window::new("Wind item")
                .anchor(egui::Align2::RIGHT_BOTTOM, [-18., -112.])
                .resizable(false)
                .default_width(290.)
                .show(ctx, |ui| {
                    ui.label("Manual local wind — no automatic gusts");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.radial, false, "Directional drag");
                        ui.selectable_value(&mut self.radial, true, "Radial click");
                    });
                    draw_gust_controls(ui, &mut self.field.manual_gust, self.radial);
                    ui.add(
                        egui::Slider::new(&mut self.speed_multiplier, 0.1..=4.)
                            .text("Speed multiplier"),
                    );
                    ui.small("Drag distance x multiplier = travel speed. Arrow = 1 second.");
                    if let Some(drag) = &self.drag {
                        let preview = drag.preview(
                            self.field.manual_gust,
                            self.speed_multiplier,
                            self.radial,
                            self.field.time(),
                        );
                        ui.label(format!(
                            "Travel speed: {:.0} voxels/s",
                            preview.settings.speed
                        ));
                    }
                    ui.label(&self.status);
                    ui.small("Left drag and release: wind. Right drag: camera.");
                    ui.small("Esc cancels aiming. Choose another item to stop aiming.");
                });
        }
        let project = |world: Vec3| -> Option<Pos2> {
            let clip = matrix * world.extend(1.);
            if clip.w <= 0.0001 {
                return None;
            }
            let ndc = clip.truncate() / clip.w;
            Some(Pos2::new(
                (ndc.x * 0.5 + 0.5) * extent.x,
                (ndc.y * 0.5 + 0.5) * extent.y,
            ))
        };
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            "wind_demo_overlay".into(),
        ));
        let draw_band = |gust: &Gust, time: f32| {
            let center = gust_display_center(gust, time);
            let half = gust.half_extents();
            let forward = gust.direction * half.x / 256.;
            let side = Vec2::new(-gust.direction.y, gust.direction.x) * half.y / 256.;
            let position = |uv: Vec2| {
                let offset = forward * uv.x + side * uv.y;
                project(center + Vec3::new(offset.x, 0., offset.y))
            };
            let mut mesh = egui::Mesh::default();
            const STEPS: u32 = 16;
            for y in 0..=STEPS {
                for x in 0..=STEPS {
                    let uv = Vec2::new(x as f32, y as f32) * (2. / STEPS as f32) - Vec2::ONE;
                    let Some(point) = position(uv) else {
                        return;
                    };
                    let alpha = (gust.settings.spatial_weight(uv) * 55.) as u8;
                    mesh.colored_vertex(
                        point,
                        Color32::from_rgba_unmultiplied(130, 200, 240, alpha),
                    );
                }
            }
            for y in 0..STEPS {
                for x in 0..STEPS {
                    let a = y * (STEPS + 1) + x;
                    let b = a + STEPS + 1;
                    mesh.add_triangle(a, a + 1, b);
                    mesh.add_triangle(a + 1, b + 1, b);
                }
            }
            painter.add(egui::Shape::mesh(mesh));
            let corners: Option<Vec<Pos2>> = [
                Vec2::new(-1., -1.),
                Vec2::new(1., -1.),
                Vec2::new(1., 1.),
                Vec2::new(-1., 1.),
            ]
            .into_iter()
            .map(position)
            .collect();
            if let Some(corners) = corners {
                painter.add(egui::Shape::closed_line(
                    corners,
                    Stroke::new(1., Color32::from_rgba_unmultiplied(130, 200, 240, 100)),
                ));
            }
        };
        if let Some(drag) = &self.drag {
            let preview = drag.preview(
                self.field.manual_gust,
                self.speed_multiplier,
                self.radial,
                self.field.time(),
            );
            if !self.radial && preview.direction != Vec2::ZERO {
                draw_band(&preview, self.field.time());
            }
            if let (Some(start), Some(end)) = (
                project(preview.origin),
                project(preview.center(self.field.time() + 1.)),
            ) {
                painter.circle_stroke(start, 5., Stroke::new(1.5, Color32::LIGHT_GREEN));
                painter.arrow(start, end - start, Stroke::new(2., Color32::LIGHT_GREEN));
            }
        }
        for gust in &self.field.gusts {
            if !gust.radial {
                draw_band(gust, self.field.time());
                continue;
            }
            let center = gust_display_center(gust, self.field.time());
            let radius = gust.settings.speed * (self.field.time() - gust.start) / 256.;
            let mut last = None;
            for segment in 0..=48 {
                let angle = segment as f32 * std::f32::consts::TAU / 48.;
                let point = center + Vec3::new(angle.cos(), 0., angle.sin()) * radius;
                let current = project(point);
                if let (Some(a), Some(b)) = (last, current) {
                    painter.line_segment([a, b], Stroke::new(1., Color32::from_rgb(130, 200, 240)));
                }
                last = current;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn released_preview_starts_at_the_clicked_height() {
        let clicked = Vec3::new(0.8, 0.73, 1.2);
        let mut field = WindField::default();
        let drag = Drag {
            origin: clicked,
            endpoint: clicked + Vec3::X * 0.4,
            start_screen: Vec2::ZERO,
        };
        assert!(drag.release(&mut field, 1., false));
        assert_eq!(gust_display_center(&field.gusts[0], field.time()), clicked);
    }

    #[test]
    fn drag_length_and_multiplier_match_the_one_second_preview() {
        let drag = Drag {
            origin: Vec3::new(0.2, 0.73, 1.),
            endpoint: Vec3::new(0.5, 0.73, 1.4),
            start_screen: Vec2::ZERO,
        };
        let mut field = WindField::default();
        let preview = drag.preview(field.manual_gust, 2., false, field.time());
        assert!((preview.settings.speed - 256.).abs() < 1e-4);
        assert!(drag.release(&mut field, 2., false));
        let released = &field.gusts[0];
        assert_eq!(released.settings, preview.settings);
        assert_eq!(released.origin, preview.origin);
        let expected = drag.origin + (drag.endpoint - drag.origin) * 2.;
        assert!(released
            .center(field.time() + 1.)
            .abs_diff_eq(expected, 1e-6));
    }
}

impl App {
    pub(super) fn handle_wind_prototype_event(&mut self, event: &WindowEvent) -> bool {
        let Some(prototype) = self.wind_prototype.as_ref() else {
            return false;
        };
        if matches!(event, WindowEvent::Focused(false)) {
            self.wind_prototype.as_mut().unwrap().cancel();
            return false;
        }
        if let WindowEvent::KeyboardInput { event, .. } = event {
            if event.state == ElementState::Pressed
                && event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                && prototype.drag.is_some()
            {
                self.wind_prototype.as_mut().unwrap().cancel();
                return true;
            }
            if event.state == ElementState::Pressed
                && matches!(
                    event.physical_key,
                    PhysicalKey::Code(KeyCode::KeyR | KeyCode::KeyG | KeyCode::KeyC)
                )
            {
                self.wind_prototype.as_mut().unwrap().cancel();
            }
            return false;
        }
        if !matches!(
            event,
            WindowEvent::MouseInput { .. } | WindowEvent::CursorMoved { .. }
        ) {
            return false;
        }
        if self.player_tools.selected_tool() != PlayerTool::Wind {
            return false;
        }
        if !self.is_orbit_edit_camera_mode()
            || self.config_panel_visible
            || self.card_display_visible
        {
            self.wind_prototype.as_mut().unwrap().cancel();
            return false;
        }
        if let WindowEvent::MouseInput {
            state: ElementState::Pressed,
            button: MouseButton::Right,
            ..
        } = event
        {
            if prototype.drag.is_some() {
                self.wind_prototype.as_mut().unwrap().cancel();
                return true;
            }
        }
        let cursor = self.cursor_position_physical.unwrap_or(Vec2::ZERO);
        let ray = self
            .tracer
            .camera_ray_from_screen_position(cursor, self.window_state.window_extent());
        let target = if let Some(drag) = &prototype.drag {
            // Fix the drag plane at the initial hit height. Uneven terrain does
            // not bend the gesture or change its origin during a drag.
            ray.and_then(|(origin, direction)| {
                let t = (drag.origin.y - origin.y) / direction.y;
                (direction.y.abs() > 0.0001 && t.is_finite() && t > 0.)
                    .then_some(origin + direction * t)
            })
        } else {
            ray.and_then(|(origin, direction)| {
                self.query_terrain_ray_cpu(origin, direction)
                    .map(|hit| hit.position)
            })
        };
        let prototype = self.wind_prototype.as_mut().unwrap();
        match event {
            WindowEvent::CursorMoved { .. } => {
                if let (Some(drag), Some(target)) = (prototype.drag.as_mut(), target) {
                    drag.endpoint = target;
                }
                false
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                if *state == ElementState::Pressed {
                    if let Some(origin) = target {
                        prototype.drag = Some(Drag {
                            origin,
                            start_screen: cursor,
                            endpoint: origin,
                        });
                        prototype.status = "Aim, then release. Right click or Esc cancels.".into();
                    } else {
                        prototype.status = "Point at terrain to choose a gust origin.".into();
                    }
                } else if let Some(drag) = prototype.drag.take() {
                    let threshold = 6. * self.window_state.window().scale_factor() as f32;
                    if prototype.radial || (cursor - drag.start_screen).length() >= threshold {
                        let sent = drag.release(
                            &mut prototype.field,
                            prototype.speed_multiplier,
                            prototype.radial,
                        );
                        prototype.status = if sent {
                            "Gust released; drag again to add another."
                        } else {
                            "No gust: direction is empty or active gust limit reached."
                        }
                        .into();
                    } else {
                        prototype.status = "Drag to aim, or select Radial click.".into();
                    }
                }
                true // Never also dig/paint with the same left-button gesture.
            }
            _ => false,
        }
    }
}
