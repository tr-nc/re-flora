use super::ActiveVoxelType;
use glam::Vec2;
use std::time::Instant;
use uuid::Uuid;

pub(super) struct PlayerToolState {
    pub(super) selected_item_panel_slot: usize,
    pub(super) active_voxel_type: ActiveVoxelType,
    pub(super) left_mouse_held: bool,
    pub(super) right_mouse_held: bool,
    pub(super) shovel_dig_held: bool,
    pub(super) last_shovel_dig_time: Option<Instant>,
    pub(super) last_shovel_place_time: Option<Instant>,
    pub(super) last_staff_regen_time: Option<Instant>,
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
            selected_item_panel_slot: 0,
            active_voxel_type: ActiveVoxelType::All,
            left_mouse_held: false,
            right_mouse_held: false,
            shovel_dig_held: false,
            last_shovel_dig_time: None,
            last_shovel_place_time: None,
            last_staff_regen_time: None,
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
