//! Walking-only interaction; captured K intent uses the existing asynchronous
//! posed-scene picker. Only its checked primitive's canonical tree receives an impulse.
use super::App;
use crate::tracer::{PickAnswer, PickPurpose, PickTicket, SurfacePickHit};
use crate::tree_gen::kick::KickTuning;
use glam::Vec3;
use std::time::{Duration, Instant};

const MAX_AGE: Duration = Duration::from_millis(250);
struct Pending {
    ticket: PickTicket,
    requested: Instant,
    tuning: KickTuning,
    reach: f32,
}
#[derive(Default)]
pub(super) struct TreeKickInteraction {
    pending: Option<Pending>,
    last_kick: Option<Instant>,
}

fn within_reach(eye: Vec3, hit: Vec3, reach: f32) -> bool {
    eye.is_finite()
        && hit.is_finite()
        && reach.is_finite()
        && reach > 0.
        && eye.distance_squared(hit) <= reach * reach
}
fn facing_hit(eye: Vec3, forward: Vec3, hit: Vec3) -> bool {
    let toward = (hit - eye).normalize_or_zero();
    // A completed hover is briefly delayed. Hide it immediately after a turn,
    // rather than advertising K on the previously viewed tree for 250 ms.
    toward.dot(forward.normalize_or_zero()) > 2_f32.to_radians().cos()
}
impl App {
    pub(super) fn tree_kick_rustle_tuning(
        a: &crate::app::gui_config::GuiAdjustables,
    ) -> crate::audio::KickRustleTuning {
        crate::audio::KickRustleTuning {
            duration: a.tree_kick_rustle_duration.value,
            brightness: a.tree_kick_rustle_brightness.value,
            crackle: a.tree_kick_rustle_crackle.value,
            body: a.tree_kick_rustle_body.value,
            shake_frequency: a.tree_kick_frequency.value,
        }
    }
    fn tree_kick_available(&self) -> bool {
        self.is_walk_camera_mode()
            && self.keyboard_tool_shortcuts_available()
            && self.terrain_edit_pointer_available()
            && self.terrain_persistence.allows_world_updates()
            && self.tracer.raster_trees.enabled
            && self.debug_settings.adjustables.tree_kick_strength.value > 0.
    }
    fn tree_kick_ready(&self, now: Instant) -> bool {
        self.tree_kick.pending.is_none()
            && self.tree_kick.last_kick.is_none_or(|last| {
                now.duration_since(last).as_secs_f32()
                    >= self.debug_settings.adjustables.tree_kick_cooldown.value
            })
    }
    pub(super) fn picked_tree_id(&self, hit: SurfacePickHit) -> Option<u32> {
        let triangle = *self
            .tracer
            .raster_trees
            .scene
            .primitives
            .get(hit.tree_primitive? as usize)?;
        self.tracer
            .raster_trees
            .rest_mesh
            .triangle_tree([triangle[0], triangle[1], triangle[2]])
    }
    pub(super) fn cancel_tree_kick(&mut self) {
        if let Some(p) = self.tree_kick.pending.take() {
            self.tracer.cancel_surface_pick(p.ticket);
        }
    }
    pub(super) fn update_tree_kick_prompt(&mut self) -> bool {
        let now = Instant::now();
        if !self.tree_kick_available()
            || self
                .tree_kick
                .pending
                .as_ref()
                .is_some_and(|p| now.duration_since(p.requested) > MAX_AGE)
        {
            self.cancel_tree_kick();
            return false;
        }
        // Keep the shared hover updated even during cooldown.
        let hover = match self.observe_pointer_hover() {
            super::terrain_picking::HoverObservation::Resolved(pick) => Some(pick),
            super::terrain_picking::HoverObservation::Pending
            | super::terrain_picking::HoverObservation::Unavailable => None,
        };
        self.tree_kick_ready(now)
            && hover.and_then(|h| h.hit).is_some_and(|hit| {
                self.picked_tree_id(hit)
                    .is_some_and(|id| self.can_accept_tree_kick(id))
                    && within_reach(
                        self.tracer.camera_position(),
                        hit.world_position,
                        self.debug_settings.adjustables.tree_kick_reach.value / 256.,
                    )
                    && facing_hit(
                        self.tracer.camera_position(),
                        self.tracer.camera_front(),
                        hit.world_position,
                    )
            })
    }
    pub(super) fn request_tree_kick(&mut self) {
        if !self.update_tree_kick_prompt() {
            return;
        }
        let Some((origin, direction)) = self.terrain_edit_ray() else {
            return;
        };
        let ray = crate::tracer::TerrainRayQuery { origin, direction };
        // Requery the captured press, not a potentially old cached hover hit.
        // No per-key GPU submission/wait and no tree-topology rebuild.
        match self.tracer.request_surface_pick(
            ray,
            self.visible_terrain_revision,
            PickPurpose::Kick,
        ) {
            Ok(ticket) => {
                self.tree_kick.pending = Some(Pending {
                    ticket,
                    requested: Instant::now(),
                    tuning: self.tree_kick_tuning(),
                    reach: self.debug_settings.adjustables.tree_kick_reach.value / 256.,
                })
            }
            Err(error) => log::debug!("[TREE_KICK] admission deferred: {error:#}"),
        }
    }
    pub(super) fn complete_tree_kick(
        &mut self,
        answer: PickAnswer,
        now: Instant,
    ) -> anyhow::Result<()> {
        if self
            .tree_kick
            .pending
            .as_ref()
            .is_none_or(|p| p.ticket != answer.ticket)
        {
            return Ok(());
        }
        let pending = self.tree_kick.pending.take().unwrap();
        if !self.tree_kick_available()
            || now.duration_since(pending.requested) > MAX_AGE
            || !self
                .tracer
                .pick_answer_is_current(&answer, self.visible_terrain_revision)
        {
            return Ok(());
        }
        let resolved = self.resolve_pointer_pick(answer.ray, answer.hit);
        let Some(hit) = resolved.hit else {
            return Ok(());
        };
        if !within_reach(
            self.tracer.camera_position(),
            hit.world_position,
            pending.reach,
        ) || !within_reach(answer.ray.origin, hit.world_position, pending.reach)
        {
            return Ok(());
        }
        let Some(id) = self.picked_tree_id(hit) else {
            return Ok(());
        };
        if self.kick_tree(id, answer.ray.direction, pending.tuning)? {
            self.tree_kick.last_kick = Some(now);
            log::debug!(
                "[TREE_KICK] tree={id} impulse={} asynchronous=true",
                pending.tuning.strength
            );
        }
        Ok(())
    }
}

pub(super) fn draw_prompt(ctx: &egui::Context, visible: bool) {
    use crate::egui_renderer::matte_glass::MatteFrameExt;
    if !visible {
        return;
    }
    egui::Area::new("tree_kick_prompt".into())
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0., 36.))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .show_matte(ui, |ui| crate::app::ui_text::status(ui, "K · Kick"));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kick_reach_is_surface_distance_and_hover_hides_after_turning() {
        let eye = Vec3::new(1., 0.1, 1.);
        let hit = eye + Vec3::Z * 0.17;
        assert!(within_reach(eye, hit, 0.18));
        assert!(!within_reach(eye, hit, 0.16));
        assert!(!within_reach(Vec3::NAN, hit, 0.18));
        assert!(facing_hit(eye, Vec3::Z, hit));
        assert!(!facing_hit(eye, Vec3::X, hit));
        assert!(!facing_hit(eye, Vec3::Z, eye + Vec3::new(0.02, 0., 0.17)));
    }
}
