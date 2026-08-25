use glam::{Vec2, Vec3};
use std::time::{Duration, Instant};
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

impl ContinuousTerrainToolAction {
    fn tracks_path(self) -> bool {
        matches!(
            self,
            Self::StaffRegenerate | Self::StaffRemove | Self::Water | Self::Fertilize | Self::Till
        )
    }
}

#[derive(Debug, Default)]
struct TerrainStroke {
    last_dab_time: Option<Instant>,
    previous_center: Option<Vec3>,
}

impl TerrainStroke {
    fn ready(&self, now: Instant, interval: Duration) -> bool {
        self.last_dab_time
            .is_none_or(|last_dab| now.duration_since(last_dab) >= interval)
    }

    fn record_dab(&mut self, action: ContinuousTerrainToolAction, now: Instant, center: Vec3) {
        self.last_dab_time = Some(now);
        self.previous_center = action.tracks_path().then_some(center);
    }

    fn defer(&mut self, now: Instant) {
        self.last_dab_time = Some(now);
        self.previous_center = None;
    }

    fn interrupt(&mut self) {
        self.previous_center = None;
    }

    fn restart(&mut self) {
        self.last_dab_time = None;
        self.previous_center = None;
    }
}

#[derive(Debug)]
struct TerrainStrokeRuntime {
    shovel_dig: TerrainStroke,
    shovel_place: TerrainStroke,
    smooth: TerrainStroke,
    staff_regenerate: TerrainStroke,
    staff_remove: TerrainStroke,
    hoe_trim: TerrainStroke,
    water: TerrainStroke,
    fertilize: TerrainStroke,
    till: TerrainStroke,
    next_flora_paint_dab_serial: u32,
    active_flora_paint_dab_serial: Option<u32>,
    last_flora_paint_release_time: Option<Instant>,
    next_fertilizer_stroke_seed: u32,
    active_fertilizer_stroke_seed: Option<u32>,
}

impl Default for TerrainStrokeRuntime {
    fn default() -> Self {
        Self {
            shovel_dig: TerrainStroke::default(),
            shovel_place: TerrainStroke::default(),
            smooth: TerrainStroke::default(),
            staff_regenerate: TerrainStroke::default(),
            staff_remove: TerrainStroke::default(),
            hoe_trim: TerrainStroke::default(),
            water: TerrainStroke::default(),
            fertilize: TerrainStroke::default(),
            till: TerrainStroke::default(),
            next_flora_paint_dab_serial: 0,
            active_flora_paint_dab_serial: None,
            last_flora_paint_release_time: None,
            next_fertilizer_stroke_seed: 1,
            active_fertilizer_stroke_seed: None,
        }
    }
}

impl TerrainStrokeRuntime {
    fn tracker(&self, action: ContinuousTerrainToolAction) -> &TerrainStroke {
        match action {
            ContinuousTerrainToolAction::ShovelDig => &self.shovel_dig,
            ContinuousTerrainToolAction::ShovelPlace => &self.shovel_place,
            ContinuousTerrainToolAction::Smooth => &self.smooth,
            ContinuousTerrainToolAction::StaffRegenerate => &self.staff_regenerate,
            ContinuousTerrainToolAction::StaffRemove => &self.staff_remove,
            ContinuousTerrainToolAction::HoeTrim => &self.hoe_trim,
            ContinuousTerrainToolAction::Water => &self.water,
            ContinuousTerrainToolAction::Fertilize => &self.fertilize,
            ContinuousTerrainToolAction::Till => &self.till,
        }
    }

    fn tracker_mut(&mut self, action: ContinuousTerrainToolAction) -> &mut TerrainStroke {
        match action {
            ContinuousTerrainToolAction::ShovelDig => &mut self.shovel_dig,
            ContinuousTerrainToolAction::ShovelPlace => &mut self.shovel_place,
            ContinuousTerrainToolAction::Smooth => &mut self.smooth,
            ContinuousTerrainToolAction::StaffRegenerate => &mut self.staff_regenerate,
            ContinuousTerrainToolAction::StaffRemove => &mut self.staff_remove,
            ContinuousTerrainToolAction::HoeTrim => &mut self.hoe_trim,
            ContinuousTerrainToolAction::Water => &mut self.water,
            ContinuousTerrainToolAction::Fertilize => &mut self.fertilize,
            ContinuousTerrainToolAction::Till => &mut self.till,
        }
    }

    fn ready(&self, action: ContinuousTerrainToolAction, now: Instant, interval: Duration) -> bool {
        self.tracker(action).ready(now, interval)
    }

    fn previous_center(&self, action: ContinuousTerrainToolAction) -> Option<Vec3> {
        self.tracker(action).previous_center
    }

    fn record_dab(&mut self, action: ContinuousTerrainToolAction, now: Instant, center: Vec3) {
        self.tracker_mut(action).record_dab(action, now, center);
    }

    fn defer(&mut self, action: ContinuousTerrainToolAction, now: Instant) {
        self.tracker_mut(action).defer(now);
        self.clear_action_metadata(action);
    }

    fn interrupt(&mut self, action: ContinuousTerrainToolAction) {
        self.tracker_mut(action).interrupt();
        self.clear_action_metadata(action);
    }

    fn restart(&mut self, action: ContinuousTerrainToolAction) {
        self.tracker_mut(action).restart();
        self.clear_action_metadata(action);
    }

    fn interrupt_paths(&mut self) {
        for action in [
            ContinuousTerrainToolAction::StaffRegenerate,
            ContinuousTerrainToolAction::StaffRemove,
            ContinuousTerrainToolAction::Water,
            ContinuousTerrainToolAction::Fertilize,
            ContinuousTerrainToolAction::Till,
        ] {
            self.interrupt(action);
        }
    }

    fn clear_action_metadata(&mut self, action: ContinuousTerrainToolAction) {
        match action {
            ContinuousTerrainToolAction::StaffRegenerate => {
                self.active_flora_paint_dab_serial = None;
                self.last_flora_paint_release_time = None;
            }
            ContinuousTerrainToolAction::Fertilize => {
                self.active_fertilizer_stroke_seed = None;
            }
            _ => {}
        }
    }

    fn flora_paint_dab(
        &mut self,
        now: Instant,
        release_interval: Duration,
        spaced_releases: bool,
    ) -> (u32, bool) {
        if !spaced_releases {
            return (self.next_flora_paint_dab_serial, true);
        }

        if let (Some(active_serial), Some(last_release)) = (
            self.active_flora_paint_dab_serial,
            self.last_flora_paint_release_time,
        ) {
            if now.duration_since(last_release) < release_interval {
                return (active_serial, false);
            }
        }

        let serial = self.next_flora_paint_dab_serial;
        self.next_flora_paint_dab_serial = self.next_flora_paint_dab_serial.wrapping_add(1);
        self.active_flora_paint_dab_serial = Some(serial);
        self.last_flora_paint_release_time = Some(now);
        (serial, true)
    }

    fn fertilizer_stroke_seed(&mut self) -> u32 {
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
    strokes: TerrainStrokeRuntime,
    pub(super) terrain_edit_radius: f32,
    pub(super) flora_paint_selection_index: usize,
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
            strokes: TerrainStrokeRuntime::default(),
            terrain_edit_radius: super::TERRAIN_EDIT_DEFAULT_RADIUS,
            flora_paint_selection_index: 0,
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

        if state == ElementState::Released {
            match button {
                MouseButton::Left => {
                    for action in [
                        ContinuousTerrainToolAction::StaffRegenerate,
                        ContinuousTerrainToolAction::Water,
                        ContinuousTerrainToolAction::Fertilize,
                        ContinuousTerrainToolAction::Till,
                    ] {
                        self.strokes.interrupt(action);
                    }
                }
                MouseButton::Right => self
                    .strokes
                    .interrupt(ContinuousTerrainToolAction::StaffRemove),
                _ => {}
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
            if continuous.tracks_path() {
                self.strokes.restart(continuous);
            }
            self.pointer = ToolPointerState::Continuous(
                buttons
                    .non_empty()
                    .expect("a pressed tool action must retain its pointer button"),
            );
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
        self.strokes.interrupt_paths();
    }

    pub(super) fn finish_pointer_release(&mut self) -> bool {
        let active = self.continuous_hold_active();
        if !active {
            self.strokes.interrupt_paths();
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

    pub(super) fn stroke_ready(
        &self,
        action: ContinuousTerrainToolAction,
        now: Instant,
        interval: Duration,
    ) -> bool {
        self.strokes.ready(action, now, interval)
    }

    pub(super) fn previous_stroke_center(
        &self,
        action: ContinuousTerrainToolAction,
    ) -> Option<Vec3> {
        self.strokes.previous_center(action)
    }

    pub(super) fn record_stroke_dab(
        &mut self,
        action: ContinuousTerrainToolAction,
        now: Instant,
        center: Vec3,
    ) {
        self.strokes.record_dab(action, now, center);
    }

    pub(super) fn defer_stroke(&mut self, action: ContinuousTerrainToolAction, now: Instant) {
        self.strokes.defer(action, now);
    }

    pub(super) fn interrupt_stroke(&mut self, action: ContinuousTerrainToolAction) {
        self.strokes.interrupt(action);
    }

    pub(super) fn restart_stroke(&mut self, action: ContinuousTerrainToolAction) {
        self.strokes.restart(action);
    }

    pub(super) fn flora_paint_dab(
        &mut self,
        now: Instant,
        release_interval: Duration,
        spaced_releases: bool,
    ) -> (u32, bool) {
        self.strokes
            .flora_paint_dab(now, release_interval, spaced_releases)
    }

    pub(super) fn fertilizer_stroke_seed(&mut self) -> u32 {
        self.strokes.fertilizer_stroke_seed()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ContinuousTerrainToolAction, PlaceableKind, PlayerTool, PlayerToolPointerAction,
        PlayerToolRuntime,
    };
    use glam::Vec3;
    use std::time::{Duration, Instant};
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
            let mut runtime = PlayerToolRuntime {
                selected_tool: tool,
                ..Default::default()
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
        let first = runtime.fertilizer_stroke_seed();
        assert_eq!(runtime.fertilizer_stroke_seed(), first);

        runtime.interrupt_stroke(ContinuousTerrainToolAction::Fertilize);
        let next = runtime.fertilizer_stroke_seed();
        assert_ne!(next, first);
        assert_ne!(next, 0);
    }

    #[test]
    fn stroke_runtime_owns_cooldown_and_path_lifecycle() {
        let mut runtime = PlayerToolRuntime::default();
        let action = ContinuousTerrainToolAction::Water;
        let started = Instant::now();
        let center = Vec3::new(1.0, 2.0, 3.0);
        let interval = Duration::from_millis(80);

        assert!(runtime.stroke_ready(action, started, interval));
        runtime.record_stroke_dab(action, started, center);
        assert_eq!(runtime.previous_stroke_center(action), Some(center));
        assert!(!runtime.stroke_ready(action, started + Duration::from_millis(79), interval));
        assert!(runtime.stroke_ready(action, started + interval, interval));

        runtime.defer_stroke(action, started + interval);
        assert_eq!(runtime.previous_stroke_center(action), None);
        assert!(!runtime.stroke_ready(
            action,
            started + interval + Duration::from_millis(1),
            interval
        ));
    }

    #[test]
    fn releasing_each_swept_tool_breaks_its_path_including_tiller() {
        let cases = [
            (
                super::STAFF_SLOT_INDEX,
                ContinuousTerrainToolAction::StaffRegenerate,
            ),
            (
                super::WATERING_SLOT_INDEX,
                ContinuousTerrainToolAction::Water,
            ),
            (
                super::FERTILIZER_SLOT_INDEX,
                ContinuousTerrainToolAction::Fertilize,
            ),
            (super::TILLER_SLOT_INDEX, ContinuousTerrainToolAction::Till),
        ];
        for (slot, action) in cases {
            let mut runtime = PlayerToolRuntime::default();
            runtime.select_item_panel_slot(slot);
            runtime.set_pointer_button_state(MouseButton::Left, ElementState::Pressed);
            runtime.begin_pointer_action(MouseButton::Left);
            runtime.record_stroke_dab(action, Instant::now(), Vec3::ONE);

            runtime.set_pointer_button_state(MouseButton::Left, ElementState::Released);
            runtime.finish_pointer_release();

            assert_eq!(runtime.previous_stroke_center(action), None);
        }
    }

    #[test]
    fn flora_release_serial_is_stable_inside_one_release_interval() {
        let mut runtime = PlayerToolRuntime::default();
        let started = Instant::now();
        let interval = Duration::from_millis(100);

        assert_eq!(runtime.flora_paint_dab(started, interval, true), (0, true));
        assert_eq!(
            runtime.flora_paint_dab(started + Duration::from_millis(99), interval, true),
            (0, false)
        );
        assert_eq!(
            runtime.flora_paint_dab(started + interval, interval, true),
            (1, true)
        );

        runtime.interrupt_stroke(ContinuousTerrainToolAction::StaffRegenerate);
        assert_eq!(
            runtime.flora_paint_dab(started + interval, interval, true),
            (2, true)
        );
    }
}
