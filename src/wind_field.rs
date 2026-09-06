//! PROTOTYPE: horizontal wind evolution and finite, explicitly released gusts.
//! No window, terrain editor, serialization, or vegetation-response dependencies.
use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};
mod transport;
use transport::{Transport, CELLS, SIDE};

pub const MAX_GUSTS: usize = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct WindFieldFrame {
    pub domain: [f32; 4],             // extent x/z, side, enabled
    pub cells: [[f32; 4]; CELLS / 2], // two horizontal vectors per aligned element
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
    pub depth: f32, // full forward depth in voxels
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
    fn spatial_weight(&self, local: Vec2) -> f32 {
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
    pub start: f32,
}

impl Gust {
    pub fn center(&self, time: f32) -> Vec3 {
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
    transport: Transport,
    pub saved_sources: Vec<crate::wind::WindSource>,
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
            transport: Transport::default(),
            saved_sources: Vec::new(),
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
    pub fn release(&mut self, origin: Vec3, direction: Vec2) -> bool {
        self.release_with_settings(origin, direction, self.manual_gust)
    }
    pub fn release_with_settings(
        &mut self,
        origin: Vec3,
        direction: Vec2,
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
        if direction == Vec2::ZERO {
            return false;
        }
        self.gusts.push(Gust {
            origin,
            direction,
            settings,
            start: self.time,
        });
        log::info!("[WIND_PROTOTYPE] release origin={origin:?} direction={direction:?} speed_voxels_s={} active={}", settings.speed, self.gusts.len());
        true
    }
    pub fn advance(&mut self, wall_time: f32) {
        let dt = self
            .last_wall_time
            .map_or(0., |previous| (wall_time - previous).max(0.));
        self.last_wall_time = Some(wall_time);
        self.time += dt;
        let phase = self.time * std::f32::consts::TAU / self.wander_period.max(1.);
        let wander =
            (phase.sin() * 0.7 + (phase * 0.617).sin() * 0.3) * self.wander_degrees.to_radians();
        let target = self.heading_degrees.to_radians() + wander;
        let delta = (target - self.heading)
            .sin()
            .atan2((target - self.heading).cos());
        self.heading += delta * (1. - (-dt / 0.6).exp());
        self.gusts
            .retain(|gust| self.time - gust.start < gust.settings.duration);
        let direction = self.direction();
        let noise = fastnoise_lite::FastNoiseLite::with_seed(3181);
        let steps = (dt.min(1.) * 60.).ceil() as usize;
        for step in 0..steps {
            let h = dt.min(1.) / steps as f32;
            let time = self.time - h * (steps - step - 1) as f32;
            let inflow = |p: Vec2| {
                if self.mode == 0 {
                    let mean = self.saved_sources.iter().fold(Vec2::ZERO, |sum, s| {
                        let a = s.direction_degrees.to_radians();
                        sum + Vec2::new(a.cos(), a.sin()) * s.gain
                    });
                    let detail = crate::wind::Wind::new().sample_sources(
                        Vec3::new(p.x, 0., p.y) / 256.,
                        time,
                        &self.saved_sources,
                    );
                    return (mean + Vec2::new(detail.x, detail.z) * 0.25).clamp_length_max(5.);
                }
                let phase = time * self.evolution_rate;
                let q = p * (100. / self.detail_scale.max(1.)) + Vec2::splat(phase * 20.);
                let n = noise.get_noise_2d(q.x, q.y);
                let side = if self.mode == 2 {
                    self.detail_strength * n
                } else {
                    0.
                };
                direction * self.strength * (1. + n * 0.25)
                    + Vec2::new(-direction.y, direction.x) * side
            };
            let local = |p: Vec2| {
                self.gusts.iter().fold(Vec2::ZERO, |sum, gust| {
                    let age = time - gust.start;
                    if age < 0. || age >= gust.settings.duration {
                        return sum;
                    }
                    let center = gust.center(time) * 256.;
                    let delta = p - Vec2::new(center.x, center.z);
                    let side = Vec2::new(-gust.direction.y, gust.direction.x);
                    let q =
                        Vec2::new(delta.dot(gust.direction), delta.dot(side)) / gust.half_extents();
                    let life = age / gust.settings.duration;
                    let smooth = |v: f32| {
                        let t = v.clamp(0., 1.);
                        t * t * (3. - 2. * t)
                    };
                    let envelope = smooth(life / 0.15) * smooth((1. - life) / 0.35);
                    sum + gust.direction
                        * gust.settings.strength
                        * gust.settings.spatial_weight(q)
                        * envelope
                })
            };
            self.transport
                .step(h, self.propagation_speed.max(0.), inflow, local);
        }
    }
    pub fn set_extent(&mut self, extent: Vec2) {
        self.transport.extent = extent.max(Vec2::ONE);
    }
    pub fn sample(&self, world_voxels: Vec2) -> Vec2 {
        self.transport.sample(world_voxels)
    }
    pub fn frame(&self) -> WindFieldFrame {
        let mut frame = WindFieldFrame {
            domain: [
                self.transport.extent.x,
                self.transport.extent.y,
                SIDE as f32,
                1.,
            ],
            ..WindFieldFrame::default()
        };
        for (i, pair) in self.transport.values().chunks_exact(2).enumerate() {
            frame.cells[i] = [pair[0].x, pair[0].y, pair[1].x, pair[1].y];
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wind_requires_a_finite_nonzero_direction() {
        let mut field = WindField::default();
        assert!(!field.release(Vec3::ZERO, Vec2::ZERO));
        assert!(!field.release(Vec3::ZERO, Vec2::splat(f32::NAN)));
        assert!(field.gusts.is_empty());
        assert!(field.release(Vec3::ZERO, Vec2::Y));
        assert_eq!(field.gusts[0].direction, Vec2::Y);
    }
    #[test]
    fn manual_gusts_work_in_every_background_mode() {
        for mode in 0..=2 {
            let mut field = WindField {
                mode,
                ..WindField::default()
            };
            field.advance(0.);
            assert!(field.release(Vec3::ZERO, Vec2::X));
            field.advance(0.5);
            assert_eq!(field.gusts.len(), 1);
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
        field.release(Vec3::ZERO, Vec2::X);
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
