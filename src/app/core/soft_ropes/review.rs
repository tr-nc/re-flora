//! Opt-in native validation. Real GPU pick delivery, pointer handlers and per-slot rendering.
use super::super::{terrain_picking::Resolved, ui_style, PlayerToolPointerAction};
use super::*;
use crate::tracer::TerrainRayQuery;
use glam::Vec2;
use std::time::Instant;

impl App {
    fn review_rope_ray(&mut self, id: RopeId) -> Result<()> {
        let r = self.terrain_physics.collision_world.rope(id).unwrap();
        let point = r.positions()[r.positions().len() / 2] / UNITS;
        let origin = self.tracer.camera_position();
        self.pointer_picking.active = Some(Resolved {
            ray: TerrainRayQuery {
                origin,
                direction: (point - origin).normalize(),
            },
            hit: None,
        });
        Ok(())
    }
    pub(super) fn advance_rope_review(&mut self) -> Result<()> {
        let Some(frame) = self.soft_ropes.review_frame else {
            return Ok(());
        };
        let center = Vec3::new(434., 216., 400.) / UNITS;
        if frame == 0 {
            ensure!(
                self.rooftop_scene.is_some(),
                "RE_FLORA_ROPE_REVIEW requires --rooftop-poc"
            );
            ensure!(
                Material::from_index(self.debug_settings.adjustables.rope_material.value)
                    == Material::Hemp,
                "rope review needs the default Hemp material"
            );
            for (i, material) in [Material::Hemp, Material::Hose, Material::Kite]
                .into_iter()
                .enumerate()
            {
                let a = center + Vec3::new(-0.40, 0.28, i as f32 * 0.22 - 0.22);
                let b = if material == Material::Kite {
                    a + Vec3::new(0.38, 0.5, 0.)
                } else {
                    a + Vec3::X * 0.80
                };
                self.spawn_soft_rope(material, a, b)?;
            }
            let target = center + Vec3::new(0., 0.40, 0.);
            self.camera_control.apply_snapshot_mode(true);
            self.camera_control.set_orbit_focus(target);
            ensure!(
                self.tracer
                    .set_camera_pose_looking_at(target + Vec3::new(1.2, 1.0, 1.7), target),
                "rope review camera"
            );
            let extent = self.window_state.window_extent();
            self.cursor_position_physical = Some(Vec2::new(
                extent.width as f32 * 0.5,
                extent.height as f32 * 0.5,
            ));
            self.select_item_panel_slot(ui_style::ROPE_SLOT_INDEX);
            self.wind_prototype.field.release_with_settings(
                center,
                Vec2::X,
                crate::wind_field::GustSettings {
                    strength: 1.2,
                    speed: 0.,
                    width: 1024.,
                    depth: 1024.,
                    duration: 30.,
                    ..Default::default()
                },
            );
            log::info!("[ROPE_REVIEW] started strands=3 gpu_mesh=lit_scene per_slot_buffers=true");
        }
        if frame == 10 || frame == 30 {
            let point = center + Vec3::new(if frame == 10 { -0.25 } else { 0.25 }, 0., -0.36);
            ensure!(
                self.queue_captured_pointer_pick(
                    PlayerToolPointerAction::FenceAction,
                    TerrainRayQuery {
                        origin: point + Vec3::Y * 0.8,
                        direction: Vec3::NEG_Y
                    },
                    Instant::now(),
                    false
                ),
                "queue native rope placement"
            );
        }
        if frame == 25 {
            ensure!(
                self.soft_ropes.pending.is_some(),
                "first GPU-delivered click must mark a rope anchor"
            );
        }
        if frame == 40 {
            ensure!(
                self.queue_captured_pointer_pick(
                    PlayerToolPointerAction::FenceAction,
                    TerrainRayQuery {
                        origin: center + Vec3::Y * 0.8,
                        direction: Vec3::NEG_Y
                    },
                    Instant::now(),
                    false
                ),
                "queue cancellation probe"
            );
            self.pointer_picking.active = Some(Resolved {
                ray: TerrainRayQuery {
                    origin: center + Vec3::splat(5.),
                    direction: Vec3::Y,
                },
                hit: None,
            });
            ensure!(
                self.handle_rope_pointer(MouseButton::Right, ElementState::Pressed),
                "cancel an in-flight GPU placement"
            );
            self.pointer_picking.active = None;
        }
        if frame == 50 {
            ensure!(
                self.soft_ropes.entries.len() == 4 && self.soft_ropes.pending.is_none(),
                "second GPU-delivered click must publish exactly one strand"
            );
        }
        if frame == 60 {
            let id = self.soft_ropes.entries[0].0;
            self.review_rope_ray(id)?;
            ensure!(
                self.handle_rope_pointer(MouseButton::Left, ElementState::Pressed),
                "native rope pointer press"
            );
            self.pointer_picking.active = None;
            ensure!(
                self.soft_ropes.grab.is_some_and(|g| g.0 == id),
                "pointer must select the front hemp strand"
            );
        }
        if frame == 90 {
            ensure!(
                self.handle_rope_pointer(MouseButton::Left, ElementState::Released)
                    && self.soft_ropes.grab.is_none(),
                "native pointer release"
            );
            let id = self.soft_ropes.entries[0].0;
            self.review_rope_ray(id)?;
            ensure!(
                self.handle_rope_pointer(MouseButton::Left, ElementState::Pressed),
                "second native grab"
            );
            self.pointer_picking.active = None;
            self.select_item_panel_slot(ui_style::THIN_POLE_SLOT_INDEX);
            ensure!(
                self.soft_ropes.grab.is_none(),
                "tool switch must release rope drag"
            );
            self.select_item_panel_slot(ui_style::ROPE_SLOT_INDEX);
        }
        if frame == 95 {
            let id = self.soft_ropes.entries[3].0;
            let r = self.terrain_physics.collision_world.rope(id).unwrap();
            let point = *r.positions().last().unwrap() / UNITS;
            let origin = self.tracer.camera_position();
            self.pointer_picking.active = Some(Resolved {
                ray: TerrainRayQuery {
                    origin,
                    direction: (point - origin).normalize(),
                },
                hit: None,
            });
            let previous = self.modifiers;
            self.modifiers
                .set(winit::keyboard::ModifiersState::SHIFT, true);
            let consumed = self.handle_rope_pointer(MouseButton::Left, ElementState::Pressed);
            self.modifiers = previous;
            self.pointer_picking.active = None;
            let r = self.terrain_physics.collision_world.rope(id).unwrap();
            ensure!(
                consumed && !r.is_pinned(r.positions().len() - 1),
                "Shift-click must release the selected endpoint"
            );
        }
        if frame == 100 {
            let id = self.soft_ropes.entries[3].0;
            self.review_rope_ray(id)?;
            ensure!(
                self.handle_rope_pointer(MouseButton::Right, ElementState::Pressed),
                "right-click removal"
            );
            self.pointer_picking.active = None;
            ensure!(
                self.terrain_physics.collision_world.rope(id).is_none()
                    && self.soft_ropes.entries.len() == 3,
                "right-click must remove only the selected strand"
            );
        }
        if frame == 120 {
            let saved = self.capture_soft_ropes();
            for r in &saved {
                r.validate()?;
            }
            let before = serde_json::to_vec(&saved)?;
            self.restore_soft_ropes(serde_json::from_slice(&before)?)?;
            ensure!(
                before == serde_json::to_vec(&self.capture_soft_ropes())?,
                "rope save/load roundtrip changed geometry or material lengths"
            );
            ensure!(
                self.terrain_physics.collision_world.rope_contact_failures() == 0,
                "rope contact failures"
            );
            let max_strain = self
                .soft_ropes
                .entries
                .iter()
                .map(|(id, _)| {
                    self.terrain_physics
                        .collision_world
                        .rope(*id)
                        .unwrap()
                        .max_strain()
                })
                .fold(0., f32::max);
            log::info!("[ROPE_REVIEW] verified=true materials=3 async_placement=true pending_pick_cancel=true pointer_grab_release=true tool_cancel=true endpoint_release=true pointer_removal=true snapshot_roundtrip=true max_strain={max_strain:.5} query_failures=0");
        }
        if frame >= 120 && frame % 120 == 0 {
            let max_strain = self
                .soft_ropes
                .entries
                .iter()
                .map(|(id, _)| {
                    self.terrain_physics
                        .collision_world
                        .rope(*id)
                        .unwrap()
                        .max_strain()
                })
                .fold(0., f32::max);
            ensure!(
                max_strain < 0.15,
                "native gust/contact stretch regression at frame {frame}: {max_strain}"
            );
            log::info!(
                "[ROPE_REVIEW] frame={frame} max_strain={max_strain:.5} gust_contact_stable=true"
            );
        }
        self.soft_ropes.review_frame = Some(frame.saturating_add(1));
        Ok(())
    }
}
