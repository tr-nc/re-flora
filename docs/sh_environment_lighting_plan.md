# SH Environment Lighting Plan

## Background

Re: Flora currently has two lighting paths that meet only at composition:

- terrain is rendered by the compute tracer;
- flora, tree leaves, particles, fruit, and props are rendered by raster pipelines.

The terrain tracer evaluates a primary voxel hit, direct sun lighting, fixed ambient light, and an
optional stochastic diffuse second ray. The second ray samples one cosine-weighted direction, traces
the voxel scene, and returns sun-lit one-bounce color from the secondary hit. One sample per pixel is
noisy, so the terrain color is passed through temporal accumulation and an A-Trous spatial denoiser.

Raster flora and leaves do not exist in the terrain acceleration structure. They animate in the
vertex shader, sample the sun/terrain/leaf/cloud shadow resources, and currently combine direct sun
with the same fixed ambient color. Their fragment shaders receive an already-lit interpolated color.
This is efficient for moving vegetation, but it means terrain and raster objects do not yet share a
real environment-lighting representation.

The project is expected to add environment lighting and local light sources. A stable shared
environment representation is useful before local-light selection becomes complex:

- terrain and raster vegetation should receive the same sky/environment diffuse light;
- moving vegetation should evaluate lighting at its current animated position without entering the
  terrain acceleration structure;
- the normal gameplay path should avoid a full-resolution stochastic second ray when a stable
  low-frequency representation provides sufficient quality;
- ReSTIR should remain an optional scaling mechanism for many expensive light candidates, not a
  prerequisite for the first environment-lighting implementation.

The main terrain radiance denoiser is distinct from the temporal filters used by animated leaf
shadows, VSM, and clouds. Removing the stochastic second ray may make the main radiance denoiser
unnecessary, but it does not automatically remove those independent histories.

## Goal

Introduce a shared spherical-harmonic environment irradiance path that can be evaluated by both the
terrain tracer and raster-rendered objects.

The initial result should:

1. replace fixed ambient lighting with directional, linear-HDR sky/environment diffuse lighting;
2. use one SH convention and one shader evaluator across terrain, flora, leaves, and other stylized
   raster objects;
3. keep direct sun, local direct lights, and their visibility separate from SH environment light;
4. allow the stochastic terrain second ray to be disabled and then removed if release-mode quality
   and performance validation accept the SH result;
5. allow the main terrain radiance denoiser to be disabled and then removed if the remaining direct
   shadow signal is stable enough without it;
6. leave a clean extension point for local probe SH and ReSTIR DI without requiring either in the
   first implementation.

## Non-goals

- Do not put animated flora or leaves into the terrain voxel acceleration structure.
- Do not implement local light sources in the initial SH step.
- Do not implement ReSTIR in the initial SH step.
- Do not use SH for sharp direct sun, sharp local lights, or specular reflections.
- Do not treat SH as a replacement for local visibility, contact shadowing, or detailed color
  bleeding.
- Do not remove leaf-shadow, VSM, or cloud temporal reconstruction as part of main radiance-denoiser
  cleanup.
- Do not rewrite the raster vegetation path into a deferred/G-buffer renderer in this phase.

## Lighting Model

Normal gameplay should move toward:

```text
terrain primary hit or animated raster surface
    -> material/albedo and stylized normal
    -> direct sun * sun visibility
    -> direct local lights * per-light visibility        (future)
    -> shared environment irradiance SH
    -> optional local probe irradiance                   (future)
    -> final color
```

The first SH representation should contain diffuse environment irradiance only:

- use linear HDR values;
- exclude the explicit sun disc and other lights already evaluated as direct lighting;
- store cosine-convolved irradiance coefficients so shader consumers only evaluate the SH basis and
  multiply by albedo;
- begin with second-order SH (nine RGB coefficients), while retaining release measurements that can
  justify a lower-order representation if vegetation bandwidth or register pressure is material;
- define the world-space coordinate convention and coefficient ordering in one shared shader module.

The public shader-facing API should be position-aware even while the first implementation is global:

```text
sampleEnvironmentIrradiance(world_position, normal) -> linear RGB irradiance
```

The global implementation may ignore `world_position`. Keeping it in the contract allows a later
probe grid to interpolate spatially varying SH without changing every material call site.

## Source and Update Policy

SH coefficient generation must stay consistent with the visible environment:

- derive coefficients from the same authored sky/environment parameters used by composition;
- update coefficients when the environment revision changes, including meaningful sun/time-of-day
  changes;
- avoid counting the direct sun twice;
- do not temporally smooth deterministic coefficient updates unless a measured visual discontinuity
  requires it;
- add an explicit environment revision so future local probes can invalidate or refresh predictably.

The implementation should choose the coefficient-generation location only after a small parity
prototype:

1. CPU integration is acceptable if sky parameters have a single source of truth and CPU/shader
   parity is tested.
2. GPU coefficient generation is acceptable if it reuses the shader sky model without introducing
   a costly per-frame reduction or synchronization path.
3. A precomputed time-of-day table is acceptable only if interpolation and authored-sky changes stay
   maintainable.

The chosen method should be documented with its error and update cost rather than selected by
assumption.

## Implementation Plan

### Phase 1: Establish a measurable baseline

- Capture fixed-camera day, sunset, and night reference images.
- Record a moving-camera and windy-vegetation reference.
- Run the release `render-steady` scenario and retain `frame.render`, `tracer.render`,
  `tracer.pass`, and `denoiser.pass` evidence where available.
- Record current denoiser history and fresh-sample metrics.
- Separate current second-ray contribution from direct sun and ambient in diagnostic captures so the
  visual behavior being replaced is explicit.

Baseline captured on 2026-07-29 from `c64e7ef4` plus the two documentation commits on Apple M4 Pro:

- the hidden `render-steady` run used 155 post-warmup samples at the normal full-resolution render
  extent; medians were `frame.render=13465 us`, `tracer.render=10721 us`,
  `tracer.shadow_prepass=1407 us`, and `composition.pass=278 us`;
- the isolated 2560x1440 denoiser history run measured mean frame-to-frame luma delta `0.036034`
  and spatial gradient `2.356452`;
- the matching fresh-sample run measured luma delta `0.462752` and spatial gradient `2.362386`,
  confirming that temporal history, rather than extra spatial blur, provides most of the current
  stochastic stability;
- hidden fixed-camera captures covered day (`time_of_day=0.455705`), sunset
  (`time_of_day=0.74`), and night (`time_of_day=0.0`);
- a day capture with `debug_bool=false` isolated the current direct-sun plus fixed-ambient path from
  the normal `debug_bool=true` path that includes the stochastic second ray.

The reports, logs, and PNG captures remain local under `target/sh-environment-baseline/`. Their image
SHA-256 values are:

```text
day with second ray     e8d4ab97e6c3233dc650f81dd04bd5564da71f5a2e4643e101afaa1a6e8100ce
day without second ray  36ded733cfc20649e437a19207820593decf82d9bd3024449f1af25ead00cf79
sunset with second ray  ab657e7393041df369ade257af544c66141ed955fac3b7099f09808aed8487f3
night with second ray   3061d89338b85b676ebc21df55790ffacad0fb3af99546615cff1e655d0094fb
```

### Phase 2: Define and validate the SH contract

- Add an environment-lighting resource containing nine aligned RGB irradiance coefficients and an
  environment revision.
- Add shared SH basis/evaluation helpers used by compute and raster shaders.
- Define coefficient order, handedness, up axis, normalization, and cosine-convolution convention.
- Add deterministic tests or validation utilities for:
  - constant-color environment;
  - upper-hemisphere environment;
  - a rotated directional lobe;
  - non-negative/clamped final diffuse irradiance policy, if clamping is required.
- Decide and document CPU, GPU, or table-based coefficient generation using a parity comparison
  against direct numerical environment integration.

Implemented on 2026-07-29 with the following contract:

- the project uses Y-up real L2 SH in Sloan order adapted to the world axes:
  `Y00, Y1-1(z), Y10(y), Y11(x), Y2-2(xz), Y2-1(zy), Y20(3y²-1), Y21(xy),
  Y22(x²-z²)`;
- the nine RGB values are cosine-convolved irradiance coefficients. The Lambertian band factors
  `pi`, `2pi/3`, and `pi/4` are applied during projection, so shader consumers only evaluate the
  basis, clamp negative reconstruction to zero, and multiply by albedo;
- the existing shading uniform carries nine aligned `float4` coefficients plus an environment
  revision. The shared position-aware shader function is available to both compute and raster
  modules; its position argument is reserved for future probe lookup;
- authored sky keyframes now live in one Slang data module. The visible sky shader imports it
  directly, while `build.rs` generates matching Rust constants from that same source instead of
  maintaining a second color table;
- a deterministic 2048-direction Fibonacci-sphere CPU projection is cached and only recomputed when
  the normalized sun direction changes. The explicit sun disc is excluded because direct sun remains
  a separate lighting term;
- tests cover constant light, upper-hemisphere orientation, rotated lobes, negative reconstruction,
  revision changes, and comparison with a 32768-sample direct diffuse integral. The real-sky test
  bounds per-channel absolute error below `0.025`;
- all 254 tests passed, all 81 Slang entry points compiled, and a hidden release screenshot run
  exited successfully. Refactoring the authored sky data changed the fixed day capture by
  `RMSE=0.009722` and mean absolute RGB error `0.000503`; the sparse maximum difference comes from
  dynamic leaves and shadows rather than a broad sky mismatch.

### Phase 3: Integrate SH into the terrain tracer

- Evaluate environment irradiance at the primary terrain hit.
- Keep direct sun and its terrain/leaf/cloud transmittance unchanged.
- Add a temporary development comparison switch between:
  - current fixed ambient plus stochastic second ray;
  - SH environment irradiance without the second ray.
- Do not keep a permanent user-facing formula/tuning matrix if the comparison has selected one
  production path.
- Validate daylight, sunset, night, upward/downward normals, cavities, and newly exposed terrain.
- If global SH makes enclosed or downward-facing regions implausibly bright, evaluate a cheap
  environment-visibility or bent-normal term before introducing full probes.

During comparison, the terrain integration used the existing development `debug_bool` as a
temporary A/B switch: `true` retained fixed ambient plus the stochastic second ray, while `false`
evaluated SH irradiance at the primary hit and skipped the second ray. Direct sun and all
terrain/leaf/cloud transmittance stayed identical in both paths. Phase 5 removed the switch.

Hidden fixed-camera day, sunset, and night captures completed successfully. Relative to the old
second-ray captures, full-frame RGB means changed from `0.4054` to `0.4219` by day, `0.4548` to
`0.4803` at sunset, and `0.2414` to `0.2190` at night. The larger color difference is intentional:
terrain now receives directional sky hue instead of neutral fixed ambient. The open-scene capture
does not justify adding a visibility approximation yet; cavity and under-canopy behavior remains an
acceptance item before retiring the comparison path.

### Phase 4: Integrate SH into raster lighting

- Route the same environment resource through flora, leaves, fruit, particles, sprinkler, and other
  stylized raster consumers that currently call the shared sun-plus-ambient helper.
- Evaluate SH using each consumer's current animated world position and existing stylized normal.
- Keep the first integration in the current vertex-lit architecture unless measurements or visible
  interpolation artifacts justify fragment lighting.
- Preserve tree-leaf transmission as a direct-sun effect; do not fold it into SH.
- Confirm full-resolution and LOD vegetation use the same environment-lighting path.

During comparison, the shared stylized raster helper selected fixed ambient or SH using the same
temporary `debug_bool` switch as terrain. Flora, tree leaves, attached and dynamic fruit, textured
particles, and sprinklers pass their current animated voxel center and existing stylized shading
normal to the position-aware evaluator. Full-resolution and LOD flora/leaves call the same helper;
tree-leaf transmission remains an added direct-sun term. Phase 5 made SH unconditional.

All raster entry points compiled, all 254 tests passed, and hidden release captures covered day,
sunset, night, normal LOD selection, and a forced-LOD run. The animated canopy follows the same
environment hue and night intensity as terrain without entering the terrain acceleration
structure.

### Phase 5: Retire the stochastic second ray

- Compare the SH path against the baseline using fixed and moving cameras.
- Check whether lost local terrain bounce/color bleeding is acceptable for the intended stylized
  outdoor scene.
- If necessary, add a bounded environment-visibility approximation rather than immediately restoring
  per-pixel stochastic GI.
- Remove the normal-gameplay second ray only after the SH path is visually accepted.
- Remove the old runtime branch and dead shader resources in a separate validated commit.
- Keep an offline or development-only reference mode only if it remains useful for probe validation
  and does not burden the production shader.

The SH path was accepted for the current stylized outdoor scene after day, sunset, night, animated
canopy, shadowed wall, and forced-LOD captures. No widespread cavity or under-canopy over-lighting
justified a global visibility multiplier, so the initial implementation keeps the SH signal
unoccluded and leaves the position-aware probe extension for future scenes that demonstrate a local
visibility problem.

The stochastic diffuse ray, its temporary branch, fixed ambient uniform, and their GUI/CPU plumbing
were removed. The weighted-cosine blue-noise resource remains because the god-ray shader still uses
it; it is no longer used by terrain diffuse or direct-light sampling.

A matched hidden release `render-steady` run produced 276 post-warmup samples:

- `frame.render` median fell from `13465 us` to `4749.5 us` (`-64.7%`);
- `tracer.render` median fell from `10721 us` to `2614 us` (`-75.6%`);
- `tracer.shadow_prepass` remained within run variance (`1407 us` to `1441 us`);
- `composition.pass` median fell from `278 us` to `98 us`.

The post-removal fixed day capture retained the preceding SH result
(`mean RGB 0.421940` versus `0.421996`; mean absolute difference `0.000595`), with sparse
differences caused by animation and temporal shadows.

### Phase 6: Reassess the main terrain radiance denoiser

- First run the raw SH-lit tracer without temporal or spatial radiance denoising.
- Inspect:
  - PCSS shimmer from frame-varying sample patterns;
  - terrain edge aliasing;
  - animated leaf-shadow response on terrain;
  - camera-motion stability;
  - terrain-edit response.
- Prefer filtering the remaining noisy visibility signal directly over filtering final RGB radiance.
- If the raw result is stable, remove the main temporal and A-Trous radiance passes and their
  normal/position/voxel-ID/motion/accumulation history resources.
- Keep leaf-shadow, VSM, cloud, and future probe histories independent.
- Commit denoiser removal separately from second-ray removal so performance and visual effects remain
  attributable.

The first truly raw prototype kept the terrain tracer's stochastic sun-disc direction and PCSS
visibility but bypassed both temporal accumulation and A-Trous. It measured mean frame-to-frame luma
delta `0.696825` with p99 delta `7`, making simple denoiser deletion unacceptable. The instability
was therefore isolated to direct visibility rather than the deterministic SH environment term.

Terrain direct visibility now uses the existing VSM result with a fixed explicit-sun direction.
With the main denoiser still bypassed, that prototype measured mean luma delta `0.007904`, p99 delta
`0`, noticeable-pixel ratio `0.000173`, and spatial gradient `2.402917`. This is both substantially
more stable and sharper than the old history result (`0.036034` mean delta, `2.356452` gradient).

The accepted implementation removes both main radiance shaders, their compute pipelines, eleven
render-sized normal/position/voxel-ID/motion/accumulation/ping-pong textures, two uniform buffers,
GUI controls, descriptor updates, history copies, and the obsolete `--no-denoise` and
fresh-sample benchmark modes. Composition now unpacks the tracer's RGBE output directly.

A final 64-frame hidden release capture on the completed path measured mean luma delta `0.008950`,
p99 delta `0`, noticeable-pixel ratio `0.000178`, and spatial gradient `2.402048`. The VSM,
leaf-shadow, cloud, and cloud-shadow temporal pipelines remain separate resources and passes. This
removes one source of lag from animated leaf shadows, but it intentionally does not remove the
leaf-opacity history itself.

Matched 2560x1440 hidden release measurements show the cost reduction attributable to this phase:

- relative to the accepted SH path that had already removed the second ray, `frame.render` fell from
  `4749.5 us` to `3325 us` (`-30.0%`) and `tracer.render` fell from `2614 us` to `1198 us`
  (`-54.2%`);
- relative to the original baseline, `frame.render` fell by `75.3%` and `tracer.render` by `88.8%`;
- `tracer.shadow_prepass` measured `1306 us` and `composition.pass` measured `96 us`.

The matched run used `--hidden --windowed` because a display-enumeration change made an unmatched
hidden run select a 5120x2880 physical extent instead of the baseline's 2560x1440. That 5K run is
retained locally but is not used for the comparison.

Final hidden day, sunset, and night captures all completed at 2560x1440. Their whole-frame mean RGB
values were `0.424808`, `0.481037`, and `0.218320`. The final day image differed from the accepted
VSM raw prototype by only `0.000625` mean absolute RGB, confirming that deleting the resources and
passes did not change the selected image result.

### Phase 7: Add spatially varying irradiance only when needed

If global SH cannot represent terrain cavities, structures, or local bounce, extend
`sampleEnvironmentIrradiance` with a chunk-aligned or camera-relative probe volume:

- store local irradiance SH plus confidence/visibility metadata;
- update a bounded subset of dirty probes per frame;
- trace terrain from probes using the existing voxel traversal;
- sample the visible sky on a miss;
- invalidate nearby probes after terrain or environment revisions;
- allow raster vegetation to receive probe lighting without becoming traceable terrain geometry.

Direct moving local lights should still be evaluated directly. Do not hide sharp local-light changes
inside a slowly updated probe field.

No local probe volume is required for the current outdoor target. The open-sky, shadowed-wall,
under-canopy, day, sunset, and night captures did not show a broad visibility error that justifies
probe memory, update scheduling, or terrain-edit invalidation. The position-aware shared evaluator
is the retained extension point. Reassess probes when interiors, deep cavities, or local bounce
become important enough that global SH visibly over-lights them.

### Phase 8: Add local lights, then evaluate ReSTIR from evidence

- Introduce a common local-light candidate description with stable light identity, type,
  position/direction, radiance, range, sampling PDF, and shadow policy.
- Start with per-chunk, tiled, or clustered light lists and a bounded direct-light loop.
- Define shadow tiers separately:
  - sun keeps the detailed animated-leaf opacity path;
  - important local lights may receive dedicated shadows;
  - ordinary local lights may use terrain-only or no shadows;
  - decorative lights may remain unshadowed.
- Do not expect ReSTIR to make raster leaves visible to the voxel tracer; foliage visibility still
  needs its own representation.
- Prototype ReSTIR DI only when release measurements show that candidate traversal or visibility
  queries are a material cost.

ReSTIR entry criteria:

- many simultaneously relevant local/emissive/environment candidates;
- a bounded direct-light loop no longer meets the frame budget;
- evaluating one or a few selected visibility queries can replace many expensive queries;
- reservoir memory and passes fit macOS/MoltenVK limits;
- temporal rejection remains stable with moving vegetation and camera motion;
- the candidate beats clustered/chunk lighting in release A/B runs at comparable image quality.

ReSTIR is not justified in the current renderer. Environment lighting is one deterministic SH
evaluation and the sun is one explicit direct-light candidate, so there is no large candidate set
for reservoir sampling to reduce. The local-light candidate/list infrastructure is intentionally
deferred with the local-light feature, which is outside this plan's initial scope. When local lights
arrive, start with chunk/tiled/clustered lists and explicit shadow tiers; add ReSTIR DI only after
release measurements satisfy the criteria above.

## Validation Plan

All runtime validation for this plan must run in hidden mode:

- always pass `--hidden` to app runs, visual captures, fixed-camera comparisons, benchmarks, and
  smoke tests;
- use `--mute` by default unless audio behavior is explicitly part of the validation;
- do not launch the visible game as an automatic validation step;
- capture any required screenshots from a `--hidden` run.

Every shader or Rust implementation step should follow the repository validation ladder:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Performance conclusions require release-mode app measurements in `--hidden` mode. Use the checked-in
performance and denoiser benchmark tooling, hidden fixed-camera snapshots, repeated runs, and
order-reversed A/B execution where appropriate.

Visual validation should cover:

- day, sunset, and night;
- open sky, downward-facing surfaces, terrain cavities, and under-canopy regions;
- fixed and moving cameras;
- calm and high-wind vegetation;
- terrain edits and time-of-day changes;
- full-resolution and LOD flora/leaves;
- SH enabled with second ray disabled;
- raw tracer output with the main radiance denoiser disabled.

Acceptance should check both consistency and intent:

- terrain and raster objects receive the same environment hue and directionality;
- explicit sun is not double-counted in SH;
- moving vegetation responds immediately to environment changes;
- removing the second ray does not produce unacceptable loss of scene readability;
- removing the main radiance denoiser does not expose unacceptable shimmer;
- leaf-shadow/VSM/cloud histories continue to behave independently;
- the SH path improves or preserves the release frame budget on target GPUs, including macOS.

## Risks

- Global SH has no local visibility and can over-light cavities or areas beneath dense canopies.
- Low-order SH cannot represent sharp environment features; those must remain direct lights or use a
  different representation.
- Duplicating the sky model between CPU and shader can make visible sky and irradiance drift apart.
- Evaluating high-order SH per vegetation vertex may add bandwidth/register cost despite being much
  cheaper than ray traversal.
- Removing the radiance denoiser may expose PCSS temporal noise that was previously hidden.
- Local probe updates can leak, lag, or become expensive after widespread terrain/environment
  changes.
- ReSTIR cannot solve missing foliage visibility data and should not be introduced as a substitute
  for an explicit shadow policy.

## Checklist

Final validation compiled all 79 remaining Slang entry points, passed `cargo check`, and passed 253
Rust tests with one release-only audio benchmark ignored. Exact `rustfmt --check` passed for the
changed Rust files outside `crates/re-flora-shader-build/src/lib.rs`; the workspace-wide formatting
check still reports two pre-existing rustfmt differences in that file outside this plan's edited
shader inventory. `git diff --check`, Python bytecode compilation, the hidden muted release smoke
run, log inspection, the stability benchmark, the matched performance run, and all three hidden
captures passed.

- [x] Record the background, target architecture, non-goals, and staged plan.
- [x] Capture current visual, denoiser, and release performance baselines.
- [x] Define the SH coefficient convention and shared shader evaluator.
- [x] Select and validate the SH coefficient-generation method.
- [x] Add the global environment irradiance resource and revision tracking.
- [x] Integrate SH into the terrain tracer behind a temporary comparison path.
- [x] Integrate SH into all relevant raster flora/leaf/prop lighting paths.
- [x] Validate terrain/raster lighting consistency across time of day and motion.
- [x] Decide whether global SH needs a cheap environment-visibility term.
- [x] Remove the stochastic normal-gameplay second ray after visual acceptance.
- [x] Re-test raw tracer stability without the main radiance denoiser.
- [x] Remove the main radiance denoiser only if shadow and camera-motion stability pass.
- [x] Confirm leaf-shadow, VSM, and cloud temporal paths remain independent.
- [x] Run formatting, checks, tests, hidden muted release validation, and inspect logs.
- [x] Run release A/B performance and image-quality comparisons.
- [x] Document whether and when local probe SH is required.
- [x] Defer local-light candidate/list infrastructure to the future local-light feature.
- [x] Do not implement ReSTIR DI because its entry criteria are not met.
