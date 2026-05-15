use super::ui_style::{
    HOE_SLOT_INDEX, MAX_VOXEL_STORAGE_PER_TYPE, SHOVEL_SLOT_INDEX, STAFF_SLOT_INDEX,
};
use super::App;
use crate::app::world_edits::TerrainRemovalEdit;
use crate::tracer::PlayerCollisionResult;
use glam::{Vec2, Vec3};
use std::time::Instant;
use winit::event::DeviceEvent;
use winit::event_loop::ActiveEventLoop;

impl App {
    const PLAYER_COLLISION_RAY_DISTANCE: f32 = 2.0;
    const PLAYER_COLLISION_RING_RAY_COUNT: usize = 32;
    const PLAYER_COLLISION_RAY_HALF_KERNEL_SIZE: i32 = 1;
    const PLAYER_COLLISION_RAY_OFFSET: f32 = 1.0 / 256.0;
    const PLAYER_COLLISION_GROUND_MISS_DISTANCE: f32 = 1.0e10;

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

    fn active_voxel_type_id(&self) -> Option<u32> {
        self.active_voxel_type.voxel_type()
    }

    fn voxel_count(&self, voxel_type: super::ActiveVoxelType) -> u32 {
        match voxel_type {
            super::ActiveVoxelType::All => super::BACKPACK_VOXEL_TYPES
                .iter()
                .map(|voxel_type| self.voxel_count(*voxel_type))
                .sum(),
            super::ActiveVoxelType::Dirt => self.backpack_dirt_count,
            super::ActiveVoxelType::Sand => self.backpack_sand_count,
            super::ActiveVoxelType::CherryWood => self.backpack_cherry_wood_count,
            super::ActiveVoxelType::OakWood => self.backpack_oak_wood_count,
            super::ActiveVoxelType::Rock => self.backpack_rock_count,
        }
    }

    fn active_voxel_count(&self) -> u32 {
        self.voxel_count(self.active_voxel_type)
    }

    fn is_active_voxel_storage_full(&self) -> bool {
        if self.active_voxel_type == super::ActiveVoxelType::All {
            return super::BACKPACK_VOXEL_TYPES
                .iter()
                .all(|voxel_type| self.voxel_count(*voxel_type) >= MAX_VOXEL_STORAGE_PER_TYPE);
        }

        self.active_voxel_count() >= MAX_VOXEL_STORAGE_PER_TYPE
    }

    fn active_voxel_storage_remaining(&self) -> u32 {
        if self.active_voxel_type == super::ActiveVoxelType::All {
            return super::BACKPACK_VOXEL_TYPES
                .iter()
                .map(|voxel_type| {
                    MAX_VOXEL_STORAGE_PER_TYPE.saturating_sub(self.voxel_count(*voxel_type))
                })
                .sum();
        }

        MAX_VOXEL_STORAGE_PER_TYPE.saturating_sub(self.active_voxel_count())
    }

    fn add_voxel_to_backpack(&mut self, voxel_type: super::ActiveVoxelType, amount: u32) {
        match voxel_type {
            super::ActiveVoxelType::All => unreachable!("All is not a concrete backpack voxel"),
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

    fn add_active_voxel_to_backpack(&mut self, amount: u32) {
        self.add_voxel_to_backpack(self.active_voxel_type, amount);
    }

    fn add_removed_voxels_to_backpack(&mut self, stats: &crate::builder::ChunkModifyStats) {
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

    fn first_placeable_voxel_type(&self) -> Option<super::ActiveVoxelType> {
        if self.active_voxel_type != super::ActiveVoxelType::All {
            return (self.active_voxel_count() > 0).then_some(self.active_voxel_type);
        }

        super::BACKPACK_VOXEL_TYPES
            .iter()
            .copied()
            .find(|voxel_type| self.voxel_count(*voxel_type) > 0)
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

        Ok(self
            .query_terrain_ray_cpu(origin, direction)
            .filter(|hit| (*hit - origin).length() <= max_distance))
    }

    pub(super) fn query_player_collision_cpu(&self) -> PlayerCollisionResult {
        let player_pos = self.tracer.camera_position();
        let camera_front = self.tracer.camera_front();
        let flattened_front = Vec3::new(camera_front.x, 0.0, camera_front.z).normalize_or_zero();
        let horizontal_front = if flattened_front.length_squared() <= f32::EPSILON {
            Vec3::Z
        } else {
            flattened_front
        };
        let right = horizontal_front.cross(Vec3::Y).normalize_or_zero();

        let kernel_radius = Self::PLAYER_COLLISION_RAY_HALF_KERNEL_SIZE;
        let mut weighted_sum = 0.0;
        let mut sum_of_weights = 0.0;
        let mut ceiling_distance = Self::PLAYER_COLLISION_RAY_DISTANCE;

        for x in -kernel_radius..=kernel_radius {
            for y in -kernel_radius..=kernel_radius {
                let offset = Vec2::new(x as f32, y as f32) * Self::PLAYER_COLLISION_RAY_OFFSET;
                let ray_origin = player_pos + Vec3::new(offset.x, 0.0, offset.y);

                let ground_distance = self
                    .query_terrain_ray_cpu(ray_origin, Vec3::NEG_Y)
                    .map(|hit| (hit - ray_origin).length())
                    .unwrap_or(Self::PLAYER_COLLISION_GROUND_MISS_DISTANCE);
                let weight = Self::player_collision_gaussian_weight(x, y);
                weighted_sum += ground_distance * weight;
                sum_of_weights += weight;

                let ceiling_hit_distance = self
                    .query_terrain_ray_cpu(ray_origin, Vec3::Y)
                    .map(|hit| (hit - ray_origin).length())
                    .unwrap_or(Self::PLAYER_COLLISION_RAY_DISTANCE)
                    .min(Self::PLAYER_COLLISION_RAY_DISTANCE);
                ceiling_distance = ceiling_distance.min(ceiling_hit_distance);
            }
        }

        let mut ring_distances = Vec::with_capacity(Self::PLAYER_COLLISION_RING_RAY_COUNT);
        ring_distances.push(
            self.query_player_collision_ring_distance(player_pos, horizontal_front)
                .min(Self::PLAYER_COLLISION_RAY_DISTANCE),
        );

        for ring_index in 1..Self::PLAYER_COLLISION_RING_RAY_COUNT {
            let angle = 2.0 * std::f32::consts::PI * (ring_index - 1) as f32
                / (Self::PLAYER_COLLISION_RING_RAY_COUNT - 1) as f32;
            let direction =
                (horizontal_front * angle.cos() + right * angle.sin()).normalize_or_zero();
            let direction = if direction.length_squared() <= f32::EPSILON {
                horizontal_front
            } else {
                direction
            };
            ring_distances.push(
                self.query_player_collision_ring_distance(player_pos, direction)
                    .min(Self::PLAYER_COLLISION_RAY_DISTANCE),
            );
        }

        PlayerCollisionResult {
            ground_distance: weighted_sum / sum_of_weights.max(1e-8),
            ceiling_distance,
            ring_distances,
        }
    }

    fn query_player_collision_ring_distance(&self, origin: Vec3, direction: Vec3) -> f32 {
        self.query_terrain_ray_cpu(origin, direction)
            .map(|hit| (hit - origin).length())
            .unwrap_or(Self::PLAYER_COLLISION_RAY_DISTANCE)
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

    fn player_collision_gaussian_weight(x: i32, y: i32) -> f32 {
        let sigma = Self::PLAYER_COLLISION_RAY_HALF_KERNEL_SIZE as f32 * 0.5 + 0.5;
        let sigma2 = sigma * sigma;
        (-(x * x + y * y) as f32 / (2.0 * sigma2)).exp()
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
                        self.active_voxel_type_id(),
                        Some(remaining_capacity),
                    )
                    .map(|readback| {
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
