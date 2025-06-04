use glam::{UVec3, Vec3};

#[derive(Debug, Clone)]
pub struct Aabb3 {
    min: Vec3,
    max: Vec3,
}

impl Default for Aabb3 {
    fn default() -> Self {
        Self {
            min: Vec3::ZERO,
            max: Vec3::ZERO,
        }
    }
}

impl Aabb3 {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn min(&self) -> Vec3 {
        self.min
    }

    pub fn min_uvec3(&self) -> glam::UVec3 {
        self.min.floor().as_uvec3()
    }

    pub fn max(&self) -> Vec3 {
        self.max
    }

    pub fn max_uvec3(&self) -> glam::UVec3 {
        self.max.ceil().as_uvec3()
    }

    /// Returns a new `Aabb3` that encloses both `self` and `other`.
    /// If an AABB has no size, it's not considered in the union.
    /// If `self` has no size, `other` is returned (cloned).
    /// If `other` has no size, `self` is returned (cloned).
    /// If neither has size, a default AABB is returned.
    pub fn union(&self, other: &Aabb3) -> Aabb3 {
        let self_has_size = self.has_size();
        let other_has_size = other.has_size();

        if self_has_size && other_has_size {
            let min = self.min().min(other.min());
            let max = self.max().max(other.max());
            Aabb3::new(min, max)
        } else if self_has_size {
            self.clone() // other has no size
        } else if other_has_size {
            other.clone() // self has no size
        } else {
            Aabb3::default() // neither has size
        }
    }

    pub fn center(&self) -> Vec3 {
        (self.max() + self.min()) * 0.5
    }

    pub fn dimensions(&self) -> Vec3 {
        self.max() - self.min()
    }

    pub fn has_size(&self) -> bool {
        self.min.x < self.max.x && self.min.y < self.max.y && self.min.z < self.max.z
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UAabb3 {
    min: UVec3,
    max: UVec3,
}

impl Default for UAabb3 {
    fn default() -> Self {
        // Default to an empty AABB with min at (0, 0, 0) and max at (0, 0, 0)
        Self {
            min: UVec3::ZERO,
            max: UVec3::ZERO,
        }
    }
}

impl UAabb3 {
    /// Creates a new `UAabb3`.
    ///
    /// It's usually a good idea to ensure `min` is less than or equal to `max`
    /// on each axis, though this constructor doesn't enforce it.
    pub fn new(min: UVec3, max: UVec3) -> Self {
        Self { min, max }
    }

    /// Returns the minimum corner of the `UAabb3`.
    pub fn min(&self) -> UVec3 {
        self.min
    }

    /// Returns the maximum corner of the `UAabb3`.
    pub fn max(&self) -> UVec3 {
        self.max
    }

    /// Returns a new `UAabb3` that encloses both `self` and `other`.
    /// If an AABB has no size, it's not considered in the union.
    /// If `self` has no size, `other` is returned.
    /// If `other` has no size, `self` is returned.
    /// If neither has size, a default UAabb3 is returned.
    pub fn union(&self, other: &UAabb3) -> UAabb3 {
        let self_has_size = self.has_size();
        let other_has_size = other.has_size();

        if self_has_size && other_has_size {
            let min = self.min().min(other.min());
            let max = self.max().max(other.max());
            UAabb3::new(min, max)
        } else if self_has_size {
            *self // other has no size
        } else if other_has_size {
            *other // self has no size
        } else {
            UAabb3::default() // neither has size
        }
    }

    /// Returns the center of the `UAabb3` as a `Vec3`.
    ///
    /// Since the min and max are unsigned integers, the center might
    /// not be an integer, so `Vec3` is used for precision.
    pub fn center(&self) -> Vec3 {
        (self.min.as_vec3() + self.max.as_vec3()) * 0.5
    }

    /// Returns the dimensions (width, height, depth) of the `UAabb3`.
    ///
    /// This will panic if `max` is less than `min` on any axis due to unsigned subtraction.
    /// Consider adding checks or using `saturating_sub` if this is a concern.
    pub fn dimensions(&self) -> UVec3 {
        self.max - self.min
    }

    /// Checks if the AABB has a valid range (min <= max on all axes).
    pub fn is_valid(&self) -> bool {
        self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z
    }

    /// Returns the width of the AABB (x-axis).
    /// Panics if `max.x < min.x`.
    pub fn width(&self) -> u32 {
        self.max.x - self.min.x
    }

    /// Returns the height of the AABB (y-axis).
    /// Panics if `max.y < min.y`.
    pub fn height(&self) -> u32 {
        self.max.y - self.min.y
    }

    /// Returns the depth of the AABB (z-axis).
    /// Panics if `max.z < min.z`.
    pub fn depth(&self) -> u32 {
        self.max.z - self.min.z
    }

    /// Checks if a point is contained within the AABB (inclusive).
    pub fn contains_point(&self, point: UVec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// Checks if another AABB is fully contained within this AABB.
    pub fn contains_aabb(&self, other: &UAabb3) -> bool {
        self.min.x <= other.min.x
            && self.min.y <= other.min.y
            && self.min.z <= other.min.z
            && self.max.x >= other.max.x
            && self.max.y >= other.max.y
            && self.max.z >= other.max.z
    }

    /// Checks if this AABB intersects with another AABB.
    /// Two AABBs intersect if they overlap in all three dimensions.
    pub fn intersects(&self, other: &UAabb3) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
            && self.min.z < other.max.z
            && self.max.z > other.min.z
    }

    /// Checks if the AABB has a positive size in all dimensions
    /// (i.e., min < max on all axes).
    pub fn has_size(&self) -> bool {
        self.min.x < self.max.x && self.min.y < self.max.y && self.min.z < self.max.z
    }
}
