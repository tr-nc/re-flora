//! Garden adapter: saved materials, pointer gestures and simplified kite loading.
//! The CollisionWorld owns every live node and the only physics accumulator.
//! Anchors are fixed world points, not bindings that follow objects or terrain edits.
mod review;
use super::{player_tools::PlayerTool, App};
use crate::{
    rope::{initial_desc, Material, SavedRope, UNITS},
    tracer::RopeRenderStrand,
};
use anyhow::{ensure, Result};
use glam::Vec3;
use re_flora_physics::rope::RopeId;
use winit::event::{ElementState, MouseButton};

#[derive(Default)]
pub(super) struct SoftRopes {
    entries: Vec<(RopeId, Material)>,
    pending: Option<Vec3>,
    grab: Option<(RopeId, usize, Vec3, Vec3)>, // node, frozen drag plane point/normal
    review_frame: Option<u32>,
}
impl SoftRopes {
    pub fn new() -> Self {
        Self {
            review_frame: std::env::var_os("RE_FLORA_ROPE_REVIEW").map(|_| 0),
            ..Self::default()
        }
    }
}
impl App {
    pub(super) fn capture_soft_ropes(&self) -> Vec<SavedRope> {
        self.soft_ropes
            .entries
            .iter()
            .filter_map(|(id, material)| {
                self.terrain_physics
                    .collision_world
                    .rope(*id)
                    .map(|r| SavedRope::capture(*material, r))
            })
            .collect()
    }
    pub(super) fn restore_soft_ropes(&mut self, saved: Vec<SavedRope>) -> Result<()> {
        ensure!(saved.len() <= 64, "too many saved ropes");
        for s in &saved {
            s.validate()?;
        }
        let ids = self
            .terrain_physics
            .collision_world
            .replace_ropes(saved.iter().map(SavedRope::desc).collect())?;
        self.soft_ropes.entries = ids
            .into_iter()
            .zip(saved.into_iter().map(|s| s.material))
            .collect();
        self.soft_ropes.grab = None;
        self.soft_ropes.pending = None;
        self.render_soft_ropes()
    }
    fn spawn_soft_rope(&mut self, material: Material, a: Vec3, b: Vec3) -> Result<()> {
        let desc = initial_desc(
            material,
            a,
            b,
            self.debug_settings.adjustables.rope_slack.value,
        )?;
        ensure!(
            self.terrain_physics
                .collision_world
                .rope_path_is_clear(&desc)?,
            "The new strand would pass through a solid. Choose closer, unobstructed anchors."
        );
        let id = self.terrain_physics.collision_world.spawn_rope(desc)?;
        self.soft_ropes.entries.push((id, material));
        log::info!("[ROPE] spawned material={} strands={} points={} clock=collision_world contacts=static_capsules",material.name(),self.soft_ropes.entries.len(),self.terrain_physics.collision_world.rope(id).unwrap().positions().len());
        Ok(())
    }
    pub(super) fn try_soft_rope_placement(&mut self) {
        if !self.terrain_edit_pointer_available()
            || !self.terrain_persistence.allows_world_updates()
        {
            return;
        }
        let Some(pick) = self.pointer_picking.active else {
            return;
        };
        let material = Material::from_index(self.debug_settings.adjustables.rope_material.value);
        let Some(support) = self
            .terrain_physics
            .collision_world
            .cast_character_surface_ray(
                pick.ray.origin * UNITS,
                pick.ray.direction,
                super::SHOVEL_RAY_QUERY_DISTANCE * UNITS,
            )
        else {
            self.garden_feedback = Some("Surface collision is not ready for an anchor.".into());
            return;
        };
        let distance = (support.position / UNITS).distance(pick.ray.origin);
        // Fixed model surfaces may be absent from the voxel GPU pick. Never anchor through
        // a nearer visible surface whose collider is pending, or to animated wood.
        if pick.hit.is_some_and(|hit| {
            hit.distance + 0.03 < distance
                || (hit.tree_primitive.is_some() && hit.distance < distance + 0.03)
        }) {
            self.garden_feedback = Some(
                "Choose a fixed surface with published collision, not an animated branch.".into(),
            );
            return;
        }
        let anchor = support.position / UNITS
            + support.normal * (material.physics().radius * 2. + 1. / UNITS);
        let result = if material == Material::Kite {
            self.soft_ropes.pending = None;
            self.spawn_soft_rope(material, anchor, anchor + Vec3::new(0.28, 0.55, 0.08))
        } else if let Some(start) = self.soft_ropes.pending {
            self.spawn_soft_rope(material, start, anchor)
                .map(|()| self.soft_ropes.pending = None)
        } else {
            self.soft_ropes.pending = Some(anchor);
            self.garden_feedback =
                Some("Rope start marked. Click the other end; right-click to cancel.".into());
            return;
        };
        self.garden_feedback=Some(match result {Ok(())=>format!("{} placed. Drag to pull; Shift-click an end to release; right-click a strand to remove.",material.name()),Err(e)=>e.to_string()});
    }
    pub(super) fn cancel_rope_gesture(&mut self) {
        if let Some((id, _, _, _)) = self.soft_ropes.grab.take() {
            if let Some(r) = self.terrain_physics.collision_world.rope_mut(id) {
                let _ = r.set_grab(None);
            }
        }
        self.soft_ropes.pending = None;
    }
    pub(super) fn cycle_rope_material(&mut self) {
        self.cancel_pointer_pick();
        self.cancel_rope_gesture();
        let value = &mut self.debug_settings.adjustables.rope_material.value;
        *value = (*value + 1) % 3;
        self.garden_feedback = Some(format!("Rope: {}", Material::from_index(*value).name()));
    }
    fn pick_soft_rope(
        &mut self,
        origin: Vec3,
        direction: Vec3,
        endpoints_only: bool,
    ) -> Option<(RopeId, usize)> {
        let max_distance = self
            .terrain_physics
            .collision_world
            .cast_character_surface_ray(
                origin * UNITS,
                direction,
                super::SHOVEL_RAY_QUERY_DISTANCE * UNITS,
            )
            .map_or(super::SHOVEL_RAY_QUERY_DISTANCE, |h| {
                h.position.distance(origin * UNITS) / UNITS + 0.015
            });
        let mut nearest = max_distance;
        let mut selected = None;
        for (id, _) in &self.soft_ropes.entries {
            let Some(rope) = self.terrain_physics.collision_world.rope(*id) else {
                continue;
            };
            for (i, p) in rope.positions().iter().enumerate() {
                if endpoints_only && i != 0 && i + 1 != rope.positions().len() {
                    continue;
                }
                let p = *p / UNITS;
                let along = (p - origin).dot(direction);
                if along < 0. || along > nearest {
                    continue;
                }
                if p.distance(origin + direction * along) < (rope.radius() / UNITS + 0.014) {
                    nearest = along;
                    selected = Some((*id, i));
                }
            }
        }
        selected
    }
    pub(super) fn handle_rope_pointer(&mut self, button: MouseButton, state: ElementState) -> bool {
        if button == MouseButton::Left
            && state == ElementState::Released
            && self.soft_ropes.grab.is_some()
        {
            self.cancel_rope_gesture();
            return true;
        }
        if self.player_tools.selected_tool() != PlayerTool::Rope
            || !self.terrain_edit_pointer_available()
            || !self.terrain_persistence.allows_world_updates()
        {
            return false;
        }
        if state != ElementState::Pressed {
            return false;
        }
        let Some((origin, direction)) = self.terrain_edit_ray() else {
            return false;
        };
        let picked = self.pick_soft_rope(
            origin,
            direction,
            self.modifiers.shift_key() && button == MouseButton::Left,
        );
        if button == MouseButton::Right {
            self.cancel_pointer_pick();
            self.cancel_rope_gesture();
            if let Some((id, _)) = picked {
                self.terrain_physics.collision_world.remove_rope(id);
                self.soft_ropes.entries.retain(|e| e.0 != id);
                self.garden_feedback = Some("Rope removed.".into());
            }
            return true;
        }
        if button != MouseButton::Left {
            return false;
        }
        let Some((id, index)) = picked else {
            return false;
        };
        self.cancel_pointer_pick();
        let rope = self.terrain_physics.collision_world.rope_mut(id).unwrap();
        if self.modifiers.shift_key() {
            if index == 0 {
                rope.release_start();
            } else if index + 1 == rope.positions().len() {
                rope.set_end_pinned(false);
            }
            return true;
        }
        if rope.is_pinned(index) {
            self.garden_feedback =
                Some("Shift-click this endpoint to release its world anchor.".into());
            return true;
        }
        let point = rope.positions()[index] / UNITS;
        rope.set_grab(Some((index, point * UNITS)))
            .expect("validated rope pick");
        self.soft_ropes.grab = Some((id, index, point, self.tracer.camera_front()));
        true
    }
    pub(super) fn prepare_soft_ropes(&mut self) -> Result<()> {
        self.advance_rope_review()?;
        if self.player_tools.selected_tool() != PlayerTool::Rope
            || !self.terrain_edit_pointer_available()
        {
            self.cancel_rope_gesture();
        }
        if let Some((id, index, point, normal)) = self.soft_ropes.grab {
            if let Some((origin, direction)) = self.terrain_edit_ray() {
                let denom = direction.dot(normal);
                if denom.abs() > 0.01 {
                    let distance = (point - origin).dot(normal) / denom;
                    if (0.0..=super::SHOVEL_RAY_QUERY_DISTANCE * 2.).contains(&distance) {
                        if let Some(rope) = self.terrain_physics.collision_world.rope_mut(id) {
                            rope.set_grab(Some((index, (origin + direction * distance) * UNITS)))?;
                        }
                    }
                }
            }
        }
        let wind = self.wind_prototype.field.frame();
        let gain = self
            .debug_settings
            .adjustables
            .rope_wind_response
            .value
            .clamp(0., 3.);
        for (id, material) in &self.soft_ropes.entries {
            if let Some(rope) = self.terrain_physics.collision_world.rope_mut(*id) {
                let p = *rope.positions().last().unwrap() / UNITS;
                let (all, end) =
                    crate::rope::wind_acceleration(*material, wind.sample_world(p), gain);
                rope.set_acceleration(all, end)?;
            }
        }
        ensure!(
            self.terrain_physics.collision_world.rope_contact_failures() == 0,
            "unsupported rope collision pair"
        );
        Ok(())
    }
    pub(super) fn render_soft_ropes(&mut self) -> Result<()> {
        let strands = self
            .soft_ropes
            .entries
            .iter()
            .filter_map(|(id, material)| {
                let r = self.terrain_physics.collision_world.rope(*id)?;
                Some(RopeRenderStrand {
                    points: r.positions().iter().map(|p| *p / UNITS).collect(),
                    radius: r.radius() / UNITS,
                    material: *material,
                    pins: [r.is_pinned(0), r.is_pinned(r.positions().len() - 1)],
                })
            })
            .collect::<Vec<_>>();
        self.tracer
            .stage_rope_mesh(&strands, self.soft_ropes.pending)
    }
}
