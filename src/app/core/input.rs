use super::ui_style::{
    HOE_SLOT_INDEX, MAX_VOXEL_STORAGE_PER_TYPE, SHOVEL_SLOT_INDEX, STAFF_SLOT_INDEX,
};
use super::App;
use crate::app::world_edits::TerrainRemovalEdit;
use crate::tracer::TerrainRayQuery;
use glam::{Vec2, Vec3};
use std::time::Instant;
use winit::event::DeviceEvent;
use winit::event_loop::ActiveEventLoop;

impl App {
    pub(super) fn sync_cursor_with_panels(&mut self) {
        let any_panel_open = self.config_panel_visible || self.settings_panel_visible;
        self.window_state.set_cursor_visibility(any_panel_open);
        self.window_state.set_cursor_grab(!any_panel_open);
        if any_panel_open {
            self.shovel_dig_held = false;
            self.stop_terrain_edit_loop_sound();
        }
    }

    pub(super) fn is_shovel_selected(&self) -> bool {
        self.selected_item_panel_slot == SHOVEL_SLOT_INDEX
    }

    pub(super) fn is_staff_selected(&self) -> bool {
        self.selected_item_panel_slot == STAFF_SLOT_INDEX
    }

    pub(super) fn is_hoe_selected(&self) -> bool {
        self.selected_item_panel_slot == HOE_SLOT_INDEX
    }

    fn active_voxel_type_id(&self) -> u32 {
        self.active_voxel_type.voxel_type()
    }

    fn active_voxel_count(&self) -> u32 {
        match self.active_voxel_type {
            super::ActiveVoxelType::Dirt => self.backpack_dirt_count,
            super::ActiveVoxelType::Sand => self.backpack_sand_count,
            super::ActiveVoxelType::CherryWood => self.backpack_cherry_wood_count,
            super::ActiveVoxelType::OakWood => self.backpack_oak_wood_count,
            super::ActiveVoxelType::Rock => self.backpack_rock_count,
        }
    }

    fn is_active_voxel_storage_full(&self) -> bool {
        self.active_voxel_count() >= MAX_VOXEL_STORAGE_PER_TYPE
    }

    fn active_voxel_storage_remaining(&self) -> u32 {
        MAX_VOXEL_STORAGE_PER_TYPE.saturating_sub(self.active_voxel_count())
    }

    fn add_active_voxel_to_backpack(&mut self, amount: u32) {
        match self.active_voxel_type {
            super::ActiveVoxelType::Dirt => {
                self.backpack_dirt_count = self
                    .backpack_dirt_count
                    .saturating_add(amount)
                    .min(MAX_VOXEL_STORAGE_PER_TYPE)
            }
            super::ActiveVoxelType::Sand => {
                self.backpack_sand_count = self
                    .backpack_sand_count
                    .saturating_add(amount)
                    .min(MAX_VOXEL_STORAGE_PER_TYPE)
            }
            super::ActiveVoxelType::CherryWood => {
                self.backpack_cherry_wood_count = self
                    .backpack_cherry_wood_count
                    .saturating_add(amount)
                    .min(MAX_VOXEL_STORAGE_PER_TYPE)
            }
            super::ActiveVoxelType::OakWood => {
                self.backpack_oak_wood_count = self
                    .backpack_oak_wood_count
                    .saturating_add(amount)
                    .min(MAX_VOXEL_STORAGE_PER_TYPE)
            }
            super::ActiveVoxelType::Rock => {
                self.backpack_rock_count = self
                    .backpack_rock_count
                    .saturating_add(amount)
                    .min(MAX_VOXEL_STORAGE_PER_TYPE)
            }
        }
    }

    fn remove_active_voxel_from_backpack(&mut self, amount: u32) {
        match self.active_voxel_type {
            super::ActiveVoxelType::Dirt => {
                self.backpack_dirt_count = self.backpack_dirt_count.saturating_sub(amount)
            }
            super::ActiveVoxelType::Sand => {
                self.backpack_sand_count = self.backpack_sand_count.saturating_sub(amount)
            }
            super::ActiveVoxelType::CherryWood => {
                self.backpack_cherry_wood_count =
                    self.backpack_cherry_wood_count.saturating_sub(amount)
            }
            super::ActiveVoxelType::OakWood => {
                self.backpack_oak_wood_count = self.backpack_oak_wood_count.saturating_sub(amount)
            }
            super::ActiveVoxelType::Rock => {
                self.backpack_rock_count = self.backpack_rock_count.saturating_sub(amount)
            }
        }
    }

    pub(super) fn start_terrain_edit_loop_sound(&mut self, position: Vec3) {
        if let Some(uuid) = self.terrain_edit_loop_sound {
            if self.terrain_edit_loop_sound_muted {
                if let Err(err) = self
                    .spatial_sound_manager
                    .update_source_volume(uuid, super::TERRAIN_EDIT_LOOP_VOLUME_DB)
                {
                    log::error!("Failed to unmute terrain edit loop sound: {}", err);
                } else {
                    self.terrain_edit_loop_sound_muted = false;
                }
            }

            if let Err(err) = self.spatial_sound_manager.update_source_pos(uuid, position) {
                log::error!("Failed to update terrain edit loop sound position: {}", err);
            }
            return;
        }

        match self.spatial_sound_manager.add_looping_spatial_source(
            super::TERRAIN_EDIT_LOOP_PATH,
            super::TERRAIN_EDIT_LOOP_VOLUME_DB,
            position,
            true,
        ) {
            Ok(uuid) => {
                self.terrain_edit_loop_sound = Some(uuid);
                self.terrain_edit_loop_sound_muted = false;
            }
            Err(err) => {
                log::error!("Failed to start terrain edit loop sound: {}", err);
            }
        }
    }

    pub(super) fn stop_terrain_edit_loop_sound(&mut self) {
        if self.terrain_edit_loop_sound_muted {
            return;
        }

        if let Some(uuid) = self.terrain_edit_loop_sound {
            if let Err(err) = self
                .spatial_sound_manager
                .update_source_volume(uuid, super::TERRAIN_EDIT_LOOP_MUTED_VOLUME_DB)
            {
                log::error!("Failed to mute terrain edit loop sound: {}", err);
            } else {
                self.terrain_edit_loop_sound_muted = true;
            }
        }
    }

    pub(super) fn play_item_panel_scroll_sound(&self) {
        if let Err(err) = self.spatial_sound_manager.add_non_spatial_source(
            super::ITEM_PANEL_SCROLL_SFX_PATH,
            super::ITEM_PANEL_SCROLL_SFX_VOLUME_DB,
        ) {
            log::error!("Failed to play item panel scroll sound: {}", err);
        }
    }

    fn query_camera_ray_terrain_intersection(
        &mut self,
        max_distance: f32,
    ) -> anyhow::Result<Option<Vec3>> {
        if max_distance <= 0.0 {
            return Ok(None);
        }

        let origin = self.tracer.camera_position();
        let direction = self.tracer.camera_front();
        if direction.length_squared() <= f32::EPSILON {
            return Ok(None);
        }

        let sample = self
            .tracer
            .query_terrain_ray_with_validity(TerrainRayQuery { origin, direction })?;

        let distance = if sample.is_valid {
            (sample.position - origin).length()
        } else {
            f32::INFINITY
        };

        if sample.is_valid && distance <= max_distance {
            return Ok(Some(sample.position));
        }

        Ok(None)
    }

    pub(super) fn update_terrain_query_debug_text(&mut self) {
        let origin = self.tracer.camera_position();
        let direction = self.tracer.camera_front();

        if direction.length_squared() <= f32::EPSILON {
            self.terrain_query_debug_text = "not hit".to_owned();
            return;
        }

        match self
            .tracer
            .query_terrain_ray_with_validity(TerrainRayQuery { origin, direction })
        {
            Ok(sample) if sample.is_valid => {
                self.terrain_query_debug_text = format!(
                    "hit: ({:.3}, {:.3}, {:.3})",
                    sample.position.x, sample.position.y, sample.position.z
                );
            }
            Ok(_) => {
                self.terrain_query_debug_text = "not hit".to_owned();
            }
            Err(err) => {
                log::error!("Failed terrain ray query for debug panel: {}", err);
                self.terrain_query_debug_text = "not hit".to_owned();
            }
        }
    }

    pub(super) fn try_shovel_dig(&mut self, now: Instant) {
        if self.window_state.is_cursor_visible() || !self.is_shovel_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        if self.is_active_voxel_storage_full() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        let remaining_capacity = self.active_voxel_storage_remaining();
        if remaining_capacity == 0 {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        match self.query_camera_ray_terrain_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_dig) = self.last_shovel_dig_time {
                    if now.duration_since(last_dig) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                if let Err(err) = self
                    .apply_surface_terrain_removal(
                        TerrainRemovalEdit {
                            center,
                            radius: super::SHOVEL_REMOVE_RADIUS,
                        },
                        Some(self.active_voxel_type_id()),
                        Some(remaining_capacity),
                    )
                    .map(|stats| {
                        let harvested = stats.count_removed(self.active_voxel_type_id());
                        self.add_active_voxel_to_backpack(harvested);
                        self.spawn_terrain_harvest_particles(center, &stats);
                    })
                {
                    log::error!("Failed to apply terrain removal: {}", err);
                    return;
                }
                self.last_shovel_dig_time = Some(now);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.last_shovel_dig_time = Some(now);
            }
            Err(err) => {
                log::error!("Shovel carve attempt failed during terrain query: {}", err);
            }
        }
    }

    pub(super) fn try_staff_regenerate(&mut self, now: Instant) {
        if self.window_state.is_cursor_visible() || !self.is_staff_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        match self.query_camera_ray_terrain_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_regen) = self.last_staff_regen_time {
                    if now.duration_since(last_regen) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                if let Err(err) = self.apply_surface_flora_regeneration(TerrainRemovalEdit {
                    center,
                    radius: super::SHOVEL_REMOVE_RADIUS,
                }) {
                    log::error!("Failed to apply flora regeneration: {}", err);
                    return;
                }
                self.last_staff_regen_time = Some(now);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.last_staff_regen_time = Some(now);
            }
            Err(err) => {
                log::error!(
                    "Staff regeneration attempt failed during terrain query: {}",
                    err
                );
            }
        }
    }

    pub(super) fn try_shovel_place(&mut self, now: Instant) {
        if self.window_state.is_cursor_visible() || !self.is_shovel_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        if self.active_voxel_count() == 0 {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        let active_voxel_type = self.active_voxel_type_id();
        let active_voxel_count = self.active_voxel_count();

        match self.query_camera_ray_terrain_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_place) = self.last_shovel_place_time {
                    if now.duration_since(last_place) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                if let Err(err) = self
                    .apply_surface_terrain_placement(
                        TerrainRemovalEdit {
                            center,
                            radius: super::SHOVEL_REMOVE_RADIUS,
                        },
                        active_voxel_type,
                        active_voxel_count,
                    )
                    .map(|stats| {
                        self.remove_active_voxel_from_backpack(
                            stats.count_added(self.active_voxel_type_id()),
                        );
                    })
                {
                    log::error!("Failed to apply terrain placement: {}", err);
                    return;
                }
                self.last_shovel_place_time = Some(now);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.last_shovel_place_time = Some(now);
            }
            Err(err) => {
                log::error!("Shovel place attempt failed during terrain query: {}", err);
            }
        }
    }

    pub(super) fn try_hoe_trim(&mut self, now: Instant) {
        if self.window_state.is_cursor_visible() || !self.is_hoe_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        match self.query_camera_ray_terrain_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_trim) = self.last_hoe_trim_time {
                    if now.duration_since(last_trim) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                if let Err(err) = self.apply_flora_trim(TerrainRemovalEdit {
                    center,
                    radius: super::SHOVEL_REMOVE_RADIUS,
                }) {
                    log::error!("Failed to apply flora trim: {}", err);
                    return;
                }
                self.last_hoe_trim_time = Some(now);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.last_hoe_trim_time = Some(now);
            }
            Err(err) => {
                log::error!("Hoe trim attempt failed during terrain query: {}", err);
            }
        }
    }

    pub fn on_device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if !self.window_state.is_cursor_visible() {
                self.accumulated_mouse_delta += Vec2::new(delta.0 as f32, delta.1 as f32);
            }
        }
    }
}
