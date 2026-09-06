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

fn draw_gust_controls(ui: &mut egui::Ui, settings: &mut GustSettings) {
    ui.add(egui::Slider::new(&mut settings.strength, 0. ..=8.).text("Gust strength"));
    ui.add(egui::Slider::new(&mut settings.radius, 8. ..=100.).text("Gust size (voxels)"));
    ui.add(egui::Slider::new(&mut settings.duration, 0.5..=8.).text("Gust lifetime (s)"));
    ui.add(egui::Slider::new(&mut settings.speed, 10. ..=180.).text("Gust travel speed"));
}

pub(super) struct WindPrototype {
    pub field: WindField,
    radial: bool,
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
            self.field.release(Vec2::new(0.7, 1.), Vec2::X, false);
            self.field.release(Vec2::new(1., 1.), Vec2::ZERO, true);
        }
        if self.scripted && !self.script_completed && self.field.time() > 4. {
            assert!(
                self.field.gusts.iter().all(|gust| gust.automatic),
                "expired manual gusts leaked"
            );
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
                    for (mode, name) in [
                        (0, "A Original"),
                        (1, "B Turning"),
                        (2, "C Auto gusts"),
                        (3, "D Local detail"),
                    ] {
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
                ui.add_enabled_ui(self.field.mode >= 2, |ui| {
                    ui.label("Automatic gusts — C / D only");
                    ui.add(
                        egui::Slider::new(&mut self.field.gust_interval, 1. ..=15.)
                            .text("Gust interval (s)"),
                    );
                    ui.collapsing("Automatic gust shape / strength", |ui| {
                        draw_gust_controls(ui, &mut self.field.automatic_gust);
                    });
                });
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
                ui.horizontal(|ui| {
                    ui.toggle_value(&mut self.field.paused, "Hold new wind");
                    if ui.button("Clear gusts").clicked() {
                        self.field.clear();
                    }
                    if ui.button("Restart wind").clicked() {
                        self.field.restart();
                    }
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
                    ui.label("Manual local wind — works in A / B / C / D");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.radial, false, "Directional drag");
                        ui.selectable_value(&mut self.radial, true, "Radial click");
                    });
                    draw_gust_controls(ui, &mut self.field.manual_gust);
                    if !self.radial {
                        ui.small("4:1 crosswind band; soft edges, strongest at center.");
                    }
                    ui.small("These settings do not change automatic gusts.");
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
        let draw_band = |center: Vec3, direction: Vec2, half_extents: Vec2| {
            let forward = direction * half_extents.x / 256.;
            let side = Vec2::new(-direction.y, direction.x) * half_extents.y / 256.;
            // Nested soft fills indicate central strength, not separate gusts.
            for scale in [1., 0.65, 0.3] {
                let corners = [
                    forward + side,
                    forward - side,
                    -forward - side,
                    -forward + side,
                ];
                let points: Option<Vec<Pos2>> = corners
                    .into_iter()
                    .map(|offset| project(center + Vec3::new(offset.x, 0., offset.y) * scale))
                    .collect();
                if let Some(points) = points {
                    painter.add(egui::Shape::convex_polygon(
                        points,
                        Color32::from_rgba_unmultiplied(130, 200, 240, 12),
                        Stroke::new(1., Color32::from_rgba_unmultiplied(130, 200, 240, 100)),
                    ));
                }
            }
        };
        if let Some(drag) = &self.drag {
            let delta = drag.endpoint - drag.origin;
            let direction = Vec2::new(delta.x, delta.z).normalize_or_zero();
            if !self.radial && direction != Vec2::ZERO {
                draw_band(
                    drag.origin,
                    direction,
                    Gust::band_half_extents(self.field.manual_gust.radius),
                );
            }
            if let (Some(start), Some(end)) = (project(drag.origin), project(drag.endpoint)) {
                painter.circle_stroke(start, 7., Stroke::new(2., Color32::LIGHT_GREEN));
                painter.arrow(start, end - start, Stroke::new(3., Color32::LIGHT_GREEN));
            }
        }
        // These are event footprints on the horizontal plane, not sampled wind
        // vectors or terrain-following flow visualization.
        for gust in self.field.visible_gusts() {
            let age = self.field.time() - gust.start;
            let center = if gust.radial {
                gust.origin
            } else {
                gust.origin + gust.direction * gust.speed * age / 256.
            };
            if !gust.radial {
                draw_band(
                    Vec3::new(center.x, 0.5, center.y),
                    gust.direction,
                    gust.half_extents(),
                );
                continue;
            }
            let radius = gust.speed * age / 256.;
            let mut last = None;
            for segment in 0..=32 {
                let angle = segment as f32 * std::f32::consts::TAU / 32.;
                let point = center + Vec2::new(angle.cos(), angle.sin()) * radius;
                let current = project(Vec3::new(point.x, 0.5, point.y));
                if let (Some(a), Some(b)) = (last, current) {
                    painter.line_segment([a, b], Stroke::new(1., Color32::from_rgb(130, 200, 240)));
                }
                last = current;
            }
        }
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
                    let delta = drag.endpoint - drag.origin;
                    let threshold = 6. * self.window_state.window().scale_factor() as f32;
                    if prototype.radial || (cursor - drag.start_screen).length() >= threshold {
                        let sent = prototype.field.release(
                            Vec2::new(drag.origin.x, drag.origin.z),
                            Vec2::new(delta.x, delta.z),
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
