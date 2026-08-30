use super::CanopyAudioDiagnosticCounters;
use crate::audio::{
    canopy_audio_diagnostic_pose, CanopyAudioDiagnosticPose, CanopyAudioTelemetrySnapshot,
    CanopyAudioTrajectoryPhase,
};
use glam::Vec3;

const ACOUSTIC_SETTLE_SECONDS: f32 = 0.1;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CanopyAudioAcousticBudget {
    Default,
    Constrained,
}

#[derive(Debug)]
pub(super) enum CanopyAudioVegetationStartup {
    DiagnosticLayout,
    BudgetStressLayout,
}

impl CanopyAudioVegetationStartup {
    pub(super) fn plants_budget_stress_trees(&self) -> bool {
        matches!(self, Self::BudgetStressLayout)
    }
}

#[derive(Debug)]
pub(super) struct CanopyAudioStartupPlan {
    acoustic_budget: CanopyAudioAcousticBudget,
    vegetation: CanopyAudioVegetationStartup,
}

impl CanopyAudioStartupPlan {
    pub(super) fn diagnostic(budget_stress: bool) -> Self {
        Self {
            acoustic_budget: if budget_stress {
                CanopyAudioAcousticBudget::Constrained
            } else {
                CanopyAudioAcousticBudget::Default
            },
            vegetation: if budget_stress {
                CanopyAudioVegetationStartup::BudgetStressLayout
            } else {
                CanopyAudioVegetationStartup::DiagnosticLayout
            },
        }
    }

    pub(super) fn into_effects(self) -> (CanopyAudioAcousticBudget, CanopyAudioVegetationStartup) {
        (self.acoustic_budget, self.vegetation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CanopyAudioWindPolicy {
    Configured,
    Diagnostic,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CanopyAudioPhaseLog {
    pub(super) elapsed_seconds: f32,
    pub(super) phase: CanopyAudioTrajectoryPhase,
    pub(super) pose: CanopyAudioDiagnosticPose,
}

#[derive(Debug)]
pub(super) enum CanopyAudioFrameCommand {
    Standard,
    Diagnostic {
        permit_revision: u64,
        tree_origin_world: Vec3,
        time_seconds: f32,
        pose: CanopyAudioDiagnosticPose,
        active: Option<CanopyAudioPhaseLog>,
        permit_previous_phase: Option<CanopyAudioTrajectoryPhase>,
    },
}

impl CanopyAudioFrameCommand {
    pub(super) fn wind_policy(&self) -> CanopyAudioWindPolicy {
        match self {
            Self::Standard => CanopyAudioWindPolicy::Configured,
            Self::Diagnostic { .. } => CanopyAudioWindPolicy::Diagnostic,
        }
    }

    pub(super) fn pose(&self) -> Option<CanopyAudioDiagnosticPose> {
        match self {
            Self::Standard => None,
            Self::Diagnostic { pose, .. } => Some(*pose),
        }
    }

    #[cfg(test)]
    pub(super) fn phase_log(&self) -> Option<CanopyAudioPhaseLog> {
        match self {
            Self::Standard => None,
            Self::Diagnostic { active, .. } => *active,
        }
    }
}

#[derive(Debug)]
pub(super) enum CanopyAudioFrameEffect {
    Rejected,
    Applied {
        start_observation: Option<CanopyAudioStartObservation>,
        telemetry_counters: Option<CanopyAudioDiagnosticCounters>,
    },
}

pub(super) struct CanopyAudioFrameReceipt {
    started: bool,
    phase_log: Option<CanopyAudioPhaseLog>,
    telemetry: Option<CanopyAudioTelemetry>,
}

impl CanopyAudioFrameReceipt {
    pub(super) fn standard(effect: CanopyAudioFrameEffect) -> Self {
        let telemetry = match effect {
            CanopyAudioFrameEffect::Rejected => None,
            CanopyAudioFrameEffect::Applied {
                telemetry_counters, ..
            } => telemetry_counters.map(|counters| CanopyAudioTelemetry {
                marker: AudioTelemetryMarker::NotDiagnostic,
                counters,
            }),
        };
        Self {
            started: false,
            phase_log: None,
            telemetry,
        }
    }

    pub(super) fn started(&self) -> bool {
        self.started
    }

    pub(super) fn phase_log(&self) -> Option<CanopyAudioPhaseLog> {
        self.phase_log
    }

    pub(super) fn telemetry(&self) -> Option<&CanopyAudioTelemetry> {
        self.telemetry.as_ref()
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CanopyAudioStartObservation {
    time_seconds: f32,
    response_matches_published_scene: bool,
    render_rejected_response_count: u64,
    counters: CanopyAudioDiagnosticCounters,
}

impl CanopyAudioStartObservation {
    pub(super) fn new(
        time_seconds: f32,
        response_matches_published_scene: bool,
        snapshot: &CanopyAudioTelemetrySnapshot,
    ) -> Self {
        Self {
            time_seconds,
            response_matches_published_scene,
            render_rejected_response_count: snapshot.petal_render_rejected_response_count,
            counters: CanopyAudioDiagnosticCounters::from_snapshot(snapshot),
        }
    }
}

pub(super) enum AudioTelemetryMarker {
    NotDiagnostic,
    WaitingForStart,
    Active(f32, CanopyAudioTrajectoryPhase),
}

pub(super) struct CanopyAudioTelemetry {
    pub(super) marker: AudioTelemetryMarker,
    pub(super) counters: CanopyAudioDiagnosticCounters,
}

pub(super) struct CanopyAudioDiagnosticRuntime {
    start_time_seconds: Option<f32>,
    previous_phase: Option<CanopyAudioTrajectoryPhase>,
    counter_baseline: Option<CanopyAudioDiagnosticCounters>,
    acoustic_ready_since_seconds: Option<f32>,
    last_render_rejected_response_count: Option<u64>,
    transaction_revision: u64,
}

impl CanopyAudioDiagnosticRuntime {
    pub(super) fn new() -> Self {
        Self {
            start_time_seconds: None,
            previous_phase: None,
            counter_baseline: None,
            acoustic_ready_since_seconds: None,
            last_render_rejected_response_count: None,
            transaction_revision: 0,
        }
    }

    pub(super) fn begin_frame(
        &self,
        tree_origin_world: Vec3,
        time_seconds: f32,
    ) -> CanopyAudioFrameCommand {
        let (pose, active) = self.start_time_seconds.map_or_else(
            || (canopy_audio_diagnostic_pose(tree_origin_world, 0.0), None),
            |start_time_seconds| {
                let elapsed_seconds = (time_seconds - start_time_seconds).max(0.0);
                let pose = canopy_audio_diagnostic_pose(tree_origin_world, elapsed_seconds);
                let active =
                    (self.previous_phase != Some(pose.phase)).then_some(CanopyAudioPhaseLog {
                        elapsed_seconds,
                        phase: pose.phase,
                        pose,
                    });
                (pose, active)
            },
        );
        CanopyAudioFrameCommand::Diagnostic {
            permit_revision: self.transaction_revision,
            tree_origin_world,
            time_seconds,
            pose,
            active,
            permit_previous_phase: self.previous_phase,
        }
    }

    pub(super) fn finish_frame(
        &mut self,
        command: CanopyAudioFrameCommand,
        effect: CanopyAudioFrameEffect,
    ) -> anyhow::Result<CanopyAudioFrameReceipt> {
        let CanopyAudioFrameCommand::Diagnostic {
            permit_revision,
            tree_origin_world,
            time_seconds,
            pose,
            active,
            permit_previous_phase,
        } = command
        else {
            anyhow::bail!("active canopy diagnostic received a standard frame command")
        };
        anyhow::ensure!(
            self.transaction_revision == permit_revision
                && self.previous_phase == permit_previous_phase,
            "stale canopy audio frame command"
        );
        let CanopyAudioFrameEffect::Applied {
            start_observation,
            telemetry_counters,
        } = effect
        else {
            return Ok(CanopyAudioFrameReceipt {
                started: false,
                phase_log: None,
                telemetry: None,
            });
        };

        self.previous_phase = self.start_time_seconds.map(|_| pose.phase);
        let started = start_observation
            .map(|observation| self.apply_start_observation(observation))
            .transpose()?
            .unwrap_or(false);
        let telemetry = telemetry_counters.map(|counters| {
            let counters = self
                .counter_baseline
                .map_or(counters, |baseline| counters.activity_since(baseline));
            let marker = self.start_time_seconds.map_or(
                AudioTelemetryMarker::WaitingForStart,
                |start_time_seconds| {
                    let elapsed_seconds = (time_seconds - start_time_seconds).max(0.0);
                    let phase =
                        canopy_audio_diagnostic_pose(tree_origin_world, elapsed_seconds).phase;
                    AudioTelemetryMarker::Active(elapsed_seconds, phase)
                },
            );
            CanopyAudioTelemetry { marker, counters }
        });
        self.transaction_revision = self.transaction_revision.wrapping_add(1);
        Ok(CanopyAudioFrameReceipt {
            started,
            phase_log: active,
            telemetry,
        })
    }

    fn apply_start_observation(
        &mut self,
        observation: CanopyAudioStartObservation,
    ) -> anyhow::Result<bool> {
        if self.start_time_seconds.is_some() {
            return Ok(false);
        }
        if !observation.response_matches_published_scene {
            self.acoustic_ready_since_seconds = None;
            self.last_render_rejected_response_count = None;
            return Ok(false);
        }
        if self.last_render_rejected_response_count
            != Some(observation.render_rejected_response_count)
        {
            self.acoustic_ready_since_seconds = Some(observation.time_seconds);
            self.last_render_rejected_response_count =
                Some(observation.render_rejected_response_count);
            return Ok(false);
        }
        let ready_since_seconds = self
            .acoustic_ready_since_seconds
            .unwrap_or(observation.time_seconds);
        self.acoustic_ready_since_seconds = Some(ready_since_seconds);
        let settled = observation.time_seconds >= ready_since_seconds
            && observation.time_seconds - ready_since_seconds >= ACOUSTIC_SETTLE_SECONDS;
        if settled {
            self.start_time_seconds = Some(observation.time_seconds);
            self.counter_baseline = Some(observation.counters);
        }
        Ok(settled)
    }
}
