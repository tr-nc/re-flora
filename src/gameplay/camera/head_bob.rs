use super::CameraHeadBobDesc;
use glam::{Mat4, Vec3};
use std::f32::consts::TAU;

pub struct HeadBob {
    blend: f32,
    sprint_blend: f32,
    prev_step_phase: f32,
    step_parity: bool,
    pub offset_y: f32,
    pub offset_x: f32,
    pub roll_rad: f32,
}

impl HeadBob {
    pub fn new() -> Self {
        Self {
            blend: 0.0,
            sprint_blend: 0.0,
            prev_step_phase: 0.0,
            step_parity: false,
            offset_y: 0.0,
            offset_x: 0.0,
            roll_rad: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.blend = 0.0;
        self.sprint_blend = 0.0;
        self.prev_step_phase = 0.0;
        self.step_parity = false;
        self.offset_y = 0.0;
        self.offset_x = 0.0;
        self.roll_rad = 0.0;
    }

    pub fn update(
        &mut self,
        step_phase: f32,
        is_active: bool,
        is_running: bool,
        desc: &CameraHeadBobDesc,
        dt: f32,
    ) {
        let target = if is_active { 1.0 } else { 0.0 };
        let t = (desc.smoothing_speed * dt).clamp(0.0, 1.0);
        self.blend += (target - self.blend) * t;
        let sprint_target = if is_running { 1.0 } else { 0.0 };
        self.sprint_blend += (sprint_target - self.sprint_blend) * t;

        if is_active && step_phase < self.prev_step_phase {
            self.step_parity = !self.step_parity;
        }
        self.prev_step_phase = step_phase;

        let amp_mul = 1.0 + (desc.sprint_amplitude_mul - 1.0) * self.sprint_blend;
        let phase_rad = step_phase * TAU;

        let target_offset_y = phase_rad.sin() * desc.vertical_amplitude * amp_mul * self.blend;

        let lateral_sign = if self.step_parity { -1.0 } else { 1.0 };
        let lateral_wave = (phase_rad * 0.5).sin();
        let target_offset_x =
            lateral_wave * desc.horizontal_amplitude * amp_mul * self.blend * lateral_sign;

        let roll_amp_rad = desc.roll_amplitude_deg.to_radians();
        let target_roll_rad = lateral_wave * roll_amp_rad * amp_mul * self.blend * lateral_sign;

        self.offset_y += (target_offset_y - self.offset_y) * t;
        self.offset_x += (target_offset_x - self.offset_x) * t;
        self.roll_rad += (target_roll_rad - self.roll_rad) * t;
    }

    pub fn apply_to_view_mat(&self, view_mat: Mat4, right: Vec3, up: Vec3) -> Mat4 {
        if self.offset_x.abs() <= f32::EPSILON
            && self.offset_y.abs() <= f32::EPSILON
            && self.roll_rad.abs() <= f32::EPSILON
        {
            return view_mat;
        }

        let bob_translation = Mat4::from_translation(right * self.offset_x + up * self.offset_y);
        let roll_rotation = Mat4::from_rotation_z(self.roll_rad);
        roll_rotation * view_mat * bob_translation.inverse()
    }
}
