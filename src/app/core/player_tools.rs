use glam::{Vec2, Vec3};
use std::time::Instant;
use uuid::Uuid;

pub(super) struct PlayerToolState {
    pub(super) selected_item_panel_slot: usize,
    pub(super) left_mouse_held: bool,
    pub(super) right_mouse_held: bool,
    pub(super) shovel_dig_held: bool,
    pub(super) terrain_edit_radius: f32,
    pub(super) last_shovel_dig_time: Option<Instant>,
    pub(super) last_shovel_place_time: Option<Instant>,
    pub(super) last_smooth_time: Option<Instant>,
    pub(super) last_staff_regen_time: Option<Instant>,
    pub(super) last_staff_remove_time: Option<Instant>,
    pub(super) last_staff_regen_center: Option<Vec3>,
    pub(super) last_staff_remove_center: Option<Vec3>,
    pub(super) flora_paint_selection_index: usize,
    pub(super) last_hoe_trim_time: Option<Instant>,
    pub(super) backpack_dirt_count: u32,
    pub(super) backpack_sand_count: u32,
    pub(super) backpack_cherry_wood_count: u32,
    pub(super) backpack_oak_wood_count: u32,
    pub(super) backpack_rock_count: u32,
    pub(super) terrain_edit_loop_sound: Option<Uuid>,
    pub(super) terrain_edit_loop_sound_muted: bool,
    pub(super) backpack_summary_panel_screen_pos: Option<Vec2>,
}

impl Default for PlayerToolState {
    fn default() -> Self {
        Self {
            selected_item_panel_slot: super::ui_style::STAFF_SLOT_INDEX,
            left_mouse_held: false,
            right_mouse_held: false,
            shovel_dig_held: false,
            terrain_edit_radius: super::TERRAIN_EDIT_DEFAULT_RADIUS,
            last_shovel_dig_time: None,
            last_shovel_place_time: None,
            last_smooth_time: None,
            last_staff_regen_time: None,
            last_staff_remove_time: None,
            last_staff_regen_center: None,
            last_staff_remove_center: None,
            flora_paint_selection_index: 0,
            last_hoe_trim_time: None,
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
