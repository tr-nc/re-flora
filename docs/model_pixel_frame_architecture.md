# Model Pixel Frame ownership

`src/tracer/model_pixel_frame.rs` owns the renderer-internal publication protocol
for particle model pixels (butterflies and fallen leaves), attached apples,
fallen apples and flowers. Geometry-free Slang surface lookup, pose, shading and
display form the frame seam; geometry exists only in cache baking. The restored
[shared model-surface cache](performance/model-pixel-cache.md) is nested in this
owner: immutable surfaces are generated once per configuration; frame-local tiles
perform lookup and relighting, not repeated geometry sampling.

## Interface decision

Two plausible interfaces were considered:

1. A typed-key replacement for `ModelPixelTiles::get`. This avoids bit collisions
   but leaves the host responsible for reconstructing matching tile/object sizes,
   dispatch ranges and graphics bindings. It does not hide the protocol.
2. Semantic adapter methods returning `PreparedModelPixels`. This is the chosen
   seam: the module computes a batch and prepares its draw against the **same**
   storage pair, range and pipeline. The result exposes only `record`, not buffers,
   descriptor arrays, allocation keys or a list of internal steps.

`Tracer` supplies native pipeline dependencies, scene facts and profiling scopes.
It still owns pass placement, viewport, quad vertex/index bindings and the outer
render pass. Tree draws retain the pose push used for compute instead of rebuilding
it later. Deleting this module would restore buffer identities, capacities,
compute/display declarations and range pairing to four adapters, not merely
remove a forwarding method.

## Ownership and invariants

- `ModelPixelFrame` owns the shared surface cache, particle pose adapter and frame-local storage.
  `ButterflyMeshRenderer` prepares poses/materials, sorts back-to-front and
  publishes instances and draw indices **once**, after visibility and
  tile offsets are final. There is no triangle stream, repair graph, reference tile
  stream, zero-view sentinel or geometry-mode argument in the frame interface.
  Instances reference immutable leaf variants/animation frames. CPU preparation
  cannot upload partial metadata. `ModelPixelFrame` allocates/uploads only the
  instance and draw-index streams in the ready fence slot. `PreparedModelPixels` retains
  and binds the matching draw-index buffer, rather than exposing it to `Tracer`.
- `ParticleTiles` packs in that sorted draw order and returns instance-indexed
  offsets. Invisible models reserve no cells. Contiguous particle batches remain
  at most 64 MiB and 65,535 instances (the portable Z-dispatch bound), and each
  object buffer addresses the complete particle instance stream, including indices
  outside the batch's local draw range. These are same-frame batches, not a leaf
  admission limit; even invisible/8px models split at the dispatch bound.
- Private batch identities distinguish particles, tree IDs and fallen fruit.
  Tile and four-float4 object allocations form one pair; no bitfield namespaces
  or raw lookup keys cross the interface. A second preparation of the same batch
  in one frame is rejected instead of overwriting its live storage.
- Storage grows on demand with the existing power-of-two capacities and portable
  128 MiB per-binding guard. Resolution reductions reuse capacity. Allocation or
  compute failure cannot publish a draw from a partially prepared pair.
- At a ready slot's next begin, allocations absent from that slot's previous use
  retire. An empty frame publishes no stale draws. Other slots are untouched.
  This preserves the original last-use reuse/retirement cadence.
- **FrameManager owns fence readiness.** The caller must already have waited for
  the selected slot. **PipelineTopology/native pipelines own descriptor and extent
  retirement.** The model module delegates transient frame initialization and
  descriptor preparation; it neither waits for fences nor invents a second
  retirement mechanism. Prepared draws are frame-local and recorded before the
  next slot reuse.

The intentional shadow → externally owned terrain moisture → scene seam remains
unchanged. See [the render transaction decision](render_frame_transaction_architecture_review.md).
The original frame-ownership refactor did not change App scheduling, simulation
or shader policy. The later cache restoration preserves its compute/draw and
fence/descriptor boundaries; cache generation is prepared once before consumers.

## Regression surface

`model_pixel_frame::tests` exercises the same internal storage/publication
interface used by the native adapters, with identifiable in-memory allocations
rather than a mock Vulkan implementation. It covers reversed sorted draws beyond
64 MiB, 200,000 low-resolution/invisible models, demand-sized input publication
and ready-slot reuse/retirement, mixed resolutions/offscreen objects, all four
adapter identities, compute/draw pairing, empty frames, removal and slot-specific
retirement, growth/shrink transitions, duplicate publication, allocation/compute
failure and binding limits. The particle adapter's publication test observes the
actual upload callback after complete sorted offsets are installed. The former
standalone packing tests are replaced, not copied beside the new seam.

Transitive shader-import guards discover every model-pixel entry point (including
future consumers), rejecting bake/triangle imports in compute as well as graphics.
Native checks remain essential for GPU resource use and visual preservation:

```sh
CARGO_BUILD_JOBS=4 cargo fmt --check
CARGO_BUILD_JOBS=4 cargo check
CARGO_BUILD_JOBS=4 cargo test model_pixel
CARGO_BUILD_JOBS=4 cargo test tracer::butterfly_mesh
CARGO_BUILD_JOBS=4 cargo test tracer::apple_pixel
flock --close /tmp/re-flora-vegi-cache-controls-gpu.lock env -u WAYLAND_DISPLAY CARGO_BUILD_JOBS=4 \
  cargo run --release -- --hidden --mute --auto-exit 0.5
flock --close /tmp/re-flora-vegi-cache-controls-gpu.lock env -u WAYLAND_DISPLAY CARGO_BUILD_JOBS=4 \
  node scripts/validate-apple-model.mjs --seconds 12 --stage-one
flock --close /tmp/re-flora-vegi-cache-controls-gpu.lock env -u WAYLAND_DISPLAY CARGO_BUILD_JOBS=4 \
  python3 scripts/validate_butterfly_mesh.py --seconds 12
flock --close /tmp/re-flora-vegi-cache-controls-gpu.lock env -u WAYLAND_DISPLAY CARGO_BUILD_JOBS=4 \
  node scripts/validate-leaf-model.mjs --seconds 20
```

Current cache-only checks and explicit diagnostic migrations are recorded in
[the cache-only handoff](evidence/cache-only-renderer.md). Geometry readback happens
once per bank rebuild, never in any frame consumer.

The following is historical worker evidence; the later controller acceptance in
[evidence/model-vine-ddgi-acceptance.md](evidence/model-vine-ddgi-acceptance.md)
records the resolved leaf-review/projection issues. The retirement merge retains
those diagnostic corrections without relaxing their numerical checks.

Worker evidence and exact outcomes live under `target/improve-delivery/`. The
unmodified baseline leaf runner is currently obstructed by the automatic playable
vine demo taking its camera after the leaf fixture starts: the baseline screenshot
shows the vine wall, and its 8/64px modes have no checked samples. This is not a
passing leaf numerical oracle and no checker tolerance was relaxed. The stage-one
native fixture independently covers all three consumers, both orthographic
display modes, independent resolutions, actual fruit drops and resize. Neither
unit tests nor these correctness runs claim a performance improvement or final
aggregate acceptance.

The worker's P1 run passed formatting/check, 24 model-pixel tests (one existing
ignored experiment), 16 particle-adapter tests, one apple-shape test, hidden
Release smoke, apple stage-one, butterfly oracle and tree/resize lifecycle.
All 126 butterfly tile PNGs matched the baseline SHA-256 hashes byte-for-byte.
Baseline and candidate also completed an eight-second, 2880×1620 production run
with 1,100 rotating leaves and 21 butterflies, all at 64px, crossing the 64 MiB
batch limit without review readbacks. The strict leaf runner remained red on both
revisions; its empty early captures are **not** counted as numerical evidence.
