//! Optical proxy for non-leaf harvest/debug points. Actual leaves only use the
//! shared model renderer; there is no leaf-sprite encoding or fallback here.
use crate::particles::{ParticleRenderKind, ParticleSnapshot};
use glam::Vec3;

pub(super) fn encode(snapshot: &ParticleSnapshot) -> [f32; 4] {
    if snapshot.kind != ParticleRenderKind::LitDebris {
        return [0.; 4];
    }
    let v = snapshot.velocity;
    Vec3::new(-v.x, v.y.abs() + 0.05, -v.z)
        .normalize()
        .extend(1.)
        .to_array()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::{ParticleSpawn, ParticleSystem};
    #[test]
    fn non_leaf_optical_proxy_is_preserved_without_a_leaf_sprite_path() {
        let mut system = ParticleSystem::new(1);
        system.spawn(ParticleSpawn::default()).unwrap();
        let mut snapshots = Vec::new();
        system.write_snapshots(&mut snapshots);
        let mut sample = snapshots[0];
        assert_eq!(encode(&sample), [0.; 4]);
        sample.kind = ParticleRenderKind::LitDebris;
        assert_eq!(encode(&sample), [0., 1., 0., 1.]);
        sample.kind = ParticleRenderKind::WaterDroplet;
        assert_eq!(encode(&sample), [0.; 4]);
    }
}
