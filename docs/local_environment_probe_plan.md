# Local Environment Probe Plan

> This document records the completed local-SH probe implementation. Its approved replacement is
> specified in the [DDGI Migration Plan](ddgi_migration_plan.md).

## Background

Re: Flora now uses one shared global L2 spherical-harmonic environment irradiance representation
for both terrain and raster-rendered flora, leaves, fruit, particles, and props. The stochastic
terrain diffuse second ray and the main screen-space RGB temporal/A-Trous denoiser have been
removed. Direct sun and the independent VSM, leaf-shadow, cloud, and cloud-shadow histories remain
separate.

The global SH representation is deterministic and inexpensive, but it has no position-dependent
terrain visibility. A surface deep inside a roofed chamber therefore receives the same environment
irradiance as a matching open-sky surface with the same normal. Direct-sun shadowing can make the
chamber visibly darker, but the remaining environment fill is still too uniform and can leak
through roofs and walls.

The finite world currently contains `2 x 2 x 2` terrain chunks, each containing `256 x 256 x 256`
voxels. This makes a fixed world-aligned probe grid a practical first design: its coordinates and
maximum resource requirements are known, terrain edits can address it directly, and consumers do
not need camera-relative scrolling or probe residency logic.

The deterministic
[environment-lighting test scene](./environment_lighting_test_scene.md) provides matching roofed and
open rock bays, a portal transition, comparison plinths, and a retained raster tree. It is the
primary development and acceptance scene for the probe implementation.

## Goal

Add a world-aligned, spatially varying environment probe volume whose grid is fixed between
explicit rebuilds but whose spacing is adjustable:

1. reduces environment light in terrain-occluded regions without restoring a per-pixel diffuse
   second ray;
2. supplies the same local SH irradiance to terrain and animated raster consumers;
3. exposes a runtime-adjustable probe spacing with deterministic CLI control;
4. visualizes probe placement, state, and useful lighting summaries at controllable debug cost;
5. updates predictably after terrain and environment revisions;
6. remains deterministic enough that it does not require a screen-space RGB denoiser;
7. records enough release-mode timing and memory evidence to select a default density.

## Current Decision

- Use a finite, world-aligned local probe grid. The world does not need camera-relative scrolling,
  probe paging, or dynamic residency for the first implementation.
- Express density as probe spacing in terrain voxels. Support several fixed spacing choices and
  rebuild explicitly when the choice changes.
- Store deterministic terrain visibility separately from derived SH irradiance so an environment
  revision can refresh lighting without retracing terrain.
- Let terrain and every raster environment-lighting consumer use the same position-aware SH
  sampler. Animated grass and leaves receive the local environment result but do not become terrain
  ray-tracing geometry or probe occluders.
- Visualize all probes directly from GPU data. Do not add probe picking, a selected-probe concept,
  a per-probe detail panel, or full-volume readback.
- Keep the removed terrain diffuse second ray and main RGB temporal/A-Trous denoiser removed. Probe
  updates may be scheduled over time, but the final screen image must not require stochastic
  screen-space reconstruction.
- Keep direct sun and future local direct lights outside the environment probe field. Evaluate
  ReSTIR DI only later, and only if measured local-light candidate or shadow cost justifies it.

Phases 1 through 7 are complete: density and resource controls exist, all-probe visualization
exists, terrain plus raster consumers share the local field, deterministic visibility and local SH
are scheduled in bounded batches, interpolation uses leak-resistant weights, authored-environment
revisions reproject retained visibility without terrain rays, and normal terrain rebuild plans
automatically request a prioritized then full-volume probe refresh. A GPU aggregate pass reports
the exact state distribution after convergence without reading back the probe volume. Matched
release evidence selects 32-voxel spacing as the production default; 16 remains a quality option,
8 a stress option, and 64 a coarse development option.

## Non-goals

- Do not add animated flora or leaves to terrain voxel traversal.
- Do not treat probes as a replacement for direct sun, local direct lights, or their shadows.
- Do not implement ReSTIR in this phase.
- Do not implement multi-bounce GI or terrain color bleeding in the first visibility-probe step.
- Do not restore the removed main RGB temporal or A-Trous denoiser.
- Do not draw text, ray sets, or SH lobes for every probe simultaneously.
- Do not add probe picking, a selected-probe panel, or per-probe CPU/GPU readback.
- Do not make probe visualization part of normal gameplay rendering.
- Do not automatically persist runtime density experiments to `config/gui.toml`.

## Probe Density Contract

Expose density as **probe spacing in terrain voxels**. Spacing is less ambiguous than a generic
density value and maps directly to terrain edits, probe invalidation, and visual inspection.

The initial supported candidates should be discrete powers of two:

| Spacing | Grid for the current 512-voxel world axis | Probe count | Aligned RGB L2 SH only |
| ---: | ---: | ---: | ---: |
| 64 voxels | `9 x 9 x 9` | `729` | about `0.10 MiB` |
| 32 voxels | `17 x 17 x 17` | `4,913` | about `0.67 MiB` |
| 16 voxels | `33 x 33 x 33` | `35,937` | about `4.94 MiB` |
| 8 voxels | `65 x 65 x 65` | `274,625` | about `37.71 MiB` |

The SH column counts only nine aligned RGB `float4` coefficients per probe. Visibility, hit-distance,
state, revision, relocation, and scheduling data add to these lower bounds and must be reported by
the implementation.

Matched hidden release measurements select 32-voxel spacing. It passes the chamber, roof, wall, and
portal checks with only a sub-one-code-value mean-luma difference from 8-voxel spacing in the
roofed-interior crop, while using about one fifty-fifth of the memory and converging about fifty-five
times faster. The 16-voxel spacing remains a quality/debug option and 8-voxel spacing remains a
stress option.

Density must be controllable through both:

- a deterministic CLI override, planned as `--environment-probe-spacing-voxels <N>`;
- a runtime debug UI using discrete choices and an explicit **Apply/Rebuild** action.

The UI must show the resulting grid dimensions, probe count, estimated or allocated GPU bytes, and
rebuild status before or while applying a new density. It must not rebuild continuously while a
slider is dragged.

## Initial Lighting Model

The first local-probe implementation should capture **terrain visibility of the authored
environment**, not general dynamic radiance.

For each valid probe:

1. trace a deterministic set of directions through the existing terrain voxel traversal;
2. classify an environment miss versus a terrain hit and retain enough directional visibility or
   first-hit-distance information for reconstruction and leak-resistant interpolation;
3. evaluate the same authored sky model used by the global SH path on visible directions;
4. exclude the explicit sun disc because direct sun remains separate;
5. project the visible environment into the existing L2 irradiance convention.

This separates environment changes from terrain visibility:

- a time-of-day or authored-sky revision should recompute local irradiance from retained visibility
  information without retracing terrain;
- a terrain revision should retrace affected probe visibility;
- moving raster vegetation should neither dirty probes nor become a probe occluder.

The existing shader contract remains the public consumer API:

```text
sampleEnvironmentIrradiance(world_position, normal) -> linear RGB irradiance
```

Terrain primary hits and raster vertices must use the same grid lookup, interpolation rules, SH
ordering, clamping policy, and environment revision.

## Probe Placement and Validity

A regular grid will place many probes inside terrain, especially below the playable surface. Each
probe therefore needs explicit state rather than assuming every grid point is usable:

- inactive or outside the useful world region;
- inside solid;
- relocation pending;
- valid;
- dirty;
- updating;
- relocation failed.

The first implementation should classify occupancy deterministically. A probe inside solid may be
relocated within a bounded portion of its grid cell to a nearby empty location. The original grid
coordinate and the actual sampling position must both remain inspectable. Probes that cannot be
relocated safely should stay invalid and must not contribute to interpolation.

Invalid probes may reduce effective density near surfaces. Density comparisons must therefore report
both total and valid probe counts rather than only the nominal grid dimensions.

## Interpolation and Light-Leak Policy

Plain trilinear interpolation is insufficient near walls and portals because an exterior probe can
contribute to an interior surface on the other side of thin terrain.

The shared sampler should be developed in stages:

1. trilinear position weights over valid neighbours;
2. surface-normal and probe-direction weighting;
3. probe-to-surface visibility or directional hit-distance weighting;
4. confidence and backface rejection;
5. stable fallback to global SH when no trustworthy local probe contribution exists.

The roof, side walls, deep chamber, portal, and matching plinths in the test scene are explicit
light-leak tests. Raising density alone is not an accepted substitute for visibility-aware
interpolation.

## Visualization

Probe visualization is a first-class development feature and should be available before real probe
tracing affects scene lighting.

Render probe markers using a dedicated instanced billboard or small diamond path:

- one compact marker per visible probe;
- one draw call where practical;
- probe state and display color read directly from GPU probe data;
- no full-volume CPU readback;
- depth testing and marker size controlled by the debug view;
- no draw submission or visualization-only compute work while disabled.

Planned visualization modes:

- **State:** valid, inactive, inside-solid, dirty, updating, and relocation-failed colors;
- **Sky visibility:** grayscale or heat-map encoding of visible-environment fraction;
- **Irradiance:** tonemapped local SH evaluated for a fixed world-up normal;
- **Revision:** deterministic environment-revision coloring;
- **Relocation:** paired original-grid and actual-sample markers, with different size and color.

Planned filters:

- all probes, which is the default view and requires no selection;
- valid only;
- invalid only;
- dirty or updating only;
- camera-radius limit;
- instance stride/downsampling;
- marker size;
- depth-tested versus always-visible markers.

Filters are volume-level display and cost controls, not a probe-selection mechanism. No marker is
interactive and the renderer does not maintain a selected probe.

The interaction contract is intentionally read-only and volume-wide: enabling the debug view shows
the probe field immediately. State, sky visibility, irradiance, revision, and relocation information are
encoded by marker color, size, or paired positions. There is no click, hover, selection,
single-probe inspector, or selection-dependent rendering path.

The renderer must record the visualization pass separately in GPU profiling. Production performance
comparisons run with visualization disabled; its enabled debug cost is measured and reported
separately.

## Update and Invalidation Policy

### Initialization and density rebuild

- Allocate the complete finite grid.
- Classify and relocate probes.
- Trace and derive local irradiance using a bounded work budget.
- Use global SH as a fallback until local probe data is trustworthy.
- Expose progress and counts in logs and the debug panel.

Resource replacement must respect frames in flight. A density change is an explicit rebuild, not an
in-place reinterpretation of old probe data.

### Environment revision

- Reuse retained terrain visibility or hit-distance data.
- Re-evaluate the authored environment and reproject local SH without terrain rays.
- Update all affected probes in the same frame if release measurements show the work is affordable.
- Do not temporally smooth deterministic sky-color changes by default.

### Terrain revision

A terrain edit can theoretically change a long visibility path, so a fixed dirty radius is not
always correct. Begin with a conservative policy:

- immediately prioritize probes near the edited bounds and the camera;
- schedule a bounded full-volume visibility refresh for correctness;
- visualize dirty, updating, and valid transitions;
- record time to local response and time to full convergence.

Only replace the conservative refresh with dependency-based invalidation after measurements show
that full refresh work or convergence time is a material problem.

Probe updates may be budgeted or accumulated in probe space, but the final screen image must not
depend on a reintroduced screen-space RGB denoiser. Deterministic direction sets, revision tracking,
confidence, and completed-set swaps should be preferred over noisy per-frame random replacement.

## Implementation Phases

### Phase 1: Grid, density, and resource accounting

- Define world-to-grid and grid-to-world transforms.
- Add supported spacing validation and deterministic CLI parsing.
- Add runtime discrete density selection with explicit rebuild.
- Allocate state and coefficient resources.
- Log grid dimensions, counts, valid counts, and GPU byte totals.
- Add pure tests for grid endpoints, flattening, interpolation coordinates, and invalid spacing.

### Phase 2: Visualization

- Add the instanced marker renderer.
- Add state, filtering, radius, stride, depth, and marker-size controls.
- Measure visualization-off and visualization-on GPU/CPU cost.

### Phase 3: Shared probe sampling with global-copy data

- Fill valid probes with the existing global SH coefficients.
- Route terrain and raster consumers through the position-aware grid sampler.
- Confirm that the global-copy mode does not materially change the current image.
- Validate full-resolution and LOD flora/leaves through the same sampler.

### Phase 4: Deterministic terrain visibility

Implement this phase as small, independently validated commits:

1. **Visibility resource contract**
   - add a fixed, indexable 64-direction deterministic full-sphere set;
   - retain compact first-hit distances, with a reserved miss representation;
   - keep the direction/SH projection table separate from per-probe visibility;
   - report coefficient, state, visibility, direction-table, and total allocation bytes.
2. **Occupancy classification and relocation**
   - classify outside, empty, and inside-solid grid positions from terrain data;
   - relocate inside-solid probes deterministically within a bounded part of their grid cell;
   - keep failed probes invalid and expose original versus relocated positions in the existing
     visualization modes.
3. **Visibility tracing and SH derivation**
   - trace the fixed directions through the existing terrain traversal;
   - write first-hit or miss information without random per-frame sampling;
   - evaluate the authored environment on visible directions, exclude the explicit sun, and derive
     local L2 irradiance SH.
4. **Bounded update scheduling**
   - update a bounded, observable probe batch when full-volume work does not fit the frame budget;
   - track dirty, updating, valid, environment revision, and terrain revision state;
   - keep global SH fallback active until each local sample is trustworthy;
   - confirm identical output and state counts across repeated hidden runs.

### Phase 5: Leak-resistant interpolation

- Add validity, normal, direction, distance, and confidence weighting as required.
- Validate the roofed/open plinth comparison.
- Reject wall, roof, portal-halo, and invalid-probe leaks.
- Use the nearest trustworthy local probe when directional weights reject every neighbour, and
  retain global SH only when no usable local probe exists.

### Phase 6: Revisions and terrain editing

- Rebuild local irradiance without terrain rays after an environment revision.
- Mark and refresh probe visibility after terrain edits.
- Extend the deterministic test scenario with a scripted roof opening/closure or equivalent edit.
- Validate local response, full convergence, and absence of stale probe lighting.

### Phase 7: Density and performance decision

- Compare at least 32-, 16-, and 8-voxel spacing where resource limits permit.
- Measure normal rendering, initialization, environment refresh, terrain refresh, and memory.
- Measure debug visualization separately.
- Select and document the production default from release evidence.
- Retain other supported densities as development/quality options only if their maintenance and
  resource costs remain reasonable.

## Validation

All automated application validation for this work must run in hidden mode. Use `--mute` unless
audio is explicitly under test, and capture visualization screenshots from hidden runs.

The normal Rust/shader validation ladder is:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --hidden --tail-latest-log 200
```

The deterministic probe scene should use:

```bash
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 32 \
  --screenshot player-default target/environment-probes-32.png \
  --screenshot-delay 2 \
  --auto-exit 10
```

Add `--environment-probe-visualization` to capture the current probe grid. Screenshot readiness
must eventually wait for the requested probe state in addition to terrain rebuild completion.

Visual and logged acceptance:

- the roofed plinth is darker than the matching open-sky plinth;
- irradiance darkens stably from the portal toward the chamber back wall;
- no bright halo or discontinuity appears at the portal;
- no environment irradiance leaks through the roof or side walls;
- terrain and the retained raster tree use consistent environment hue;
- time-of-day changes are not delayed by visibility retracing;
- terrain edits produce visible and logged dirty-to-valid transitions;
- repeated hidden runs at one configuration produce stable results;
- invalid or unavailable local samples fall back safely to global SH.

Release performance evidence must include:

- `frame.render` and `tracer.render`;
- probe lookup cost in terrain and raster paths where measurable;
- probe initialization and update dispatch timings;
- allocated bytes by probe resource;
- update rays and probes per frame;
- time to local response and full convergence after terrain edits;
- visualization disabled cost;
- visualization enabled cost at the same density and camera.

## Local Lights and ReSTIR Boundary

Local direct lights remain a later, separate feature. They should begin with a common light buffer,
chunk/tiled/clustered candidate lists, and explicit shadow tiers shared by terrain and raster
consumers. Moving local lights must not be hidden inside a slowly converging environment probe
field.

ReSTIR DI should be evaluated only after release measurements show that many simultaneous local
light candidates or their visibility queries exceed the frame budget. It does not replace probe
visibility, local-light lists for raster vegetation, or an explicit foliage-shadow policy.

## Commit and Completion Checklist

Each implementation phase should remain a small, independently validated commit before the next
phase begins.

- [x] Define probe spacing, grid transforms, state, and resource accounting.
- [x] Add CLI density control and explicit runtime rebuild control.
- [x] Add deterministic grid and interpolation-coordinate tests.
- [x] Visualize all probes with low-cost instanced markers.
- [x] Add visualization modes, filters, and separate GPU timing.
- [x] Route terrain and raster lighting through global-copy probe sampling.
- [x] Allocate and report deterministic direction and per-probe visibility resources.
- [x] Classify empty/outside/solid probes and relocate solid probes deterministically.
- [x] Trace deterministic terrain visibility from valid probes.
- [x] Derive spatially varying local SH with bounded, repeatable update scheduling.
- [x] Recompute local SH on environment revisions without terrain retracing.
- [x] Reject wall, roof, portal, and invalid-probe light leaks.
- [x] Add conservative terrain-edit invalidation and convergence tracking.
- [x] Extend the test scenario with a deterministic probe invalidation edit.
- [x] Report exact aggregate probe-state counts without full-volume readback.
- [x] Compare 32-, 16-, and 8-voxel density in hidden release runs.
- [x] Select the default density from image, timing, and memory evidence.
- [x] Confirm the main RGB denoiser remains absent.
- [x] Confirm VSM, leaf-shadow, cloud, and cloud-shadow histories remain independent.
- [x] Run formatting, checks, tests, hidden muted release validation, and log inspection.
- [x] Document final resource layout, update cost, visualization cost, and known limitations.

### Phase 1 Evidence

The initial resource layout uses one 144-byte RGB L2 SH record and one 64-byte state/summary record
per probe. At 16-voxel spacing, the hidden release run allocated a `33 x 33 x 33` grid with 35,937
probes: 5,174,928 coefficient bytes plus 2,299,968 summary bytes, or 7.13 MiB total. Probes remain
inactive in this phase, so the valid count is intentionally zero until placement/global-copy work.

The CLI accepts only 64, 32, 16, or 8 voxels. The non-persisted debug control previews the selected
grid, probe count, and allocation, then replaces resources only after **Apply / Rebuild**. The
validated phase used:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute \
  --environment-probe-spacing-voxels 16 --auto-exit 0.5
cargo run --release -- --hidden --tail-latest-log 200
```

### Phase 2 Evidence

The visualization reads SH and summary records directly from the GPU probe buffers and submits one
indexed instanced diamond draw. It performs no full-volume readback. The debug panel exposes state,
sky-visibility, irradiance, age/revision, and relocation views; valid/invalid/update filters; camera
radius; instance stride; marker size; and depth-tested or always-visible rendering. Disabled mode
does not submit the draw or enter the independent `graphics.environment_probes` GPU scope.

A hidden 2560 x 1440 release capture of the 16-voxel grid rendered all 35,937 inactive phase-1
probes and saved `target/environment-probes-visualization.png`. Across 23 steady-state GPU samples
from matched eight-second runs, the enabled probe scope averaged 8.70 us. The full
`frame.render`/`tracer.render` averages were 5,293.83/2,978.61 us with visualization and
5,325.48/2,977.22 us without it, so the whole-frame delta was below single-run noise. Tracked CPU
time averaged 0.127 ms enabled versus 0.044 ms disabled; total host frame time was 8.265 ms enabled
versus 8.424 ms disabled and likewise did not show an actionable regression.

The phase was validated with:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 16 \
  --environment-probe-visualization \
  --screenshot player-default target/environment-probes-visualization.png \
  --screenshot-delay 4 --auto-exit 8 --perf
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 16 \
  --auto-exit 8 --perf
```

### Phase 3 Evidence

An environment-revision-triggered GPU pass copies the current global L2 SH coefficients into every
probe and marks the complete grid valid without a full-volume CPU upload. The 16-voxel hidden run
reported revision 1 and 35,937/35,937 valid probes. A compute-to-compute/vertex barrier makes the
new coefficients visible to terrain and raster consumers in the same command buffer; a preceding
read-to-write barrier protects replacement while earlier queued consumers may still be reading.

Terrain, full-resolution and LOD flora/leaves, sprinklers, dynamic fruit, and particles now share
the position-and-normal probe sampler. Global-copy mode uses a uniform-field fast path: it selects
and validates the nearest probe at the requested world position and reads nine SH coefficients.
The eight-neighbor interpolation path is retained for the later spatially varying field. Missing,
invalid, or environment-revision-mismatched data falls back to the uniform global SH immediately.

The roof/chamber terrain crop from matched before/after hidden screenshots measured SSIM 0.998584
and PSNR 59.25 dB; remaining differences were subpixel-level. The first implementation
unnecessarily loaded eight identical probes and raised `frame.render` from 5,325.48 to 6,136.59 us.
The uniform-field fast path reduced the matched 23-sample average to 5,403.17 us. Its internal
`tracer.render`/`graphics.pass` scopes measured 3,182.13/2,407.26 us versus the direct-global
baseline's 2,977.22/2,141.04 us. Phase 7 must remeasure and optimize the truly local interpolation
path; the global-copy bridge itself is not assumed free.

The phase used the normal formatting/check/test ladder and:

```bash
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 16 \
  --screenshot player-default target/environment-probes-global-copy-fast.png \
  --screenshot-delay 4 --auto-exit 8 --perf
```

### Phase 4 Visibility Resource Evidence

The visibility resource contract uses a deterministic, directly indexable `8 x 8` octahedral
full-sphere direction set. Each direction record stores its unit vector, nominal solid angle, and
nine scalar L2 irradiance projection weights. Nonnegative-Y records participate in authored-sky SH
projection; all 64 records retain terrain hit distance for future directional uses. The shared
direction table is 10,240 bytes. Each probe stores 128 bytes containing 64 packed `u16` first-hit
distances plus 256 bytes containing 64 packed `(mean distance, mean squared distance)` pairs. Two
`u16::MAX` hit distances explicitly represent environment misses. A moment pair initialized to
`u32::MAX` represents maximum distance and maximum squared distance before the first trace.

For the hot consumer path, a sample reads and bilinearly blends four packed moment pairs rather than
turning four raw hit distances into independent binary visibility decisions. Six additional exact
axial distances remain quantized to ten bits in two previously reserved words of the 64-byte
summary; they add no allocation bytes.

Together with the existing 144-byte coefficient and 64-byte summary records, the current layout is
592 bytes per probe plus the fixed 10 KiB direction table and a 64-byte aggregate
statistics/readback pair. Hidden release allocation runs reported:

| Spacing | Probes | Coefficients | State | Visibility | Directions | Stats | Total |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 32 voxels | 4,913 | 707,472 B | 314,432 B | 1,886,592 B | 10,240 B | 64 B | 2,918,800 B / 2.78 MiB |
| 16 voxels | 35,937 | 5,174,928 B | 2,299,968 B | 13,799,808 B | 10,240 B | 64 B | 21,285,008 B / 20.30 MiB |
| 8 voxels | 274,625 | 39,546,000 B | 17,576,000 B | 105,456,000 B | 10,240 B | 64 B | 162,588,304 B / 155.06 MiB |

All allocations completed on the release Vulkan path, seeded all probes from global SH, reported
no error, panic, or validation message, and exited successfully. This validates the resource layout
and stress allocation only; it is not yet evidence for tracing cost or an 8-voxel production
default.

The step used:

```bash
cargo check
cargo test
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 16 --auto-exit 0.5
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 8 --auto-exit 0.5
cargo run --release -- --hidden --tail-latest-log 200
```

### Phase 4 Placement Classification Evidence

A dedicated one-thread-per-probe compute pass now classifies placement directly from the terrain
voxel atlas. Grid endpoints outside the atlas remain inactive. An original point in an empty voxel
uses that voxel's center as its sample position and becomes dirty. A point inside solid terrain
searches deterministically along `+Y`, `+X`, `-X`, `+Z`, `-Z`, then `-Y`, up to half of the probe
spacing. A relocation candidate must be empty with one empty voxel of axial clearance; failure
leaves the probe invalid in the relocation-failed state.

The pass records original and sample positions, relocation distance, placement completion, state,
and environment revision in the existing summary record. It does not make a classified probe
usable: dirty and invalid probes continue to fall back to global SH until visibility tracing
completes. Later global-copy refreshes preserve classified placement state instead of accidentally
marking every probe valid again.

The deterministic test scene requests another classification after its terrain edits and rebuilds
complete. A hidden 16-voxel run classified all 35,937 records once for the startup world and again
after the gallery edit. `target/environment-probes-classified.png` shows the full volume with dirty,
inactive, and relocation-failed state colors. The release log contained no error, panic, or Vulkan
validation message and the application exited successfully.

The step used:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 16 \
  --environment-probe-visualization \
  --screenshot player-default target/environment-probes-classified.png \
  --screenshot-delay 4 --auto-exit 8 --perf
```

### Phase 4 Visibility Tracing and Local SH Evidence

A compute pass now traces the fixed 64-direction full-sphere set plus six exact axial directions
through the existing terrain contree for every dirty, validly placed probe. It stores quantized
first-hit distances, reconstructs the authored environment radiance from the current global L2
coefficients, and projects only visible nonnegative-Y directions back into the existing
irradiance-SH convention. A quadrature correction preserves the exact global SH field for a fully
open probe. The explicit sun remains outside this pass.

The scheduler processes 128 probe records per frame. Consumers stay in uniform global-SH fallback
until the full set completes, avoiding a moving boundary between local and fallback lighting during
initialization. Screenshot readiness also waits for the local field. At 16-voxel spacing, the
post-edit test volume completed all 35,937 records in 2.35 seconds in the current 70-ray hidden run.
The sampled `environment_probes.trace` GPU scopes averaged 0.65 ms per active frame, with observed
samples from 0.05 ms to 1.09 ms on the Apple M4 Pro. These sparse 30-frame profiler samples describe
the current implementation but are not yet the final density comparison.

`target/environment-probes-local-sh.png` shows the complete probe volume after dirty probes become
valid. With markers disabled, `target/environment-probes-local-sh-clean.png` makes the roofed bay
substantially darker than the previous global-copy baseline while the open bay remains lit. The
roofed-building crop changed from the global-copy baseline with SSIM 0.932379. Repeating the same
hidden run produced the same processed count and a deep-chamber SSIM of 0.992634; remaining image
variation includes independent shadow histories and animated raster content.

This validates deterministic terrain visibility, spatially varying local SH, bounded scheduling,
and global fallback during convergence. It does not yet validate environment-only re-derivation
cost, terrain-edit invalidation, or a production density.

The step used:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 16 \
  --environment-probe-visualization \
  --screenshot player-default target/environment-probes-local-sh.png \
  --screenshot-delay 4 --auto-exit 8 --perf
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 16 \
  --screenshot player-default target/environment-probes-local-sh-clean.png \
  --screenshot-delay 4 --auto-exit 6 --perf
```

### Phase 5 Leak-Resistant Interpolation Evidence

The shared terrain and raster sampler now combines trilinear position, relocation confidence,
surface-normal orientation, and probe-to-surface hit-distance weights. Six exact axial hit
distances are packed into the existing summary, selected by probe-to-surface direction, and blended
continuously by squared direction components. Surface-normal weighting only accepts probes in the
surface's incident hemisphere, so a probe on the far side of a wall or roof cannot bleed through
with a small residual weight. If trilinear weights collapse, the fallback uses the nearest probe
that passed both the hemisphere and hit-distance tests. A surface with usable but fully rejected
neighbours receives zero local environment light instead of sampling an untrusted probe or
reintroducing bright global fill; global SH remains the fallback only when no local probe is valid.

An initial nearest-octahedral-direction implementation produced visible blocks on the chamber back
wall. Four-direction bilinear lookup removed those blocks but increased the post-convergence
`tracer.render` sample average from 3.74 ms to 4.99 ms. That version was rejected. The packed axial
version keeps the wall smooth and measured 4.18 ms in the same sparse profiler comparison, about
0.45 ms above basic trilinear interpolation and about 0.81 ms below the rejected bilinear version.
Final density benchmarking must repeat this comparison with matched longer captures.

`target/environment-probes-leak-resistant-axial.png` shows no bright roof, side-wall, or portal halo
at the deterministic acceptance camera. The roofed building remains substantially darker than the
open bay. Its crop has SSIM 0.989854 versus the pre-weighting local-SH image, confirming that the
weighting changes the intended boundary region without replacing the overall lighting result. The
run completed all 35,937 probes, saved the screenshot only after local-field readiness, reported no
error, panic, or Vulkan validation message, and exited successfully.

A later uniform-albedo regression scene exposed residual blue rectangular bands on the roofed
chamber's back wall and wall/floor boundaries. The six-axis approximation could still trust a probe
whose selected axial rays missed the intervening wall, even though the actual probe-to-surface
direction was blocked. The consumer therefore now reads the existing 64-direction visibility
record and bilinearly blends the four surrounding octahedral hit distances. This adds no tracing
rays and preserves the surface-hemisphere and trusted-fallback rules. In
`target/environment-lighting-directional-visibility.png`, the rectangular bands from the matching
uniform-albedo baseline are gone while the valid circular opening remains lit. The 32-voxel field
converged through terrain revision 3, the run reported no error, panic, or Vulkan validation
message, and the application exited successfully. Performance was intentionally not used as an
acceptance criterion for this correctness restoration and remains follow-up work.

Doubling density to 16-voxel spacing exposed a second failure in that raw-distance consumer: each
probe's visibility still changed too abruptly when a surface crossed its first-hit threshold, so
the interpolation weights revealed the probe lattice as large wall cells. Disabling only
directional visibility removed the cells, while a 4x4 angular reconstruction did not; this isolated
the discontinuous per-probe visibility field rather than SH projection, albedo, or octahedral
bilinear interpolation.

The update pass now filters the 64 retained ray distances into directional first and second moments.
For each 8x8 octahedral output direction it applies a cosine-to-the-eighth kernel, clamps ray distance
to 1.5 times the probe-cell diagonal, and stores normalized `u16` mean and mean-square values in one
`u32`. The lower exponent is intentional for this sparse 64-ray field; a narrow high-exponent kernel
would preserve the same undersampling bands. The shared sampler bilinearly interpolates the moments,
applies the existing two-voxel surface bias, and evaluates a cubed Chebyshev visibility bound with a
0.05 floor. It deliberately does not apply the reference implementation's later cubic small-weight
crush: an A/B capture showed that crush reintroduced visible cell boundaries at 16-voxel spacing.

`target/environment-lighting-spacing-16-distance-moments-no-crush.png` is smooth on the left and
right walls at the failure density while keeping the roofed back wall and portal boundary dark. The
default-density regression capture
`target/environment-lighting-spacing-32-distance-moments-no-crush.png` likewise keeps the original
leak fixed. Both runs reached the final roof-closure terrain revision before capture, reported no
error, panic, or Vulkan validation message, and exited successfully.

The step used:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 16 \
  --screenshot player-default target/environment-probes-leak-resistant-axial.png \
  --screenshot-delay 4 --auto-exit 7 --perf
```

### Phase 6 Environment-Revision Evidence

The deterministic test scene now waits for the initial local field, changes from time-of-day
`0.455705` to `0.535705` exactly once, and waits for the resulting environment revision before
becoming screenshot-ready. The revision pass reads the retained directional hit distances and
reprojects all valid local SH coefficients without calling terrain traversal.

At 16-voxel spacing, the hidden release run reprojected 35,937 probes in one
`environment_probes.rederive` dispatch measured at 490 us on the Apple M4 Pro. The log reported
revision `1 -> 2`, `terrain_rays=0`, and no new `visibility trace started` line after the
environment-change request. `target/environment-probes-environment-refresh.png` captured the
settled revision-2 field at 2560 x 1440. The run exited successfully with no error, panic, or Vulkan
validation message.

The step used:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 16 \
  --screenshot player-default target/environment-probes-environment-refresh.png \
  --screenshot-delay 4 --auto-exit 8 --perf
```

### Phase 6 Terrain-Revision Evidence

Every successful runtime `WorldEditPlan` containing a terrain mesh rebuild now unions its rebuild
bounds and requests a new probe terrain revision. Classification writes that revision into each
summary. In the edit frame, two `3 x 3 x 3` probe regions are traced first—one around the edited
bound and one around the camera—then the existing 128-probe-per-frame cursor performs the
conservative full-volume refresh. Once an initial local field exists, valid probes in those
priority regions are immediately available while dirty neighbours safely fall back; the full field
does not revert to the single global-copy lookup during the refresh.

The deterministic scene now opens a bounded skylight above the roofed plinth after the
environment-only refresh, waits for that terrain revision to converge, restores the roof, and waits
for the closure revision before declaring the scene ready. At 16-voxel spacing the initial gallery,
roof opening, and roof closure converged in 2.37 s, 2.45 s, and 2.39 s respectively. Each edited
revision scheduled 54 priority probes in 0.16–0.28 ms of CPU record time before the full 35,937-probe
refresh. The retained-partial-field flag was false for initial construction and true for both
post-convergence edits.

`target/environment-probes-terrain-refresh.png` was captured after roof restoration and terrain
revision 3 convergence. The roofed chamber returned to the expected dark result, the run exited
successfully, and the log contained no error, panic, or Vulkan validation message.

The step used:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 16 \
  --screenshot player-default target/environment-probes-terrain-refresh.png \
  --screenshot-delay 4 --auto-exit 13 --perf
```

### Aggregate Probe-State Evidence

An independent one-thread-per-probe GPU reduction now counts inactive, inside-solid,
relocation-pending, valid, dirty, updating, and relocation-failed records after each full
convergence. Only the seven aggregate `u32` counters are copied to a 32-byte CPU staging buffer;
the implementation does not read back individual probes and does not add picking or selection.
The debug panel exposes the resulting volume-wide counts beside the existing all-probe
visualization.

At 32-voxel spacing, the post-gallery, roof-opening, and roof-closure fields each summed exactly to
the 4,913-probe grid. The final closed-roof revision reported 817 inactive, 3,148 valid, and 948
relocation-failed probes, with every transient state at zero. The allocation log reported
1,661,072 bytes total, including 64 bytes for the GPU counters and staging buffer.
`target/environment-probes-state-counts.png` captured the visible complete field after exact terrain
revision 3 readiness. The hidden release run exited successfully with no error, panic, or Vulkan
validation message.

The step used:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 32 \
  --environment-probe-visualization \
  --screenshot player-default target/environment-probes-state-counts.png \
  --screenshot-delay 2 --auto-exit 7 --perf
```

### Phase 7 Density and Performance Evidence

Matched 2560 x 1440 hidden release runs on the Apple M4 Pro used the same deterministic camera,
time-of-day revision, gallery construction, roof opening, roof closure, and 128-probe update budget.
Probe visualization was disabled. Steady GPU samples begin only after exact roof-closure revision 3
readiness:

| Spacing | Final valid / total | Allocation | Gallery / open / close convergence | Environment rederive | Steady `frame.render` avg / median | Steady `tracer.render` avg / median |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 32 voxels | 3,148 / 4,913 | 1.58 MiB | 0.330 / 0.335 / 0.333 s | 184 us | 6,561.87 / 6,146 us (31 samples) | 4,233.77 / 4,339 us |
| 16 voxels | 24,991 / 35,937 | 11.53 MiB | 2.378 / 2.580 / 2.482 s | 863 us | 6,883.43 / 6,729 us (23 samples) | 4,510.83 / 4,592 us |
| 8 voxels | 199,060 / 274,625 | 88.01 MiB | 18.000 / 18.403 / 18.379 s | 2,532 us | 6,676.76 / 6,602 us (50 samples) | 4,368.82 / 4,431 us |

The steady lookup always considers the same eight neighbouring probes, so density did not produce a
monotonic frame-time trend; the observed spread is small compared with independent scene activity.
Density instead dominates allocation and full-volume convergence. Active 128-probe trace samples
had 464/570/465 us medians and 1,017/1,088/1,042 us p95 values at 32/16/8 spacing. The fixed
54-probe edit and camera priority work remained density-independent: CPU scheduling took
0.16–0.24 ms, and the two GPU priority dispatches together took about 0.92–1.77 ms.

`target/environment-probes-density-32.png`, `target/environment-probes-density-16.png`, and
`target/environment-probes-density-8.png` all pass the roof, side-wall, portal-halo, dark-gradient,
and matching-open-bay review. Dynamic particles and vegetation differ because denser runs reach
readiness later, so the density decision uses static terrain crops rather than full-frame SSIM. The
roofed-interior mean luma was 66.683, 65.847, and 65.795 at 32, 16, and 8 voxels respectively; the
32-to-8 difference is 0.888 on an 8-bit scale. The roofed back-wall difference was 0.757.

The selected production default remains 32 voxels. Compared with 8 voxels, it uses about one
fifty-fifth of the memory and converges about fifty-five times faster without a material acceptance
image benefit from the denser field. Sixteen voxels remains a quality/debug option. Eight voxels is
a stress option. Sixty-four voxels remains supported as a coarse resource/debug option but is not
acceptance-qualified as the production density.

At the selected 32-voxel density, a matched visualization-enabled run measured
`graphics.environment_probes` at 8.24 us average across 21 steady samples. Whole-frame averages were
6,505.38/4,146.95 us for `frame.render`/`tracer.render` with visualization and
6,561.87/4,233.77 us without it; that reversed whole-frame delta is measurement noise rather than a
speedup. Disabled mode submits no marker draw.

The density runs used:

```bash
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 32 \
  --screenshot player-default target/environment-probes-density-32.png \
  --screenshot-delay 2 --auto-exit 10 --perf

cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 16 \
  --screenshot player-default target/environment-probes-density-16.png \
  --screenshot-delay 2 --auto-exit 14 --perf

cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --environment-probe-spacing-voxels 8 \
  --screenshot player-default target/environment-probes-density-8.png \
  --screenshot-delay 2 --auto-exit 68 --perf
```

### Completion Audit

The terrain tracer now writes deterministic SH-lit RGBE directly to `compute_output_tex`, and the
composition pass immediately unpacks that value as terrain color. `ComputePipelines` has no main
radiance temporal or A-Trous pipeline, and tracer resources contain no main RGB history or spatial
ping-pong textures. The retained `--denoiser-bench` name is a compatibility name for raw
frame-stability capture, not a normal rendering pass.

Terrain, full-resolution and LOD flora, full-resolution and LOD leaves, dynamic fruit, sprinklers,
and particle vertices all reach the same `sampleEnvironmentIrradiance(world_position, normal)`
contract through `applyStylizedVoxelLighting`. Grass and rapidly moving leaves therefore receive
position-dependent environment light while remaining in the raster pipeline and outside probe
occlusion tracing.

The temporal systems that remain are direct-effect histories, not a screen-space RGB denoiser:

- VSM owns `shadow_map_tex_for_vsm_prev` and `shadow_map_history_valid`;
- leaf opacity owns `leaf_shadow_opacity_prev_tex` and `leaf_shadow_history_valid`;
- cloud color owns `cloud_history_tex` and `cloud_history_valid`;
- cloud shadow owns `cloud_shadow_history_tex` and `cloud_shadow_history_valid`.

Each has its own pass, texture, validity flag, copy, and reset path. Probe updates do not read or
write any of these histories.

The 32-voxel local field is not faster than the uniform global-SH bridge: the earlier matched bridge
averaged 5,325.48/2,977.22 us for `frame.render`/`tracer.render`, so local visibility currently adds
about 1.24/1.26 ms. It remains substantially below the historical stochastic-second-ray plus main
denoiser baseline, whose medians were 13,465/10,721 us; the current 32-voxel medians are
6,146/4,339 us. The historical comparison spans an evolved scene and should be treated as
architectural context, while the three density rows above are the matched production decision.

### Runtime DDGI Terrain-Edit Relocation

This subsection is the current DDGI status and supersedes the archived local-SH implementation
evidence earlier in this document. In particular, current runtime invalidation does not use global
SH, nearest-valid fill, local priority regions, or a partially trusted active field.

Startup classification and voxel-native relocation still run only after initial terrain is ready.
Runtime terrain edits are now supported by a correctness-first full-volume staging rebuild. The
edited world domain returns strict-zero environment irradiance until the exact latest terrain
revision reaches Ready; promotion
then switches terrain and raster consumers to one immutable build token and revision. Edits during
a build obsolete the older candidate, while density changes remain queued behind terrain work.

- [x] Reclassify, relocate, retrace, and atomically promote runtime terrain edits at spacing 32 and
  16, including sequential edits and latest-revision-wins replacement.
- [ ] After measurement shows a need, add dependency-exact invalidation and partial-volume refresh
  without weakening full-domain correctness, token identity, or atomic consumer promotion.

### Known Limitations

- The field represents single-bounce authored environment visibility. It does not provide terrain
  color bleeding, emissive bounce lighting, direct local lights, or their shadows.
- Animated flora and leaves consume the field but are intentionally not probe occluders. Their
  direct leaf-shadow temporal filter remains separate and can still need responsiveness tuning for
  fast motion.
- Terrain edits use conservative full-domain fail-closed invalidation and a full-volume staging
  rebuild. Dependency-exact/local refresh remains a performance optimization, and spacing 8 is not
  runtime-edit qualified.
- Relocation-failed probes remain invalid and contribute zero; the current query does not substitute
  nearest-valid or global-SH lighting. Narrow geometry below the 32-voxel sampling scale can still
  justify a temporary 16-voxel quality run.
- Runtime progress is reported as active-to-target revision/token identity, staging stage and
  filtered-probe progress, coordinator state, queued density, and full-domain fail-closed state.
  It does not use the archived local-SH aggregate/all-dirty model or per-frame full-volume readback.
- Probe spacing changes rebuild the complete finite volume explicitly. Runtime paging,
  camera-relative scrolling, dependency-exact invalidation, local direct lights, and ReSTIR DI
  remain future work.

### Final Validation

The completed branch passed:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --latest-log
```

The final test run passed 271 main-binary tests with one ignored release microbenchmark and all four
collision-benchmark tests. The final hidden release run used the selected 32-voxel default,
converged all 4,913 records to an exact aggregate count of 817 inactive, 3,148 valid, and 948
relocation-failed probes, exited successfully, and contained no error, panic, or Vulkan validation
message. Its log is
`target/re-flora-logs/re-flora-20260729-181631.771-65270.log`. The only build warning remains the
pre-existing unused `fs` import in `src/bin/collision_bench_rapier.rs`.
