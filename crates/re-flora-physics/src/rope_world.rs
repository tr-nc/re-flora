//! Strands share the rigid-body clock and the published static collision geometry.
use crate::{
    rope::{Contact, Rope, RopeDesc, RopeError, RopeId},
    *,
};
use rapier3d::parry::{
    bounding_volume::BoundingVolume,
    query::{ContactManifold, PersistentQueryDispatcher},
    shape::{Ball, Capsule},
};

impl CollisionWorld {
    pub fn spawn_rope(&mut self, desc: RopeDesc) -> Result<RopeId, RopeError> {
        if self.ropes.len() >= 64 {
            return Err(RopeError("strand capacity reached"));
        }
        let rope = Rope::new(desc)?;
        let id = RopeId(self.next_rope_id);
        self.next_rope_id = self
            .next_rope_id
            .checked_add(1)
            .ok_or(RopeError("identity exhausted"))?;
        self.ropes.insert(id, rope);
        Ok(id)
    }
    pub fn rope(&self, id: RopeId) -> Option<&Rope> {
        self.ropes.get(&id)
    }
    pub fn rope_mut(&mut self, id: RopeId) -> Option<&mut Rope> {
        self.ropes.get_mut(&id)
    }
    pub fn remove_rope(&mut self, id: RopeId) -> bool {
        self.ropes.remove(&id).is_some()
    }
    pub fn rope_count(&self) -> usize {
        self.ropes.len()
    }
    pub fn rope_contact_failures(&self) -> usize {
        self.rope_contact_failures
    }

    pub(crate) fn step_ropes(&mut self) {
        if self.ropes.is_empty() {
            return;
        }
        let queries = self.capsule_character_broad_phase.as_query_pipeline(
            self.physics.narrow_phase.query_dispatcher(),
            &self.physics.bodies,
            &self.physics.colliders,
            QueryFilter::default().exclude_sensors(),
        );
        let gravity = from_rapier_vec(self.physics.gravity);
        // Internal small steps are scheduled by the one authoritative world clock.
        let h = self.fixed_step_seconds / 2.;
        for _ in 0..2 {
            for rope in self.ropes.values_mut() {
                rope.predict(h, gravity);
                let radius = rope.radius();
                for i in 0..rope.positions().len() {
                    if rope.is_pinned(i) {
                        continue;
                    }
                    let old = rope.old_positions()[i];
                    let delta = rope.positions()[i] - old;
                    if delta.length_squared() < 1e-12 {
                        continue;
                    }
                    if let Some((_, hit)) = queries.cast_shape(
                        &Pose::from_translation(to_rapier_vec(old)),
                        to_rapier_vec(delta),
                        &Ball::new(radius),
                        ShapeCastOptions {
                            max_time_of_impact: 1.,
                            target_distance: radius * 0.02,
                            stop_at_penetration: false,
                            compute_impact_geometry_on_penetration: true,
                        },
                    ) {
                        let normal = from_rapier_vec(hit.normal1);
                        let travel = delta * hit.time_of_impact;
                        let remaining = delta - travel;
                        rope.sweep_position(
                            i,
                            old + travel + remaining - normal * remaining.dot(normal).min(0.),
                        );
                    }
                }
                for i in 0..rope.positions().len() - 1 {
                    let a = rope.positions()[i];
                    let b = rope.positions()[i + 1];
                    let center = (a + b) * 0.5;
                    let shape =
                        Capsule::new(to_rapier_vec(a - center), to_rapier_vec(b - center), radius);
                    let pose = Pose::from_translation(to_rapier_vec(center));
                    let aabb = shape.compute_aabb(&pose).loosened(radius * 0.1);
                    for (_, collider) in queries.intersect_aabb_conservative(aabb) {
                        let mut manifolds: Vec<ContactManifold<(), ()>> = Vec::new();
                        let relative = pose.inverse() * collider.position();
                        if contact_prediction::ContactPrediction
                            .contact_manifolds(
                                &relative,
                                &shape,
                                collider.shape(),
                                radius * 0.1,
                                &mut manifolds,
                                &mut None,
                            )
                            .is_err()
                        {
                            self.rope_contact_failures += 1;
                            continue;
                        }
                        for manifold in manifolds {
                            let normal = -from_rapier_vec(manifold.local_n1);
                            for p in manifold.points {
                                let world = center + from_rapier_vec(p.local_p1);
                                let t = ((world - a).dot(b - a)
                                    / (b - a).length_squared().max(1e-12))
                                .clamp(0., 1.);
                                rope.contacts.push(Contact {
                                    segment: i,
                                    t,
                                    normal,
                                    point: a.lerp(b, t) + normal * (radius * 0.02 - p.dist),
                                });
                            }
                        }
                    }
                }
                rope.solve(h);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn desc(pin_end: bool) -> RopeDesc {
        RopeDesc {
            points: (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.;
                    Vec3::new(t * 8., 12. - (t * std::f32::consts::PI).sin() * 2., 0.)
                })
                .collect(),
            material: crate::rope::RopeMaterial {
                radius: 0.08,
                linear_density: 0.1,
                stretch_rigidity: 2000.,
                bend_rigidity: 0.0001,
                damping: 1.,
                friction: 0.6,
            },
            pin_start: true,
            pin_end,
            end_mass: 0.,
        }
    }
    fn run(world: &mut CollisionWorld, frames: usize, dt: f32) {
        for _ in 0..frames {
            world.advance(dt);
        }
    }
    #[test]
    fn rope_shares_world_clock_without_rigid_bodies_and_preserves_pins() {
        let mut world = CollisionWorld::new();
        world.set_gravity(Vec3::NEG_Y * 9.8).unwrap();
        let recipe = desc(true);
        let ends = [recipe.points[0], *recipe.points.last().unwrap()];
        let id = world.spawn_rope(recipe).unwrap();
        run(&mut world, 300, 1. / 60.);
        let rope = world.rope(id).unwrap();
        assert_eq!(rope.positions()[0], ends[0]);
        assert_eq!(*rope.positions().last().unwrap(), ends[1]);
        assert!(rope.positions().iter().all(|p| p.is_finite()));
        assert!(rope.max_strain() < 0.03, "{}", rope.max_strain());
        assert!(rope.positions()[12].y < ends[0].y - 1.);
    }
    #[test]
    fn display_frame_grouping_is_not_a_second_rope_clock() {
        let mut a = CollisionWorld::new();
        let mut b = CollisionWorld::new();
        let x = a.spawn_rope(desc(true)).unwrap();
        let y = b.spawn_rope(desc(true)).unwrap();
        run(&mut a, 60, 1. / 30.);
        run(&mut b, 120, 1. / 60.);
        for (p, q) in a
            .rope(x)
            .unwrap()
            .positions()
            .iter()
            .zip(b.rope(y).unwrap().positions())
        {
            assert!(p.distance(*q) < 0.001);
        }
    }
    #[test]
    fn grabbing_releasing_and_removing_use_stable_identity() {
        let mut world = CollisionWorld::new();
        let id = world.spawn_rope(desc(true)).unwrap();
        world
            .rope_mut(id)
            .unwrap()
            .set_grab(Some((12, Vec3::new(4., 14., 2.))))
            .unwrap();
        run(&mut world, 120, 1. / 60.);
        assert!(world.rope(id).unwrap().positions()[12].z > 0.2);
        world.rope_mut(id).unwrap().set_grab(None).unwrap();
        world.rope_mut(id).unwrap().set_end_pinned(false);
        run(&mut world, 120, 1. / 60.);
        assert!(world.remove_rope(id));
        assert!(world.rope(id).is_none());
        let next = world.spawn_rope(desc(true)).unwrap();
        assert_ne!(next, id);
    }
    #[test]
    fn invalid_inputs_do_not_publish_partial_ropes() {
        let mut world = CollisionWorld::new();
        let mut invalid = desc(false);
        invalid.points[1] = invalid.points[0];
        assert!(world.spawn_rope(invalid).is_err());
        assert_eq!(world.rope_count(), 0);
        let id = world.spawn_rope(desc(true)).unwrap();
        assert!(world
            .rope_mut(id)
            .unwrap()
            .set_grab(Some((usize::MAX, Vec3::ZERO)))
            .is_err());
        assert!(world
            .rope_mut(id)
            .unwrap()
            .set_acceleration(Vec3::NAN, Vec3::ZERO)
            .is_err());
    }
    #[test]
    fn voxel_contacts_follow_removal_revisions() {
        let mut world = CollisionWorld::new();
        let brick = StaticVoxelBrickId(IVec3::ZERO);
        world.upsert_static_voxel_brick(
            brick,
            1,
            BrickOccupancy::from_filled_voxels(
                (0..32).flat_map(|x| (0..32).map(move |z| UVec3::new(x, 0, z))),
            ),
        );
        let mut d = desc(false);
        d.pin_start = false;
        for p in &mut d.points {
            *p += Vec3::new(8., 0., 12.);
        }
        let id = world.spawn_rope(d).unwrap();
        run(&mut world, 360, 1. / 60.);
        assert_eq!(world.rope_contact_failures(), 0);
        assert!(world.rope(id).unwrap().positions().iter().all(|p| p.y > 1.));
        world.remove_static_voxel_brick(brick, 2);
        run(&mut world, 60, 1. / 60.);
        assert!(world.rope(id).unwrap().positions()[12].y < 0.);
    }
    #[test]
    fn whole_segments_rest_on_fixed_geometry() {
        let mut world = CollisionWorld::new();
        // Use the public fixed mesh publication, as the application does.
        world
            .set_fixed_scene_surface(
                &[
                    Vec3::new(-20., 0., -20.),
                    Vec3::new(-20., 0., 20.),
                    Vec3::new(20., 0., 20.),
                    Vec3::new(20., 0., -20.),
                ],
                &[[0, 1, 2], [0, 2, 3]],
            )
            .unwrap();
        let mut d = desc(false);
        d.pin_start = false;
        let id = world.spawn_rope(d).unwrap();
        run(&mut world, 360, 1. / 60.);
        assert_eq!(world.rope_contact_failures(), 0);
        let r = world.rope(id).unwrap();
        assert!(
            r.positions().iter().all(|p| p.y >= 0.06),
            "{:?}",
            r.positions()
        );
        assert!(r.max_strain() < 0.05);
    }
}
