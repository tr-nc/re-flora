//! Flower intent layered over the existing fixed-step, terrain-safe flight.
//! Timings/weights are art direction, not species-wide biological constants.
use super::{
    butterfly_flight::FlightGoal,
    flower_visits::{ButterflyBodyPose, FlowerId, FlowerTarget},
};
use glam::{Mat3, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ButterflyForagingSettings {
    pub search_radius: f32,
    pub dwell_seconds: f32,
    pub preference_strength: f32,
    pub wind_limit: f32,
}
impl Default for ButterflyForagingSettings {
    fn default() -> Self {
        Self {
            search_radius: 0.7,
            dwell_seconds: 4.,
            preference_strength: 1.,
            wind_limit: 2.,
        }
    }
}
impl ButterflyForagingSettings {
    pub fn sanitized(self) -> Self {
        let defaults = Self::default();
        let finite = |v: f32, default: f32, min: f32, max: f32| {
            if v.is_finite() {
                v.clamp(min, max)
            } else {
                default
            }
        };
        Self {
            search_radius: finite(self.search_radius, defaults.search_radius, 0.08, 1.5),
            dwell_seconds: finite(self.dwell_seconds, defaults.dwell_seconds, 0.5, 12.),
            preference_strength: finite(
                self.preference_strength,
                defaults.preference_strength,
                0.,
                2.,
            ),
            wind_limit: finite(self.wind_limit, defaults.wind_limit, 0.1, 6.),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Search,
    Travel,
    Approach,
    Feed,
    Takeoff,
    Recover,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ForagingStats {
    pub choices: u32,
    pub landings: u32,
    pub departures: u32,
    pub different_flowers: u32,
    pub lost_targets: u32,
    pub wind_interruptions: u32,
    pub stalls: u32,
}
#[derive(Clone, Copy)]
pub(super) struct Intent {
    pub goal: Option<FlightGoal>,
    pub attached: Option<(Vec3, ButterflyBodyPose)>,
}
#[derive(Debug)]
pub(super) struct ButterflyBehavior {
    pub phase: Phase,
    target: Option<FlowerId>,
    elapsed: f32,
    next_decision: f32,
    best_distance: f32,
    progress_time: f32,
    settle_time: f32,
    wind_time: f32,
    heading: Quat,
    yaw: f32,
    escape: Vec3,
    preference: u32,
    memory: VecDeque<(FlowerId, f32)>,
    failed: VecDeque<(FlowerId, f32)>,
    last_landing: Option<FlowerId>,
    pub stats: ForagingStats,
}
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}
pub(super) fn orientation(normal: Vec3, tangent: Vec3, yaw: f32) -> Quat {
    let z = (tangent * yaw.cos() + normal.cross(tangent) * yaw.sin()).normalize();
    Quat::from_mat3(&Mat3::from_cols(normal.cross(z).normalize(), normal, z))
}
impl ButterflyBehavior {
    pub fn new(seed: f32, direction: Vec3) -> Self {
        let preference = hash(seed.to_bits());
        Self {
            phase: Phase::Search,
            target: None,
            elapsed: 0.,
            next_decision: (preference % 41) as f32 / 100.,
            best_distance: f32::INFINITY,
            progress_time: 0.,
            settle_time: 0.,
            wind_time: 0.,
            heading: super::butterfly_wingbeat::flight_orientation(direction, 0.),
            yaw: 0.,
            escape: Vec3::ZERO,
            preference,
            memory: VecDeque::with_capacity(4),
            failed: VecDeque::with_capacity(4),
            last_landing: None,
            stats: Default::default(),
        }
    }
    pub fn reset(&mut self) {
        self.phase = Phase::Search;
        self.target = None;
        self.elapsed = 0.;
        self.next_decision = 0.25;
        self.wind_time = 0.;
        self.settle_time = 0.;
        self.memory.clear();
        self.failed.clear();
        self.last_landing = None;
    }
    fn transition(&mut self, phase: Phase) {
        self.phase = phase;
        self.elapsed = 0.;
        self.settle_time = 0.;
        self.best_distance = f32::INFINITY;
        self.progress_time = 0.;
    }
    fn remember(list: &mut VecDeque<(FlowerId, f32)>, id: FlowerId, time: f32) {
        list.retain(|(old, _)| *old != id);
        if list.len() == 4 {
            list.pop_front();
        }
        list.push_back((id, time));
    }
    fn choose(
        &self,
        flowers: &[FlowerTarget],
        position: Vec3,
        time: f32,
        settings: ButterflyForagingSettings,
    ) -> Option<FlowerId> {
        flowers
            .iter()
            .filter(|f| {
                f.position.is_finite()
                    && f.normal.y > 0.2
                    && f.radius > 0.
                    && time - f.observed_at <= 0.5
                    && position.distance(f.position) <= settings.search_radius
                    && !self
                        .memory
                        .iter()
                        .any(|(id, t)| *id == f.id && time - t < 12.)
                    && !self
                        .failed
                        .iter()
                        .any(|(id, t)| *id == f.id && time - t < 6.)
            })
            .min_by(|a, b| {
                let score = |f: &FlowerTarget| {
                    let preference = (hash(self.preference ^ f.species) % 1000) as f32 / 1000.;
                    position.distance(f.position) / (1. + preference * settings.preference_strength)
                };
                score(a).total_cmp(&score(b)).then(a.id.cmp(&b.id))
            })
            .map(|f| f.id)
    }
    fn recover(&mut self, position: Vec3, time: f32) {
        if let Some(id) = self.target.take() {
            Self::remember(&mut self.failed, id, time);
        }
        self.escape = position + Vec3::Y * 0.025 + self.heading * Vec3::Z * 0.025;
        self.transition(Phase::Recover);
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Fixed-step intent consumes explicit world observations"
    )]
    pub fn advance(
        &mut self,
        position: Vec3,
        velocity: Vec3,
        dt: f32,
        time: f32,
        local_wind: Vec3,
        flowers: &[FlowerTarget],
        settings: ButterflyForagingSettings,
    ) -> Intent {
        let settings = settings.sanitized();
        self.elapsed += dt;
        self.wind_time = if local_wind.length() > settings.wind_limit {
            self.wind_time + dt
        } else {
            (self.wind_time - dt * 2.).max(0.)
        };
        if self.wind_time > 0.65
            && matches!(self.phase, Phase::Travel | Phase::Approach | Phase::Feed)
        {
            self.stats.wind_interruptions += 1;
            self.recover(position, time);
        }
        if self.phase == Phase::Search && self.elapsed >= self.next_decision {
            self.next_decision = self.elapsed + 0.65 + (self.preference % 17) as f32 / 100.;
            if self.wind_time < 0.2 {
                self.target = self.choose(flowers, position, time, settings);
                if self.target.is_some() {
                    self.stats.choices += 1;
                    self.transition(Phase::Travel);
                }
            }
        }
        let mut result = Intent {
            goal: None,
            attached: None,
        };
        if matches!(self.phase, Phase::Recover | Phase::Takeoff) {
            result.goal = Some(FlightGoal {
                position: self.escape,
                velocity: Vec3::ZERO,
                precision: 1.,
            });
            if self.elapsed > 0.85 {
                self.transition(Phase::Search);
                self.target = None;
                self.next_decision = 0.25;
            }
            return result;
        }
        let Some(id) = self.target else { return result };
        let Some(flower) = flowers
            .iter()
            .find(|f| f.id == id && time - f.observed_at <= 0.5)
        else {
            self.stats.lost_targets += 1;
            self.recover(position, time);
            return result;
        };
        let (phase, clearance) = super::butterfly_wingbeat::perching_pose();
        let contact = flower.position
            + flower.velocity * (time - flower.observed_at).clamp(0., 0.05)
            + flower.normal
                * clearance
                * super::system::BUTTERFLY_GUIDED_RENDER_SIZE
                * (1.53125 / 3.4);
        let distance = position.distance(contact);
        if self.phase == Phase::Feed {
            if distance > 0.04 {
                self.stats.lost_targets += 1;
                self.recover(position, time);
                return result;
            }
            if self.elapsed >= settings.dwell_seconds * (0.8 + (self.preference % 41) as f32 / 100.)
            {
                Self::remember(&mut self.memory, id, time);
                self.stats.departures += 1;
                self.escape = contact + flower.normal * 0.035 + self.heading * Vec3::Z * 0.025;
                self.transition(Phase::Takeoff);
                result.goal = Some(FlightGoal {
                    position: self.escape,
                    velocity: flower.velocity,
                    precision: 1.,
                });
            } else {
                result.attached = Some((
                    contact,
                    ButterflyBodyPose {
                        orientation: orientation(flower.normal, flower.tangent, self.yaw),
                        phase: Some(phase),
                        host: Some(id),
                        yaw: self.yaw,
                        clearance,
                    },
                ));
            }
            return result;
        }
        if distance + 0.002 < self.best_distance {
            self.best_distance = distance;
            self.progress_time = self.elapsed;
        }
        if self.elapsed - self.progress_time > 6. || self.elapsed > 30. {
            self.stats.stalls += 1;
            self.recover(position, time);
            return result;
        }
        if self.phase == Phase::Travel && distance < 0.035 {
            self.transition(Phase::Approach);
        }
        if self.phase == Phase::Approach {
            let capture = (flower.radius * 0.12).clamp(0.001, 0.0018);
            if distance < capture && (velocity - flower.velocity).length() < 0.025 {
                self.settle_time += dt;
            } else {
                self.settle_time = 0.;
            }
            if self.settle_time > 0.12 {
                let forward = self.heading * Vec3::Z;
                self.yaw = forward
                    .dot(flower.normal.cross(flower.tangent))
                    .atan2(forward.dot(flower.tangent));
                self.stats.landings += 1;
                if self.last_landing.is_some_and(|last| last != id) {
                    self.stats.different_flowers += 1;
                }
                self.last_landing = Some(id);
                self.transition(Phase::Feed);
                result.attached = Some((
                    contact,
                    ButterflyBodyPose {
                        orientation: orientation(flower.normal, flower.tangent, self.yaw),
                        phase: Some(phase),
                        host: Some(id),
                        yaw: self.yaw,
                        clearance,
                    },
                ));
                return result;
            }
        }
        let direction = contact - position;
        if direction.length_squared() > 1e-7 {
            let desired = super::butterfly_wingbeat::flight_orientation(direction, 0.);
            self.heading = self.heading.slerp(desired, 1. - (-dt * 6.).exp());
        }
        result.goal = Some(FlightGoal {
            position: contact,
            velocity: flower.velocity,
            precision: if self.phase == Phase::Approach {
                1.
            } else {
                0.
            },
        });
        result
    }
    pub fn body_pose(&self) -> ButterflyBodyPose {
        ButterflyBodyPose {
            orientation: self.heading,
            phase: None,
            host: None,
            yaw: 0.,
            clearance: 0.,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn flower(plant: u64, p: Vec3) -> FlowerTarget {
        FlowerTarget {
            id: FlowerId {
                epoch: 1,
                plant,
                head: 0,
            },
            species: plant as u32,
            position: p,
            normal: Vec3::Y,
            tangent: Vec3::Z,
            radius: 0.01,
            velocity: Vec3::ZERO,
            observed_at: 0.,
        }
    }
    fn settings() -> ButterflyForagingSettings {
        ButterflyForagingSettings::default()
    }
    #[test]
    fn selection_excludes_recent_flowers_and_failure_memory_is_bounded() {
        let mut brain = ButterflyBehavior::new(12., Vec3::X);
        let flowers = [flower(1, Vec3::ZERO), flower(2, Vec3::X * 0.1)];
        ButterflyBehavior::remember(&mut brain.memory, flowers[0].id, 0.);
        assert_eq!(
            brain.choose(&flowers, Vec3::ZERO, 0., settings()),
            Some(flowers[1].id)
        );
        for i in 0..20 {
            ButterflyBehavior::remember(&mut brain.failed, flower(i, Vec3::ZERO).id, 0.);
        }
        assert_eq!(brain.failed.len(), 4);
    }
    #[test]
    fn missing_replaced_or_stale_host_cannot_keep_attachment() {
        for mode in 0..3 {
            let mut brain = ButterflyBehavior::new(3., Vec3::X);
            let mut target = flower(1, Vec3::ZERO);
            brain.target = Some(target.id);
            brain.phase = Phase::Feed;
            let mut flowers = vec![target];
            match mode {
                0 => flowers.clear(),
                1 => flowers[0].id.epoch += 1,
                _ => target.observed_at = -1.,
            }
            if mode == 2 {
                flowers[0] = target;
            }
            let intent = brain.advance(
                Vec3::ZERO,
                Vec3::ZERO,
                1. / 120.,
                0.,
                Vec3::ZERO,
                &flowers,
                settings(),
            );
            assert!(intent.attached.is_none());
            assert_eq!(brain.phase, Phase::Recover);
        }
    }
    #[test]
    fn sustained_wind_and_no_progress_are_causal_interruptions() {
        let target = flower(1, Vec3::X * 0.3);
        for wind in [Vec3::X * 3., Vec3::ZERO] {
            let mut brain = ButterflyBehavior::new(3., Vec3::X);
            brain.phase = Phase::Travel;
            brain.target = Some(target.id);
            for step in 0..900 {
                let t = step as f32 / 120.;
                let mut f = target;
                f.observed_at = t;
                brain.advance(Vec3::ZERO, Vec3::ZERO, 1. / 120., t, wind, &[f], settings());
                if brain.phase == Phase::Recover {
                    break;
                }
            }
            assert_eq!(brain.phase, Phase::Recover);
            assert_eq!(
                brain.stats.wind_interruptions,
                u32::from(wind != Vec3::ZERO)
            );
            assert_eq!(brain.stats.stalls, u32::from(wind == Vec3::ZERO));
        }
    }
    #[test]
    fn landing_dwell_departure_and_next_flower_complete() {
        let mut brain = ButterflyBehavior::new(10., Vec3::X);
        let mut flowers = [flower(1, Vec3::ZERO), flower(2, Vec3::X * 0.1)];
        let (_, clearance) = super::super::butterfly_wingbeat::perching_pose();
        let offset = Vec3::Y
            * clearance
            * super::super::system::BUTTERFLY_GUIDED_RENDER_SIZE
            * (1.53125 / 3.4);
        brain.phase = Phase::Approach;
        brain.target = Some(flowers[0].id);
        let mut position = flowers[0].position + offset;
        for step in 0..1000 {
            let time = step as f32 / 120.;
            for f in &mut flowers {
                f.observed_at = time;
            }
            brain.advance(
                position,
                Vec3::ZERO,
                1. / 120.,
                time,
                Vec3::ZERO,
                &flowers,
                ButterflyForagingSettings {
                    dwell_seconds: 0.5,
                    ..settings()
                },
            );
            if brain.phase == Phase::Approach && brain.target == Some(flowers[1].id) {
                position = flowers[1].position + offset;
            }
            if brain.phase == Phase::Travel && brain.target == Some(flowers[1].id) {
                position = flowers[1].position + offset;
            }
            if brain.stats.different_flowers > 0 {
                break;
            }
        }
        assert_eq!(brain.stats.landings, 2);
        assert_eq!(brain.stats.departures, 1);
        assert_eq!(brain.stats.different_flowers, 1);
    }
    #[test]
    fn settings_reject_non_finite_and_unsafe_values() {
        let s = ButterflyForagingSettings {
            search_radius: f32::NAN,
            dwell_seconds: -3.,
            preference_strength: f32::INFINITY,
            wind_limit: 99.,
            ..settings()
        }
        .sanitized();
        assert_eq!(s.search_radius, 0.7);
        assert_eq!(s.dwell_seconds, 0.5);
        assert_eq!(s.preference_strength, 1.);
        assert_eq!(s.wind_limit, 6.);
    }
}
