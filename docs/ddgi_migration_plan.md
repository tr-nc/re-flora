# DDGI Migration Plan

## Status

This document records the approved design baseline for replacing the branch's local spherical-
harmonic probe field with a paper-based Dynamic Diffuse Global Illumination implementation. The
first milestone is deliberately **sky-only DDGI**: it must make authored environment lighting and
probe-to-surface visibility correct before adding indirect hit radiance, temporal convergence, or
production compression.

Implementation progress:

- M0 is complete: the three static cases, fail-closed backend selector/readiness seam, pre-albedo
  float32 irradiance capture, deterministic capture analyzer, and six-configuration runner skeleton
  are in-tree.
- M1 is complete: the deep host module, exact grid/atlas addressing, full-precision atlases,
  batch-bounded transient ray storage, octahedral reference/gutter tests, and GPU-authored global
  sky irradiance map are in-tree.
- M2 is complete: initialization is explicitly scheduled after final static terrain, GPU relocation
  uses deterministic nearest-safe placement within the nominal cage, and the bounded trace path
  records 256 deterministic full-precision radiance/signed-distance samples per valid probe. GPU
  readback counters verify the ray partition and reject non-finite records. The first batch remains
  owned by the scheduler until M3 filtering consumes it.
- M3 is complete: separate full-precision atlas filters and gutters, the visibility-aware terrain
  query, exact segment reference, permanent ablation views, and the six-configuration acceptance
  runner are in-tree. The selected DDGI backend remains fail-closed until the complete volume is
  ready.
- M4 is complete: terrain, full/LOD flora and leaves, fruit, sprinklers, particles, and water
  droplets all consume `sampleDiffuseEnvironment` with their existing world position and
  procedural normal. Every affected raster pipeline binds the same DDGI metadata, global sky,
  irradiance atlas, visibility atlas, and shared shading-info revision as terrain; vertex layouts
  are unchanged.
- M5 is complete: DDGI is the sole environment-lighting backend, the local/global SH resources,
  shaders, sampling path, selector, and legacy tests are gone, and probe visualization now reads
  DDGI metadata and atlases directly. The post-removal correctness suite passed all six
  configurations with bit-exact repeats.
- The runtime-terrain-edit milestone is complete: active and staging volumes carry immutable build
  tokens, latest-revision terrain requests obsolete older candidates, density requests remain
  queued behind terrain correctness, and promotion switches every terrain/raster consumer to one
  complete token and terrain revision. One physical staging update runs at a time, and the last
  complete active field remains consumer-visible while its replacement builds; dependency-exact
  refresh is deliberately deferred as an optimization.

The canonical terms are defined in the root [rendering glossary](../CONTEXT.md). In particular,
DDGI still uses probes. The migration replaces each probe's SH representation with directional
octahedral maps; it does not remove the probe volume.

## Outcome

The environment-lighting path now contains:

- a fixed world-aligned DDGI volume;
- one octahedral irradiance map and one octahedral visibility map per probe;
- a single global sky irradiance map for positions outside a ready volume;
- one shared `sampleDiffuseEnvironment(worldPosition, surfaceNormal)` shader seam for terrain and
  raster consumers;
- no local or global spherical-harmonic environment-lighting representation;
- direct sun and its shadow paths outside DDGI.

The temporary local-SH backend was retained only for deterministic A/B testing through M4. M5
removed it after DDGI passed the agreed acceptance suite; the global sky irradiance map is now the
only outside-volume and not-ready fallback.

## Paper Baseline

The implementation uses the papers as a progression rather than freezing the original 2019
algorithm:

1. [Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields](references/ddgi/majercik-2019-ddgi.md)
   defines the core probe field and visibility-aware eight-probe query.
2. [Scaling Probe-Based Real-Time Dynamic Global Illumination for Production](references/ddgi/majercik-2021-scaling-ddgi.md)
   is the main implementation baseline for atlas layout, query bias, backface handling, relocation
   constraints, and the production update structure.
3. [Improving Probes in Dynamic Diffuse Global Illumination](references/ddgi/rohacek-2022-improving-probes-ddgi.md)
   supplies relocation-aware interpolation and probe-cage distance-support rejection.

The archived PDFs, complete author lists, source links, and the voxel-engine integration video are
indexed in the [DDGI reference set](references/ddgi/README.md).

Paper formulas and invariants are authoritative starting points. Numerical constants are not copied
blindly: bias, distance support, and filtering values must be expressed in Re: Flora world/voxel
units and validated against the exact-visibility reference. For example, a spacing-relative paper
bias can exceed the thickness of a one- or two-voxel wall and create the very leak it is intended to
avoid.

## Deep Module and Seams

DDGI is implemented as the environment-lighting deep module:

```text
ddgi
├── volume transform, probe grid, state, and readiness
├── GPU classification and voxel-native relocation
├── batched probe-ray tracing
├── transient ray data
├── irradiance atlas update
├── visibility atlas update
├── octahedral border update
├── visibility-aware surface query
├── global sky irradiance map
└── correctness and visualization modes
```

Likely source ownership is `src/ddgi/` for host-side resource and scheduling implementation and
`shader/slang/ddgi_*.slang` for GPU implementation. Exact filenames may evolve, but atlas
addressing, state transitions, tracing, filtering, and query rules must remain local to the module.
The tracer should request high-level initialization/update work and observe status; callers must not
learn atlas coordinates, probe-state encodings, or filtering rules.

The shader seam remains intentionally small:

```text
sampleDiffuseEnvironment(worldPosition, surfaceNormal) -> linear RGB irradiance
```

The first milestone keeps one normal per voxel. Terrain already supplies the ray-marched surface
normal. Flora, leaves, fruit, sprinklers, and particles may reuse their existing procedural shading
normal when they later become DDGI consumers. A separate geometric/visibility normal will be added
only if a concrete consumer demonstrates that the distinction is necessary.

## First-Milestone Scope

### Included

- One finite, fixed, world-aligned DDGI volume.
- Spacing 32 as the default and spacing 16 as the high-density correctness regression.
- Static voxel terrain as the only DDGI occluder.
- One GPU classification/relocation pass after the initial terrain reaches its final state.
- Deterministic full probe rebuilds with 256 rays per probe and zero hysteresis.
- Direct sampling of the authored GPU sky on probe-ray misses.
- Full-precision paper-style irradiance and visibility atlases.
- Terrain consumption first, followed by vertex-level raster consumption.
- Exact segment visibility, permanent debug views, and automated linear-irradiance acceptance.

### Explicitly Deferred

- Dependency-exact invalidation and partial-volume terrain refresh.
- Random ray rotation, temporal hysteresis, adaptive convergence, and sleeping states.
- Compressed atlas formats and perceptual irradiance encoding.
- Indirect hit radiance, DDGI feedback, terrain color bleeding, and multi-bounce lighting.
- Dynamic or raster geometry as DDGI occluders.
- Camera-tracking volumes, cascades, probe paging, and volume blending.
- Formal spacing-8 qualification; spacing 64 remains a coarse debug option.
- Per-fragment raster DDGI sampling.
- Separate geometric and shading normals.

## Spatial Field and Relocation

The initial 512-voxel world uses the existing density convention:

| Spacing | Dimensions | Probe count | First-milestone role |
| ---: | ---: | ---: | --- |
| 64 voxels | `9 x 9 x 9` | 729 | Coarse debug only |
| 32 voxels | `17 x 17 x 17` | 4,913 | Default and required acceptance |
| 16 voxels | `33 x 33 x 33` | 35,937 | Required leak/grid regression |
| 8 voxels | `65 x 65 x 65` | 274,625 | Not qualification-blocking |

Relocation is a GPU voxel-native adapter rather than the papers' geometry-unaware search. It reads
the terrain occupancy atlas and preserves the paper invariants:

- a relocated probe remains within half of the minimum cell spacing from its nominal position;
- a usable position satisfies an explicit minimum surface clearance;
- relocation failure makes the probe non-contributing;
- the probe remains associated with its original nominal cage;
- interpolation accounts for the actual relocation offset.

Candidate selection is deterministic and lexicographic:

1. satisfy the minimum clearance;
2. minimize displacement from the nominal point;
3. maximize clearance among equally near candidates;
4. use a stable coordinate ordering to break remaining ties.

The GPU implementation should use a workgroup per probe so candidate evaluation and reduction can
run in parallel without per-probe CPU readback. Relocation is a correction toward the nearest safe
position, not an optimization that drives many probes toward the most open part of a room.

The initial volume still waits for finalized startup terrain. After initialization, runtime edits
build a complete replacement volume and publish it atomically as described under the runtime-edit
milestone below.

## GPU Resource Contract

### Probe Atlases

Use the paper-style tiled two-dimensional texture atlases with octahedral mapping and a one-texel
gutter around every probe tile:

| Field | Interior | Stored tile | Correctness format | Contents |
| --- | ---: | ---: | --- | --- |
| Irradiance | `8 x 8` | `10 x 10` | `RGBA32F` | Linear RGB diffuse irradiance |
| Visibility | `16 x 16` | `18 x 18` | `RG32F` | Mean distance and mean squared distance |

Full precision is an oracle format, not the final production format. It intentionally excludes
float16 precision loss, irradiance quantization, exponent encode/decode, and temporal encoding from
the first correctness investigation. Production formats will be introduced one at a time after the
full-precision field is green.

Octahedral gutter copy follows the paper's wrap and mirror rules and receives its own synthetic
directional-field test. A gutter defect can look like a probe-grid or angular seam and must be
diagnosable independently of spatial interpolation.

### Transient Ray Data

Tracing and atlas filtering communicate through a transient full-precision ray buffer for the
current update batch. Each record is one `float4`:

```text
rgb = ray radiance
w   = signed hit distance
```

| Ray result | RGB | Signed distance |
| --- | --- | --- |
| Authored-sky miss | `getSkyColor(rayDirection, sunDirection)` | Positive far distance |
| Frontface terrain hit | Zero | Positive hit distance |
| Backface terrain hit | Zero | Negative hit distance |

The ray direction is reproducible from the deterministic 256-direction Fibonacci sequence and the
ray index; it does not need to be stored in every record. Hit normals are consumed in the trace pass
to classify frontfaces and backfaces. A DDGI probe itself has no normal.

### Global Sky Irradiance

A single unoccluded 8 x 8 octahedral irradiance map is filtered directly from the authored GPU sky.
It replaces the global SH fallback after migration and is sampled only when the query is outside the
DDGI volume or the entire volume is not ready.

## Initialization and Update Pipeline

The deterministic first milestone uses this order:

```text
final initial terrain ready
→ allocate/clear DDGI resources
→ build global sky irradiance map
→ classify and relocate all nominal probes on the GPU
→ trace deterministic probe batches
→ update irradiance tiles
→ update visibility tiles
→ update octahedral gutters
→ mark the complete volume ready
```

Tracing, irradiance filtering, visibility filtering, and border updates are separate compute passes.
The separation is required for locality, ablation, and future evolution: indirect hit shading will
change the trace/ray-shading stage; temporal accumulation will change the atlas update stages; atlas
addressing and surface queries remain stable.

The transient ray buffer holds only the active batch. The scheduler may spread initialization over
frames to avoid a single watchdog-scale dispatch, but consumers must not sample a partially built
DDGI volume. During A/B migration the previous local-SH backend may remain active until DDGI reports
whole-volume readiness. Static acceptance begins only after the ready transition.

## Surface Query Contract

For a point inside a ready volume:

1. Find the point's nominal grid cell and its fixed eight corner probes.
2. Do not perform a dynamic nearest-probe search after relocation.
3. Compute position weights that account for relocation offsets while remaining non-negative and
   normalized.
4. Use each actual probe position for the probe-to-surface direction and distance.
5. Sample the probe irradiance tile in the surface-normal direction.
6. Sample first and second distance moments in the biased probe-to-surface direction.
7. Apply the 2021 surface-side/backface and world-space bias semantics.
8. Apply the 2022 cage-support distance rejection, adjusted for allowed relocation.
9. Combine trilinear, relocation confidence, surface-side, and moment-visibility weights.
10. Normalize only trustworthy contributions.

The implementation must not contain a positive minimum-visibility floor. In particular, the
current `0.05` floor is incompatible with the sealed-room contract because it guarantees a nonzero
contribution from an occluded probe.

Fallback is strict:

```text
inside a ready volume + trustworthy contribution exists → normalized DDGI result
inside a ready volume + all local contributions rejected → zero
outside the volume or whole volume not ready            → global sky irradiance
```

Invalid, relocation-failed, stale, or fully occluded probes receive zero weight. They do not cause
a global-sky fallback. Bias is recorded in world and voxel units and qualified independently at
spacing 32 and 16 instead of being tuned at one density and silently scaled at the other.

## Terrain and Raster Consumers

Terrain is the first consumer because the deterministic leak cases exercise the terrain compute
path and terrain already supplies world position and a stored voxel-surface normal. Once terrain is
green, all existing raster environment-lighting consumers move to the same shader seam:

- full-resolution and LOD flora;
- full-resolution and LOD leaves;
- dynamic fruit;
- sprinklers;
- particles and billboards that currently consume environment lighting.

Raster consumers remain consumers, not DDGI occluders. They sample at vertex level using their
voxel center and existing procedural normal, then interpolate vertex color. The milestone does not
add normal vertex attributes, expand the packed flora vertex stride, or move DDGI sampling to the
fragment shader.

## Leak Oracle and Debug Views

### Exact Visibility Reference

The correctness harness includes a slow, non-shipping reference that traces the exact segment from
the shaded terrain point to each of its eight nominal-cage probes. It distinguishes the remaining
failure classes:

```text
exact visibility is dark, moment visibility leaks
→ moment filtering, bias, Chebyshev evaluation, or directional support is wrong

exact visibility also leaks
→ probe irradiance, relocation, cage selection, or spatial weighting is wrong
```

The exact path is enabled only by deterministic test/debug modes and is not used by normal terrain
or raster rendering.

### Static Acceptance Cases

All terrain is built before DDGI initialization. The first suite contains fixed camera presets for:

1. **Sealed room** — no sky path; settled environment irradiance must be effectively zero.
2. **Controlled portal/skylight** — legal incoming environment light must remain smooth and must
   not be removed by an over-conservative visibility rule.
3. **Thin and diagonal voxel walls** — one- and two-voxel barriers plus a staircase/diagonal wall
   expose bias, support, grid-axis, and interpolation failures.

Every case runs at spacing 32 and 16 with uniform albedo, fixed authored sky/sun state, fixed camera,
and deterministic ray directions. The current post-initialization roof-open/roof-close sequence is
not part of this milestone.

### Permanent Debug Views

The DDGI module retains these modes after the fix:

- final linear DDGI irradiance;
- moment-estimated visibility;
- exact segment visibility;
- absolute moment-versus-exact visibility error;
- unnormalized/normalized weight sum and dominant probe;
- probe state, relocation, and atlas-tile inspection sufficient to diagnose gutter errors.

Normal rendering does not execute exact visibility rays or debug-only outputs.

### Automated Acceptance

A planned agent-runnable command such as `scripts/check_ddgi_correctness.sh` drives all three cases
at both qualified spacings. It reads or captures linear irradiance before albedo, direct sun,
tonemapping, and other post-processing. It must return a nonzero exit status for the actual leak.

The runner reports at least:

- sealed-region maximum and high-percentile irradiance relative to the open-sky reference;
- moment-versus-exact visibility and irradiance error;
- repeated-run determinism;
- octahedral gutter continuity;
- per-case pass/fail and an aggregate exit status.

Numerical thresholds will be chosen from the full-precision exact reference and recorded with the
first red/green evidence. They will not be guessed from tonemapped screenshots. Screenshots remain
supplementary human evidence.

The first M3 red/green calibration uses the absolute linear-irradiance error P99 against the exact
eight-segment reference:

| Case | Spacing | Pre-fix P99 | Accepted P99 | Threshold |
| --- | ---: | ---: | ---: | ---: |
| Thin/diagonal walls | 32 | `0.16427` | `0.13841` | `0.15` |
| Thin/diagonal walls | 16 | `0.13483` | `0.13078` | `0.133` |

The sealed cases require maximum irradiance and exact-reference error P99 no greater than
`0.00001`. The portal cases require irradiance P99 of at least `0.10` and exact-reference error P99
no greater than `0.01`. Every final capture is repeated and required to be bit-exact. The M5
post-removal run at `target/ddgi-correctness/20260731T222545Z-119363` passed all six configurations:
sealed was exactly zero at both spacings; portal reference-error P99 was `0.004334` at spacing 32
and `0.003165` at spacing 16; walls reference-error P99 was `0.138412` and `0.130781`, respectively.

The calibrated fix first keeps the query bias at `0.25` voxel at both qualified probe spacings; the
prior expression accidentally applied `0.25` of a probe spacing and could move the query four or
eight voxels through a wall. Cage-support filtering distinguishes a true sky miss from a distant
geometry hit: sky misses clamp to the cage support distance, while distant geometry samples are
rejected so they cannot pull a local depth lobe through thin geometry.

The later packed-voxel gate starts that same `0.25` voxel bias along the stored voxel normal, never
the camera ray. Its optical DDA advances all tied axes together at exact voxel edge/corner
crossings, so a zero-area boundary touch does not count as an occluder; the cell the ray actually
enters is still tested and stale or unavailable occupancy still fails closed. The exact debug views
expose this hard visibility separately from the filtered moment term. This removed the original
tree-branch ROI's black pixels, but the later `blacky` camera snapshot exposed a separate violation:
terrain still queried DDGI from the per-pixel ray intersection, so one voxel face could contain
triangular black and lit regions even though its material normal and albedo were voxel-constant.

Terrain consumers and probe-transport terrain hits now construct one canonical receiver from the
voxel center and stored voxel normal by intersecting that direction with the voxel cube. Probe cage
selection, weighting, moments, and hard visibility therefore receive the same surface position for
every pixel of a voxel; the existing `0.25`-voxel normal bias is still applied inside the shared
query. A camera-directed capture gate recovers receiver voxel IDs and rejects any voxel containing
both black and non-black pixels. In `blacky`, mixed environment/combined voxels changed from
`867`/`513` to `0`/`0` at spacing 32 and remained `0`/`0` at spacing 16. Entirely black voxels are a
separate remaining visibility-quality issue: environment/combined black pixels changed from
`36,318`/`23,798` to `19,232`/`13,441` at spacing 32 and measured `14,198`/`8,981` at spacing 16.
The post-change walls reference-error P99 is `0.02630` at spacing 32 and `0.00402` at spacing 16,
with all six correctness cases still accepted.

## Migration Milestones

Each milestone is a focused, validated commit before the next begins.

### M0: Static Harness and Backend Seam

- Replace the phase-one dynamic terrain-edit sequence with finalized static test geometry.
- Add the temporary local-SH/DDGI backend selector and readiness reporting.
- Establish linear-irradiance capture plumbing and the acceptance runner skeleton.

### M1: DDGI Module and Full-Precision Atlases

- Add the new host-side DDGI module without changing consumers.
- Allocate paper-style irradiance/visibility atlases and transient ray storage.
- Implement atlas addressing, octahedral mapping, and synthetic gutter tests.
- Build the global sky irradiance map directly from the authored GPU sky.

### M2: GPU Placement and Deterministic Ray Data

- Run one GPU voxel-native classification/relocation pass after final terrain initialization.
- Implement nearest-safe deterministic placement and nominal-cage ownership.
- Trace 256 deterministic rays per probe in bounded batches.
- Record full-precision radiance and signed distance.

### M3: Atlas Filtering and Terrain Query

- Filter ray data into 8 x 8 irradiance and 16 x 16 distance-moment tiles.
- Update octahedral gutters in independent passes.
- Implement the 2021 query plus 2022 relocation/support corrections.
- Add the exact terrain visibility reference and permanent ablation views.
- Make all six static spacing/case configurations red-capable, then green.

### M4: Raster Consumers

- Route all raster consumers through the shared DDGI sampler at vertex level.
- Preserve existing procedural one-normal-per-voxel behavior and packed vertex layouts.
- Verify terrain and every raster path select the same backend and atlas revision.

### M5: Remove SH Environment Lighting

- Replace the global-SH fallback with the global sky irradiance map.
- Delete local-SH probe buffers, update/reprojection shaders, sampling logic, and tests.
- Remove the temporary backend selector.
- Re-run the complete correctness suite and normal repository validation.

### M6: Runtime Terrain Edits

- Publish a terrain revision only after its deferred GPU terrain rebuild is idle, then prepare a
  same-spacing staging volume for that exact revision.
- Keep the last complete active field available while edited staging is incomplete. Consumer exact
  visibility uses the latest published voxel terrain, while active irradiance, relocation, and
  moment data may remain stale until promotion.
- Give each staging allocation an immutable serial token. A later edit obsoletes older work, so an
  obsolete Ready notification can neither promote nor clear the latest refresh request.
- Arbitrate one physical staging slot with terrain priority. Batch continuous player terrain dabs
  and publish one refresh request on left- or right-mouse release. A later release may update the
  queued latest revision while the current candidate finishes, but it cannot allocate a second
  concurrent staging update. A density rebuild remains queued behind terrain work.
- Promote metadata, irradiance, visibility, spacing, and revision as one consumer-visible unit.
  The centralized promotion seam records terrain compute and representative flora raster consumers
  against the same active token and terrain revision.
- Expose active/target identity, builder progress, coordinator state, queued density, and resident
  active-field availability in the Environment Probes debug panel.

The permanent unattended gate is:

```bash
scripts/check_ddgi_runtime_terrain_edits.sh
```

It runs initial-open, runtime-closed, sequential-reopened, and edit-during-build latest-wins states
at spacing 32 and 16. Every final state is captured twice and must be bit-exact. Open states require
linear irradiance P99 of at least `0.10`; closed requires maximum irradiance at most `0.00001`.
Every final capture is also compared with the same-camera exact-irradiance oracle (`0.01` maximum
P99 error for portal states). The runner checks active/target/token and shared-consumer evidence,
rejects promotion of obsolete terrain revision 2, scans logs for validation/descriptor/stale
readback failures, and returns one aggregate exit status with one output directory.

The gate also captures the ordinary final DDGI output while the latest terrain candidate is still
`BuildingTerrain`: active revision 1 remains bound, target revision 3 has a nonzero staging token
and GPU filtering progress, neither terrain candidate has promoted, and the older resident active
field must produce finite, nonnegative, nonzero terrain irradiance. Log ordering proves the older
candidate releases the single update slot before the latest queued build starts. A separate
flora-enabled runtime run requires a nonzero flora instance draw to report the exact final active
token and terrain revision recorded by the shared consumer promotion seam. Capture runs are
one-shot tasks and exit immediately after the file is successfully flushed; the default 60-second
auto-exit is only a slow-machine or failure timeout and can be overridden with
`DDGI_RUNTIME_TERRAIN_EDIT_AUTO_EXIT`.

## Validation Ladder

Every shader/Rust milestone follows the repository policy:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --latest-log
```

`cargo check` regenerates shader-derived Rust structures; generated files are never edited by hand.
The hidden release run is inspected for Vulkan validation messages, errors, and readiness/state
evidence. The DDGI correctness runner and deterministic captures supplement this ladder; ordinary
unit tests must remain fast and must not absorb long-running GPU/window acceptance work.

## Later Evolution

After the sky-only static field is correct:

1. **Production storage and convergence** — introduce compact atlas formats, irradiance encoding,
   randomized ray rotations, temporal hysteresis, and convergence controls one at a time against the
   full-precision static oracle.
2. **Indirect hit shading** — first shade terrain hits for a single indirect bounce, then add
   previous-DDGI feedback for multi-bounce propagation without changing atlas/query seams.
3. **Dependency-exact terrain refresh** — measure and record probe-to-geometry dependencies, then
   replace full-volume staging with a provably safe local refresh. This is an optimization; it must
   preserve active-field continuity, single-update serialization, and latest-wins promotion.
4. **Scale and activity** — qualify spacing 8, measure sleeping/vigilant states, and only then
   consider tracking volumes, cascades, paging, and cross-volume blending.
5. **Additional geometry** — consider dynamic DDGI occluders only after a specific visual need and
   an update/convergence design justify their cost.

The first milestone is complete: the sealed room is dark without hidden ambient floors, a valid
opening remains lit, thin and diagonal walls stay within their calibrated exact-reference limits,
spacing 16 does not expose a probe lattice in the acceptance capture, and the approximate moment
query is measured against exact visibility rather than accepted by appearance alone.
