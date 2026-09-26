//! A canopy owns reusable sockets; a detached leaf owns one socket generation.
//! Wind is an instantaneous load, never an emission-rate accumulator. Planning
//! is read-only: callers publish the matching render/flight handoff before commit.
use crate::wind_field::WindFieldFrame;
use glam::UVec3;

#[derive(Clone, Copy, Debug)]
pub struct LeafLifecycleSettings {
    /// Multiplies connection strength (not wind speed).
    pub strength: f32,
    pub half_life_seconds: f32,
    pub recovery_seconds: f32,
    pub growth_seconds: f32,
}

impl Default for LeafLifecycleSettings {
    fn default() -> Self {
        Self {
            strength: 1.0,
            half_life_seconds: 120.0,
            recovery_seconds: 8.0,
            growth_seconds: 20.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LeafId {
    pub tree: u32,
    /// A committed tree replacement starts a new socket topology.
    pub topology: u64,
    pub socket: u32,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct LeafDetachment {
    pub id: LeafId,
    pub world_voxel: UVec3,
    pub growth: f32,
    pub seed: u32,
}

#[derive(Clone, Debug)]
struct Socket {
    position: UVec3,
    seed: u32,
    generation: u32,
    /// Initial leaves are mature. Replacement leaves grow after this time.
    growing_since: Option<f64>,
    mature_since: f64,
}

#[derive(Clone, Debug)]
pub struct LeafCanopy {
    tree: u32,
    topology: u64,
    sockets: Vec<Socket>,
}

impl LeafCanopy {
    pub fn new(tree: u32, topology: u64, positions: &[UVec3], now: f64) -> Self {
        Self {
            tree,
            topology,
            sockets: positions
                .iter()
                .map(|&position| Socket {
                    position,
                    // Geometry identity, not dense ordering, determines initial variation.
                    seed: mix(tree
                        ^ mix(position.x)
                        ^ mix(position.y.wrapping_add(13))
                        ^ mix(position.z.wrapping_add(97))),
                    generation: 0,
                    growing_since: None,
                    mature_since: now,
                })
                .collect(),
        }
    }

    pub fn reset(&mut self, now: f64) {
        for socket in &mut self.sockets {
            socket.generation = 0;
            socket.growing_since = None;
            socket.mature_since = now;
        }
    }

    pub fn generation(&self, slot: usize) -> u32 {
        self.sockets[slot].generation
    }

    pub fn len(&self) -> usize {
        self.sockets.len()
    }

    pub fn growth(&self, slot: usize, now: f64, settings: LeafLifecycleSettings) -> f32 {
        self.sockets
            .get(slot)
            .map_or(0.0, |socket| socket.growth(now, settings))
    }

    /// Samples the same per-visible-voxel rest coordinates as the attached wind
    /// solver, rather than the old emission cluster/branch-tip representative.
    /// Physical handoff uses the last published posed leaf, not these rest roots.
    pub fn plan(
        &self,
        now: f64,
        settings: LeafLifecycleSettings,
        wind: &WindFieldFrame,
    ) -> Vec<LeafDetachment> {
        self.sockets
            .iter()
            .enumerate()
            .filter_map(|(index, socket)| {
                let growth = socket.growth(now, settings);
                if growth <= 0.0 {
                    return None;
                }
                let load = wind
                    .sample_world(socket.position.as_vec3() / 256.0)
                    .length_squared()
                    * growth
                    * growth;
                (load >= socket.strength(now, settings)).then_some(LeafDetachment {
                    id: LeafId {
                        tree: self.tree,
                        topology: self.topology,
                        socket: index as u32,
                        generation: socket.generation,
                    },
                    world_voxel: socket.position,
                    growth,
                    seed: socket.life_seed(),
                })
            })
            .collect()
    }

    /// Commits a successfully published transfer. Stale, duplicate and removed
    /// topology events cannot detach a replacement generation.
    pub fn commit(
        &mut self,
        event: LeafDetachment,
        now: f64,
        settings: LeafLifecycleSettings,
    ) -> bool {
        if event.id.tree != self.tree || event.id.topology != self.topology {
            return false;
        }
        let Some(socket) = self.sockets.get_mut(event.id.socket as usize) else {
            return false;
        };
        if socket.generation != event.id.generation || socket.position != event.world_voxel {
            return false;
        }
        socket.generation = socket.generation.wrapping_add(1);
        let delay = settings.recovery_seconds.max(0.0) * socket.recovery_variation();
        let start = now + f64::from(delay);
        socket.growing_since = Some(start);
        socket.mature_since = start + f64::from(socket.growth_duration(settings));
        true
    }

    /// Grow-only parameters may be edited live without a per-frame strength
    /// update. The publication is derived from one time, not a second timer.
    pub fn write_growth(&self, now: f64, settings: LeafLifecycleSettings, output: &mut Vec<f32>) {
        output.clear();
        output.extend(
            self.sockets
                .iter()
                .map(|socket| socket.growth(now, settings)),
        );
    }
}

impl Socket {
    fn life_seed(&self) -> u32 {
        mix(self.seed ^ self.generation.wrapping_mul(0x9e3779b9))
    }

    fn recovery_variation(&self) -> f32 {
        0.8 + 0.4 * unit(self.life_seed() ^ 0x91e10da5)
    }

    fn growth_duration(&self, settings: LeafLifecycleSettings) -> f32 {
        settings.growth_seconds.max(0.01) * (0.8 + 0.4 * unit(self.life_seed() ^ 0x7f4a7c15))
    }

    fn growth(&self, now: f64, settings: LeafLifecycleSettings) -> f32 {
        self.growing_since.map_or(1.0, |start| {
            let t =
                ((now - start) / f64::from(self.growth_duration(settings))).clamp(0.0, 1.0) as f32;
            t * t * (3.0 - 2.0 * t)
        })
    }

    fn strength(&self, now: f64, settings: LeafLifecycleSettings) -> f32 {
        let threshold_speed = 0.7 + 2.0 * unit(self.life_seed());
        let initial = threshold_speed * threshold_speed;
        let mature = self.growing_since.map_or(self.mature_since, |start| {
            start + f64::from(self.growth_duration(settings))
        });
        let age = (now - mature).max(0.0);
        let decay = (-age / f64::from(settings.half_life_seconds.max(0.01))).exp2() as f32;
        // No spontaneous shedding at zero wind, even after a very long time.
        (0.09 + (initial - 0.09) * decay) * settings.strength.max(0.001)
    }
}

fn mix(mut x: u32) -> u32 {
    x = (x ^ (x >> 16)).wrapping_mul(0x7feb352d);
    x = (x ^ (x >> 15)).wrapping_mul(0x846ca68b);
    x ^ (x >> 16)
}

fn unit(seed: u32) -> f32 {
    (mix(seed) >> 8) as f32 / 16_777_216.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    fn canopy() -> LeafCanopy {
        LeafCanopy::new(
            7,
            11,
            &(0..128)
                .map(|x| UVec3::new(x, 128, 128))
                .collect::<Vec<_>>(),
            0.0,
        )
    }

    #[test]
    fn strong_wind_detaches_every_leaf_immediately_without_a_retention_floor() {
        let settings = LeafLifecycleSettings::default();
        let mut canopy = canopy();
        let events = canopy.plan(0.0, settings, &WindFieldFrame::uniform(Vec2::X * 10.0));
        assert_eq!(events.len(), canopy.len());
        for event in events {
            assert!(canopy.commit(event, 0.0, settings));
            assert!(!canopy.commit(event, 0.0, settings));
        }
        assert!(canopy
            .plan(0.0, settings, &WindFieldFrame::uniform(Vec2::X * 10.0))
            .is_empty());
        assert!((0..canopy.len()).all(|i| canopy.growth(i, 0.0, settings) == 0.0));
    }

    #[test]
    fn modest_wind_selects_stable_individual_strengths_and_weakening_exposes_more() {
        let canopy = canopy();
        let settings = LeafLifecycleSettings::default();
        let wind = WindFieldFrame::uniform(Vec2::X * 1.2);
        let first = canopy.plan(0.0, settings, &wind);
        assert!(!first.is_empty() && first.len() < canopy.len());
        let later = canopy.plan(300.0, settings, &wind);
        assert!(later.len() > first.len());
        assert!(first
            .iter()
            .all(|event| later.iter().any(|other| other.id == event.id)));
        assert_eq!(
            first.iter().map(|e| e.id).collect::<Vec<_>>(),
            canopy
                .plan(0.0, settings, &wind)
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn local_wind_reaches_real_leaf_not_a_cluster_representative() {
        let canopy = LeafCanopy::new(7, 11, &[UVec3::ZERO, UVec3::new(511, 128, 0)], 0.0);
        let mut wind = WindFieldFrame::uniform(Vec2::X * 10.0);
        for (i, pair) in wind.cells.iter_mut().enumerate() {
            pair[0] = if i * 2 % 32 == 31 { 10.0 } else { 0.0 };
            pair[2] = if (i * 2 + 1) % 32 == 31 { 10.0 } else { 0.0 };
        }
        let events = canopy.plan(0.0, LeafLifecycleSettings::default(), &wind);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id.socket, 1);
    }

    #[test]
    fn calm_never_sheds_and_regeneration_has_a_new_generation_and_fresh_strength() {
        let mut canopy = canopy();
        let settings = LeafLifecycleSettings::default();
        assert!(canopy
            .plan(1e9, settings, &WindFieldFrame::default())
            .is_empty());
        let event = canopy.plan(0.0, settings, &WindFieldFrame::uniform(Vec2::X * 10.0))[0];
        assert!(canopy.commit(event, 0.0, settings));
        assert_eq!(canopy.growth(0, 0.0, settings), 0.0);
        assert!((0.0..1.0).contains(&canopy.growth(0, 15.0, settings)));
        assert_eq!(canopy.growth(0, 40.0, settings), 1.0);
        assert!(!canopy.commit(event, 40.0, settings));
        let next = canopy.plan(40.0, settings, &WindFieldFrame::uniform(Vec2::X * 10.0))[0];
        assert_eq!(next.id.generation, event.id.generation + 1);
        assert_ne!(next.seed, event.seed);
    }

    #[test]
    fn plans_are_frame_rate_independent_and_failure_leaves_the_source_attached() {
        let canopy = canopy();
        let settings = LeafLifecycleSettings::default();
        let wind = WindFieldFrame::uniform(Vec2::X * 1.2);
        for _ in 0..600 {
            let _ = canopy.plan(2.0, settings, &wind);
        }
        let after = canopy.plan(10.0, settings, &wind);
        let fresh = super::tests::canopy().plan(10.0, settings, &wind);
        assert_eq!(
            after.iter().map(|e| e.id).collect::<Vec<_>>(),
            fresh.iter().map(|e| e.id).collect::<Vec<_>>()
        );
        assert!((0..canopy.len()).all(|i| canopy.growth(i, 10.0, settings) == 1.0));
    }

    #[test]
    fn replacement_and_reset_do_not_inherit_stale_missing_leaves() {
        let settings = LeafLifecycleSettings::default();
        let mut canopy = canopy();
        let wind = WindFieldFrame::uniform(Vec2::X * 10.0);
        let event = canopy.plan(0.0, settings, &wind)[0];
        let mut replacement = LeafCanopy::new(7, 12, &[event.world_voxel], 0.0);
        assert!(!replacement.commit(event, 0.0, settings));
        canopy.commit(event, 0.0, settings);
        canopy.reset(0.0);
        assert_eq!(canopy.growth(0, 0.0, settings), 1.0);
        assert_eq!(canopy.plan(0.0, settings, &wind).len(), canopy.len());
    }
}
