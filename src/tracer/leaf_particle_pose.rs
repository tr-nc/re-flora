//! Detached-leaf render encoding. Geometry choice never enters the particle simulation.
use crate::particles::{ParticleRenderKind, ParticleSnapshot};
use glam::Vec3;

// Mirrored by leaf_particle_pose.slang. Bit 31 remains the existing sprite flip.
const LEAF_FLIGHT_BIT: u32 = 1 << 30;
const LEAF_ROTATING_PLATE_BIT: u32 = 1 << 29;

pub(super) fn encode(snapshot: &ParticleSnapshot, rotating_plate: bool) -> ([f32; 4], u32) {
    if snapshot.kind != ParticleRenderKind::Leaf {
        return ([0.0; 4], 0);
    }
    if let Some(orientation) = snapshot.leaf_orientation {
        // Both B representations use the same held simulation pose for optics.
        let flags = LEAF_FLIGHT_BIT
            | if rotating_plate {
                LEAF_ROTATING_PLATE_BIT
            } else {
                0
            };
        return (orientation.to_array(), flags);
    }
    // Unchecked A retains its original motion-derived optical proxy, regardless
    // of the saved B geometry preference (including non-falling leaf particles).
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
    fn geometry_switch_changes_only_render_flag_throughout_live_flight() {
        let mut system = ParticleSystem::new(1);
        system.set_leaf_flight_enabled(true);
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
            let billboard = encode(snapshot, false);
            let plate = encode(snapshot, true);
            assert_eq!(billboard.0, snapshot.leaf_orientation.unwrap().to_array());
            assert_eq!(billboard.0, plate.0);
            assert_eq!(billboard.1, LEAF_FLIGHT_BIT);
            assert_eq!(billboard.1 ^ plate.1, LEAF_ROTATING_PLATE_BIT);
            if previous.is_some_and(|pose| pose != billboard.0) {
                assert!(system.last_tick_step().did_step);
                pose_changes += 1;
            }
            previous = Some(billboard.0);
        }
        assert!(
            pose_changes > 10,
            "B optical pose must keep evolving on world ticks"
        );
    }

    #[test]
    fn legacy_and_non_leaf_rendering_ignore_saved_plate_preference() {
        let mut system = ParticleSystem::new(1);
        system.spawn(ParticleSpawn::default()).unwrap();
        let mut snapshots = Vec::new();
        system.write_snapshots(&mut snapshots);
        let mut snapshot = snapshots[0];
        assert_eq!(encode(&snapshot, false), encode(&snapshot, true));
        assert_eq!(encode(&snapshot, true), ([0., 1., 0., 1.], 0));
        for kind in [
            ParticleRenderKind::Butterfly,
            ParticleRenderKind::WaterDroplet,
            ParticleRenderKind::TerrainVoxel,
        ] {
            snapshot.kind = kind;
            snapshot.leaf_orientation = Some(Quat::IDENTITY);
            assert_eq!(encode(&snapshot, false), ([0.; 4], 0));
            assert_eq!(encode(&snapshot, true), ([0.; 4], 0));
        }
    }
}
