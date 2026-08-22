# Re: Flora / PetalSonic native path-traced audio research

> Date: 2026-08-22
>
> Scope: early reflections, late reverberation, and geometry-driven audio without making Steam Audio or a graphics renderer the owner of the audio lifetime.
>
> Evidence labels: **Current code** is verified in the checked-out Re: Flora and PetalSonic 0.6.0 sources; **Primary source** comes from an official project document or the authors' paper; **Recommendation** is a design or starting budget that still requires release-mode measurement.

## Conclusion first

**Recommendation: build a native hybrid acoustics module, not a monolithic real-time audio path tracer.** PetalSonic should own an asynchronous `AcousticPropagation` worker. The worker captures one immutable, revisioned geometry + spatial input, performs bounded CPU ray/path tracing, and publishes one latest-only immutable `AcousticResponse`. The existing PetalSonic render thread should only crossfade that response and run bounded DSP: direct-path filtering, a small set of directional early-reflection taps, and a shared three-band feedback delay network (FDN) for late reverberation.

```text
Re: Flora game/render lifetime                    PetalSonic lifetime

Contree CPU snapshot -- Arc + revision --+      publish_spatial_frame(...)
                                         |                 |
                                         v                 v
                              latest complete AcousticInput
                                         |
                              AcousticPropagation worker
                           ray/path trace; no audio deadline
                                         |
                                atomic latest-only publish
                                         v
                              immutable AcousticResponse
                                         |
                            PetalSonic render thread: DSP
                  direct filter + early taps/HRTF + shared FDN
                                         |
                              lock-free device ring buffer
```

This preserves the useful part of "path-traced audio"—geometry-derived paths, delay, direction, and frequency-band energy—without tracing geometry on every audio block or coupling PetalSonic to Vulkan. It also matches strong first-party precedents: Valve says reflection/path simulation is CPU-intensive and should run on a separate thread, with results transported to the audio mixer; Schissler and Manocha asynchronously build impulse responses and atomically swap completed results while the output path performs convolution. [Valve simulation contract](https://valvesoftware.github.io/steam-audio/doc/capi/simulation.html), [Valve integration contract](https://valvesoftware.github.io/steam-audio/doc/capi/integration.html), [Schissler and Manocha, *Interactive Sound Propagation and Rendering for Large Multi-Source Scenes*](https://gamma-web.iacs.umd.edu/MULTISOURCE/paper.pdf)

The first audible milestone should be **listener-centric three-band late response + one shared FDN reverb bus**. Bounded, priority-source early paths should follow after that. This order creates spaciousness at cost largely independent of the thousands of declared emitters, then spends per-source work only where directionally distinct echoes matter.

## What the current code actually does

### Re: Flora is already on the native rendering plan

**Current code.** Re: Flora depends on PetalSonic 0.6.0, supplies a native `.petalhrtf`, selects `SpatialQuality::LowLatency`, uses a 48 kHz / 1024-frame world block, and supplies both any-hit and closest-hit Contree adapters through an `AcousticSceneSnapshot` ([`Cargo.toml`](../Cargo.toml), [`spatial_sound_manager.rs`](../src/audio/spatial_sound_manager.rs)). PetalSonic resolves this quality profile to native per-source HRTF with Ambisonics disabled; current startup logs confirm `hrtf_backend=Native`, `acoustics_backend=Native`, and `use_ambisonics=false` ([PetalSonic backend selection](https://github.com/tr-nc/petalsonic/blob/cdbce9bead1c587630a45a07cb542e519ae8e351/petalsonic/src/engine.rs#L369-L395)). At 48 kHz, 1024 frames represent a 21.33 ms render quantum.

**Current code.** PetalSonic's device callback only consumes a lock-free ring buffer; a separate `petalsonic-render` thread fills that buffer ([render thread](https://github.com/tr-nc/petalsonic/blob/cdbce9bead1c587630a45a07cb542e519ae8e351/petalsonic/src/engine.rs#L567-L590), [device callback](https://github.com/tr-nc/petalsonic/blob/cdbce9bead1c587630a45a07cb542e519ae8e351/petalsonic/src/engine.rs#L1608-L1665)). That is a valuable separation, but work on the render thread is still on the underrun critical path: when it cannot keep the ring buffer filled, the callback emits silence.

### The existing native reflection is a single tap, not path tracing

**Current code.** For every spatial source and every render quantum, `SpatialProcessor` currently:

1. traces one any-hit segment from listener to source and turns any obstruction into binary gain `0.0`;
2. traces one deterministic closest-hit ray in `normalize(direct_direction + (0, -0.65, 0))`;
3. traces one visibility segment from that hit to the source;
4. if visible, renders one delayed, HRTF-spatialized reflection tap with hard-coded maximum delay and gain.

The implementation is visible in [PetalSonic `SpatialProcessor`](https://github.com/tr-nc/petalsonic/blob/cdbce9bead1c587630a45a07cb542e519ae8e351/petalsonic/src/spatial/processor.rs#L545-L746). It has no stochastic path set, no multi-bounce transport, no accumulated late-energy response, and no late-reverb renderer. Calling it "path-traced audio" today would therefore be inaccurate. Its downward-biased one-tap heuristic is useful as a tracer integration proof, not a general early-reflection solution.

The ray queries are also synchronous with per-source audio rendering. PetalSonic's own query trait says implementations must have bounded work and must not allocate, block, or access mutable game state because they run on the render thread ([acoustics query interface](https://github.com/tr-nc/petalsonic/blob/cdbce9bead1c587630a45a07cb542e519ae8e351/petalsonic/src/acoustics.rs#L37-L65)). Moving multi-ray, multi-bounce work into this interface would defeat that contract. Real-time audio guidance is stricter still: Apple's render-thread documentation says to finish quickly and avoid heap allocation or locks. [Apple `Audio.GeneratorRenderHandler`](https://developer.apple.com/documentation/realitykit/audio/generatorrenderhandler)

### The geometry representation is reusable, but its snapshot semantics need tightening

**Current code.** Re: Flora already asynchronously reads completed Contree chunk data back from the GPU, decodes CPU chunk caches on a worker, and publishes `Arc<ContreeRayQueryState>` versions by shallow-cloning the scene index and `Arc` chunk map ([`ContreeBuilder`](../src/builder/contree/mod.rs)). The any-hit and closest-hit adapters use only these CPU structures; they do not expose Vulkan devices, command buffers, or GPU buffers to PetalSonic. This is exactly the geometry-reuse seam needed for a native CPU acoustics worker.

The current integration does not, however, publish true geometry generations to PetalSonic:

- Re: Flora constructs `AcousticSceneSnapshot::new(1, ...)` once ([`SpatialSoundManager::new`](../src/audio/spatial_sound_manager.rs)).
- `ContreeAnyHitRayTracer` internally swaps `Arc<ContreeRayQueryState>` behind an `RwLock` whenever CPU chunk state changes ([snapshot publication](../src/builder/contree/mod.rs)).
- Each any-hit or closest-hit batch independently `try_read`s and clones whatever internal state is current at that moment ([ray adapters](../src/builder/contree/mod.rs)).
- PetalSonic consequently never sees an acoustic-scene version after `1`, even though its world already supports monotonically increasing scene publication ([`publish_acoustic_scene`](https://github.com/tr-nc/petalsonic/blob/cdbce9bead1c587630a45a07cb542e519ae8e351/petalsonic/src/world.rs#L708-L727)). A direct query and a later reflection query in the same render quantum can therefore capture different internal Contree revisions.

That is acceptable for the current coarse tap but not for a multi-bounce solve. One solve must capture one immutable `Arc` generation and tag its result with both geometry and spatial-input revisions. A completed older solve must never overwrite a newer response.

**Current code.** Contree already records aggregate ray-call, processed-source, hit, failure, and elapsed-time atomics, but does not expose them to Re: Flora diagnostics. PetalSonic also defines `physics_simulation_time_us`, yet the current spatial processor never increments it. Before adding more acoustics work, the project needs truthful solver observability rather than relying only on final underruns.

## Recommended deep module and seam

`AcousticPropagation` should be a **deep module**: callers retain the two concepts they already understand—complete spatial frames and immutable acoustic-scene versions—while worker scheduling, ray budgets, temporal accumulation, source prioritization, path extraction, FDN parameter estimation, response retirement, and cancellation stay inside its implementation.

The external interface should remain small:

```rust
world.publish_spatial_frame(SpatialFrame { revision, listener, emitters })?;
world.publish_acoustic_scene(AcousticSceneSnapshot { version, query_snapshot })?;
let diagnostics = world.diagnostics();
```

`AcousticResponse` should be private to PetalSonic because its only consumer is PetalSonic's render thread. Exposing tap lists, worker queues, FDN coefficients, or trace scheduling to Re: Flora would create a shallow module and spread the acoustics lifetime back across both repositories.

The internal query seam should represent **one captured immutable scene**, not a stable adapter with hidden mutable state:

```rust
trait AcousticRayQuerySnapshot: Send + Sync {
    fn trace_any_batch(&self, rays: &[Ray], out: &mut [bool]);
    fn trace_closest_batch(&self, rays: &[Ray], out: &mut [Option<Hit>]);
}
```

This is a real seam rather than hypothetical indirection because it has two justified adapters:

- production: a `ContreeAcousticSnapshot` holding one `Arc<ContreeRayQueryState>`;
- tests: a deterministic analytic-room or in-memory voxel adapter.

The worker's latest-only state machine should be:

```text
idle
  -> capture newest complete spatial + geometry input
  -> solve within current quality budget
  -> if superseded, discard result
  -> publish complete AcousticResponse atomically
  -> retain last complete response until a newer one finishes
```

An `AcousticResponse` should carry at least:

- `spatial_revision` and `geometry_version`;
- per-priority-source direct-path three-band gains;
- a bounded list of early taps: source identity, delay, arrival direction, and three-band gain;
- listener-centric late response: pre-delay, three-band energy/RT60, wet gain, and optional first-order directional energy;
- publication time for response-age diagnostics.

The render thread should read one response at each quantum edge, crossfade parameter changes over multiple quanta, and continue using the last complete response when the worker misses an update. It must never wait for geometry, path tracing, scene retirement, or an IR build. Valve's own integration guide calls transport from update/simulation threads to the mixer the crucial integration task, and its simulation interface explicitly forbids direct simulation with occlusion on the audio-processing thread and directs reflection/pathing work to separate threads. [Valve integration guide](https://valvesoftware.github.io/steam-audio/doc/capi/integration.html), [Valve simulation interface](https://valvesoftware.github.io/steam-audio/doc/capi/simulation.html)

## Technique comparison

| Technique | What it provides | Cost/update shape | Fit for editable Re: Flora terrain | Decision |
|---|---|---|---|---|
| Real-time geometric ray/path tracing | Dynamic direct paths, directional early paths, high-order energy estimates | Trace cost grows with rays, bounces, visibility connections, and naive per-source work; update can be much slower than sample rate | Strong if run asynchronously against captured Contree snapshots | Use as a **low-rate producer**, not the renderer |
| Baked wave/probe transfer | Wave effects including diffraction; cheap runtime interpolation | Expensive offline bake plus probe memory/disk; static geometry assumption | Poor as the primary path for terrain that can be edited or loaded arbitrarily | Optional later cache for immutable authored spaces |
| Ray-traced early taps + parametric/FDN late reverb | Distinct echoes and directions early; stable, cheap dense tail | Bounded taps for priority sources; one listener-centric late solve and shared DSP bus | Strong | **Recommended default** |
| Short convolution early window + FDN tail | More faithful fixed early response than taps; bounded tail | Partitioned convolution and IR crossfades still add state/cost, but the window can be capped | Possible after the tap path is measured | Optional quality tier |
| Full multi-second convolution per source | Maximum detail for a fixed sampled IR | IR generation, storage, time-varying crossfade, and convolution scale poorly with source/channel count | Poor default for thousands of emitters and dynamic terrain | Reject as production default; retain as reference |
| Manual reverb zones | Very low runtime cost | Artist-authored and disconnected from geometry changes | Useful only as override/fallback | Not the physical default |

### Why the hybrid is the best first target

**Primary source.** Valve's reflection interface explicitly distinguishes full convolution, parametric FDN, and a hybrid where the initial IR is convolved and the late portion estimates parametric reverb. Its documentation states that convolution preserves the most detail but can use significant CPU, while FDN is cheaper but cannot reproduce individual echoes, especially outdoors. It exposes three-band RT60, EQ, and the delay where the parametric tail begins. [Valve reflection effect](https://valvesoftware.github.io/steam-audio/doc/capi/reflections-effect.html)

**Primary source.** Schissler and Manocha compute low-sample-rate path-traced responses, extract direct sound, early reflections, and late-reverb parameters, and render a constant number of short HRTF convolutions independent of source count. Their tested implementation reported roughly 9–15× improvement over its full-convolution comparison while retaining similar audio; those historical numbers are evidence for the representation split, not a performance promise for Re: Flora. [*Interactive Sound Rendering on Mobile Devices using Ray-Parameterized Reverberation Filters*](https://arxiv.org/abs/1803.00430)

**Primary source.** Microsoft's precomputed wave system independently divides responses into spatially detailed early peaks and a late response stored once per room, demonstrating that early/late separation is not specific to one ray-tracing SDK. Its benefit includes diffraction, but it depends on offline simulation of a complex **static** scene. [Microsoft Research, *Precomputed Wave Simulation for Real-Time Sound Propagation of Dynamic Sources in Complex Scenes*](https://www.microsoft.com/en-us/research/publication/precomputed-wave-simulation-real-time-sound-propagation-dynamic-sources-complex-scenes/)

### Why listener-centric tracing matters

Naively launching thousands of rays per source makes cost approximately source-linear and wastes paths that never reach the listener. Schissler and Manocha instead trace backward from the listener, connect paths to sources, and cluster distant sources; their paper reports sub-linear source scaling and demonstrates high-order transport in scenes with up to 200 sources. The exact historical timings do not transfer to current hardware or Contree, but the work-sharing direction does. [*Interactive Sound Propagation and Rendering for Large Multi-Source Scenes*](https://gamma-web.iacs.umd.edu/MULTISOURCE/paper.pdf)

For Re: Flora, one listener-centric late field can feed a shared reverb bus. Early reflection connection tests should be limited to perceptually important sources—loud, near, currently playing, and not masked—not all 8,192 possible emitters. Tree ambience clusters already reduce authored wind sources; the acoustics worker should preserve that leverage rather than expanding them again.

### Where baking and probes fit

Baked wave fields are attractive for fixed authored houses because their runtime is stable and they can retain diffraction and low-frequency behavior that ordinary geometric rays miss. They are a poor universal answer for Re: Flora because terrain snapshots and runtime edits invalidate precomputed geometry. Uniform probes also need high density in narrow spaces; Microsoft's adaptive-probe work specifically changes density using local spatial diameter to avoid undersampling corners, corridors, and stairways. [Microsoft Research, *Adaptive Sampling for Sound Propagation*](https://www.microsoft.com/en-us/research/publication/adaptive-sampling-for-sound-propagation/)

A later optional cache could key baked or progressively accumulated late-response cells by terrain/chunk dependency revisions. It must fall back to the live worker on any mismatch. It should not be required for correctness.

### Why geometry can be shared but lighting cannot

Reusing Contree means reusing occupancy, closest-hit distance, normals, chunk revisioning, and acceleration data. Acoustic transport still needs its own payload: propagation time, three-band energy, surface absorption/scattering/transmission, and eventually diffraction or portal approximations. Geometric acoustics assumes scene primitives are large relative to wavelength; the cited real-time system uses separate diffraction approximations for wave effects. A visual GI/DDGI result therefore cannot be relabeled as an acoustic response. [Schissler and Manocha, geometric-acoustics assumptions](https://gamma-web.iacs.umd.edu/MULTISOURCE/paper.pdf)

The current Re: Flora adapter returns `AcousticMaterial::default()` for every Contree hit. A credible next step must map voxel/material types to a small, data-driven three-band table. No high-frequency material taxonomy is needed initially; a few stable categories such as soil/stone/wood/foliage/water are enough to expose absorption and scattering behavior for measurement.

## Starting update and cost budgets

The following are **recommendation starting points, not externally validated targets**. Release-mode fixed-scene sweeps must decide the final values.

| Output | Initial simulation cadence | Initial work cap | Audio rendering |
|---|---:|---:|---|
| direct occlusion / transmission | latest-only 30 Hz | 1–4 any-hit samples for at most 32 priority sources | three-band gain smoothed every quantum |
| early reflections | 10–15 Hz | 64–256 listener rays, 1–2 bounces, at most 8 taps each for 8 priority sources | fractional delay + three-band gain + HRTF arrival direction |
| late response | 2–5 Hz | 256–1,024 listener rays, up to 8–16 energy bounces; stop by energy threshold | one shared three-band FDN, optionally first-order directional input |
| geometry publication | on completed Contree revision | shallow `Arc` snapshot only; coalesce superseded revisions | keep prior `AcousticResponse` until solve completes |

Why not update everything at the same rate? Direct occlusion is tightly tied to visible motion; early paths need moderate responsiveness; dense late reverberation is statistically smoother and can use longer temporal history. Adaptive impulse-response work explicitly gives early arrivals a shorter response time and late reverberation a longer one to exploit temporal coherence. [Schissler and Manocha, *Adaptive Impulse Response Modeling for Interactive Sound Propagation*](https://gamma-web.iacs.umd.edu/ADAPTIVEIR/paper.pdf)

All caps must be adaptive under a fixed wall-time budget: reduce late rays first, then early source count/taps, while preserving direct sound. Never let a backlog accumulate; solve only the newest complete input.

## Phased implementation recommendation

### Phase 0 — make the current cost and revisions observable

Do this before changing sound quality:

- expose Contree ray-query calls/rays, p50/p95/p99 batch time, miss/failure count, and captured geometry revision;
- measure PetalSonic render p50/p95/p99, response age p50/p95/p99, worker solve time, superseded solves, and dropped publications;
- correlate each acoustics spike with ring occupancy and underruns;
- add a deterministic impulse/step harness that records direct gain, tap delays/directions, and late parameters for a fixed scene;
- define release scenes: open field, single wall, small room, narrow doorway/corridor, and changing terrain.

Acceptance: current audio behavior is unchanged, `physics_simulation_time_us` is truthful, and toggling the present one-tap reflection produces a measurable ray/render delta.

### Phase 1 — build the native late-reverb renderer without geometry tracing

Add one shared listener-centric three-band FDN reverb bus to PetalSonic. Drive it first with explicit test parameters (`pre_delay`, low/mid/high RT60, wet level, damping) and crossfade changes. Keep direct HRTF intact.

Acceptance: zero steady-state allocation, no new device-callback work, bounded render p99, deterministic decay-envelope tests, and no clicks while parameters change.

This phase intentionally proves the DSP consumer before the expensive producer. FDN is a standard parametric late-reverb representation; Valve's official effect interface uses FDN for its lower-cost parametric mode and three-band RT60 parameters. [Valve reflection effect](https://valvesoftware.github.io/steam-audio/doc/capi/reflections-effect.html)

### Phase 2 — correct the scene seam and add `AcousticPropagation`

- publish a monotonically versioned `ContreeAcousticSnapshot` containing one captured `Arc<ContreeRayQueryState>`;
- make the PetalSonic-owned worker consume latest complete spatial + scene generations;
- publish a private immutable `AcousticResponse` and retain the previous response until replacement;
- cancel/discard superseded results by revision; never block the render thread during scene retirement;
- initially solve only direct three-band occlusion/transmission and a listener-centric late-energy histogram that drives the Phase 1 FDN.

Acceptance: one solve never mixes geometry revisions; rapid terrain edits coalesce; the render thread can continue indefinitely on the last complete response; response age and underrun budgets pass the release gate.

### Phase 3 — replace the one-tap heuristic with bounded early paths

Trace listener-centric paths and connect vertices to priority sources with visibility rays. Start with 1–2 bounces and a small three-band material table. Extract, sort, and temporally stabilize a bounded set of taps by perceptual energy. Render each retained tap with propagation delay, band gain, and native HRTF direction; merge the remaining energy into the late histogram.

Acceptance: the single-wall scene produces the expected first-order delay; the doorway case changes smoothly with motion; taps remain stable at a fixed listener; cost scales with the configured priority/tap cap, not declared emitter capacity.

### Phase 4 — only add sophistication that survives measurement

Candidates, in priority order:

1. source clustering and first-order directional late field;
2. short partitioned-convolution early window as an optional quality tier;
3. portal/diffraction approximation for narrow openings;
4. revision-keyed baked/progressive response cells for immutable terrain;
5. an optional GPU ray-query adapter that publishes the same `AcousticResponse`.

The GPU adapter must remain an implementation detail behind the same seam. Valve notes that its GPU ray tracer can be much faster but requires care not to consume graphics GPU time; sharing an existing ray tracer is supported precisely through callbacks rather than ownership of the game renderer. [Valve programmer's guide](https://valvesoftware.github.io/steam-audio/doc/capi/guide.html), [Valve scene callbacks](https://valvesoftware.github.io/steam-audio/doc/capi/scene.html)

## Rejected alternatives

### Re-enable Steam Audio as the world-owned acoustics backend

Rejected as the production direction. It reintroduces context/simulator/effect lifetime into PetalSonic and does not solve the project's previous performance and ownership problems. Steam Audio remains useful as an offline/reference oracle for impulse responses and parameter comparisons, not as a required runtime dependency.

### Run path tracing once per source per 1024-frame block

Rejected. At the current settings this schedules geometry work every 21.33 ms and scales it with active sources. The present single closest-hit plus visibility query is already on the render path; multiplying it into hundreds of rays and bounces would directly threaten ring-buffer refill. Valve itself says not to run occlusion on the audio-processing thread and to put reflections/pathing on a separate thread. [Valve simulation interface](https://valvesoftware.github.io/steam-audio/doc/capi/simulation.html)

### Keep one long, time-varying convolution IR per source

Rejected as the default. Full convolution preserves individual echoes but carries IR generation, memory, crossfade, channel, and convolution cost per source. Valve documents significant CPU usage for full convolution and offers parametric/hybrid modes for this trade-off; Schissler and Manocha's ray-parameterized renderer reduces the number of convolutions to a listener-dependent constant. [Valve reflection effect](https://valvesoftware.github.io/steam-audio/doc/capi/reflections-effect.html), [ray-parameterized reverberation](https://arxiv.org/abs/1803.00430)

### Couple PetalSonic directly to Vulkan/graphics path tracing by default

Rejected. It makes device loss, queue submission, renderer teardown, readback, and graphics scheduling part of the audio module's interface. It also risks competing with the frame renderer. CPU Contree snapshots already provide a viable adapter with independent ownership. GPU execution can be evaluated later behind the same immutable-response seam; it must never require the audio thread to submit or wait for graphics work.

### Use only baked probes

Rejected as the universal solution. Baking is excellent for static scenes and wave effects, but arbitrary loaded terrain and runtime edits invalidate it. Uniform probes also undersample narrow spatial features unless density and interpolation account for geometry. [Microsoft precomputed wave simulation](https://www.microsoft.com/en-us/research/publication/precomputed-wave-simulation-real-time-sound-propagation-dynamic-sources-complex-scenes/), [Microsoft adaptive sampling](https://www.microsoft.com/en-us/research/publication/adaptive-sampling-for-sound-propagation/)

### Build a real-time wave solver first

Rejected for this project stage. Wave solvers capture diffraction and low-frequency effects that geometric rays approximate poorly, but their cost depends strongly on spatial resolution/frequency and scene volume; successful game systems commonly precompute and compress static-scene wave fields. The hybrid geometric/FDN plan reaches the requested audible effects while preserving dynamic terrain. [Microsoft Project Triton publications](https://www.microsoft.com/en-us/research/project/project-triton/publications/), [Microsoft precomputed wave simulation](https://www.microsoft.com/en-us/research/publication/precomputed-wave-simulation-real-time-sound-propagation-dynamic-sources-complex-scenes/)

## Decision

Proceed with **native asynchronous hybrid acoustics**:

1. shared three-band FDN consumer;
2. version-correct immutable Contree scene adapter;
3. PetalSonic-owned latest-only `AcousticPropagation` worker and private `AcousticResponse`;
4. listener-centric late-energy estimation;
5. bounded priority-source early taps;
6. only then evaluate diffraction, probes, short convolution, or GPU execution.

This is a general project solution rather than a scene-specific trick. It uses Re: Flora's authoritative geometry and revision lifecycle, lets PetalSonic keep audio scheduling and DSP ownership, and makes both CPU and any future GPU solver replaceable without changing the render-thread contract.
