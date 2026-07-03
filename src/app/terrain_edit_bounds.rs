use crate::app::{core::CHUNK_DIM, world_edits::TerrainBrushEdit};
use glam::{UVec3, Vec2, Vec3};

/// The currently unlocked terrain editing area.
///
/// Terrain chunks are addressed in chunk-space, where each terrain chunk spans one world unit
/// in X/Z. The max corner is exclusive for point membership, matching chunk index semantics:
/// a point at x == max.x belongs to the neighboring chunk, not this range.
pub(crate) const INITIAL_EDITABLE_TERRAIN_BOUNDS: EditableTerrainBounds =
    EditableTerrainBounds::from_chunk_range(UVec3::ZERO, CHUNK_DIM);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EditableTerrainBounds {
    chunk_min: UVec3,
    chunk_max_exclusive: UVec3,
}

impl EditableTerrainBounds {
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
        if !point.is_finite() {
            return false;
        }

        let min = self.min_xz();
        let max = self.max_xz_exclusive();
        point.x >= min.x && point.z >= min.y && point.x < max.x && point.z < max.y
    }

    pub(crate) fn contains_brush_endpoint(self, edit: TerrainBrushEdit) -> bool {
        self.contains_point_xz(edit.end)
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
    fn initial_editable_area_accepts_points_in_unlocked_chunks_xz() {
        assert!(INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_point_xz(Vec3::new(1.5, 0.5, 1.5)));
        assert!(INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_point_xz(Vec3::new(0.0, 0.5, 0.0)));
        assert!(INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_point_xz(Vec3::new(1.99, 0.5, 1.5)));

        assert!(!INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_point_xz(Vec3::new(-0.01, 0.5, 1.5)));
        assert!(!INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_point_xz(Vec3::new(2.0, 0.5, 1.5)));
    }

    #[test]
    fn editable_area_checks_only_brush_endpoint_xz() {
        assert!(
            INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_brush_endpoint(TerrainBrushEdit {
                start: Vec3::new(-1.0, 0.5, -1.0),
                end: Vec3::new(0.02, 0.5, 0.02),
                radius: 0.5,
            })
        );
        assert!(
            !INITIAL_EDITABLE_TERRAIN_BOUNDS.contains_brush_endpoint(TerrainBrushEdit {
                start: Vec3::new(1.5, 0.5, 1.5),
                end: Vec3::new(3.0, 0.5, 1.5),
                radius: 0.1,
            })
        );
    }
}
