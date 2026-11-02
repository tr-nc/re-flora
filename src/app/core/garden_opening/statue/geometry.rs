//! Original, hand-authored guardian: crowned head, stern carved face, armour,
//! hands resting on an upright sword, folded legs and a restrained aureole.
//! Dimensions are temple-local voxels. No imported mesh or external asset.
use crate::{
    builder::{VOXEL_TYPE_LIMESTONE, VOXEL_TYPE_ROCK},
    geom::{Cuboid, RoundCone},
};
use glam::{Quat, Vec3};

pub(super) const MIN: Vec3 = Vec3::new(-35., 3., -50.);
pub(super) const MAX: Vec3 = Vec3::new(35., 65., -7.);

pub(super) struct Sculpture {
    stone: Vec<Cuboid>,
    body: Vec<Cuboid>,
    curves: Vec<RoundCone>,
    cuts: Vec<Cuboid>,
}
fn block(lo: [f32; 3], hi: [f32; 3]) -> Cuboid {
    Cuboid::from_min_max(Vec3::from_array(lo), Vec3::from_array(hi))
}
fn beam(a: Vec3, b: Vec3, radius: f32) -> Cuboid {
    Cuboid::new_oriented(
        (a + b) * 0.5,
        Vec3::new(radius, a.distance(b) * 0.5, radius),
        Quat::from_rotation_arc(Vec3::Y, (b - a).normalize()),
    )
}
fn inside(shape: &Cuboid, p: Vec3) -> bool {
    (shape.rotation().conjugate() * (p - shape.center()))
        .abs()
        .cmple(shape.half_size())
        .all()
}
impl Sculpture {
    pub(super) fn new() -> Self {
        let stone = vec![
            block([-35., 3., -50.], [35., 7., -7.]),
            block([-32., 7., -48.], [32., 10., -9.]),
            block([-29., 10., -46.], [29., 12., -11.]),
        ];
        let mut body = vec![
            // Folded robes / knees, belt and a broad, tapering breastplate.
            block([-25., 12., -42.], [25., 19., -13.]),
            block([-19., 19., -37.], [19., 23., -17.]),
            block([-14., 23., -37.], [14., 32., -18.]),
            block([-18., 32., -38.], [18., 41., -19.]),
            block([-12., 39., -35.], [12., 43., -21.]),
            block([-4., 26., -20.], [4., 39., -17.]),
            // Neck, face, jaw and stepped crown.
            block([-5., 41., -30.], [5., 45., -20.]),
            block([-9., 45., -33.], [9., 57., -16.]),
            block([-7., 43., -30.], [7., 47., -17.]),
            block([-11., 57., -35.], [11., 60., -15.]),
            block([-4., 60., -30.], [4., 65., -19.]),
            block([-10., 60., -29.], [-7., 63., -21.]),
            block([7., 60., -29.], [10., 63., -21.]),
            // Projecting nose, moustache and a firm lower lip; eyes / mouth are carved.
            block([-1.5, 48., -17.], [1.5, 54., -11.]),
            block([-5., 46., -17.], [5., 48., -12.]),
            block([-3., 44., -18.], [3., 45., -14.]),
            // Ceremonial sword: narrow blade, crossguard, grip and pommel.
            block([-1.5, 13., -12.], [1.5, 30., -9.]),
            block([-10., 30., -14.], [10., 32., -10.]),
            block([-1.5, 32., -13.], [1.5, 37., -10.]),
            block([-2.5, 37., -14.], [2.5, 39., -9.]),
        ];
        let mut curves = Vec::new();
        let mut cuts = vec![block([-4., 45., -18.], [4., 46., -11.])];
        for sign in [-1_f32, 1.] {
            let point = |x: f32, y: f32, z: f32| Vec3::new(sign * x, y, z);
            // Angular robe folds and boots keep the base wide and grounded.
            body.push(beam(point(8., 18., -36.), point(22., 16., -18.), 4.));
            body.push(Cuboid::new(point(17., 14., -14.), Vec3::new(7., 2., 3.)));
            // Shoulder guards, bent forearms and both hands on the sword.
            curves.push(RoundCone::new(
                6.,
                point(21., 36., -29.),
                5.,
                point(23., 33., -25.),
            ));
            body.push(beam(point(23., 33., -25.), point(20., 26., -18.), 4.));
            body.push(beam(point(20., 26., -18.), point(5., 32., -13.), 3.));
            curves.push(RoundCone::new(
                3.,
                point(5., 32., -13.),
                3.,
                point(5., 33., -13.),
            ));
            // Ears, cheekbones and inward-sloping brows read as a stern face.
            body.push(Cuboid::new(point(10., 50., -25.), Vec3::new(2., 4., 3.)));
            body.push(beam(point(2., 53., -15.), point(8., 55., -15.), 1.2));
            body.push(Cuboid::new(point(7., 48., -16.), Vec3::new(2., 1.5, 1.5)));
            cuts.push(Cuboid::new(point(5., 51., -15.), Vec3::new(2.5, 1., 3.)));
        }
        // A slender broken-from-the-body ring, not a solid plate behind the head.
        for i in 0..40 {
            let p = |step: usize| {
                let angle = step as f32 / 40. * std::f32::consts::TAU;
                Vec3::new(22. * angle.cos(), 40. + 22. * angle.sin(), -43.)
            };
            curves.push(RoundCone::new(1.5, p(i), 1.5, p(i + 1)));
        }
        Self {
            stone,
            body,
            curves,
            cuts,
        }
    }

    pub(super) fn material(&self, p: Vec3) -> u8 {
        if self.stone.iter().any(|s| inside(s, p)) {
            return VOXEL_TYPE_ROCK as u8;
        }
        if !self.cuts.iter().any(|s| inside(s, p))
            && (self.body.iter().any(|s| inside(s, p))
                || self.curves.iter().any(|s| s.signed_distance(p) <= 0.))
        {
            VOXEL_TYPE_LIMESTONE as u8
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guardian_has_a_carved_face_a_visible_sword_and_an_open_halo() {
        let g = Sculpture::new();
        assert_eq!(g.material(Vec3::new(5., 51., -16.)), 0, "carved eye");
        assert_eq!(
            g.material(Vec3::new(0., 51., -12.)),
            VOXEL_TYPE_LIMESTONE as u8,
            "nose"
        );
        assert_eq!(
            g.material(Vec3::new(0., 22., -10.)),
            VOXEL_TYPE_LIMESTONE as u8,
            "sword"
        );
        assert_eq!(
            g.material(Vec3::new(0., 61., -43.)),
            VOXEL_TYPE_LIMESTONE as u8,
            "aureole"
        );
        assert_eq!(g.material(Vec3::new(0., 40., -43.)), 0, "open ring");
        assert_eq!(
            g.material(Vec3::new(0., 5., -25.)),
            VOXEL_TYPE_ROCK as u8,
            "plinth"
        );
    }
    #[test]
    fn sculpture_is_mirrored_at_both_world_sizes_and_never_fills_the_entrance() {
        let g = Sculpture::new();
        for scale in [0.5, 1.] {
            for z in -51..=0 {
                for y in 0..=66 {
                    for x in 0..37 {
                        let p =
                            (Vec3::new(x as f32, y as f32, z as f32) + Vec3::splat(0.5)) / scale;
                        let opposite = Vec3::new(-p.x, p.y, p.z);
                        assert_eq!(g.material(p), g.material(opposite), "{p:?}");
                    }
                }
            }
        }
        for y in 0..70 {
            assert_eq!(g.material(Vec3::new(0., y as f32, 0.)), 0);
        }
    }
}
