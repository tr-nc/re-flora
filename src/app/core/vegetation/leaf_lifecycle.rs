//! The real-leaf experiment's single publication owner. Rendering, flight and
//! ecology observe the same committed socket state; App does not manage leaf timers.
use super::*;
use crate::leaf_lifecycle::{LeafCanopy, LeafLifecycleSettings};
mod validation;

#[derive(Clone, Default)]
pub(super) struct LeafLifecycleRuntime {
    pub enabled: bool,
    pub time: f64,
    settings: LeafLifecycleSettings,
    next_report: f64,
    detached: usize,
    validation: Option<validation::Validation>,
}

impl LeafLifecycleRuntime {
    pub fn new() -> Self {
        Self {
            validation: validation::Validation::from_env(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct CanopyState {
    pub canopy: LeafCanopy,
    pub growth: Vec<f32>,
    pub live_slots: Vec<u32>,
    pub area_fraction: f32,
}

impl CanopyState {
    pub fn new(tree: u32, record: &TreeRecord, time: f64) -> Self {
        let positions = &record.leaf_render_positions;
        Self {
            canopy: LeafCanopy::new(
                tree,
                record.canopy_acoustic_descriptor.generation(),
                positions,
                time,
            ),
            growth: vec![1.0; positions.len()],
            live_slots: (0..positions.len() as u32).collect(),
            area_fraction: 1.0,
        }
    }

    fn publish(&mut self, now: f64, settings: LeafLifecycleSettings) {
        self.canopy.write_growth(now, settings, &mut self.growth);
        self.area_fraction =
            self.growth.iter().map(|g| g * g).sum::<f32>() / self.growth.len().max(1) as f32;
        self.live_slots.clear();
        self.live_slots.extend(
            self.growth
                .iter()
                .enumerate()
                .filter_map(|(i, &growth)| (growth >= 0.5).then_some(i as u32)),
        );
    }
}

impl App {
    pub(in crate::app::core) fn leaf_lifecycle_validation_active(&self) -> bool {
        self.trees.leaf_lifecycle.validation.is_some()
    }

    /// The fixture drives the same field for the detachment rule and attached GPU response.
    pub(in crate::app::core) fn leaf_validation_wind(
        &self,
    ) -> Option<crate::wind_field::WindFieldFrame> {
        self.trees
            .leaf_lifecycle
            .validation
            .as_ref()
            .map(validation::Validation::wind)
    }

    /// Mode changes run even when particle drawing is disabled. One setting owns
    /// both the old emitter and the experiment; no hidden second enabled flag.
    pub(in crate::app::core) fn sync_leaf_lifecycle_mode(&mut self) {
        validation::drive(self);
        let a = &self.debug_settings.adjustables;
        let enabled = a.real_leaf_lifecycle.value;
        let settings = LeafLifecycleSettings {
            strength: a.leaf_connection_strength.value,
            half_life_seconds: a.leaf_connection_half_life.value,
            recovery_seconds: a.leaf_regrowth_delay.value,
            growth_seconds: a.leaf_regrowth_duration.value,
        };
        if enabled != self.trees.leaf_lifecycle.enabled {
            self.particle_system.clear_leaves();
            let validation = self.trees.leaf_lifecycle.validation.take();
            self.trees.leaf_lifecycle = LeafLifecycleRuntime {
                enabled,
                settings,
                validation,
                ..Default::default()
            };
            for (&tree_id, record) in &mut self.trees.records {
                record.leaf_lifecycle = enabled.then(|| CanopyState::new(tree_id, record, 0.0));
                self.tree_audio_manager.set_leaf_coverage(tree_id, 1.0);
                if let Some(tree) = self
                    .surface_builder
                    .resources
                    .instances
                    .leaves_instances
                    .get_mut(&tree_id)
                {
                    tree.resources
                        .leaf_state
                        .set(&vec![1.0; record.leaf_render_positions.len().max(1)]);
                }
            }
            self.ecology.clear();
            self.tracer.invalidate_local_direct_sun_shadow_histories();
            log::info!("[LEAF_LIFECYCLE][MODE] enabled={enabled} sockets=reset leaf_particles=cleared other_particles=preserved storage=session_only");
        }
        self.trees.leaf_lifecycle.settings = settings;
    }

    /// Run after existing particles have advanced and before snapshots publish,
    /// so a newborn falling leaf's first frame is exactly its source pose.
    pub(in crate::app::core) fn advance_leaf_lifecycle(&mut self, dt: f32) -> Result<()> {
        if !self.trees.leaf_lifecycle.enabled
            || !self.render_flags.enable_leaves
            || !self.terrain_persistence.allows_world_updates()
            || !self.launch_owners.allows_ambient_particle_emitters()
        {
            return Ok(());
        }
        let started = Instant::now();
        let runtime = &mut self.trees.leaf_lifecycle;
        runtime.time += f64::from(if runtime.validation.is_some() {
            0.1
        } else {
            dt.max(0.0)
        });
        let now = runtime.time;
        let settings = runtime.settings;
        let wind = self
            .leaf_validation_wind()
            .unwrap_or_else(|| self.wind_prototype.field.frame());
        let mut requests = Vec::new();
        for record in self.trees.records.values() {
            if let Some(state) = &record.leaf_lifecycle {
                requests.extend(
                    state
                        .canopy
                        .plan(now, settings, &wind)
                        .into_iter()
                        .map(|event| {
                            (
                                event,
                                record.leaf_render_local_positions[event.id.socket as usize],
                            )
                        }),
                );
            }
        }
        let plan_us = started.elapsed().as_micros();
        let a = &self.debug_settings.adjustables;
        let rgb = |c: egui::Color32| Vec3::new(c.r() as f32, c.g() as f32, c.b() as f32) / 255.0;
        let colors = crate::tracer::solid_flora_height_color_tables(
            rgb(a.leaves_bottom_color.value),
            rgb(a.leaves_tip_color.value),
        );
        // Reserve the entire requested batch before GPU handoff or socket changes.
        // No canopy floor, rate limiter, delayed queue or silently failed spawn.
        self.particle_system.reserve_for_batch(requests.len());
        let handoffs = self.tracer.gather_leaf_handoffs(&requests, colors)?;
        let detached = handoffs.len();
        for (event, release) in handoffs {
            let handle = self
                .particle_system
                .spawn_attached_leaf(release)
                .expect("complete handoff batch was reserved before publication");
            debug_assert_eq!(self.particle_system.leaf_origin(handle), Some(event.id));
            let state = self
                .trees
                .records
                .get_mut(&event.id.tree)
                .unwrap()
                .leaf_lifecycle
                .as_mut()
                .unwrap();
            assert!(
                state.canopy.commit(event, now, settings),
                "fresh handoff must commit exactly once"
            );
        }
        let mut leaves = 0;
        let mut empty = 0;
        let mut growing = 0;
        for (&tree_id, record) in &mut self.trees.records {
            if let Some(state) = &mut record.leaf_lifecycle {
                state.publish(now, settings);
                self.tree_audio_manager
                    .set_leaf_coverage(tree_id, state.area_fraction);
                leaves += state.canopy.len();
                empty += state.growth.iter().filter(|&&g| g == 0.0).count();
                growing += state.growth.iter().filter(|&&g| g > 0.0 && g < 1.0).count();
                if let Some(tree) = self
                    .surface_builder
                    .resources
                    .instances
                    .leaves_instances
                    .get_mut(&tree_id)
                {
                    if !state.growth.is_empty() {
                        tree.resources.leaf_state.set(&state.growth);
                    }
                }
            }
        }
        if detached > 0 {
            self.tracer.invalidate_local_direct_sun_shadow_histories();
        }
        let runtime = &mut self.trees.leaf_lifecycle;
        runtime.detached += detached;
        if now >= runtime.next_report || detached > 0 {
            log::info!("[LEAF_LIFECYCLE][FRAME] time={now:.3} sockets={leaves} empty={empty} growing={growing} detached={detached} total_detached={} plan_us={plan_us} total_us={} particles={} capacity={}",
                runtime.detached, started.elapsed().as_micros(), self.particle_system.alive_count(), self.particle_system.capacity());
            runtime.next_report = now + 2.0;
        }
        Ok(())
    }
}
