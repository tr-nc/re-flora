use super::placeables::{PipeRayHit, PlaceableKind, SprinklerPlacementTarget};
use super::player_tools::{ContinuousTerrainToolAction, PlayerTool, PlayerToolSelectionUpdate};
use super::App;
use crate::app::terrain_edit_bounds::INITIAL_EDITABLE_TERRAIN_BOUNDS;
use crate::app::world_edits::{
    TerrainBrushEdit, TerrainRemovalEdit, TreeAddOptions, TreePlacement,
};
use crate::flora::species;
use crate::tracer::TerrainEditPreviewShape;
use glam::{Vec2, Vec3};
use std::time::{Duration, Instant};
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::KeyCode;

fn scroll_delta_lines(delta: MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 120.0,
    }
}

fn terrain_edit_endpoint_within_editable_chunk(center: Vec3) -> bool {
    INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_point_xz(center)
}

fn terrain_brush_endpoint_within_editable_chunk(edit: TerrainBrushEdit) -> bool {
    INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_brush_endpoint(edit)
}

fn select_sprinkler_placement_target(
    terrain_hit: Option<(f32, Vec3)>,
    pipe_hit: Option<PipeRayHit>,
) -> Option<SprinklerPlacementTarget> {
    match (terrain_hit, pipe_hit) {
        (Some((terrain_distance, _)), Some(pipe_hit)) if pipe_hit.distance <= terrain_distance => {
            Some(SprinklerPlacementTarget::Pipe(pipe_hit.attachment))
        }
        (Some((_, terrain_position)), _) => {
            Some(SprinklerPlacementTarget::Terrain(terrain_position))
        }
        (None, Some(pipe_hit)) => Some(SprinklerPlacementTarget::Pipe(pipe_hit.attachment)),
        (None, None) => None,
    }
}

const TERRAIN_EDIT_PREVIEW_VALID_COLOR: Vec3 = Vec3::new(0.45, 0.86, 1.0);
const TERRAIN_EDIT_PREVIEW_INVALID_COLOR: Vec3 = Vec3::new(1.0, 0.08, 0.06);

#[derive(Clone, Copy, Debug)]
pub(super) struct TerrainEditHover {
    pub(super) center: Vec3,
    pub(super) is_editable: bool,
}

impl App {
    fn blocking_panel_open(&self) -> bool {
        self.config_panel_visible || self.card_display_visible
    }

    pub(super) fn is_free_look_camera_mode(&self) -> bool {
        self.camera_control.is_free_look()
    }

    pub(super) fn is_free_fly_camera_mode(&self) -> bool {
        self.camera_control.is_free_fly()
    }

    pub(super) fn is_walk_camera_mode(&self) -> bool {
        self.camera_control.is_walk()
    }

    pub(super) fn is_orbit_edit_camera_mode(&self) -> bool {
        self.camera_control.is_orbit_edit()
    }

    pub(super) fn keyboard_tool_shortcuts_available(&self) -> bool {
        !self.blocking_panel_open()
    }

    pub(super) fn terrain_edit_pointer_available(&self) -> bool {
        !self.blocking_panel_open()
            && self.launch_owners.glass_experiment_settings().is_none()
            && (!self.window_state.is_cursor_visible() || self.is_orbit_edit_camera_mode())
    }

    pub(super) fn reset_camera_movement_input(&mut self) {
        self.tracer.reset_camera_input();
        self.camera_control.reset_motion();
    }

    fn window_center_physical(&self) -> Vec2 {
        let extent = self.window_state.window_extent();
        Vec2::new(extent.width as f32 * 0.5, extent.height as f32 * 0.5)
    }

    fn center_logical_cursor(&mut self) {
        let center = self.window_center_physical();
        self.cursor_position_physical = Some(center);
        self.camera_control.sync_orbit_drag_position(center);
        let _ = self.window_state.center_cursor();
    }

    pub(super) fn sync_cursor_with_panels(&mut self) {
        let was_cursor_visible = self.window_state.is_cursor_visible();
        let cursor_visible = self.blocking_panel_open() || self.is_orbit_edit_camera_mode();

        if cursor_visible && !was_cursor_visible {
            // Wayland rejects cursor warps after the pointer is unlocked, so center while still
            // locked and only then release the cursor for visible UI/orbit modes.
            self.center_logical_cursor();
        }

        self.window_state.set_cursor_grab(!cursor_visible);

        if self.blocking_panel_open() {
            self.player_tools.cancel_continuous_hold();
            self.stop_terrain_edit_loop_sound();
            self.reset_camera_movement_input();
        }
    }

    pub(super) fn toggle_camera_control_mode(&mut self) {
        let entered_orbit_edit = self.camera_control.cycle_mode();
        self.player_tools.cancel_continuous_hold();
        self.stop_terrain_edit_loop_sound();
        self.camera_control.reset_mode_transition_motion();
        self.tracer.reset_camera_velocity();

        if entered_orbit_edit {
            self.sync_orbit_focus_from_current_view();
        }
        self.sync_cursor_with_panels();
    }

    fn current_view_center_ray(&self) -> Option<(Vec3, Vec3)> {
        self.screen_center_camera_ray().or_else(|| {
            let origin = self.tracer.camera_position();
            let direction = self.tracer.camera_front();
            (origin.is_finite()
                && direction.is_finite()
                && direction.length_squared() > f32::EPSILON)
                .then_some((origin, direction))
        })
    }

    fn sync_orbit_focus_from_current_view(&mut self) {
        let Some((origin, direction)) = self.current_view_center_ray() else {
            return;
        };

        let terrain_hit = self
            .query_terrain_ray_cpu(origin, direction)
            .map(|hit| hit.position);
        self.camera_control
            .sync_focus_from_view_ray(origin, direction, terrain_hit);
    }

    fn look_at_orbit_focus_from_current_position(&mut self) {
        let (position, focus) = self
            .camera_control
            .focus_look_at_pose(self.tracer.camera_position(), self.tracer.camera_front());
        self.tracer.set_camera_pose_looking_at(position, focus);
    }

    pub(super) fn update_camera_for_current_mode(
        &mut self,
        frame_delta_time: f32,
        sim_time_seconds: f64,
    ) -> Vec<crate::gameplay::camera::FootstepEvent> {
        if self.is_free_fly_camera_mode() {
            self.tracer.update_fly_camera(frame_delta_time);
        } else if self.is_walk_camera_mode() {
            if frame_delta_time > f32::EPSILON && frame_delta_time.is_finite() {
                let request = self
                    .tracer
                    .prepare_walk_camera_movement(frame_delta_time, sim_time_seconds);
                let result = self
                    .terrain_physics
                    .move_player_capsule(request, frame_delta_time)
                    .unwrap_or_else(|err| {
                        log::error!("Failed to move player capsule: {err:#}");
                        crate::gameplay::camera::PlayerWalkMovementResult::BLOCKED
                    });
                self.tracer.apply_walk_camera_movement(
                    frame_delta_time,
                    sim_time_seconds,
                    request,
                    result,
                );
            }
        } else {
            self.queue_orbit_keyboard_camera_pan(frame_delta_time);
            self.update_orbit_camera_motion(frame_delta_time);
        }
        self.update_mouse_wheel_camera_dolly(frame_delta_time);
        self.tracer.take_footstep_events()
    }

    fn orbit_camera_spherical(&self) -> (f32, f32, f32) {
        self.camera_control
            .orbit_spherical(self.tracer.camera_position())
    }

    fn apply_orbit_camera_spherical(&mut self, azimuth: f32, elevation: f32, distance: f32) {
        let (position, focus) = self.camera_control.orbit_pose(azimuth, elevation, distance);
        self.tracer.set_camera_pose_looking_at(position, focus);
    }

    pub(super) fn handle_orbit_keyboard_camera_input(
        &mut self,
        code: KeyCode,
        state: ElementState,
    ) {
        self.camera_control.handle_orbit_keyboard_input(code, state);
    }

    fn queue_orbit_keyboard_camera_pan(&mut self, frame_delta_time: f32) {
        let available = self.orbit_mouse_drag_available();
        let camera_front = self.tracer.camera_front();
        let camera_position = self.tracer.camera_position();
        self.camera_control.queue_orbit_keyboard_pan(
            camera_front,
            camera_position,
            frame_delta_time,
            available,
        );
    }

    fn orbit_mouse_drag_available(&self) -> bool {
        self.is_orbit_edit_camera_mode() && !self.blocking_panel_open()
    }

    fn update_orbit_camera_motion(&mut self, frame_delta_time: f32) {
        let available = self.orbit_mouse_drag_available();
        let motion = self
            .camera_control
            .advance_orbit_motion(frame_delta_time, available);
        if motion.pan_delta.length_squared() > f32::EPSILON {
            self.translate_orbit_camera(motion.pan_delta);
        }

        if motion.rotation_delta.length_squared() > f32::EPSILON {
            let (azimuth, elevation, distance) = self.orbit_camera_spherical();
            self.apply_orbit_camera_spherical(
                azimuth + motion.rotation_delta.x,
                elevation + motion.rotation_delta.y,
                distance,
            );
        }
    }

    fn screen_center_camera_ray(&self) -> Option<(Vec3, Vec3)> {
        let extent = self.window_state.window_extent();
        let center = Vec2::new(extent.width as f32 * 0.5, extent.height as f32 * 0.5);
        self.tracer.camera_ray_from_screen_position(center, extent)
    }

    fn acquire_orbit_focus_from_screen_center(&mut self) {
        let Some((origin, direction)) = self.screen_center_camera_ray() else {
            return;
        };
        let terrain_hit = self
            .query_terrain_ray_cpu(origin, direction)
            .map(|hit| hit.position);
        if self
            .camera_control
            .acquire_focus_from_terrain_hit(origin, direction, terrain_hit)
        {
            self.look_at_orbit_focus_from_current_position();
        }
    }

    pub(super) fn set_orbit_mouse_drag_state(
        &mut self,
        button: MouseButton,
        state: ElementState,
    ) -> bool {
        match state {
            ElementState::Pressed
                if self.orbit_mouse_drag_available() && button == MouseButton::Middle =>
            {
                self.stop_terrain_edit_loop_sound();
                self.camera_control
                    .begin_orbit_pan_drag(button, self.cursor_position_physical);
                true
            }
            ElementState::Pressed
                if self.orbit_mouse_drag_available() && button == MouseButton::Right =>
            {
                self.player_tools
                    .set_pointer_button_state(MouseButton::Right, ElementState::Released);
                self.stop_terrain_edit_loop_sound();
                self.acquire_orbit_focus_from_screen_center();
                self.camera_control
                    .begin_orbit_rotation_drag(button, self.cursor_position_physical);
                true
            }
            ElementState::Released => self.camera_control.end_orbit_drag(button),
            _ => false,
        }
    }

    pub(super) fn sync_orbit_mouse_drag_position(&mut self, position_physical: Vec2) {
        self.camera_control
            .sync_orbit_drag_position(position_physical);
    }

    pub(super) fn handle_orbit_mouse_drag(&mut self, position_physical: Vec2) {
        let (_, elevation, _) = self.orbit_camera_spherical();
        let available = self.orbit_mouse_drag_available();
        let camera_front = self.tracer.camera_front();
        self.camera_control.handle_orbit_drag(
            position_physical,
            available,
            camera_front,
            elevation,
        );
    }

    fn translate_orbit_camera(&mut self, delta: Vec3) {
        let new_focus = self.camera_control.translate_orbit_focus(delta);
        let new_position = self.tracer.camera_position() + delta;
        self.tracer
            .set_camera_pose_looking_at(new_position, new_focus);
    }

    pub(super) fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let scroll_lines = scroll_delta_lines(delta);
        if self.modifiers.shift_key() {
            if self.terrain_edit_pointer_available() && self.is_terrain_edit_radius_tool_selected()
            {
                self.adjust_terrain_edit_radius(scroll_lines);
            }
            return;
        }

        if self.camera_scroll_available() {
            self.camera_control.queue_mouse_wheel_dolly(scroll_lines);
        }
    }

    fn camera_scroll_available(&self) -> bool {
        self.is_orbit_edit_camera_mode() && !self.blocking_panel_open()
    }

    fn update_mouse_wheel_camera_dolly(&mut self, frame_delta_time: f32) {
        let available = self.camera_scroll_available();
        let camera_position = self.tracer.camera_position();
        let Some((position, focus)) =
            self.camera_control
                .advance_orbit_dolly(frame_delta_time, available, camera_position)
        else {
            return;
        };
        self.tracer.set_camera_pose_looking_at(position, focus);
    }

    pub(super) fn set_tool_mouse_button_state(&mut self, button: MouseButton, state: ElementState) {
        self.player_tools.set_pointer_button_state(button, state);
    }

    pub(super) fn refresh_terrain_edit_hold_from_mouse_buttons(&mut self) {
        if !self.player_tools.finish_pointer_release() {
            self.stop_terrain_edit_loop_sound();
            if let Err(err) = self.finish_player_terrain_connectivity_hold() {
                log::error!("Failed to resolve detached terrain after edit release: {err:#}");
            }
        }
    }

    pub(super) fn execute_continuous_terrain_tool_action(
        &mut self,
        action: ContinuousTerrainToolAction,
        now: Instant,
    ) {
        match action {
            ContinuousTerrainToolAction::ShovelDig => self.try_shovel_dig(now),
            ContinuousTerrainToolAction::ShovelPlace => self.try_shovel_place(now),
            ContinuousTerrainToolAction::Smooth => self.try_terrain_smooth(now),
            ContinuousTerrainToolAction::StaffRegenerate => self.try_staff_regenerate(now),
            ContinuousTerrainToolAction::StaffRemove => self.try_staff_remove_flora(now),
            ContinuousTerrainToolAction::HoeTrim => self.try_hoe_trim(now),
            ContinuousTerrainToolAction::Water => self.try_watering_brush(now),
            ContinuousTerrainToolAction::Fertilize => self.try_fertilizer_brush(now),
            ContinuousTerrainToolAction::Till => self.try_tiller_brush(now),
        }
    }

    fn apply_player_tool_selection_update(&mut self, update: PlayerToolSelectionUpdate) {
        if !update.changed() {
            return;
        }
        if update.cancel_placeable_interaction() {
            self.cancel_pipe_drag();
        }
        if update.active_tool_changed() {
            self.stop_terrain_edit_loop_sound();
        }
        self.play_item_panel_scroll_sound();
    }

    pub(super) fn select_item_panel_slot(&mut self, slot_idx: usize) {
        let update = self.player_tools.select_item_panel_slot(slot_idx);
        self.apply_player_tool_selection_update(update);
    }

    pub(super) fn select_placeable_tool(&mut self, slot_idx: usize) {
        let update = self.player_tools.select_placeable_tool(slot_idx);
        self.apply_player_tool_selection_update(update);
    }

    pub(super) fn current_flora_paint_selection(&self) -> species::FloraPaintSelection {
        let selections = species::PLAYER_FLORA_PAINT_SELECTIONS;
        let selection_idx = self.player_tools.flora_paint_selection_index % selections.len();
        selections[selection_idx]
    }

    pub(super) fn current_flora_paint_selection_label(&self) -> &'static str {
        species::flora_paint_selection_label(self.current_flora_paint_selection())
    }

    pub(super) fn current_flora_paint_dab_interval(&self) -> Duration {
        Duration::from_millis(
            species::flora_paint_brush_settings(self.current_flora_paint_selection())
                .dab_interval_ms,
        )
    }

    fn current_flora_paint_release_interval(&self) -> Duration {
        Duration::from_millis(
            species::flora_paint_brush_settings(self.current_flora_paint_selection())
                .release_interval_ms,
        )
    }

    fn current_staff_regen_paint_dab_serial(&mut self, now: Instant) -> (u32, bool) {
        let paint_brush = species::flora_paint_brush_settings(self.current_flora_paint_selection());
        let release_interval = self.current_flora_paint_release_interval();
        let spaced_releases =
            paint_brush.soft_spacing_voxels > 0 && paint_brush.plants_per_release > 0;
        self.player_tools
            .flora_paint_dab(now, release_interval, spaced_releases)
    }

    pub(super) fn select_flora_paint_selection_index(&mut self, selection_idx: usize) {
        let selection_count = species::PLAYER_FLORA_PAINT_SELECTIONS.len();
        if selection_idx >= selection_count {
            return;
        }

        let current_selection_idx = self.player_tools.flora_paint_selection_index % selection_count;
        if selection_idx == current_selection_idx {
            self.player_tools.flora_paint_selection_index = selection_idx;
            return;
        }

        self.player_tools.flora_paint_selection_index = selection_idx;
        self.player_tools
            .restart_stroke(ContinuousTerrainToolAction::StaffRegenerate);
        self.play_item_panel_scroll_sound();
        log::info!(
            "Grow brush flora selection: {}",
            self.current_flora_paint_selection_label()
        );
    }

    pub(super) fn cycle_flora_paint_selection(&mut self) {
        let selection_count = species::PLAYER_FLORA_PAINT_SELECTIONS.len();
        if selection_count == 0 {
            return;
        }

        let current_selection_idx = self.player_tools.flora_paint_selection_index % selection_count;
        self.select_flora_paint_selection_index((current_selection_idx + 1) % selection_count);
    }

    pub(super) fn is_shovel_selected(&self) -> bool {
        self.player_tools.selected_tool() == PlayerTool::Shovel
    }

    pub(super) fn is_smooth_selected(&self) -> bool {
        self.player_tools.selected_tool() == PlayerTool::Smooth
    }

    pub(super) fn is_terrain_edit_radius_tool_selected(&self) -> bool {
        self.player_tools.selected_tool().uses_terrain_edit_radius()
    }

    pub(super) fn terrain_edit_preview_shape(&self) -> TerrainEditPreviewShape {
        if self.is_place_tool_selected() {
            match self.current_placeable_kind() {
                PlaceableKind::Tree | PlaceableKind::Sprinkler | PlaceableKind::Pipe => {
                    TerrainEditPreviewShape::SurfaceCircle
                }
            }
        } else {
            TerrainEditPreviewShape::Sphere
        }
    }

    pub(super) fn terrain_edit_preview_color(&self, is_editable: bool) -> Vec3 {
        if is_editable {
            TERRAIN_EDIT_PREVIEW_VALID_COLOR
        } else {
            TERRAIN_EDIT_PREVIEW_INVALID_COLOR
        }
    }

    pub(super) fn is_staff_selected(&self) -> bool {
        self.player_tools.selected_tool() == PlayerTool::Staff
    }

    pub(super) fn is_hoe_selected(&self) -> bool {
        self.player_tools.selected_tool() == PlayerTool::Hoe
    }

    pub(super) fn is_place_tool_selected(&self) -> bool {
        self.player_tools.selected_tool() == PlayerTool::Placeable
    }

    pub(super) fn is_watering_selected(&self) -> bool {
        self.player_tools.selected_tool() == PlayerTool::Watering
    }

    pub(super) fn is_soil_inspector_selected(&self) -> bool {
        self.player_tools.selected_tool() == PlayerTool::SoilInspector
    }

    pub(super) fn is_fertilizer_selected(&self) -> bool {
        self.player_tools.selected_tool() == PlayerTool::Fertilizer
    }

    pub(super) fn is_tiller_selected(&self) -> bool {
        self.player_tools.selected_tool() == PlayerTool::Tiller
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

    fn terrain_edit_ray(&self) -> Option<(Vec3, Vec3)> {
        if self.is_orbit_edit_camera_mode() {
            let extent = self.window_state.window_extent();
            let cursor_pos = self.cursor_position_physical.unwrap_or_else(|| {
                Vec2::new(extent.width as f32 * 0.5, extent.height as f32 * 0.5)
            });
            self.tracer
                .camera_ray_from_screen_position(cursor_pos, extent)
        } else {
            Some((self.tracer.camera_position(), self.tracer.camera_front()))
        }
    }

    fn query_terrain_edit_ray_intersection(
        &mut self,
        max_distance: f32,
    ) -> anyhow::Result<Option<Vec3>> {
        if max_distance <= 0.0 {
            return Ok(None);
        }

        let Some((origin, direction)) = self.terrain_edit_ray() else {
            return Ok(None);
        };
        if direction.length_squared() <= f32::EPSILON {
            return Ok(None);
        }

        Ok(self
            .query_terrain_ray_cpu(origin, direction)
            .map(|hit| hit.position)
            .filter(|hit| (*hit - origin).length() <= max_distance))
    }

    fn query_sprinkler_placement_target(
        &self,
        max_distance: f32,
    ) -> Option<SprinklerPlacementTarget> {
        if max_distance <= 0.0 {
            return None;
        }
        let (origin, direction) = self.terrain_edit_ray()?;
        let direction = direction.normalize_or_zero();
        if direction == Vec3::ZERO {
            return None;
        }

        let terrain_hit = self
            .query_terrain_ray_cpu(origin, direction)
            .map(|hit| ((hit.position - origin).length(), hit.position))
            .filter(|(distance, _)| *distance <= max_distance);
        let pipe_hit = self
            .irrigation_network
            .ray_attachment(origin, direction, max_distance);

        select_sprinkler_placement_target(terrain_hit, pipe_hit)
    }

    pub(super) fn query_terrain_ray_cpu(
        &self,
        origin: Vec3,
        direction: Vec3,
    ) -> Option<crate::builder::ContreeCpuRayHit> {
        self.contree_builder
            .query_terrain_ray_cpu(origin, direction)
    }

    pub(super) fn query_terrain_height_cpu(&self, pos_xz: Vec2) -> f32 {
        self.query_terrain_ray_cpu(Vec3::new(pos_xz.x, 10.0, pos_xz.y), Vec3::NEG_Y)
            .map(|hit| hit.position.y)
            .unwrap_or(0.0)
    }

    pub(super) fn try_shovel_dig(&mut self, now: Instant) {
        if !self.terrain_edit_pointer_available() || !self.is_shovel_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }
        let action = ContinuousTerrainToolAction::ShovelDig;

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if !self
                    .player_tools
                    .stroke_ready(action, now, super::SHOVEL_DIG_INTERVAL)
                {
                    return;
                }

                if let Err(err) = self
                    .apply_surface_terrain_removal(
                        TerrainRemovalEdit {
                            center,
                            radius: self.player_tools.terrain_edit_radius,
                        },
                        // Backpack material selection is status-only, so removal accepts every
                        // concrete voxel type.
                        None,
                        None,
                        None,
                    )
                    .map(|readback| {
                        let removed_total: u32 = readback.stats.removed_counts.iter().sum();
                        if removed_total == 0 {
                            self.stop_terrain_edit_loop_sound();
                            return;
                        }

                        let material_mode = self.voxel_material_mode();
                        self.voxel_backpack
                            .deposit_removed(&readback.stats, material_mode);
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
                self.player_tools.record_stroke_dab(action, now, center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.defer_stroke(action, now);
            }
            Err(err) => {
                log::error!("Shovel carve attempt failed during terrain query: {}", err);
            }
        }
    }

    pub(super) fn try_terrain_smooth(&mut self, now: Instant) {
        if !self.terrain_edit_pointer_available() || !self.is_smooth_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }
        let action = ContinuousTerrainToolAction::Smooth;

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if !self
                    .player_tools
                    .stroke_ready(action, now, super::SHOVEL_DIG_INTERVAL)
                {
                    return;
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
                self.player_tools.record_stroke_dab(action, now, center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.defer_stroke(action, now);
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
        let action = ContinuousTerrainToolAction::StaffRegenerate;
        if !self.terrain_edit_pointer_available() || !self.is_staff_selected() {
            self.stop_terrain_edit_loop_sound();
            self.player_tools.interrupt_stroke(action);
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                let dab_interval = self.current_flora_paint_dab_interval();
                if !self.player_tools.stroke_ready(action, now, dab_interval) {
                    return;
                }

                let edit = TerrainBrushEdit::from_previous_center(
                    self.player_tools.previous_stroke_center(action),
                    center,
                    self.player_tools.terrain_edit_radius,
                );
                if !terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                let (paint_dab_serial, is_release_step) =
                    self.current_staff_regen_paint_dab_serial(now);
                if let Err(err) =
                    self.apply_surface_flora_regeneration(edit, paint_dab_serial, is_release_step)
                {
                    log::error!("Failed to apply flora regeneration: {}", err);
                    self.player_tools.interrupt_stroke(action);
                    return;
                }
                self.player_tools.record_stroke_dab(action, now, center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.defer_stroke(action, now);
            }
            Err(err) => {
                self.player_tools.interrupt_stroke(action);
                log::error!(
                    "Staff regeneration attempt failed during terrain query: {}",
                    err
                );
            }
        }
    }

    pub(super) fn try_staff_remove_flora(&mut self, now: Instant) {
        let action = ContinuousTerrainToolAction::StaffRemove;
        if !self.terrain_edit_pointer_available() || !self.is_staff_selected() {
            self.stop_terrain_edit_loop_sound();
            self.player_tools.interrupt_stroke(action);
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if !self
                    .player_tools
                    .stroke_ready(action, now, super::SHOVEL_DIG_INTERVAL)
                {
                    return;
                }

                let edit = TerrainBrushEdit::from_previous_center(
                    self.player_tools.previous_stroke_center(action),
                    center,
                    self.player_tools.terrain_edit_radius,
                );
                if !terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                if let Err(err) = self.apply_surface_flora_removal(edit) {
                    log::error!("Failed to apply flora removal: {}", err);
                    self.player_tools.interrupt_stroke(action);
                    return;
                }
                self.player_tools.record_stroke_dab(action, now, center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.defer_stroke(action, now);
            }
            Err(err) => {
                self.player_tools.interrupt_stroke(action);
                log::error!(
                    "Staff flora removal attempt failed during terrain query: {}",
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

    pub(super) fn terrain_edit_hover(&mut self) -> Option<TerrainEditHover> {
        if !self.terrain_edit_pointer_available() || !self.is_terrain_edit_radius_tool_selected() {
            return None;
        }

        if self.is_place_tool_selected()
            && self.current_placeable_kind() == PlaceableKind::Sprinkler
        {
            return self
                .query_sprinkler_placement_target(super::SHOVEL_RAY_QUERY_DISTANCE)
                .map(|target| {
                    let center = target.position();
                    TerrainEditHover {
                        center,
                        is_editable: terrain_edit_endpoint_within_editable_chunk(center),
                    }
                });
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(hit) => hit.map(|center| TerrainEditHover {
                center,
                is_editable: self.terrain_edit_preview_position_is_editable(center),
            }),
            Err(err) => {
                log::error!("Terrain edit preview query failed: {}", err);
                None
            }
        }
    }

    fn terrain_edit_preview_position_is_editable(&self, center: Vec3) -> bool {
        terrain_edit_endpoint_within_editable_chunk(center)
    }

    pub(super) fn try_shovel_place(&mut self, now: Instant) {
        if !self.terrain_edit_pointer_available() || !self.is_shovel_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }
        let action = ContinuousTerrainToolAction::ShovelPlace;

        // Placement ignores material selection and uses the first stored voxel type.
        let Some((place_voxel, place_voxel_count)) = self.voxel_backpack.first_available() else {
            self.stop_terrain_edit_loop_sound();
            return;
        };

        let place_voxel_type_id = place_voxel.voxel_type();

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if !self
                    .player_tools
                    .stroke_ready(action, now, super::SHOVEL_DIG_INTERVAL)
                {
                    return;
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
                        self.voxel_backpack
                            .withdraw(place_voxel, readback.stats.count_added(place_voxel_type_id));
                    })
                {
                    log::error!("Failed to apply terrain placement: {}", err);
                    return;
                }
                self.player_tools.record_stroke_dab(action, now, center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.defer_stroke(action, now);
            }
            Err(err) => {
                log::error!("Shovel place attempt failed during terrain query: {}", err);
            }
        }
    }

    pub(super) fn try_hoe_trim(&mut self, now: Instant) {
        if !self.terrain_edit_pointer_available() || !self.is_hoe_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }
        let action = ContinuousTerrainToolAction::HoeTrim;

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if !self
                    .player_tools
                    .stroke_ready(action, now, super::SHOVEL_DIG_INTERVAL)
                {
                    return;
                }

                if let Err(err) = self.apply_flora_trim(TerrainRemovalEdit {
                    center,
                    radius: self.player_tools.terrain_edit_radius,
                }) {
                    log::error!("Failed to apply flora trim: {}", err);
                    return;
                }
                self.player_tools.record_stroke_dab(action, now, center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.defer_stroke(action, now);
            }
            Err(err) => {
                log::error!("Hoe trim attempt failed during terrain query: {}", err);
            }
        }
    }

    pub(super) fn try_watering_brush(&mut self, now: Instant) {
        let action = ContinuousTerrainToolAction::Water;
        if !self.terrain_edit_pointer_available() || !self.is_watering_selected() {
            self.stop_terrain_edit_loop_sound();
            self.player_tools.interrupt_stroke(action);
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if !self
                    .player_tools
                    .stroke_ready(action, now, super::SHOVEL_DIG_INTERVAL)
                {
                    return;
                }

                let edit = TerrainBrushEdit::from_previous_center(
                    self.player_tools.previous_stroke_center(action),
                    center,
                    self.player_tools.terrain_edit_radius,
                );
                if !terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                if let Err(err) = self.add_watering_brush_moisture(edit) {
                    log::error!("Failed to apply watering brush: {}", err);
                    self.player_tools.interrupt_stroke(action);
                    return;
                }
                self.player_tools.record_stroke_dab(action, now, center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.defer_stroke(action, now);
            }
            Err(err) => {
                self.player_tools.interrupt_stroke(action);
                log::error!(
                    "Watering brush attempt failed during terrain query: {}",
                    err
                );
            }
        }
    }

    pub(super) fn try_fertilizer_brush(&mut self, now: Instant) {
        let action = ContinuousTerrainToolAction::Fertilize;
        if !self.terrain_edit_pointer_available() || !self.is_fertilizer_selected() {
            self.stop_terrain_edit_loop_sound();
            self.player_tools.interrupt_stroke(action);
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if !self
                    .player_tools
                    .stroke_ready(action, now, super::SHOVEL_DIG_INTERVAL)
                {
                    return;
                }

                let edit = TerrainBrushEdit::from_previous_center(
                    self.player_tools.previous_stroke_center(action),
                    center,
                    self.player_tools.terrain_edit_radius,
                );
                if !terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                let stroke_seed = self.player_tools.fertilizer_stroke_seed();
                if let Err(err) = self.add_fertilizer_brush_fertility(edit, stroke_seed) {
                    log::error!("Failed to apply fertilizer brush: {}", err);
                    self.player_tools.interrupt_stroke(action);
                    return;
                }
                self.player_tools.record_stroke_dab(action, now, center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.defer_stroke(action, now);
            }
            Err(err) => {
                self.player_tools.interrupt_stroke(action);
                log::error!(
                    "Fertilizer brush attempt failed during terrain query: {}",
                    err
                );
            }
        }
    }

    pub(super) fn try_tiller_brush(&mut self, now: Instant) {
        let action = ContinuousTerrainToolAction::Till;
        if !self.terrain_edit_pointer_available() || !self.is_tiller_selected() {
            self.stop_terrain_edit_loop_sound();
            self.player_tools.interrupt_stroke(action);
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if !self
                    .player_tools
                    .stroke_ready(action, now, super::SHOVEL_DIG_INTERVAL)
                {
                    return;
                }

                let edit = TerrainBrushEdit::from_previous_center(
                    self.player_tools.previous_stroke_center(action),
                    center,
                    self.player_tools.terrain_edit_radius,
                );
                if !terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                if let Err(err) = self.mix_tiller_brush_soil(edit) {
                    log::error!("Failed to apply tiller brush: {}", err);
                    self.player_tools.interrupt_stroke(action);
                    return;
                }
                self.player_tools.record_stroke_dab(action, now, center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.defer_stroke(action, now);
            }
            Err(err) => {
                self.player_tools.interrupt_stroke(action);
                log::error!("Tiller brush attempt failed during terrain query: {}", err);
            }
        }
    }

    pub(super) fn try_update_pipe_drag_preview(&mut self) {
        if !self.irrigation_network.route_active() {
            return;
        }
        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) if terrain_edit_endpoint_within_editable_chunk(center) => {
                if let Err(err) = self.update_pipe_drag_preview(center) {
                    log::error!("Failed to update irrigation pipe preview: {err}");
                }
            }
            Ok(_) => {}
            Err(err) => log::error!("Pipe placement preview query failed: {err}"),
        }
    }

    pub(super) fn try_finish_pipe_drag(&mut self) {
        if !self.irrigation_network.route_active() {
            return;
        }
        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) if terrain_edit_endpoint_within_editable_chunk(center) => {
                if let Err(err) = self.finish_pipe_drag(center) {
                    log::error!("Failed to place irrigation pipe: {err}");
                }
            }
            Ok(_) => self.cancel_pipe_drag(),
            Err(err) => {
                self.cancel_pipe_drag();
                log::error!("Pipe placement release query failed: {err}");
            }
        }
    }

    pub(super) fn try_placeable_placement(&mut self) {
        if !self.terrain_edit_pointer_available() || !self.is_place_tool_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        let placeable_kind = self.current_placeable_kind();
        if placeable_kind == PlaceableKind::Sprinkler {
            self.stop_terrain_edit_loop_sound();
            if let Some(target) =
                self.query_sprinkler_placement_target(super::SHOVEL_RAY_QUERY_DISTANCE)
            {
                if terrain_edit_endpoint_within_editable_chunk(target.position()) {
                    if let Err(err) = self.apply_sprinkler_placement(target) {
                        log::error!("Failed to place sprinkler: {}", err);
                    }
                }
            }
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                self.stop_terrain_edit_loop_sound();
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    return;
                }
                match placeable_kind {
                    PlaceableKind::Tree => {
                        let tree_desc = self.tree_placement_preview_desc.clone();
                        if let Err(err) = self.add_tree(
                            tree_desc,
                            TreePlacement::World(center),
                            TreeAddOptions::default().with_new_id(),
                        ) {
                            log::error!("Failed to plant tree: {}", err);
                        } else {
                            log::info!("Planted tree at {:?}", center);
                            if let Err(err) = self.advance_tree_placement_preview() {
                                log::error!("Failed to prepare the next tree preview: {err}");
                            }
                        }
                    }
                    PlaceableKind::Sprinkler => unreachable!("sprinkler placement handled above"),
                    PlaceableKind::Pipe => self.begin_pipe_drag(center),
                }
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
            }
            Err(err) => {
                log::error!("Placeable placement failed during terrain query: {}", err);
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
            if self.is_free_look_camera_mode() && !self.window_state.is_cursor_visible() {
                self.camera_control
                    .accumulate_free_look_mouse_delta(Vec2::new(delta.0 as f32, delta.1 as f32));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::select_sprinkler_placement_target;
    use crate::app::core::placeables::{PipeAttachment, PipeRayHit, SprinklerPlacementTarget};
    use glam::Vec3;

    #[test]
    fn sprinkler_target_prefers_a_pipe_in_front_of_terrain() {
        let pipe_hit = PipeRayHit {
            distance: 1.0,
            attachment: PipeAttachment {
                position_voxels: Vec3::new(10.0, 20.0, 30.0),
            },
        };

        let target = select_sprinkler_placement_target(
            Some((2.0, Vec3::new(0.5, 0.25, 0.5))),
            Some(pipe_hit),
        )
        .unwrap();

        let SprinklerPlacementTarget::Pipe(attachment) = target else {
            panic!("frontmost pipe should win");
        };
        assert_eq!(attachment.position_voxels, Vec3::new(10.0, 20.0, 30.0));
    }

    #[test]
    fn sprinkler_target_keeps_terrain_in_front_of_a_pipe() {
        let terrain_position = Vec3::new(0.5, 0.25, 0.5);
        let pipe_hit = PipeRayHit {
            distance: 2.0,
            attachment: PipeAttachment {
                position_voxels: Vec3::new(10.0, 20.0, 30.0),
            },
        };

        let target =
            select_sprinkler_placement_target(Some((1.0, terrain_position)), Some(pipe_hit))
                .unwrap();

        let SprinklerPlacementTarget::Terrain(position) = target else {
            panic!("frontmost terrain should win");
        };
        assert_eq!(position, terrain_position);
    }
}
