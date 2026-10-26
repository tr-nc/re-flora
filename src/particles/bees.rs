//! Bounded residents of durable solitary-bee houses. Only houses persist; trips
//! resume from the nest after loading. Flower contacts are the shared GPU poses.
use super::{
    flower_visits::{ButterflyBodyPose, FlowerId, FlowerTarget},
    ButterflyFrame, ParticleRenderKind, ParticleSnapshot,
};
use glam::{Vec3, Vec4};
pub(crate) const MAX_HIVES: usize = 8;
pub(crate) const RESIDENTS: usize = 8;
pub(crate) const CAPACITY: usize = MAX_HIVES * RESIDENTS;
const CLEARANCE: f32 = 0.002;
#[derive(Clone, Copy, Debug)]
pub(crate) struct HiveSite {
    pub id: u32,
    pub position: Vec3,
}
impl HiveSite {
    pub fn entrance(self) -> Vec3 {
        self.position
    }
}
#[derive(Clone, Copy, Debug)]
enum Trip {
    Home(f32),
    Out {
        host: FlowerId,
        from: Vec3,
        time: f32,
        duration: f32,
    },
    Visit {
        host: FlowerId,
        time: f32,
    },
    Back {
        from: Vec3,
        time: f32,
        duration: f32,
    },
}
struct Bee {
    hive: u32,
    slot: usize,
    round: u32,
    position: Vec3,
    velocity: Vec3,
    trip: Trip,
    body: Option<ButterflyBodyPose>,
    presentation: super::butterfly_presentation::ButterflyPresentation,
}
#[derive(Default)]
pub(crate) struct Bees {
    residents: Vec<Bee>,
    pub visits: u64,
    pub returns: u64,
    pub lost_hosts: u64,
}
fn duration(from: Vec3, to: Vec3) -> f32 {
    (from.distance(to) / 0.10 + 0.4).clamp(0.8, 10.)
}
fn flight(from: Vec3, to: Vec3, t: f32) -> Vec3 {
    from.lerp(to, t) + Vec3::Y * (4. * t * (1. - t) * 0.06)
}
fn homeward(bee: &mut Bee, site: HiveSite) {
    bee.body = None;
    bee.trip = Trip::Back {
        from: bee.position,
        time: 0.,
        duration: duration(bee.position, site.entrance()),
    };
}
impl Bees {
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.residents.len()
    }
    #[cfg(test)]
    pub fn outside(&self) -> usize {
        self.residents
            .iter()
            .filter(|b| !matches!(b.trip, Trip::Home(_)))
            .count()
    }
    pub fn clear(&mut self) {
        self.residents.clear();
    }
    pub fn update(&mut self, dt: f32, sites: &[HiveSite], flowers: &[FlowerTarget], day: bool) {
        assert!(sites.len() <= MAX_HIVES);
        self.residents
            .retain(|b| sites.iter().any(|s| s.id == b.hive));
        for &site in sites {
            if !self.residents.iter().any(|b| b.hive == site.id) {
                for slot in 0..RESIDENTS {
                    self.residents.push(Bee {
                        hive: site.id,
                        slot,
                        round: 0,
                        position: site.entrance(),
                        velocity: Vec3::ZERO,
                        trip: Trip::Home(0.5 + slot as f32 * 0.37),
                        body: None,
                        presentation: Default::default(),
                    });
                }
            }
        }
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        // App ticks are normally <=50ms; a suspended window does not teleport an
        // insect through multiple lifecycle transitions in one presentation.
        let dt = dt.min(0.1);
        let mut claimed: std::collections::BTreeSet<_> = self
            .residents
            .iter()
            .filter_map(|b| match b.trip {
                Trip::Out { host, .. } | Trip::Visit { host, .. } => Some(host),
                _ => None,
            })
            .collect();
        for bee in &mut self.residents {
            let site = *sites.iter().find(|s| s.id == bee.hive).unwrap();
            let previous = bee.position;
            if !day && matches!(bee.trip, Trip::Out { .. } | Trip::Visit { .. }) {
                homeward(bee, site);
            }
            match bee.trip {
                Trip::Home(wait) => {
                    bee.position = site.entrance();
                    bee.body = None;
                    bee.trip = Trip::Home(if day {
                        (wait - dt).max(0.)
                    } else {
                        wait.max(0.5 + bee.slot as f32 * 0.37)
                    });
                    if day && wait <= 0. {
                        let candidates: Vec<_> = flowers
                            .iter()
                            .filter(|f| {
                                !claimed.contains(&f.id)
                                    && f.position.is_finite()
                                    && f.position.distance_squared(site.position) <= 0.8f32.powi(2)
                            })
                            .collect();
                        if !candidates.is_empty() {
                            let target =
                                candidates[(bee.slot + bee.round as usize * 3 + bee.hive as usize)
                                    % candidates.len()];
                            bee.trip = Trip::Out {
                                host: target.id,
                                from: bee.position,
                                time: 0.,
                                duration: duration(bee.position, target.position),
                            };
                            claimed.insert(target.id);
                        }
                    }
                }
                Trip::Out {
                    host,
                    from,
                    time,
                    duration,
                } => {
                    if let Some(target) = flowers.iter().find(|f| f.id == host) {
                        let t = ((time + dt) / duration).min(1.);
                        bee.position = flight(from, target.position + target.normal * CLEARANCE, t);
                        if t >= 1. {
                            bee.trip = Trip::Visit {
                                host,
                                time: 2. + bee.slot as f32 * 0.13,
                            };
                            self.visits += 1;
                        } else {
                            bee.trip = Trip::Out {
                                host,
                                from,
                                time: time + dt,
                                duration,
                            };
                        }
                    } else {
                        self.lost_hosts += 1;
                        homeward(bee, site);
                    }
                }
                Trip::Visit { host, time } => {
                    if let Some(target) = flowers.iter().find(|f| f.id == host) {
                        let yaw = bee.slot as f32 * 0.8;
                        bee.position = target.position + target.normal * CLEARANCE;
                        bee.body = Some(ButterflyBodyPose {
                            orientation: super::butterfly_behavior::orientation(
                                target.normal,
                                target.tangent,
                                yaw,
                            ),
                            phase: Some(0.),
                            host: Some(host),
                            yaw,
                            clearance: CLEARANCE,
                        });
                        if time <= dt {
                            homeward(bee, site);
                        } else {
                            bee.trip = Trip::Visit {
                                host,
                                time: time - dt,
                            };
                        }
                    } else {
                        self.lost_hosts += 1;
                        homeward(bee, site);
                    }
                }
                Trip::Back {
                    from,
                    time,
                    duration,
                } => {
                    let t = ((time + dt) / duration).min(1.);
                    bee.position = flight(from, site.entrance(), t);
                    if t >= 1. {
                        self.returns += 1;
                        bee.round = bee.round.wrapping_add(1);
                        bee.trip = Trip::Home(2. + bee.slot as f32 * 0.2);
                    } else {
                        bee.trip = Trip::Back {
                            from,
                            time: time + dt,
                            duration,
                        };
                    }
                }
            }
            bee.velocity = (bee.position - previous) / dt;
        }
    }
    pub fn append(
        &mut self,
        frame: ButterflyFrame,
        size_scale: f32,
        out: &mut Vec<ParticleSnapshot>,
    ) {
        let size_scale = if size_scale.is_finite() {
            size_scale.clamp(0.1, 2.)
        } else {
            0.5
        };
        for b in &mut self.residents {
            if matches!(b.trip, Trip::Home(_)) {
                continue;
            }
            let pose = b
                .presentation
                .sample_with_body(frame, b.position, b.velocity, None, b.body);
            let mut snap = model_snapshot(
                ParticleRenderKind::Bee,
                pose.position,
                0.026 * size_scale,
                frame,
            );
            snap.velocity = pose.velocity;
            snap.animation_phase_offset = b.slot as f32 / RESIDENTS as f32;
            snap.butterfly_body_pose = pose.body;
            out.push(snap);
        }
    }
}
fn model_snapshot(
    kind: ParticleRenderKind,
    position: Vec3,
    size: f32,
    frame: ButterflyFrame,
) -> ParticleSnapshot {
    ParticleSnapshot {
        position_ws: position,
        velocity: Vec3::ZERO,
        color: Vec4::ONE,
        size,
        kind,
        palette_index: 0,
        animation_phase_offset: 0.,
        animation_sample_time: Some(frame.time_seconds()),
        butterfly_wingbeat: None,
        butterfly_body_pose: None,
        leaf_orientation: None,
        leaf_shape_seed: None,
        leaf_geometry: None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn site(id: u32) -> HiveSite {
        HiveSite {
            id,
            position: Vec3::ONE,
        }
    }
    fn flower() -> FlowerTarget {
        FlowerTarget {
            id: FlowerId {
                epoch: 1,
                plant: 9,
                head: 0,
            },
            species: 2,
            position: Vec3::ONE + Vec3::new(0.12, -0.02, 0.12),
            normal: Vec3::Y,
            tangent: Vec3::Z,
            radius: 0.02,
            velocity: Vec3::ZERO,
            observed_at: 0.,
        }
    }
    #[test]
    fn residents_visit_real_hosts_return_home_and_rest_at_night() {
        let mut bees = Bees::default();
        for _ in 0..600 {
            bees.update(0.05, &[site(1)], &[flower()], true);
        }
        assert_eq!(bees.len(), RESIDENTS);
        assert!(bees.visits >= 3 && bees.returns >= 2);
        for _ in 0..240 {
            bees.update(0.05, &[site(1)], &[flower()], false);
        }
        assert_eq!(bees.outside(), 0);
        let visits = bees.visits;
        for _ in 0..100 {
            bees.update(0.05, &[site(1)], &[flower()], false);
        }
        assert_eq!(bees.visits, visits);
    }
    #[test]
    fn missing_flower_and_removed_house_cannot_leave_orphan_residents() {
        let mut bees = Bees::default();
        for _ in 0..75 {
            bees.update(0.05, &[site(1)], &[flower()], true);
        }
        for _ in 0..240 {
            bees.update(0.05, &[site(1)], &[], true);
        }
        assert!(bees.lost_hosts > 0);
        assert_eq!(bees.outside(), 0);
        bees.update(0.05, &[], &[], true);
        assert_eq!(bees.len(), 0);
        let sites: Vec<_> = (0..MAX_HIVES as u32).map(site).collect();
        bees.update(0.05, &sites, &[], true);
        assert_eq!(bees.len(), CAPACITY);
        assert_eq!(bees.outside(), 0);
    }
    #[test]
    fn live_size_and_update_rate_control_bee_snapshots_independently() {
        let mut bees = Bees::default();
        for _ in 0..70 {
            bees.update(0.05, &[site(1)], &[flower()], true);
        }
        let mut original = Vec::new();
        bees.append(ButterflyFrame::at(5., 8), 1., &mut original);
        assert!(!original.is_empty());
        for bee in &mut bees.residents {
            bee.position += Vec3::X;
            bee.velocity = Vec3::Z;
        }
        let mut held = Vec::new();
        bees.append(ButterflyFrame::at(5.05, 8), 0.5, &mut held);
        for (before, after) in original.iter().zip(&held) {
            assert_eq!(after.size, before.size * 0.5);
            assert_eq!(after.position_ws, before.position_ws);
            assert_eq!(after.velocity, before.velocity);
            assert_eq!(after.animation_sample_time, before.animation_sample_time);
        }
        let mut faster = Vec::new();
        bees.append(ButterflyFrame::at(5.05, 24), 0.5, &mut faster);
        for (before, after) in original.iter().zip(&faster) {
            assert_eq!(after.position_ws, before.position_ws + Vec3::X);
            assert_eq!(after.velocity, Vec3::Z);
            assert_ne!(after.animation_sample_time, before.animation_sample_time);
        }
    }

    #[test]
    fn visits_retain_stable_host_identity() {
        let mut out = Vec::new();
        let mut bees = Bees::default();
        for _ in 0..70 {
            bees.update(0.05, &[site(1)], &[flower()], true);
        }
        bees.append(ButterflyFrame::at(5., 24), 0.5, &mut out);
        assert!(out.iter().any(|p| p
            .butterfly_body_pose
            .is_some_and(|p| p.host == Some(flower().id))));
    }
}
