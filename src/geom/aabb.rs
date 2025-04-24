use glam::Vec3;

#[derive(Debug, Clone)]
pub struct Aabb {
    min: Vec3,
    max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn min(&self) -> Vec3 {
        self.min
    }

    pub fn max(&self) -> Vec3 {
        self.max
    }

    /// Returns a new Aabb that encloses both self and the other Aabb.
    ///
    /// The offset is invalid after this operation.
    pub fn union(&self, other: &Aabb) -> Aabb {
        let min = self.min().min(other.min());
        let max = self.max().max(other.max());
        Aabb::new(min, max)
    }

    pub fn center(&self) -> Vec3 {
        (self.max() + self.min()) * 0.5
    }

    pub fn dimensions(&self) -> Vec3 {
        self.max() - self.min()
    }

    // utilities:
    
    /// Compute the union (bounding box) of the chunks in the range [start, end)
    pub fn get_union_aabb(aabbs: &[Aabb], start: usize, end: usize) -> Aabb {
        let mut aabb = aabbs[start].clone();
        for i in start + 1..end {
            aabb = aabb.union(&aabbs[i]);
        }
        aabb
    }
}
