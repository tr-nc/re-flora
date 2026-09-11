//! Experimental HTML v4/v5 inlet recipe. Outputs game wind strength, not SI velocity.
//! One fixed mapping keeps the pattern anchored as the garden extent changes.
use glam::Vec2;

const VOXELS_PER_DEMO_UNIT: f64 = 24.;

#[derive(Clone, Copy, Debug)]
pub struct NaturalInflow {
    pub strength: f32,
    pub variation: f32,
    pub surge: f32,
    pub range_voxels: f32,
}

impl Default for NaturalInflow {
    fn default() -> Self {
        Self {
            strength: 0.9,
            variation: 0.32,
            surge: 0.4,
            range_voxels: 108.,
        }
    }
}

// Keep the HTML hash in double precision: f32 loses its fractional random bits.
fn hash(x: f64, y: f64) -> f64 {
    let n = (x * 127.1 + y * 311.7 + 19.19).sin() * 43758.5453;
    n - n.floor()
}
fn smooth(x: f64) -> f64 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}
fn noise(x: f64, y: f64) -> f64 {
    let (i, j) = (x.floor(), y.floor());
    let (a, b) = (smooth(x - i), smooth(y - j));
    let mix = |x: f64, y: f64, t: f64| x + (y - x) * t;
    2. * mix(
        mix(hash(i, j), hash(i + 1., j), a),
        mix(hash(i, j + 1.), hash(i + 1., j + 1.), a),
        b,
    ) - 1.
}

impl NaturalInflow {
    pub fn sample(&self, position_voxels: Vec2, time: f32, heading_degrees: f32) -> Vec2 {
        let p = position_voxels.as_dvec2() / VOXELS_PER_DEMO_UNIT;
        let q = p.y + 0.17 * p.x;
        let s = self.range_voxels.max(24.) as f64 / VOXELS_PER_DEMO_UNIT;
        let t = time as f64;
        let broad = noise(q / s, t / (s * 1.5) + 4.);
        let fine = noise(q / (s * 0.42) + 21., t / (s * 0.52) - 7.);
        let slow = noise(q / (s * 2.4) - 3., t / (s * 4.2) + 17.);
        let organization =
            0.8 * noise(q / (s * 2.5) + 61., t / 10. + 8.) + 0.2 * noise(q / s - 31., t / 4. + 19.);
        let crest = smooth((organization - 0.05) / 0.45);
        let variation = self.variation.clamp(0., 0.55) as f64;
        let quiet = 1. + variation * (0.8 * broad + 0.3 * fine + 0.28 * slow);
        let strength = self.strength.max(0.) as f64
            * quiet
            * (1. + self.surge.clamp(0., 0.9) as f64 * (1.9 * crest - 0.18));
        let angle = (heading_degrees as f64
            + 5. * noise(t / 26., 4.)
            + variation * 27. * noise(q / (s * 0.8) + 43., t / (s * 1.2)))
        .to_radians();
        Vec2::new(angle.cos() as f32, angle.sin() as f32) * strength as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inlet_is_continuous_spatially_varied_and_contains_lulls_and_crests() {
        let inlet = NaturalInflow::default();
        let p = Vec2::new(0., 168.);
        let mut min = f32::MAX;
        let mut max = 0_f32;
        let mut spatial_difference = 0_f32;
        for i in 0..2400 {
            let t = i as f32 * 0.1;
            let w = inlet.sample(p, t, 220.);
            assert!(w.is_finite());
            min = min.min(w.length());
            max = max.max(w.length());
            assert!((w - inlet.sample(p, t + 0.001, 220.)).length() < 0.01);
            assert!((w - inlet.sample(p + Vec2::splat(0.001), t, 220.)).length() < 0.01);
            spatial_difference =
                spatial_difference.max((w - inlet.sample(p + Vec2::Y * 192., t, 220.)).length());
        }
        assert!(min < 0.8 && min > 0.3, "min={min}");
        assert!(max > 1.4 && max < 3., "max={max}");
        assert!(spatial_difference > 0.3);
    }

    #[test]
    fn zero_strength_is_calm_and_heading_rotates_the_same_recipe() {
        let mut inlet = NaturalInflow::default();
        let p = Vec2::new(31., 79.);
        let a = inlet.sample(p, 13., 0.);
        let b = inlet.sample(p, 13., 180.);
        assert!((a + b).length() < 1e-5);
        inlet.strength = 0.;
        assert_eq!(inlet.sample(p, 13., 0.), Vec2::ZERO);
    }
}
