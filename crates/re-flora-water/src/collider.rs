use glam::{IVec3, UVec3, Vec3};
use std::{collections::HashMap, sync::Arc};

/// Axis-aligned world-space container used by the initial tiny pond test.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterBoxCollider {
    pub min_ws: Vec3,
    pub max_ws: Vec3,
}

impl WaterBoxCollider {
    pub const fn new(min_ws: Vec3, max_ws: Vec3) -> Self {
        Self { min_ws, max_ws }
    }

    pub fn extent(self) -> Vec3 {
        self.max_ws - self.min_ws
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn contains(self, point_ws: Vec3) -> bool {
        point_ws.cmpge(self.min_ws).all() && point_ws.cmple(self.max_ws).all()
    }

    pub fn clamp_point(self, point_ws: Vec3, padding: f32) -> Vec3 {
        point_ws.clamp(
            self.min_ws + Vec3::splat(padding),
            self.max_ws - Vec3::splat(padding),
        )
    }
}

impl Default for WaterBoxCollider {
    fn default() -> Self {
        // Keep the initial pond in terrain chunk (1, 0, 1). The app builds the
        // first water terrain collider for that chunk-aligned box.
        Self::new(Vec3::new(1.0, 0.0, 1.0), Vec3::new(2.0, 1.0, 2.0))
    }
}

/// One chunk-aligned 3D signed-distance terrain collider chunk.
///
/// The world-space bounds are always derived from `chunk_id`: `[chunk_id, chunk_id + 1]`.
/// `sdf_ws` uses the convention needed by the water solver:
/// negative values are terrain solid, positive values are empty/water, and the
/// gradient points away from solid terrain.
#[derive(Clone, Debug, PartialEq)]
pub struct WaterTerrainColliderChunk {
    pub chunk_id: IVec3,
    pub dim: UVec3,
    pub sdf_ws: Vec<f32>,
    pub revision: u64,
}

impl WaterTerrainColliderChunk {
    pub fn validate(&self) {
        assert!(self.dim.x >= 2 && self.dim.y >= 2 && self.dim.z >= 2);

        let expected_len =
            self.dim
                .x
                .checked_mul(self.dim.y)
                .and_then(|xy| xy.checked_mul(self.dim.z))
                .expect("terrain collider chunk dimensions overflow") as usize;
        assert_eq!(self.sdf_ws.len(), expected_len);
        assert!(self.sdf_ws.iter().all(|sdf| sdf.is_finite()));
    }

    pub fn bounds_min_ws(&self) -> Vec3 {
        self.chunk_id.as_vec3()
    }

    pub fn bounds_max_ws(&self) -> Vec3 {
        self.bounds_min_ws() + Vec3::ONE
    }

    pub fn bounds_ws(&self) -> (Vec3, Vec3) {
        (self.bounds_min_ws(), self.bounds_max_ws())
    }

    pub fn contains_ws(&self, point_ws: Vec3) -> bool {
        let (bounds_min_ws, bounds_max_ws) = self.bounds_ws();
        point_ws.cmpge(bounds_min_ws).all() && point_ws.cmple(bounds_max_ws).all()
    }

    pub fn sample_sdf_ws(&self, point_ws: Vec3) -> Option<f32> {
        if !self.contains_ws(point_ws) {
            return None;
        }
        Some(self.sample_sdf_clamped_ws(point_ws))
    }

    pub fn sample_normal_ws(&self, point_ws: Vec3) -> Option<Vec3> {
        if !self.contains_ws(point_ws) {
            return None;
        }
        Some(self.sample_normal_clamped_ws(point_ws))
    }

    pub fn sample_sdf_and_normal_ws(&self, point_ws: Vec3) -> Option<(f32, Vec3)> {
        if !self.contains_ws(point_ws) {
            return None;
        }
        Some((
            self.sample_sdf_clamped_ws(point_ws),
            self.sample_normal_clamped_ws(point_ws),
        ))
    }

    pub fn cell_size_ws(&self) -> Vec3 {
        Vec3::ONE / (self.dim - UVec3::ONE).as_vec3()
    }

    fn sample_sdf_clamped_ws(&self, point_ws: Vec3) -> f32 {
        debug_assert!(self.dim.x >= 2 && self.dim.y >= 2 && self.dim.z >= 2);
        debug_assert_eq!(
            self.sdf_ws.len(),
            (self.dim.x as usize) * (self.dim.y as usize) * (self.dim.z as usize)
        );

        let (bounds_min_ws, bounds_max_ws) = self.bounds_ws();
        let extent = bounds_max_ws - bounds_min_ws;
        debug_assert!(extent.cmpgt(Vec3::ZERO).all());

        let uvw = ((point_ws - bounds_min_ws) / extent).clamp(Vec3::ZERO, Vec3::ONE);
        let grid_pos = uvw * (self.dim - UVec3::ONE).as_vec3();
        let base = grid_pos.floor().as_uvec3().min(self.dim - UVec3::splat(2));
        let next = base + UVec3::ONE;
        let f = grid_pos - base.as_vec3();

        let c000 = self.sdf_at(base.x, base.y, base.z);
        let c100 = self.sdf_at(next.x, base.y, base.z);
        let c010 = self.sdf_at(base.x, next.y, base.z);
        let c110 = self.sdf_at(next.x, next.y, base.z);
        let c001 = self.sdf_at(base.x, base.y, next.z);
        let c101 = self.sdf_at(next.x, base.y, next.z);
        let c011 = self.sdf_at(base.x, next.y, next.z);
        let c111 = self.sdf_at(next.x, next.y, next.z);

        let x00 = lerp(c000, c100, f.x);
        let x10 = lerp(c010, c110, f.x);
        let x01 = lerp(c001, c101, f.x);
        let x11 = lerp(c011, c111, f.x);
        let y0 = lerp(x00, x10, f.y);
        let y1 = lerp(x01, x11, f.y);
        lerp(y0, y1, f.z)
    }

    fn sample_normal_clamped_ws(&self, point_ws: Vec3) -> Vec3 {
        let cell_size = self.cell_size_ws();
        let dx = self.sample_axis_derivative(point_ws, Vec3::X, cell_size.x);
        let dy = self.sample_axis_derivative(point_ws, Vec3::Y, cell_size.y);
        let dz = self.sample_axis_derivative(point_ws, Vec3::Z, cell_size.z);
        let normal = Vec3::new(dx, dy, dz).normalize_or_zero();
        if normal.is_finite() && normal.length_squared() > 0.0 {
            normal
        } else {
            Vec3::Y
        }
    }

    fn sample_axis_derivative(&self, point_ws: Vec3, axis: Vec3, step_ws: f32) -> f32 {
        if step_ws <= 0.0 || !step_ws.is_finite() {
            return 0.0;
        }

        let (bounds_min_ws, bounds_max_ws) = self.bounds_ws();
        let p0 = (point_ws - axis * step_ws).clamp(bounds_min_ws, bounds_max_ws);
        let p1 = (point_ws + axis * step_ws).clamp(bounds_min_ws, bounds_max_ws);
        let denom = (p1 - p0).dot(axis);
        if denom.abs() <= f32::EPSILON {
            return 0.0;
        }

        (self.sample_sdf_clamped_ws(p1) - self.sample_sdf_clamped_ws(p0)) / denom
    }

    fn sdf_at(&self, x: u32, y: u32, z: u32) -> f32 {
        self.sdf_ws
            [((z as usize * self.dim.y as usize + y as usize) * self.dim.x as usize) + x as usize]
    }
}

/// Cache of terrain collider chunks indexed by water collider chunk id.
#[derive(Clone, Debug, Default)]
pub struct WaterTerrainColliderSet {
    pub chunks: HashMap<IVec3, Arc<WaterTerrainColliderChunk>>,
}

impl WaterTerrainColliderSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_chunk(chunk: WaterTerrainColliderChunk) -> Self {
        let mut set = Self::new();
        set.insert_chunk(Arc::new(chunk));
        set
    }

    pub fn insert_chunk(&mut self, chunk: Arc<WaterTerrainColliderChunk>) {
        chunk.validate();
        self.chunks.insert(chunk.chunk_id, chunk);
    }

    pub fn validate(&self) {
        for chunk in self.chunks.values() {
            chunk.validate();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn sample_sdf_ws(&self, point_ws: Vec3) -> Option<f32> {
        self.sample_sdf_and_normal_ws(point_ws).map(|(sdf, _)| sdf)
    }

    pub fn sample_normal_ws(&self, point_ws: Vec3) -> Option<Vec3> {
        self.sample_sdf_and_normal_ws(point_ws)
            .map(|(_, normal)| normal)
    }

    pub fn sample_sdf_and_normal_ws(&self, point_ws: Vec3) -> Option<(f32, Vec3)> {
        self.chunks
            .values()
            .filter_map(|chunk| chunk.sample_sdf_and_normal_ws(point_ws))
            .min_by(|(a, _), (b, _)| match (*a < 0.0, *b < 0.0) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.abs().total_cmp(&b.abs()),
            })
    }

    pub fn bounds_ws(&self) -> Option<(Vec3, Vec3)> {
        let mut chunks = self.chunks.values();
        let first = chunks.next()?;
        let (mut min_ws, mut max_ws) = first.bounds_ws();
        for chunk in chunks {
            let (chunk_min_ws, chunk_max_ws) = chunk.bounds_ws();
            min_ws = min_ws.min(chunk_min_ws);
            max_ws = max_ws.max(chunk_max_ws);
        }
        Some((min_ws, max_ws))
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_chunk_samples_trilinear_sdf() {
        let chunk = WaterTerrainColliderChunk {
            chunk_id: IVec3::ZERO,
            dim: UVec3::new(2, 2, 2),
            sdf_ws: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
            revision: 7,
        };

        chunk.validate();
        assert!((chunk.sample_sdf_ws(Vec3::splat(0.5)).unwrap() - 3.5).abs() < 1.0e-6);
    }

    #[test]
    fn terrain_chunk_rejects_outside_samples() {
        let chunk = flat_floor_chunk(0.25);

        assert!(chunk.sample_sdf_ws(Vec3::new(-0.1, 0.5, 0.5)).is_none());
        assert!(chunk.sample_normal_ws(Vec3::new(0.5, 0.5, 0.5)).is_some());
    }

    #[test]
    fn terrain_chunk_samples_floor_normal_from_sdf_gradient() {
        let chunk = flat_floor_chunk(0.25);

        assert_vec3_near(
            chunk.sample_normal_ws(Vec3::new(0.5, 0.5, 0.5)).unwrap(),
            Vec3::Y,
        );
    }

    #[test]
    fn terrain_chunk_samples_ceiling_normal_from_sdf_gradient() {
        let chunk = sdf_chunk(|p| 0.75 - p.y);

        assert_vec3_near(
            chunk.sample_normal_ws(Vec3::new(0.5, 0.5, 0.5)).unwrap(),
            -Vec3::Y,
        );
    }

    #[test]
    fn terrain_collider_set_selects_containing_chunk() {
        let left = WaterTerrainColliderChunk {
            chunk_id: IVec3::ZERO,
            dim: UVec3::splat(2),
            sdf_ws: vec![1.0; 8],
            revision: 1,
        };
        let right = WaterTerrainColliderChunk {
            chunk_id: IVec3::X,
            dim: UVec3::splat(2),
            sdf_ws: vec![2.0; 8],
            revision: 1,
        };
        let mut set = WaterTerrainColliderSet::new();
        set.insert_chunk(Arc::new(left));
        set.insert_chunk(Arc::new(right));

        assert_eq!(set.sample_sdf_ws(Vec3::new(0.25, 0.5, 0.5)), Some(1.0));
        assert_eq!(set.sample_sdf_ws(Vec3::new(1.25, 0.5, 0.5)), Some(2.0));
        assert!(set.sample_sdf_ws(Vec3::new(2.25, 0.5, 0.5)).is_none());
    }

    #[test]
    #[should_panic]
    fn terrain_chunk_rejects_invalid_sdf_count() {
        WaterTerrainColliderChunk {
            chunk_id: IVec3::ZERO,
            dim: UVec3::new(2, 2, 2),
            sdf_ws: vec![0.0; 7],
            revision: 0,
        }
        .validate();
    }

    fn flat_floor_chunk(floor_y: f32) -> WaterTerrainColliderChunk {
        sdf_chunk(|p| p.y - floor_y)
    }

    fn sdf_chunk(sdf: impl Fn(Vec3) -> f32) -> WaterTerrainColliderChunk {
        let dim = UVec3::new(4, 4, 4);
        let mut sdf_ws = Vec::new();
        for z in 0..dim.z {
            for y in 0..dim.y {
                for x in 0..dim.x {
                    let p = Vec3::new(
                        x as f32 / (dim.x - 1) as f32,
                        y as f32 / (dim.y - 1) as f32,
                        z as f32 / (dim.z - 1) as f32,
                    );
                    sdf_ws.push(sdf(p));
                }
            }
        }
        WaterTerrainColliderChunk {
            chunk_id: IVec3::ZERO,
            dim,
            sdf_ws,
            revision: 0,
        }
    }

    fn assert_vec3_near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() < 1.0e-6,
            "actual {actual:?} expected {expected:?}"
        );
    }
}
