# DDGI Terrain Indirect Transport Specification

Status: `ready-for-agent`

This is a local project specification. It intentionally replaces issue-tracker publication for
this work.

## Problem Statement

Re: Flora's DDGI Volume currently provides authored sky lighting and visibility-aware environment
lighting, but it does not transport radiance through a terrain hit. A DDGI Probe ray that misses
terrain records sky radiance, while a front-face terrain hit records visibility distance and zero
radiance. This makes the existing field a useful sky-only seed, but it cannot show sunlight or sky
light reflecting from one voxel surface onto another, terrain color bleeding, or temporally
propagated multi-bounce diffuse illumination.

The visible terrain and Raster Consumers already receive direct sun and shadowing through separate
rendering paths. Moving that direct-light result into the DDGI Volume would blur and delay direct
shadows because a probe field is spatially sparse, directionally filtered, and updated over time.
The missing behavior is instead to use direct sun and the previous diffuse field when shading a
DDGI Probe ray's terrain hit, so that the resulting reflected radiance becomes indirect light for
other surfaces.

Editable voxel terrain makes correctness more important than continuity or performance. After an
occupancy change, an old Visibility Map may describe a wall or opening that no longer exists. Such
data must never be allowed to create stale shadows or renewed light leaks. At the same time, sun,
sky, and palette changes do not invalidate geometry visibility and should converge without making
environment lighting disappear.

## Solution

Keep direct sun and its exact visible-surface shadows outside DDGI. Extend the DDGI deep module so
that a front-face voxel-terrain hit reflects two sources into the originating DDGI Probe:

- direct sun evaluated at the hit with current-revision voxel visibility; and
- visibility-aware diffuse lighting sampled from one immutable source Irradiance Map.

Deliver the work in two independently validated stages. First, use the current-revision sky-only
field as the immutable source and produce a deterministic strict single-bounce field. Then reuse
the same source/destination transport path with the previous complete DDGI field as its source,
allowing repeated full-volume iterations to converge toward multi-bounce diffuse illumination.

The first implementation uses fixed probe-ray directions, zero hysteresis, full-precision storage,
and whole-volume deterministic updates. It publishes only complete iterations. Geometry changes
remain full-domain fail-closed until the latest geometry revision has a complete single-bounce
result. Radiance-only changes retain the last valid field and coalesce pending work to the latest
radiance revision.

## User Stories

1. As a player, I want directly sunlit surfaces to retain crisp, immediate shadows, so that adding
   indirect lighting does not make direct lighting look blurred or delayed.
2. As a player, I want sunlight reflected from voxel terrain to illuminate nearby shaded surfaces,
   so that the world has convincing diffuse light transport.
3. As a player, I want visible sky light reflected from terrain to contribute indirect lighting,
   so that single-bounce environment lighting is not limited to the sun.
4. As a player, I want a colored voxel wall to tint nearby indirect illumination, so that terrain
   materials participate in color bleeding.
5. As a player, I want light to propagate around a two-surface dogleg over later updates, so that
   the DDGI field visibly supports multi-bounce transport.
6. As a player, I want a sealed room with no light source to remain dark through every bounce
   iteration, so that feedback cannot invent energy.
7. As a player, I want thin walls, roofs, and diagonal barriers to remain leak-free after indirect
   transport is enabled, so that the previous visibility correction is preserved.
8. As a player editing terrain, I want a newly placed wall to reject stale environment lighting,
   so that old probes cannot shine through geometry that now exists.
9. As a player editing terrain, I want a newly opened roof or doorway to receive lighting from the
   latest geometry revision, so that the field does not publish an obsolete rebuild.
10. As a player editing terrain repeatedly, I want only the newest geometry result to become
    visible, so that late GPU work cannot overwrite a more recent edit.
11. As a player editing terrain, I want direct sun to remain available while DDGI is fail-closed,
    so that invalidating environment lighting does not disable all lighting.
12. As a player, I want the first post-edit DDGI result to be a complete single-bounce field, so
    that I never see a probe batch or grid sweep across the world.
13. As a player, I want later multi-bounce iterations to appear atomically, so that convergence is
    spatially coherent.
14. As a player using the automatic day/night cycle, I want direct sun to move immediately while
    indirect lighting follows smoothly, so that the DDGI Volume does not repeatedly black out.
15. As a player, I want rapid sun or sky changes to coalesce to the latest state, so that obsolete
    intermediate lighting states do not build an unbounded queue.
16. As an artist, I want the current sky-only brightness to remain substantially unchanged after
    the energy convention is corrected, so that this feature does not unintentionally restyle the
    game.
17. As an artist, I want the authored voxel type color and intentional per-voxel hash variation to
    affect bounced light, so that color bleeding matches stable terrain appearance.
18. As an artist, I want moisture, fertility, and edit-preview tint excluded from the initial
    bounce model, so that transient presentation details do not silently require DDGI rebuilds.
19. As an artist changing the sun color, sun lighting luminance, sky parameters, or voxel palette,
    I want indirect lighting to converge to the latest radiance state, so that the field cannot
    remain permanently stale.
20. As an artist changing DDGI spacing, I want the old valid density to remain visible until the
    replacement single-bounce field is complete, so that a non-geometric rebuild does not black out
    environment lighting.
21. As a rendering developer, I want one immutable source Irradiance Map for every transport
    iteration, so that output does not depend on probe batch order.
22. As a rendering developer, I want separate source and destination ownership, so that no shader
    reads values already overwritten by the current iteration.
23. As a rendering developer, I want the Visibility Map owned by a geometry revision, so that
    radiance convergence cannot accidentally mutate geometry visibility.
24. As a rendering developer, I want each build and publication to identify its geometry revision,
    radiance revision, spacing, transport stage, iteration, and unique token, so that stale data is
    diagnosable and cannot be promoted.
25. As a rendering developer, I want a radiometrically stable diffuse convention, so that repeated
    feedback does not gain an unintended factor of pi on every bounce.
26. As a rendering developer, I want non-finite or divergent feedback exposed as an error rather
    than hidden by an internal brightness clamp, so that energy bugs remain observable.
27. As a rendering developer, I want measured convergence rather than a hard-coded bounce count,
    so that different scenes stop based on actual field stability.
28. As a rendering developer, I want an explicit hard iteration limit and `NonConverged` state, so
    that a broken field cannot update forever without evidence.
29. As a rendering developer, I want to inspect the sky seed, single-bounce result, a specified
    feedback iteration, and the converged field, so that each transport stage can be diagnosed
    independently.
30. As a rendering developer, I want captures to record revisions, tokens, stage, iteration, and
    convergence deltas, so that a plausible image cannot conceal stale history.
31. As a rendering developer, I want the same shared environment-lighting interface to serve
    terrain and Raster Consumers, so that the transport implementation stays local to the DDGI
    module.
32. As a rendering developer, I want Raster Consumers to receive terrain-bounced illumination
    without becoming DDGI Occluders, so that this milestone does not expand the trace scene.
33. As a rendering developer, I want fixed rays and zero hysteresis for the first feedback
    implementation, so that repeated runs are directly comparable.
34. As a rendering developer, I want the strict single-bounce result committed and validated before
    temporal multi-bounce is enabled, so that hit shading and recursion failures remain separable.
35. As a QA engineer, I want deterministic pre-albedo linear-light captures at spacing 32 and 16,
    so that color transport and the high-density leak/grid regression are machine-checkable.
36. As a QA engineer, I want batch-order invariance demonstrated by captures, so that immutable
    source ownership is verified through external behavior.
37. As a QA engineer, I want a sunlit color donor case and a two-bounce dogleg case, so that single-
    and multi-bounce behavior cannot be confused.
38. As a QA engineer, I want the existing sealed, portal, wall, runtime-edit, and in-flight edit
    cases to remain passing, so that new transport does not regress visibility or publication
    correctness.
39. As an automated coding agent, I want one hidden release-mode runner with explicit pass/fail
    evidence, so that the feature can be validated without visual guesswork.
40. As a maintainer, I want performance optimizations deferred until the deterministic result is
    correct, so that compressed formats, adaptive updates, and temporal filtering do not obscure
    the first implementation.

## Implementation Decisions

- The existing DDGI deep module remains the owner of volume transforms, DDGI Probe state,
  classification, relocation, tracing, Irradiance Maps, Visibility Maps, atlas gutters, revision
  ownership, convergence, publication, and debug state. Callers must not learn atlas coordinates,
  ping-pong indices, or batch ordering.
- The shared environment-lighting interface remains the only consumer seam for terrain and Raster
  Consumers. Its value is defined as linear diffuse irradiance normalized by pi in engine lighting
  units, so a diffuse consumer applies its stable base albedo exactly once.
- Direct sun and visible-surface shadowing remain outside the DDGI consumer seam. DDGI transports
  energy caused by direct sun; it does not replace direct lighting at the final receiver.
- A DDGI Probe ray miss records the authored sky for the iteration's radiance snapshot. A
  front-face opaque voxel-terrain hit evaluates diffuse reflected radiance. A backface hit retains
  the existing non-contributing radiance and signed-distance behavior.
- Front-face hit shading uses the voxel-marched Surface Normal and stable terrain base albedo. The
  albedo includes voxel type and intentional per-voxel hash variation. Moisture, fertility, and
  edit-preview tint are excluded.
- Direct sun at a probe-ray hit uses the iteration's sun lighting state and an exact shadow ray
  against the same voxel geometry revision. It does not reuse the visible renderer's VSM,
  leaf-shadow, or cloud-shadow result.
- The first strict single-bounce result samples the current-revision, visibility-aware sky-only
  seed at the hit. It must never substitute unoccluded Global Sky Irradiance for a local hit inside
  a ready DDGI Volume.
- The recursive result samples the complete previous DDGI field at the hit position and Surface
  Normal through the existing visibility-aware eight-probe query. It does not spawn an indirect
  hemisphere of secondary rays. A direct-sun shadow ray is the only additional hit-origin ray in
  this milestone.
- A bounce iteration means one complete application of the transport update over every valid DDGI
  Probe. The sky-only field is S0, the strict single-bounce field is S1, and each subsequent complete
  source/destination update produces S2, S3, and so on. Later states contain the retained lower-order
  lighting plus newly propagated higher-order diffuse transport.
- Every iteration reads one immutable source Irradiance Map and writes one distinct destination
  Irradiance Map. No batch may read values written by the same iteration.
- The strict S0-to-S1 bootstrap receives one builder-owned full-precision scratch Irradiance Map.
  It does not duplicate the Visibility Map or probe metadata. At current layouts the additional
  storage is approximately 7.58 MiB at spacing 32 and 55.08 MiB at spacing 16.
- Ordinary feedback may use the active complete field as the immutable source and the staging field
  as the destination. Ownership swaps only after a complete iteration.
- The Visibility Map, classification, relocation, and visibility revision are owned by one geometry
  revision. S0, S1, and all feedback iterations for that geometry reuse the same immutable
  visibility result. A geometry edit produces a replacement visibility result.
- Full-precision atlas formats remain authoritative. The normalized diffuse-energy convention must
  be corrected before recursive feedback. Authored sky calibration may be adjusted so the current
  sky-only presentation remains substantially unchanged.
- The first recursive implementation uses the existing deterministic probe directions, zero
  hysteresis, and complete-volume updates. Random rotation, history blending, sleeping, and adaptive
  scheduling are later work.
- Convergence is measured after each complete feedback iteration using maximum absolute and
  relative RGB delta over valid Irradiance Map texels. Convergence requires two consecutive
  iterations below calibrated thresholds.
- A measured hard iteration limit prevents unbounded work. Reaching it without convergence leaves
  the latest finite current-revision field visible, exposes `NonConverged`, records the measured
  deltas, and fails the correctness gate.
- DDGI storage remains non-negative, unclamped linear HDR. Non-finite output prevents that iteration
  from publishing. No internal indirect-strength control, energy clamp, or display-space compression
  may hide divergence; display mapping remains a final-composition concern.
- Geometry revision identity covers voxel occupancy/topology and all visibility-sensitive inputs.
  A geometry change invalidates the full DDGI Volume, because a conservative local dependency set
  is not yet available.
- Geometry publication uses strict latest-revision-wins semantics. Superseded GPU work may finish,
  but a token that is not the latest geometry request must never become active.
- During full-domain geometry invalidation, environment and indirect queries fail closed to zero.
  Direct sun remains available through its independent visible-surface path. The first new DDGI data
  that may publish is a complete S1 for the latest geometry revision.
- Radiance revision identity covers sun direction, sun lighting color and luminance, authored sky
  parameters, voxel base palette, and hash-color variance. It excludes the deferred dynamic material
  effects.
- Each S0, S1, or feedback iteration latches one immutable radiance revision snapshot. New radiance
  changes never alter batches already belonging to that iteration.
- Radiance-only updates retain the previous valid field. The in-flight consistent iteration may
  finish; pending radiance requests coalesce to the latest revision, skipping obsolete queued
  intermediates. This is intentionally different from strict geometry latest-revision-wins
  publication.
- A geometry bootstrap is not restarted merely because the sun moves. It publishes its internally
  consistent geometry/radiance snapshot, then immediately schedules convergence toward the latest
  pending radiance revision. A newer geometry revision still supersedes the entire bootstrap.
- Scheduling priority is geometry rebuild, then density rebuild, then radiance/feedback convergence.
  A density request does not invalidate geometry, so the old active density remains consumer-visible
  until the replacement density has a complete S1. A geometry edit preempts density and feedback
  work.
- Build and publication identity includes a unique serial, geometry revision, radiance revision,
  spacing, transport stage, and iteration. A feedback destination also records the immutable source
  identity from which it was derived.
- Complete transport states are externally observable as `SeedSky`, `SingleBounce`, `Feedback` with
  an iteration number, `Converged`, and `NonConverged`. Runtime status and captures also expose active
  and staging identities, progress, publication state, and convergence deltas.
- Probe batching remains bounded and may span multiple render frames. A partial batch sequence is
  never consumer-visible; only a complete S1 or complete feedback iteration may be promoted.
- Delivery is staged and committed after each validated step. The first delivery establishes the
  normalized energy convention, immutable seed ownership, probe-hit material/direct/sky shading,
  S1 publication, observability, and its correctness gate. The second delivery enables previous-field
  feedback, measured convergence, and radiance-revision scheduling without changing the consumer
  seam.

## Testing Decisions

- The highest end-to-end test seam is the existing hidden release-mode environment-lighting scene,
  pre-albedo linear-light capture, capture analyzer, and DDGI correctness runner. This seam exercises
  the real Vulkan path and the shared environment-lighting interface used by terrain and Raster
  Consumers.
- Tests assert externally visible lighting, revision ownership, state transitions, captures, and
  log evidence. They must not assert atlas allocation details, descriptor indices, private ping-pong
  choices, or shader function decomposition.
- Pure unit tests remain appropriate for geometry/radiance request coalescing, strict geometry token
  publication, scheduling priority, transport-stage transitions, convergence classification,
  normalized diffuse reference math, batch coverage, and source/destination identity. These tests
  use the DDGI module's host interface and do not require a GPU.
- Existing octahedral addressing, gutter, relocation, exact segment visibility, moment-visibility,
  active/staging promotion, terrain-edit, and latest-revision tests remain prior art and regression
  coverage.
- A deterministic single-bounce donor scene contains a sunlit saturated-color voxel surface and a
  neutral receiver with no direct-sun visibility. Its S0 receiver region must not contain the donor
  bounce; S1 must show a calibrated positive donor-channel advantage while the receiver remains
  directly shadowed.
- A deterministic two-bounce dogleg prevents a direct or one-surface light path to its final receiver.
  The final receiver remains at its calibrated S1 baseline and gains the expected indirect signal
  beginning with S2. Captures record every inspected iteration.
- A sealed zero-energy room has no sky or sun path. Its terrain-hit RGB must remain exactly zero from
  S0 through the converged result at both required spacings. This is the primary no-created-energy
  and no-recursive-leak gate.
- Batch-order invariance runs S1 with at least two deterministic probe-batch traversal orders. The
  resulting pre-albedo captures must be bit-exact, proving that every hit read the same immutable S0.
- The existing portal and walls cases continue comparing moment visibility against exact visibility
  and approximate irradiance against the exact-reference mode. Enabling transport must not weaken
  their calibrated leak limits.
- The existing in-flight terrain-edit case continues requiring strict-zero DDGI output while the full
  domain is invalidated. It additionally proves that S0 and partial S1 never become consumer-visible,
  and that an obsolete geometry token cannot publish.
- A completed terrain edit proves that S1 is the first published current-geometry field and that all
  terrain and Raster Consumers bind the same active transport identity.
- A radiance-change case proves that direct sun responds immediately, the old valid DDGI field remains
  visible, the in-flight iteration retains one radiance snapshot, queued intermediates coalesce, and
  the latest revision eventually becomes active.
- A density-change case proves that the previous valid spacing remains visible during construction,
  the replacement spacing first publishes at S1, and a concurrent geometry edit takes priority.
- Energy-convention validation compares the sky-only field before and after normalization at fixed
  authored settings. Thresholds preserve the intended presentation while separately proving that
  recursive irradiance remains finite, non-negative, and convergent.
- Convergence captures record maximum absolute and relative delta for every iteration. Thresholds and
  the hard iteration limit are calibrated from the sealed room, portal, donor, and dogleg convergence
  curves, then committed as deterministic runner policy rather than selected by appearance alone.
- A synthetic analyzer test rejects non-finite values, wrong stage/revision metadata, a purported
  converged result above threshold, and any nonzero sealed-room terrain-hit channel that a luminance-
  only check might miss.
- Required automated coverage runs at spacing 32 and spacing 16. Repeated captures at the same stage,
  revision, and iteration must be bit-exact while fixed rays and zero hysteresis are active.
- Shader or Rust implementation changes follow the repository validation ladder: formatting check,
  shader-aware compile check, deterministic unit tests, hidden muted release run, and inspection of
  the per-worktree run log. The specialized DDGI correctness and runtime-edit runners supplement this
  ladder.

## Out of Scope

- Replacing final visible-surface direct sun or its VSM, leaf, and cloud shadow paths with DDGI.
- Indirect hemisphere secondary rays, path tracing, or a ray tree spawned at every probe hit.
- Raster Consumers becoming DDGI Occluders or indirect-light emitters.
- Grass, leaves, fruit, sprinklers, particles, water droplets, or other raster geometry participating
  in probe visibility or bounce generation.
- Emissive voxels, point lights, spot lights, area lights, specular GI, glossy reflection, refraction,
  and translucent transmission.
- Moisture, fertility, edit-preview tint, or other frequently changing terrain presentation effects
  contributing to hit albedo.
- Random probe-ray rotation, temporal hysteresis, sleeping or vigilant probe states, adaptive probe
  budgets, prioritized subsets, and production temporal denoising.
- Compact or perceptual atlas formats, half precision, memory compression, and performance-driven
  reductions in ray count or update frequency.
- Dependency-exact local invalidation. Geometry edits continue to invalidate the full DDGI Volume.
- Camera-tracking volumes, cascades, paging, volume blending, or formal spacing-8 qualification.
- Per-fragment Raster Consumer sampling or changes to existing packed instance/vertex layouts.
- An indirect-strength slider or internal brightness clamp used to tune around incorrect transport
  units.
- Publishing this specification to GitHub or another external issue tracker.

## Further Notes

- "Fail closed" means returning zero environment/indirect contribution when the DDGI module cannot
  prove that cached geometry visibility is valid. It does not stop rendering and does not remove the
  independent direct-sun path.
- "Strict latest-revision-wins" applies to geometry publication: once a newer geometry request exists,
  an older build cannot publish even if its GPU work finishes later.
- Radiance scheduling instead finishes the current immutable iteration and coalesces pending requests
  to the latest radiance revision. An internally consistent older iteration may publish before the
  next iteration jumps directly to the latest pending state.
- A bounce iteration is one full-volume update from one immutable Irradiance Map to another. It may
  span many render frames and probe batches. S2 is not only a second-bounce term; it retains lower-
  order lighting while adding approximately one further order of diffuse propagation.
- The paper-based steady state is previous-field feedback. The deterministic S1 milestone is both a
  development oracle and the future clean bootstrap after destructive geometry edits; it is not a
  competing permanent backend.
- The project's archived DDGI papers and dynamic-scene research remain the algorithmic references.
  Paper formulas define the starting point, while voxel-unit biases, thresholds, and convergence
  limits are calibrated against Re: Flora's deterministic captures.
