//! Bounded, stable night-flower light clusters. Blinking insects deliberately do
//! not restart multi-frame GI every flash. Flower emission itself stays per-petal.
use super::*;
use crate::lighting::{
    LocalLight, LocalLightProviderSnapshot, PointLight, ProviderId, SourceLight, SourceLightKey,
};
use std::collections::BTreeMap;

const PROVIDER: ProviderId = ProviderId::new(4);
const MAX_FLOWER_LIGHTS: usize = 2;
#[derive(Default)]
pub(super) struct NightGarden {
    next_update: f64,
    revision: u64,
    sources: Vec<SourceLight>,
    pub(super) fireflies: crate::particles::fireflies::Fireflies,
    firefly_review_start: Option<f32>,
}

pub(super) fn night_factor(altitude: f32) -> f32 {
    let t = ((-altitude - 0.05) / 0.35).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn blue_species() -> u32 {
    crate::flora::models::FLOWER_REGISTRY
        .iter()
        .position(|(id, _, _)| *id == "forget-me-not")
        .expect("glowing blue flower registered") as u32
        + crate::flora::species::MODEL_FLOWER_FIRST_SPECIES
}
fn flower_sources(
    plants: impl Iterator<Item = (u32, UVec3)>,
    listener: Vec3,
    factor: f32,
) -> Vec<SourceLight> {
    if factor <= 0. {
        return Vec::new();
    }
    let blue = blue_species();
    let mut clusters = BTreeMap::<[u32; 3], (Vec3, u32)>::new();
    for (species, root) in plants {
        if species != blue {
            continue;
        }
        let p = (root.as_vec3() + Vec3::new(0.5, 1., 0.5)) / 256.;
        if p.distance_squared(listener) > 4. {
            continue;
        }
        let cluster = clusters.entry((root / 64).to_array()).or_default();
        cluster.0 += p;
        cluster.1 += 1;
    }
    // Stable spatial order, not frame-varying brightest/lightest selection.
    // Higher-priority authored/terrain sources keep the registry's eight-light budget.
    clusters
        .into_iter()
        .take(MAX_FLOWER_LIGHTS)
        .map(|(cell, (sum, count))| {
            let position = sum / count as f32 + Vec3::Y * 0.035;
            let light = PointLight::new(
                position,
                Vec3::new(0.20, 0.48, 1.),
                0.0008 * factor * (count as f32).sqrt().min(3.),
                0.015,
                0.24,
            )
            .expect("bounded blue-flower light");
            SourceLight::new(
                SourceLightKey::new(
                    u64::from(cell[0]) | (u64::from(cell[1]) << 32),
                    u64::from(cell[2]),
                ),
                LocalLight::Point(light),
            )
        })
        .collect()
}
impl NightGarden {
    fn publish(
        &mut self,
        registry: &mut LocalLightRegistry,
        sources: Vec<SourceLight>,
    ) -> Result<bool> {
        if sources == self.sources {
            return Ok(false);
        }
        let revision = self.revision + 1;
        let snapshot = LocalLightProviderSnapshot::new(PROVIDER, revision, sources.clone())
            .map_err(|e| anyhow::anyhow!("night flower snapshot: {e:?}"))?;
        registry
            .reconcile(snapshot)
            .map_err(|e| anyhow::anyhow!("night flower lights: {e:?}"))?;
        self.sources = sources;
        self.revision = revision;
        Ok(true)
    }
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
    pub(super) fn update_night_flower_lights(&mut self, sun_altitude: f32) -> Result<()> {
        let now = self.time_info.time_since_start_duration().as_secs_f64();
        if now < self.night_garden.next_update {
            return Ok(());
        }
        // Quantization avoids continuously restarting GI as the world clock moves.
        // The visible petal emission remains smooth in the shader.
        let enabled = self.render_flags.enable_flora
            && self.terrain_persistence.allows_world_updates()
            && std::env::var_os("RE_FLORA_NIGHT_FLOWER_LIGHTS_OFF").is_none();
        let factor = if enabled {
            (night_factor(sun_altitude) * 8.).round() / 8.
        } else {
            0.
        };
        let sources = flower_sources(
            self.surface_builder.authored_flora_base_positions(),
            self.tracer.camera_position(),
            factor,
        );
        if self.night_garden.publish(&mut self.local_lights, sources)? {
            log::info!(
                "[NIGHT_FLOWERS] clusters={} budget={} night={} revision={} direct_and_ddgi=true",
                self.night_garden.sources.len(),
                MAX_FLOWER_LIGHTS,
                factor,
                self.night_garden.revision
            );
        }
        self.night_garden.next_update = now + 0.5;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn night_glow_is_zero_by_day_and_smooth_at_twilight() {
        assert_eq!(night_factor(1.), 0.);
        assert_eq!(night_factor(0.), 0.);
        assert_eq!(night_factor(-0.05), 0.);
        assert!((night_factor(-0.225) - 0.5).abs() < 1e-6);
        assert_eq!(night_factor(-0.4), 1.);
        assert_eq!(night_factor(-1.), 1.);
    }
    #[test]
    fn only_nearby_blue_flowers_emit_and_clusters_are_bounded_and_stable() {
        let mut roots: Vec<_> = (0..8)
            .map(|i| (blue_species(), UVec3::new(i * 64, 32, 64)))
            .collect();
        roots.push((0, UVec3::ZERO));
        roots.push((blue_species(), UVec3::splat(8192)));
        let lights = flower_sources(roots.iter().copied(), Vec3::ZERO, 1.);
        assert_eq!(lights.len(), MAX_FLOWER_LIGHTS);
        roots.reverse();
        assert_eq!(lights, flower_sources(roots.into_iter(), Vec3::ZERO, 1.));
        assert!(flower_sources(
            [(0, UVec3::ZERO), (blue_species(), UVec3::splat(8192))].into_iter(),
            Vec3::ZERO,
            1.
        )
        .is_empty());
        assert!(
            flower_sources([(blue_species(), UVec3::ZERO)].into_iter(), Vec3::ZERO, 0.).is_empty()
        );
    }
    #[test]
    fn unchanged_flowers_do_not_restart_gi_and_day_or_removal_retires_only_our_lights() {
        let mut registry = LocalLightRegistry::default();
        let other = registry.add(LocalLight::Point(
            PointLight::new(Vec3::ZERO, Vec3::ONE, 1., 0.1, 1.).unwrap(),
        ));
        let mut garden = NightGarden::default();
        let sources = flower_sources([(blue_species(), UVec3::ZERO)].into_iter(), Vec3::ZERO, 1.);
        assert!(garden.publish(&mut registry, sources.clone()).unwrap());
        let revision = registry.snapshot().source_revision();
        assert!(!garden.publish(&mut registry, sources).unwrap());
        assert_eq!(registry.snapshot().source_revision(), revision);
        assert!(garden.publish(&mut registry, Vec::new()).unwrap());
        assert_eq!(registry.snapshot().lights().len(), 1);
        assert_eq!(registry.snapshot().lights()[0].id(), other);
    }
}
