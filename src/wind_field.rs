//! PROTOTYPE: horizontal wind evolution and finite, explicitly released gusts.
//! No window, terrain editor, serialization, or vegetation-response dependencies.
use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};

pub const MAX_GUSTS: usize = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct WindFieldFrame {
    pub background: [f32; 4],
    pub transport: [f32; 4],
    pub detail: [f32; 4],
    pub gust_origins: [[f32; 4]; MAX_GUSTS],
    pub gust_directions: [[f32; 4]; MAX_GUSTS],
    pub gust_shapes: [[f32; 4]; MAX_GUSTS], // half depth, half width, lifetime, softness
}

impl Default for WindFieldFrame {
    fn default() -> Self {
        Self::zeroed()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GustSettings {
    pub strength: f32,
    pub speed: f32, // voxels/s, resolved from the gesture before emission
    pub width: f32, // full crosswind span in voxels
    pub depth: f32, // full forward depth / radial ring thickness in voxels
    pub duration: f32,
    pub softness: f32,
}

impl Default for GustSettings {
    fn default() -> Self {
        Self {
            strength: 3.,
            speed: 70.,
            width: 192.,
            depth: 48.,
            duration: 3.,
            softness: 1.,
        }
    }
}

impl GustSettings {
    pub fn half_extents(&self) -> Vec2 {
        Vec2::new(self.depth, self.width) * 0.5
    }
    pub fn spatial_weight(&self, local: Vec2) -> f32 {
        fn edge(value: f32, softness: f32) -> f32 {
            let softness = softness.clamp(0.05, 1.);
            let t = ((value.abs() - (1. - softness)) / softness).clamp(0., 1.);
            1. - t * t * (3. - 2. * t)
        }
        edge(local.x, self.softness) * edge(local.y, self.softness)
    }
}

#[derive(Clone, Copy)]
pub struct Gust {
    // Authored height is preserved for visualization; force remains horizontal.
    pub origin: Vec3,
    pub direction: Vec2,
    pub settings: GustSettings,
    pub radial: bool,
    pub start: f32,
}

impl Gust {
    pub fn center(&self, time: f32) -> Vec3 {
        if self.radial {
            return self.origin;
        }
        self.origin
            + Vec3::new(self.direction.x, 0., self.direction.y)
                * (self.settings.speed * (time - self.start).max(0.) / 256.)
    }
    pub fn half_extents(&self) -> Vec2 {
        self.settings.half_extents()
    }
}

pub struct WindField {
    pub mode: u32, // 0 original, 1 turning, 2 local detail
    pub heading_degrees: f32,
    pub strength: f32,
    pub propagation_speed: f32,
    pub wander_degrees: f32,
    pub wander_period: f32,
    pub detail_strength: f32,
    pub detail_scale: f32,
    pub evolution_rate: f32,
    pub manual_gust: GustSettings,
    pub gusts: Vec<Gust>,
    time: f32,
    last_wall_time: Option<f32>,
    heading: f32,
    offset: Vec2,
}

impl Default for WindField {
    fn default() -> Self {
        Self {
            mode: 2,
            heading_degrees: 220.,
            strength: 1.5,
            propagation_speed: 50.,
            wander_degrees: 22.,
            wander_period: 24.,
            detail_strength: 0.4,
            detail_scale: 60.,
            evolution_rate: 0.25,
            manual_gust: GustSettings::default(),
            gusts: Vec::new(),
            time: 0.,
            last_wall_time: None,
            heading: 220_f32.to_radians(),
            offset: Vec2::ZERO,
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
    pub fn release(&mut self, origin: Vec3, direction: Vec2, radial: bool) -> bool {
        self.release_with_settings(origin, direction, radial, self.manual_gust)
    }
    pub fn release_with_settings(
        &mut self,
        origin: Vec3,
        direction: Vec2,
        radial: bool,
        settings: GustSettings,
    ) -> bool {
        if !origin.is_finite()
            || !direction.is_finite()
            || !settings.speed.is_finite()
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
            settings,
            radial,
            start: self.time,
        });
        log::info!("[WIND_PROTOTYPE] release radial={radial} origin={origin:?} direction={direction:?} speed_voxels_s={} active={}", settings.speed, self.gusts.len());
        true
    }
    pub fn advance(&mut self, wall_time: f32) {
        let dt = self
            .last_wall_time
            .map_or(0., |previous| (wall_time - previous).max(0.));
        self.last_wall_time = Some(wall_time);
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
        self.offset += (old_direction + self.direction()) * (0.5 * dt * self.propagation_speed);
        self.gusts
            .retain(|gust| self.time - gust.start < gust.settings.duration);
    }
    pub fn frame(&self) -> WindFieldFrame {
        let mut frame = WindFieldFrame {
            background: [
                self.direction().x,
                self.direction().y,
                self.strength,
                self.mode as f32,
            ],
            transport: [
                self.offset.x,
                self.offset.y,
                self.time,
                self.propagation_speed,
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
                gust.origin.z * 256.,
                gust.start,
                u32::from(gust.radial) as f32,
            ];
            frame.gust_directions[i] = [
                gust.direction.x,
                gust.direction.y,
                gust.settings.strength,
                gust.settings.speed,
            ];
            let extent = gust.half_extents();
            frame.gust_shapes[i] = [
                extent.x,
                extent.y,
                gust.settings.duration,
                gust.settings.softness,
            ];
            frame.detail[3] += 1.;
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manual_gusts_work_in_every_background_mode() {
        for mode in 0..=2 {
            let mut field = WindField {
                mode,
                ..WindField::default()
            };
            field.advance(0.);
            assert!(field.release(Vec3::ZERO, Vec2::X, false));
            field.advance(0.5);
            assert_eq!(field.frame().detail[3], 1.);
        }
    }
    #[test]
    fn advancing_any_background_never_emits_a_gust() {
        let mut field = WindField::default();
        for mode in 0..=2 {
            field.mode = mode;
            for step in 0..10 {
                field.advance((mode * 100 + step * 10) as f32);
            }
            assert!(field.gusts.is_empty());
        }
    }
    #[test]
    fn release_snapshots_settings_and_expires_naturally() {
        let mut field = WindField::default();
        field.manual_gust.strength = 1.;
        field.advance(0.);
        field.release(Vec3::ZERO, Vec2::X, false);
        field.manual_gust.strength = 2.;
        assert_eq!(field.gusts[0].settings.strength, 1.);
        field.advance(10.);
        assert!(field.gusts.is_empty());
    }
    #[test]
    fn softness_and_dimensions_are_independent() {
        let mut settings = GustSettings::default();
        let extent = settings.half_extents();
        let soft_edge = settings.spatial_weight(Vec2::new(0., 0.75));
        settings.softness = 0.2;
        assert!(settings.spatial_weight(Vec2::new(0., 0.75)) > soft_edge);
        assert_eq!(settings.half_extents(), extent);
        assert_eq!(settings.spatial_weight(Vec2::new(0., 1.01)), 0.);
    }
}
