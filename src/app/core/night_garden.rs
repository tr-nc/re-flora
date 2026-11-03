//! Night visibility and firefly presentation. Flowers use ordinary surface lighting.
use super::*;

#[derive(Default)]
pub(super) struct NightGarden {
    pub(super) fireflies: crate::particles::fireflies::Fireflies,
    firefly_review_start: Option<f32>,
}

pub(super) fn night_factor(altitude: f32) -> f32 {
    let t = ((-altitude - 0.05) / 0.35).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

impl App {
    pub(super) fn current_night_visibility(&self) -> f32 {
        let gui = &self.debug_settings.adjustables;
        let (altitude, _) = Self::calculate_sun_position(
            self.world_clock.live_time_of_day(),
            gui.latitude.value,
            gui.season.value,
        );
        night_factor(altitude)
    }
    pub(super) fn append_firefly_models(&mut self, frame: crate::particles::ButterflyFrame) {
        let night = self.current_night_visibility();
        if !self.render_flags.enable_particles || !self.terrain_persistence.allows_world_updates() {
            return;
        }
        let review = std::env::var("RE_FLORA_NIGHT_REVIEW").unwrap_or_default();
        if self.night_garden.firefly_review_start.is_none()
            && matches!(review.as_str(), "fireflies" | "fireflies-cycle")
        {
            let y = self.query_terrain_height_cpu(Vec2::new(0.82, 1.22));
            for i in 0..24 {
                self.night_garden.fireflies.spawn(
                    Vec3::new(
                        0.74 + (i % 8) as f32 * 0.025,
                        y,
                        1.18 + (i / 8) as f32 * 0.035,
                    ),
                    frame.time_seconds() - 2.,
                );
            }
            self.night_garden.firefly_review_start = Some(frame.time_seconds());
            log::info!("[FIREFLY_REVIEW] spawned=24 model_cache=true terrain_voxels=false night={night} saved=false");
        }
        if review == "fireflies-cycle"
            && night > 0.
            && self
                .night_garden
                .firefly_review_start
                .is_some_and(|t| frame.time_seconds() - t >= 4.)
        {
            self.set_manual_time_of_day(0.45);
            log::info!("[FIREFLY_REVIEW] switched_to_day=true saved=false");
        }
        self.night_garden
            .fireflies
            .append(frame, night, &mut self.particle_snapshots);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn firefly_visibility_is_zero_by_day_and_smooth_at_twilight() {
        assert_eq!(night_factor(1.), 0.);
        assert_eq!(night_factor(0.), 0.);
        assert_eq!(night_factor(-0.05), 0.);
        assert!((night_factor(-0.225) - 0.5).abs() < 1e-6);
        assert_eq!(night_factor(-0.4), 1.);
        assert_eq!(night_factor(-1.), 1.);
    }
}
