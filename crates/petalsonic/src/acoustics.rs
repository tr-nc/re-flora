use crate::math::Vec3;
use std::sync::Arc;

/// A world-space acoustics ray.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcousticRay {
    pub origin: Vec3,
    pub direction: Vec3,
}

/// Acoustic material properties used by custom ray tracing backends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcousticMaterial {
    /// Low, mid, and high band absorption in the inclusive range 0..=1.
    pub absorption: [f32; 3],
    /// Diffuse reflection ratio in the inclusive range 0..=1.
    pub scattering: f32,
    /// Low, mid, and high direct-path transmission in the inclusive range 0..=1.
    pub transmission: [f32; 3],
}

impl Default for AcousticMaterial {
    fn default() -> Self {
        Self {
            absorption: [0.10, 0.20, 0.30],
            scattering: 0.05,
            transmission: [0.100, 0.050, 0.030],
        }
    }
}

/// Closest-hit result for an acoustics ray.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcousticHit {
    pub distance: f32,
    pub normal: Vec3,
    pub material: AcousticMaterial,
}

/// Immutable ray-query view of exactly one captured geometry generation.
///
/// Implementations may share immutable acceleration chunks with the host, but must never hide a
/// mutable scene behind this interface. PetalSonic calls these methods only from its acoustics
/// worker, so implementations may use ordinary CPU traversal; they must still keep batch work
/// bounded and must not access mutable game state.
pub trait AcousticRayQuerySnapshot: Send + Sync {
    /// Writes one result per ray. All input and output slices have the same length.
    fn trace_any_hit_batch(
        &self,
        rays: &[AcousticRay],
        min_distances: &[f32],
        max_distances: &[f32],
        hits: &mut [bool],
    );

    /// Writes one closest-hit result per ray. All input and output slices have the same length.
    fn trace_closest_hit_batch(
        &self,
        rays: &[AcousticRay],
        min_distances: &[f32],
        max_distances: &[f32],
        hits: &mut [Option<AcousticHit>],
    );
}

/// Immutable, shareable acoustic-scene generation.
#[derive(Clone)]
pub struct AcousticSceneSnapshot {
    version: u64,
    query: Arc<dyn AcousticRayQuerySnapshot>,
}

impl AcousticSceneSnapshot {
    pub fn new(version: u64, query: Arc<dyn AcousticRayQuerySnapshot>) -> Self {
        Self { version, query }
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub(crate) fn query(&self) -> &Arc<dyn AcousticRayQuerySnapshot> {
        &self.query
    }
}

impl std::fmt::Debug for AcousticSceneSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcousticSceneSnapshot")
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PlaneSnapshot;

    impl AcousticRayQuerySnapshot for PlaneSnapshot {
        fn trace_any_hit_batch(
            &self,
            rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            for (ray, hit) in rays.iter().zip(hits.iter_mut()) {
                *hit = ray.direction.y < 0.0;
            }
        }

        fn trace_closest_hit_batch(
            &self,
            rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            for (ray, hit) in rays.iter().zip(hits.iter_mut()) {
                *hit = (ray.direction.y < 0.0).then_some(AcousticHit {
                    distance: 1.0,
                    normal: Vec3::Y,
                    material: AcousticMaterial::default(),
                });
            }
        }
    }

    #[test]
    fn scene_snapshot_keeps_one_immutable_query_generation() {
        let scene = AcousticSceneSnapshot::new(7, Arc::new(PlaneSnapshot));
        let rays = [AcousticRay {
            origin: Vec3::ZERO,
            direction: -Vec3::Y,
        }];
        let mut hits = [false];
        scene
            .query()
            .trace_any_hit_batch(&rays, &[0.0], &[10.0], &mut hits);
        assert_eq!(scene.version(), 7);
        assert_eq!(hits, [true]);
    }
}
