use glam::Vec3;
use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{KeyCode, PhysicalKey},
};

#[derive(Debug)]
pub struct AxesState {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

impl Default for AxesState {
    fn default() -> Self {
        Self {
            forward: false,
            backward: false,
            left: false,
            right: false,
            up: false,
            down: false,
        }
    }
}

/// Stores the current state of the camera's movement.
#[derive(Debug)]
pub struct MovementState {
    normal_speed: f32,
    boosted_speed_mul: f32,
    pub is_boosted: bool,
    pub axes: AxesState,
    pub jump_requested: bool,
}

impl MovementState {
    pub fn new(normal_speed: f32, boosted_speed_mul: f32) -> Self {
        Self {
            normal_speed,
            boosted_speed_mul,
            is_boosted: false,
            axes: AxesState::default(),
            jump_requested: false,
        }
    }

    pub fn get_velocity(&self, front: Vec3, right: Vec3, up: Vec3) -> Vec3 {
        let mut velocity = Vec3::ZERO;
        if self.axes.forward {
            velocity += front;
        }
        if self.axes.backward {
            velocity -= front;
        }
        if self.axes.left {
            velocity -= right;
        }
        if self.axes.right {
            velocity += right;
        }
        if self.axes.up {
            velocity += up;
        }
        if self.axes.down {
            velocity -= up;
        }
        velocity.normalize_or_zero() * self.current_speed()
    }

    pub fn current_speed(&self) -> f32 {
        if self.is_boosted {
            self.normal_speed * self.boosted_speed_mul
        } else {
            self.normal_speed
        }
    }

    /// Handles keyboard input for movement controls
    pub fn handle_keyboard(&mut self, key_event: &KeyEvent) {
        if let PhysicalKey::Code(code) = key_event.physical_key {
            if key_event.repeat {
                return;
            }

            match key_event.state {
                ElementState::Pressed => match code {
                    KeyCode::ShiftLeft => self.is_boosted = true,
                    KeyCode::KeyW => self.axes.forward = true,
                    KeyCode::KeyS => self.axes.backward = true,
                    KeyCode::KeyA => self.axes.left = true,
                    KeyCode::KeyD => self.axes.right = true,
                    KeyCode::Space => {
                        self.axes.up = true;
                        self.jump_requested = true;
                    }
                    KeyCode::ControlLeft => self.axes.down = true,
                    _ => {}
                },
                ElementState::Released => match code {
                    KeyCode::ShiftLeft => self.is_boosted = false,
                    KeyCode::KeyW => self.axes.forward = false,
                    KeyCode::KeyS => self.axes.backward = false,
                    KeyCode::KeyA => self.axes.left = false,
                    KeyCode::KeyD => self.axes.right = false,
                    KeyCode::Space => self.axes.up = false,
                    KeyCode::ControlLeft => self.axes.down = false,
                    _ => {}
                },
            }
        }
    }

    /// Resets the jump request flag after it's been processed
    pub fn reset_jump_request(&mut self) {
        self.jump_requested = false;
    }

    /// Checks if the player is currently moving horizontally
    pub fn is_moving_horizontally(&self) -> bool {
        self.axes.forward || self.axes.backward || self.axes.left || self.axes.right
    }
}
