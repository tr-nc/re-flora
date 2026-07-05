use super::placeables::PlaceableKind;
use super::ui_style::{
    FERTILIZER_SLOT_INDEX, HAND_SLOT_INDEX, HOE_SLOT_INDEX, ITEM_PANEL_SLOT_COUNT,
    PLACEABLE_PANEL_SLOT_COUNT, PLACE_TOOL_SLOT_INDEX, SHOVEL_SLOT_INDEX, SMOOTH_SLOT_INDEX,
    SOIL_INSPECTOR_SLOT_INDEX, SPRINKLER_PLACEABLE_SLOT_INDEX, SPRINKLER_SLOT_INDEX,
    STAFF_SLOT_INDEX, TILLER_SLOT_INDEX, TREE_PLACEABLE_SLOT_INDEX, TREE_SLOT_INDEX,
    WATERING_SLOT_INDEX,
};
use super::App;
use crate::app::terrain_edit_bounds::INITIAL_EDITABLE_TERRAIN_BOUNDS;
use crate::app::world_edits::{
    TerrainBrushEdit, TerrainRemovalEdit, TreeAddOptions, TreePlacement,
};
use crate::builder::ChunkModifyStats;
use crate::flora::species;
use crate::tracer::TerrainEditPreviewShape;
use glam::{Vec2, Vec3};
use rand::RngExt;
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

fn orbit_offset_to_spherical(mut offset: Vec3) -> (f32, f32, f32) {
    if !offset.is_finite() || offset.length_squared() <= f32::EPSILON {
        offset = Vec3::Z * super::ORBIT_CAMERA_MIN_DISTANCE;
    }

    let distance = offset.length().clamp(
        super::ORBIT_CAMERA_MIN_DISTANCE,
        super::ORBIT_CAMERA_MAX_DISTANCE,
    );
    let mut elevation = (offset.y / distance).asin().clamp(
        -super::ORBIT_CAMERA_MAX_ELEVATION_RAD,
        super::ORBIT_CAMERA_MAX_ELEVATION_RAD,
    );
    if !elevation.is_finite() {
        elevation = 0.0;
    }
    let azimuth = offset.x.atan2(offset.z);

    (azimuth, elevation, distance)
}

fn terrain_edit_endpoint_within_editable_chunk(center: Vec3) -> bool {
    INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_point_xz(center)
}

fn terrain_brush_endpoint_within_editable_chunk(edit: TerrainBrushEdit) -> bool {
    INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_brush_endpoint(edit)
}

const TERRAIN_EDIT_PREVIEW_VALID_COLOR: Vec3 = Vec3::new(0.45, 0.86, 1.0);
const TERRAIN_EDIT_PREVIEW_INVALID_COLOR: Vec3 = Vec3::new(1.0, 0.08, 0.06);

#[derive(Clone, Copy, Debug)]
pub(super) struct TerrainEditHover {
    pub(super) center: Vec3,
    pub(super) is_editable: bool,
}

fn orbit_planar_camera_axes(camera_front: Vec3) -> (Vec3, Vec3) {
    let mut planar_front = Vec3::new(camera_front.x, 0.0, camera_front.z).normalize_or_zero();
    if planar_front.length_squared() <= f32::EPSILON {
        planar_front = -Vec3::Z;
    }
    let planar_right = Vec3::new(-planar_front.z, 0.0, planar_front.x);
    (planar_front, planar_right)
}

fn orbit_focus_pan_delta(drag_delta_physical: Vec2, camera_front: Vec3, distance: f32) -> Vec3 {
    if !drag_delta_physical.is_finite() || drag_delta_physical.length_squared() <= f32::EPSILON {
        return Vec3::ZERO;
    }

    let (planar_front, planar_right) = orbit_planar_camera_axes(camera_front);
    let pan_scale = distance.clamp(
        super::ORBIT_CAMERA_MIN_DISTANCE,
        super::ORBIT_CAMERA_MAX_DISTANCE,
    ) * super::ORBIT_CAMERA_MOUSE_PAN_UNITS_PER_PIXEL_AT_UNIT_DISTANCE;

    (planar_front * drag_delta_physical.y - planar_right * drag_delta_physical.x) * pan_scale
}

fn orbit_anchored_pan_delta(anchor: Vec3, ray_origin: Vec3, ray_direction: Vec3) -> Option<Vec3> {
    if !anchor.is_finite()
        || !ray_origin.is_finite()
        || !ray_direction.is_finite()
        || ray_direction.length_squared() <= f32::EPSILON
        || ray_direction.y.abs() <= 0.01
    {
        return None;
    }

    let distance_to_anchor_height = (anchor.y - ray_origin.y) / ray_direction.y;
    if !distance_to_anchor_height.is_finite() || distance_to_anchor_height <= 0.0 {
        return None;
    }

    let desired_origin_xz = Vec2::new(anchor.x, anchor.z)
        - Vec2::new(ray_direction.x, ray_direction.z) * distance_to_anchor_height;
    let delta_xz = desired_origin_xz - Vec2::new(ray_origin.x, ray_origin.z);
    Some(Vec3::new(delta_xz.x, 0.0, delta_xz.y))
}

fn orbit_focus_from_view_ray(
    ray_origin: Vec3,
    ray_direction: Vec3,
    terrain_hit: Option<Vec3>,
    previous_focus: Vec3,
) -> Option<Vec3> {
    if !ray_origin.is_finite()
        || !ray_direction.is_finite()
        || ray_direction.length_squared() <= f32::EPSILON
    {
        return None;
    }

    let ray_direction = ray_direction.normalize();
    if let Some(hit) = terrain_hit {
        let hit_offset = hit - ray_origin;
        if hit.is_finite()
            && hit_offset.is_finite()
            && hit_offset.length() <= super::ORBIT_CAMERA_FOCUS_RAY_QUERY_DISTANCE
            && hit_offset.dot(ray_direction) > 0.0
        {
            return Some(hit);
        }
    }

    let previous_distance = (previous_focus - ray_origin).length();
    let fallback_distance = if previous_distance.is_finite() && previous_distance > f32::EPSILON {
        previous_distance
    } else {
        1.0
    }
    .clamp(
        super::ORBIT_CAMERA_MIN_DISTANCE,
        super::ORBIT_CAMERA_MAX_DISTANCE,
    );

    Some(ray_origin + ray_direction * fallback_distance)
}

impl App {
    fn blocking_panel_open(&self) -> bool {
        self.config_panel_visible || self.card_display_visible
    }

    pub(super) fn is_free_look_camera_mode(&self) -> bool {
        self.camera_control_mode.is_free_look()
    }

    pub(super) fn is_orbit_edit_camera_mode(&self) -> bool {
        self.camera_control_mode.is_orbit_edit()
    }

    pub(super) fn keyboard_tool_shortcuts_available(&self) -> bool {
        !self.blocking_panel_open()
    }

    pub(super) fn terrain_edit_pointer_available(&self) -> bool {
        !self.blocking_panel_open()
            && (!self.window_state.is_cursor_visible() || self.is_orbit_edit_camera_mode())
    }

    pub(super) fn reset_camera_movement_input(&mut self) {
        self.tracer.reset_camera_input();
        self.orbit_keyboard_pan_input.reset();
        self.reset_orbit_mouse_drag();
        self.mouse_wheel_dolly.reset();
    }

    pub(super) fn sync_cursor_with_panels(&mut self) {
        let cursor_visible = self.blocking_panel_open() || self.is_orbit_edit_camera_mode();
        self.window_state.set_cursor_grab(!cursor_visible);
        if self.blocking_panel_open() {
            self.player_tools.shovel_dig_held = false;
            self.stop_terrain_edit_loop_sound();
            self.reset_camera_movement_input();
        }
    }

    pub(super) fn toggle_camera_control_mode(&mut self) {
        self.camera_control_mode = if self.is_orbit_edit_camera_mode() {
            super::CameraControlMode::FreeLook
        } else {
            super::CameraControlMode::OrbitEdit
        };
        self.player_tools.shovel_dig_held = false;
        self.stop_terrain_edit_loop_sound();
        self.reset_camera_movement_input();

        if self.is_orbit_edit_camera_mode() {
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

        let terrain_hit = self.query_terrain_ray_cpu(origin, direction);
        if let Some(focus) =
            orbit_focus_from_view_ray(origin, direction, terrain_hit, self.orbit_camera_focus)
        {
            self.orbit_camera_focus = focus;
        }
    }

    fn look_at_orbit_focus_from_current_position(&mut self) {
        let focus = self.orbit_camera_focus;
        let mut position = self.tracer.camera_position();
        let offset = position - focus;
        if offset.length_squared() <= super::ORBIT_CAMERA_MIN_DISTANCE.powi(2) {
            let fallback = -self.tracer.camera_front().normalize_or_zero();
            let fallback = if fallback.length_squared() > f32::EPSILON {
                fallback
            } else {
                Vec3::Z
            };
            position = focus + fallback * super::ORBIT_CAMERA_MIN_DISTANCE;
        }
        self.tracer.set_camera_pose_looking_at(position, focus);
    }

    pub(super) fn update_camera_for_current_mode(&mut self, frame_delta_time: f32) {
        if self.is_free_look_camera_mode() {
            self.tracer
                .update_camera(frame_delta_time, self.is_fly_mode);
        } else {
            self.update_orbit_keyboard_camera_pan(frame_delta_time);
        }
        self.update_mouse_wheel_camera_dolly(frame_delta_time);
    }

    fn orbit_camera_spherical(&self) -> (f32, f32, f32) {
        orbit_offset_to_spherical(self.tracer.camera_position() - self.orbit_camera_focus)
    }

    fn apply_orbit_camera_spherical(&mut self, azimuth: f32, elevation: f32, distance: f32) {
        let azimuth = if azimuth.is_finite() { azimuth } else { 0.0 };
        let elevation = if elevation.is_finite() {
            elevation.clamp(
                -super::ORBIT_CAMERA_MAX_ELEVATION_RAD,
                super::ORBIT_CAMERA_MAX_ELEVATION_RAD,
            )
        } else {
            0.0
        };
        let distance = distance.clamp(
            super::ORBIT_CAMERA_MIN_DISTANCE,
            super::ORBIT_CAMERA_MAX_DISTANCE,
        );

        let horizontal_radius = distance * elevation.cos();
        let focus = self.orbit_camera_focus;
        let position = focus
            + Vec3::new(
                azimuth.sin() * horizontal_radius,
                elevation.sin() * distance,
                azimuth.cos() * horizontal_radius,
            );
        self.tracer.set_camera_pose_looking_at(position, focus);
    }

    pub(super) fn handle_orbit_keyboard_camera_input(
        &mut self,
        code: KeyCode,
        state: ElementState,
    ) {
        let pressed = state == ElementState::Pressed;
        self.orbit_keyboard_pan_input.handle_key(code, pressed);
    }

    fn update_orbit_keyboard_camera_pan(&mut self, frame_delta_time: f32) {
        if !self.orbit_mouse_drag_available()
            || frame_delta_time <= f32::EPSILON
            || !frame_delta_time.is_finite()
        {
            self.orbit_keyboard_pan_input.reset();
            return;
        }

        let input = self.orbit_keyboard_pan_input.input_vector();
        if input.length_squared() <= f32::EPSILON {
            return;
        }

        let (planar_front, planar_right) = orbit_planar_camera_axes(self.tracer.camera_front());
        let distance = (self.tracer.camera_position() - self.orbit_camera_focus)
            .length()
            .clamp(
                super::ORBIT_CAMERA_MIN_DISTANCE,
                super::ORBIT_CAMERA_MAX_DISTANCE,
            );
        let speed = distance * super::ORBIT_CAMERA_KEYBOARD_PAN_UNITS_PER_SECOND_AT_UNIT_DISTANCE;
        let pan_delta = (planar_right * input.x + Vec3::Y * input.y + planar_front * input.z)
            * speed
            * frame_delta_time;
        if pan_delta.length_squared() > f32::EPSILON {
            self.translate_orbit_camera(pan_delta);
        }
    }

    fn orbit_mouse_drag_available(&self) -> bool {
        self.is_orbit_edit_camera_mode() && !self.blocking_panel_open()
    }

    fn reset_orbit_mouse_drag(&mut self) {
        self.orbit_mouse_drag_held = false;
        self.orbit_mouse_drag_button = None;
        self.orbit_mouse_drag_pan_active = false;
        self.orbit_mouse_drag_anchor = None;
        self.orbit_mouse_drag_last_position_physical = None;
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
        if direction.length_squared() <= f32::EPSILON {
            return;
        }

        let Some(focus) = self
            .query_terrain_ray_cpu(origin, direction)
            .filter(|hit| (*hit - origin).length() <= super::ORBIT_CAMERA_FOCUS_RAY_QUERY_DISTANCE)
        else {
            return;
        };

        self.orbit_camera_focus = focus;
        self.look_at_orbit_focus_from_current_position();
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
                self.orbit_mouse_drag_held = true;
                self.orbit_mouse_drag_button = Some(button);
                self.orbit_mouse_drag_pan_active = true;
                self.orbit_mouse_drag_anchor = self.orbit_mouse_pan_anchor();
                self.orbit_mouse_drag_last_position_physical = self.cursor_position_physical;
                true
            }
            ElementState::Pressed
                if self.orbit_mouse_drag_available() && button == MouseButton::Right =>
            {
                self.player_tools.right_mouse_held = false;
                self.stop_terrain_edit_loop_sound();
                self.orbit_mouse_drag_held = true;
                self.orbit_mouse_drag_button = Some(button);
                self.orbit_mouse_drag_pan_active = false;
                self.orbit_mouse_drag_anchor = None;
                self.acquire_orbit_focus_from_screen_center();
                self.orbit_mouse_drag_last_position_physical = self.cursor_position_physical;
                true
            }
            ElementState::Released if self.orbit_mouse_drag_button == Some(button) => {
                self.reset_orbit_mouse_drag();
                true
            }
            _ => false,
        }
    }

    pub(super) fn sync_orbit_mouse_drag_position(&mut self, position_physical: Vec2) {
        if self.orbit_mouse_drag_held {
            self.orbit_mouse_drag_last_position_physical = Some(position_physical);
        }
    }

    pub(super) fn handle_orbit_mouse_drag(&mut self, position_physical: Vec2) {
        if !self.orbit_mouse_drag_held {
            return;
        }
        if !self.orbit_mouse_drag_available() {
            self.reset_orbit_mouse_drag();
            return;
        }

        let Some(previous_position) = self.orbit_mouse_drag_last_position_physical else {
            self.orbit_mouse_drag_last_position_physical = Some(position_physical);
            return;
        };
        self.orbit_mouse_drag_last_position_physical = Some(position_physical);

        let drag_delta = position_physical - previous_position;
        if drag_delta.length_squared() <= f32::EPSILON {
            return;
        }

        self.apply_orbit_mouse_drag_delta(drag_delta, position_physical);
    }

    fn apply_orbit_mouse_drag_delta(&mut self, drag_delta_physical: Vec2, position_physical: Vec2) {
        if self.orbit_mouse_drag_pan_active {
            if self.apply_orbit_mouse_anchored_pan(position_physical) {
                return;
            }
            self.apply_orbit_mouse_pan_delta(drag_delta_physical);
            return;
        }

        let (mut azimuth, mut elevation, distance) = self.orbit_camera_spherical();
        azimuth -= drag_delta_physical.x * super::ORBIT_CAMERA_MOUSE_DRAG_RADIANS_PER_PIXEL;
        elevation += drag_delta_physical.y * super::ORBIT_CAMERA_MOUSE_DRAG_RADIANS_PER_PIXEL;
        self.apply_orbit_camera_spherical(azimuth, elevation, distance);
    }

    fn orbit_mouse_pan_anchor(&self) -> Option<Vec3> {
        let (origin, direction) = self.terrain_edit_ray()?;
        if direction.length_squared() <= f32::EPSILON {
            return None;
        }

        self.query_terrain_ray_cpu(origin, direction)
            .filter(|hit| (*hit - origin).length() <= super::SHOVEL_RAY_QUERY_DISTANCE)
    }

    fn apply_orbit_mouse_anchored_pan(&mut self, position_physical: Vec2) -> bool {
        let Some(anchor) = self.orbit_mouse_drag_anchor else {
            return false;
        };

        let Some((origin, direction)) = self
            .tracer
            .camera_ray_from_screen_position(position_physical, self.window_state.window_extent())
        else {
            return false;
        };
        let Some(pan_delta) = orbit_anchored_pan_delta(anchor, origin, direction) else {
            return false;
        };
        if pan_delta.length_squared() <= f32::EPSILON {
            return true;
        }

        self.translate_orbit_camera(pan_delta);
        true
    }

    fn apply_orbit_mouse_pan_delta(&mut self, drag_delta_physical: Vec2) {
        let position = self.tracer.camera_position();
        let focus = self.orbit_camera_focus;
        let distance = (position - focus).length();
        let pan_delta =
            orbit_focus_pan_delta(drag_delta_physical, self.tracer.camera_front(), distance);
        if pan_delta.length_squared() <= f32::EPSILON {
            return;
        }

        self.translate_orbit_camera(pan_delta);
    }

    fn translate_orbit_camera(&mut self, delta: Vec3) {
        let new_focus = self.orbit_camera_focus + delta;
        let new_position = self.tracer.camera_position() + delta;
        self.orbit_camera_focus = new_focus;
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
            self.mouse_wheel_dolly.add_scroll_lines(scroll_lines);
        }
    }

    fn camera_scroll_available(&self) -> bool {
        self.is_orbit_edit_camera_mode() && !self.blocking_panel_open()
    }

    fn update_mouse_wheel_camera_dolly(&mut self, frame_delta_time: f32) {
        if !self.camera_scroll_available() {
            self.mouse_wheel_dolly.reset();
            return;
        }

        let scroll_lines = self.mouse_wheel_dolly.advance(frame_delta_time);
        if scroll_lines.abs() <= f32::EPSILON {
            return;
        }

        self.apply_mouse_wheel_camera_delta(scroll_lines);
    }

    fn apply_mouse_wheel_camera_delta(&mut self, scroll_lines: f32) {
        let forward_distance = scroll_lines
            * super::ORBIT_CAMERA_DOLLY_SPEED
            * super::MOUSE_WHEEL_DOLLY_SECONDS_PER_LINE;
        let (azimuth, elevation, distance) = self.orbit_camera_spherical();
        self.apply_orbit_camera_spherical(azimuth, elevation, distance - forward_distance);
    }

    pub(super) fn set_tool_mouse_button_state(&mut self, button: MouseButton, state: ElementState) {
        if button == MouseButton::Left {
            self.player_tools.left_mouse_held = state == ElementState::Pressed;
            self.reset_staff_regen_stroke_tracking();
            self.reset_watering_stroke_tracking();
            self.reset_fertilizing_stroke_tracking();
            if state == ElementState::Pressed {
                self.player_tools.last_staff_regen_time = None;
            }
        }
        if button == MouseButton::Right {
            self.player_tools.right_mouse_held = state == ElementState::Pressed;
            self.player_tools.last_staff_remove_center = None;
            if state == ElementState::Pressed {
                self.player_tools.last_staff_remove_time = None;
            }
        }
    }

    pub(super) fn refresh_terrain_edit_hold_from_mouse_buttons(&mut self) {
        self.player_tools.shovel_dig_held =
            self.player_tools.left_mouse_held || self.player_tools.right_mouse_held;
        if !self.player_tools.shovel_dig_held {
            self.stop_terrain_edit_loop_sound();
            self.reset_staff_stroke_tracking();
            self.reset_watering_stroke_tracking();
            self.reset_fertilizing_stroke_tracking();
        }
    }

    fn reset_staff_stroke_tracking(&mut self) {
        self.reset_staff_regen_stroke_tracking();
        self.player_tools.last_staff_remove_center = None;
    }

    fn reset_staff_regen_stroke_tracking(&mut self) {
        self.player_tools.last_staff_regen_center = None;
        self.player_tools.last_staff_regen_release_time = None;
        self.player_tools.active_staff_regen_paint_dab_serial = None;
    }

    fn reset_watering_stroke_tracking(&mut self) {
        self.player_tools.last_watering_center = None;
    }

    fn reset_fertilizing_stroke_tracking(&mut self) {
        self.player_tools.last_fertilizing_center = None;
        self.player_tools.active_fertilizer_stroke_seed = None;
    }

    fn active_fertilizer_stroke_seed(&mut self) -> u32 {
        if let Some(seed) = self.player_tools.active_fertilizer_stroke_seed {
            return seed;
        }

        let seed = self.player_tools.next_fertilizer_stroke_seed.max(1);
        self.player_tools.next_fertilizer_stroke_seed = seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223)
            .max(1);
        self.player_tools.active_fertilizer_stroke_seed = Some(seed);
        seed
    }

    fn reset_tilling_stroke_tracking(&mut self) {
        self.player_tools.last_tilling_center = None;
    }

    pub(super) fn clear_selected_tool(&mut self) {
        if self.player_tools.selected_item_panel_slot.is_some() {
            self.player_tools.selected_item_panel_slot = None;
            self.reset_staff_stroke_tracking();
            self.reset_watering_stroke_tracking();
            self.reset_fertilizing_stroke_tracking();
            self.reset_tilling_stroke_tracking();
            self.player_tools.shovel_dig_held = false;
            self.stop_terrain_edit_loop_sound();
            self.play_item_panel_scroll_sound();
        }
    }

    pub(super) fn activate_item_panel_slot(&mut self, slot_idx: usize) {
        if slot_idx == HAND_SLOT_INDEX {
            self.clear_selected_tool();
            return;
        }

        if slot_idx <= PLACE_TOOL_SLOT_INDEX
            && Some(slot_idx) != self.player_tools.selected_item_panel_slot
        {
            self.player_tools.selected_item_panel_slot = Some(slot_idx);
            self.reset_staff_stroke_tracking();
            self.reset_watering_stroke_tracking();
            self.reset_fertilizing_stroke_tracking();
            self.reset_tilling_stroke_tracking();
            self.play_item_panel_scroll_sound();
        }
    }

    pub(super) fn select_item_panel_slot(&mut self, slot_idx: usize) {
        if slot_idx == TREE_SLOT_INDEX {
            self.select_placeable_tool(TREE_PLACEABLE_SLOT_INDEX);
        } else if slot_idx == SPRINKLER_SLOT_INDEX {
            self.select_placeable_tool(SPRINKLER_PLACEABLE_SLOT_INDEX);
        } else if slot_idx == HAND_SLOT_INDEX
            || Some(slot_idx) == self.player_tools.selected_item_panel_slot
        {
            self.clear_selected_tool();
        } else if slot_idx < ITEM_PANEL_SLOT_COUNT {
            self.activate_item_panel_slot(slot_idx);
        }
    }

    pub(super) fn select_placeable_panel_slot(&mut self, slot_idx: usize) {
        if slot_idx < PLACEABLE_PANEL_SLOT_COUNT
            && slot_idx != self.player_tools.selected_placeable_panel_slot
        {
            self.player_tools.selected_placeable_panel_slot = slot_idx;
            self.play_item_panel_scroll_sound();
        }
    }

    pub(super) fn select_placeable_tool(&mut self, slot_idx: usize) {
        if slot_idx >= PLACEABLE_PANEL_SLOT_COUNT {
            return;
        }

        if self.is_place_tool_selected()
            && slot_idx == self.player_tools.selected_placeable_panel_slot
        {
            self.clear_selected_tool();
            return;
        }

        self.select_placeable_panel_slot(slot_idx);
        self.activate_item_panel_slot(TREE_SLOT_INDEX);
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

    fn consume_next_flora_paint_dab_serial(&mut self, now: Instant) -> u32 {
        let paint_dab_serial = self.flora_paint_dab_serial;
        self.flora_paint_dab_serial = self.flora_paint_dab_serial.wrapping_add(1);
        self.player_tools.active_staff_regen_paint_dab_serial = Some(paint_dab_serial);
        self.player_tools.last_staff_regen_release_time = Some(now);
        paint_dab_serial
    }

    fn current_staff_regen_paint_dab_serial(&mut self, now: Instant) -> (u32, bool) {
        let paint_brush = species::flora_paint_brush_settings(self.current_flora_paint_selection());
        if paint_brush.soft_spacing_voxels == 0 || paint_brush.plants_per_release == 0 {
            return (self.flora_paint_dab_serial, true);
        }

        if let (Some(active_serial), Some(last_release)) = (
            self.player_tools.active_staff_regen_paint_dab_serial,
            self.player_tools.last_staff_regen_release_time,
        ) {
            if now.duration_since(last_release) < self.current_flora_paint_release_interval() {
                return (active_serial, false);
            }
        }

        (self.consume_next_flora_paint_dab_serial(now), true)
    }

    pub(super) fn cycle_flora_paint_selection(&mut self) {
        let selection_count = species::PLAYER_FLORA_PAINT_SELECTIONS.len();
        self.player_tools.flora_paint_selection_index =
            (self.player_tools.flora_paint_selection_index + 1) % selection_count;
        self.player_tools.last_staff_regen_time = None;
        self.reset_staff_regen_stroke_tracking();
        self.play_item_panel_scroll_sound();
        log::info!(
            "Grow brush flora selection: {}",
            self.current_flora_paint_selection_label()
        );
    }

    pub(super) fn is_shovel_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == Some(SHOVEL_SLOT_INDEX)
    }

    pub(super) fn is_smooth_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == Some(SMOOTH_SLOT_INDEX)
    }

    pub(super) fn is_terrain_edit_radius_tool_selected(&self) -> bool {
        self.is_shovel_selected()
            || self.is_smooth_selected()
            || self.is_staff_selected()
            || self.is_hoe_selected()
            || self.is_place_tool_selected()
            || self.is_watering_selected()
            || self.is_soil_inspector_selected()
            || self.is_fertilizer_selected()
            || self.is_tiller_selected()
    }

    pub(super) fn terrain_edit_preview_shape(&self) -> TerrainEditPreviewShape {
        if self.is_place_tool_selected() {
            match self.current_placeable_kind() {
                PlaceableKind::Tree => TerrainEditPreviewShape::TreeBillboard,
                PlaceableKind::Sprinkler => TerrainEditPreviewShape::SurfaceCircle,
            }
        } else if self.is_watering_selected() || self.is_fertilizer_selected() {
            TerrainEditPreviewShape::Sphere
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
        self.player_tools.selected_item_panel_slot == Some(STAFF_SLOT_INDEX)
    }

    pub(super) fn is_hoe_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == Some(HOE_SLOT_INDEX)
    }

    pub(super) fn is_place_tool_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == Some(TREE_SLOT_INDEX)
    }

    pub(super) fn is_watering_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == Some(WATERING_SLOT_INDEX)
    }

    pub(super) fn is_soil_inspector_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == Some(SOIL_INSPECTOR_SLOT_INDEX)
    }

    pub(super) fn is_fertilizer_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == Some(FERTILIZER_SLOT_INDEX)
    }

    pub(super) fn is_tiller_selected(&self) -> bool {
        self.player_tools.selected_item_panel_slot == Some(TILLER_SLOT_INDEX)
    }

    fn active_voxel_type_id(&self) -> Option<u32> {
        // No selected backpack voxel means terrain removal accepts every concrete voxel type.
        super::ActiveVoxelType::All.voxel_type()
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

    fn add_voxel_to_backpack(&mut self, voxel_type: super::ActiveVoxelType, amount: u32) {
        match voxel_type {
            super::ActiveVoxelType::All => unreachable!("All is not a concrete backpack voxel"),
            super::ActiveVoxelType::Dirt => {
                self.player_tools.backpack_dirt_count =
                    self.player_tools.backpack_dirt_count.saturating_add(amount)
            }
            super::ActiveVoxelType::Sand => {
                self.player_tools.backpack_sand_count =
                    self.player_tools.backpack_sand_count.saturating_add(amount)
            }
            super::ActiveVoxelType::CherryWood => {
                self.player_tools.backpack_cherry_wood_count = self
                    .player_tools
                    .backpack_cherry_wood_count
                    .saturating_add(amount)
            }
            super::ActiveVoxelType::OakWood => {
                self.player_tools.backpack_oak_wood_count = self
                    .player_tools
                    .backpack_oak_wood_count
                    .saturating_add(amount)
            }
            super::ActiveVoxelType::Rock => {
                self.player_tools.backpack_rock_count =
                    self.player_tools.backpack_rock_count.saturating_add(amount)
            }
        }
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
        // Placement also ignores material selection: use any available stored voxel.
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
        if !self.terrain_edit_pointer_available() || !self.is_shovel_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_shovel_dig_time = Some(now);
                    return;
                }
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
                        None,
                        None,
                    )
                    .map(|readback| {
                        let removed_total: u32 = readback.stats.removed_counts.iter().sum();
                        if removed_total == 0 {
                            self.stop_terrain_edit_loop_sound();
                            return;
                        }

                        self.add_removed_voxels_to_backpack(&readback.stats);
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
        if !self.terrain_edit_pointer_available() || !self.is_smooth_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_smooth_time = Some(now);
                    return;
                }
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
        if !self.terrain_edit_pointer_available() || !self.is_staff_selected() {
            self.stop_terrain_edit_loop_sound();
            self.reset_staff_regen_stroke_tracking();
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_staff_regen_time = Some(now);
                    self.reset_staff_regen_stroke_tracking();
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_regen) = self.player_tools.last_staff_regen_time {
                    if now.duration_since(last_regen) < self.current_flora_paint_dab_interval() {
                        return;
                    }
                }

                let edit = TerrainBrushEdit::from_previous_center(
                    self.player_tools.last_staff_regen_center,
                    center,
                    self.player_tools.terrain_edit_radius,
                );
                if !terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_staff_regen_time = Some(now);
                    self.reset_staff_regen_stroke_tracking();
                    return;
                }
                let (paint_dab_serial, is_release_step) =
                    self.current_staff_regen_paint_dab_serial(now);
                if let Err(err) =
                    self.apply_surface_flora_regeneration(edit, paint_dab_serial, is_release_step)
                {
                    log::error!("Failed to apply flora regeneration: {}", err);
                    self.reset_staff_regen_stroke_tracking();
                    return;
                }
                self.player_tools.last_staff_regen_time = Some(now);
                self.player_tools.last_staff_regen_center = Some(center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.last_staff_regen_time = Some(now);
                self.reset_staff_regen_stroke_tracking();
            }
            Err(err) => {
                self.reset_staff_regen_stroke_tracking();
                log::error!(
                    "Staff regeneration attempt failed during terrain query: {}",
                    err
                );
            }
        }
    }

    pub(super) fn try_staff_remove_flora(&mut self, now: Instant) {
        if !self.terrain_edit_pointer_available() || !self.is_staff_selected() {
            self.stop_terrain_edit_loop_sound();
            self.player_tools.last_staff_remove_center = None;
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_staff_remove_time = Some(now);
                    self.player_tools.last_staff_remove_center = None;
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_remove) = self.player_tools.last_staff_remove_time {
                    if now.duration_since(last_remove) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                let edit = TerrainBrushEdit::from_previous_center(
                    self.player_tools.last_staff_remove_center,
                    center,
                    self.player_tools.terrain_edit_radius,
                );
                if !terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_staff_remove_time = Some(now);
                    self.player_tools.last_staff_remove_center = None;
                    return;
                }
                if let Err(err) = self.apply_surface_flora_removal(edit) {
                    log::error!("Failed to apply flora removal: {}", err);
                    self.player_tools.last_staff_remove_center = None;
                    return;
                }
                self.player_tools.last_staff_remove_time = Some(now);
                self.player_tools.last_staff_remove_center = Some(center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.last_staff_remove_time = Some(now);
                self.player_tools.last_staff_remove_center = None;
            }
            Err(err) => {
                self.player_tools.last_staff_remove_center = None;
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

        let Some(place_voxel_type) = self.first_placeable_voxel_type() else {
            self.stop_terrain_edit_loop_sound();
            return;
        };

        let place_voxel_type_id = place_voxel_type
            .voxel_type()
            .expect("placeable voxel type should be concrete");
        let place_voxel_count = self.voxel_count(place_voxel_type);

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_shovel_place_time = Some(now);
                    return;
                }
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
        if !self.terrain_edit_pointer_available() || !self.is_hoe_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_hoe_trim_time = Some(now);
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_trim) = self.player_tools.last_hoe_trim_time {
                    if now.duration_since(last_trim) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                if let Err(err) = self.apply_flora_trim(TerrainRemovalEdit {
                    center,
                    radius: self.player_tools.terrain_edit_radius,
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

    pub(super) fn try_watering_brush(&mut self, now: Instant) {
        if !self.terrain_edit_pointer_available() || !self.is_watering_selected() {
            self.stop_terrain_edit_loop_sound();
            self.reset_watering_stroke_tracking();
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_watering_time = Some(now);
                    self.reset_watering_stroke_tracking();
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_water) = self.player_tools.last_watering_time {
                    if now.duration_since(last_water) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                let edit = TerrainBrushEdit::from_previous_center(
                    self.player_tools.last_watering_center,
                    center,
                    self.player_tools.terrain_edit_radius,
                );
                if !terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_watering_time = Some(now);
                    self.reset_watering_stroke_tracking();
                    return;
                }
                if let Err(err) = self.add_watering_brush_moisture(edit) {
                    log::error!("Failed to apply watering brush: {}", err);
                    self.reset_watering_stroke_tracking();
                    return;
                }
                self.player_tools.last_watering_time = Some(now);
                self.player_tools.last_watering_center = Some(center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.last_watering_time = Some(now);
                self.reset_watering_stroke_tracking();
            }
            Err(err) => {
                self.reset_watering_stroke_tracking();
                log::error!(
                    "Watering brush attempt failed during terrain query: {}",
                    err
                );
            }
        }
    }

    pub(super) fn try_fertilizer_brush(&mut self, now: Instant) {
        if !self.terrain_edit_pointer_available() || !self.is_fertilizer_selected() {
            self.stop_terrain_edit_loop_sound();
            self.reset_fertilizing_stroke_tracking();
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_fertilizing_time = Some(now);
                    self.reset_fertilizing_stroke_tracking();
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_fertilize) = self.player_tools.last_fertilizing_time {
                    if now.duration_since(last_fertilize) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                let edit = TerrainBrushEdit::from_previous_center(
                    self.player_tools.last_fertilizing_center,
                    center,
                    self.player_tools.terrain_edit_radius,
                );
                if !terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_fertilizing_time = Some(now);
                    self.reset_fertilizing_stroke_tracking();
                    return;
                }
                let stroke_seed = self.active_fertilizer_stroke_seed();
                if let Err(err) = self.add_fertilizer_brush_fertility(edit, stroke_seed) {
                    log::error!("Failed to apply fertilizer brush: {}", err);
                    self.reset_fertilizing_stroke_tracking();
                    return;
                }
                self.player_tools.last_fertilizing_time = Some(now);
                self.player_tools.last_fertilizing_center = Some(center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.last_fertilizing_time = Some(now);
                self.reset_fertilizing_stroke_tracking();
            }
            Err(err) => {
                self.reset_fertilizing_stroke_tracking();
                log::error!(
                    "Fertilizer brush attempt failed during terrain query: {}",
                    err
                );
            }
        }
    }

    pub(super) fn try_tiller_brush(&mut self, now: Instant) {
        if !self.terrain_edit_pointer_available() || !self.is_tiller_selected() {
            self.stop_terrain_edit_loop_sound();
            self.reset_tilling_stroke_tracking();
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_tilling_time = Some(now);
                    self.reset_tilling_stroke_tracking();
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                if let Some(last_till) = self.player_tools.last_tilling_time {
                    if now.duration_since(last_till) < super::SHOVEL_DIG_INTERVAL {
                        return;
                    }
                }

                let edit = TerrainBrushEdit::from_previous_center(
                    self.player_tools.last_tilling_center,
                    center,
                    self.player_tools.terrain_edit_radius,
                );
                if !terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.last_tilling_time = Some(now);
                    self.reset_tilling_stroke_tracking();
                    return;
                }
                if let Err(err) = self.mix_tiller_brush_fertility(edit) {
                    log::error!("Failed to apply tiller brush: {}", err);
                    self.reset_tilling_stroke_tracking();
                    return;
                }
                self.player_tools.last_tilling_time = Some(now);
                self.player_tools.last_tilling_center = Some(center);
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.last_tilling_time = Some(now);
                self.reset_tilling_stroke_tracking();
            }
            Err(err) => {
                self.reset_tilling_stroke_tracking();
                log::error!("Tiller brush attempt failed during terrain query: {}", err);
            }
        }
    }

    pub(super) fn try_placeable_placement(&mut self) {
        if !self.terrain_edit_pointer_available() || !self.is_place_tool_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                self.stop_terrain_edit_loop_sound();
                if !terrain_edit_endpoint_within_editable_chunk(center) {
                    return;
                }
                match self.current_placeable_kind() {
                    PlaceableKind::Tree => {
                        let mut tree_desc = self.debug_tree_desc.clone();
                        tree_desc.seed = rand::rng().random::<u64>();
                        if let Err(err) = self.add_tree(
                            tree_desc,
                            TreePlacement::World(center),
                            TreeAddOptions::default().with_new_id(),
                        ) {
                            log::error!("Failed to plant tree: {}", err);
                        } else {
                            log::info!("Planted tree at {:?}", center);
                        }
                    }
                    PlaceableKind::Sprinkler => {
                        if let Err(err) = self.apply_sprinkler_placement(center) {
                            log::error!("Failed to place sprinkler: {}", err);
                        }
                    }
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
                self.accumulated_mouse_delta += Vec2::new(delta.0 as f32, delta.1 as f32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        orbit_anchored_pan_delta, orbit_focus_from_view_ray, orbit_focus_pan_delta,
        orbit_offset_to_spherical,
    };
    use glam::{Vec2, Vec3};

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.0001,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn orbit_spherical_preserves_angles_at_minimum_distance() {
        let min_distance = super::super::ORBIT_CAMERA_MIN_DISTANCE;
        let (azimuth, elevation, distance) = orbit_offset_to_spherical(Vec3::X * min_distance);

        assert_near(azimuth, std::f32::consts::FRAC_PI_2);
        assert_near(elevation, 0.0);
        assert_near(distance, min_distance);
    }

    #[test]
    fn orbit_focus_pan_delta_stays_on_xz_plane() {
        let delta = orbit_focus_pan_delta(Vec2::new(24.0, -16.0), Vec3::new(0.2, -0.9, -0.4), 2.0);

        assert_near(delta.y, 0.0);
        assert!(delta.x.abs() > 0.0 || delta.z.abs() > 0.0);
    }

    #[test]
    fn orbit_focus_pan_delta_uses_camera_planar_axes() {
        let delta = orbit_focus_pan_delta(Vec2::new(10.0, -20.0), -Vec3::Z, 1.0);
        let scale = super::super::ORBIT_CAMERA_MOUSE_PAN_UNITS_PER_PIXEL_AT_UNIT_DISTANCE;

        assert_near(delta.x, -10.0 * scale);
        assert_near(delta.y, 0.0);
        assert_near(delta.z, 20.0 * scale);
    }

    #[test]
    fn orbit_anchored_pan_delta_keeps_anchor_under_ray_on_xz_plane() {
        let anchor = Vec3::new(0.5, 0.25, 0.5);
        let ray_origin = Vec3::new(0.0, 1.25, 0.0);
        let ray_direction = Vec3::new(0.25, -0.5, 0.1).normalize();
        let delta = orbit_anchored_pan_delta(anchor, ray_origin, ray_direction).unwrap();

        assert_near(delta.y, 0.0);
        let shifted_origin = ray_origin + delta;
        let t = (anchor.y - shifted_origin.y) / ray_direction.y;
        let hit = shifted_origin + ray_direction * t;
        assert_near(hit.x, anchor.x);
        assert_near(hit.y, anchor.y);
        assert_near(hit.z, anchor.z);
    }

    #[test]
    fn orbit_anchored_pan_delta_rejects_near_horizontal_rays() {
        let delta = orbit_anchored_pan_delta(
            Vec3::new(0.5, 0.25, 0.5),
            Vec3::new(0.0, 1.25, 0.0),
            Vec3::new(1.0, 0.001, 0.0),
        );

        assert!(delta.is_none());
    }

    #[test]
    fn orbit_focus_from_view_ray_prefers_center_terrain_hit() {
        let origin = Vec3::new(0.0, 1.0, 0.0);
        let direction = Vec3::new(0.0, -0.5, -1.0).normalize();
        let hit = origin + direction * 1.75;
        let previous_focus = Vec3::new(1.0, 0.5, 1.0);

        let focus =
            orbit_focus_from_view_ray(origin, direction, Some(hit), previous_focus).unwrap();

        assert_near(focus.x, hit.x);
        assert_near(focus.y, hit.y);
        assert_near(focus.z, hit.z);
    }

    #[test]
    fn orbit_focus_from_view_ray_falls_back_along_current_view() {
        let origin = Vec3::new(0.0, 1.0, 0.0);
        let direction = Vec3::new(1.0, -0.25, 0.5).normalize();
        let previous_focus = origin + Vec3::Z * 2.0;

        let focus = orbit_focus_from_view_ray(origin, direction, None, previous_focus).unwrap();
        let focus_direction = (focus - origin).normalize();

        assert_near(focus_direction.x, direction.x);
        assert_near(focus_direction.y, direction.y);
        assert_near(focus_direction.z, direction.z);
        assert_near((focus - origin).length(), 2.0);
    }
}
