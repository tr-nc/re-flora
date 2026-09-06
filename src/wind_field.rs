//! PROTOTYPE: 2D wind authoring and finite gust lifetimes. No window, tool,
//! terrain editor, serialization, or vegetation-response dependencies.
use bytemuck::{Pod, Zeroable};
use glam::Vec2;

pub const MAX_GUSTS: usize = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct WindFieldFrame {
    pub background: [f32; 4], // direction xz, strength, comparison mode (0 = legacy)
    pub transport: [f32; 4],  // integrated offset in voxels, time, propagation speed
    pub detail: [f32; 4],     // scale, disturbance strength, evolution rate, gust count
    pub gust_origins: [[f32; 4]; MAX_GUSTS], // xz voxels, start seconds, radial flag
    pub gust_directions: [[f32; 4]; MAX_GUSTS], // direction xz, strength, speed voxels/s
    pub gust_shapes: [[f32; 4]; MAX_GUSTS], // half length, half width, duration, reserved
}

impl Default for WindFieldFrame {
    fn default() -> Self {
        Self::zeroed()
    }
}

#[derive(Clone, Copy)]
pub struct Gust {
    pub origin: Vec2, // world units, horizontal plane
    pub direction: Vec2,
    pub strength: f32,
    pub speed: f32,  // voxels/s
    pub radius: f32, // voxels
    pub duration: f32,
    pub radial: bool,
    pub start: f32,
}

impl Gust {
    pub fn band_half_extents(radius: f32) -> Vec2 {
        Vec2::new(radius * 0.25, radius)
    }

    /// Forward depth and crosswind span; directional gusts form a 4:1 wind band.
    pub fn half_extents(&self) -> Vec2 {
        if self.radial {
            Vec2::splat(self.radius)
        } else {
            Self::band_half_extents(self.radius)
        }
    }
}

pub struct WindField {
    pub mode: u32,
    pub heading_degrees: f32,
    pub strength: f32,
    pub propagation_speed: f32,
    pub wander_degrees: f32,
    pub wander_period: f32,
    pub detail_strength: f32,
    pub detail_scale: f32,
    pub evolution_rate: f32,
    pub auto_gusts: bool,
    pub gust_interval: f32,
    pub gust_strength: f32,
    pub gust_speed: f32,
    pub gust_radius: f32,
    pub gust_duration: f32,
    pub gusts: Vec<Gust>,
    pub paused: bool,
    time: f32,
    last_wall_time: Option<f32>,
    heading: f32,
    offset: Vec2,
    next_auto: f32,
}

impl Default for WindField {
    fn default() -> Self {
        Self {
            mode: 3,
            heading_degrees: 220.,
            strength: 1.5,
            propagation_speed: 50.,
            wander_degrees: 22.,
            wander_period: 24.,
            detail_strength: 0.4,
            detail_scale: 60.,
            evolution_rate: 0.25,
            auto_gusts: false,
            gust_interval: 5.,
            gust_strength: 3.,
            gust_speed: 70.,
            gust_radius: 30.,
            gust_duration: 3.,
            gusts: Vec::new(),
            paused: false,
            time: 0.,
            last_wall_time: None,
            heading: 220_f32.to_radians(),
            offset: Vec2::ZERO,
            next_auto: 2.,
        }
    }
}

impl WindField {
    pub fn time(&self) -> f32 {
        self.time
    }
    pub fn direction(&self) -> Vec2 {
        Vec2::new(self.heading.cos(), self.heading.sin())
    }

    pub fn release(&mut self, origin: Vec2, direction: Vec2, radial: bool) -> bool {
        if !origin.is_finite()
            || !direction.is_finite()
            || self.mode < 2
            || self.gusts.len() >= MAX_GUSTS
        {
            return false;
        }
        let direction = direction.normalize_or_zero();
        if !radial && direction == Vec2::ZERO {
            return false;
        }
        self.gusts.push(Gust {
            origin,
            direction,
            radial,
            start: self.time,
            strength: self.gust_strength,
            speed: self.gust_speed,
            radius: self.gust_radius,
            duration: self.gust_duration,
        });
        log::info!("[WIND_PROTOTYPE] release radial={radial} origin={origin:?} direction={direction:?} active={}", self.gusts.len());
        true
    }

    pub fn clear(&mut self) {
        self.gusts.clear();
    }

    pub fn restart(&mut self) {
        self.gusts.clear();
        self.time = 0.;
        self.offset = Vec2::ZERO;
        self.heading = self.heading_degrees.to_radians();
        self.next_auto = 2.;
    }

    pub fn advance(&mut self, wall_time: f32) {
        let dt = self
            .last_wall_time
            .map_or(0., |previous| (wall_time - previous).max(0.));
        self.last_wall_time = Some(wall_time);
        if self.paused {
            return;
        }
        self.time += dt;
        let old_direction = self.direction();
        let phase = self.time * std::f32::consts::TAU / self.wander_period.max(1.);
        let wander =
            (phase.sin() * 0.7 + (phase * 0.617).sin() * 0.3) * self.wander_degrees.to_radians();
        let target = self.heading_degrees.to_radians() + wander;
        let delta = (target - self.heading)
            .sin()
            .atan2((target - self.heading).cos());
        self.heading += delta * (1. - (-dt / 0.6).exp());
        // Integrate displacement: editing heading/speed never reinterprets elapsed time.
        self.offset += (old_direction + self.direction()) * (0.5 * dt * self.propagation_speed);
        self.gusts
            .retain(|gust| self.time - gust.start < gust.duration);
        if self.auto_gusts && self.mode >= 2 && self.time >= self.next_auto {
            self.next_auto = self.time + self.gust_interval;
            // Fixed sequence: replay is deterministic; no global random generator.
            let cross = Vec2::new(-self.direction().y, self.direction().x);
            let center = Vec2::splat(1.) + cross * (self.time * 1.618).sin() * 0.3;
            self.release(center - self.direction() * 0.45, self.direction(), false);
        }
    }

    pub fn frame(&self) -> WindFieldFrame {
        let mut frame = WindFieldFrame {
            background: [
                self.direction().x,
                self.direction().y,
                self.strength,
                self.mode as f32,
            ],
            // Prototype time is deliberately separate: Pause holds the input field,
            // while existing vegetation mechanics may continue to settle.
            transport: [
                self.offset.x,
                self.offset.y,
                self.time,
                if self.paused {
                    0.
                } else {
                    self.propagation_speed
                },
            ],
            detail: [
                self.detail_scale,
                self.detail_strength,
                self.evolution_rate,
                0.,
            ],
            ..WindFieldFrame::default()
        };
        for (i, gust) in self.gusts.iter().take(MAX_GUSTS).enumerate() {
            frame.gust_origins[i] = [
                gust.origin.x * 256.,
                gust.origin.y * 256.,
                gust.start,
                u32::from(gust.radial) as f32,
            ];
            frame.gust_directions[i] = [
                gust.direction.x,
                gust.direction.y,
                gust.strength,
                gust.speed,
            ];
            let extent = gust.half_extents();
            frame.gust_shapes[i] = [extent.x, extent.y, gust.duration, 0.];
            frame.detail[3] += 1.;
        }
        frame
    }
}
