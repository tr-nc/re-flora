//! Shared, bounded opportunity scheduling over committed vegetation groups.
//! No animal state or GPU resources live here. Counts describe habitat supply, not emitters.
use glam::Vec3;
use rand::{rngs::SmallRng, RngExt, SeedableRng};
use std::collections::HashMap;

pub(crate) const RANGE: f32 = 1.4;
pub(crate) const ATTEMPTS: usize = 2;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub(crate) enum RegionKey {
    Surface(usize, u32), // Stable chunk resource slot and species, until world replacement.
    Canopy(u32),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Region {
    pub key: RegionKey,
    pub kind: usize, // grass, authored plant, canopy
    pub count: u32,
    pub center: Vec3,
    pub radius: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Habitat {
    pub region: RegionKey,
    pub slot: u32,
    pub token: u64, // Root identity or committed canopy generation.
    pub position: Vec3,
    pub kind: usize,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Animal {
    Butterfly = 0,
    Cicada = 1,
}
struct Clock {
    next: Option<f64>,
    rests: HashMap<RegionKey, f64>,
}
pub(crate) struct Ecology {
    regions: Vec<Region>,
    weighted: [Vec<(usize, f64)>; 2],
    totals: [f64; 2],
    clocks: [Clock; 2],
    rng: SmallRng,
}
impl Ecology {
    pub fn new(seed: u64) -> Self {
        Self {
            regions: Vec::new(),
            weighted: Default::default(),
            totals: [0.; 2],
            clocks: std::array::from_fn(|_| Clock {
                next: None,
                rests: HashMap::new(),
            }),
            rng: SmallRng::seed_from_u64(seed),
        }
    }
    /// Metadata only: O(chunks + trees), never O(grass blades + leaf voxels).
    /// Call on a low-frequency observation cadence; sampling between publications is O(log groups).
    pub fn publish(&mut self, mut regions: Vec<Region>, listener: Vec3) {
        regions.retain(|r| {
            r.count > 0
                && r.kind < 3
                && r.center.is_finite()
                && r.radius.is_finite()
                && r.center.distance(listener) <= RANGE + r.radius
        });
        regions.sort_by_key(|r| r.key);
        for animal in 0..2 {
            self.weighted[animal].clear();
            self.totals[animal] = 0.;
            for (i, r) in regions.iter().enumerate() {
                let normalized = r.count as f64 / [1000., 8., 200.][r.kind];
                let preference = [[1., 1.5, 1.], [1., 1., 1.5]][animal][r.kind];
                // Limit each group's influence as well as total frequency.
                self.totals[animal] += preference * normalized / (1. + normalized);
                self.weighted[animal].push((i, self.totals[animal]));
            }
            self.clocks[animal]
                .rests
                .retain(|key, _| regions.binary_search_by_key(key, |r| r.key).is_ok());
            if regions.is_empty() {
                self.clocks[animal].next = None;
            }
        }
        self.regions = regions;
    }
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }
    /// No backlogged births: disabled/full/empty states disarm; re-entry draws a fresh delay.
    /// At most one opportunity per call, even following a long pause.
    pub fn due(&mut self, animal: Animal, now: f64, available: bool, rate_scale: f64) -> bool {
        let a = animal as usize;
        let total = self.totals[a];
        if !available || total == 0. || !rate_scale.is_finite() || rate_scale <= 0. {
            self.clocks[a].next = None;
            return false;
        }
        let due = self.clocks[a].next.is_some_and(|next| now >= next);
        if due || self.clocks[a].next.is_none() {
            let lambda = [0.20, 0.25][a] * total / (1. + total) * rate_scale.min(10.);
            let delay = -(1. - self.rng.random::<f64>()).ln() / lambda;
            self.clocks[a].next = Some(now + [1.5, 2.7][a] + delay);
        }
        due
    }
    pub fn candidate(&mut self, animal: Animal, now: f64) -> Option<(Region, u32)> {
        let a = animal as usize;
        if self.totals[a] == 0. {
            return None;
        }
        let value = self.rng.random::<f64>() * self.totals[a];
        let i = self.weighted[a].partition_point(|(_, cumulative)| *cumulative <= value);
        let r = self.regions[self.weighted[a][i].0];
        if self.clocks[a]
            .rests
            .get(&r.key)
            .is_some_and(|until| now < *until)
        {
            return None;
        }
        Some((r, self.rng.random_range(0..r.count)))
    }
    pub fn accepted(&mut self, animal: Animal, key: RegionKey, now: f64) {
        let rest = match animal {
            Animal::Butterfly => 3.,
            Animal::Cicada => 24. + self.rng.random::<f64>() * 16.,
        };
        self.clocks[animal as usize].rests.insert(key, now + rest);
    }
    pub fn clear(&mut self) {
        self.regions.clear();
        self.weighted.iter_mut().for_each(Vec::clear);
        self.totals = [0.; 2];
        for clock in &mut self.clocks {
            clock.next = None;
            clock.rests.clear();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn region(kind: usize, count: u32) -> Region {
        Region {
            key: RegionKey::Surface(kind, kind as u32),
            kind,
            count,
            center: Vec3::ZERO,
            radius: 0.1,
        }
    }
    #[test]
    fn static_vegetation_continues_to_offer_all_kinds_and_new_positions() {
        for kind in 0..3 {
            let mut ecology = Ecology::new(41);
            ecology.publish(vec![region(kind, 1000)], Vec3::ZERO);
            for animal in [Animal::Butterfly, Animal::Cicada] {
                let mut slots = std::collections::HashSet::new();
                for tick in 0..60000 {
                    let now = tick as f64 * 0.1;
                    if ecology.due(animal, now, true, 1.) {
                        if let Some((r, slot)) = ecology.candidate(animal, now) {
                            assert_eq!(r.kind, kind);
                            slots.insert(slot);
                            ecology.accepted(animal, r.key, now);
                        }
                    }
                }
                assert!(slots.len() > 30, "kind={kind} animal={animal:?}");
            }
        }
    }
    #[test]
    fn empty_full_far_and_replaced_worlds_do_not_accumulate_bursts() {
        let mut e = Ecology::new(3);
        assert!(!e.due(Animal::Cicada, 1000., true, 1.));
        e.publish(vec![region(0, u32::MAX)], Vec3::ZERO);
        assert!(!e.due(Animal::Cicada, 1000., true, 1.));
        assert!(!e.due(Animal::Cicada, 2000., false, 1.));
        assert!(!e.due(Animal::Cicada, 3000., true, 1.));
        assert!(e.due(Animal::Cicada, 4000., true, 1.));
        assert!(!e.due(Animal::Cicada, 4000., true, 1.));
        e.accepted(Animal::Cicada, region(0, 1).key, 4000.);
        assert!(e.candidate(Animal::Cicada, 4001.).is_none());
        e.publish(vec![region(0, 10)], Vec3::splat(100.));
        assert_eq!(e.region_count(), 0);
        assert!(e.clocks[1].rests.is_empty());
        e.clear();
        assert!(!e.due(Animal::Butterfly, 5000., true, 1.));
    }
    #[test]
    fn density_saturates_and_rest_state_is_bounded_by_live_groups() {
        let mut e = Ecology::new(3);
        e.publish(vec![region(0, 1000)], Vec3::ZERO);
        let sparse = e.totals[0];
        e.publish(vec![region(0, u32::MAX)], Vec3::ZERO);
        assert!(e.totals[0] < 2. * sparse);
        for i in 0..1000 {
            let mut r = region(0, 5);
            r.key = RegionKey::Canopy(i);
            e.publish(vec![r], Vec3::ZERO);
            e.accepted(Animal::Cicada, r.key, 0.);
            assert_eq!(e.clocks[1].rests.len(), 1);
        }
    }
}
