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
    /// Validate and allocate all states before replacing the live set (snapshot publication).
    pub fn replace_ropes(&mut self, descs: Vec<RopeDesc>) -> Result<Vec<RopeId>, RopeError> {
        if descs.len() > 64 {
            return Err(RopeError("strand capacity reached"));
        }
        let next = self
            .next_rope_id
            .checked_add(descs.len() as u64)
            .ok_or(RopeError("identity exhausted"))?;
        let mut replacement = std::collections::BTreeMap::new();
        for (i, desc) in descs.into_iter().enumerate() {
            replacement.insert(RopeId(self.next_rope_id + i as u64), Rope::new(desc)?);
        }
        let ids = replacement.keys().copied().collect();
        self.ropes = replacement;
        self.next_rope_id = next;
        self.rope_contact_failures = 0;
        Ok(ids)
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
    /// Placement preflight against the same published geometry and capsule contacts as stepping.
    /// Existing snapshots may start in contact, but new paths must not start through a solid.
    pub fn rope_path_is_clear(&mut self, desc: &RopeDesc) -> Result<bool, RopeError> {
        desc.validate()?;
        self.sync_capsule_character_broad_phase();
        let queries=self.capsule_character_broad_phase.as_query_pipeline(self.physics.narrow_phase.query_dispatcher(),&self.physics.bodies,&self.physics.colliders,QueryFilter::default().exclude_sensors());
        for (i,points) in desc.points.windows(2).enumerate() {
            for contact in segment_contacts(&queries,points[0],points[1],desc.material.radius,i)? {
                let depth=(contact.point-points[0].lerp(points[1],contact.t)).dot(contact.normal);
                if depth>desc.material.radius*0.07 { return Ok(false); }
            }
        }
        Ok(true)
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
                    match segment_contacts(&queries,a,b,radius,i) {
                        Ok(contacts)=>rope.contacts.extend(contacts),
                        Err(_)=>self.rope_contact_failures+=1,
                    }
                }
                rope.solve(h);
            }
        }
    }
}

fn segment_contacts(queries:&QueryPipeline<'_>,a:Vec3,b:Vec3,radius:f32,segment:usize)->Result<Vec<Contact>,RopeError> {
    let center=(a+b)*0.5;
    let shape=Capsule::new(to_rapier_vec(a-center),to_rapier_vec(b-center),radius);
    let pose=Pose::from_translation(to_rapier_vec(center));
    let aabb=shape.compute_aabb(&pose).loosened(radius*0.1);
    let mut contacts=Vec::new();
    for (_,collider) in queries.intersect_aabb_conservative(aabb) {
        let mut manifolds:Vec<ContactManifold<(),()>>=Vec::new();
        let relative=pose.inverse()*collider.position();
        contact_prediction::ContactPrediction.contact_manifolds(&relative,&shape,collider.shape(),radius*0.1,&mut manifolds,&mut None).map_err(|_|RopeError("unsupported static contact pair"))?;
        for manifold in manifolds {
            let normal=-from_rapier_vec(manifold.local_n1);
            for p in manifold.points {
                let world=center+from_rapier_vec(p.local_p1);
                let t=((world-a).dot(b-a)/(b-a).length_squared().max(1e-12)).clamp(0.,1.);
                contacts.push(Contact{segment,t,normal,point:a.lerp(b,t)+normal*(radius*0.02-p.dist)});
            }
        }
    }
    Ok(contacts)
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
            rest_lengths: None,
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
    fn endpoint_tension_converges_across_node_counts_and_mass_ratios() {
        for segments in [16,64,128] {for mass in [0.015,0.15] {
            let mut world=CollisionWorld::new();world.set_gravity(Vec3::NEG_Y*9.8).unwrap();
            let mut d=desc(false);d.points=(0..=segments).map(|i|Vec3::new(0.,2.-i as f32/segments as f32*2.,0.)).collect();d.material.radius=0.0015;d.material.linear_density=0.004;d.material.stretch_rigidity=800.;d.material.bend_rigidity=0.;d.end_mass=mass;
            let id=world.spawn_rope(d).unwrap();run(&mut world,120,1./60.);
            let r=world.rope(id).unwrap();assert!(r.max_strain()<0.01,"segments={segments} mass={mass} strain={}",r.max_strain());
            assert_eq!(r.positions()[0],Vec3::Y*2.);
        }}
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
    fn replacement_is_atomic_and_placement_rejects_initial_solid_intersections() {
        let mut world=CollisionWorld::new();let id=world.spawn_rope(desc(true)).unwrap();
        let mut bad=desc(true);bad.rest_lengths=Some(vec![1.]);
        assert!(world.replace_ropes(vec![desc(true),bad]).is_err());assert!(world.rope(id).is_some());assert_eq!(world.rope_count(),1);
        let brick=StaticVoxelBrickId(IVec3::ZERO);
        world.upsert_static_voxel_brick(brick,1,BrickOccupancy::from_filled_voxels((0..16).flat_map(|x|(0..16).flat_map(move |y|(0..16).map(move |z|UVec3::new(x,y,z))))));
        let mut d=desc(true);for p in &mut d.points {p.z=8.;}
        assert!(!world.rope_path_is_clear(&d).unwrap());
        world.remove_static_voxel_brick(brick,2);assert!(world.rope_path_is_clear(&d).unwrap());
        let ids=world.replace_ropes(vec![d]).unwrap();assert_ne!(ids[0],id);assert!(world.rope(id).is_none());
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
