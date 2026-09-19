//! Audio ownership only. Shared ecology decides when and where a call may begin.
use super::SpatialSoundManager;
use crate::ecology::Habitat;
use anyhow::Result;
use uuid::Uuid;
const CLIPS: [&str; 2] = [
    "assets/sfx/summer_cicadas/dog_day_01.wav",
    "assets/sfx/summer_cicadas/linne_01.wav",
];
const MAX_CALLS: usize = 3;

fn call_profile(habitat_kind: usize) -> (super::mixer::AudioCategory, usize, f32) {
    if habitat_kind == 2 {
        (super::mixer::AudioCategory::TreeCicadas, 0, -20.0)
    } else {
        (super::mixer::AudioCategory::GroundCicadas, 1, -24.0)
    }
}

struct ActiveCall {
    site: Habitat,
    source: Uuid,
    end: f64,
}
pub(crate) struct SummerCicadas {
    audio: SpatialSoundManager,
    durations: [f64; 2],
    active: Vec<ActiveCall>,
    next_start: f64,
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
                .all(|d| d.is_finite() && *d > 0. && *d <= 15.),
            "cicada calls must be finite and at most 15 seconds"
        );
        log::info!("[AUDIO][CICADAS][ASSETS] loaded=2 durations={durations:?} max_calls={MAX_CALLS} shared_ecology=true start_gap=2.7");
        Ok(Self {
            audio,
            durations,
            active: Vec::new(),
            next_start: 0.,
            next_summary: 0.,
            started: 0,
            retired: 0,
            high_water: 0,
        })
    }
    pub(crate) fn active_habitats(&self) -> impl Iterator<Item = Habitat> + '_ {
        self.active.iter().map(|c| c.site)
    }
    pub(crate) fn has_capacity(&self) -> bool {
        self.active.len() < MAX_CALLS
    }
    pub(crate) fn start(&mut self, site: Habitat, now: f64) -> Result<bool> {
        if !self.has_capacity()
            || now < self.next_start
            || self
                .active
                .iter()
                .any(|c| c.site.position.distance_squared(site.position) < 0.22 * 0.22)
        {
            return Ok(false);
        }
        let (category, clip, gain_db) = call_profile(site.kind);
        let source =
            self.audio
                .add_spatial_one_shot(category, CLIPS[clip], gain_db, site.position)?;
        let end = now + self.durations[clip] + 0.15;
        self.active.push(ActiveCall { site, source, end });
        self.next_start = now + 2.7;
        self.started += 1;
        self.high_water = self.high_water.max(self.active.len());
        log::info!(
            "[AUDIO][CICADAS][CALL] time={now:.3} habitat={site:?} clip={} active={} end={end:.3}",
            CLIPS[clip],
            self.active.len()
        );
        Ok(true)
    }
    /// A low-frequency live validation supplies only still-valid hosts. Failed reads must not
    /// keep an unverified emitter alive. Expired calls are retired every frame regardless.
    pub(crate) fn update(&mut self, now: f64, valid: Option<&[Habitat]>) -> Result<()> {
        let mut i = 0;
        while i < self.active.len() {
            let c = &self.active[i];
            let lost = valid.is_some_and(|v| !v.contains(&c.site));
            if lost || now >= c.end {
                self.retire(
                    i,
                    if lost {
                        "habitat_lost"
                    } else {
                        "call_complete"
                    },
                )?;
            } else {
                i += 1;
            }
        }
        if now >= self.next_summary {
            let runtime = self.audio.runtime_diagnostics();
            log::info!("[AUDIO][CICADAS][SUMMARY] time={now:.3} active={} high_water={} started={} retired={} bound={MAX_CALLS} runtime_total_emitters={} runtime_total_voices={}", self.active.len(), self.high_water, self.started, self.retired, runtime.active_emitters, runtime.active_voices);
            self.next_summary = now + 5.;
        }
        Ok(())
    }
    fn retire(&mut self, index: usize, reason: &str) -> Result<()> {
        self.audio.try_remove_source(self.active[index].source)?;
        log::info!(
            "[AUDIO][CICADAS][RETIRE] habitat={:?} reason={reason} remaining={}",
            self.active[index].site,
            self.active.len() - 1
        );
        self.active.swap_remove(index);
        self.retired += 1;
        Ok(())
    }
    pub(crate) fn clear(&mut self, reason: &str) -> Result<()> {
        while !self.active.is_empty() {
            self.retire(self.active.len() - 1, reason)?;
        }
        self.next_start = 0.;
        log::info!(
            "[AUDIO][CICADAS][CLEAR] reason={reason} active=0 started={} retired={} high_water={}",
            self.started,
            self.retired,
            self.high_water
        );
        Ok(())
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use crate::audio::mixer::AudioCategory;

    #[test]
    fn habitats_route_to_independent_buses_without_changing_assets_or_trims() {
        assert_eq!(call_profile(2), (AudioCategory::TreeCicadas, 0, -20.0));
        for kind in [0, 1] {
            assert_eq!(call_profile(kind), (AudioCategory::GroundCicadas, 1, -24.0));
        }
    }
}
