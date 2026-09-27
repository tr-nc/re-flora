//! One terrain description shared by analytic tests and the real editable voxel fixtures.
use glam::{IVec3, UVec3, Vec3};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fixture {
    #[default]
    Flat,
    Hole,
    Outward,
    Inward,
    Slope,
    Ground,
    Pole,
}

impl Fixture {
    pub const ALL: [Self; 7] = [
        Self::Flat,
        Self::Hole,
        Self::Outward,
        Self::Inward,
        Self::Slope,
        Self::Ground,
        Self::Pole,
    ];
    pub fn from_index(index: u32) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or_default()
    }
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Hole => "hole",
            Self::Outward => "outward",
            Self::Inward => "inward",
            Self::Slope => "slope",
            Self::Ground => "ground",
            Self::Pole => "pole",
        }
    }
    pub fn bounds() -> (UVec3, UVec3) {
        (UVec3::new(224, 190, 280), UVec3::new(288, 302, 366))
    }
    pub fn seed(self) -> (Vec3, Vec3, IVec3) {
        if self == Self::Pole {
            (
                Vec3::new(255.5, 198.5, 316.8),
                Vec3::Z,
                IVec3::new(255, 198, 315),
            )
        } else if self == Self::Ground {
            (
                Vec3::new(255.5, 192.83, 310.5),
                Vec3::Y,
                IVec3::new(255, 191, 310),
            )
        } else {
            (
                Vec3::new(255.5, 198.5, 306.8),
                Vec3::Z,
                IVec3::new(255, 198, 305),
            )
        }
    }
    fn depth(self, y: i32) -> (i32, i32) {
        match self {
            Self::Outward if y >= 242 => (300, 314),
            Self::Inward => (288, if y >= 242 { 298 } else { 306 }),
            Self::Slope => {
                let front = 303 + (y - 192) / 2;
                (front - 6, front)
            }
            _ => (300, 306),
        }
    }
    pub fn hole(self) -> Option<(UVec3, UVec3)> {
        (self == Self::Hole).then_some((UVec3::new(249, 232, 280), UVec3::new(263, 250, 366)))
    }
    #[cfg(test)]
    pub fn solid(self, cell: IVec3) -> bool {
        if self == Self::Pole {
            return self.boxes().iter().any(|(min, max)| {
                cell.cmpge(min.as_ivec3()).all() && cell.cmplt(max.as_ivec3()).all()
            });
        }
        if (190..192).contains(&cell.y) {
            let (back, front) = self.footing_depth();
            return (224..288).contains(&cell.x) && (back..front).contains(&cell.z);
        }
        if !(224..288).contains(&cell.x) || !(192..300).contains(&cell.y) {
            return false;
        }
        let (back, front) = self.depth(cell.y);
        if !(back..front).contains(&cell.z) {
            return false;
        }
        !self.hole().is_some_and(|(min, max)| {
            cell.cmpge(min.as_ivec3()).all() && cell.cmplt(max.as_ivec3()).all()
        })
    }
    fn footing_depth(self) -> (i32, i32) {
        if self == Self::Ground {
            (288, 326)
        } else {
            self.depth(192)
        }
    }
    /// Merge identical adjacent height layers into cuboids for a single terrain transaction.
    /// Every shape has a two-voxel footing embedded in the sampled natural ground.
    pub fn boxes(self) -> Vec<(UVec3, UVec3)> {
        if self == Self::Pole {
            return vec![
                (UVec3::new(242, 190, 292), UVec3::new(270, 192, 320)),
                (UVec3::new(246, 192, 296), UVec3::new(266, 300, 316)),
            ];
        }
        let (back, front) = self.footing_depth();
        let mut boxes = vec![(
            UVec3::new(224, 190, back as u32),
            UVec3::new(288, 192, front as u32),
        )];
        let mut y = 192;
        while y < 300 {
            let (back, front) = self.depth(y);
            let mut end = y + 1;
            while end < 300 && self.depth(end) == (back, front) {
                end += 1;
            }
            boxes.push((
                UVec3::new(224, y as u32, back as u32),
                UVec3::new(288, end as u32, front as u32),
            ));
            y = end;
        }
        boxes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_cuboids_match_the_analytic_support_fixtures() {
        for fixture in Fixture::ALL {
            let boxes = fixture.boxes();
            for y in 190..302 {
                for z in 278..368 {
                    for x in [223, 224, 248, 249, 255, 262, 263, 287, 288] {
                        let cell = IVec3::new(x, y, z);
                        let in_box = boxes.iter().any(|(min, max)| {
                            cell.cmpge(min.as_ivec3()).all() && cell.cmplt(max.as_ivec3()).all()
                        });
                        let in_hole = fixture.hole().is_some_and(|(min, max)| {
                            cell.cmpge(min.as_ivec3()).all() && cell.cmplt(max.as_ivec3()).all()
                        });
                        assert_eq!(
                            fixture.solid(cell),
                            in_box && !in_hole,
                            "{fixture:?} {cell:?}"
                        );
                    }
                }
            }
        }
    }
}
