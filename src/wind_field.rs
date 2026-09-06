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

impl WindFieldFrame {
    pub fn uniform(velocity: Vec2) -> Self {
        Self {
            domain: [512., 512., SIDE as f32, 1.],
            cells: [[velocity.x, velocity.y, velocity.x, velocity.y]; CELLS / 2],
        }
    }

    pub fn sample_world(&self, position: Vec3) -> Vec3 {
        if self.domain[3] == 0. {
            return Vec3::ZERO;
        }
        let velocity = transport::sample_grid(
            Vec2::new(self.domain[0], self.domain[1]),
            Vec2::new(position.x, position.z) * 256.,
            |i| {
                let pair = self.cells[i / 2];
                let offset = (i % 2) * 2;
                Vec2::new(pair[offset], pair[offset + 1])
            },
        );
        Vec3::new(velocity.x, 0., velocity.y)
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
    pub background_enabled: bool,
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
}

impl Default for WindField {
    fn default() -> Self {
        Self {
            background_enabled: true,
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
                if !self.background_enabled {
                    return Vec2::ZERO;
                }
                let phase = time * self.evolution_rate;
                let q = p * (100. / self.detail_scale.max(1.)) + Vec2::splat(phase * 20.);
                let n = noise.get_noise_2d(q.x, q.y);
                let side = self.detail_strength * n;
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
    fn manual_gusts_work_with_or_without_background() {
        for background_enabled in [false, true] {
            let mut field = WindField {
                background_enabled,
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
        for background_enabled in [false, true] {
            let mut field = WindField {
                background_enabled,
                ..WindField::default()
            };
            for step in 0..10 {
                field.advance(step as f32);
            }
            assert!(field.gusts.is_empty());
        }
    }
    #[test]
    fn disabled_background_is_calm_but_manual_wind_still_propagates() {
        let mut field = WindField {
            background_enabled: false,
            ..WindField::default()
        };
        field.advance(0.);
        field.advance(1.);
        assert!(field.frame().cells.iter().flatten().all(|v| *v == 0.));
        field.release(Vec3::new(1., 0.5, 1.), Vec2::X);
        field.advance(2.);
        assert!(field.sample(Vec2::new(1.2, 1.) * 256.).x > 0.01);
    }

    #[test]
    fn published_snapshot_matches_live_sampling_in_world_coordinates() {
        let mut field = WindField::default();
        field.set_extent(Vec2::new(640., 384.));
        field.advance(0.);
        field.release(Vec3::new(1., 0.5, 0.75), Vec2::new(1., 0.3));
        field.advance(1.);
        let frame = field.frame();
        for x in [-0.1, 0., 0.37, 1.1, 2.8] {
            for z in [-0.2, 0., 0.79, 1.6] {
                let expected = field.sample(Vec2::new(x, z) * 256.);
                let actual = frame.sample_world(Vec3::new(x, 0.7, z));
                assert!((actual - Vec3::new(expected.x, 0., expected.y)).length() < 1e-6);
            }
        }
        assert_eq!(
            WindFieldFrame::default().sample_world(Vec3::ONE),
            Vec3::ZERO
        );
    }

    #[test]
    fn local_disturbance_alone_controls_crosswind_detail() {
        let build = |detail_strength| {
            let mut field = WindField {
                heading_degrees: 0.,
                heading: 0.,
                wander_degrees: 0.,
                detail_strength,
                ..WindField::default()
            };
            field.advance(0.);
            field.advance(1.);
            field.frame()
        };
        let smooth = build(0.);
        assert!(smooth.cells.iter().all(|c| c[1] == 0. && c[3] == 0.));
        let detailed = build(1.);
        assert!(detailed
            .cells
            .iter()
            .any(|c| c[1].abs() > 0.001 || c[3].abs() > 0.001));
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
