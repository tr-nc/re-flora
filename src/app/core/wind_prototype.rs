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
    fn preview(&self, settings: GustSettings, multiplier: f32, time: f32) -> Gust {
        let delta = self.endpoint - self.origin;
        let planar = Vec2::new(delta.x, delta.z);
        let speed = planar.length() * 256. * multiplier;
        Gust {
            origin: self.origin,
            direction: planar.normalize_or_zero(),
            settings: GustSettings { speed, ..settings },
            start: time,
        }
    }

    fn release(&self, field: &mut WindField, multiplier: f32) -> bool {
        let preview = self.preview(field.manual_gust, multiplier, field.time());
        field.release_with_settings(preview.origin, preview.direction, preview.settings)
    }
}

fn draw_gust_controls(ui: &mut egui::Ui, settings: &mut GustSettings) {
    ui.add(egui::Slider::new(&mut settings.strength, 0. ..=8.).text("Gust strength"));
    ui.add(egui::Slider::new(&mut settings.width, 8. ..=512.).text("Width (voxels)"));
    ui.add(egui::Slider::new(&mut settings.depth, 4. ..=256.).text("Depth (voxels)"));
    ui.add(egui::Slider::new(&mut settings.softness, 0.05..=1.).text("Edge softness"));
    ui.add(egui::Slider::new(&mut settings.duration, 0.5..=8.).text("Gust lifetime (s)"));
}

pub(super) struct WindPrototype {
    pub field: WindField,
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
            self.field.release(Vec3::new(0.7, 0.5, 1.), Vec2::X);
            self.field.release(Vec3::new(1., 0.5, 1.), Vec2::Y);
        }
        if self.scripted && !self.script_completed && self.field.time() > 4. {
            assert!(self.field.gusts.is_empty(), "expired manual gusts leaked");
            self.script_completed = true;
            log::info!("[WIND_PROTOTYPE] smoke=passed directional_gusts_expired=true");
        }
    }

    pub fn controls(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Background Wind", |ui| {
            ui.label("Temporary controls — never saved to GUI config");
            ui.horizontal_wrapped(|ui| {
                for (mode, name) in [
                    (0, "Saved inflow"),
                    (1, "Turning inflow"),
                    (2, "Detailed inflow"),
                ] {
                    ui.selectable_value(&mut self.field.mode, mode, name);
                }
            });
            ui.small("Boundary inflow and the Wind item share one transported field.");
            if self.field.mode == 0 {
                ui.label("Saved sources feed the scene edges, never the whole scene at once.");
            }
            ui.add_enabled_ui(self.field.mode > 0, |ui| {
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
            });
            ui.separator();
            ui.collapsing("Local detail / transport", |ui| {
                ui.add(
                    egui::Slider::new(&mut self.field.propagation_speed, 0. ..=150.)
                        .text("Transport speed"),
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
            ui.small("Wind input only; plant inertia continues. Audio / free particles unchanged.");
        });
        ui.collapsing("Wind Item", |ui| {
            ui.label("Manual local wind — no automatic gusts");
            draw_gust_controls(ui, &mut self.field.manual_gust);
            ui.add(
                egui::Slider::new(&mut self.speed_multiplier, 0.1..=4.).text("Speed multiplier"),
            );
            ui.small("Drag distance x multiplier = travel speed. Arrow = 1 second.");
            if let Some(drag) = &self.drag {
                let preview = drag.preview(
                    self.field.manual_gust,
                    self.speed_multiplier,
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

    // World-space feedback remains independent of whether the settings are expanded.
    pub fn overlay(
        &mut self,
        ctx: &egui::Context,
        matrix: Mat4,
        extent: Vec2,
        wind_selected: bool,
    ) {
        if !wind_selected {
            self.cancel();
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
                self.field.time(),
            );
            if preview.direction != Vec2::ZERO {
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
            draw_band(gust, self.field.time());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_controls_preserve_aiming_and_have_no_global_direction_widget() {
        let mut prototype = WindPrototype {
            field: WindField::default(),
            speed_multiplier: 1.7,
            drag: Some(Drag {
                origin: Vec3::ZERO,
                endpoint: Vec3::X,
                start_screen: Vec2::ZERO,
            }),
            status: String::new(),
            scripted: false,
            script_started: false,
            script_completed: false,
        };
        let original = prototype.field.frame();
        let context = egui::Context::default();
        context.memory_mut(|memory| memory.set_everything_is_visible(true));
        let mut settings = crate::app::gui_config::DebugSettings::load();
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            settings.draw(ui, |section, ui| {
                if section == "Wind" {
                    prototype.controls(ui);
                }
            });
        });
        fn text(shape: &egui::Shape, labels: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(s) => labels.push(s.galley.job.text.clone()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| text(s, labels)),
                _ => {}
            }
        }
        let mut labels = Vec::new();
        for shape in output.shapes {
            text(&shape.shape, &mut labels);
        }
        for title in [
            "Background Wind",
            "Wind Item",
            "Width (voxels)",
            "Edge softness",
            "Speed multiplier",
        ] {
            assert_eq!(
                labels
                    .iter()
                    .filter(|label| label.as_str() == title)
                    .count(),
                1,
                "missing or duplicated {title}"
            );
        }
        assert!(!labels
            .iter()
            .any(|label| label.contains("Main direction") || label.contains("Compass:")));
        assert!(prototype.drag.is_some());
        assert_eq!(prototype.field.frame(), original);
        assert_eq!(prototype.speed_multiplier, 1.7);
    }

    #[test]
    fn released_preview_starts_at_the_clicked_height() {
        let clicked = Vec3::new(0.8, 0.73, 1.2);
        let mut field = WindField::default();
        let drag = Drag {
            origin: clicked,
            endpoint: clicked + Vec3::X * 0.4,
            start_screen: Vec2::ZERO,
        };
        assert!(drag.release(&mut field, 1.));
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
        let preview = drag.preview(field.manual_gust, 2., field.time());
        assert!((preview.settings.speed - 256.).abs() < 1e-4);
        assert!(drag.release(&mut field, 2.));
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
                    if (cursor - drag.start_screen).length() >= threshold {
                        let sent = drag.release(&mut prototype.field, prototype.speed_multiplier);
                        prototype.status = if sent {
                            "Gust released; drag again to add another."
                        } else {
                            "No gust: direction is empty or active gust limit reached."
                        }
                        .into();
                    } else {
                        prototype.status = "Drag to choose direction and speed.".into();
                    }
                }
                true // Never also dig/paint with the same left-button gesture.
            }
            _ => false,
        }
    }
}
