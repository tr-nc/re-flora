use super::CameraDesc;
use crate::{
    audio::{AudioEngine, ClipCache, SoundDataConfig},
    vkn::Extent2D,
};
use anyhow::Result;
use glam::{Mat4, Vec2, Vec3, Vec4};
use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{KeyCode, PhysicalKey},
};

pub struct PlayerClipCaches {
    pub walk: ClipCache,
    pub jump: ClipCache,
    pub land: ClipCache,
    pub sneak: ClipCache,
    pub run: ClipCache,
    pub sprint: ClipCache,

    // foot-step intervals (seconds)
    pub walk_interval: f32,
    pub run_interval: f32,
}

impl PlayerClipCaches {
    fn new() -> Result<Self> {
        let jump = Self::load_clip_cache("jump", 10)?;
        let land = Self::load_clip_cache("land", 10)?;
        let walk = Self::load_clip_cache("walk", 25)?;
        let sneak = Self::load_clip_cache("sneak", 25)?;
        let run = Self::load_clip_cache("run", 25)?;
        let sprint = Self::load_clip_cache("sprint", 25)?;

        Ok(Self {
            walk,
            jump,
            land,
            sneak,
            run,
            sprint,
            walk_interval: 0.35,
            run_interval: 0.25,
        })
    }

    fn load_clip_cache(sample_name: &str, sample_count: usize) -> Result<ClipCache> {
        let prefix_path = "assets/sfx/raw/Footsteps SFX - Undergrowth & Leaves/TomWinandySFX - FS_UndergrowthLeaves_";
        let clip_paths: Vec<String> = (0..sample_count)
            .map(|i| {
                format!(
                    "{}{}_{}.wav",
                    prefix_path,
                    sample_name,
                    format!("{:02}", i + 1)
                )
            })
            .collect();
        let clip_cache = ClipCache::from_files(
            &clip_paths,
            SoundDataConfig {
                volume: -10.0,
                ..Default::default()
            },
        )?;
        Ok(clip_cache)
    }
}

pub struct PlayerAudioController {
    audio_engine: AudioEngine,
    clip_caches: PlayerClipCaches,
    // time elapsed since last step sound
    time_since_last_step: f32,
}

impl PlayerAudioController {
    pub fn new(audio_engine: AudioEngine) -> Result<Self> {
        let clip_caches = PlayerClipCaches::new()?;
        Ok(Self {
            audio_engine,
            clip_caches,
            time_since_last_step: 0.0,
        })
    }

    pub fn play_jump(&mut self) {
        let clip = self.clip_caches.jump.next();
        self.audio_engine.play(&clip).unwrap();
    }

    pub fn play_land(&mut self) {
        let clip = self.clip_caches.land.next();
        self.audio_engine.play(&clip).unwrap();
    }

    /// Call this once per frame from the camera update.
    pub fn update_walk_sound(
        &mut self,
        is_on_ground: bool,
        is_moving: bool,
        is_running: bool,
        frame_delta_time: f32,
    ) {
        let interval = if is_running {
            self.clip_caches.run_interval
        } else {
            self.clip_caches.walk_interval
        };

        if !(is_on_ground && is_moving) {
            self.time_since_last_step = interval;
            return;
        }

        self.time_since_last_step += frame_delta_time;
        if self.time_since_last_step >= interval {
            let cache = if is_running {
                &mut self.clip_caches.run
            } else {
                &mut self.clip_caches.walk
            };
            let clip = cache.next();
            self.audio_engine.play(&clip).unwrap();
            self.time_since_last_step = 0.0;
        }
    }
}

#[derive(Debug)]
struct AxesState {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
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
struct MovementState {
    normal_speed: f32,
    boosted_speed_mul: f32,
    is_boosted: bool,
    axes: AxesState,
    jump_requested: bool,
}

impl MovementState {
    fn new(normal_speed: f32, boosted_speed_mul: f32) -> Self {
        Self {
            normal_speed,
            boosted_speed_mul,
            is_boosted: false,
            axes: AxesState::default(),
            jump_requested: false,
        }
    }

    fn get_velocity(&self, front: Vec3, right: Vec3, up: Vec3) -> Vec3 {
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

    fn current_speed(&self) -> f32 {
        if self.is_boosted {
            self.normal_speed * self.boosted_speed_mul
        } else {
            self.normal_speed
        }
    }
}

struct CameraVectors {
    front: Vec3,
    up: Vec3,
    right: Vec3,
}

impl CameraVectors {
    const WORLD_UP: Vec3 = Vec3::new(0.0, 1.0, 0.0);

    fn new() -> Self {
        Self {
            front: Vec3::ZERO,
            up: Vec3::ZERO,
            right: Vec3::ZERO,
        }
    }

    /// Updates the camera's front, right, and up vectors based on the current yaw and pitch.
    fn update(&mut self, yaw: f32, pitch: f32) {
        self.front = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        )
        .normalize();
        self.right = self.front.cross(Self::WORLD_UP).normalize();
        self.up = self.right.cross(self.front).normalize();
    }
}

pub struct Camera {
    position: Vec3,

    /// The initial yaw of the camera in radians.
    yaw: f32,

    /// The initial pitch of the camera in radians.
    pitch: f32,

    vectors: CameraVectors,
    movement_state: MovementState,
    desc: CameraDesc,

    /// vertical velocity used by walk/gravity mode (m/s, +y up)
    vertical_velocity: f32,

    player_audio_controller: PlayerAudioController,
    was_on_ground: bool,
}

impl Camera {
    pub fn new(
        initial_position: Vec3,
        initial_yaw: f32,
        initial_pitch: f32,
        desc: CameraDesc,
        audio_engine: AudioEngine,
    ) -> Result<Self> {
        let mut camera = Self {
            position: initial_position,
            vectors: CameraVectors::new(),
            yaw: initial_yaw.to_radians(),
            pitch: initial_pitch.to_radians(),
            movement_state: MovementState::new(
                desc.movement.normal_speed,
                desc.movement.boosted_speed_mul,
            ),
            desc,
            vertical_velocity: 0.0,
            player_audio_controller: PlayerAudioController::new(audio_engine)?,
            was_on_ground: false,
        };

        camera.vectors.update(camera.yaw, camera.pitch);
        Ok(camera)
    }

    pub fn on_resize(&mut self, screen_extent: Extent2D) {
        self.desc.aspect_ratio = screen_extent.width as f32 / screen_extent.height as f32;
    }

    #[allow(dead_code)]
    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// Returns the camera's position as a Vec4 with the w component set to 1.0.
    #[allow(dead_code)]
    pub fn position_vec4(&self) -> Vec4 {
        Vec4::new(self.position.x, self.position.y, self.position.z, 1.0)
    }

    pub fn get_view_mat(&self) -> Mat4 {
        Mat4::look_at_rh(
            self.position,
            self.position + self.vectors.front,
            self.vectors.up,
        )
    }

    pub fn calculate_proj_mat(v_fov: f32, aspect_ratio: f32, z_near: f32, z_far: f32) -> Mat4 {
        let proj = Mat4::perspective_rh(v_fov.to_radians(), aspect_ratio, z_near, z_far);
        let flip_y = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
        flip_y * proj
    }

    pub fn get_proj_mat(&self) -> Mat4 {
        Self::calculate_proj_mat(
            self.desc.projection.v_fov,
            self.desc.aspect_ratio,
            self.desc.projection.z_near,
            self.desc.projection.z_far,
        )
    }

    /// Only controls the camera's movement state based on the key event.
    pub fn handle_keyboard(&mut self, key_event: &KeyEvent) {
        if let PhysicalKey::Code(code) = key_event.physical_key {
            if key_event.repeat {
                return;
            }

            match key_event.state {
                ElementState::Pressed => match code {
                    KeyCode::ShiftLeft => self.movement_state.is_boosted = true,
                    KeyCode::KeyW => self.movement_state.axes.forward = true,
                    KeyCode::KeyS => self.movement_state.axes.backward = true,
                    KeyCode::KeyA => self.movement_state.axes.left = true,
                    KeyCode::KeyD => self.movement_state.axes.right = true,
                    KeyCode::Space => {
                        self.movement_state.axes.up = true;
                        self.movement_state.jump_requested = true;
                    }
                    KeyCode::ControlLeft => self.movement_state.axes.down = true,
                    _ => {}
                },
                ElementState::Released => match code {
                    KeyCode::ShiftLeft => self.movement_state.is_boosted = false,
                    KeyCode::KeyW => self.movement_state.axes.forward = false,
                    KeyCode::KeyS => self.movement_state.axes.backward = false,
                    KeyCode::KeyA => self.movement_state.axes.left = false,
                    KeyCode::KeyD => self.movement_state.axes.right = false,
                    KeyCode::Space => self.movement_state.axes.up = false,
                    KeyCode::ControlLeft => self.movement_state.axes.down = false,
                    _ => {}
                },
            }
        }
    }

    /// Limits the yaw to prevent the camera from spinning indefinitely.
    /// The yaw is clamped to the range (-π, π).
    fn limit_yaw(&mut self) {
        if self.yaw > std::f32::consts::PI {
            self.yaw -= 2.0 * std::f32::consts::PI;
        }
        if self.yaw < -std::f32::consts::PI {
            self.yaw += 2.0 * std::f32::consts::PI;
        }
    }

    /// Clamps the pitch to prevent the camera from flipping.
    fn clamp_pitch(&mut self) {
        const CAMERA_LIM_RAD: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
        if self.pitch > CAMERA_LIM_RAD {
            self.pitch = CAMERA_LIM_RAD;
        }
        if self.pitch < -CAMERA_LIM_RAD {
            self.pitch = -CAMERA_LIM_RAD;
        }
    }

    pub fn handle_mouse(&mut self, delta: Vec2) {
        const SENSITIVITY_MULTIPLIER: f32 = 0.001;
        // the delta is positive when moving the mouse to the right / down
        // so we need to invert the pitch delta so that when mouse is going up, pitch increases
        self.yaw += delta.x as f32 * self.desc.movement.mouse_sensitivity * SENSITIVITY_MULTIPLIER;
        self.pitch -=
            delta.y as f32 * self.desc.movement.mouse_sensitivity * SENSITIVITY_MULTIPLIER;

        self.limit_yaw();
        self.clamp_pitch();

        self.vectors.update(self.yaw, self.pitch);
    }

    fn movement_basis(&self) -> (Vec3, Vec3) {
        // discard vertical component so movement happens in world-space XZ plane
        let mut horizontal_front = Vec3::new(self.vectors.front.x, 0.0, self.vectors.front.z);
        // if the camera looks straight up/down, fallback to yaw to keep movement responsive
        if horizontal_front.length_squared() < f32::EPSILON {
            horizontal_front = Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos());
        }
        horizontal_front = horizontal_front.normalize();

        let horizontal_right = horizontal_front.cross(Vec3::Y).normalize();
        (horizontal_front, horizontal_right)
    }

    #[allow(dead_code)]
    pub fn update_transform_fly_mode(&mut self, frame_delta_time: f32) {
        // move in the camera's local axes (front/right/up)
        self.position += self.movement_state.get_velocity(
            self.vectors.front,
            self.vectors.right,
            self.vectors.up,
        ) * frame_delta_time;
    }

    pub fn update_transform_walk_mode(&mut self, frame_delta_time: f32, ground_distance: f32) {
        const GRAVITY_G: f32 = 2.0; // gravity acceleration (m/s²)
        const JUMP_IMPULSE: f32 = 0.4; // initial jump velocity (m/s)
        const GROUND_EPSILON: f32 = 0.01; // tolerance when comparing to ground
        const Y_SMOOTHING_ALPHA: f32 = 0.2; // fraction used to lerp camera height to ground

        // compute horizontal movement basis (XZ plane)
        let (front, right) = self.movement_basis();
        let horizontal_velocity = self.movement_state.get_velocity(front, right, Vec3::ZERO);

        // detect whether the player is on the ground
        let is_on_ground = ground_distance <= self.desc.camera_height + GROUND_EPSILON;

        // === vertical motion & jump handling ===
        if is_on_ground {
            // clamp any remaining downward velocity when touching ground
            if self.vertical_velocity < 0.0 {
                self.vertical_velocity = 0.0;
            }

            if self.movement_state.jump_requested {
                // launch the jump
                self.vertical_velocity = JUMP_IMPULSE;
                // play jump sound once, immediately when leaving the ground
                self.player_audio_controller.play_jump();
            } else {
                // stick to ground smoothly
                let ground_level_y = self.position.y - ground_distance;
                let target_camera_y = ground_level_y + self.desc.camera_height;
                self.position.y += (target_camera_y - self.position.y) * Y_SMOOTHING_ALPHA;
            }
        } else {
            // airborne: apply gravity
            self.vertical_velocity -= GRAVITY_G * frame_delta_time;
        }

        // reset the jump flag after use
        self.movement_state.jump_requested = false;

        // === integrate total velocity ===
        let total_velocity = horizontal_velocity + Vec3::new(0.0, self.vertical_velocity, 0.0);
        self.position += total_velocity * frame_delta_time;

        // === audio: foot-steps, jump, land ===
        // player considered "moving" when any horizontal axis key is pressed
        let is_moving = self.movement_state.axes.forward
            || self.movement_state.axes.backward
            || self.movement_state.axes.left
            || self.movement_state.axes.right;

        // "running" if boosted (Shift) – 用于选择 run vs. walk clip 及步频
        let is_running = self.movement_state.is_boosted;

        // per-frame update for walk / run sounds
        self.player_audio_controller.update_walk_sound(
            is_on_ground,
            is_moving,
            is_running,
            frame_delta_time,
        );

        // play landing sound once when transitioning air → ground
        if is_on_ground && !self.was_on_ground {
            self.player_audio_controller.play_land();
        }

        // remember current on-ground state for next frame
        self.was_on_ground = is_on_ground;
    }
}
