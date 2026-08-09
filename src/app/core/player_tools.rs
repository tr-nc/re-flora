use glam::{Vec2, Vec3};
use std::time::Instant;
use uuid::Uuid;
use winit::event::{ElementState, MouseButton};

use super::placeables::PlaceableKind;
use super::ui_style::{
    FERTILIZER_SLOT_INDEX, HAND_SLOT_INDEX, HOE_SLOT_INDEX, PIPE_PLACEABLE_SLOT_INDEX,
    PIPE_SLOT_INDEX, SHOVEL_SLOT_INDEX, SMOOTH_SLOT_INDEX, SOIL_INSPECTOR_SLOT_INDEX,
    SPRINKLER_PLACEABLE_SLOT_INDEX, SPRINKLER_SLOT_INDEX, STAFF_SLOT_INDEX, TILLER_SLOT_INDEX,
    TREE_PLACEABLE_SLOT_INDEX, TREE_SLOT_INDEX, WATERING_SLOT_INDEX,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PlayerTool {
    #[default]
    Hand,
    Staff,
    Shovel,
    Smooth,
    Hoe,
    Watering,
    SoilInspector,
    Fertilizer,
    Tiller,
    Placeable,
}

impl PlayerTool {
    fn from_item_panel_slot(slot_idx: usize) -> Option<Self> {
        match slot_idx {
            HAND_SLOT_INDEX => Some(Self::Hand),
            STAFF_SLOT_INDEX => Some(Self::Staff),
            SHOVEL_SLOT_INDEX => Some(Self::Shovel),
            SMOOTH_SLOT_INDEX => Some(Self::Smooth),
            HOE_SLOT_INDEX => Some(Self::Hoe),
            WATERING_SLOT_INDEX => Some(Self::Watering),
            SOIL_INSPECTOR_SLOT_INDEX => Some(Self::SoilInspector),
            FERTILIZER_SLOT_INDEX => Some(Self::Fertilizer),
            TILLER_SLOT_INDEX => Some(Self::Tiller),
            TREE_SLOT_INDEX => Some(Self::Placeable),
            _ => None,
        }
    }

    fn item_panel_slot(self) -> usize {
        match self {
            Self::Hand => HAND_SLOT_INDEX,
            Self::Staff => STAFF_SLOT_INDEX,
            Self::Shovel => SHOVEL_SLOT_INDEX,
            Self::Smooth => SMOOTH_SLOT_INDEX,
            Self::Hoe => HOE_SLOT_INDEX,
            Self::Watering => WATERING_SLOT_INDEX,
            Self::SoilInspector => SOIL_INSPECTOR_SLOT_INDEX,
            Self::Fertilizer => FERTILIZER_SLOT_INDEX,
            Self::Tiller => TILLER_SLOT_INDEX,
            Self::Placeable => TREE_SLOT_INDEX,
        }
    }

    pub(super) fn uses_terrain_edit_radius(self) -> bool {
        self != Self::Hand
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PlayerToolSelectionUpdate {
    active_tool_changed: bool,
    placeable_changed: bool,
    cancel_placeable_interaction: bool,
}

impl PlayerToolSelectionUpdate {
    pub(super) fn changed(self) -> bool {
        self.active_tool_changed || self.placeable_changed
    }

    pub(super) fn active_tool_changed(self) -> bool {
        self.active_tool_changed
    }

    pub(super) fn cancel_placeable_interaction(self) -> bool {
        self.cancel_placeable_interaction
    }
}

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
    selected_tool: PlayerTool,
    selected_placeable: PlaceableKind,
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
            selected_tool: PlayerTool::Hand,
            selected_placeable: PlaceableKind::Tree,
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
    pub(super) fn selected_tool(&self) -> PlayerTool {
        self.selected_tool
    }

    pub(super) fn selected_placeable(&self) -> PlaceableKind {
        self.selected_placeable
    }

    pub(super) fn selected_item_panel_display_slot(&self) -> usize {
        if self.selected_tool != PlayerTool::Placeable {
            return self.selected_tool.item_panel_slot();
        }
        match self.selected_placeable {
            PlaceableKind::Tree => TREE_SLOT_INDEX,
            PlaceableKind::Sprinkler => SPRINKLER_SLOT_INDEX,
            PlaceableKind::Pipe => PIPE_SLOT_INDEX,
        }
    }

    pub(super) fn clear_selected_tool(&mut self) -> PlayerToolSelectionUpdate {
        self.change_active_tool(PlayerTool::Hand)
    }

    pub(super) fn select_item_panel_slot(&mut self, slot_idx: usize) -> PlayerToolSelectionUpdate {
        let requested_placeable = match slot_idx {
            TREE_SLOT_INDEX => Some(PlaceableKind::Tree),
            SPRINKLER_SLOT_INDEX => Some(PlaceableKind::Sprinkler),
            PIPE_SLOT_INDEX => Some(PlaceableKind::Pipe),
            _ => None,
        };
        if let Some(placeable) = requested_placeable {
            return self.select_placeable(placeable);
        }

        let Some(requested_tool) = PlayerTool::from_item_panel_slot(slot_idx) else {
            return PlayerToolSelectionUpdate::default();
        };
        if requested_tool == self.selected_tool && requested_tool != PlayerTool::Hand {
            self.clear_selected_tool()
        } else {
            self.change_active_tool(requested_tool)
        }
    }

    pub(super) fn select_placeable_tool(&mut self, slot_idx: usize) -> PlayerToolSelectionUpdate {
        let Some(placeable) = Self::placeable_from_panel_slot(slot_idx) else {
            return PlayerToolSelectionUpdate::default();
        };
        self.select_placeable(placeable)
    }

    fn select_placeable(&mut self, placeable: PlaceableKind) -> PlayerToolSelectionUpdate {
        if self.selected_tool == PlayerTool::Placeable && self.selected_placeable == placeable {
            return self.clear_selected_tool();
        }

        let active_tool_changed = self.selected_tool != PlayerTool::Placeable;
        let placeable_changed = self.selected_placeable != placeable;
        let cancel_placeable_interaction =
            self.selected_tool == PlayerTool::Placeable || placeable_changed;
        self.selected_tool = PlayerTool::Placeable;
        self.selected_placeable = placeable;
        if active_tool_changed {
            self.reset_for_active_tool_change();
        }
        PlayerToolSelectionUpdate {
            active_tool_changed,
            placeable_changed,
            cancel_placeable_interaction,
        }
    }

    fn change_active_tool(&mut self, selected_tool: PlayerTool) -> PlayerToolSelectionUpdate {
        if self.selected_tool == selected_tool {
            return PlayerToolSelectionUpdate::default();
        }
        let cancel_placeable_interaction = self.selected_tool == PlayerTool::Placeable;
        self.selected_tool = selected_tool;
        self.reset_for_active_tool_change();
        PlayerToolSelectionUpdate {
            active_tool_changed: true,
            cancel_placeable_interaction,
            ..PlayerToolSelectionUpdate::default()
        }
    }

    fn placeable_from_panel_slot(slot_idx: usize) -> Option<PlaceableKind> {
        match slot_idx {
            TREE_PLACEABLE_SLOT_INDEX => Some(PlaceableKind::Tree),
            SPRINKLER_PLACEABLE_SLOT_INDEX => Some(PlaceableKind::Sprinkler),
            PIPE_PLACEABLE_SLOT_INDEX => Some(PlaceableKind::Pipe),
            _ => None,
        }
    }

    fn reset_for_active_tool_change(&mut self) {
        self.reset_staff_stroke_tracking();
        self.reset_watering_stroke_tracking();
        self.reset_fertilizing_stroke_tracking();
        self.reset_tilling_stroke_tracking();
        self.cancel_continuous_hold();
    }

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
        match (self.selected_tool, button) {
            (PlayerTool::Placeable, MouseButton::Left) => {
                Some(PlayerToolPointerAction::PlaceablePlacement)
            }
            (PlayerTool::Placeable, MouseButton::Right) => {
                Some(PlayerToolPointerAction::CancelPlaceable)
            }
            _ => None,
        }
    }

    fn continuous_action_for_button(
        &self,
        button: MouseButton,
    ) -> Option<ContinuousTerrainToolAction> {
        match (self.selected_tool, button) {
            (PlayerTool::Shovel, MouseButton::Left) => Some(ContinuousTerrainToolAction::ShovelDig),
            (PlayerTool::Shovel, MouseButton::Right) => {
                Some(ContinuousTerrainToolAction::ShovelPlace)
            }
            (PlayerTool::Smooth, MouseButton::Left) => Some(ContinuousTerrainToolAction::Smooth),
            (PlayerTool::Staff, MouseButton::Left) => {
                Some(ContinuousTerrainToolAction::StaffRegenerate)
            }
            (PlayerTool::Staff, MouseButton::Right) => {
                Some(ContinuousTerrainToolAction::StaffRemove)
            }
            (PlayerTool::Hoe, MouseButton::Left) => Some(ContinuousTerrainToolAction::HoeTrim),
            (PlayerTool::Watering, MouseButton::Left) => Some(ContinuousTerrainToolAction::Water),
            (PlayerTool::Fertilizer, MouseButton::Left) => {
                Some(ContinuousTerrainToolAction::Fertilize)
            }
            (PlayerTool::Tiller, MouseButton::Left) => Some(ContinuousTerrainToolAction::Till),
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
    use super::{
        ContinuousTerrainToolAction, PlaceableKind, PlayerTool, PlayerToolPointerAction,
        PlayerToolRuntime,
    };
    use winit::event::{ElementState, MouseButton};

    #[test]
    fn hand_is_the_default_player_tool() {
        let runtime = PlayerToolRuntime::default();

        assert_eq!(runtime.selected_tool(), PlayerTool::Hand);
        assert_eq!(runtime.selected_placeable(), PlaceableKind::Tree);
        assert_eq!(
            runtime.selected_item_panel_display_slot(),
            super::HAND_SLOT_INDEX
        );
    }

    #[test]
    fn item_panel_slots_are_translated_at_the_runtime_boundary() {
        let tool_cases = [
            (super::HAND_SLOT_INDEX, PlayerTool::Hand),
            (super::STAFF_SLOT_INDEX, PlayerTool::Staff),
            (super::SHOVEL_SLOT_INDEX, PlayerTool::Shovel),
            (super::SMOOTH_SLOT_INDEX, PlayerTool::Smooth),
            (super::HOE_SLOT_INDEX, PlayerTool::Hoe),
            (super::WATERING_SLOT_INDEX, PlayerTool::Watering),
            (super::SOIL_INSPECTOR_SLOT_INDEX, PlayerTool::SoilInspector),
            (super::FERTILIZER_SLOT_INDEX, PlayerTool::Fertilizer),
            (super::TILLER_SLOT_INDEX, PlayerTool::Tiller),
        ];
        for (slot, expected) in tool_cases {
            let mut runtime = PlayerToolRuntime::default();
            runtime.select_item_panel_slot(slot);
            assert_eq!(runtime.selected_tool(), expected);
            assert_eq!(runtime.selected_item_panel_display_slot(), slot);
        }

        let placeable_cases = [
            (
                super::TREE_SLOT_INDEX,
                PlaceableKind::Tree,
                super::TREE_SLOT_INDEX,
            ),
            (
                super::SPRINKLER_SLOT_INDEX,
                PlaceableKind::Sprinkler,
                super::SPRINKLER_SLOT_INDEX,
            ),
            (
                super::PIPE_SLOT_INDEX,
                PlaceableKind::Pipe,
                super::PIPE_SLOT_INDEX,
            ),
        ];
        for (slot, expected_placeable, expected_display_slot) in placeable_cases {
            let mut runtime = PlayerToolRuntime::default();
            runtime.select_item_panel_slot(slot);
            assert_eq!(runtime.selected_tool(), PlayerTool::Placeable);
            assert_eq!(runtime.selected_placeable(), expected_placeable);
            assert_eq!(
                runtime.selected_item_panel_display_slot(),
                expected_display_slot
            );
        }

        let mut runtime = PlayerToolRuntime::default();
        assert!(!runtime.select_item_panel_slot(usize::MAX).changed());
        assert_eq!(runtime.selected_tool(), PlayerTool::Hand);
    }

    #[test]
    fn selecting_the_active_tool_toggles_back_to_hand() {
        let mut runtime = PlayerToolRuntime::default();
        assert!(runtime
            .select_item_panel_slot(super::SHOVEL_SLOT_INDEX)
            .active_tool_changed());
        let update = runtime.select_item_panel_slot(super::SHOVEL_SLOT_INDEX);

        assert!(update.active_tool_changed());
        assert_eq!(runtime.selected_tool(), PlayerTool::Hand);
    }

    #[test]
    fn placeable_selection_is_atomic_and_reports_external_cleanup() {
        let mut runtime = PlayerToolRuntime::default();
        let selected = runtime.select_placeable_tool(super::SPRINKLER_PLACEABLE_SLOT_INDEX);
        assert!(selected.active_tool_changed());
        assert!(selected.cancel_placeable_interaction());
        assert_eq!(runtime.selected_tool(), PlayerTool::Placeable);
        assert_eq!(runtime.selected_placeable(), PlaceableKind::Sprinkler);

        let cleared = runtime.select_placeable_tool(super::SPRINKLER_PLACEABLE_SLOT_INDEX);
        assert!(cleared.active_tool_changed());
        assert!(cleared.cancel_placeable_interaction());
        assert_eq!(runtime.selected_tool(), PlayerTool::Hand);

        assert!(!runtime.select_placeable_tool(usize::MAX).changed());
    }

    #[test]
    fn every_continuous_tool_maps_pointer_input_to_one_semantic_action() {
        let cases = [
            (
                PlayerTool::Shovel,
                MouseButton::Left,
                ContinuousTerrainToolAction::ShovelDig,
            ),
            (
                PlayerTool::Shovel,
                MouseButton::Right,
                ContinuousTerrainToolAction::ShovelPlace,
            ),
            (
                PlayerTool::Smooth,
                MouseButton::Left,
                ContinuousTerrainToolAction::Smooth,
            ),
            (
                PlayerTool::Staff,
                MouseButton::Left,
                ContinuousTerrainToolAction::StaffRegenerate,
            ),
            (
                PlayerTool::Staff,
                MouseButton::Right,
                ContinuousTerrainToolAction::StaffRemove,
            ),
            (
                PlayerTool::Hoe,
                MouseButton::Left,
                ContinuousTerrainToolAction::HoeTrim,
            ),
            (
                PlayerTool::Watering,
                MouseButton::Left,
                ContinuousTerrainToolAction::Water,
            ),
            (
                PlayerTool::Fertilizer,
                MouseButton::Left,
                ContinuousTerrainToolAction::Fertilize,
            ),
            (
                PlayerTool::Tiller,
                MouseButton::Left,
                ContinuousTerrainToolAction::Till,
            ),
        ];

        for (tool, button, expected) in cases {
            let mut runtime = PlayerToolRuntime::default();
            runtime.selected_tool = tool;
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
        let mut runtime = PlayerToolRuntime::default();
        runtime.select_item_panel_slot(super::TREE_SLOT_INDEX);

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
        let mut runtime = PlayerToolRuntime::default();
        runtime.select_item_panel_slot(super::SHOVEL_SLOT_INDEX);

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
        let mut runtime = PlayerToolRuntime::default();
        runtime.select_item_panel_slot(super::SHOVEL_SLOT_INDEX);
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
