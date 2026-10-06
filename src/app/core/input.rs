use super::placeables::PlaceableKind;
use super::player_tools::{ContinuousTerrainToolAction, PlayerTool, PlayerToolSelectionUpdate};
use super::App;
use crate::app::terrain_edit_bounds::INITIAL_EDITABLE_TERRAIN_BOUNDS;
use crate::app::world_edits::{TerrainBrushEdit, TreeAddOptions, TreePlacement};
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

impl App {
    fn terrain_edit_endpoint_within_editable_chunk(&self, center: Vec3) -> bool {
        if self.rooftop_scene.is_some() {
            super::rooftop_scene::RooftopScene::allows_soil(center)
        } else {
            INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_point_xz(center)
        }
    }

    fn terrain_brush_endpoint_within_editable_chunk(&self, edit: TerrainBrushEdit) -> bool {
        if self.rooftop_scene.is_some() {
            self.terrain_edit_endpoint_within_editable_chunk(edit.end)
        } else {
            INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_brush_endpoint(edit)
        }
    }
}

const TERRAIN_EDIT_PREVIEW_VALID_COLOR: Vec3 = Vec3::new(0.45, 0.86, 1.0);
const TERRAIN_EDIT_PREVIEW_INVALID_COLOR: Vec3 = Vec3::new(1.0, 0.08, 0.06);

#[derive(Clone, Copy, Debug)]
pub(super) struct TerrainEditHover {
    pub(super) center: Vec3,
    pub(super) is_editable: bool,
}

pub(super) fn panel_blocks_world(config_open: bool, card_open: bool, orbit_edit: bool) -> bool {
    card_open || (config_open && !orbit_edit)
}

fn gui_owns_pointer(ctx: &egui::Context, position: Option<Vec2>) -> bool {
    ctx.egui_is_using_pointer()
        || position.is_some_and(|position| {
            let position = position / ctx.pixels_per_point();
            ctx.layer_id_at(egui::pos2(position.x, position.y))
                .is_some_and(|layer| layer.order != egui::Order::Background)
        })
}

pub(super) fn gui_consumes_nonkeyboard_event(
    pointer_event: bool,
    consumed: bool,
    pointer_owned: bool,
) -> bool {
    // egui-winit's consumed bit uses the previous egui frame's pointer position.
    // For mouse events the current hit-test plus ongoing UI drag owns routing.
    if pointer_event {
        pointer_owned
    } else {
        consumed
    }
}

impl App {
    fn blocking_panel_open(&self) -> bool {
        panel_blocks_world(
            self.config_panel_visible,
            self.card_display_visible,
            self.is_orbit_edit_camera_mode(),
        )
    }

    pub(super) fn gui_blocks_world_pointer(&self) -> bool {
        if !self.window_state.is_cursor_visible() {
            return false;
        }
        let ctx = self.egui_renderer.context();
        // Use the latest window-event position, not last frame's egui hover position.
        // A slider drag remains owned by egui even after leaving its window.
        gui_owns_pointer(ctx, self.cursor_position_physical)
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
        !self.blocking_panel_open() && !self.gui_wants_keyboard_input()
    }

    pub(super) fn terrain_edit_pointer_available(&self) -> bool {
        !self.blocking_panel_open()
            && !self.camera_control.zoom_in_progress()
            && !self.gui_blocks_world_pointer()
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
        if self
            .camera_control
            .sync_debug_panel_mode(self.config_panel_visible)
        {
            self.player_tools.cancel_continuous_hold();
            self.stop_terrain_edit_loop_sound();
            self.tracer.reset_camera_velocity();
            if self.is_orbit_edit_camera_mode() {
                self.sync_orbit_focus_from_current_view();
            }
        }
        let was_cursor_visible = self.window_state.is_cursor_visible();
        let cursor_visible = self.config_panel_visible
            || self.card_display_visible
            || self.is_orbit_edit_camera_mode();

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

    pub(super) fn sync_orbit_focus_from_current_view(&mut self) {
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
        self.review_orbit_limit();
        self.camera_control.set_orbit_elevation_limit(
            self.debug_settings
                .adjustables
                .camera_orbit_max_elevation
                .value,
        );
        if let Some((position, focus)) = self
            .camera_control
            .orbit_limit_correction(self.tracer.camera_position())
        {
            self.tracer.set_camera_pose_looking_at(position, focus);
        }
        self.review_camera_zoom();
        self.review_walk_entry();
        if self.camera_control.zoom_in_progress() {
            if !self.blocking_panel_open() {
                let step = self
                    .camera_control
                    .advance_zoom_transition(frame_delta_time);
                if let Some((pose, done)) = step {
                    let yaw = pose.yaw_deg.to_radians();
                    let pitch = pose.pitch_deg.to_radians();
                    let front = Vec3::new(
                        yaw.sin() * pitch.cos(),
                        pitch.sin(),
                        -yaw.cos() * pitch.cos(),
                    );
                    self.tracer
                        .set_camera_pose_looking_at(pose.position, pose.position + front);
                    if done {
                        self.tracer.reset_camera_velocity();
                        self.sync_cursor_with_panels();
                        log::info!(
                            "[CAMERA_ZOOM] completed walking={} yaw={} pitch={}",
                            self.is_walk_camera_mode(),
                            pose.yaw_deg,
                            pose.pitch_deg
                        );
                    }
                }
            }
            return self.tracer.take_footstep_events();
        }
        if self.is_free_fly_camera_mode() {
            self.tracer.update_fly_camera(frame_delta_time);
        } else if self.is_walk_camera_mode() {
            if self.camera_control.take_walk_entry_check() {
                let eye = self.tracer.camera_position();
                let height = self
                    .tracer
                    .prepare_walk_camera_movement(0., 0.)
                    .camera_height;
                match self.terrain_physics.recover_walk_entry(eye, height) {
                    Ok(Some(safe_eye)) => {
                        if safe_eye != eye {
                            let front = self.tracer.camera_front();
                            self.tracer
                                .set_camera_pose_looking_at(safe_eye, safe_eye + front);
                        }
                        self.tracer.reset_camera_velocity();
                        log::info!(
                            "[WALK_ENTRY] accepted original={eye:?} safe={safe_eye:?} capsule=true"
                        );
                    }
                    result => {
                        log::warn!("[WALK_ENTRY] rejected; remaining in free flight: {result:?}");
                        self.camera_control.apply_snapshot_mode(true);
                        self.tracer.reset_camera_velocity();
                        self.sync_cursor_with_panels();
                        return self.tracer.take_footstep_events();
                    }
                }
            }
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
        self.is_orbit_edit_camera_mode()
            && !self.camera_control.zoom_in_progress()
            && !self.blocking_panel_open()
            && !self.gui_blocks_world_pointer()
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
                if self.orbit_mouse_drag_available()
                    && button == MouseButton::Right
                    && (self.rooftop_scene.is_none()
                        || self.modifiers.alt_key()
                        || !self.player_tools.has_secondary_pointer_action()) =>
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
            if self.is_walk_camera_mode() {
                if scroll_lines.is_finite() && scroll_lines < 0. {
                    self.prepare_camera_zoom_switch();
                    self.camera_control
                        .begin_zoom_to_edit(self.tracer.camera_pose());
                    self.sync_cursor_with_panels();
                    log::info!("[CAMERA_ZOOM] started destination=orbit-edit preview=false");
                }
            } else {
                self.camera_control.queue_mouse_wheel_dolly(scroll_lines);
            }
        }
    }

    fn review_orbit_limit(&mut self) {
        let Some(frame) = self.camera_control.orbit_limit_review else {
            return;
        };
        self.camera_control.orbit_limit_review = Some(frame + 1);
        let stage = (frame / 20).min(2) as usize;
        let degrees = [45_f32, 25., 60.][stage];
        if frame.is_multiple_of(20) && frame <= 40 {
            self.debug_settings
                .adjustables
                .camera_orbit_max_elevation
                .value = degrees;
            let focus = self
                .camera_control
                .flora_showcase_center(self.tracer.camera_position());
            let angle = 80_f32.to_radians();
            self.tracer
                .set_camera_pose_looking_at(focus + Vec3::new(0., angle.sin(), angle.cos()), focus);
        } else if matches!(frame, 1 | 21 | 41) {
            let (_, angle, _) = self
                .camera_control
                .orbit_spherical(self.tracer.camera_position());
            assert!((angle.to_degrees() - degrees).abs() < 0.01);
            log::info!("[ORBIT_LIMIT_REVIEW] passed stage={stage} limit={degrees} elevation={} saved=false", angle.to_degrees());
            if frame == 41 {
                self.camera_control.orbit_limit_review = None;
            }
        }
    }

    fn review_walk_entry(&mut self) {
        let Some(frame) = self.camera_control.walk_entry_review else {
            return;
        };
        self.camera_control.walk_entry_review = Some(frame + 1);
        if matches!(frame, 0 | 30) {
            let center = super::camera_control::ORBIT_CAMERA_DEFAULT_FOCUS;
            let hit = self
                .query_terrain_ray_cpu(Vec3::new(center.x, 2., center.z), Vec3::NEG_Y)
                .expect("walk review needs terrain");
            let height = self
                .tracer
                .prepare_walk_camera_movement(0., 0.)
                .camera_height;
            let mut pose = self.tracer.camera_pose();
            pose.position = hit.position + Vec3::Y * (height - 0.025);
            self.tracer.apply_camera_pose(pose);
            self.camera_control.apply_snapshot_mode(true);
            self.toggle_camera_control_mode();
        } else if matches!(frame, 1 | 31) {
            assert!(
                self.is_walk_camera_mode(),
                "embedded walking entry should recover"
            );
            let eye = self.tracer.camera_position();
            let height = self
                .tracer
                .prepare_walk_camera_movement(0., 0.)
                .camera_height;
            assert_eq!(
                self.terrain_physics
                    .recover_walk_entry(eye, height)
                    .unwrap(),
                Some(eye),
                "published camera must already be clear"
            );
            log::info!(
                "[WALK_ENTRY_REVIEW] passed frame={frame} recovered=true whole_capsule=true"
            );
            if frame == 31 {
                self.camera_control.walk_entry_review = None;
            }
        }
    }

    // Opt-in native diagnostic drives the production wheel/capsule paths.
    fn review_camera_zoom(&mut self) {
        let Some((phase, frames)) = self.camera_control.zoom_review else {
            return;
        };
        self.camera_control.zoom_review = Some((phase, frames + 1));
        if self.camera_control.zoom_in_progress() || frames < 120 {
            return;
        }
        match phase {
            0 => {
                let center = super::camera_control::ORBIT_CAMERA_DEFAULT_FOCUS;
                let hit = self
                    .query_terrain_ray_cpu(Vec3::new(center.x, 2., center.z), Vec3::NEG_Y)
                    .expect("zoom review needs terrain");
                let height = self
                    .tracer
                    .prepare_walk_camera_movement(0., 0.)
                    .camera_height;
                let mut pose = self.tracer.camera_pose();
                pose.position = hit.position + Vec3::Y * (height + 0.002);
                pose.yaw_deg = 36.;
                pose.pitch_deg = 80.;
                self.tracer.apply_camera_pose(pose);
                self.camera_control.apply_snapshot_mode(false);
                self.sync_cursor_with_panels();
                self.handle_mouse_wheel(MouseScrollDelta::LineDelta(0., -1.));
                assert!(
                    self.camera_control.zoom_in_progress(),
                    "walk wheel must directly start withdrawal without preview"
                );
                self.camera_control.zoom_review = Some((1, 0));
            }
            1 => {
                assert!(self.is_orbit_edit_camera_mode());
                let pose = self.tracer.camera_pose();
                assert!((pose.pitch_deg + 25.).abs() < 0.01);
                assert!((pose.yaw_deg - 36.).abs() < 0.01);
                assert!(self.window_state.is_cursor_visible());
                self.handle_mouse_wheel(MouseScrollDelta::LineDelta(0., 12.));
                self.camera_control.zoom_review = Some((2, 0));
            }
            2 => {
                assert!(
                    self.is_walk_camera_mode(),
                    "zoom must land using capsule, not remain in orbit"
                );
                assert!(!self.window_state.is_cursor_visible());
                let pose = self.tracer.camera_pose();
                assert!(pose.position.is_finite());
                assert!(pose.pitch_deg.abs() < 0.01);
                assert!((pose.yaw_deg - 36.).abs() < 0.01);
                log::info!("[CAMERA_ZOOM_REVIEW] passed preview=false wheel_roundtrip=true yaw_preserved=true edit_pitch=-25 landing_capsule=true cursor=true");
                self.camera_control.zoom_review = None;
            }
            _ => unreachable!("native zoom review phase"),
        }
    }

    fn prepare_camera_zoom_switch(&mut self) {
        self.player_tools.cancel_continuous_hold();
        self.stop_terrain_edit_loop_sound();
        self.reset_camera_movement_input();
        self.tracer.reset_camera_velocity();
    }

    fn camera_scroll_available(&self) -> bool {
        (self.is_orbit_edit_camera_mode() || self.is_walk_camera_mode())
            && !self.camera_control.zoom_in_progress()
            && !self.blocking_panel_open()
            && !self.gui_blocks_world_pointer()
    }

    fn update_mouse_wheel_camera_dolly(&mut self, frame_delta_time: f32) {
        let available = self.camera_scroll_available();
        let camera_position = self.tracer.camera_position();
        if !self.is_orbit_edit_camera_mode() {
            return;
        }
        match self
            .camera_control
            .advance_orbit_dolly(frame_delta_time, available, camera_position)
        {
            Some(super::camera_control::OrbitDollyAction::Pose(position, focus)) => {
                self.tracer.set_camera_pose_looking_at(position, focus);
            }
            Some(super::camera_control::OrbitDollyAction::Land) => self.try_zoom_into_walk(),
            None => {}
        }
    }

    fn try_zoom_into_walk(&mut self) {
        let start = self.tracer.camera_pose();
        let focus = self.camera_control.flora_showcase_center(start.position);
        let origin = Vec3::new(focus.x, start.position.y.max(focus.y) + 0.25, focus.z);
        let Some(hit) = self.query_terrain_ray_cpu(origin, Vec3::NEG_Y) else {
            log::info!("[CAMERA_ZOOM] landing rejected reason=no-terrain");
            return;
        };
        // Sweep the real player capsule onto the published collision floor, not just
        // a brush-ray height. No floor / blocked capsule means stay in edit mode.
        let height = self
            .tracer
            .prepare_walk_camera_movement(0., 0.)
            .camera_height;
        let eye_above = hit.position + Vec3::Y * (height + 0.12);
        let request = crate::gameplay::camera::PlayerWalkMovementRequest {
            camera_position: eye_above,
            camera_height: height,
            desired_translation: Vec3::new(0., -0.122, 0.),
        };
        let result = match self.terrain_physics.move_player_capsule(request, 1. / 60.) {
            Ok(result) => result,
            Err(error) => {
                log::warn!("[CAMERA_ZOOM] landing rejected reason=capsule-query {error:#}");
                return;
            }
        };
        let eye = eye_above + result.translation;
        if !result.grounded || !eye.is_finite() || eye.y < hit.position.y + height - 0.01 {
            log::info!("[CAMERA_ZOOM] landing rejected reason=unsafe-capsule");
            return;
        }
        self.prepare_camera_zoom_switch();
        self.camera_control.begin_zoom_to_walk(start, eye);
        log::info!(
            "[CAMERA_ZOOM] started destination=walk ground={:?} eye={eye:?} capsule=true",
            hit.position
        );
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
            ContinuousTerrainToolAction::Till => self.try_tiller_brush(now),
        }
    }

    fn apply_player_tool_selection_update(&mut self, update: PlayerToolSelectionUpdate) {
        if !update.changed() {
            return;
        }
        if update.active_tool_changed() {
            self.wind_prototype.cancel();
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
                PlaceableKind::Tree | PlaceableKind::Sprinkler => {
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
            crate::audio::mixer::AudioCategory::Terrain,
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
            crate::audio::mixer::AudioCategory::Interface,
            super::ITEM_PANEL_SCROLL_SFX_PATH,
            super::ITEM_PANEL_SCROLL_SFX_VOLUME_DB,
        ) {
            log::error!("Failed to play item panel scroll sound: {}", err);
        }
    }

    pub(super) fn terrain_edit_ray(&self) -> Option<(Vec3, Vec3)> {
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
            .filter(|hit| (*hit - origin).length() <= max_distance)
            .filter(|hit| {
                self.rooftop_scene.is_none()
                    || super::rooftop_scene::RooftopScene::allows_soil(*hit)
            })
            .filter(|hit| {
                // A bare model surface is an Edit support, not a plantable voxel substrate.
                self.is_shovel_selected()
                    || self.rooftop_scene.as_ref().is_none_or(|s| {
                        s.ray_hit(origin, direction)
                            .is_none_or(|p| p.distance(*hit) > 1e-5)
                    })
            }))
    }

    pub(super) fn query_terrain_ray_cpu(
        &self,
        origin: Vec3,
        direction: Vec3,
    ) -> Option<crate::builder::ContreeCpuRayHit> {
        let direction = direction.normalize_or_zero();
        if direction == Vec3::ZERO {
            return None;
        }
        let mut terrain = self
            .contree_builder
            .query_terrain_ray_cpu(origin, direction);
        if let Some(position) = self
            .rooftop_scene
            .as_ref()
            .and_then(|s| s.ray_hit(origin, direction))
        {
            if terrain.is_none_or(|hit| position.distance(origin) < hit.position.distance(origin)) {
                terrain = Some(crate::builder::ContreeCpuRayHit {
                    position,
                    voxel_type: crate::builder::VOXEL_TYPE_ROCK,
                });
            }
        }
        if self.tracer.raster_trees.posed_surface.is_none() {
            return terrain;
        }
        let cells = &self.tracer.raster_trees.rest_mesh.solid_cells;
        for _ in 0..2048 {
            let Some(hit) = terrain else {
                break;
            };
            let cell = ((hit.position + direction * 1e-6) * 256.)
                .floor()
                .as_uvec3();
            if hit.voxel_type != 5 || !cells.contains(&cell.to_array()) {
                break;
            }
            let lower = cell.as_vec3() / 256.;
            let upper = lower + Vec3::splat(1. / 256.);
            let mut advance = f32::INFINITY;
            for axis in 0..3 {
                if direction[axis].abs() > 1e-8 {
                    let face = if direction[axis] > 0. {
                        upper[axis]
                    } else {
                        lower[axis]
                    };
                    advance = advance.min((face - hit.position[axis]) / direction[axis]);
                }
            }
            terrain = self.contree_builder.query_terrain_ray_cpu(
                hit.position + direction * (advance.max(0.) + 1e-6),
                direction,
            );
        }
        if let Some(hit) = self.query_tree_surface_ray(origin, direction) {
            if terrain.is_none_or(|terrain| hit.distance < terrain.position.distance(origin)) {
                return Some(crate::builder::ContreeCpuRayHit {
                    position: hit.world_position,
                    voxel_type: 5,
                });
            }
        }
        terrain
    }

    pub(super) fn query_tree_surface_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
    ) -> Option<crate::tree_gen::skin::SurfaceHit> {
        self.tracer.raster_trees.posed_surface.as_ref()?;
        self.tracer.raster_trees.raycast(
            origin,
            direction,
            self.terrain_physics.tree_ray_candidates(origin, direction),
        )
    }

    fn tree_edit_rest_center(&self, center: Vec3) -> Option<Vec3> {
        let (origin, direction) = self.terrain_edit_ray()?;
        let hit = self.query_tree_surface_ray(origin, direction)?;
        (hit.world_position.distance(center) < 1e-4).then_some(hit.rest_position)
    }

    pub(super) fn query_terrain_height_cpu(&self, pos_xz: Vec2) -> f32 {
        self.query_terrain_ray_cpu(Vec3::new(pos_xz.x, 10.0, pos_xz.y), Vec3::NEG_Y)
            .map(|hit| hit.position.y)
            .unwrap_or(0.0)
    }

    pub(super) fn try_shovel_dig(&mut self, now: Instant) {
        if !self.terrain_edit_pointer_available() || !self.is_shovel_selected() {
            self.player_tools
                .interrupt_stroke(ContinuousTerrainToolAction::ShovelDig);
            self.stop_terrain_edit_loop_sound();
            return;
        }
        let action = ContinuousTerrainToolAction::ShovelDig;

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !self.terrain_edit_endpoint_within_editable_chunk(center) {
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

                let tree_rest_center = self.tree_edit_rest_center(center);
                // Rest-space wood and world-space terrain must never share a path anchor.
                // Tree edits remain single dabs until we can track the hit tree's identity.
                if tree_rest_center.is_some() {
                    self.player_tools.interrupt_stroke(action);
                }
                let edit = self
                    .player_tools
                    .stroke_edit(action, tree_rest_center.unwrap_or(center));
                if tree_rest_center.is_none()
                    && !self.terrain_brush_endpoint_within_editable_chunk(edit)
                {
                    self.player_tools.defer_stroke(action, now);
                    self.stop_terrain_edit_loop_sound();
                    return;
                }
                if let Err(err) = self
                    .apply_surface_terrain_removal(
                        edit,
                        // Tree hits edit their rest-space wood, leaving nearby terrain intact.
                        tree_rest_center.map(|_| 5),
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
                        if self.rooftop_scene.is_none() {
                            self.voxel_backpack
                                .deposit_removed(&readback.stats, material_mode);
                        }
                        self.spawn_terrain_harvest_particles(
                            center,
                            &readback.stats,
                            &readback.sampled_positions_world,
                        );
                    })
                {
                    log::error!("Failed to apply terrain removal: {}", err);
                    self.player_tools.interrupt_stroke(action);
                    return;
                }
                self.player_tools.record_stroke_dab(action, now, center);
                if tree_rest_center.is_some() {
                    self.player_tools.interrupt_stroke(action);
                }
            }
            Ok(None) => {
                self.stop_terrain_edit_loop_sound();
                self.player_tools.defer_stroke(action, now);
            }
            Err(err) => {
                self.player_tools.interrupt_stroke(action);
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
                if !self.terrain_edit_endpoint_within_editable_chunk(center) {
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
                if !self.terrain_edit_endpoint_within_editable_chunk(center) {
                    self.stop_terrain_edit_loop_sound();
                    self.player_tools.defer_stroke(action, now);
                    return;
                }
                self.start_terrain_edit_loop_sound(center);

                let dab_interval = self.current_flora_paint_dab_interval();
                if !self.player_tools.stroke_ready(action, now, dab_interval) {
                    return;
                }
                if self.current_flora_paint_selection()
                    == species::FloraPaintSelection::ClimbingVine
                {
                    // A vine is one root per stroke, not a painted occupancy field.
                    if self.player_tools.previous_stroke_center(action).is_none() {
                        let direction = self.terrain_edit_ray().map(|(_, direction)| direction);
                        match direction
                            .map(|direction| self.plant_climbing_at_surface(center, direction))
                        {
                            Some(Ok(true)) => {}
                            Some(Ok(false)) | None => {
                                self.player_tools.defer_stroke(action, now);
                                return;
                            }
                            Some(Err(err)) => {
                                log::error!("Failed to plant climbing vine: {err}");
                                self.player_tools.interrupt_stroke(action);
                                return;
                            }
                        }
                    }
                    self.player_tools.record_stroke_dab(action, now, center);
                    return;
                }

                let edit = self.player_tools.stroke_edit(action, center);
                if !self.terrain_brush_endpoint_within_editable_chunk(edit) {
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
                if !self.terrain_edit_endpoint_within_editable_chunk(center) {
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

                let edit = self.player_tools.stroke_edit(action, center);
                if !self.terrain_brush_endpoint_within_editable_chunk(edit) {
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
        self.terrain_edit_endpoint_within_editable_chunk(center)
            && (self.rooftop_scene.is_none()
                || super::rooftop_scene::RooftopScene::allows_soil(center))
    }

    pub(super) fn try_shovel_place(&mut self, now: Instant) {
        if !self.terrain_edit_pointer_available() || !self.is_shovel_selected() {
            self.player_tools
                .interrupt_stroke(ContinuousTerrainToolAction::ShovelPlace);
            self.stop_terrain_edit_loop_sound();
            return;
        }
        let action = ContinuousTerrainToolAction::ShovelPlace;

        // The unsaved proof selects a semantic material without manufacturing inventory.
        // Normal garden placement retains its original first-stored-material behavior.
        let available = self
            .rooftop_scene
            .as_ref()
            .map(|s| (s.material, u32::MAX))
            .or_else(|| self.voxel_backpack.first_available());
        let Some((place_voxel, place_voxel_count)) = available else {
            self.player_tools.interrupt_stroke(action);
            self.stop_terrain_edit_loop_sound();
            return;
        };

        let place_voxel_type_id = place_voxel.voxel_type();

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !self.terrain_edit_endpoint_within_editable_chunk(center) {
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

                let edit = self.player_tools.stroke_edit(action, center);
                if !self.terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.player_tools.defer_stroke(action, now);
                    self.stop_terrain_edit_loop_sound();
                    return;
                }
                if let Err(err) = self
                    .apply_surface_terrain_placement(edit, place_voxel_type_id, place_voxel_count)
                    .map(|readback| {
                        if self.rooftop_scene.is_none() {
                            self.voxel_backpack.withdraw(
                                place_voxel,
                                readback.stats.count_added(place_voxel_type_id),
                            );
                        }
                    })
                {
                    log::error!("Failed to apply terrain placement: {}", err);
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
                log::error!("Shovel place attempt failed during terrain query: {}", err);
            }
        }
    }

    pub(super) fn try_hoe_trim(&mut self, now: Instant) {
        if !self.terrain_edit_pointer_available() || !self.is_hoe_selected() {
            self.player_tools
                .interrupt_stroke(ContinuousTerrainToolAction::HoeTrim);
            self.stop_terrain_edit_loop_sound();
            return;
        }
        let action = ContinuousTerrainToolAction::HoeTrim;

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !self.terrain_edit_endpoint_within_editable_chunk(center) {
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

                let edit = self.player_tools.stroke_edit(action, center);
                if !self.terrain_brush_endpoint_within_editable_chunk(edit) {
                    self.player_tools.defer_stroke(action, now);
                    self.stop_terrain_edit_loop_sound();
                    return;
                }
                if let Err(err) = self.apply_flora_trim_path(edit) {
                    log::error!("Failed to apply flora trim: {}", err);
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
                if !self.terrain_edit_endpoint_within_editable_chunk(center) {
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

                let edit = self.player_tools.stroke_edit(action, center);
                if !self.terrain_brush_endpoint_within_editable_chunk(edit) {
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

    pub(super) fn try_tiller_brush(&mut self, now: Instant) {
        let action = ContinuousTerrainToolAction::Till;
        if !self.terrain_edit_pointer_available() || !self.is_tiller_selected() {
            self.stop_terrain_edit_loop_sound();
            self.player_tools.interrupt_stroke(action);
            return;
        }

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                if !self.terrain_edit_endpoint_within_editable_chunk(center) {
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

                let edit = self.player_tools.stroke_edit(action, center);
                if !self.terrain_brush_endpoint_within_editable_chunk(edit) {
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

    pub(super) fn try_placeable_placement(&mut self) {
        if !self.terrain_edit_pointer_available() || !self.is_place_tool_selected() {
            self.stop_terrain_edit_loop_sound();
            return;
        }

        let placeable_kind = self.current_placeable_kind();

        match self.query_terrain_edit_ray_intersection(super::SHOVEL_RAY_QUERY_DISTANCE) {
            Ok(Some(center)) => {
                self.stop_terrain_edit_loop_sound();
                if !self.terrain_edit_endpoint_within_editable_chunk(center) {
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
                    PlaceableKind::Sprinkler => {
                        if let Err(err) = self.apply_sprinkler_placement(center) {
                            log::error!("Failed to place sprinkler: {err}");
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
                self.camera_control
                    .accumulate_free_look_mouse_delta(Vec2::new(delta.0 as f32, delta.1 as f32));
            }
        }
    }
}

#[cfg(test)]
mod panel_input_tests {
    use super::{gui_consumes_nonkeyboard_event, gui_owns_pointer, panel_blocks_world};

    #[test]
    fn latest_pointer_position_blocks_ui_but_leaves_world_available() {
        let ctx = egui::Context::default();
        let mut panel = egui::Rect::NOTHING;
        for _ in 0..2 {
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                panel = egui::Window::new("Debug input test")
                    .fixed_pos(egui::pos2(20.0, 20.0))
                    .show(ui.ctx(), |ui| {
                        ui.label("Controls");
                    })
                    .unwrap()
                    .response
                    .rect;
            });
        }
        let physical = |p: egui::Pos2| Some(glam::Vec2::new(p.x, p.y) * ctx.pixels_per_point());
        assert!(gui_owns_pointer(&ctx, physical(panel.center())));
        assert!(!gui_owns_pointer(
            &ctx,
            physical(panel.max + egui::vec2(100., 100.))
        ));
        let _ = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::PointerMoved(panel.center())],
                ..Default::default()
            },
            |ui| {
                egui::Window::new("Debug input test")
                    .fixed_pos(egui::pos2(20., 20.))
                    .show(ui.ctx(), |ui| {
                        ui.label("Controls");
                    });
            },
        );
        // Winit has queued a move out of Debug, followed by a press, before the next
        // egui frame. Its consumed response still reflects the previous hover.
        assert!(ctx.egui_wants_pointer_input());
        let owned = gui_owns_pointer(&ctx, physical(panel.max + egui::vec2(100., 100.)));
        assert!(!gui_consumes_nonkeyboard_event(
            true,
            ctx.egui_wants_pointer_input(),
            owned
        ));
    }

    #[test]
    fn debug_panel_is_non_modal_only_in_orbit_edit() {
        assert!(!panel_blocks_world(true, false, true));
        assert!(panel_blocks_world(true, false, false));
        for orbit in [false, true] {
            assert!(!panel_blocks_world(false, false, orbit));
            for debug in [false, true] {
                assert!(panel_blocks_world(debug, true, orbit));
            }
        }
    }
}
