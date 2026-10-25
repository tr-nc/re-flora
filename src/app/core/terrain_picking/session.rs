//! Gameplay ownership of one captured edit and the newest completed hover.
//! No App, GPU resources, clock reads, or world mutation: callers supply current
//! input facts and the Tracer's scene-validity verdict at completion time.
use super::{Resolved, MAX_PICK_AGE};
use crate::app::core::{
    placeables::PlaceableKind,
    plant_catalog::PlantVariety,
    player_tools::{
        ContinuousTerrainToolAction, PlayerTool, PlayerToolPointerAction, PlayerToolRuntime,
    },
};
use crate::tracer::{PickAnswer, PickPurpose, PickTicket};
use anyhow::Result;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Selection {
    tool: PlayerTool,
    variety: PlantVariety,
    placeable: PlaceableKind,
    radius: u32,
}
#[derive(Clone, Copy)]
pub(super) struct PointerContext {
    available: bool,
    selection: Selection,
    held_action: Option<ContinuousTerrainToolAction>,
}
impl PointerContext {
    pub(super) fn new(tools: &PlayerToolRuntime, available: bool) -> Self {
        Self {
            available,
            selection: Selection {
                tool: tools.selected_tool(),
                variety: tools.planting.variety(),
                placeable: tools.selected_placeable(),
                radius: tools.terrain_edit_radius.to_bits(),
            },
            held_action: tools.active_continuous_action(),
        }
    }
}
struct Pending {
    ticket: PickTicket,
    action: PlayerToolPointerAction,
    selection: Selection,
    requested: Instant,
    require_hold: bool,
}
pub(super) struct AcceptedPick {
    pub action: PlayerToolPointerAction,
    pub resolved: Resolved,
}
#[derive(Default)]
pub(super) struct PointerPickSession {
    pending: Option<Pending>,
    hover: Option<(PickAnswer, Instant)>,
}
impl PointerPickSession {
    /// Submission runs only when this session can own the result. A transport
    /// failure must leave admission open; a second request cannot replace a click.
    pub(super) fn request(
        &mut self,
        action: PlayerToolPointerAction,
        context: PointerContext,
        now: Instant,
        require_hold: bool,
        submit: impl FnOnce() -> Result<PickTicket>,
    ) -> Result<bool> {
        if self.pending.is_some() || !context.available {
            return Ok(false);
        }
        let ticket = submit()?;
        self.pending = Some(Pending {
            ticket,
            action,
            selection: context.selection,
            requested: now,
            require_hold,
        });
        Ok(true)
    }
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
    /// Early transport cleanup only; complete() also enforces the deadline.
    pub(super) fn expired(&self, now: Instant) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| now.duration_since(p.requested) > MAX_PICK_AGE)
    }
    pub(super) fn cancel(&mut self) -> Option<PickTicket> {
        self.hover = None;
        self.pending.take().map(|p| p.ticket)
    }
    /// A matching edit consumes its intent exactly once, even when rejected.
    /// Unrelated answers never consume it. Hover is latest-*completed*-wins, not
    /// latest-requested-wins, so sustained camera motion cannot starve feedback.
    pub(super) fn complete(
        &mut self,
        answer: PickAnswer,
        current_scene: bool,
        context: PointerContext,
        now: Instant,
    ) -> Option<AcceptedPick> {
        if answer.purpose == PickPurpose::Hover {
            if context.available
                && current_scene
                && self
                    .hover
                    .as_ref()
                    .is_none_or(|(old, _)| answer.ticket > old.ticket)
            {
                self.hover = Some((answer, now));
            }
            return None;
        }
        if answer.purpose != PickPurpose::Edit
            || self
                .pending
                .as_ref()
                .is_none_or(|p| p.ticket != answer.ticket)
        {
            return None;
        }
        let pending = self.pending.take().unwrap();
        let hold_matches = !pending.require_hold
            || match pending.action {
                PlayerToolPointerAction::Continuous(action) => context.held_action == Some(action),
                _ => true,
            };
        if !context.available
            || !current_scene
            || pending.selection != context.selection
            || !hold_matches
            || now.duration_since(pending.requested) > MAX_PICK_AGE
        {
            return None;
        }
        Some(AcceptedPick {
            action: pending.action,
            resolved: Resolved {
                ray: answer.ray,
                hit: answer.hit,
            },
        })
    }
    pub(super) fn hover(
        &self,
        now: Instant,
        current_scene: impl FnOnce(&PickAnswer) -> bool,
    ) -> Option<Resolved> {
        let (answer, completed) = self.hover.as_ref()?;
        (now.duration_since(*completed) <= MAX_PICK_AGE && current_scene(answer)).then_some(
            Resolved {
                ray: answer.ray,
                hit: answer.hit,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::core::{
        plant_catalog::PlantType,
        ui_style::{SHOVEL_SLOT_INDEX, STONE_SLOT_INDEX},
    };
    use crate::tracer::{SurfacePickHit, TerrainRayQuery};
    use glam::Vec3;
    use std::time::Duration;
    use winit::event::{ElementState, MouseButton};

    fn answer(ticket: u64, purpose: PickPurpose) -> PickAnswer {
        PickAnswer::test_fixture(
            ticket,
            purpose,
            TerrainRayQuery {
                origin: Vec3::new(ticket as f32, 1., 0.),
                direction: Vec3::NEG_Y,
            },
            None,
        )
    }
    fn context(tools: &PlayerToolRuntime) -> PointerContext {
        PointerContext::new(tools, true)
    }
    fn press_dig() -> (PlayerToolRuntime, PlayerToolPointerAction) {
        let mut tools = PlayerToolRuntime::default();
        tools.select_item_panel_slot(SHOVEL_SLOT_INDEX);
        tools.set_pointer_button_state(MouseButton::Left, ElementState::Pressed);
        let action = tools.begin_pointer_action(MouseButton::Left).unwrap();
        (tools, action)
    }
    fn request(
        session: &mut PointerPickSession,
        ticket: u64,
        action: PlayerToolPointerAction,
        tools: &PlayerToolRuntime,
        now: Instant,
        hold: bool,
    ) {
        session
            .request(action, context(tools), now, hold, || {
                Ok(answer(ticket, PickPurpose::Edit).ticket)
            })
            .unwrap();
    }
    #[test]
    fn quick_release_keeps_initial_click_and_its_captured_miss_not_the_hover() {
        let now = Instant::now();
        let (mut tools, action) = press_dig();
        let mut session = PointerPickSession::default();
        request(&mut session, 1, action, &tools, now, false);
        let mut hover = answer(2, PickPurpose::Hover);
        hover.hit = Some(SurfacePickHit {
            world_position: Vec3::ZERO,
            rest_position: Vec3::ZERO,
            distance: 1.,
            voxel_type: crate::builder::VOXEL_TYPE_DIRT,
            tree_primitive: None,
        });
        assert!(session
            .complete(hover, true, context(&tools), now)
            .is_none());
        tools.set_pointer_button_state(MouseButton::Left, ElementState::Released);
        let accepted = session
            .complete(answer(1, PickPurpose::Edit), true, context(&tools), now)
            .unwrap();
        assert_eq!(accepted.action, action);
        assert_eq!(accepted.resolved.ray.origin, Vec3::new(1., 1., 0.));
        assert!(accepted.resolved.hit.is_none());
        assert!(session.hover(now, |_| true).unwrap().hit.is_some());
        assert!(session
            .complete(answer(1, PickPurpose::Edit), true, context(&tools), now)
            .is_none());
    }
    #[test]
    fn hold_dabs_require_the_original_held_action_at_completion() {
        for mode in 0..3 {
            let now = Instant::now();
            let (mut tools, action) = press_dig();
            let mut session = PointerPickSession::default();
            request(&mut session, 1, action, &tools, now, true);
            if mode != 0 {
                tools.set_pointer_button_state(MouseButton::Left, ElementState::Released);
            }
            if mode == 2 {
                tools.set_pointer_button_state(MouseButton::Right, ElementState::Pressed);
                tools.begin_pointer_action(MouseButton::Right);
            }
            assert_eq!(
                session
                    .complete(answer(1, PickPurpose::Edit), true, context(&tools), now)
                    .is_some(),
                mode == 0
            );
            assert!(
                session.cancel().is_none(),
                "rejected matching intent must be consumed too"
            );
        }
    }
    #[test]
    fn admission_never_submits_when_unavailable_or_busy_and_transport_failure_leaves_it_open() {
        let now = Instant::now();
        let (tools, action) = press_dig();
        let mut session = PointerPickSession::default();
        session
            .request(
                action,
                PointerContext::new(&tools, false),
                now,
                false,
                || panic!("unavailable"),
            )
            .unwrap();
        assert!(session
            .request(action, context(&tools), now, false, || anyhow::bail!(
                "queue full"
            ))
            .is_err());
        request(&mut session, 1, action, &tools, now, false);
        session
            .request(action, context(&tools), now, false, || {
                panic!("cannot replace pending click")
            })
            .unwrap();
        assert_eq!(session.cancel(), Some(answer(1, PickPurpose::Edit).ticket));
        request(&mut session, 2, action, &tools, now, false);
        assert!(session
            .complete(answer(2, PickPurpose::Edit), true, context(&tools), now)
            .is_some());
    }
    #[test]
    fn wrong_ticket_or_purpose_cannot_consume_a_pending_edit() {
        let now = Instant::now();
        let (tools, action) = press_dig();
        let mut session = PointerPickSession::default();
        request(&mut session, 1, action, &tools, now, false);
        for unrelated in [
            answer(2, PickPurpose::Edit),
            answer(1, PickPurpose::Kick),
            answer(1, PickPurpose::Validation),
        ] {
            assert!(session
                .complete(unrelated, true, context(&tools), now)
                .is_none());
        }
        assert!(session
            .complete(answer(1, PickPurpose::Edit), true, context(&tools), now)
            .is_some());
    }
    #[test]
    fn every_captured_selection_fact_is_rechecked_before_applying_an_edit() {
        for changed in 0..4 {
            let now = Instant::now();
            let (mut tools, action) = press_dig();
            let mut session = PointerPickSession::default();
            request(&mut session, 1, action, &tools, now, false);
            match changed {
                0 => {
                    tools.select_item_panel_slot(STONE_SLOT_INDEX);
                }
                1 => {
                    tools.select_plant_type(PlantType::Tree);
                }
                2 => {
                    tools.terrain_edit_radius *= 2.;
                }
                _ => {}
            }
            let mut current = context(&tools);
            // Tree is not currently selectable as a placeable, but remains a
            // captured selection fact for compatibility with the existing guard.
            if changed == 3 {
                current.selection.placeable = PlaceableKind::Tree;
            }
            assert!(session
                .complete(answer(1, PickPurpose::Edit), true, current, now)
                .is_none());
            assert!(session.cancel().is_none());
        }
    }
    #[test]
    fn deadline_and_scene_validity_are_enforced_without_a_prior_poll_cleanup() {
        for (age, scene, available, expected) in [
            (MAX_PICK_AGE, true, true, true),
            (MAX_PICK_AGE + Duration::from_nanos(1), true, true, false),
            (Duration::ZERO, false, true, false),
            (Duration::ZERO, true, false, false),
        ] {
            let now = Instant::now();
            let (tools, action) = press_dig();
            let mut session = PointerPickSession::default();
            request(&mut session, 1, action, &tools, now, false);
            assert_eq!(session.expired(now + age), age > MAX_PICK_AGE);
            assert_eq!(
                session
                    .complete(
                        answer(1, PickPurpose::Edit),
                        scene,
                        PointerContext::new(&tools, available),
                        now + age
                    )
                    .is_some(),
                expected
            );
            assert!(session.cancel().is_none());
        }
    }
    #[test]
    fn hover_uses_newest_current_completion_without_extending_age_for_duplicates() {
        let now = Instant::now();
        let tools = PlayerToolRuntime::default();
        let mut session = PointerPickSession::default();
        assert!(session.hover(now, |_| true).is_none());
        session.complete(answer(2, PickPurpose::Hover), true, context(&tools), now);
        let later = now + Duration::from_millis(100);
        session.complete(answer(1, PickPurpose::Hover), true, context(&tools), later);
        session.complete(answer(2, PickPurpose::Hover), true, context(&tools), later);
        session.complete(answer(3, PickPurpose::Hover), false, context(&tools), later);
        session.complete(
            answer(4, PickPurpose::Hover),
            true,
            PointerContext::new(&tools, false),
            later,
        );
        assert_eq!(session.hover(later, |_| true).unwrap().ray.origin.x, 2.);
        assert!(
            session.hover(later, |_| false).is_none(),
            "scene can change after completion"
        );
        assert!(session.hover(now + MAX_PICK_AGE, |_| true).is_some());
        assert!(session
            .hover(now + MAX_PICK_AGE + Duration::from_nanos(1), |_| true)
            .is_none());
        session.complete(answer(5, PickPurpose::Hover), true, context(&tools), later);
        assert_eq!(session.hover(later, |_| true).unwrap().ray.origin.x, 5.);
    }
    #[test]
    fn cancellation_clears_hover_and_rejects_inflight_edits_after_new_admission() {
        let now = Instant::now();
        let (tools, action) = press_dig();
        let mut session = PointerPickSession::default();
        request(&mut session, 1, action, &tools, now, false);
        session.complete(answer(2, PickPurpose::Hover), true, context(&tools), now);
        assert_eq!(session.cancel(), Some(answer(1, PickPurpose::Edit).ticket));
        assert!(session.hover(now, |_| true).is_none());
        request(&mut session, 3, action, &tools, now, false);
        assert!(session
            .complete(answer(1, PickPurpose::Edit), true, context(&tools), now)
            .is_none());
        assert!(session
            .complete(answer(3, PickPurpose::Edit), true, context(&tools), now)
            .is_some());
    }
    #[test]
    fn discrete_stone_placement_is_accepted_once_without_a_continuous_hold() {
        let now = Instant::now();
        let mut tools = PlayerToolRuntime::default();
        tools.select_item_panel_slot(STONE_SLOT_INDEX);
        tools.set_pointer_button_state(MouseButton::Left, ElementState::Pressed);
        let action = tools.begin_pointer_action(MouseButton::Left).unwrap();
        assert_eq!(action, PlayerToolPointerAction::StonePlacement);
        let mut session = PointerPickSession::default();
        request(&mut session, 1, action, &tools, now, false);
        tools.set_pointer_button_state(MouseButton::Left, ElementState::Released);
        assert_eq!(
            session
                .complete(answer(1, PickPurpose::Edit), true, context(&tools), now)
                .unwrap()
                .action,
            action
        );
        assert!(session
            .complete(answer(1, PickPurpose::Edit), true, context(&tools), now)
            .is_none());
    }
}
