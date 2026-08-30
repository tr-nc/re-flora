use super::CanopyAudioDiagnosticCounters;
use crate::audio::{
    canopy_audio_diagnostic_pose, CanopyAudioDiagnosticPose, CanopyAudioTelemetrySnapshot,
    CanopyAudioTrajectoryPhase,
};
use glam::Vec3;

const ACOUSTIC_SETTLE_SECONDS: f32 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CanopyAudioSetup {
    Disabled,
    Diagnostic { budget_stress: bool },
}

#[derive(Clone, Copy)]
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

#[derive(Clone, Copy)]
pub(super) struct ReadinessPermit {
    ready_since_seconds: Option<f32>,
    render_rejected_response_count: Option<u64>,
}

#[derive(Clone, Copy)]
pub(super) struct StartCommit {
    time_seconds: f32,
    counters: CanopyAudioDiagnosticCounters,
}

pub(super) enum CanopyAudioStartTxn {
    Inactive,
    AlreadyStarted,
    Observation {
        permit: ReadinessPermit,
        next: ReadinessPermit,
        start: Option<StartCommit>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CanopyAudioStartResult {
    Observed,
    Rejected,
}

pub(super) enum CanopyAudioTrajectoryTxn {
    Inactive,
    Waiting {
        pose: CanopyAudioDiagnosticPose,
    },
    Active {
        pose: CanopyAudioDiagnosticPose,
        elapsed_seconds: f32,
        phase_changed: bool,
        permit_previous_phase: Option<CanopyAudioTrajectoryPhase>,
    },
}

impl CanopyAudioTrajectoryTxn {
    pub(super) fn pose(&self) -> Option<CanopyAudioDiagnosticPose> {
        match self {
            Self::Inactive => None,
            Self::Waiting { pose } | Self::Active { pose, .. } => Some(*pose),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CanopyAudioTrajectoryResult {
    Applied,
    Rejected,
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
    budget_stress: bool,
}

impl CanopyAudioDiagnosticRuntime {
    pub(super) fn new(budget_stress: bool) -> Self {
        Self {
            start_time_seconds: None,
            previous_phase: None,
            counter_baseline: None,
            acoustic_ready_since_seconds: None,
            last_render_rejected_response_count: None,
            budget_stress,
        }
    }

    pub(super) fn setup(&self) -> CanopyAudioSetup {
        CanopyAudioSetup::Diagnostic {
            budget_stress: self.budget_stress,
        }
    }

    pub(super) fn begin_start(
        &self,
        observation: CanopyAudioStartObservation,
    ) -> CanopyAudioStartTxn {
        if self.start_time_seconds.is_some() {
            return CanopyAudioStartTxn::AlreadyStarted;
        }
        let permit = ReadinessPermit {
            ready_since_seconds: self.acoustic_ready_since_seconds,
            render_rejected_response_count: self.last_render_rejected_response_count,
        };
        let (next, start) = if !observation.response_matches_published_scene {
            (
                ReadinessPermit {
                    ready_since_seconds: None,
                    render_rejected_response_count: None,
                },
                None,
            )
        } else if permit.render_rejected_response_count
            != Some(observation.render_rejected_response_count)
        {
            (
                ReadinessPermit {
                    ready_since_seconds: Some(observation.time_seconds),
                    render_rejected_response_count: Some(
                        observation.render_rejected_response_count,
                    ),
                },
                None,
            )
        } else {
            let ready_since_seconds = permit
                .ready_since_seconds
                .unwrap_or(observation.time_seconds);
            let settled = observation.time_seconds >= ready_since_seconds
                && observation.time_seconds - ready_since_seconds >= ACOUSTIC_SETTLE_SECONDS;
            (
                ReadinessPermit {
                    ready_since_seconds: Some(ready_since_seconds),
                    render_rejected_response_count: permit.render_rejected_response_count,
                },
                settled.then_some(StartCommit {
                    time_seconds: observation.time_seconds,
                    counters: observation.counters,
                }),
            )
        };
        CanopyAudioStartTxn::Observation {
            permit,
            next,
            start,
        }
    }

    pub(super) fn finish_start(
        &mut self,
        transaction: CanopyAudioStartTxn,
        result: CanopyAudioStartResult,
    ) -> anyhow::Result<bool> {
        match transaction {
            CanopyAudioStartTxn::Inactive => {
                anyhow::bail!("active canopy diagnostic received an inactive start transaction")
            }
            CanopyAudioStartTxn::AlreadyStarted => return Ok(false),
            CanopyAudioStartTxn::Observation {
                permit,
                next,
                start,
            } => {
                anyhow::ensure!(
                    self.acoustic_ready_since_seconds == permit.ready_since_seconds
                        && self.last_render_rejected_response_count
                            == permit.render_rejected_response_count,
                    "stale canopy audio start transaction"
                );
                if result == CanopyAudioStartResult::Rejected {
                    return Ok(false);
                }
                self.acoustic_ready_since_seconds = next.ready_since_seconds;
                self.last_render_rejected_response_count = next.render_rejected_response_count;
                if let Some(start) = start {
                    self.start_time_seconds = Some(start.time_seconds);
                    self.counter_baseline = Some(start.counters);
                    return Ok(true);
                }
                Ok(false)
            }
        }
    }

    pub(super) fn begin_trajectory(
        &self,
        tree_origin_world: Vec3,
        time_seconds: f32,
    ) -> CanopyAudioTrajectoryTxn {
        let Some(start_time_seconds) = self.start_time_seconds else {
            return CanopyAudioTrajectoryTxn::Waiting {
                pose: canopy_audio_diagnostic_pose(tree_origin_world, 0.0),
            };
        };
        let elapsed_seconds = (time_seconds - start_time_seconds).max(0.0);
        let pose = canopy_audio_diagnostic_pose(tree_origin_world, elapsed_seconds);
        CanopyAudioTrajectoryTxn::Active {
            pose,
            elapsed_seconds,
            phase_changed: self.previous_phase != Some(pose.phase),
            permit_previous_phase: self.previous_phase,
        }
    }

    pub(super) fn finish_trajectory(
        &mut self,
        transaction: CanopyAudioTrajectoryTxn,
        result: CanopyAudioTrajectoryResult,
    ) -> anyhow::Result<()> {
        match transaction {
            CanopyAudioTrajectoryTxn::Inactive => anyhow::bail!(
                "active canopy diagnostic received an inactive trajectory transaction"
            ),
            CanopyAudioTrajectoryTxn::Waiting { .. } => Ok(()),
            CanopyAudioTrajectoryTxn::Active {
                pose,
                permit_previous_phase,
                ..
            } => {
                anyhow::ensure!(
                    self.previous_phase == permit_previous_phase,
                    "stale canopy audio trajectory transaction"
                );
                if result == CanopyAudioTrajectoryResult::Applied {
                    self.previous_phase = Some(pose.phase);
                }
                Ok(())
            }
        }
    }

    pub(super) fn telemetry(
        &self,
        tree_origin_world: Vec3,
        time_seconds: f32,
        snapshot: &CanopyAudioTelemetrySnapshot,
    ) -> CanopyAudioTelemetry {
        let counters = self.counter_baseline.map_or_else(
            || CanopyAudioDiagnosticCounters::from_snapshot(snapshot),
            |baseline| {
                CanopyAudioDiagnosticCounters::from_snapshot(snapshot).activity_since(baseline)
            },
        );
        let marker = self.start_time_seconds.map_or(
            AudioTelemetryMarker::WaitingForStart,
            |start_time_seconds| {
                let elapsed_seconds = (time_seconds - start_time_seconds).max(0.0);
                let phase = canopy_audio_diagnostic_pose(tree_origin_world, elapsed_seconds).phase;
                AudioTelemetryMarker::Active(elapsed_seconds, phase)
            },
        );
        CanopyAudioTelemetry { marker, counters }
    }
}
