//! Game material recipes and version-compatible strand snapshots. Physics owns live motion.
use glam::Vec3;
use re_flora_physics::rope::{Rope, RopeDesc, RopeMaterial};
use serde::{Deserialize, Serialize};

pub const UNITS: f32 = 256.;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Material {
    Hemp,
    Hose,
    Kite,
}
impl Material {
    pub fn from_index(index: u32) -> Self {
        match index {
            1 => Self::Hose,
            2 => Self::Kite,
            _ => Self::Hemp,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Hemp => "Hemp rope",
            Self::Hose => "Garden hose",
            Self::Kite => "Kite line",
        }
    }
    pub fn physics(self) -> RopeMaterial {
        let (radius, density, stretch, bend, damping) = match self {
            Self::Hemp => (0.004, 0.08, 1500., 0.00001, 1.1),
            Self::Hose => (0.009, 0.35, 2500., 0.0015, 2.0),
            Self::Kite => (0.0015, 0.004, 800., 0., 0.8),
        };
        RopeMaterial {
            radius,
            linear_density: density,
            stretch_rigidity: stretch,
            bend_rigidity: bend,
            damping,
            friction: 0.65,
        }
    }
    pub fn end_mass(self) -> f32 {
        if self == Self::Kite {
            0.015
        } else {
            0.
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedRope {
    pub material: Material,
    pub points: Vec<[f32; 3]>,
    pub rest_lengths: Vec<f32>,
    pub pin_start: bool,
    pub pin_end: bool,
}
impl SavedRope {
    pub fn capture(material: Material, rope: &Rope) -> Self {
        Self {
            material,
            points: rope
                .positions()
                .iter()
                .map(|p| (*p / UNITS).to_array())
                .collect(),
            rest_lengths: rope.rest_lengths().iter().map(|l| l / UNITS).collect(),
            pin_start: rope.is_pinned(0),
            pin_end: rope.is_pinned(rope.positions().len() - 1),
        }
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (2..=129).contains(&self.points.len())
                && self.rest_lengths.len() + 1 == self.points.len(),
            "invalid rope snapshot size"
        );
        anyhow::ensure!(
            self.points.iter().all(|p| Vec3::from_array(*p).is_finite()
                && Vec3::from_array(*p).abs().max_element() < 1000.),
            "invalid rope position"
        );
        anyhow::ensure!(
            self.rest_lengths
                .iter()
                .all(|l| l.is_finite() && *l > 0.0002 && *l < 1.)
                && self.rest_lengths.iter().sum::<f32>() < 8.,
            "invalid rope material length"
        );
        self.desc().validate()?;
        Ok(())
    }
    pub fn desc(&self) -> RopeDesc {
        RopeDesc {
            points: self
                .points
                .iter()
                .map(|p| Vec3::from_array(*p) * UNITS)
                .collect(),
            rest_lengths: Some(self.rest_lengths.iter().map(|l| l * UNITS).collect()),
            material: self.material.physics().scaled(UNITS),
            pin_start: self.pin_start,
            pin_end: self.pin_end,
            end_mass: self.material.end_mass(),
        }
    }
}

/// Stylized forces from game wind strength (not an SI velocity or a kite flight model).
/// The extra load belongs only to the kite endpoint, never to the shared constraint solver.
pub fn wind_acceleration(material: Material, strength: Vec3, gain: f32) -> (Vec3, Vec3) {
    let v = strength.clamp_length_max(2.) * gain.clamp(0., 3.);
    let load = if material == Material::Kite {
        v * 14. + Vec3::Y * (v.length_squared() * 22.)
    } else {
        Vec3::ZERO
    };
    (v * UNITS, load * UNITS)
}

/// Uniform arc-length samples keep the curvature discretization independent of parameter t.
pub fn initial_desc(material: Material, a: Vec3, b: Vec3, slack: f32) -> anyhow::Result<RopeDesc> {
    let distance = a.distance(b);
    anyhow::ensure!(
        a.is_finite() && b.is_finite() && (0.06..=3.).contains(&distance),
        "Choose endpoints 0.06–3 world units apart"
    );
    let sag = distance * (slack.clamp(1., 1.6) - 1.).sqrt() * 0.55;
    let mut samples = Vec::new();
    let mut lengths = vec![0.];
    for i in 0..=256 {
        let t = i as f32 / 256.;
        let p = a.lerp(b, t) + Vec3::Y * (std::f32::consts::PI * t).sin() * sag;
        if let Some(last) = samples.last() {
            lengths.push(lengths.last().unwrap() + p.distance(*last));
        }
        samples.push(p);
    }
    let segments = ((distance / 0.018).ceil() as usize).clamp(16, 64);
    let mut points = Vec::new();
    let mut cursor = 1;
    for i in 0..=segments {
        let d = lengths[256] * i as f32 / segments as f32;
        while cursor < 256 && lengths[cursor] < d {
            cursor += 1;
        }
        let t = (d - lengths[cursor - 1]) / (lengths[cursor] - lengths[cursor - 1]);
        points.push(samples[cursor - 1].lerp(samples[cursor], t) * UNITS);
    }
    Ok(RopeDesc {
        points,
        rest_lengths: None,
        material: material.physics().scaled(UNITS),
        pin_start: true,
        pin_end: material == Material::Hemp,
        end_mass: material.end_mass(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_preserve_rest_lengths_instead_of_baking_stretch() {
        let mut world = re_flora_physics::CollisionWorld::new();
        let id = world
            .spawn_rope(initial_desc(Material::Hemp, Vec3::Y, Vec3::ONE, 1.12).unwrap())
            .unwrap();
        for _ in 0..120 {
            world.advance(1. / 60.);
        }
        let saved = SavedRope::capture(Material::Hemp, world.rope(id).unwrap());
        let decoded: SavedRope =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        decoded.validate().unwrap();
        let next = world.spawn_rope(decoded.desc()).unwrap();
        assert_eq!(
            world.rope(id).unwrap().rest_lengths(),
            world.rope(next).unwrap().rest_lengths()
        );
        assert_eq!(
            world.rope(id).unwrap().positions(),
            world.rope(next).unwrap().positions()
        );
    }
    #[test]
    fn wind_supports_kite_but_calm_air_does_not() {
        let mut heights = Vec::new();
        for strength in [Vec3::ZERO, Vec3::X] {
            let mut world = re_flora_physics::CollisionWorld::new();
            world.set_gravity(Vec3::NEG_Y * 9.8 * UNITS).unwrap();
            let id = world
                .spawn_rope(
                    initial_desc(Material::Kite, Vec3::Y, Vec3::new(0.3, 1.5, 0.), 1.15).unwrap(),
                )
                .unwrap();
            let (all, end) = wind_acceleration(Material::Kite, strength, 1.);
            world
                .rope_mut(id)
                .unwrap()
                .set_acceleration(all, end)
                .unwrap();
            for _ in 0..240 {
                world.advance(1. / 60.);
            }
            let r = world.rope(id).unwrap();
            assert!(r.max_strain() < 0.1, "{}", r.max_strain());
            heights.push(r.positions().last().unwrap().y / UNITS);
        }
        assert!(heights[0] < 0.9 && heights[1] > 1.1, "{heights:?}");
        assert_eq!(wind_acceleration(Material::Hose, Vec3::X, 1.).1, Vec3::ZERO);
    }
    #[test]
    fn kite_stays_bounded_when_a_ramping_gust_lifts_it_off_static_ground() {
        for dt in [1. / 60., 1. / 120.] {
            let mut world = re_flora_physics::CollisionWorld::new();
            world.set_gravity(Vec3::NEG_Y * 9.8 * UNITS).unwrap();
            let center = Vec3::new(434., 216., 400.) / UNITS;
            let corners = [
                Vec3::new(-10., center.y, -10.),
                Vec3::new(-10., center.y, 10.),
                Vec3::new(10., center.y, 10.),
                Vec3::new(10., center.y, -10.),
            ]
            .map(|p| p * UNITS);
            world
                .set_fixed_scene_surface(&corners, &[[0, 1, 2], [0, 2, 3]])
                .unwrap();
            let a = center + Vec3::new(-0.4, 0.28, 0.22);
            let id = world
                .spawn_rope(
                    initial_desc(Material::Kite, a, a + Vec3::new(0.38, 0.5, 0.), 1.15).unwrap(),
                )
                .unwrap();
            let mut wind = crate::wind_field::WindField::default();
            wind.set_extent(glam::Vec2::splat(1024.));
            wind.release_with_settings(
                center,
                glam::Vec2::X,
                crate::wind_field::GustSettings {
                    strength: 1.2,
                    speed: 0.,
                    width: 1024.,
                    depth: 1024.,
                    duration: 30.,
                    ..Default::default()
                },
            );
            for i in 0..(10. / dt) as usize {
                let p = *world.rope(id).unwrap().positions().last().unwrap() / UNITS;
                let (all, end) =
                    wind_acceleration(Material::Kite, wind.frame().sample_world(p), 1.);
                world
                    .rope_mut(id)
                    .unwrap()
                    .set_acceleration(all, end)
                    .unwrap();
                world.advance(dt);
                wind.advance(i as f32 * dt);
                let strain = world.rope(id).unwrap().max_strain();
                assert!(
                    strain < 0.15,
                    "dt={dt} time={} strain={strain}",
                    i as f32 * dt
                );
            }
        }
    }
    #[test]
    fn invalid_snapshot_and_span_are_rejected() {
        assert!(initial_desc(Material::Hose, Vec3::ZERO, Vec3::ZERO, 1.1).is_err());
        let saved = SavedRope {
            material: Material::Hemp,
            points: vec![[0.; 3]; 500],
            rest_lengths: vec![],
            pin_start: true,
            pin_end: false,
        };
        assert!(saved.validate().is_err());
    }
    #[test]
    fn presets_share_arc_sampling_but_not_bending_or_endpoint_mass() {
        for material in [Material::Hemp, Material::Hose, Material::Kite] {
            let d = initial_desc(material, Vec3::Y, Vec3::ONE, 1.15).unwrap();
            let lengths: Vec<_> = d.points.windows(2).map(|p| p[0].distance(p[1])).collect();
            assert!(
                lengths.iter().copied().fold(0., f32::max)
                    / lengths.iter().copied().fold(f32::MAX, f32::min)
                    < 1.01
            );
        }
        assert!(Material::Hose.physics().bend_rigidity > Material::Hemp.physics().bend_rigidity);
        assert!(Material::Kite.end_mass() > 0.);
    }
}
