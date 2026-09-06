//! A small colony attached to live vegetation. Habitats are facts, not persistent world entities.
use super::SpatialSoundManager;
use anyhow::Result;
use glam::Vec3;
use std::collections::HashMap;
use uuid::Uuid;

const CLIPS: [&str; 2] = [
    "assets/sfx/summer_cicadas/dog_day_01.wav",
    "assets/sfx/summer_cicadas/linne_01.wav",
];
const MAX_HABITATS_PER_KIND: usize = 6;
const MAX_CALLS: usize = 3;
const MIN_START_GAP: f64 = 2.7;
const AUDIBLE_RADIUS: f32 = 1.4; // World units; the shared spatial engine scales distance by 15.
const HABITAT_SPACING: f32 = 0.22;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CicadaHabitatKey {
    Grass([u32; 3]),
    Canopy(u32, u64, u64), // Canonical tree, canopy generation, real leaf sample.
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CicadaHabitat {
    pub key: CicadaHabitatKey,
    pub position: Vec3,
}

impl CicadaHabitat {
    fn is_grass(self) -> bool {
        matches!(self.key, CicadaHabitatKey::Grass(_))
    }

    fn seed(self) -> u64 {
        let words = match self.key {
            CicadaHabitatKey::Grass([x, y, z]) => [x as u64, y as u64, z as u64, 0],
            CicadaHabitatKey::Canopy(tree, generation, sample) => {
                [tree as u64, generation, sample, 1]
            }
        };
        words
            .into_iter()
            .fold(0x243f_6a88_85a3_08d3, |seed, word| mix(seed ^ word))
    }
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn fraction(seed: u64) -> f64 {
    (mix(seed) >> 11) as f64 / (1_u64 << 53) as f64
}

fn select_habitats(mut candidates: Vec<CicadaHabitat>, listener: Vec3) -> Vec<CicadaHabitat> {
    candidates.retain(|site| {
        site.position.is_finite()
            && site.position.distance_squared(listener) <= AUDIBLE_RADIUS * AUDIBLE_RADIUS
    });
    candidates.sort_by(|a, b| {
        a.position
            .distance_squared(listener)
            .total_cmp(&b.position.distance_squared(listener))
            .then_with(|| a.key.cmp(&b.key))
    });
    let mut selected: Vec<CicadaHabitat> = Vec::new();
    for candidate in candidates {
        if selected
            .iter()
            .filter(|site| site.is_grass() == candidate.is_grass())
            .count()
            >= MAX_HABITATS_PER_KIND
        {
            continue;
        }
        if selected.iter().any(|site| {
            site.key == candidate.key
                || (site.is_grass() == candidate.is_grass()
                    && site.position.distance_squared(candidate.position)
                        < HABITAT_SPACING * HABITAT_SPACING)
        }) {
            continue;
        }
        selected.push(candidate);
    }
    selected.sort_by_key(|site| site.key);
    selected
}

struct Schedule {
    next: f64,
    calls: u64,
}

#[derive(Default)]
struct Cadence {
    sites: Vec<CicadaHabitat>,
    schedules: HashMap<CicadaHabitatKey, Schedule>,
    next_start: f64,
}

impl Cadence {
    fn reconcile(&mut self, sites: Vec<CicadaHabitat>, now: f64) {
        self.schedules
            .retain(|key, _| sites.iter().any(|site| site.key == *key));
        for site in &sites {
            self.schedules.entry(site.key).or_insert_with(|| Schedule {
                next: now + 0.4 + fraction(site.seed()) * 7.0,
                calls: 0,
            });
        }
        self.sites = sites;
    }

    fn next_call(&mut self, now: f64, active: &[CicadaHabitatKey]) -> Option<CicadaHabitat> {
        if active.len() >= MAX_CALLS || now < self.next_start {
            return None;
        }
        let site = self
            .sites
            .iter()
            .filter(|site| !active.contains(&site.key))
            .filter(|site| self.schedules[&site.key].next <= now)
            .min_by(|a, b| {
                self.schedules[&a.key]
                    .next
                    .total_cmp(&self.schedules[&b.key].next)
                    .then_with(|| a.key.cmp(&b.key))
            })
            .copied()?;
        self.next_start = now + MIN_START_GAP;
        Some(site)
    }

    fn defer(&mut self, site: CicadaHabitat, end: f64) {
        let schedule = self.schedules.get_mut(&site.key).unwrap();
        schedule.calls += 1;
        schedule.next = end + 12.0 + fraction(site.seed() ^ schedule.calls) * 16.0;
    }
}

struct ActiveCall {
    site: CicadaHabitat,
    source: Uuid,
    end: f64,
}

pub(crate) struct SummerCicadas {
    audio: SpatialSoundManager,
    durations: [f64; 2],
    cadence: Cadence,
    active: Vec<ActiveCall>,
    next_refresh: f64,
    next_summary: f64,
    started: u64,
    retired: u64,
    high_water: usize,
}

impl SummerCicadas {
    pub(crate) fn new(audio: SpatialSoundManager) -> Result<Self> {
        let durations = [
            audio.transient_clip_duration_seconds(CLIPS[0])?,
            audio.transient_clip_duration_seconds(CLIPS[1])?,
        ];
        anyhow::ensure!(
            durations
                .iter()
                .all(|d| d.is_finite() && *d > 0.0 && *d <= 15.0),
            "cicada calls must be finite and at most 15 seconds"
        );
        log::info!("[AUDIO][CICADAS][ASSETS] loaded=2 durations={durations:?} max_calls={MAX_CALLS} max_habitats={} start_gap={MIN_START_GAP} rest=12..28s", 2 * MAX_HABITATS_PER_KIND);
        Ok(Self {
            audio,
            durations,
            cadence: Cadence::default(),
            active: Vec::new(),
            next_refresh: 0.0,
            next_summary: 0.0,
            started: 0,
            retired: 0,
            high_water: 0,
        })
    }

    pub(crate) fn active_habitats(&self) -> impl Iterator<Item = CicadaHabitatKey> + '_ {
        self.active.iter().map(|call| call.site.key)
    }

    pub(crate) fn refresh_due(&self, now: f64) -> bool {
        now >= self.next_refresh
    }

    pub(crate) fn refresh(&mut self, candidates: Vec<CicadaHabitat>, listener: Vec3, now: f64) {
        let count = candidates.len();
        let sites = select_habitats(candidates, listener);
        let changed = self.cadence.sites.iter().map(|s| s.key).collect::<Vec<_>>()
            != sites.iter().map(|s| s.key).collect::<Vec<_>>();
        if changed {
            log::info!("[AUDIO][CICADAS][HABITATS] candidates={count} grass={} canopy={} listener={listener:?}",
                sites.iter().filter(|s| s.is_grass()).count(), sites.iter().filter(|s| !s.is_grass()).count());
        }
        self.cadence.reconcile(sites, now);
        self.next_refresh = now + 0.5;
    }

    pub(crate) fn update(&mut self, now: f64) -> Result<()> {
        // Destroy before allocating. Failed retirements retain their slot and ownership for retry.
        let mut index = 0;
        while index < self.active.len() {
            let call = &self.active[index];
            let habitat_lost = !self.cadence.schedules.contains_key(&call.site.key);
            if now >= call.end || habitat_lost {
                self.retire(
                    index,
                    if habitat_lost {
                        "habitat_lost"
                    } else {
                        "call_complete"
                    },
                )?;
            } else {
                index += 1;
            }
        }
        let active = self
            .active
            .iter()
            .map(|call| call.site.key)
            .collect::<Vec<_>>();
        if let Some(site) = self.cadence.next_call(now, &active) {
            let clip = usize::from(site.is_grass());
            let end = now + self.durations[clip] + 0.15;
            self.cadence.defer(site, end);
            let gain_db = if site.is_grass() { -24.0 } else { -20.0 };
            let source = self
                .audio
                .add_spatial_one_shot(CLIPS[clip], gain_db, site.position)?;
            self.active.push(ActiveCall { site, source, end });
            self.started += 1;
            self.high_water = self.high_water.max(self.active.len());
            log::info!("[AUDIO][CICADAS][CALL] time={now:.3} habitat={:?} position={:?} clip={} gain_db={gain_db} active={} end={end:.3}", site.key, site.position, CLIPS[clip], self.active.len());
        }
        if now >= self.next_summary {
            let runtime = self.audio.runtime_diagnostics();
            log::info!("[AUDIO][CICADAS][SUMMARY] time={now:.3} habitats={} active={} high_water={} started={} retired={} bound={MAX_CALLS} runtime_total_emitters={} runtime_total_voices={}",
                self.cadence.sites.len(), self.active.len(), self.high_water, self.started, self.retired, runtime.active_emitters, runtime.active_voices);
            self.next_summary = now + 5.0;
        }
        Ok(())
    }

    fn retire(&mut self, index: usize, reason: &str) -> Result<()> {
        let call = &self.active[index];
        self.audio.try_remove_source(call.source)?;
        log::info!(
            "[AUDIO][CICADAS][RETIRE] habitat={:?} reason={reason} remaining={}",
            call.site.key,
            self.active.len() - 1
        );
        self.active.swap_remove(index);
        self.retired += 1;
        Ok(())
    }

    pub(crate) fn clear(&mut self, reason: &str) -> Result<()> {
        self.cadence = Cadence::default();
        self.next_refresh = 0.0;
        while !self.active.is_empty() {
            self.retire(self.active.len() - 1, reason)?;
        }
        log::info!("[AUDIO][CICADAS][CLEAR] reason={reason} active=0 habitats=0 started={} retired={} high_water={}", self.started, self.retired, self.high_water);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sites() -> Vec<CicadaHabitat> {
        (0..40)
            .map(|i| CicadaHabitat {
                key: if i % 2 == 0 {
                    CicadaHabitatKey::Grass([i, 0, 0])
                } else {
                    CicadaHabitatKey::Canopy(i, 1, 1)
                },
                position: Vec3::new((i % 5) as f32 * 0.25, 0.0, (i / 5) as f32 * 0.25),
            })
            .collect()
    }

    #[test]
    fn habitat_selection_is_bounded_spaced_nearby_and_order_independent() {
        let candidates = sites();
        let selected = select_habitats(candidates.clone(), Vec3::ZERO);
        assert_eq!(selected.len(), 12);
        assert_eq!(selected.iter().filter(|s| s.is_grass()).count(), 6);
        let reversed = select_habitats(candidates.into_iter().rev().collect(), Vec3::ZERO);
        assert_eq!(
            selected.iter().map(|s| s.key).collect::<Vec<_>>(),
            reversed.iter().map(|s| s.key).collect::<Vec<_>>()
        );
        for (i, a) in selected.iter().enumerate() {
            assert!(a.position.length() <= AUDIBLE_RADIUS);
            for b in &selected[i + 1..] {
                assert!(
                    a.is_grass() != b.is_grass()
                        || a.position.distance(b.position) >= HABITAT_SPACING
                );
            }
        }
        assert!(select_habitats(sites(), Vec3::splat(100.0)).is_empty());
    }

    #[test]
    fn ten_minutes_of_calls_have_bounded_concurrency_stagger_and_rests() {
        let mut cadence = Cadence::default();
        cadence.reconcile(select_habitats(sites(), Vec3::ZERO), 0.0);
        let mut active: Vec<(CicadaHabitatKey, f64)> = Vec::new();
        let mut last_end = HashMap::new();
        let mut previous_start = -100.0;
        let mut starts = 0;
        for tick in 0..6000 {
            let now = tick as f64 * 0.1;
            active.retain(|(_, end)| *end > now);
            let keys = active.iter().map(|(key, _)| *key).collect::<Vec<_>>();
            if let Some(site) = cadence.next_call(now, &keys) {
                assert!(now - previous_start >= MIN_START_GAP - 1e-9);
                if let Some(end) = last_end.get(&site.key) {
                    assert!(now - end >= 12.0);
                }
                previous_start = now;
                let end = now + 10.2;
                cadence.defer(site, end);
                last_end.insert(site.key, end);
                active.push((site.key, end));
                starts += 1;
            }
            assert!(active.len() <= MAX_CALLS);
            assert!(cadence.schedules.len() <= 12);
        }
        assert!(starts > 100);
        cadence.reconcile(Vec::new(), 600.0);
        assert!(cadence.schedules.is_empty());
        assert!(cadence.next_call(1000.0, &[]).is_none());
        cadence.reconcile(select_habitats(sites(), Vec3::ZERO), 1000.0);
        assert!(cadence.next_call(1000.0, &[]).is_none());
    }
}
