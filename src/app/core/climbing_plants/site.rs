//! Place the test patch beside the startup tree, against natural ground rather than foliage.
use super::{Fixture, Terrain};
use glam::{IVec3, UVec3, Vec3};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Site {
    translation: IVec3,
}
impl Site {
    pub const COLUMN: UVec3 = UVec3::new(383, 0, 300);

    pub fn find(terrain: &impl Terrain, world_dim: UVec3) -> Option<Self> {
        if Self::COLUMN.cmpge(world_dim).any() || !terrain.current() {
            return None;
        }
        for y in (0..world_dim.y).rev() {
            let cell = Self::COLUMN + UVec3::Y * y;
            let material = u32::from(terrain.voxel(cell.as_ivec3())?);
            // Do not ground the fixture on wood, foliage or its own limestone after a reload.
            if !matches!(
                material,
                crate::builder::VOXEL_TYPE_DIRT
                    | crate::builder::VOXEL_TYPE_SAND
                    | crate::builder::VOXEL_TYPE_ROCK
            ) {
                continue;
            }
            let site = Self {
                translation: IVec3::new(
                    cell.x as i32 - 255,
                    y as i32 + 1 - 192,
                    cell.z as i32 - 300,
                ),
            };
            return (site.bounds().1.cmple(world_dim).all() && terrain.current()).then_some(site);
        }
        None
    }
    pub fn point(self, point: Vec3) -> Vec3 {
        point + self.translation.as_vec3()
    }
    pub fn cell(self, cell: IVec3) -> IVec3 {
        cell + self.translation
    }
    pub fn voxel_box(self, (min, max): (UVec3, UVec3)) -> (UVec3, UVec3) {
        (
            self.cell(min.as_ivec3()).max(IVec3::ZERO).as_uvec3(),
            self.cell(max.as_ivec3()).max(IVec3::ZERO).as_uvec3(),
        )
    }
    pub fn bounds(self) -> (UVec3, UVec3) {
        self.voxel_box(Fixture::bounds())
    }
    pub fn seed(self, fixture: Fixture) -> (Vec3, Vec3, IVec3) {
        let (p, n, cell) = fixture.seed();
        (self.point(p), n, self.cell(cell))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Landscape {
        top: i32,
        pending: bool,
        stale: bool,
    }
    impl Terrain for Landscape {
        fn voxel(&self, c: IVec3) -> Option<u8> {
            if self.pending {
                return None;
            }
            Some(if c.y < self.top {
                crate::builder::VOXEL_TYPE_DIRT as u8
            } else if (self.top + 30..self.top + 80).contains(&c.y) {
                crate::builder::VOXEL_TYPE_CHERRY_WOOD as u8
            } else {
                0
            })
        }
        fn current(&self) -> bool {
            !self.stale
        }
    }
    #[test]
    fn fixture_is_grounded_at_different_heights_and_not_on_the_tree() {
        for top in [1, 72, 105, 155] {
            let terrain = Landscape {
                top,
                pending: false,
                stale: false,
            };
            let site = Site::find(&terrain, UVec3::splat(512)).unwrap();
            let base = site.point(Vec3::new(255.5, 192.0, 300.5));
            assert_eq!(
                base.y, top as f32,
                "fixture floats above natural ground or rests on wood"
            );
            assert_eq!(base.x, 383.5, "fixture still overlaps the startup tree");
            for fixture in Fixture::ALL {
                let (lo, hi) = site.voxel_box(fixture.boxes()[0]);
                let foundation_cell = Site::COLUMN + UVec3::Y * (top as u32 - 1);
                assert!(
                    foundation_cell.cmpge(lo).all() && foundation_cell.cmplt(hi).all(),
                    "footing does not reach the actual soil"
                );
                let (p, _, cell) = site.seed(fixture);
                assert_eq!(
                    p - cell.as_vec3(),
                    fixture.seed().0 - fixture.seed().2.as_vec3()
                );
            }
            assert!(site.bounds().1.cmple(UVec3::splat(512)).all());
        }
    }
    #[test]
    fn unknown_stale_absent_or_too_high_ground_never_authors_a_floating_fallback() {
        for terrain in [
            Landscape {
                top: 105,
                pending: true,
                stale: false,
            },
            Landscape {
                top: 105,
                pending: false,
                stale: true,
            },
            Landscape {
                top: 0,
                pending: false,
                stale: false,
            },
            Landscape {
                top: 480,
                pending: false,
                stale: false,
            },
        ] {
            assert_eq!(Site::find(&terrain, UVec3::splat(512)), None);
        }
    }
}
