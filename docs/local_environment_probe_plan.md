# Local Environment Probe Plan

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

Add a fixed-density, spatially varying environment probe volume that:

1. reduces environment light in terrain-occluded regions without restoring a per-pixel diffuse
   second ray;
2. supplies the same local SH irradiance to terrain and animated raster consumers;
3. exposes a runtime-adjustable probe spacing with deterministic CLI control;
4. visualizes probe placement, state, and useful lighting summaries at controllable debug cost;
5. allows one probe to be selected for detailed inspection;
6. updates predictably after terrain and environment revisions;
7. remains deterministic enough that it does not require a screen-space RGB denoiser;
8. records enough release-mode timing and memory evidence to select a default density.

## Non-goals

- Do not add animated flora or leaves to terrain voxel traversal.
- Do not treat probes as a replacement for direct sun, local direct lights, or their shadows.
- Do not implement ReSTIR in this phase.
- Do not implement multi-bounce GI or terrain color bleeding in the first visibility-probe step.
- Do not restore the removed main RGB temporal or A-Trous denoiser.
- Do not draw text, ray sets, or SH lobes for every probe simultaneously.
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

The initial quality hypothesis is that 16-voxel spacing will resolve the test chamber and portal
more reliably, while 32-voxel spacing may be a materially cheaper production choice. The default
must be selected by matched release measurements and visual comparison; 8-voxel spacing should
remain a stress/quality candidate unless it proves affordable.

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

## Visualization and Inspection

Probe visualization is a first-class development feature and should be available before real probe
tracing affects scene lighting.

### All-probe view

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
- **Age/revision:** stale-to-current update coloring;
- **Relocation:** original grid point plus an offset line to the actual probe position.

Planned filters:

- all probes;
- valid only;
- invalid only;
- dirty or updating only;
- camera-radius limit;
- instance stride/downsampling;
- marker size;
- depth-tested versus always-visible markers.

The renderer must record the visualization pass separately in GPU profiling. Production performance
comparisons run with visualization disabled; its enabled debug cost is measured and reported
separately.

### Selected-probe view

Displaying labels and detailed lobes for every probe would be unreadable and unnecessarily
expensive. Instead, the camera crosshair or mouse should select one probe for a detailed panel.

The selected-probe panel should expose, where available:

- grid coordinate and flat index;
- original and relocated world positions;
- current state, confidence, and validity reason;
- environment and terrain revisions;
- dirty/update age and last updated frame;
- visible-direction count or fraction;
- hit-distance summary;
- neighbouring probes and final interpolation weights for an inspected surface sample;
- the nine RGB SH coefficients.

Optional selected-probe geometry may show:

- deterministic trace directions;
- environment misses and terrain hits in different colors;
- first-hit distances;
- the reconstructed SH lobe.

Only the selected probe should request detailed directional visualization or a small asynchronous
readback. The normal all-probe view should rely on compact GPU-resident summary data.

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

### Phase 2: Visualization and selection

- Add the instanced marker renderer.
- Add state, filtering, radius, stride, depth, and marker-size controls.
- Add deterministic CPU or analytic probe selection.
- Add the selected-probe information panel.
- Measure visualization-off and visualization-on GPU/CPU cost.

### Phase 3: Shared probe sampling with global-copy data

- Fill valid probes with the existing global SH coefficients.
- Route terrain and raster consumers through the position-aware grid sampler.
- Confirm that the global-copy mode does not materially change the current image.
- Validate full-resolution and LOD flora/leaves through the same sampler.

### Phase 4: Deterministic terrain visibility

- Add fixed probe direction generation.
- Trace terrain visibility and first-hit information.
- Derive local environment irradiance without the explicit sun.
- Visualize valid, hit, miss, confidence, and update state.
- Confirm repeatable output across identical hidden runs.

### Phase 5: Leak-resistant interpolation

- Add validity, normal, direction, distance, and confidence weighting as required.
- Validate the roofed/open plinth comparison.
- Reject wall, roof, portal-halo, and invalid-probe leaks.
- Retain a stable global-SH fallback for underspecified samples.

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
  --environment-probe-spacing-voxels 16 \
  --screenshot player-default target/environment-probes-16.png \
  --screenshot-delay 4 \
  --auto-exit 8
```

The exact CLI is planned and becomes authoritative only after implementation. Screenshot readiness
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

- [ ] Define probe spacing, grid transforms, state, and resource accounting.
- [ ] Add CLI density control and explicit runtime rebuild control.
- [ ] Add deterministic grid and interpolation-coordinate tests.
- [ ] Visualize all probes with low-cost instanced markers.
- [ ] Add visualization modes, filters, and separate GPU timing.
- [ ] Add selected-probe picking and detailed information.
- [ ] Route terrain and raster lighting through global-copy probe sampling.
- [ ] Trace deterministic terrain visibility from valid probes.
- [ ] Recompute local SH on environment revisions without terrain retracing.
- [ ] Reject wall, roof, portal, and invalid-probe light leaks.
- [ ] Add conservative terrain-edit invalidation and convergence tracking.
- [ ] Extend the test scenario with a deterministic probe invalidation edit.
- [ ] Compare 32-, 16-, and 8-voxel density in hidden release runs.
- [ ] Select the default density from image, timing, and memory evidence.
- [ ] Confirm the main RGB denoiser remains absent.
- [ ] Confirm VSM, leaf-shadow, cloud, and cloud-shadow histories remain independent.
- [ ] Run formatting, checks, tests, hidden muted release validation, and log inspection.
- [ ] Document final resource layout, update cost, visualization cost, and known limitations.
