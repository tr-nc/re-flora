# ForaAudio

High-level goals

- World-driven API on the main thread: you own and update a 3D world (listener + sources).
- Fixed-size audio processing thread(s) that use audionimbus (Steam Audio) for spatialization.
- Decoding with Symphonia; optional resampling on load to a world-wide sample rate.
- Playback via CPAL, with a lock-free SPSC ring buffer bridging fixed-size producer blocks to variable-size device callbacks.
- Real-time safe in the audio callback; no allocations/locks on the RT path.
- One-shot and loop sources, with automatic removal of finished one-shots via events.

Core types

- ForaWorld: main-thread controller; owns engine configuration and object registry.
- ForaAudioData: decoded PCM samples in memory (Arc), cloned cheaply.
- ForaAudioSource: a source instance referencing ForaAudioData; holds pose and playback config.
- ForaAudioListener: the single listener; holds pose.
- AudioEngine (internal): manages threads, ring buffer, spatializer, CPAL stream.

External dependencies

- symphonia (audio decode)
- cpal (audio playback)
- audionimbus (steam audio binding)
- rubato for resampling
- glam
- anyhow

Data model

- Units: meters for positions, right-handed coordinate system, quaternions for orientation.
- Engine format: f32, interleaved, channels = 2 (configurable later).
- Fixed block size: default 1024 frames; configurable at world creation.
- Global sample rate: default 48_000 Hz; fixed for the lifetime of the world.

Public API sketch
Config

```rust
pub struct ForaConfig {
    pub sample_rate: u32,    // default 48_000
    pub block_size: usize,   // default 1024
    pub channels: u16,       // default 2 (stereo)
    pub ring_blocks: usize,  // default 8 (capacity = block_size * ring_blocks)
}

impl Default for ForaConfig { /* sensible defaults */ }
```

Math and poses

```rust
#[derive(Clone, Copy, Debug)]
pub struct Vec3 { pub x: f32, pub y: f32, pub z: f32 }

#[derive(Clone, Copy, Debug)]
pub struct Quat { pub x: f32, pub y: f32, pub z: f32, pub w: f32 }

#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub position: Vec3,
    pub orientation: Quat,
    pub velocity: Vec3, // optional; can be zero if unused
}
```

ForaAudioData

```rust
use std::sync::Arc;

#[derive(Clone)]
pub struct ForaAudioData(Arc<ForaAudioDataInner>);

pub struct ForaAudioDataInner {
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: usize,       // total frames per channel
    pub samples: Arc<[f32]>, // interleaved PCM f32, length = frames * channels
    pub duration_sec: f32,
}

impl ForaAudioData {
    pub fn load_from_path<P: AsRef<std::path::Path>>(
        path: P,
        target_sample_rate: u32,
        options: LoadOptions,
    ) -> Result<ForaAudioData, ForaError>;

    pub fn channel_count(&self) -> u16;
    pub fn sample_rate(&self) -> u32;
    pub fn frames(&self) -> usize;
    pub fn duration(&self) -> f32;
}

pub struct LoadOptions {
    pub downmix_to_mono: bool, // recommended true for spatial emitters
    pub normalize: bool,       // optional, default false
}
```

ForaAudioSource

```rust
#[derive(Clone, Copy, Debug)]
pub enum PlayMode { OneShot, Loop }

#[derive(Clone, Debug)]
pub struct SourceParams {
    pub gain_db: f32,      // default 0 dB
    pub pitch: f32,        // playback rate multiplier; default 1.0
    pub spatial: bool,     // true -> spatialized; false -> 2D (bypass spatializer)
    pub start_frame: usize,// initial offset into audio data
}

#[derive(Clone, Debug)]
pub struct ForaAudioSource {
    pub data: ForaAudioData, // cheap clone handle
    pub pose: Pose,
    pub params: SourceParams,
    pub mode: PlayMode,
}

impl ForaAudioSource {
    pub fn new(data: ForaAudioData) -> Self;
    pub fn with_params(mut self, params: SourceParams) -> Self;
    pub fn with_pose(mut self, pose: Pose) -> Self;
    pub fn with_mode(mut self, mode: PlayMode) -> Self;
}
```

ForaAudioListener

```rust
#[derive(Clone, Debug)]
pub struct ForaAudioListener {
    pub pose: Pose,
}

impl ForaAudioListener {
    pub fn new(pose: Pose) -> Self;
    pub fn set_pose(&mut self, pose: Pose);
}
```

World and control

```rust
use std::time::Duration;

pub type SourceId = u64;

pub struct ForaWorld {
    // main-thread owned; not Send across threads by default if you prefer
    cfg: ForaConfig,
    listener: Option<ForaAudioListener>,
    // registry mirrors engine state
    sources: std::collections::HashMap<SourceId, ForaAudioSource>,
    next_id: SourceId,

    // internals
    engine: AudioEngineHandle, // thread-safe handle to the backend engine
}

impl ForaWorld {
    pub fn new(cfg: ForaConfig) -> Result<Self, ForaError>;

    pub fn set_listener(&mut self, listener: ForaAudioListener) -> Result<(), ForaError>;
    pub fn update_listener(&mut self, pose: Pose) -> Result<(), ForaError>;

    pub fn add_source(&mut self, src: ForaAudioSource) -> Result<SourceId, ForaError>;
    pub fn update_source_pose(&mut self, id: SourceId, pose: Pose) -> Result<(), ForaError>;
    pub fn update_source_params(&mut self, id: SourceId, params: SourceParams) -> Result<(), ForaError>;
    pub fn remove_source(&mut self, id: SourceId) -> Result<(), ForaError>;

    // Start/stop audio device and processing
    pub fn start(&mut self) -> Result<(), ForaError>;
    pub fn stop(&mut self) -> Result<(), ForaError>;

    // Poll engine -> main thread events (e.g., one-shot finished, underrun, device change)
    pub fn poll_events(&mut self) -> Vec<ForaEvent>;

    // Optional: report estimated output latency
    pub fn output_latency(&self) -> Duration;
}

#[derive(Debug)]
pub enum ForaEvent {
    SourceFinished(SourceId),
    Underrun { missing_frames: usize },
    Overrun,
    DeviceChanged,
}
```

Error handling

```rust
#[derive(thiserror::Error, Debug)]
pub enum ForaError {
    #[error("audio backend error: {0}")]
    Backend(String),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("resample error: {0}")]
    Resample(String),
    #[error("invalid operation: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
}
```

Runtime and threading architecture

- Main thread:
  - Owns ForaWorld; updates listener and source poses/params each frame.
  - Calls add/remove/update; these push commands to the engine via a lock-free queue or crossbeam channel.
  - Polls events from engine; removes one-shot sources on SourceFinished.
- Engine threads:
  - Producer/mixer thread:
    - Runs at fixed block_size frames per tick.
    - Non-blocking poll of command queue; updates internal state (listener, sources).
    - For each active source, advances playhead, fetches a frame window from audio data, wraps if Loop, marks finished if OneShot reaches end.
    - Downmix to mono if spatializer requires mono.
    - Calls audionimbus to spatialize all spatial sources into an interleaved output buffer for exactly block_size frames.
    - Mixes non-spatial (2D) sources directly into the output buffer.
    - Writes the block to the output ring buffer.
    - Emits SourceFinished events to event queue (non-blocking).
  - CPAL output callback:
    - Reads exactly requested frames from the output ring buffer, zero-fills any shortfall, and records underruns.
- Ring buffer:
  - SPSC, capacity = block_size * ring_blocks frames, interleaved f32.
  - Zero-copy slices for reads/writes that handle wrap-around.
  - Read/write cursors kept in frames, not blocks.

Engine internals (sketch)

```rust
struct AudioEngineHandle {
    cmd_tx: crossbeam_channel::Sender<EngineCmd>,
    evt_rx: crossbeam_channel::Receiver<ForaEvent>,
    // cpal stream lifetime handle
    stream_ctl: Arc<StreamController>,
    metrics: Arc<EngineMetrics>,
}

enum EngineCmd {
    SetListener(ListenerState),
    AddSource { id: SourceId, state: SourceState },
    UpdateSourcePose { id: SourceId, pose: Pose },
    UpdateSourceParams { id: SourceId, params: SourceParams },
    RemoveSource(SourceId),
    Start,
    Stop,
}

struct ListenerState { pose: Pose }

struct SourceState {
    id: SourceId,
    data: ForaAudioData, // Arc-backed
    pose: Pose,
    params: SourceParams,
    mode: PlayMode,
    playhead: usize, // frames; engine-owned
    finished: bool,
    spatial: bool,
}
```

Spatializer integration (audionimbus)

- Define a trait and a concrete adapter so you can mock in tests:

```rust
pub trait Spatializer: Send {
    fn prepare(&mut self, sample_rate: u32, block_size: usize, out_channels: u16) -> Result<(), ForaError>;
    fn process(
        &mut self,
        listener: &ListenerState,
        sources: &[SourceBlock<'_>],
        out_interleaved: &mut [f32], // len = block_size * out_channels
    ) -> Result<(), ForaError>;
}

pub struct SourceBlock<'a> {
    pub mono: &'a [f32], // length = block_size
    pub pose: Pose,
    pub gain_lin: f32,
    pub id: SourceId,
}
```

- The audionimbus adapter translates poses and audio buffers to the Steam Audio API. Initialize (HRIRs, scene config) in prepare().

Audio flow per tick

1. Apply pending EngineCmds.
2. Clear scratch output buffer (block_size * channels).
3. For each source:
   - Extract block_size frames from its data at playhead.
   - If channels > 1 and spatial: downmix to mono scratch; if non-spatial and stereo output, mix interleaved directly.
   - Apply pitch (optional MVP: 1.0 only).
   - Apply gain.
   - For OneShot, if past end:
     - mix only the remaining frames
     - queue SourceFinished
     - remove after emission
4. Spatialize all prepared SourceBlock(s) into the output buffer.
5. Write to output ring buffer.
6. Repeat until stop.

CPAL integration

- Negotiate f32 stereo at world sample_rate if possible; if device differs:
  - For MVP, allow device sample_rate mismatch but still render at world rate and let CPAL resample (or create a final small resampler); Recommended: render at device rate to avoid drift.
- Callback:

```rust
move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
    // output length = frames * channels (variable frames)
    let frames = output.len() / channels;
    let read = output_ring.pop_frames(output);
    if read < frames {
        // zero-fill remainder; record underrun
        for s in &mut output[read*channels..] { *s = 0.0; }
        metrics.underruns.fetch_add(1, Ordering::Relaxed);
    }
}
```

Resampling strategy

- On load:
  - Decode with Symphonia into f32 planar/interleaved.
  - If decoded sample_rate != world.sample_rate: resample to world sample_rate, once, into a new Vec<f32>.
  - If options.downmix_to_mono and channels > 1: downmix at load (saves CPU later).
- Trade-offs:
  - Good for SFX/short loops (keeps engine simple and RT-safe).
  - For very long assets, add a streaming mode later (decode thread + per-source FIFO).

Ring buffer design

- API:

```rust
pub struct AudioRing {
    // capacity in frames, interleaved
}
impl AudioRing {
    pub fn with_capacity(frames: usize, channels: u16) -> Self;
    pub fn available_read(&self) -> usize;  // frames
    pub fn available_write(&self) -> usize; // frames
    pub fn push_frames(&self, data: &[f32]) -> usize; // returns frames written
    pub fn pop_frames(&self, dst: &mut [f32]) -> usize; // returns frames read
    // Optional: reserve/commit zero-copy slices for perf
}
```

- Use atomics for head/tail; producer and consumer each own one end.

Main-thread workflow

- Create the world

```rust
let mut world = ForaWorld::new(ForaConfig::default())?;
world.set_listener(ForaAudioListener::new(Pose { /* ... */ }))?;
world.start()?;
```

- Load audio and create sources

```rust
let data = ForaAudioData::load_from_path("assets/sfx/explosion.wav", 48_000, LoadOptions {
    downmix_to_mono: true,
    normalize: false,
})?;

let src = ForaAudioSource::new(data.clone())
    .with_pose(Pose { /* ... */ })
    .with_mode(PlayMode::OneShot)
    .with_params(SourceParams {
        gain_db: -3.0,
        pitch: 1.0,
        spatial: true,
        start_frame: 0,
    });

let id = world.add_source(src)?;
```

- Update per frame

```rust
world.update_listener(new_listener_pose)?;
world.update_source_pose(id, new_source_pose)?;
for evt in world.poll_events() {
    if let ForaEvent::SourceFinished(id) = evt {
        // Optional: remove from your own registry; world can also auto-remove
        let _ = world.remove_source(id);
    }
}
```

- Shutdown

```rust
world.stop()?;
```

Auto-removal of one-shots

- Engine marks a OneShot source finished when it passes its last sample.
- It emits ForaEvent::SourceFinished(id).
- World, upon poll_events, removes it automatically from its own registry (you can also expose a setting to let the caller handle removal).

Performance and real-time hygiene

- Preallocate:
  - Output ring buffer (block_size * ring_blocks frames).
  - Scratch buffers for mono downmix and spatializer IO (block_size per source, reused).
- Avoid:
  - Locks, allocations, and blocking syscalls in the mixer thread and CPAL callback.
- Target:
  - Producer tick worst-case < 60% of block time. For 1024 @ 48k, keep < ~12 ms.
  - Underrun handling: zero-fill and record metrics—never block.

Drift and device changes (later milestone)

- If device rate != world rate or changes:
  - Add a tiny high-quality resampler at the output or switch engine rate to device rate.
- Subscribe to CPAL device change events and rebuild stream; keep ring and state stable.

Testing and validation

- Unit tests:
  - Ring buffer wrap and partial reads/writes.
  - OneShot completion and event emission.
  - Resampler correctness on simple tones.
- Integration:
  - Decode+load WAV/OGG/MP3 with Symphonia; compare durations.
  - Mock spatializer that just pans to validate engine mixing without audionimbus.
  - Null CPAL backend (if feasible) for deterministic CI.
- Benchmarks:
  - Producer tick time vs number of sources.
  - Throughput for various block sizes.

Crate layout

- fora_audio/
  - src/
    - lib.rs
    - world.rs
    - data.rs
    - source.rs
    - listener.rs
    - engine/
      - mod.rs
      - cpal_backend.rs
      - ring.rs
      - spatializer.rs
      - audionimbus_adapter.rs
    - decode/
      - symphonia_loader.rs
      - resample.rs
    - math.rs
    - error.rs
    - events.rs
    - config.rs
  - examples/
    - play_one_shot.rs
    - loop_spatial.rs

MVP implementation order

1. Error, config, math, events scaffolding.
2. Symphonia loader that returns ForaAudioData; implement simple resampler and mono downmix.
3. Ring buffer (SPSC) and basic CPAL output with a synthetic tone.
4. Spatializer trait + mock spatializer that just copies mono to stereo with panning.
5. Engine thread: commands, mixing of mono sources into output, loop/one-shot with events.
6. Audionimbus adapter; validate performance with a few dozen sources.
7. World API: add/update/remove listener and sources, start/stop, poll events.
8. Examples and benchmarks.

Notes and decisions

- Memory vs streaming: this design loads full samples into memory. It’s ideal for SFX/VOs/short loops. For long music files, introduce a streaming source type later.
- Channel policies:
  - Spatial emitters: recommend mono assets; downmix on load if not mono.
  - Non-spatial sources (UI, music): bypass spatializer; support stereo mixing.
- Sample rate: fixed per world instance. Changing it requires rebuilding the engine and reloading data or resampling anew.

This plan aligns with your constraints: main-thread world control, fixed-size spatialization via audionimbus, and a robust RT-safe playback path that meets the 21.3 ms deadline for 1024-frame blocks at 48 kHz.
