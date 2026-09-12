//! Detached-leaf optical encoding. All particle geometry stays screen-facing.
use crate::particles::{ParticleRenderKind, ParticleSnapshot};
use glam::Vec3;

// Mirrored by leaf_particle_pose.slang. Bit 31 remains the existing sprite flip.
const LEAF_FLIGHT_BIT: u32 = 1 << 30;

pub(super) fn encode(snapshot: &ParticleSnapshot) -> ([f32; 4], u32) {
    if snapshot.kind != ParticleRenderKind::Leaf {
        return ([0.0; 4], 0);
    }
    if let Some(orientation) = snapshot.leaf_orientation {
        return (orientation.to_array(), LEAF_FLIGHT_BIT);
    }
    // Non-falling leaf-colored particles retain their existing optical proxy.
    let velocity = snapshot.velocity;
    let normal = Vec3::new(-velocity.x, velocity.y.abs() + 0.05, -velocity.z).normalize();
    (normal.extend(1.0).to_array(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::{ParticleForces, ParticleSpawn, ParticleSystem};
    use glam::Quat;

    #[test]
    fn falling_leaf_optics_always_use_the_published_simulation_pose() {
        let mut system = ParticleSystem::new(1);
        system
            .spawn(ParticleSpawn {
                position: Vec3::new(0., 2., 0.),
                size: crate::particles::STANDARD_PARTICLE_SIZE,
                lifetime: 30.,
                ..ParticleSpawn::default()
            })
            .unwrap();
        let mut snapshots = Vec::new();
        let mut previous = None;
        let mut pose_changes = 0;
        for _ in 0..360 {
            system.update(1. / 120., ParticleForces::default());
            system.write_snapshots(&mut snapshots);
            let snapshot = &snapshots[0];
            let billboard = encode(snapshot);
            assert_eq!(billboard.0, snapshot.leaf_orientation.unwrap().to_array());
            assert_eq!(billboard.1, LEAF_FLIGHT_BIT);
            if previous.is_some_and(|pose| pose != billboard.0) {
                assert!(system.last_tick_step().did_step);
                pose_changes += 1;
            }
            previous = Some(billboard.0);
        }
        assert!(
            pose_changes > 10,
            "optical pose must keep evolving on world ticks"
        );
    }

    #[test]
    fn non_falling_and_non_leaf_particles_keep_their_existing_optical_inputs() {
        let mut system = ParticleSystem::new(1);
        system
            .spawn(ParticleSpawn {
                motion_mode: crate::particles::MotionMode::Free,
                ..ParticleSpawn::default()
            })
            .unwrap();
        let mut snapshots = Vec::new();
        system.write_snapshots(&mut snapshots);
        let mut snapshot = snapshots[0];
        assert_eq!(encode(&snapshot), ([0., 1., 0., 1.], 0));
        for kind in [
            ParticleRenderKind::Butterfly,
            ParticleRenderKind::WaterDroplet,
            ParticleRenderKind::TerrainVoxel,
        ] {
            snapshot.kind = kind;
            snapshot.leaf_orientation = Some(Quat::IDENTITY);
            assert_eq!(encode(&snapshot), ([0.; 4], 0));
        }
    }
}
