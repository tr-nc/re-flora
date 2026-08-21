use crate::app::terrain_edit_bounds::INITIAL_EDITABLE_TERRAIN_BOUNDS;
use glam::{Vec2, Vec3};
use winit::event::{ElementState, MouseButton};
use winit::keyboard::KeyCode;

const ORBIT_CAMERA_DEFAULT_FOCUS_HEIGHT: f32 = 0.5;
const ORBIT_CAMERA_MIN_DISTANCE: f32 = 0.2;
const ORBIT_CAMERA_MAX_DISTANCE: f32 = 5.0;
const ORBIT_CAMERA_DOLLY_SPEED: f32 = 0.75;
const ORBIT_CAMERA_FOCUS_RAY_QUERY_DISTANCE: f32 = 10.0;
const ORBIT_CAMERA_MOUSE_DRAG_RADIANS_PER_PIXEL: f32 = 0.005;
const ORBIT_CAMERA_MOUSE_PAN_UNITS_PER_PHYSICAL_PIXEL: f32 = 0.001;
const ORBIT_CAMERA_DELTA_INTERPOLATION_RATE: f32 = 14.0;
const ORBIT_CAMERA_DELTA_SNAP_DISTANCE: f32 = 0.00001;
const ORBIT_CAMERA_KEYBOARD_PAN_UNITS_PER_SECOND_AT_UNIT_DISTANCE: f32 = 0.9;
const ORBIT_CAMERA_KEYBOARD_PAN_FAR_ZOOM_BOOST_START: f32 = 0.95;
const ORBIT_CAMERA_KEYBOARD_PAN_FAR_ZOOM_MAX_MULTIPLIER: f32 = 1.6;
const ORBIT_CAMERA_MAX_ELEVATION_RAD: f32 = std::f32::consts::FRAC_PI_2 - 0.04;
const MOUSE_WHEEL_DOLLY_SECONDS_PER_LINE: f32 = 0.16;
const MOUSE_WHEEL_DOLLY_INTERPOLATION_RATE: f32 = 16.0;
const MOUSE_WHEEL_DOLLY_SNAP_LINES: f32 = 0.001;
const MOUSE_WHEEL_DOLLY_MAX_PENDING_LINES: f32 = 24.0;
const FREE_LOOK_MOUSE_SMOOTHING: f32 = 0.4;

pub(super) const ORBIT_CAMERA_DEFAULT_FOCUS: Vec3 =
    INITIAL_EDITABLE_TERRAIN_BOUNDS.center_at_height(ORBIT_CAMERA_DEFAULT_FOCUS_HEIGHT);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum CameraControlMode {
    FreeFly,
    Walk,
    #[default]
    OrbitEdit,
}

impl CameraControlMode {
    fn next(self) -> Self {
        match self {
            Self::OrbitEdit => Self::FreeFly,
            Self::FreeFly => Self::Walk,
            Self::Walk => Self::OrbitEdit,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct OrbitKeyboardPanInput {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

impl OrbitKeyboardPanInput {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn handle_key(&mut self, code: KeyCode, pressed: bool) {
        match code {
            KeyCode::KeyW | KeyCode::ArrowUp => self.forward = pressed,
            KeyCode::KeyS | KeyCode::ArrowDown => self.backward = pressed,
            KeyCode::KeyA | KeyCode::ArrowLeft => self.left = pressed,
            KeyCode::KeyD | KeyCode::ArrowRight => self.right = pressed,
            KeyCode::KeyE => self.up = pressed,
            KeyCode::KeyQ => self.down = pressed,
            _ => {}
        }
    }

    fn input_vector(self) -> Vec3 {
        let mut input = Vec3::ZERO;
        if self.forward {
            input.z += 1.0;
        }
        if self.backward {
            input.z -= 1.0;
        }
        if self.left {
            input.x -= 1.0;
        }
        if self.right {
            input.x += 1.0;
        }
        if self.up {
            input.y += 1.0;
        }
        if self.down {
            input.y -= 1.0;
        }
        input.normalize_or_zero()
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct OrbitDeltaSmoother {
    current_delta: Vec3,
    target_delta: Vec3,
}

impl OrbitDeltaSmoother {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn add_delta(&mut self, delta: Vec3) {
        if delta.is_finite() {
            self.target_delta += delta;
        }
    }

    fn pending_delta(&self) -> Vec3 {
        self.target_delta - self.current_delta
    }

    fn advance(&mut self, frame_delta_time: f32) -> Vec3 {
        if frame_delta_time <= f32::EPSILON || !frame_delta_time.is_finite() {
            return Vec3::ZERO;
        }

        let remaining_delta = self.pending_delta();
        if remaining_delta.length_squared() <= ORBIT_CAMERA_DELTA_SNAP_DISTANCE.powi(2) {
            self.reset();
            return remaining_delta;
        }

        let alpha = (1.0 - (-ORBIT_CAMERA_DELTA_INTERPOLATION_RATE * frame_delta_time).exp())
            .clamp(0.0, 1.0);
        let mut advanced_delta = remaining_delta * alpha;
        self.current_delta += advanced_delta;

        let remaining_after_advance = self.target_delta - self.current_delta;
        if remaining_after_advance.length_squared() <= ORBIT_CAMERA_DELTA_SNAP_DISTANCE.powi(2) {
            advanced_delta += remaining_after_advance;
            self.reset();
        }

        advanced_delta
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct MouseWheelDollySmoother {
    current_lines: f32,
    target_lines: f32,
}

impl MouseWheelDollySmoother {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn add_scroll_lines(&mut self, scroll_lines: f32) {
        if scroll_lines.abs() <= f32::EPSILON || !scroll_lines.is_finite() {
            return;
        }

        let pending_lines = (self.target_lines - self.current_lines + scroll_lines).clamp(
            -MOUSE_WHEEL_DOLLY_MAX_PENDING_LINES,
            MOUSE_WHEEL_DOLLY_MAX_PENDING_LINES,
        );
        self.target_lines = self.current_lines + pending_lines;
    }

    fn advance(&mut self, frame_delta_time: f32) -> f32 {
        if frame_delta_time <= f32::EPSILON || !frame_delta_time.is_finite() {
            return 0.0;
        }

        let remaining_lines = self.target_lines - self.current_lines;
        if remaining_lines.abs() <= MOUSE_WHEEL_DOLLY_SNAP_LINES {
            self.reset();
            return 0.0;
        }

        let alpha = (1.0 - (-MOUSE_WHEEL_DOLLY_INTERPOLATION_RATE * frame_delta_time).exp())
            .clamp(0.0, 1.0);
        let mut advanced_lines = remaining_lines * alpha;
        self.current_lines += advanced_lines;

        let remaining_after_advance = self.target_lines - self.current_lines;
        if remaining_after_advance.abs() <= MOUSE_WHEEL_DOLLY_SNAP_LINES {
            advanced_lines += remaining_after_advance;
            self.reset();
        }

        advanced_lines
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OrbitDragKind {
    Pan,
    Rotate,
}

#[derive(Clone, Copy, Debug)]
struct OrbitDrag {
    button: MouseButton,
    kind: OrbitDragKind,
    last_position_physical: Option<Vec2>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OrbitMotion {
    pub(super) pan_delta: Vec3,
    pub(super) rotation_delta: Vec3,
}

pub(super) struct CameraControlRuntime {
    mode: CameraControlMode,
    orbit_focus: Vec3,
    keyboard_pan: OrbitKeyboardPanInput,
    orbit_drag: Option<OrbitDrag>,
    pan_smoother: OrbitDeltaSmoother,
    rotation_smoother: OrbitDeltaSmoother,
    dolly_smoother: MouseWheelDollySmoother,
    accumulated_mouse_delta: Vec2,
    smoothed_mouse_delta: Vec2,
}

impl Default for CameraControlRuntime {
    fn default() -> Self {
        Self {
            mode: CameraControlMode::default(),
            orbit_focus: ORBIT_CAMERA_DEFAULT_FOCUS,
            keyboard_pan: OrbitKeyboardPanInput::default(),
            orbit_drag: None,
            pan_smoother: OrbitDeltaSmoother::default(),
            rotation_smoother: OrbitDeltaSmoother::default(),
            dolly_smoother: MouseWheelDollySmoother::default(),
            accumulated_mouse_delta: Vec2::ZERO,
            smoothed_mouse_delta: Vec2::ZERO,
        }
    }
}

impl CameraControlRuntime {
    pub(super) fn is_free_look(&self) -> bool {
        matches!(
            self.mode,
            CameraControlMode::FreeFly | CameraControlMode::Walk
        )
    }

    pub(super) fn is_free_fly(&self) -> bool {
        self.mode == CameraControlMode::FreeFly
    }

    pub(super) fn is_walk(&self) -> bool {
        self.mode == CameraControlMode::Walk
    }

    pub(super) fn is_orbit_edit(&self) -> bool {
        self.mode == CameraControlMode::OrbitEdit
    }

    pub(super) fn cycle_mode(&mut self) -> bool {
        self.mode = self.mode.next();
        self.is_orbit_edit()
    }

    pub(super) fn apply_snapshot_mode(&mut self, fly_mode: bool) {
        self.mode = if fly_mode {
            CameraControlMode::FreeFly
        } else {
            CameraControlMode::Walk
        };
        self.accumulated_mouse_delta = Vec2::ZERO;
        self.smoothed_mouse_delta = Vec2::ZERO;
    }

    pub(super) fn set_orbit_focus(&mut self, focus: Vec3) {
        if focus.is_finite() {
            self.orbit_focus = focus;
        }
    }

    pub(super) fn sync_focus_from_view_ray(
        &mut self,
        ray_origin: Vec3,
        ray_direction: Vec3,
        terrain_hit: Option<Vec3>,
    ) -> bool {
        let Some(focus) =
            orbit_focus_from_view_ray(ray_origin, ray_direction, terrain_hit, self.orbit_focus)
        else {
            return false;
        };
        self.orbit_focus = focus;
        true
    }

    pub(super) fn acquire_focus_from_terrain_hit(
        &mut self,
        ray_origin: Vec3,
        ray_direction: Vec3,
        terrain_hit: Option<Vec3>,
    ) -> bool {
        let Some(focus) = valid_terrain_focus(ray_origin, ray_direction, terrain_hit) else {
            return false;
        };
        self.orbit_focus = focus;
        true
    }

    pub(super) fn orbit_spherical(&self, camera_position: Vec3) -> (f32, f32, f32) {
        orbit_offset_to_spherical(camera_position - self.orbit_focus)
    }

    pub(super) fn orbit_pose(&self, azimuth: f32, elevation: f32, distance: f32) -> (Vec3, Vec3) {
        let azimuth = if azimuth.is_finite() { azimuth } else { 0.0 };
        let elevation = if elevation.is_finite() {
            elevation.clamp(
                -ORBIT_CAMERA_MAX_ELEVATION_RAD,
                ORBIT_CAMERA_MAX_ELEVATION_RAD,
            )
        } else {
            0.0
        };
        let distance = distance.clamp(ORBIT_CAMERA_MIN_DISTANCE, ORBIT_CAMERA_MAX_DISTANCE);
        let horizontal_radius = distance * elevation.cos();
        let position = self.orbit_focus
            + Vec3::new(
                azimuth.sin() * horizontal_radius,
                elevation.sin() * distance,
                azimuth.cos() * horizontal_radius,
            );
        (position, self.orbit_focus)
    }

    pub(super) fn focus_look_at_pose(
        &self,
        mut camera_position: Vec3,
        camera_front: Vec3,
    ) -> (Vec3, Vec3) {
        let offset = camera_position - self.orbit_focus;
        if offset.length_squared() <= ORBIT_CAMERA_MIN_DISTANCE.powi(2) {
            let fallback = -camera_front.normalize_or_zero();
            let fallback = if fallback.length_squared() > f32::EPSILON {
                fallback
            } else {
                Vec3::Z
            };
            camera_position = self.orbit_focus + fallback * ORBIT_CAMERA_MIN_DISTANCE;
        }
        (camera_position, self.orbit_focus)
    }

    pub(super) fn translate_orbit_focus(&mut self, delta: Vec3) -> Vec3 {
        if delta.is_finite() {
            self.orbit_focus += delta;
        }
        self.orbit_focus
    }

    pub(super) fn reset_motion(&mut self) {
        self.keyboard_pan.reset();
        self.reset_mode_transition_motion();
    }

    pub(super) fn reset_mode_transition_motion(&mut self) {
        self.orbit_drag = None;
        self.pan_smoother.reset();
        self.rotation_smoother.reset();
        self.dolly_smoother.reset();
    }

    pub(super) fn accumulate_free_look_mouse_delta(&mut self, delta: Vec2) {
        if delta.is_finite() {
            self.accumulated_mouse_delta += delta;
        }
    }

    pub(super) fn take_smoothed_free_look_mouse_delta(&mut self) -> Vec2 {
        let mouse_delta = std::mem::take(&mut self.accumulated_mouse_delta);
        self.smoothed_mouse_delta = self.smoothed_mouse_delta * FREE_LOOK_MOUSE_SMOOTHING
            + mouse_delta * (1.0 - FREE_LOOK_MOUSE_SMOOTHING);
        self.smoothed_mouse_delta
    }

    pub(super) fn handle_orbit_keyboard_input(&mut self, code: KeyCode, state: ElementState) {
        self.keyboard_pan
            .handle_key(code, state == ElementState::Pressed);
    }

    pub(super) fn queue_orbit_keyboard_pan(
        &mut self,
        camera_front: Vec3,
        camera_position: Vec3,
        frame_delta_time: f32,
        available: bool,
    ) {
        if !available || frame_delta_time <= f32::EPSILON || !frame_delta_time.is_finite() {
            self.keyboard_pan.reset();
            return;
        }

        let input = self.keyboard_pan.input_vector();
        if input.length_squared() <= f32::EPSILON {
            return;
        }

        let (planar_front, planar_right) = orbit_planar_camera_axes(camera_front);
        let distance = (camera_position - self.orbit_focus).length();
        let speed = orbit_keyboard_pan_speed(distance);
        let pan_delta = (planar_right * input.x + Vec3::Y * input.y + planar_front * input.z)
            * speed
            * frame_delta_time;
        self.pan_smoother.add_delta(pan_delta);
    }

    pub(super) fn advance_orbit_motion(
        &mut self,
        frame_delta_time: f32,
        available: bool,
    ) -> OrbitMotion {
        if !available {
            self.orbit_drag = None;
            self.pan_smoother.reset();
            self.rotation_smoother.reset();
            return OrbitMotion::default();
        }
        OrbitMotion {
            pan_delta: self.pan_smoother.advance(frame_delta_time),
            rotation_delta: self.rotation_smoother.advance(frame_delta_time),
        }
    }

    pub(super) fn begin_orbit_pan_drag(
        &mut self,
        button: MouseButton,
        position_physical: Option<Vec2>,
    ) {
        self.rotation_smoother.reset();
        self.orbit_drag = Some(OrbitDrag {
            button,
            kind: OrbitDragKind::Pan,
            last_position_physical: position_physical,
        });
    }

    pub(super) fn begin_orbit_rotation_drag(
        &mut self,
        button: MouseButton,
        position_physical: Option<Vec2>,
    ) {
        self.pan_smoother.reset();
        self.rotation_smoother.reset();
        self.orbit_drag = Some(OrbitDrag {
            button,
            kind: OrbitDragKind::Rotate,
            last_position_physical: position_physical,
        });
    }

    pub(super) fn end_orbit_drag(&mut self, button: MouseButton) -> bool {
        if self.orbit_drag.is_some_and(|drag| drag.button == button) {
            self.orbit_drag = None;
            true
        } else {
            false
        }
    }

    pub(super) fn sync_orbit_drag_position(&mut self, position_physical: Vec2) {
        if let Some(drag) = self.orbit_drag.as_mut() {
            drag.last_position_physical = Some(position_physical);
        }
    }

    pub(super) fn handle_orbit_drag(
        &mut self,
        position_physical: Vec2,
        available: bool,
        camera_front: Vec3,
        current_elevation: f32,
    ) {
        let Some(drag) = self.orbit_drag.as_mut() else {
            return;
        };
        if !available {
            self.orbit_drag = None;
            self.pan_smoother.reset();
            self.rotation_smoother.reset();
            return;
        }

        let Some(previous_position) = drag.last_position_physical.replace(position_physical) else {
            return;
        };
        let drag_delta = position_physical - previous_position;
        if drag_delta.length_squared() <= f32::EPSILON {
            return;
        }

        match drag.kind {
            OrbitDragKind::Pan => self
                .pan_smoother
                .add_delta(orbit_focus_pan_delta(drag_delta, camera_front)),
            OrbitDragKind::Rotate => {
                let mut rotation_delta = orbit_rotation_delta(drag_delta);
                rotation_delta.y = clamp_orbit_elevation_delta(
                    current_elevation,
                    self.rotation_smoother.pending_delta().y,
                    rotation_delta.y,
                );
                self.rotation_smoother.add_delta(rotation_delta);
            }
        }
    }

    pub(super) fn queue_mouse_wheel_dolly(&mut self, scroll_lines: f32) {
        self.dolly_smoother.add_scroll_lines(scroll_lines);
    }

    pub(super) fn advance_orbit_dolly(
        &mut self,
        frame_delta_time: f32,
        available: bool,
        camera_position: Vec3,
    ) -> Option<(Vec3, Vec3)> {
        if !available {
            self.dolly_smoother.reset();
            return None;
        }
        let scroll_lines = self.dolly_smoother.advance(frame_delta_time);
        if scroll_lines.abs() <= f32::EPSILON {
            return None;
        }

        let forward_distance =
            scroll_lines * ORBIT_CAMERA_DOLLY_SPEED * MOUSE_WHEEL_DOLLY_SECONDS_PER_LINE;
        let (azimuth, elevation, distance) = self.orbit_spherical(camera_position);
        Some(self.orbit_pose(azimuth, elevation, distance - forward_distance))
    }
}

fn orbit_offset_to_spherical(mut offset: Vec3) -> (f32, f32, f32) {
    if !offset.is_finite() || offset.length_squared() <= f32::EPSILON {
        offset = Vec3::Z * ORBIT_CAMERA_MIN_DISTANCE;
    }

    let distance = offset
        .length()
        .clamp(ORBIT_CAMERA_MIN_DISTANCE, ORBIT_CAMERA_MAX_DISTANCE);
    let mut elevation = (offset.y / distance).asin().clamp(
        -ORBIT_CAMERA_MAX_ELEVATION_RAD,
        ORBIT_CAMERA_MAX_ELEVATION_RAD,
    );
    if !elevation.is_finite() {
        elevation = 0.0;
    }
    (offset.x.atan2(offset.z), elevation, distance)
}

fn orbit_planar_camera_axes(camera_front: Vec3) -> (Vec3, Vec3) {
    let mut planar_front = Vec3::new(camera_front.x, 0.0, camera_front.z).normalize_or_zero();
    if planar_front.length_squared() <= f32::EPSILON {
        planar_front = -Vec3::Z;
    }
    let planar_right = Vec3::new(-planar_front.z, 0.0, planar_front.x);
    (planar_front, planar_right)
}

fn orbit_focus_pan_delta(drag_delta_physical: Vec2, camera_front: Vec3) -> Vec3 {
    if !drag_delta_physical.is_finite() || drag_delta_physical.length_squared() <= f32::EPSILON {
        return Vec3::ZERO;
    }
    let (planar_front, planar_right) = orbit_planar_camera_axes(camera_front);
    (planar_front * drag_delta_physical.y - planar_right * drag_delta_physical.x)
        * ORBIT_CAMERA_MOUSE_PAN_UNITS_PER_PHYSICAL_PIXEL
}

fn orbit_rotation_delta(drag_delta_physical: Vec2) -> Vec3 {
    if !drag_delta_physical.is_finite() || drag_delta_physical.length_squared() <= f32::EPSILON {
        return Vec3::ZERO;
    }
    Vec3::new(-drag_delta_physical.x, drag_delta_physical.y, 0.0)
        * ORBIT_CAMERA_MOUSE_DRAG_RADIANS_PER_PIXEL
}

fn clamp_orbit_elevation_delta(
    current_elevation: f32,
    pending_elevation_delta: f32,
    requested_elevation_delta: f32,
) -> f32 {
    let target_elevation =
        (current_elevation + pending_elevation_delta + requested_elevation_delta).clamp(
            -ORBIT_CAMERA_MAX_ELEVATION_RAD,
            ORBIT_CAMERA_MAX_ELEVATION_RAD,
        );
    target_elevation - current_elevation - pending_elevation_delta
}

fn orbit_keyboard_pan_speed(distance: f32) -> f32 {
    let distance = distance.clamp(ORBIT_CAMERA_MIN_DISTANCE, ORBIT_CAMERA_MAX_DISTANCE);
    let zoom_range = ORBIT_CAMERA_MAX_DISTANCE - ORBIT_CAMERA_MIN_DISTANCE;
    let normalized_zoom = ((distance - ORBIT_CAMERA_MIN_DISTANCE) / zoom_range).clamp(0.0, 1.0);
    let far_zoom_progress = ((normalized_zoom - ORBIT_CAMERA_KEYBOARD_PAN_FAR_ZOOM_BOOST_START)
        / (1.0 - ORBIT_CAMERA_KEYBOARD_PAN_FAR_ZOOM_BOOST_START))
        .clamp(0.0, 1.0);
    let far_zoom_easing = far_zoom_progress * far_zoom_progress * (3.0 - 2.0 * far_zoom_progress);
    let far_zoom_multiplier =
        1.0 + (ORBIT_CAMERA_KEYBOARD_PAN_FAR_ZOOM_MAX_MULTIPLIER - 1.0) * far_zoom_easing;
    distance * ORBIT_CAMERA_KEYBOARD_PAN_UNITS_PER_SECOND_AT_UNIT_DISTANCE * far_zoom_multiplier
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
    if let Some(hit) = valid_terrain_focus(ray_origin, ray_direction, terrain_hit) {
        return Some(hit);
    }

    let previous_distance = (previous_focus - ray_origin).length();
    let fallback_distance = if previous_distance.is_finite() && previous_distance > f32::EPSILON {
        previous_distance
    } else {
        1.0
    }
    .clamp(ORBIT_CAMERA_MIN_DISTANCE, ORBIT_CAMERA_MAX_DISTANCE);
    Some(ray_origin + ray_direction * fallback_distance)
}

fn valid_terrain_focus(
    ray_origin: Vec3,
    ray_direction: Vec3,
    terrain_hit: Option<Vec3>,
) -> Option<Vec3> {
    if !ray_origin.is_finite()
        || !ray_direction.is_finite()
        || ray_direction.length_squared() <= f32::EPSILON
    {
        return None;
    }
    let hit = terrain_hit?;
    let ray_direction = ray_direction.normalize();
    let hit_offset = hit - ray_origin;
    (hit.is_finite()
        && hit_offset.is_finite()
        && hit_offset.length() <= ORBIT_CAMERA_FOCUS_RAY_QUERY_DISTANCE
        && hit_offset.dot(ray_direction) > 0.0)
        .then_some(hit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.0001,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn mode_defaults_to_orbit_and_cycles_through_fly_and_walk() {
        let mut runtime = CameraControlRuntime::default();
        assert!(runtime.is_orbit_edit());
        assert!(!runtime.cycle_mode());
        assert!(runtime.is_free_fly());
        assert!(!runtime.cycle_mode());
        assert!(runtime.is_walk());
        assert!(runtime.cycle_mode());
        assert!(runtime.is_orbit_edit());
    }

    #[test]
    fn keyboard_pan_normalizes_diagonal_motion() {
        let mut input = OrbitKeyboardPanInput::default();
        input.handle_key(KeyCode::KeyW, true);
        input.handle_key(KeyCode::KeyD, true);
        let direction = input.input_vector();
        assert!((direction.length() - 1.0).abs() <= 0.0001);
        assert!(direction.x > 0.0);
        assert!(direction.z > 0.0);
    }

    #[test]
    fn mode_transition_reset_preserves_held_keyboard_input() {
        let mut runtime = CameraControlRuntime::default();
        runtime.handle_orbit_keyboard_input(KeyCode::KeyW, ElementState::Pressed);

        runtime.reset_mode_transition_motion();

        assert_eq!(runtime.keyboard_pan.input_vector(), Vec3::Z);
    }

    #[test]
    fn drag_state_has_one_owner_and_matching_release() {
        let mut runtime = CameraControlRuntime::default();
        runtime.begin_orbit_pan_drag(MouseButton::Middle, Some(Vec2::new(1.0, 2.0)));
        runtime.begin_orbit_rotation_drag(MouseButton::Right, Some(Vec2::new(3.0, 4.0)));
        assert!(!runtime.end_orbit_drag(MouseButton::Middle));
        runtime.sync_orbit_drag_position(Vec2::new(3.0, 4.0));
        assert!(runtime.end_orbit_drag(MouseButton::Right));
        assert!(!runtime.end_orbit_drag(MouseButton::Right));
    }

    #[test]
    fn orbit_spherical_preserves_angles_at_minimum_distance() {
        let (azimuth, elevation, distance) =
            orbit_offset_to_spherical(Vec3::X * ORBIT_CAMERA_MIN_DISTANCE);
        assert_near(azimuth, std::f32::consts::FRAC_PI_2);
        assert_near(elevation, 0.0);
        assert_near(distance, ORBIT_CAMERA_MIN_DISTANCE);
    }

    #[test]
    fn orbit_focus_pan_delta_uses_camera_planar_axes() {
        let delta = orbit_focus_pan_delta(Vec2::new(10.0, -20.0), -Vec3::Z);
        assert_near(
            delta.x,
            -10.0 * ORBIT_CAMERA_MOUSE_PAN_UNITS_PER_PHYSICAL_PIXEL,
        );
        assert_near(delta.y, 0.0);
        assert_near(
            delta.z,
            20.0 * ORBIT_CAMERA_MOUSE_PAN_UNITS_PER_PHYSICAL_PIXEL,
        );
    }

    #[test]
    fn orbit_focus_pan_delta_stays_on_xz_plane() {
        let delta = orbit_focus_pan_delta(Vec2::new(24.0, -16.0), Vec3::new(0.2, -0.9, -0.4));
        assert_near(delta.y, 0.0);
        assert!(delta.x.abs() > 0.0 || delta.z.abs() > 0.0);
    }

    #[test]
    fn orbit_rotation_delta_maps_mouse_axes_to_azimuth_and_elevation() {
        let delta = orbit_rotation_delta(Vec2::new(10.0, -20.0));
        assert_near(delta.x, -10.0 * ORBIT_CAMERA_MOUSE_DRAG_RADIANS_PER_PIXEL);
        assert_near(delta.y, -20.0 * ORBIT_CAMERA_MOUSE_DRAG_RADIANS_PER_PIXEL);
        assert_near(delta.z, 0.0);
    }

    #[test]
    fn orbit_rotation_smoothing_respects_elevation_limits() {
        let max = ORBIT_CAMERA_MAX_ELEVATION_RAD;
        assert_near(clamp_orbit_elevation_delta(max - 0.1, 0.0, 0.3), 0.1);
        assert_near(clamp_orbit_elevation_delta(-max + 0.1, 0.0, -0.3), -0.1);
        assert_near(clamp_orbit_elevation_delta(max - 0.2, 0.1, 0.3), 0.1);
    }

    #[test]
    fn orbit_keyboard_pan_accelerates_only_at_far_zoom() {
        assert_near(orbit_keyboard_pan_speed(1.0), 0.9);
        let boost_start_distance = ORBIT_CAMERA_MIN_DISTANCE
            + (ORBIT_CAMERA_MAX_DISTANCE - ORBIT_CAMERA_MIN_DISTANCE)
                * ORBIT_CAMERA_KEYBOARD_PAN_FAR_ZOOM_BOOST_START;
        assert_near(
            orbit_keyboard_pan_speed(boost_start_distance),
            boost_start_distance * ORBIT_CAMERA_KEYBOARD_PAN_UNITS_PER_SECOND_AT_UNIT_DISTANCE,
        );
        let near_far_edge = orbit_keyboard_pan_speed(4.8);
        let maximum = orbit_keyboard_pan_speed(ORBIT_CAMERA_MAX_DISTANCE);
        assert!(maximum > near_far_edge * 1.5);
        assert_near(maximum, 7.2);

        let mut previous = orbit_keyboard_pan_speed(ORBIT_CAMERA_MIN_DISTANCE);
        for step in 1..=48 {
            let distance = ORBIT_CAMERA_MIN_DISTANCE + step as f32 * 0.1;
            let speed = orbit_keyboard_pan_speed(distance);
            assert!(speed >= previous);
            previous = speed;
        }
    }

    #[test]
    fn focus_from_view_ray_prefers_hit_and_falls_back_along_view() {
        let origin = Vec3::new(0.0, 1.0, 0.0);
        let direction = Vec3::new(0.0, -0.5, -1.0).normalize();
        let hit = origin + direction * 1.75;
        let previous_focus = origin + Vec3::Z * 2.0;
        assert_eq!(
            orbit_focus_from_view_ray(origin, direction, Some(hit), previous_focus),
            Some(hit)
        );
        let fallback = orbit_focus_from_view_ray(origin, direction, None, previous_focus).unwrap();
        let focus_direction = (fallback - origin).normalize();
        assert_near(focus_direction.x, direction.x);
        assert_near(focus_direction.y, direction.y);
        assert_near(focus_direction.z, direction.z);
        assert_near((fallback - origin).length(), 2.0);
    }

    #[test]
    fn orbit_delta_smoother_eases_and_preserves_full_delta() {
        let mut smoother = OrbitDeltaSmoother::default();
        let target_delta = Vec3::new(0.25, 0.0, -0.1);
        smoother.add_delta(target_delta);
        let mut total_advanced = smoother.advance(1.0 / 60.0);
        assert!(total_advanced.length() < target_delta.length());
        for _ in 0..120 {
            total_advanced += smoother.advance(1.0 / 60.0);
        }
        assert!((total_advanced - target_delta).length() <= 0.0001);
        assert_eq!(smoother.current_delta, Vec3::ZERO);
        assert_eq!(smoother.target_delta, Vec3::ZERO);
    }

    #[test]
    fn orbit_delta_smoother_preserves_continuous_keyboard_distance() {
        let mut smoother = OrbitDeltaSmoother::default();
        let frame_delta_time = 1.0 / 60.0;
        let velocity = Vec3::new(0.9, 0.0, -0.4);
        let mut total_advanced = Vec3::ZERO;

        for _ in 0..60 {
            smoother.add_delta(velocity * frame_delta_time);
            total_advanced += smoother.advance(frame_delta_time);
        }
        assert!(total_advanced.length() < velocity.length());

        for _ in 0..120 {
            total_advanced += smoother.advance(frame_delta_time);
        }
        assert!((total_advanced - velocity).length() <= 0.0001);
        assert_eq!(smoother.current_delta, Vec3::ZERO);
        assert_eq!(smoother.target_delta, Vec3::ZERO);
    }

    #[test]
    fn dolly_smoother_interpolates_toward_target() {
        let mut smoother = MouseWheelDollySmoother::default();
        smoother.add_scroll_lines(1.0);
        let first_step = smoother.advance(1.0 / 60.0);
        assert!(first_step > 0.0);
        assert!(first_step < 1.0);

        let mut total_advanced = first_step;
        for _ in 0..120 {
            total_advanced += smoother.advance(1.0 / 60.0);
        }
        assert!((total_advanced - 1.0).abs() <= 0.0001);
        assert_eq!(smoother.current_lines, 0.0);
        assert_eq!(smoother.target_lines, 0.0);
    }

    #[test]
    fn dolly_smoother_clamps_and_preserves_scroll() {
        let mut smoother = MouseWheelDollySmoother::default();
        smoother.add_scroll_lines(100.0);
        assert_eq!(
            smoother.target_lines - smoother.current_lines,
            MOUSE_WHEEL_DOLLY_MAX_PENDING_LINES
        );
        let mut total_advanced = 0.0;
        for _ in 0..180 {
            total_advanced += smoother.advance(1.0 / 60.0);
        }
        assert!((total_advanced - MOUSE_WHEEL_DOLLY_MAX_PENDING_LINES).abs() <= 0.0001);

        smoother.add_scroll_lines(1.0);
        smoother.advance(1.0 / 60.0);
        smoother.add_scroll_lines(-100.0);
        assert_eq!(
            smoother.target_lines - smoother.current_lines,
            -MOUSE_WHEEL_DOLLY_MAX_PENDING_LINES
        );
    }
}
