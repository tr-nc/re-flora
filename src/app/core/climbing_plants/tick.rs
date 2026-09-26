//! Own the playable vine's advance protocol, not its solver or App presentation.
use crate::builder::ContreeCpuVoxelSourceDependency;
use crate::climbing_plants::{Plant, Pruned, Terrain};
use crate::geom::UAabb3;
use glam::Vec3;
use std::time::Instant;

const MOTION_HZ: f32 = 20.0;
const MOTION_DT: f32 = 1.0 / MOTION_HZ;

/// Availability and identity of one immutable export. This is not another Terrain
/// abstraction: Patch and the test Wall still implement the existing Terrain seam.
pub(super) struct Snapshot<'a, T> {
    pub terrain: &'a T,
    pub dependencies: &'a [ContreeCpuVoxelSourceDependency],
}

#[derive(Clone, Copy)]
pub(super) enum Cadence {
    Play {
        dt: f32,
        growth_per_second: f32,
    },
    /// Deliberately accelerated fixture sampling, independent of world time.
    Review {
        growing: bool,
    },
}

#[derive(Clone, Copy)]
pub(super) struct Search {
    pub turn: f32,
    pub reach: f32,
    pub rate: f32,
}

#[derive(Clone, Copy)]
pub(super) struct Tuning {
    pub spacing: f32,
    pub flexibility: f32,
    /// Reviews keep the seeded phenotype; play can change artistic controls live.
    pub search: Option<Search>,
}

#[derive(Clone, Copy)]
pub(super) enum Action {
    PruneHighest,
    PruneToRoot,
    DisconnectRoot,
}

#[derive(Default)]
struct Actions {
    prune_highest: bool,
    prune_to_root: bool,
    disconnect_root: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Event {
    PrunedHighest(Pruned),
    PrunedToRoot(Pruned),
    DisconnectedRoot,
    SupportPruned {
        pruned: Pruned,
        remaining_nodes: usize,
    },
}

#[derive(Default)]
pub(super) struct Timings {
    pub revalidate_us: u128,
    pub growth_us: u128,
    pub pose_us: u128,
}

#[derive(Default)]
pub(super) struct Report {
    /// False when snapshot acquisition/revalidation held the update. An unavailable
    /// motion step still permits publishing earlier committed work, as before.
    pub publish: bool,
    pub waiting_for_terrain: bool,
    pub events: Vec<Event>,
    pub growth_attempts: u32,
    pub motion_steps: u32,
    pub nodes_before_motion: usize,
    pub timings: Timings,
}

pub(super) struct VineTick {
    plant: Plant,
    growth_clock: QuantumClock,
    motion_clock: QuantumClock,
    terrain_dirty: bool,
    last_dependencies: Vec<ContreeCpuVoxelSourceDependency>,
    actions: Actions,
    growth_blocked: bool,
}

impl VineTick {
    /// A new plant always starts with new clocks and validation history.
    pub fn new(plant: Plant) -> Self {
        Self {
            plant,
            growth_clock: QuantumClock::default(),
            motion_clock: QuantumClock::default(),
            terrain_dirty: true,
            last_dependencies: Vec::new(),
            actions: Actions::default(),
            growth_blocked: false,
        }
    }

    pub fn plant(&self) -> &Plant {
        &self.plant
    }

    pub fn growth_blocked(&self) -> bool {
        self.growth_blocked
    }

    /// Repeated button requests coalesce and wait for a current snapshot. Their
    /// execution order is stable regardless of the order in which UI queues them.
    pub fn request(&mut self, action: Action) {
        match action {
            Action::PruneHighest => self.actions.prune_highest = true,
            Action::PruneToRoot => self.actions.prune_to_root = true,
            Action::DisconnectRoot => self.actions.disconnect_root = true,
        }
    }

    pub fn observe_edit(&mut self, bound: UAabb3) {
        // Cover backing between sparse anchors as well as stem clearance.
        let (min, max) = self.plant.nodes.iter().fold(
            (self.plant.nodes[0].position, self.plant.nodes[0].position),
            |(min, max), node| (min.min(node.position), max.max(node.position)),
        );
        let min = (min - Vec3::splat(3.0)).max(Vec3::ZERO).floor().as_uvec3();
        let max = (max + Vec3::splat(3.0)).ceil().as_uvec3();
        if min.cmple(bound.max()).all() && max.cmpge(bound.min()).all() {
            self.terrain_dirty = true;
        }
    }

    pub fn advance(
        &mut self,
        snapshot: Option<Snapshot<'_, impl Terrain>>,
        cadence: Cadence,
        tuning: Tuning,
        profile: bool,
    ) -> Report {
        let start = profile.then(Instant::now);
        let elapsed_us = || start.map_or(0, |start| start.elapsed().as_micros());
        let mut report = Report {
            waiting_for_terrain: true,
            ..Report::default()
        };
        let Some(Snapshot {
            terrain,
            dependencies,
        }) = snapshot
        else {
            return report;
        };
        // Readiness is checked every time, even if dependency identities match.
        if !terrain.current() {
            return report;
        }
        if let Some(search) = tuning.search {
            self.plant.set_search_tuning(search.turn, search.reach);
            self.plant.set_search_rate(search.rate);
        }
        let actions = std::mem::take(&mut self.actions);
        if actions.prune_highest {
            if let Some(pruned) = self.plant.prune_highest_attachment() {
                report.events.push(Event::PrunedHighest(pruned));
            }
        }
        if actions.prune_to_root {
            report
                .events
                .push(Event::PrunedToRoot(self.plant.prune_to_root()));
        }
        if actions.disconnect_root {
            self.plant.disconnect_root();
            report.events.push(Event::DisconnectedRoot);
        }
        // A failed validation never acknowledges the new dependencies or edit.
        // Plant keeps its existing transactional pruning/solver semantics.
        if self.terrain_dirty || self.last_dependencies != dependencies {
            let Some(pruned) = self.plant.revalidate(terrain) else {
                return report;
            };
            self.terrain_dirty = false;
            self.last_dependencies.clear();
            self.last_dependencies.extend_from_slice(dependencies);
            if pruned.removed > 0 {
                report.events.push(Event::SupportPruned {
                    pruned,
                    remaining_nodes: self.plant.nodes.len(),
                });
            }
        }
        report.timings.revalidate_us = elapsed_us();
        report.waiting_for_terrain = false;
        report.publish = true;
        report.nodes_before_motion = self.plant.nodes.len();
        let (pose_steps, exploring) = match cadence {
            Cadence::Play { dt, .. } => (self.motion_clock.quanta(dt, MOTION_HZ), true),
            Cadence::Review { growing } => (2, growing),
        };
        for i in 0..pose_steps {
            // Never batch all births ahead of all motion: both schedules use the
            // same birth-before-motion semantics, but not the same clock policy.
            let births = match cadence {
                Cadence::Play {
                    growth_per_second, ..
                } => self.growth_clock.quanta(MOTION_DT, growth_per_second),
                Cadence::Review { growing } => u32::from(i == 0 && growing),
            };
            let begin = elapsed_us();
            for _ in 0..births {
                self.plant.grow(terrain, tuning.spacing);
            }
            report.timings.growth_us += elapsed_us() - begin;
            report.growth_attempts += births;
            if self
                .plant
                .step_motion(
                    terrain,
                    MOTION_DT,
                    tuning.flexibility,
                    tuning.spacing,
                    exploring,
                )
                .is_none()
            {
                report.waiting_for_terrain = true;
                break;
            }
            report.motion_steps += 1;
        }
        if report.growth_attempts > 0 {
            self.growth_blocked = self.plant.nodes.len() == report.nodes_before_motion;
        }
        report.timings.pose_us =
            elapsed_us() - report.timings.revalidate_us - report.timings.growth_us;
        report
    }
}

#[derive(Default)]
struct QuantumClock {
    accumulator: f64,
}
impl QuantumClock {
    fn quanta(&mut self, dt: f32, speed: f32) -> u32 {
        if !dt.is_finite() || !speed.is_finite() || dt <= 0.0 || speed <= 0.0 {
            return 0;
        }
        // Bound work, dropping debt. f32 world durations need roundoff tolerance
        // so five 10 ms frames and one 50 ms frame produce the same quantum.
        self.accumulator = (self.accumulator + f64::from(dt) * f64::from(speed)).min(8.0);
        let quanta = (self.accumulator + 1e-6).floor() as u32;
        self.accumulator = (self.accumulator - f64::from(quanta)).max(0.0);
        quanta
    }
}

#[cfg(test)]
mod tests;
