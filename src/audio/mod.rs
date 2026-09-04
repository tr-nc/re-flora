mod audio_clip_cache;

mod audio_telemetry_router;
pub(crate) use audio_telemetry_router::{
    AudioTelemetryObservations, AudioTelemetryRouter, CanopyAcousticObservation,
    LocalFootstepTelemetryObservations,
};

mod canopy_acoustics;
pub use canopy_acoustics::{
    CanopyAcousticDescriptor, CanopyAcousticSampleId, CanopyAcousticSampleProvenance,
};

mod canopy_audio_diagnostics;
pub use canopy_audio_diagnostics::{
    canopy_audio_diagnostic_pose, CanopyAudioDiagnosticPose, CanopyAudioTrajectoryPhase,
    LegacyBranchEndpointLayout,
};

mod canopy_audio_lifecycle;
pub use canopy_audio_lifecycle::{
    ActiveCanopyAcousticGeneration, CanopyAudioGenerationKey, CanopyAudioLifecycle,
    CanopyAudioLifecycleSnapshot, CanopyAudioSourceKey, CanopyTreeLifecycleDiagnostics,
};

mod canopy_distributed_emitter_adapter;
use canopy_distributed_emitter_adapter::CanopyDistributedEmitterAdapter;

mod canopy_audio_telemetry;
pub use canopy_audio_telemetry::{
    CanopyAcousticSolveStatus, CanopyAudioSampleTelemetry, CanopyAudioTelemetry,
    CanopyAudioTelemetryDiagnostics, CanopyAudioTelemetrySnapshot, CanopyAudioTreeTelemetry,
    CanopyExtentAcousticObservation, CanopyOcclusionClassification, CanopyRouteAcousticObservation,
    CanopySampleAcousticObservation,
};

mod local_player_footsteps;
pub(super) use local_player_footsteps::{local_footstep_correlation_id, LocalPlayerFootstepAudio};

mod spatial_frame;
pub(crate) use spatial_frame::{SpatialFrame, SpatialFrameFacts};

mod spatial_sound_manager;
pub use spatial_sound_manager::SpatialSoundManager;

mod tree_audio_source;
pub use tree_audio_source::CanopyAudioVoice;

mod tree_rustle;
pub use tree_rustle::{TreeRustleControl, TreeRustleFactory, TreeRustleParams};

mod tree_audio_manager;
pub use tree_audio_manager::TreeAudioManager;
pub(crate) use tree_audio_manager::TreeAudioPublicationCheckpoint;
