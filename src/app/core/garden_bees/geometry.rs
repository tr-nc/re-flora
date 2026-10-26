//! Deterministic, face-connected solid voxel recipes; no external model assets.
use super::*;
use crate::builder::{VOXEL_TYPE_CHERRY_WOOD, VOXEL_TYPE_OAK_WOOD};
use glam::IVec3;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum HiveStyle {
    Artificial,
    Natural,
}
impl Hive {
    pub(super) fn local_cell(&self, p: IVec3) -> UVec3 {
        let p = match self.quarter_turn {
            0 => p,
            1 => IVec3::new(p.z, p.y, -p.x),
            2 => IVec3::new(-p.x, p.y, -p.z),
            _ => IVec3::new(-p.z, p.y, p.x),
        };
        (UVec3::from(self.base).as_ivec3() + p).as_uvec3()
    }
    pub(super) fn cells(&self) -> BTreeMap<[u32; 3], u32> {
        let mut cells = BTreeMap::new();
        let mut put = |x, y, z, material| {
            cells.insert(self.local_cell(IVec3::new(x, y, z)).to_array(), material);
        };
        // Narrow square post, physically joined to the nest's underside.
        for y in 0..34 {
            for z in -1..=1 {
                for x in -1..=1 {
                    put(x, y, z, VOXEL_TYPE_OAK_WOOD);
                }
            }
        }
        match self.style {
            HiveStyle::Artificial => {
                for y in 32..=40 {
                    for z in -3..=3 {
                        for x in -4i32..=4 {
                            // Three recessed openings; the middle is the bees' entrance.
                            if z >= 1 && y == 36 && [-2, 0, 2].contains(&x) {
                                continue;
                            }
                            let frame = x.abs() == 4 || y == 32 || y == 40;
                            put(
                                x,
                                y,
                                z,
                                if frame {
                                    VOXEL_TYPE_OAK_WOOD
                                } else {
                                    VOXEL_TYPE_CHERRY_WOOD
                                },
                            );
                        }
                    }
                }
                for x in -5i32..=5 {
                    let y = 41 + (5 - x.abs()) / 2;
                    for z in -4..=4 {
                        for dy in 0..=1 {
                            put(x, y + dy, z, VOXEL_TYPE_OAK_WOOD);
                        }
                    }
                }
            }
            HiveStyle::Natural => {
                // Unequal rounded tiers form a stylized ochre nest, not a paper wasp nest.
                for y in 31..=44 {
                    let dy = (y as f32 - 37.) / 7.5;
                    let radius = (1. - dy * dy).max(0.).sqrt() * 5.;
                    let radius = radius + if y % 3 == 0 { 0.35 } else { -0.15 };
                    for z in -5i32..=5 {
                        for x in -5i32..=5 {
                            if (x * x + z * z) as f32 > radius * radius {
                                continue;
                            }
                            if x == 0 && y == 36 && z >= 1 {
                                continue;
                            }
                            put(
                                x,
                                y,
                                z,
                                if y % 3 == 0 {
                                    VOXEL_TYPE_OAK_WOOD
                                } else {
                                    VOXEL_TYPE_CHERRY_WOOD
                                },
                            );
                        }
                    }
                }
            }
        }
        cells
    }
    pub(super) fn entrance_cell(&self) -> UVec3 {
        self.local_cell(IVec3::new(
            0,
            36,
            match self.style {
                HiveStyle::Artificial => 4,
                HiveStyle::Natural => 6,
            },
        ))
    }
    pub(super) fn site(&self) -> HiveSite {
        HiveSite {
            id: self.id,
            position: (self.entrance_cell().as_vec3() + Vec3::splat(0.5)) / 256.,
        }
    }
    pub(super) fn bound(&self) -> UAabb3 {
        let base = UVec3::from(self.base);
        UAabb3::new(base - UVec3::new(6, 1, 6), base + UVec3::new(7, 46, 7))
    }
    pub(super) fn intact(&self, bytes: &[u8]) -> bool {
        let bound = self.bound();
        let dim = bound.max() - bound.min();
        if bytes.len() != dim.element_product() as usize {
            return false;
        }
        let at = |p: UVec3| {
            let p = p - bound.min();
            u32::from(bytes[(p.x + dim.x * (p.y + dim.y * p.z)) as usize] & VOXEL_TYPE_MASK)
        };
        at(UVec3::from(self.base) - UVec3::Y) != 0
            && at(self.entrance_cell()) == 0
            && self
                .cells()
                .into_iter()
                .all(|(p, material)| at(p.into()) == material)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_styles_are_connected_and_entrances_are_empty_in_every_orientation() {
        for style in [HiveStyle::Artificial, HiveStyle::Natural] {
            for quarter_turn in 0..4 {
                let h = Hive {
                    id: 1,
                    base: [100; 3],
                    style,
                    quarter_turn,
                };
                let cells = h.cells();
                assert!(!cells.contains_key(&h.entrance_cell().to_array()));
                let first = *cells.first_key_value().unwrap().0;
                let mut visited = BTreeSet::from([first]);
                let mut pending = vec![UVec3::from(first).as_ivec3()];
                while let Some(p) = pending.pop() {
                    for d in [
                        IVec3::X,
                        -IVec3::X,
                        IVec3::Y,
                        -IVec3::Y,
                        IVec3::Z,
                        -IVec3::Z,
                    ] {
                        let q = p + d;
                        let key = q.as_uvec3().to_array();
                        if cells.contains_key(&key) && visited.insert(key) {
                            pending.push(q);
                        }
                    }
                }
                assert_eq!(visited.len(), cells.len());
                assert_eq!(h.site().entrance(), h.site().position);
                let bound = h.bound();
                let dim = bound.max() - bound.min();
                let mut bytes = vec![0; dim.element_product() as usize];
                let index = |p: UVec3| {
                    let q = p - bound.min();
                    (q.x + dim.x * (q.y + dim.y * q.z)) as usize
                };
                for (p, material) in cells {
                    bytes[index(p.into())] = material as u8;
                }
                bytes[index(UVec3::from(h.base) - UVec3::Y)] = 2;
                assert!(h.intact(&bytes));
                bytes[index(h.entrance_cell())] = 7;
                assert!(!h.intact(&bytes));
                bytes[index(h.entrance_cell())] = 0;
                bytes[index(UVec3::from(h.base))] = 0;
                assert!(!h.intact(&bytes));
            }
        }
    }
}
