//! Opt-in native Release fixture. Production flower placement/contacts, real
//! emitter handles, real saved-field controls in memory only; no config writes.
use super::App;
use crate::particles::flower_visits::FlowerTarget;
use crate::particles::{ButterflyEmitter, ButterflySpawnSource, ParticleSystem};
use glam::{Vec2, Vec3};

pub(super) struct Review {
    mode: String,
    start: Option<f32>,
    next_log: f32,
    gust: bool,
    removed: bool,
    center: Vec3,
    passed: bool,
}
impl Review {
    pub fn from_env() -> Option<Self> {
        std::env::var("RE_FLORA_BUTTERFLY_FORAGING_REVIEW")
            .ok()
            .map(|mode| {
                assert!(
                    ["visits", "close", "wind", "lifetime"].contains(&mode.as_str()),
                    "foraging review must be visits/close/wind/lifetime"
                );
                Self {
                    mode,
                    start: None,
                    next_log: 0.,
                    gust: false,
                    removed: false,
                    center: Vec3::ZERO,
                    passed: false,
                }
            })
    }
    pub fn seed(
        &mut self,
        emitters: &mut [ButterflyEmitter],
        system: &mut ParticleSystem,
        flowers: &[FlowerTarget],
        time: f32,
    ) {
        if self.start.is_some() {
            return;
        }
        let Some(flower) = flowers.first() else {
            return;
        };
        let Some(emitter) = emitters.first_mut() else {
            return;
        };
        for i in 0..2 {
            let p = flower.position + Vec3::Y * 0.018 + Vec3::X * (i as f32 * 0.003);
            assert!(emitter
                .spawn_at(system, ButterflySpawnSource::tree_leaf(p))
                .is_some());
        }
        self.start = Some(time);
        self.center = flower.position;
        log::info!(
            "[BUTTERFLY_FORAGING][REVIEW] mode={} seeded=2 actual_heads={} saved=false",
            self.mode,
            flowers.len()
        );
    }
    pub fn observe(&mut self, emitters: &[ButterflyEmitter], time: f32) {
        let Some(start) = self.start else {
            return;
        };
        let elapsed = time - start;
        if elapsed < self.next_log {
            return;
        }
        self.next_log = elapsed + 1.;
        let mut choices = 0;
        let mut landings = 0;
        let mut departures = 0;
        let mut switched = 0;
        let mut lost = 0;
        let mut wind = 0;
        let mut stalls = 0;
        let mut phases = [0; 6];
        for e in emitters {
            let s = e.foraging_stats();
            choices += s.choices;
            landings += s.landings;
            departures += s.departures;
            switched += s.different_flowers;
            lost += s.lost_targets;
            wind += s.wind_interruptions;
            stalls += s.stalls;
            for (total, count) in phases.iter_mut().zip(e.foraging_phases()) {
                *total += count;
            }
        }
        log::info!("[BUTTERFLY_FORAGING][REVIEW] elapsed={elapsed:.2} choices={choices} landings={landings} departures={departures} different_flowers={switched} lost={lost} wind={wind} stalls={stalls} phases={phases:?}");
        if !self.passed && switched > 0 && departures > 0 {
            self.passed = true;
            log::info!("[BUTTERFLY_FORAGING][REVIEW_PASS] complete_cycle=true distinct_hosts=true");
        }
    }
}
impl App {
    pub(super) fn prepare_butterfly_foraging_review(&mut self) {
        let Some(review) = &mut self.butterfly_foraging_review else {
            return;
        };
        let now = self.time_info.time_since_start();
        let elapsed = review.start.map_or(0., |t| now - t);
        let settings = &mut self.debug_settings;
        settings.adjustables.butterflies_enabled.value = true;
        settings.butterfly_flight.variant = crate::particles::ButterflyFlightVariant::Darting;
        settings.butterfly_flight.foraging.dwell_seconds =
            if review.mode == "close" { 10. } else { 1. };
        settings.butterfly_flight.foraging.search_radius = 0.7;
        settings.butterfly_flight.foraging.wind_limit = 2.;
        settings.butterfly_flight.tuning.speed = 1.;
        settings.adjustables.butterfly_lifetime_min.value = 60.;
        settings.adjustables.butterfly_lifetime_max.value = 60.;
        if review.mode == "close" {
            if let Some(p) = self.particle_snapshots.iter().find(|p| {
                p.butterfly_body_pose
                    .is_some_and(|body| body.host.is_some())
            }) {
                let target = p.position_ws;
                self.camera_control.apply_snapshot_mode(true);
                self.camera_control.set_orbit_focus(target);
                self.tracer.set_camera_pose_looking_at(
                    target
                        + p.butterfly_body_pose.unwrap().orientation
                            * Vec3::new(0.012, 0.035, 0.018),
                    target,
                );
                self.reset_camera_movement_input();
            }
        }
        let review = self.butterfly_foraging_review.as_mut().unwrap();
        if review.mode == "lifetime" && elapsed > 1. && !review.removed {
            if let Some(host) = self
                .particle_snapshots
                .iter()
                .find_map(|p| p.butterfly_body_pose.and_then(|p| p.host))
            {
                if let Some(root) = self
                    .surface_builder
                    .get_resources()
                    .instances
                    .chunk_flora_instances
                    .iter()
                    .flat_map(|(_, r)| &r.authored_response_instances)
                    .find(|p| p.response_id == host.plant)
                    .map(|p| p.base_world_vox)
                {
                    let point = root.as_vec3() / 256.;
                    let removed = self
                        .surface_builder
                        .remove_authored_flora_for_brush(root / 256, point, point, 1. / 256.)
                        .expect("foraging fixture host deletion");
                    assert_eq!(removed, 1);
                    review.removed = true;
                    log::info!("[BUTTERFLY_FORAGING][REVIEW_REMOVE] occupied_host={host:?} removed=1 production_edit=true");
                }
            }
        }
        if review.mode == "wind" && elapsed > 8. && !review.gust {
            review.gust = true;
            assert!(self.wind_prototype.field.release_with_settings(
                review.center,
                Vec2::X,
                crate::wind_field::GustSettings {
                    strength: 6.,
                    speed: 0.,
                    width: 1024.,
                    depth: 1024.,
                    duration: 4.,
                    softness: 0.5
                }
            ));
            log::info!(
                "[BUTTERFLY_FORAGING][REVIEW_GUST] strength=6 duration=4 production_field=true"
            );
        }
    }
}
