use glam::{Vec2, Vec3};
use std::time::Instant;
use uuid::Uuid;
use winit::event::{ElementState, MouseButton};

use super::ui_style::{
    FERTILIZER_SLOT_INDEX, HOE_SLOT_INDEX, SHOVEL_SLOT_INDEX, SMOOTH_SLOT_INDEX, STAFF_SLOT_INDEX,
    TILLER_SLOT_INDEX, TREE_SLOT_INDEX, WATERING_SLOT_INDEX,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ContinuousTerrainToolAction {
    ShovelDig,
    ShovelPlace,
    Smooth,
    StaffRegenerate,
    StaffRemove,
    HoeTrim,
    Water,
    Fertilize,
    Till,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlayerToolPointerAction {
    Continuous(ContinuousTerrainToolAction),
    PlaceablePlacement,
    CancelPlaceable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PointerButtons {
    #[default]
    None,
    Left,
    Right,
    Both,
}

impl PointerButtons {
    fn with_state(self, button: MouseButton, state: ElementState) -> Self {
        let pressed = state == ElementState::Pressed;
        let (mut left, mut right) = (self.left(), self.right());
        match button {
            MouseButton::Left => left = pressed,
            MouseButton::Right => right = pressed,
            _ => return self,
        }
        match (left, right) {
            (false, false) => Self::None,
            (true, false) => Self::Left,
            (false, true) => Self::Right,
            (true, true) => Self::Both,
        }
    }

    fn left(self) -> bool {
        matches!(self, Self::Left | Self::Both)
    }

    fn right(self) -> bool {
        matches!(self, Self::Right | Self::Both)
    }

    fn non_empty(self) -> Option<PressedPointerButtons> {
        match self {
            Self::None => None,
            Self::Left => Some(PressedPointerButtons::Left),
            Self::Right => Some(PressedPointerButtons::Right),
            Self::Both => Some(PressedPointerButtons::Both),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PressedPointerButtons {
    Left,
    Right,
    Both,
}

impl PressedPointerButtons {
    fn buttons(self) -> PointerButtons {
        match self {
            Self::Left => PointerButtons::Left,
            Self::Right => PointerButtons::Right,
            Self::Both => PointerButtons::Both,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ToolPointerState {
    #[default]
    Idle,
    IdleWithButtons(PointerButtons),
    Continuous(PressedPointerButtons),
}

impl ToolPointerState {
    fn buttons(self) -> PointerButtons {
        match self {
            Self::Idle => PointerButtons::None,
            Self::IdleWithButtons(buttons) => buttons,
            Self::Continuous(buttons) => buttons.buttons(),
        }
    }

    fn idle(buttons: PointerButtons) -> Self {
        if buttons == PointerButtons::None {
            Self::Idle
        } else {
            Self::IdleWithButtons(buttons)
        }
    }
}

pub(super) struct PlayerToolRuntime {
    pub(super) selected_item_panel_slot: Option<usize>,
    pub(super) selected_placeable_panel_slot: usize,
    pointer: ToolPointerState,
    pub(super) terrain_edit_radius: f32,
    pub(super) last_shovel_dig_time: Option<Instant>,
    pub(super) last_shovel_place_time: Option<Instant>,
    pub(super) last_smooth_time: Option<Instant>,
    pub(super) last_staff_regen_time: Option<Instant>,
    pub(super) last_staff_remove_time: Option<Instant>,
    pub(super) last_staff_regen_center: Option<Vec3>,
    pub(super) last_staff_remove_center: Option<Vec3>,
    pub(super) last_staff_regen_release_time: Option<Instant>,
    pub(super) active_staff_regen_paint_dab_serial: Option<u32>,
    pub(super) flora_paint_selection_index: usize,
    pub(super) last_hoe_trim_time: Option<Instant>,
    pub(super) last_watering_time: Option<Instant>,
    pub(super) last_watering_center: Option<Vec3>,
    pub(super) last_fertilizing_time: Option<Instant>,
    pub(super) last_fertilizing_center: Option<Vec3>,
    pub(super) active_fertilizer_stroke_seed: Option<u32>,
    pub(super) next_fertilizer_stroke_seed: u32,
    pub(super) last_tilling_time: Option<Instant>,
    pub(super) last_tilling_center: Option<Vec3>,
    pub(super) backpack_dirt_count: u32,
    pub(super) backpack_sand_count: u32,
    pub(super) backpack_cherry_wood_count: u32,
    pub(super) backpack_oak_wood_count: u32,
    pub(super) backpack_rock_count: u32,
    pub(super) terrain_edit_loop_sound: Option<Uuid>,
    pub(super) terrain_edit_loop_sound_muted: bool,
    pub(super) backpack_summary_panel_screen_pos: Option<Vec2>,
}

impl Default for PlayerToolRuntime {
    fn default() -> Self {
        Self {
            selected_item_panel_slot: None,
            selected_placeable_panel_slot: super::ui_style::TREE_PLACEABLE_SLOT_INDEX,
            pointer: ToolPointerState::default(),
            terrain_edit_radius: super::TERRAIN_EDIT_DEFAULT_RADIUS,
            last_shovel_dig_time: None,
            last_shovel_place_time: None,
            last_smooth_time: None,
            last_staff_regen_time: None,
            last_staff_remove_time: None,
            last_staff_regen_center: None,
            last_staff_remove_center: None,
            last_staff_regen_release_time: None,
            active_staff_regen_paint_dab_serial: None,
            flora_paint_selection_index: 0,
            last_hoe_trim_time: None,
            last_watering_time: None,
            last_watering_center: None,
            last_fertilizing_time: None,
            last_fertilizing_center: None,
            active_fertilizer_stroke_seed: None,
            next_fertilizer_stroke_seed: 1,
            last_tilling_time: None,
            last_tilling_center: None,
            backpack_dirt_count: 0,
            backpack_sand_count: 0,
            backpack_cherry_wood_count: 0,
            backpack_oak_wood_count: 0,
            backpack_rock_count: 0,
            terrain_edit_loop_sound: None,
            terrain_edit_loop_sound_muted: true,
            backpack_summary_panel_screen_pos: None,
        }
    }
}

impl PlayerToolRuntime {
    pub(super) fn set_pointer_button_state(&mut self, button: MouseButton, state: ElementState) {
        let buttons = self.pointer.buttons().with_state(button, state);
        self.pointer = match self.pointer {
            ToolPointerState::Continuous(_) => buttons
                .non_empty()
                .map_or(ToolPointerState::Idle, ToolPointerState::Continuous),
            ToolPointerState::Idle | ToolPointerState::IdleWithButtons(_) => {
                ToolPointerState::idle(buttons)
            }
        };

        if button == MouseButton::Left {
            self.reset_staff_regen_stroke_tracking();
            self.reset_watering_stroke_tracking();
            self.reset_fertilizing_stroke_tracking();
            if state == ElementState::Pressed {
                self.last_staff_regen_time = None;
            }
        }
        if button == MouseButton::Right {
            self.last_staff_remove_center = None;
            if state == ElementState::Pressed {
                self.last_staff_remove_time = None;
            }
        }
    }

    pub(super) fn begin_pointer_action(
        &mut self,
        button: MouseButton,
    ) -> Option<PlayerToolPointerAction> {
        let buttons = self.pointer.buttons();
        self.pointer = ToolPointerState::idle(buttons);
        let action = self.pointer_action_for_button(button)?;
        if let PlayerToolPointerAction::Continuous(continuous) = action {
            self.pointer = ToolPointerState::Continuous(
                buttons
                    .non_empty()
                    .expect("a pressed tool action must retain its pointer button"),
            );
            match continuous {
                ContinuousTerrainToolAction::Water => self.last_watering_time = None,
                ContinuousTerrainToolAction::Fertilize => self.last_fertilizing_time = None,
                ContinuousTerrainToolAction::Till => self.last_tilling_time = None,
                _ => {}
            }
        }
        Some(action)
    }

    pub(super) fn active_continuous_action(&self) -> Option<ContinuousTerrainToolAction> {
        let ToolPointerState::Continuous(buttons) = self.pointer else {
            return None;
        };
        let buttons = buttons.buttons();
        if buttons.left() {
            if let Some(action) = self.continuous_action_for_button(MouseButton::Left) {
                return Some(action);
            }
        }
        if buttons.right() {
            return self.continuous_action_for_button(MouseButton::Right);
        }
        None
    }

    pub(super) fn continuous_hold_active(&self) -> bool {
        matches!(self.pointer, ToolPointerState::Continuous(_))
    }

    pub(super) fn cancel_continuous_hold(&mut self) {
        self.pointer = ToolPointerState::idle(self.pointer.buttons());
    }

    pub(super) fn finish_pointer_release(&mut self) -> bool {
        let active = self.continuous_hold_active();
        if !active {
            self.reset_staff_stroke_tracking();
            self.reset_watering_stroke_tracking();
            self.reset_fertilizing_stroke_tracking();
        }
        active
    }

    fn pointer_action_for_button(&self, button: MouseButton) -> Option<PlayerToolPointerAction> {
        if let Some(action) = self.continuous_action_for_button(button) {
            return Some(PlayerToolPointerAction::Continuous(action));
        }
        match (self.selected_item_panel_slot, button) {
            (Some(TREE_SLOT_INDEX), MouseButton::Left) => {
                Some(PlayerToolPointerAction::PlaceablePlacement)
            }
            (Some(TREE_SLOT_INDEX), MouseButton::Right) => {
                Some(PlayerToolPointerAction::CancelPlaceable)
            }
            _ => None,
        }
    }

    fn continuous_action_for_button(
        &self,
        button: MouseButton,
    ) -> Option<ContinuousTerrainToolAction> {
        match (self.selected_item_panel_slot, button) {
            (Some(SHOVEL_SLOT_INDEX), MouseButton::Left) => {
                Some(ContinuousTerrainToolAction::ShovelDig)
            }
            (Some(SHOVEL_SLOT_INDEX), MouseButton::Right) => {
                Some(ContinuousTerrainToolAction::ShovelPlace)
            }
            (Some(SMOOTH_SLOT_INDEX), MouseButton::Left) => {
                Some(ContinuousTerrainToolAction::Smooth)
            }
            (Some(STAFF_SLOT_INDEX), MouseButton::Left) => {
                Some(ContinuousTerrainToolAction::StaffRegenerate)
            }
            (Some(STAFF_SLOT_INDEX), MouseButton::Right) => {
                Some(ContinuousTerrainToolAction::StaffRemove)
            }
            (Some(HOE_SLOT_INDEX), MouseButton::Left) => Some(ContinuousTerrainToolAction::HoeTrim),
            (Some(WATERING_SLOT_INDEX), MouseButton::Left) => {
                Some(ContinuousTerrainToolAction::Water)
            }
            (Some(FERTILIZER_SLOT_INDEX), MouseButton::Left) => {
                Some(ContinuousTerrainToolAction::Fertilize)
            }
            (Some(TILLER_SLOT_INDEX), MouseButton::Left) => Some(ContinuousTerrainToolAction::Till),
            _ => None,
        }
    }

    pub(super) fn reset_staff_stroke_tracking(&mut self) {
        self.reset_staff_regen_stroke_tracking();
        self.last_staff_remove_center = None;
    }

    pub(super) fn reset_staff_regen_stroke_tracking(&mut self) {
        self.last_staff_regen_center = None;
        self.last_staff_regen_release_time = None;
        self.active_staff_regen_paint_dab_serial = None;
    }

    pub(super) fn reset_watering_stroke_tracking(&mut self) {
        self.last_watering_center = None;
    }

    pub(super) fn reset_fertilizing_stroke_tracking(&mut self) {
        self.last_fertilizing_center = None;
        self.active_fertilizer_stroke_seed = None;
    }

    pub(super) fn reset_tilling_stroke_tracking(&mut self) {
        self.last_tilling_center = None;
    }

    pub(super) fn active_fertilizer_stroke_seed(&mut self) -> u32 {
        if let Some(seed) = self.active_fertilizer_stroke_seed {
            return seed;
        }

        let seed = self.next_fertilizer_stroke_seed.max(1);
        self.next_fertilizer_stroke_seed = seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223)
            .max(1);
        self.active_fertilizer_stroke_seed = Some(seed);
        seed
    }
}

#[cfg(test)]
mod tests {
    use super::{ContinuousTerrainToolAction, PlayerToolPointerAction, PlayerToolRuntime};
    use winit::event::{ElementState, MouseButton};

    #[test]
    fn hand_is_the_default_player_tool() {
        assert_eq!(PlayerToolRuntime::default().selected_item_panel_slot, None);
    }

    #[test]
    fn every_continuous_tool_maps_pointer_input_to_one_semantic_action() {
        let cases = [
            (
                super::SHOVEL_SLOT_INDEX,
                MouseButton::Left,
                ContinuousTerrainToolAction::ShovelDig,
            ),
            (
                super::SHOVEL_SLOT_INDEX,
                MouseButton::Right,
                ContinuousTerrainToolAction::ShovelPlace,
            ),
            (
                super::SMOOTH_SLOT_INDEX,
                MouseButton::Left,
                ContinuousTerrainToolAction::Smooth,
            ),
            (
                super::STAFF_SLOT_INDEX,
                MouseButton::Left,
                ContinuousTerrainToolAction::StaffRegenerate,
            ),
            (
                super::STAFF_SLOT_INDEX,
                MouseButton::Right,
                ContinuousTerrainToolAction::StaffRemove,
            ),
            (
                super::HOE_SLOT_INDEX,
                MouseButton::Left,
                ContinuousTerrainToolAction::HoeTrim,
            ),
            (
                super::WATERING_SLOT_INDEX,
                MouseButton::Left,
                ContinuousTerrainToolAction::Water,
            ),
            (
                super::FERTILIZER_SLOT_INDEX,
                MouseButton::Left,
                ContinuousTerrainToolAction::Fertilize,
            ),
            (
                super::TILLER_SLOT_INDEX,
                MouseButton::Left,
                ContinuousTerrainToolAction::Till,
            ),
        ];

        for (slot, button, expected) in cases {
            let mut runtime = PlayerToolRuntime {
                selected_item_panel_slot: Some(slot),
                ..PlayerToolRuntime::default()
            };
            runtime.set_pointer_button_state(button, ElementState::Pressed);
            assert_eq!(
                runtime.begin_pointer_action(button),
                Some(PlayerToolPointerAction::Continuous(expected))
            );
            assert_eq!(runtime.active_continuous_action(), Some(expected));
        }
    }

    #[test]
    fn placeable_pointer_input_is_discrete_and_never_starts_a_hold() {
        let mut runtime = PlayerToolRuntime {
            selected_item_panel_slot: Some(super::TREE_SLOT_INDEX),
            ..PlayerToolRuntime::default()
        };

        runtime.set_pointer_button_state(MouseButton::Left, ElementState::Pressed);
        assert_eq!(
            runtime.begin_pointer_action(MouseButton::Left),
            Some(PlayerToolPointerAction::PlaceablePlacement)
        );
        assert!(!runtime.continuous_hold_active());
        runtime.set_pointer_button_state(MouseButton::Right, ElementState::Pressed);
        assert_eq!(
            runtime.begin_pointer_action(MouseButton::Right),
            Some(PlayerToolPointerAction::CancelPlaceable)
        );
        assert!(!runtime.continuous_hold_active());
    }

    #[test]
    fn pointer_runtime_resolves_semantic_actions_and_preserves_left_priority() {
        let mut runtime = PlayerToolRuntime {
            selected_item_panel_slot: Some(super::SHOVEL_SLOT_INDEX),
            ..PlayerToolRuntime::default()
        };

        runtime.set_pointer_button_state(MouseButton::Left, ElementState::Pressed);
        assert_eq!(
            runtime.begin_pointer_action(MouseButton::Left),
            Some(PlayerToolPointerAction::Continuous(
                ContinuousTerrainToolAction::ShovelDig
            ))
        );
        runtime.set_pointer_button_state(MouseButton::Right, ElementState::Pressed);
        assert_eq!(
            runtime.begin_pointer_action(MouseButton::Right),
            Some(PlayerToolPointerAction::Continuous(
                ContinuousTerrainToolAction::ShovelPlace
            ))
        );
        assert_eq!(
            runtime.active_continuous_action(),
            Some(ContinuousTerrainToolAction::ShovelDig)
        );

        runtime.set_pointer_button_state(MouseButton::Left, ElementState::Released);
        assert!(runtime.finish_pointer_release());
        assert_eq!(
            runtime.active_continuous_action(),
            Some(ContinuousTerrainToolAction::ShovelPlace)
        );
        runtime.set_pointer_button_state(MouseButton::Right, ElementState::Released);
        assert!(!runtime.finish_pointer_release());
        assert_eq!(runtime.active_continuous_action(), None);
    }

    #[test]
    fn cancelling_a_hold_does_not_resume_from_buttons_that_remain_pressed() {
        let mut runtime = PlayerToolRuntime {
            selected_item_panel_slot: Some(super::SHOVEL_SLOT_INDEX),
            ..PlayerToolRuntime::default()
        };
        runtime.set_pointer_button_state(MouseButton::Left, ElementState::Pressed);
        runtime.begin_pointer_action(MouseButton::Left);
        runtime.cancel_continuous_hold();

        assert!(!runtime.continuous_hold_active());
        assert_eq!(runtime.active_continuous_action(), None);
    }

    #[test]
    fn fertilizer_stroke_seed_is_stable_until_the_stroke_resets() {
        let mut runtime = PlayerToolRuntime::default();
        let first = runtime.active_fertilizer_stroke_seed();
        assert_eq!(runtime.active_fertilizer_stroke_seed(), first);

        runtime.reset_fertilizing_stroke_tracking();
        let next = runtime.active_fertilizer_stroke_seed();
        assert_ne!(next, first);
        assert_ne!(next, 0);
    }
}
