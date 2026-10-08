//! Error types for PetalSonic

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PetalSonicError {
    #[error("Invalid configuration for {field}: {reason}")]
    InvalidConfiguration { field: &'static str, reason: String },

    #[error("{resource} capacity exceeded (limit {limit})")]
    CapacityExceeded {
        resource: &'static str,
        limit: usize,
    },

    #[error("Audio control queue is full; retry on a later game update")]
    QueuePressure,

    #[error("Emitter handle is stale")]
    StaleEmitter,

    #[error("Bus handle is stale or belongs to another world")]
    StaleBus,

    #[error("Playback control is stale")]
    StalePlayback,

    #[error("Audio runtime is closed")]
    RuntimeClosed,

    #[error("Audio runtime has permanently failed")]
    RuntimeFailed,

    #[error("Audio device error: {0}")]
    AudioDevice(String),

    #[error("Audio format error: {0}")]
    AudioFormat(String),

    #[error("Required audio backend {backend} is unavailable: {reason}")]
    BackendUnavailable {
        backend: &'static str,
        reason: String,
    },

    #[error("Permanent output-device failure: {0}")]
    PermanentDeviceFailure(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Audio loading error: {0}")]
    AudioLoading(String),

    #[error("Ring buffer error: {0}")]
    RingBuffer(String),

    #[error("Engine error: {0}")]
    Engine(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Spatialization error: {0}")]
    Spatialization(String),

    #[error("Spatial audio error: {0}")]
    SpatialAudio(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, PetalSonicError>;
