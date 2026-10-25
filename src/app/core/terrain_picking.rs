//! Owns captured pointer intent until a bounded, nonblocking GPU answer arrives.
use super::{player_tools::ContinuousTerrainToolAction, App, PlayerToolPointerAction};
use crate::tracer::{PickPurpose, SurfacePickHit, TerrainRayQuery};
mod session;
use session::{PointerContext, PointerPickSession};
use std::time::{Duration, Instant};

pub(super) const MAX_PICK_AGE: Duration = Duration::from_millis(250);
#[derive(Clone, Copy)]
pub(super) struct Resolved {
    pub ray: TerrainRayQuery,
    pub hit: Option<SurfacePickHit>,
}
/// Pending GPU work is not a confirmed miss. Gameplay still accepts only a
/// current resolved answer; presentation may retain its existing world entity.
#[derive(Clone, Copy)]
pub(super) enum HoverObservation {
    Pending,
    Unavailable,
    Resolved(Resolved),
}
#[cfg(test)]
impl HoverObservation {
    pub fn resolved(self) -> Option<Resolved> {
        match self {
            Self::Resolved(pick) => Some(pick),
            _ => None,
        }
    }
}
#[derive(Default)]
pub(super) struct PointerPicking {
    pub active: Option<Resolved>,
    session: PointerPickSession,
}

/// An edit's resolved miss is also authoritative: never substitute another
/// completed hover ray while applying its captured pointer intent.
pub(super) fn captured_pick_or_hover(
    active: Option<Resolved>,
    hover: impl FnOnce() -> HoverObservation,
) -> HoverObservation {
    active.map(HoverObservation::Resolved).unwrap_or_else(hover)
}
fn continuous_pick_interval(
    action: ContinuousTerrainToolAction,
    flora_interval: Duration,
) -> Duration {
    match action {
        // Soil's saved batch clock owns its cadence, not the 80-ms dig timer.
        ContinuousTerrainToolAction::SoilDeposit => Duration::ZERO,
        ContinuousTerrainToolAction::StaffRegenerate => flora_interval,
        _ => super::SHOVEL_DIG_INTERVAL,
    }
}

impl App {
    pub(super) fn cancel_pointer_pick(&mut self) {
        self.cancel_tree_kick();
        self.soil_shovel.cancel_gesture();
        if let Some(ticket) = self.pointer_picking.session.cancel() {
            self.tracer.cancel_surface_pick(ticket);
        }
    }
    pub(super) fn queue_pointer_pick(
        &mut self,
        action: PlayerToolPointerAction,
        now: Instant,
        require_hold: bool,
    ) {
        let Some((origin, direction)) = self.terrain_edit_ray() else {
            return;
        };
        self.queue_captured_pointer_pick(
            action,
            TerrainRayQuery { origin, direction },
            now,
            require_hold,
        );
    }
    pub(super) fn queue_captured_pointer_pick(
        &mut self,
        action: PlayerToolPointerAction,
        ray: TerrainRayQuery,
        now: Instant,
        require_hold: bool,
    ) -> bool {
        let context = PointerContext::new(
            &self.player_tools,
            self.terrain_edit_pointer_available()
                && self.terrain_persistence.allows_world_updates(),
        );
        match self
            .pointer_picking
            .session
            .request(action, context, now, require_hold, || {
                self.tracer.request_surface_pick(
                    ray,
                    self.visible_terrain_revision,
                    PickPurpose::Edit,
                )
            }) {
            Ok(submitted) => submitted,
            Err(err) => {
                log::error!("Cannot queue pointer pick: {err:#}");
                false
            }
        }
    }
    pub(super) fn execute_continuous_terrain_tool_action(
        &mut self,
        action: ContinuousTerrainToolAction,
        now: Instant,
    ) {
        let interval = continuous_pick_interval(action, self.current_flora_paint_dab_interval());
        if self.player_tools.stroke_ready(action, now, interval) {
            self.queue_pointer_pick(PlayerToolPointerAction::Continuous(action), now, true);
        }
    }
    pub(super) fn poll_pointer_picks(&mut self) -> anyhow::Result<()> {
        let now = Instant::now();
        let available = self.terrain_edit_pointer_available()
            && self.terrain_persistence.allows_world_updates();
        if !available || self.pointer_picking.session.expired(now) {
            self.cancel_pointer_pick();
        }
        for answer in self.tracer.poll_surface_picks()? {
            if answer.purpose == PickPurpose::Kick {
                self.complete_tree_kick(answer, now)?;
                continue;
            }
            let current = self
                .tracer
                .pick_answer_is_current(&answer, self.visible_terrain_revision);
            let context = PointerContext::new(&self.player_tools, available);
            let Some(accepted) = self
                .pointer_picking
                .session
                .complete(answer, current, context, now)
            else {
                continue;
            };
            // Only this answer is visible to the edit body. Camera motion cannot
            // replace the captured click ray or trigger a second query for wood.
            self.apply_resolved_pointer_pick(
                accepted.action,
                accepted.resolved.ray,
                accepted.resolved.hit,
                now,
            );
            // A quick press/release still applies its initial dab. It must not
            // leave connectivity hold work open after the release happened.
            if !self.player_tools.continuous_hold_active() {
                self.finish_player_terrain_connectivity_hold()?;
            }
        }
        self.soil_shovel
            .reconcile_pending_pick(self.pointer_picking.session.has_pending());
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
            PlayerToolPointerAction::ShovelScoop | PlayerToolPointerAction::ShovelDrop => {
                self.complete_soil_shovel_pick(action)
            }
            PlayerToolPointerAction::FenceAction => {
                self.stop_terrain_edit_loop_sound();
                self.try_garden_fence_action();
            }
            PlayerToolPointerAction::LanternPlacement => {
                self.stop_terrain_edit_loop_sound();
                self.try_lantern_placement();
            }
            PlayerToolPointerAction::StonePlacement => {
                self.stop_terrain_edit_loop_sound();
                self.try_stone_placement();
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
    pub(super) fn observe_pointer_hover(&mut self) -> HoverObservation {
        let Some((origin, direction)) = self.terrain_edit_ray() else {
            return HoverObservation::Unavailable;
        };
        let ray = TerrainRayQuery { origin, direction };
        // Queued motion is latest-wins. Accept the newest *completed* answer,
        // not only the latest requested ticket (which could starve a busy GPU).
        let _ = self.tracer.request_surface_pick(
            ray,
            self.visible_terrain_revision,
            PickPurpose::Hover,
        );
        let Some(pick) = self
            .pointer_picking
            .session
            .hover(Instant::now(), |answer| {
                self.tracer
                    .pick_answer_is_current(answer, self.visible_terrain_revision)
            })
        else {
            return HoverObservation::Pending;
        };
        HoverObservation::Resolved(self.resolve_pointer_pick(pick.ray, pick.hit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn soil_retains_its_batch_clock_without_inheriting_dig_or_grow_intervals() {
        let flora = Duration::from_millis(35);
        assert_eq!(
            continuous_pick_interval(ContinuousTerrainToolAction::SoilDeposit, flora),
            Duration::ZERO
        );
        assert_eq!(
            continuous_pick_interval(ContinuousTerrainToolAction::StaffRegenerate, flora),
            flora
        );
        assert_eq!(
            continuous_pick_interval(ContinuousTerrainToolAction::ShovelDig, flora),
            super::super::SHOVEL_DIG_INTERVAL
        );
    }
    #[test]
    fn resolved_soil_click_and_miss_cannot_be_replaced_by_another_hover_ray() {
        let captured = Resolved {
            ray: TerrainRayQuery {
                origin: glam::Vec3::X,
                direction: glam::Vec3::NEG_Y,
            },
            hit: None,
        };
        let resolved = captured_pick_or_hover(Some(captured), || {
            panic!("captured miss must not read hover")
        })
        .resolved()
        .unwrap();
        assert_eq!(resolved.ray.origin, captured.ray.origin);
        assert!(resolved.hit.is_none());
        let resolved = captured_pick_or_hover(None, || HoverObservation::Resolved(captured))
            .resolved()
            .unwrap();
        assert_eq!(resolved.ray.origin, captured.ray.origin);
    }
}
