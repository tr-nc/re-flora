use super::ui_style::{
    HOE_SLOT_INDEX, ITEM_PANEL_SLOT_COUNT, MAX_VOXEL_STORAGE_PER_TYPE, SHOVEL_SLOT_INDEX,
    SMOOTH_SLOT_INDEX, STAFF_SLOT_INDEX, WATER_SLOT_INDEX,
};
use super::App;
use crate::app::world_edits::TerrainRemovalEdit;
use crate::builder::{ChunkModifyStats, EDIT_STATS_VOXEL_TYPE_COUNT};
use crate::tracer::TerrainEditPreviewShape;
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
            self.player_tools.shovel_dig_held = false;
            self.stop_terrain_edit_loop_sound();
        }
    }

    pub(super) fn select_item_panel_slot(&mut self, slot_idx: usize) {
        if slot_idx < ITEM_PANEL_SLOT_COUNT
            && slot_idx != self.player_tools.selected_item_panel_slot
        {
            self.player_tools.selected_item_panel_slot = slot_idx;
            self.play_item_panel_scroll_sound();
        }
    }

    pub(super) fn is_shovel_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == SHOVEL_SLOT_INDEX
    }

    pub(super) fn is_smooth_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == SMOOTH_SLOT_INDEX
    }

    pub(super) fn is_terrain_edit_radius_tool_selected(&self) -> bool {
        self.is_shovel_selected() || self.is_smooth_selected()
    }

    pub(super) fn terrain_edit_preview_shape(&self) -> TerrainEditPreviewShape {
        if self.is_smooth_selected() {
            TerrainEditPreviewShape::SurfaceCircle
        } else {
            TerrainEditPreviewShape::Sphere
        }
    }

    pub(super) fn is_staff_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == STAFF_SLOT_INDEX
    }

    pub(super) fn is_hoe_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == HOE_SLOT_INDEX
    }

    pub(super) fn is_water_tool_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == WATER_SLOT_INDEX
    }

    fn active_voxel_type_id(&self) -> Option<u32> {
        self.player_tools.active_voxel_type.voxel_type()
    }

    pub(super) fn voxel_count(&self, voxel_type: super::ActiveVoxelType) -> u32 {
        match voxel_type {
            super::ActiveVoxelType::All => super::BACKPACK_VOXEL_TYPES
                .iter()
                .map(|voxel_type| self.voxel_count(*voxel_type))
                .sum(),
            super::ActiveVoxelType::Dirt => self.player_tools.backpack_dirt_count,
            super::ActiveVoxelType::Sand => self.player_tools.backpack_sand_count,
            super::ActiveVoxelType::CherryWood => self.player_tools.backpack_cherry_wood_count,
            super::ActiveVoxelType::OakWood => self.player_tools.backpack_oak_wood_count,
            super::ActiveVoxelType::Rock => self.player_tools.backpack_rock_count,
        }
    }

    fn active_voxel_count(&self) -> u32 {
        self.voxel_count(self.player_tools.active_voxel_type)
    }

    fn is_active_voxel_storage_full(&self) -> bool {
        if self.player_tools.active_voxel_type == super::ActiveVoxelType::All {
            return super::BACKPACK_VOXEL_TYPES
                .iter()
                .all(|voxel_type| self.voxel_count(*voxel_type) >= MAX_VOXEL_STORAGE_PER_TYPE);
        }

        self.active_voxel_count() >= MAX_VOXEL_STORAGE_PER_TYPE
    }

    fn active_voxel_storage_remaining(&self) -> u32 {
        if self.player_tools.active_voxel_type == super::ActiveVoxelType::All {
            return super::BACKPACK_VOXEL_TYPES
                .iter()
                .map(|voxel_type| self.voxel_storage_remaining(*voxel_type))
                .sum();
        }

        self.voxel_storage_remaining(self.player_tools.active_voxel_type)
    }

    fn voxel_storage_remaining(&self, voxel_type: super::ActiveVoxelType) -> u32 {
        if voxel_type == super::ActiveVoxelType::All {
            return super::BACKPACK_VOXEL_TYPES
                .iter()
                .map(|voxel_type| self.voxel_storage_remaining(*voxel_type))
                .sum();
        }

        MAX_VOXEL_STORAGE_PER_TYPE.saturating_sub(self.voxel_count(voxel_type))
    }

    fn active_removed_voxel_limits(&self) -> [u32; EDIT_STATS_VOXEL_TYPE_COUNT] {
        let mut limits = [0; EDIT_STATS_VOXEL_TYPE_COUNT];
        let mut set_limit = |voxel_type: super::ActiveVoxelType, remaining: u32| {
            if let Some(voxel_type_id) = voxel_type.voxel_type() {
                if let Some(limit) = limits.get_mut(voxel_type_id as usize) {
                    *limit = remaining;
                }
            }
        };

        if self.player_tools.active_voxel_type == super::ActiveVoxelType::All {
            for voxel_type in super::BACKPACK_VOXEL_TYPES {
                set_limit(voxel_type, self.voxel_storage_remaining(voxel_type));
            }
        } else {
            set_limit(
                self.player_tools.active_voxel_type,
                self.voxel_storage_remaining(self.player_tools.active_voxel_type),
            );
        }

        limits
    }

    fn add_voxel_to_backpack(&mut self, voxel_type: super::ActiveVoxelType, amount: u32) {
        match voxel_type {
            super::ActiveVoxelType::All => unreachable!("All is not a concrete backpack voxel"),
            super::ActiveVoxelType::Dirt => {
                self.player_tools.backpack_dirt_count = self
                    .player_tools
                    .backpack_dirt_count
                    .saturating_add(amount)
                    .min(MAX_VOXEL_STORAGE_PER_TYPE)
            }
            super::ActiveVoxelType::Sand => {
                self.player_tools.backpack_sand_count = self
                    .player_tools
                    .backpack_sand_count
                    .saturating_add(amount)
                    .min(MAX_VOXEL_STORAGE_PER_TYPE)
            }
            super::ActiveVoxelType::CherryWood => {
                self.player_tools.backpack_cherry_wood_count = self
                    .player_tools
                    .backpack_cherry_wood_count
                    .saturating_add(amount)
                    .min(MAX_VOXEL_STORAGE_PER_TYPE)
            }
            super::ActiveVoxelType::OakWood => {
                self.player_tools.backpack_oak_wood_count = self
                    .player_tools
                    .backpack_oak_wood_count
                    .saturating_add(amount)
                    .min(MAX_VOXEL_STORAGE_PER_TYPE)
            }
            super::ActiveVoxelType::Rock => {
                self.player_tools.backpack_rock_count = self
                    .player_tools
                    .backpack_rock_count
                    .saturating_add(amount)
                    .min(MAX_VOXEL_STORAGE_PER_TYPE)
            }
        }
    }

    fn add_active_voxel_to_backpack(&mut self, amount: u32) {
        self.add_voxel_to_backpack(self.player_tools.active_voxel_type, amount);
    }

    fn add_removed_voxels_to_backpack(&mut self, stats: &ChunkModifyStats) {
        for voxel_type in super::BACKPACK_VOXEL_TYPES {
            if let Some(voxel_type_id) = voxel_type.voxel_type() {
                self.add_voxel_to_backpack(voxel_type, stats.count_removed(voxel_type_id));
            }
        }
    }

    fn remove_voxel_from_backpack(&mut self, voxel_type: super::ActiveVoxelType, amount: u32) {
        match voxel_type {
            super::ActiveVoxelType::All => unreachable!("All is not a concrete backpack voxel"),
            super::ActiveVoxelType::Dirt => {
                self.player_tools.backpack_dirt_count =
                    self.player_tools.backpack_dirt_count.saturating_sub(amount)
            }
            super::ActiveVoxelType::Sand => {
                self.player_tools.backpack_sand_count =
                    self.player_tools.backpack_sand_count.saturating_sub(amount)
            }
            super::ActiveVoxelType::CherryWood => {
                self.player_tools.backpack_cherry_wood_count = self
                    .player_tools
                    .backpack_cherry_wood_count
                    .saturating_sub(amount)
            }
            super::ActiveVoxelType::OakWood => {
                self.player_tools.backpack_oak_wood_count = self
                    .player_tools
                    .backpack_oak_wood_count
                    .saturating_sub(amount)
            }
            super::ActiveVoxelType::Rock => {
                self.player_tools.backpack_rock_count =
                    self.player_tools.backpack_rock_count.saturating_sub(amount)
            }
        }
    }

    fn first_placeable_voxel_type(&self) -> Option<super::ActiveVoxelType> {
        if self.player_tools.active_voxel_type != super::ActiveVoxelType::All {
            return (self.active_voxel_count() > 0).then_some(self.player_tools.active_voxel_type);
        }

        super::BACKPACK_VOXEL_TYPES
            .iter()
            .copied()
            .find(|voxel_type| self.voxel_count(*voxel_type) > 0)
    }

    pub(super) fn start_terrain_edit_loop_sound(&mut self, position: Vec3) {
        if let Some(uuid) = self.player_tools.terrain_edit_loop_sound {
            if self.player_tools.terrain_edit_loop_sound_muted {
                if let Err(err) = self
                    .spatial_sound_manager
                    .update_source_volume(uuid, super::TERRAIN_EDIT_LOOP_VOLUME_DB)
                {
                    log::error!("Failed to unmute terrain edit loop sound: {}", err);
                } else {
                    self.player_tools.terrain_edit_loop_sound_muted = false;
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
                self.player_tools.terrain_edit_loop_sound = Some(uuid);
                self.player_tools.terrain_edit_loop_sound_muted = false;
            }
            Err(err) => {
                log::error!("Failed to start terrain edit loop sound: {}", err);
            }
        }
    }

    pub(super) fn stop_terrain_edit_loop_sound(&mut self) {
        if self.player_tools.terrain_edit_loop_sound_muted {
            return;
        }

        if let Some(uuid) = self.player_tools.terrain_edit_loop_sound {
            if let Err(err) = self
                .spatial_sound_manager
                .update_source_volume(uuid, super::TERRAIN_EDIT_LOOP_MUTED_VOLUME_DB)
            {
                log::error!("Failed to mute terrain edit loop sound: {}", err);
            } else {
                self.player_tools.terrain_edit_loop_sound_muted = true;
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

        Ok(self
            .query_terrain_ray_cpu(origin, direction)
            .filter(|hit| (*hit - origin).length() <= max_distance))
    }

    pub(super) fn query_terrain_ray_cpu(&self, origin: Vec3, direction: Vec3) -> Option<Vec3> {
        self.contree_builder
            .query_terrain_ray_cpu(origin, direction)
    }

    pub(super) fn query_terrain_height_cpu(&self, pos_xz: Vec2) -> f32 {
        self.query_terrain_ray_cpu(Vec3::new(pos_xz.x, 10.0, pos_xz.y), Vec3::NEG_Y)
            .map(|hit| hit.y)
            .unwrap_or(0.0)
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
        let removed_voxel_limits = self.active_removed_voxel_limits();

        match self.query_camera_ray_terrain_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_dig) = self.player_tools.last_shovel_dig_time {
                    if now.duration_since(last_dig) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                if let Err(err) = self
                    .apply_surface_terrain_removal(
                        TerrainRemovalEdit {
                            center,
                            radius: self.player_tools.terrain_edit_radius,
                        },
                        self.active_voxel_type_id(),
                        Some(remaining_capacity),
                        Some(removed_voxel_limits),
                    )
                    .map(|readback| {
                        let removed_total: u32 = readback.stats.removed_counts.iter().sum();
                        if removed_total == 0 {
                            self.stop_terrain_edit_loop_sound();
                            return;
                        }

                        if let Some(active_voxel_type_id) = self.active_voxel_type_id() {
                            let harvested = readback.stats.count_removed(active_voxel_type_id);
                            self.add_active_voxel_to_backpack(harvested);
                        } else {
                            self.add_removed_voxels_to_backpack(&readback.stats);
                        }
                        self.spawn_terrain_harvest_particles(
                            center,
                            &readback.stats,
                            &readback.sampled_positions_world,
                        );
                    })
                {
                    log::error!("Failed to apply terrain removal: {}", err);
                    return;
                }
                self.player_tools.last_shovel_dig_time = Some(now);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.last_shovel_dig_time = Some(now);
            }
            Err(err) => {
                log::error!("Shovel carve attempt failed during terrain query: {}", err);
            }
        }
    }

    pub(super) fn try_terrain_smooth(&mut self, now: Instant) {
        if self.window_state.is_cursor_visible() || !self.is_smooth_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        match self.query_camera_ray_terrain_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_smooth) = self.player_tools.last_smooth_time {
                    if now.duration_since(last_smooth) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                if let Err(err) = self.apply_surface_terrain_smooth(
                    center,
                    self.player_tools.terrain_edit_radius,
                    super::TERRAIN_SMOOTH_STRENGTH,
                    super::TERRAIN_SMOOTH_MAX_DELTA,
                    super::TERRAIN_SMOOTH_DEADBAND,
                ) {
                    log::error!("Failed to apply terrain smoothing: {}", err);
                    return;
                }
                self.player_tools.last_smooth_time = Some(now);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.last_smooth_time = Some(now);
            }
            Err(err) => {
                log::error!(
                    "Terrain smoothing attempt failed during terrain query: {}",
                    err
                );
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

                if let Some(last_regen) = self.player_tools.last_staff_regen_time {
                    if now.duration_since(last_regen) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                if let Err(err) = self.apply_surface_flora_regeneration(TerrainRemovalEdit {
                    center,
                    radius: super::TERRAIN_EDIT_DEFAULT_RADIUS,
                }) {
                    log::error!("Failed to apply flora regeneration: {}", err);
                    return;
                }
                self.player_tools.last_staff_regen_time = Some(now);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.last_staff_regen_time = Some(now);
            }
            Err(err) => {
                log::error!(
                    "Staff regeneration attempt failed during terrain query: {}",
                    err
                );
            }
        }
    }

    pub(super) fn adjust_terrain_edit_radius(&mut self, scroll_lines: f32) {
        if scroll_lines.abs() <= f32::EPSILON {
            return;
        }

        let previous_radius = self.player_tools.terrain_edit_radius;
        self.player_tools.terrain_edit_radius = (self.player_tools.terrain_edit_radius
            + scroll_lines * super::TERRAIN_EDIT_RADIUS_SCROLL_STEP)
            .clamp(
                super::TERRAIN_EDIT_RADIUS_MIN,
                super::TERRAIN_EDIT_RADIUS_MAX,
            );

        if (self.player_tools.terrain_edit_radius - previous_radius).abs() > f32::EPSILON {
            self.play_item_panel_scroll_sound();
        }
    }

    pub(super) fn terrain_edit_hover_center(&mut self) -> Option<Vec3> {
        if self.window_state.is_cursor_visible() || !self.is_terrain_edit_radius_tool_selected() {
            return None;
        }

        match self.query_camera_ray_terrain_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(hit) => hit,
            Err(err) => {
                log::error!("Terrain edit preview query failed: {}", err);
                None
            }
        }
    }

    pub(super) fn try_shovel_place(&mut self, now: Instant) {
        if self.window_state.is_cursor_visible() || !self.is_shovel_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        let Some(place_voxel_type) = self.first_placeable_voxel_type() else {
            self.stop_terrain_edit_loop_sound();
            return;
        };

        let place_voxel_type_id = place_voxel_type
            .voxel_type()
            .expect("placeable voxel type should be concrete");
        let place_voxel_count = self.voxel_count(place_voxel_type);

        match self.query_camera_ray_terrain_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_place) = self.player_tools.last_shovel_place_time {
                    if now.duration_since(last_place) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                if let Err(err) = self
                    .apply_surface_terrain_placement(
                        TerrainRemovalEdit {
                            center,
                            radius: self.player_tools.terrain_edit_radius,
                        },
                        place_voxel_type_id,
                        place_voxel_count,
                    )
                    .map(|readback| {
                        self.remove_voxel_from_backpack(
                            place_voxel_type,
                            readback.stats.count_added(place_voxel_type_id),
                        );
                    })
                {
                    log::error!("Failed to apply terrain placement: {}", err);
                    return;
                }
                self.player_tools.last_shovel_place_time = Some(now);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.last_shovel_place_time = Some(now);
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

                if let Some(last_trim) = self.player_tools.last_hoe_trim_time {
                    if now.duration_since(last_trim) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                if let Err(err) = self.apply_flora_trim(TerrainRemovalEdit {
                    center,
                    radius: super::TERRAIN_EDIT_DEFAULT_RADIUS,
                }) {
                    log::error!("Failed to apply flora trim: {}", err);
                    return;
                }
                self.player_tools.last_hoe_trim_time = Some(now);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.last_hoe_trim_time = Some(now);
            }
            Err(err) => {
                log::error!("Hoe trim attempt failed during terrain query: {}", err);
            }
        }
    }

    pub(super) fn try_water_particle_spawn(&mut self) {
        if self.window_state.is_cursor_visible() || !self.is_water_tool_selected() {
            return;
        }

        match self.query_camera_ray_terrain_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                self.water_sim.request_debug_particle_spawn(
                    center,
                    super::WATER_DEBUG_SPAWN_COUNT,
                    super::WATER_DEBUG_SPAWN_RADIUS,
                );
            }
            Ok(None) => {}
            Err(err) => {
                log::error!("Water particle spawn failed during terrain query: {}", err);
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
