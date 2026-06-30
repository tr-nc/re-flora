use crate::app::world_edits::TerrainBrushEdit;
use glam::{UVec3, Vec2, Vec3};

/// The currently unlocked terrain editing area.
///
/// Terrain chunks are addressed in chunk-space, where each terrain chunk spans one world unit
/// in X/Z. The max corner is exclusive for point membership, matching chunk index semantics:
/// a point at x == max.x belongs to the neighboring chunk, not this range.
pub(crate) const INITIAL_EDITABLE_TERRAIN_BOUNDS: EditableTerrainBounds =
    EditableTerrainBounds::single_chunk(UVec3::new(1, 0, 1));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EditableTerrainBounds {
    chunk_min: UVec3,
    chunk_max_exclusive: UVec3,
}

impl EditableTerrainBounds {
    pub(crate) const fn single_chunk(chunk_index: UVec3) -> Self {
        Self::from_chunk_range(
            chunk_index,
            UVec3::new(chunk_index.x + 1, chunk_index.y + 1, chunk_index.z + 1),
        )
    }

    pub(crate) const fn from_chunk_range(chunk_min: UVec3, chunk_max_exclusive: UVec3) -> Self {
        Self {
            chunk_min,
            chunk_max_exclusive,
        }
    }

    pub(crate) const fn center(self) -> Vec3 {
        Vec3::new(
            self.chunk_min.x as f32 + (self.chunk_max_exclusive.x - self.chunk_min.x) as f32 * 0.5,
            self.chunk_min.y as f32 + (self.chunk_max_exclusive.y - self.chunk_min.y) as f32 * 0.5,
            self.chunk_min.z as f32 + (self.chunk_max_exclusive.z - self.chunk_min.z) as f32 * 0.5,
        )
    }

    pub(crate) fn contains_point_xz(self, point: Vec3) -> bool {
        self.contains_disc_xz(point, 0.0)
    }

    pub(crate) fn contains_disc_xz(self, center: Vec3, radius: f32) -> bool {
        if !center.is_finite() || !radius.is_finite() || radius < 0.0 {
            return false;
        }

        let min = self.min_xz();
        let max = self.max_xz_exclusive();
        center.x >= min.x
            && center.z >= min.y
            && center.x < max.x
            && center.z < max.y
            && center.x - radius >= min.x
            && center.z - radius >= min.y
            && center.x + radius <= max.x
            && center.z + radius <= max.y
    }

    pub(crate) fn contains_brush_stroke(self, edit: TerrainBrushEdit) -> bool {
        self.contains_disc_xz(edit.start, edit.radius)
            && self.contains_disc_xz(edit.end, edit.radius)
    }

    fn min_xz(self) -> Vec2 {
        Vec2::new(self.chunk_min.x as f32, self.chunk_min.z as f32)
    }

    fn max_xz_exclusive(self) -> Vec2 {
        Vec2::new(
            self.chunk_max_exclusive.x as f32,
            self.chunk_max_exclusive.z as f32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::INITIAL_EDITABLE_TERRAIN_BOUNDS;
    use crate::app::world_edits::TerrainBrushEdit;
    use glam::Vec3;

    #[test]
    fn initial_editable_area_accepts_only_middle_chunk_xz() {
        assert!(INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_disc_xz(Vec3::new(1.5, 0.5, 1.5), 0.25));
        assert!(INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_disc_xz(Vec3::new(1.92, 0.5, 1.5), 0.08));
        assert!(INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_point_xz(Vec3::new(1.0, 0.5, 1.0)));

        assert!(!INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_point_xz(Vec3::new(0.9, 0.5, 1.5)));
        assert!(!INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_disc_xz(Vec3::new(1.05, 0.5, 1.5), 0.08));
        assert!(!INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_point_xz(Vec3::new(2.0, 0.5, 1.5)));
    }

    #[test]
    fn initial_editable_area_requires_whole_stroke_in_middle_chunk_xz() {
        assert!(
            INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_brush_stroke(TerrainBrushEdit {
                start: Vec3::new(1.2, 0.5, 1.2),
                end: Vec3::new(1.8, 0.5, 1.8),
                radius: 0.1,
            })
        );
        assert!(
            !INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_brush_stroke(TerrainBrushEdit {
                start: Vec3::new(1.5, 0.5, 1.5),
                end: Vec3::new(2.05, 0.5, 1.5),
                radius: 0.1,
            })
        );
    }
}
