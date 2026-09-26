//! Native response fixture. Mutations use production edit/light interfaces; observations use
//! the production DDGI consumer query before direct lighting or display transforms.
use super::{App, TerrainEdit};
use crate::lighting::{LightId, LocalLight, PointLight};
use anyhow::Result;
use glam::Vec3;
use serde_json::json;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Baseline,
    OnEditing,
    OnSettling,
    OffEditing,
    OffSettling,
    Done,
}
impl Phase {
    fn label(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::OnEditing => "on-editing",
            Self::OnSettling => "on-settling",
            Self::OffEditing => "off-editing",
            Self::OffSettling => "off-settling",
            Self::Done => "done",
        }
    }
    fn editing(self) -> bool {
        matches!(self, Self::OnEditing | Self::OffEditing)
    }
}

#[derive(Debug)]
pub(super) struct IndirectResponse {
    started: Instant,
    phase: Phase,
    light: Option<LightId>,
    pending: Option<(u32, u128, Phase)>,
    samples_in_phase: u32,
    edit_count: u32,
    continuous_edits: bool,
    last_edit: Instant,
}

impl IndirectResponse {
    pub fn new(continuous_edits: bool) -> Self {
        Self {
            started: Instant::now(),
            phase: Phase::Baseline,
            light: None,
            pending: None,
            samples_in_phase: 0,
            edit_count: 0,
            continuous_edits,
            last_edit: Instant::now(),
        }
    }
    fn emit(&self, value: serde_json::Value) {
        log::info!("[DDGI_RESPONSE] {}", value);
    }
    fn enter(&mut self, phase: Phase, app: &App) {
        self.phase = phase;
        self.samples_in_phase = 0;
        self.edit_count = 0;
        self.last_edit = Instant::now();
        self.emit(
            json!({"event":"phase", "phase":phase.label(), "ms":self.started.elapsed().as_millis(),
            "geometry_revision":app.visible_terrain_revision}),
        );
    }
    pub fn step(&mut self, app: &mut App) -> Result<bool> {
        if self.phase == Phase::Done {
            return Ok(true);
        }
        if let Some((serial, ms, phase)) = self.pending {
            if let Some(evidence) = app
                .tracer
                .ddgi_response_evidence()
                .filter(|e| e.serial == serial)
            {
                self.emit(json!({"event":"sample", "phase":phase.label(), "ms":ms, "serial":serial,
                    "ready":evidence.ready, "geometry_revision":evidence.geometry_revision,
                    "radiance_revision":evidence.radiance_revision, "field_serial":evidence.field_serial,
                    "consumer_geometry_revision":evidence.consumer_geometry_revision,
                    "consumer_radiance_revision":evidence.consumer_radiance_revision,
                    "rgb":evidence.irradiance.to_array(),
                    "weight":evidence.weight, "probes":evidence.probes}));
                self.pending = None;
                self.samples_in_phase += 1;
            }
        }
        if self.phase.editing()
            && self.edit_count < 40
            && self.last_edit.elapsed() >= Duration::from_millis(100)
        {
            let edit = if self.edit_count % 2 == 0 {
                TerrainEdit::CloseSkylight
            } else {
                TerrainEdit::ReopenSkylight
            };
            if self.continuous_edits {
                app.apply_environment_lighting_terrain_edit(edit, app.visible_terrain_revision)?;
            }
            self.edit_count += 1;
            self.last_edit = Instant::now();
            self.emit(
                json!({"event":if self.continuous_edits { "edit" } else { "static-tick" },
                "phase":self.phase.label(), "ms":self.started.elapsed().as_millis(),
                "edit":self.edit_count, "geometry_revision":app.visible_terrain_revision}),
            );
        }
        // Drain the last requested observation before crossing a phase boundary.
        if self.pending.is_none() {
            let status = app.tracer.ddgi_runtime_status();
            let settled = status.staging().is_none()
                && status.active().published_field().is_some_and(|field| {
                    field.field().geometry_revision() == app.visible_terrain_revision
                        && super::is_converged_field(field)
                });
            match self.phase {
                Phase::Baseline if settled && self.samples_in_phase >= 4 => {
                    self.light = Some(
                        app.local_lights.add(LocalLight::Point(
                            PointLight::new(
                                Vec3::new(0.65, 0.7, 1.2),
                                Vec3::new(1.0, 0.4, 0.15),
                                0.05,
                                0.01,
                                1.0,
                            )
                            .expect("valid DDGI response light"),
                        )),
                    );
                    self.enter(Phase::OnEditing, app);
                }
                Phase::OnEditing | Phase::OffEditing if self.edit_count >= 40 => {
                    self.enter(
                        if self.phase == Phase::OnEditing {
                            Phase::OnSettling
                        } else {
                            Phase::OffSettling
                        },
                        app,
                    );
                }
                Phase::OnSettling if settled && self.samples_in_phase >= 4 => {
                    app.local_lights
                        .remove(self.light.take().expect("response light is on"))
                        .expect("DDGI response light remains live");
                    self.enter(Phase::OffEditing, app);
                }
                Phase::OffSettling if settled && self.samples_in_phase >= 4 => {
                    self.enter(Phase::Done, app);
                    app.auto_exit_delay = Some(0.0);
                    return Ok(true);
                }
                _ => {}
            }
        }
        if self.pending.is_none() {
            // A fixed floor receiver inside the authored chamber, disjoint from skylight edits.
            // The sampler validates the echoed receiver; camera/exposure cannot alter this point.
            let serial = app.tracer.request_ddgi_response_sample(
                Vec3::new(0.65, 100.0 / 256.0 + 0.001, 1.2),
                Vec3::Y,
            )?;
            self.pending = Some((serial, self.started.elapsed().as_millis(), self.phase));
        }
        Ok(false)
    }
}
