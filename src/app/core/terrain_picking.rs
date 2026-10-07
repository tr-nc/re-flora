//! Owns captured pointer intent until a bounded, nonblocking GPU answer arrives.
use super::{player_tools::ContinuousTerrainToolAction, App, PlayerTool, PlayerToolPointerAction};
use crate::tracer::{PickAnswer, PickPurpose, PickTicket, SurfacePickHit, TerrainRayQuery};
use std::time::{Duration, Instant};

const MAX_PICK_AGE: Duration = Duration::from_millis(250);
struct Pending {
    ticket: PickTicket,
    action: PlayerToolPointerAction,
    tool: PlayerTool,
    variety: super::plant_catalog::PlantVariety,
    placeable: super::placeables::PlaceableKind,
    radius: u32,
    requested: Instant,
    require_hold: bool,
}
#[derive(Clone, Copy)]
pub(super) struct Resolved {
    pub ray: TerrainRayQuery,
    pub hit: Option<SurfacePickHit>,
}
#[derive(Default)]
pub(super) struct PointerPicking {
    pending: Option<Pending>,
    pub active: Option<Resolved>,
    hover: Option<(PickAnswer, Instant)>,
}
fn hold_matches(
    action: PlayerToolPointerAction,
    require_hold: bool,
    active: Option<ContinuousTerrainToolAction>,
) -> bool {
    !require_hold
        || match action {
            PlayerToolPointerAction::Continuous(a) => active == Some(a),
            _ => true,
        }
}

impl App {
    pub(super) fn cancel_pointer_pick(&mut self) {
        self.cancel_tree_kick();
        if let Some(pending) = self.pointer_picking.pending.take() {
            self.tracer.cancel_surface_pick(pending.ticket);
        }
        self.pointer_picking.hover = None;
    }
    pub(super) fn queue_pointer_pick(
        &mut self,
        action: PlayerToolPointerAction,
        now: Instant,
        require_hold: bool,
    ) {
        if self.pointer_picking.pending.is_some()
            || !self.terrain_edit_pointer_available()
            || !self.terrain_persistence.allows_world_updates()
        {
            return;
        }
        let Some((origin, direction)) = self.terrain_edit_ray() else {
            return;
        };
        match self.tracer.request_surface_pick(
            TerrainRayQuery { origin, direction },
            self.visible_terrain_revision,
            PickPurpose::Edit,
        ) {
            Ok(ticket) => {
                self.pointer_picking.pending = Some(Pending {
                    ticket,
                    action,
                    tool: self.player_tools.selected_tool(),
                    variety: self.player_tools.planting.variety(),
                    placeable: self.player_tools.selected_placeable(),
                    radius: self.player_tools.terrain_edit_radius.to_bits(),
                    requested: now,
                    require_hold,
                })
            }
            Err(err) => log::error!("Cannot queue pointer pick: {err:#}"),
        }
    }
    pub(super) fn execute_continuous_terrain_tool_action(
        &mut self,
        action: ContinuousTerrainToolAction,
        now: Instant,
    ) {
        let interval = if action == ContinuousTerrainToolAction::StaffRegenerate {
            self.current_flora_paint_dab_interval()
        } else {
            super::SHOVEL_DIG_INTERVAL
        };
        if self.player_tools.stroke_ready(action, now, interval) {
            self.queue_pointer_pick(PlayerToolPointerAction::Continuous(action), now, true);
        }
    }
    pub(super) fn poll_pointer_picks(&mut self) -> anyhow::Result<()> {
        let now = Instant::now();
        let available = self.terrain_edit_pointer_available()
            && self.terrain_persistence.allows_world_updates();
        if !available
            || self
                .pointer_picking
                .pending
                .as_ref()
                .is_some_and(|p| now.duration_since(p.requested) > MAX_PICK_AGE)
        {
            self.cancel_pointer_pick();
        }
        for answer in self.tracer.poll_surface_picks()? {
            if answer.purpose == PickPurpose::Kick {
                self.complete_tree_kick(answer, now)?;
                continue;
            }
            if answer.purpose == PickPurpose::Hover {
                if self
                    .pointer_picking
                    .hover
                    .as_ref()
                    .is_none_or(|(old, _)| answer.ticket > old.ticket)
                    && available
                    && self
                        .tracer
                        .pick_answer_is_current(&answer, self.visible_terrain_revision)
                {
                    self.pointer_picking.hover = Some((answer, now));
                }
                continue;
            }
            if self
                .pointer_picking
                .pending
                .as_ref()
                .is_none_or(|p| p.ticket != answer.ticket)
            {
                continue;
            }
            let pending = self.pointer_picking.pending.take().unwrap();
            let controls_match = pending.tool == self.player_tools.selected_tool()
                && pending.variety == self.player_tools.planting.variety()
                && pending.placeable == self.player_tools.selected_placeable()
                && pending.radius == self.player_tools.terrain_edit_radius.to_bits();
            let hold_matches = hold_matches(
                pending.action,
                pending.require_hold,
                self.player_tools.active_continuous_action(),
            );
            if !available
                || !controls_match
                || !hold_matches
                || !self
                    .tracer
                    .pick_answer_is_current(&answer, self.visible_terrain_revision)
            {
                continue;
            }
            // Only this answer is visible to the edit body. Camera motion cannot
            // replace the captured click ray or trigger a second query for wood.
            self.apply_resolved_pointer_pick(pending.action, answer.ray, answer.hit, now);
            // A quick press/release still applies its initial dab. It must not
            // leave connectivity hold work open after the release happened.
            if !self.player_tools.continuous_hold_active() {
                self.finish_player_terrain_connectivity_hold()?;
            }
        }
        Ok(())
    }
    pub(super) fn apply_resolved_pointer_pick(
        &mut self,
        action: PlayerToolPointerAction,
        ray: TerrainRayQuery,
        hit: Option<SurfacePickHit>,
        now: Instant,
    ) {
        self.pointer_picking.active = Some(self.resolve_pointer_pick(ray, hit));
        match action {
            PlayerToolPointerAction::Continuous(a) => {
                self.execute_resolved_terrain_tool_action(a, now)
            }
            PlayerToolPointerAction::PlaceablePlacement | PlayerToolPointerAction::PlantTree => {
                self.try_discrete_placement()
            }
        }
        self.pointer_picking.active = None;
    }

    pub(super) fn resolve_pointer_pick(
        &self,
        ray: TerrainRayQuery,
        mut hit: Option<SurfacePickHit>,
    ) -> Resolved {
        // Fixed model supports were already CPU-owned; preserve their occlusion
        // without doing a second CPU tree/voxel traversal.
        if let Some(position) = self
            .rooftop_scene
            .as_ref()
            .and_then(|s| s.ray_hit(ray.origin, ray.direction))
        {
            let distance = position.distance(ray.origin);
            if hit.is_none_or(|h| distance < h.distance) {
                hit = Some(SurfacePickHit {
                    world_position: position,
                    rest_position: position,
                    distance,
                    voxel_type: crate::builder::VOXEL_TYPE_ROCK,
                    tree_primitive: None,
                });
            }
        }
        Resolved { ray, hit }
    }
    pub(super) fn request_pointer_hover(&mut self) -> Option<Resolved> {
        let (origin, direction) = self.terrain_edit_ray()?;
        let ray = TerrainRayQuery { origin, direction };
        // Queued motion is latest-wins. Accept the newest *completed* answer,
        // not only the latest requested ticket (which could starve a busy GPU).
        let _ = self.tracer.request_surface_pick(
            ray,
            self.visible_terrain_revision,
            PickPurpose::Hover,
        );
        let (answer, completed) = self.pointer_picking.hover.as_ref()?;
        if completed.elapsed() > MAX_PICK_AGE
            || !self
                .tracer
                .pick_answer_is_current(answer, self.visible_terrain_revision)
        {
            return None;
        }
        Some(self.resolve_pointer_pick(answer.ray, answer.hit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_click_survives_release_but_queued_hold_dabs_require_the_original_action() {
        let dig = PlayerToolPointerAction::Continuous(ContinuousTerrainToolAction::ShovelDig);
        assert!(hold_matches(dig, false, None));
        assert!(!hold_matches(dig, true, None));
        assert!(!hold_matches(
            dig,
            true,
            Some(ContinuousTerrainToolAction::ShovelPlace)
        ));
        assert!(hold_matches(
            dig,
            true,
            Some(ContinuousTerrainToolAction::ShovelDig)
        ));
        assert!(hold_matches(
            PlayerToolPointerAction::PlantTree,
            false,
            None
        ));
    }
}
