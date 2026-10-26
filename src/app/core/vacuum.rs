//! Central-aim batch collection. World ownership transfers once, then cosmetic
//! flyback runs independently; interrupted animation can never lose/duplicate goods.
use super::{garden_economy::HarvestKey, player_tools::PlayerTool, App};
use crate::{
    builder::AuthoredFloraInstance,
    particles::{MotionMode, ParticleHandle, ParticleSpawn},
    tracer::TerrainRayQuery,
};
use anyhow::Result;
use glam::{Vec2, Vec3, Vec4};

mod review;
const REACH: f32 = 1.25;
const HALF_ANGLE_TANGENT: f32 = 0.22;
const MAX_RADIUS: f32 = 0.16;
const PULSE_SECONDS: f32 = 0.12;
const BATCH: usize = 16;

#[derive(Clone, Copy)]
pub(super) struct Beam {
    pub origin: Vec3,
    pub direction: Vec3,
}
impl Beam {
    pub fn contains(self, p: Vec3, padding: f32) -> bool {
        if !p.is_finite() || !self.origin.is_finite() || !self.direction.is_normalized() {
            return false;
        }
        let offset = p - self.origin;
        let depth = offset.dot(self.direction);
        depth > 0.
            && depth <= REACH + padding
            && (offset - self.direction * depth).length()
                <= (depth * HALF_ANGLE_TANGENT).min(MAX_RADIUS) + padding
    }
}
#[derive(Clone, Copy)]
enum Item {
    Apple(HarvestKey),
    Plant(AuthoredFloraInstance),
}
struct Mote {
    handle: ParticleHandle,
    age: f32,
    start: Vec3,
    phase: f32,
}
#[derive(Default)]
pub(super) struct Vacuum {
    cooldown: f32,
    motes: Vec<Mote>,
    review: u32,
}
impl Vacuum {
    pub fn begin(&mut self) {
        self.cooldown = 0.;
    }
}

impl App {
    pub(super) fn update_vacuum(&mut self, dt: f32) -> Result<()> {
        self.review_vacuum()
            .expect("native vacuum acceptance fixture");
        let front = self.tracer.camera_front().normalize_or(Vec3::NEG_Z);
        let target = self.tracer.camera_position() + front * 0.025;
        let side = front.cross(Vec3::Y).normalize_or(Vec3::X);
        self.vacuum.motes.retain_mut(|m| {
            m.age += dt.max(0.);
            if m.age >= 0.55 || !self.particle_system.is_alive_handle(m.handle) {
                self.particle_system.despawn(m.handle);
                return false;
            }
            let t = (m.age / 0.55).clamp(0., 1.);
            let ease = t * t;
            let swirl = (side * (t * 12. + m.phase).sin() + Vec3::Y * (t * 12. + m.phase).cos())
                * (0.012 * (std::f32::consts::PI * t).sin());
            self.particle_system
                .set_position(m.handle, m.start.lerp(target, ease) + swirl);
            self.particle_system
                .set_size(m.handle, 0.004 * (1. - 0.8 * ease));
            true
        });
        if self.player_tools.selected_tool() != PlayerTool::Vacuum {
            return Ok(());
        }
        if !self.terrain_edit_pointer_available()
            || !self.terrain_persistence.allows_world_updates()
        {
            self.player_tools.cancel_continuous_hold();
            return Ok(());
        }
        if !self.player_tools.vacuum_held() {
            self.vacuum.cooldown = 0.;
            return Ok(());
        }
        self.vacuum.cooldown -= dt.max(0.);
        if self.vacuum.cooldown > 0. {
            return Ok(());
        }
        self.vacuum.cooldown = PULSE_SECONDS; // no catch-up burst after a long frame
                                              // Deliberately independent of the orbit cursor and any pending hover ticket.
        let extent = self.window_state.window_extent();
        let Some((origin, direction)) = self.tracer.camera_ray_from_screen_position(
            Vec2::new(extent.width as f32, extent.height as f32) * 0.5,
            extent,
        ) else {
            return Ok(());
        };
        self.collect_vacuum_beam(Beam {
            origin,
            direction: direction.normalize_or_zero(),
        })?;
        Ok(())
    }

    fn collect_vacuum_beam(&mut self, beam: Beam) -> Result<usize> {
        let mut candidates: Vec<_> = self
            .terrain_physics
            .vacuum_apples(beam, &self.tracer)?
            .into_iter()
            .map(|(key, p)| (Item::Apple(key), p))
            .collect();
        for plant in self.surface_builder.authored_plants() {
            // Ground plants are collected as reusable seedlings. Grass never enters
            // this owner registry, and collect_plant also enforces the catalog policy.
            let p = (plant.base_world_vox.as_vec3() + Vec3::splat(0.5)) / 256. + Vec3::Y * 0.008;
            if beam.contains(p, 0.) {
                candidates.push((Item::Plant(plant), p));
            }
        }
        candidates.sort_by(|a, b| {
            a.1.distance_squared(beam.origin)
                .total_cmp(&b.1.distance_squared(beam.origin))
        });
        candidates.truncate(64);
        let rays: Vec<_> = candidates
            .iter()
            .map(|(_, p)| TerrainRayQuery {
                origin: beam.origin,
                direction: (*p - beam.origin).normalize_or_zero(),
            })
            .collect();
        // One native scene query batch, including posed wood, terrain and fixed models.
        let hits = self.tracer.query_terrain_rays_batch_with_validity(&rays)?;
        anyhow::ensure!(
            hits.len() == candidates.len(),
            "incomplete vacuum visibility results"
        );
        let mut plants = Vec::new();
        let mut apples = 0;
        let mut count = 0;
        let mut failure = None;
        for ((item, position), hit) in candidates.into_iter().zip(hits) {
            if count == BATCH {
                break;
            }
            if hit.is_valid
                && hit.position.distance(beam.origin) + 0.002 < position.distance(beam.origin)
            {
                continue;
            }
            let result = match item {
                Item::Apple(key) => self
                    .garden_economy
                    .collect(key, || self.terrain_physics.take_harvest_apple(key)),
                Item::Plant(plant) => {
                    self.garden_economy.collect_plant(plant.species_index, || {
                        self.surface_builder.take_authored_plant(plant)
                    })
                }
            };
            match result {
                Ok(true) => {}
                Ok(false) => continue,
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
            let color = match item {
                Item::Apple(_) => {
                    apples += 1;
                    Vec4::new(0.8, 0.09, 0.025, 1.)
                }
                Item::Plant(plant) => {
                    plants.push(plant);
                    Vec4::new(0.28, 0.7, 0.12, 1.)
                }
            };
            for i in 0..3 {
                if self.vacuum.motes.len() >= 256 {
                    break;
                }
                let start = position + Vec3::Y * i as f32 * 0.003;
                if let Some(handle) = self.particle_system.spawn(ParticleSpawn {
                    position: start,
                    color,
                    size: 0.004,
                    lifetime: 0.8,
                    motion_mode: MotionMode::GuidedFlight,
                    wind_factor: 0.,
                    gravity_factor: 0.,
                    despawn_below_ground: false,
                    ..Default::default()
                }) {
                    self.vacuum.motes.push(Mote {
                        handle,
                        start,
                        age: 0.,
                        phase: i as f32 * 2.1,
                    });
                }
            }
            count += 1;
        }
        // Always publish the already-committed portion, even if later stock is full.
        if !plants.is_empty() {
            self.surface_builder.publish_collected_plants(&plants)?;
            self.tracer.invalidate_vegetation_response_history();
        }
        if apples > 0 {
            self.terrain_physics
                .publish_harvest_rendering(&mut self.tracer)?;
            let trees = self.terrain_physics.take_attached_fruit_refresh_trees();
            self.refresh_attached_tree_fruits(&trees)?;
        }
        if count > 0 {
            self.garden_feedback = Some(format!(
                "Collected {count} · {apples} apples, {} plants",
                plants.len()
            ));
            log::info!(
                "[VACUUM] collected={count} apples={apples} plants={} ownership=committed",
                plants.len()
            );
        }
        if let Some(e) = failure {
            return Err(e);
        }
        Ok(count)
    }

    pub(super) fn draw_vacuum_reticle(ctx: &egui::Context, visible: bool, active: bool) {
        if !visible {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            "vacuum_reticle".into(),
        ));
        let p = ctx.content_rect().center();
        let color = if active {
            super::ui_style::GOLD_ACCENT
        } else {
            super::ui_style::TEXT_COLOR
        };
        painter.circle_stroke(
            p,
            if active { 12. } else { 8. },
            egui::Stroke::new(1.5, color),
        );
        painter.circle_filled(p, 1.5, color);
        painter.text(
            p + egui::vec2(0., 24.),
            egui::Align2::CENTER_TOP,
            "Hold left mouse to collect",
            egui::TextStyle::Small.resolve(&ctx.global_style()),
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vacuum_cone_rejects_backwards_distant_off_axis_and_invalid_points() {
        let beam = Beam {
            origin: Vec3::ZERO,
            direction: Vec3::Z,
        };
        assert!(beam.contains(Vec3::new(0.06, 0., 0.5), 0.));
        for p in [
            Vec3::NEG_Z,
            Vec3::Z * 1.3,
            Vec3::new(0.2, 0., 0.5),
            Vec3::NAN,
            Vec3::ZERO,
        ] {
            assert!(!beam.contains(p, 0.));
        }
        assert!(!Beam {
            direction: Vec3::ZERO,
            ..beam
        }
        .contains(Vec3::Z, 0.));
    }
}
