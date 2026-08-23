mod audio_clip_cache;

mod canopy_acoustics;
pub use canopy_acoustics::{
    CanopyAcousticDescriptor, CanopyAcousticSample, CanopyAcousticSampleId,
    CanopyAcousticSampleProvenance,
};

mod canopy_audio_lifecycle;
pub use canopy_audio_lifecycle::{
    ActiveCanopyAcousticSample, CanopyAudioLifecycle, CanopyAudioLifecycleSnapshot,
    CanopyAudioSourceKey, CanopyTreeLifecycleDiagnostics,
};

mod canopy_point_emitter_adapter;
use canopy_point_emitter_adapter::CanopyPointEmitterAdapter;

mod canopy_audio_telemetry;
pub use canopy_audio_telemetry::{
    CanopyAudioSampleTelemetry, CanopyAudioTelemetry, CanopyAudioTelemetrySnapshot,
    CanopyAudioTreeTelemetry, CanopyDirectPathTelemetry,
};

mod spatial_sound_manager;
pub use spatial_sound_manager::SpatialSoundManager;

mod tree_audio_source;
pub use tree_audio_source::TreeAudioSource;

mod tree_rustle;
pub use tree_rustle::{TreeRustleControl, TreeRustleFactory, TreeRustleParams};

mod tree_audio_manager;
pub use tree_audio_manager::TreeAudioManager;
