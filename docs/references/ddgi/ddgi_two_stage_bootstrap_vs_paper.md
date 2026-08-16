# Re: Flora DDGI stages, secondary rays, and modern update strategies

Research date: 2026-08-16; implementation decision updated 2026-08-17

## Short answer

`S0`, `S1`, and `S2` are Re: Flora transport-stage labels. In this repository, `S` is best read as
**Stage**: the code calls the state enum `DdgiFieldStage`, while the paper does not define an `S0` or
`S1` notation. It is not a sample count and it is unrelated to a random-number seed.

- **S0 / `SeedSky`** is a complete, current-geometry field containing authored-sky radiance from
  probe-ray misses and current-geometry visibility. Terrain hits contribute no reflected radiance.
- **S1 / `SingleBounce`** shades terrain hits with current direct sun plus irradiance queried from
  immutable S0. It is the first field that a new staging volume may publish.
- **S2 and later / `Feedback`** shade hits from the previous complete field. Each complete update can
  propagate diffuse energy through one more surface interaction while retaining lower-order light.

`SeedSky` means a numerical **initial condition** for the recursive transport, like seeding an
iterative solver with its first known energy. It does not mean PRNG seed, and Re: Flora is not
rendering the visible sky image before the terrain. It is manufacturing the local sky-light field
that S1 will query at terrain hits.

The production DDGI path does **not** spawn a diffuse secondary ray at a probe-ray hit. It traces a
probe ray and, for direct sun, one hit-origin shadow ray; recursive diffuse transport comes from
querying the previous complete probe field. A separate GUI-controlled, terrain-only path-tracing
reference does spawn a bounded diffuse path (default 2, maximum 8 bounces), but that reference
bypasses DDGI and is not the production indirect-lighting algorithm.

Consequently, production DDGI can show `sky/sun -> red wall -> ground` color bleeding. S1 is enough
when the red wall is directly lit by sky or sun. A path requiring two ordered diffuse surfaces, such
as `sky/sun -> red wall A -> wall B -> ground`, needs S2 or a later feedback field. The committed
donor and dogleg acceptance cases measure both boundaries.

S2+ is recursive in **transport order**, but it is not yet the paper/RTXGI's full **temporal update
form**. Current feedback deliberately uses fixed directions, zero hysteresis, and deterministic
full-volume source-to-destination iterations. The accepted migration on
`feature/ddgi-temporal-lifecycle` removes transport-order stages: publish the first complete
current-revision field, then use epoch-scoped ray rotation and history accumulation until the field
converges and sleeps. Active/Staging publication and immutable source/destination fields remain
correctness requirements, not transport stages.

## What exactly are S0 and S1?

The names are project terminology, enforced by [`DdgiFieldStage` and its iteration
invariants][stage-enum]: `SeedSky` must be iteration 0, `SingleBounce` must be iteration 1, and
feedback/terminal states must be iteration 2 or later. The abbreviated `S0`, `S1`, and `S2` labels
used in logs and specifications are therefore **stage/transport-order indices**, not terminology
imported from Majercik et al. 2019.

| Re: Flora label | Complete field contains | Immutable source | Publication role |
|---|---|---|---|
| S0 `SeedSky` | authored sky on ray misses; zero radiance on terrain hits; current visibility | none | private bootstrap source |
| S1 `SingleBounce` | current direct-sun diffuse response plus sky-lit diffuse response | complete S0 | first publishable replacement field |
| S2+ `Feedback` | lower-order light plus another order of recursively propagated diffuse light | previous complete field | publishable until convergence |

S0 is not a generally displayable “ambient-only” approximation. Its zero-radiance terrain hits are
deliberate because it exists to provide a clean incident sky field for hit shading. Completing S0
makes it the private source for S1; only complete, validated S1 is promoted as the first new Active
field ([bootstrap publication][bootstrap-publication]).

## Why sky seeding precedes single bounce

At an S1 probe-ray terrain hit, the shader needs the incident diffuse irradiance at the hit so it can
compute reflected radiance. Re: Flora does not allow a new geometry revision to sample the old
Active field during bootstrap, because that field's irradiance and visibility belong to the old
geometry. S0 supplies a source that matches the new geometry revision.

The source also has to be complete and immutable. Probe updates are split into batches over several
render frames. If an S1 batch both read and overwrote the same atlas, later batches could see data
written by earlier batches. Bounce depth and output would then depend on batch order. The separate
S0 source makes every S1 probe read the same transport order.

Without S0, an implementation would have to choose a different contract:

1. Start from black, which omits locally occluded sky irradiance at S1 terrain hits.
2. Warm-start from old Active, which is faster but can carry stale light and visibility through a
   geometry edit.
3. Read and write one partially updated field, which introduces update-order dependence unless a
   previous-frame snapshot or equivalent barrier is retained.
4. Evaluate the sky integral independently at every hit, which replaces the S0 cost with another
   approximation or additional hit-origin rays.

Thus, two complete passes are not a mathematical requirement of DDGI. They are the cost of Re:
Flora's strict geometry-revision isolation, immutable source/destination rule, and atomic
publication boundary. The old Active field remains visible during this work; the tradeoff is update
latency, not a mandatory scene-wide indirect-light blackout.

This also explains the earlier 64-ray timing: roughly 40 ms for one full iteration versus a median
89.5 ms from rebuild start through S1 promotion. The latter contains one complete S0, one complete
S1, and lifecycle/validation/promotion overhead; it is not the time for a single probe pass.

## Production rays versus path-tracing secondary rays

### Production DDGI

For every probe, the production trace shader emits a fixed spherical-Fibonacci set of probe rays.
A miss records authored sky. A front-face terrain hit from S1 onward computes

`albedo * (current direct-sun irradiance + previous-complete-field irradiance)`

([probe trace and hit shading][probe-trace]). Direct sun visibility is an exact shadow ray against
the same terrain revision. There is no diffuse hemisphere/ray-tree launch from the hit; the design
contract says this explicitly ([transport specification][transport-spec-secondary]).

This still represents approximate multi-bounce diffuse transport. A probe ray that sees a red wall
stores radiance already multiplied by the wall's red albedo. Ground pixels then interpolate that
probe field, so they can receive red indirect light. On the next complete feedback iteration, a
different hit can query that red-containing field and propagate it around another corner. The
acceptance evidence shows the red donor raising red's channel share at S1, while the two-segment
dogleg remains nearly dark at S1 and gains energy at S2 ([transport acceptance][transport-acceptance]).

This is not full path tracing:

- transport is low-frequency diffuse irradiance reconstructed from probes;
- there is no explicit branching diffuse ray tree at each hit;
- there is no glossy/specular secondary transport in the DDGI field;
- probe spacing, moment visibility, and interpolation can blur detail or leak through geometry;
- the current fixed ray directions do not add new angular samples on later feedback iterations.

The complete-volume policy matters here: every feedback destination is withheld until all valid
probes have been updated from one immutable source. Current recursion is therefore deterministic
Jacobi-style field iteration, not an in-place partial history update and not a temporally blended
Monte Carlo estimator.

### Terrain path-tracing reference

When `Path Tracing Reference (Terrain)` is enabled, the terrain shader bypasses the DDGI query. From
the visible primary terrain hit it samples one diffuse direction per bounce, marches to the next
terrain hit, traces a direct-sun shadow ray there, multiplies path throughput by hit albedo, and
terminates on a sky miss or after the configured limit ([terrain reference shader][terrain-reference]).
The GUI defaults to 2 bounces and clamps the control to 0-8 ([GUI configuration][path-tracing-gui]).

So the precise answer to “does Re: Flora support secondary rays?” is:

- **yes in the terrain-only path-tracing reference**, as a bounded stochastic validation path;
- **no in production DDGI in the path-tracing sense**; production DDGI substitutes previous-field
  lookup for an explicit diffuse secondary path.

## Majercik et al. 2019 versus Re: Flora

The paper describes one recurring update loop: trace rays from active probes, shade hit surfels with
direct and indirect illumination, then blend new radiance and distance into probe atlases
([Majercik et al. 2019, section 4][paper-pdf]). Hit shading reads **previous-frame** probe data. This
both amortizes higher diffuse bounces across frames and smooths temporal discontinuities. The paper
reports irradiance hysteresis values from 0.85 to 0.98 (section 4.4), and section 5.3 describes
higher bounces as recursion across frames.

The reference configuration updates every probe every frame, but this is a scheduling choice. The
paper neither defines S0/S1 nor mandates waiting for two complete updates before display. It notes
that gathering multiple bounces before display is an optional adaptation for fixed-view use cases.

Re: Flora agrees with the paper on the core recursive approximation: shade a ray hit using a
previous complete irradiance field. It differs in update and publication policy:

| Property | Majercik 2019 recurring form | Current Re: Flora |
|---|---|---|
| Bootstrap notation | no prescribed S0/S1 barrier | explicit private S0 then publishable S1 |
| History | previous-frame atlas blended with new samples | immutable complete source, distinct complete destination |
| Temporal blend | hysteresis | no irradiance-history lerp in the filter |
| Ray directions | randomly rotated each frame | same fixed Fibonacci directions each iteration |
| Partial visibility | displayed field evolves continuously | old Active remains visible until complete staging promotion |
| Geometry revision | initialization/invalidation left to integration | strict revision ownership and atomic replacement |

Current Re: Flora therefore favors deterministic stage boundaries and edit correctness over fast
temporal convergence. With 64 fixed rays per probe, feedback improves transport order but does not
average a fresh set of angular samples; paper/RTXGI-style rotation plus hysteresis would improve
sampling convergence, at the cost of temporal state and invalidation complexity.

That distinction is the answer to whether Re: Flora has already “synced with the paper”: the
recursive radiance term is present at S2+, but random temporal sampling, hysteresis, and adaptive
probe scheduling are not.

## Clarification: temporal accumulation does not require perpetual updates

Majercik et al. do run the update loop every frame in their **reference implementation**: section
4.2 says that every scene probe is marked active and receives the same ray count every frame. That
is not stated as a requirement of the transport estimator. In the immediately preceding paragraph,
the authors report experimenting with probe subsets and distance-dependent ray counts; they choose
the all-probes schedule for simplicity. Sections 6.1-6.2 call it a conservative performance choice
and leave optimized probe selection and adaptive updates to future work ([paper, sections 4.2,
6.1-6.2][paper-pdf]).

“Temporal” therefore means **across probe-update epochs**, not necessarily across every display
frame. If a volume is updated once every four frames, its random rotation and history blend advance
once every four frames. If updates stop, the last probe textures remain valid and rendering can
reuse them indefinitely; there is no requirement to decay or recompute them merely because another
camera frame was presented.

Continued updates are useful while a static field is still converging for two independent reasons:

1. Randomly rotated 64-ray sets contribute new angular samples, reducing finite-sample error.
2. Previous-field recursion propagates additional diffuse transport orders over successive updates.

Once both effects are converged enough, a truly static scene can sleep. RTXGI explicitly supports
this policy: it says per-frame updates are common but lower-frequency and asynchronous schedules are
valid, and its Probe Variability feature exists so an application can pause ray tracing and blending
when variability settles, then re-enable them when a light-field-changing event occurs
([RTXGI volume update][rtxgi-volume-update], [RTXGI probe variability][rtxgi-variability]). A
periodic low-frequency probe update is therefore an optional safety net for untracked changes, not a
physical or algorithmic requirement. With reliable geometry and radiance revision tracking, an
event-driven wake-up is sufficient.

Re: Flora already implements the same high-level **converge then sleep** lifecycle, but for its
deterministic zero-hysteresis field iterations: two consecutive complete feedback iterations must
fall below absolute and relative delta thresholds; a converged terminal field schedules no more
feedback until a geometry, density, or radiance request wakes it
([current convergence policy][current-convergence-policy], [current scheduler stop rule][scheduler-stop-rule]).
The current result is “exact” only in the narrower engineering sense of being deterministic,
complete, and revision-consistent. Sixty-four fixed directions are still a finite angular
quadrature, not the exact rendering-equation integral.

## Accepted temporal lifecycle

The feature branch accepts the third bootstrap contract above: the first complete field may be a
lower-order estimate and may visibly converge afterward. This removes the private S0 and the rule
that publication waits for S1. It does **not** claim that one update has solved the same light paths
as the old two-update bootstrap.

The consumer-facing lifecycle is now specified as:

```text
Building -> Converging -> Converged
```

- **Building** means no complete field for the requested geometry/density revision exists yet. The
  previous Active field remains visible while a Staging volume builds.
- **Converging** means at least one complete current-revision field has been atomically published;
  subsequent update epochs improve angular sampling and recursively propagate transport.
- **Converged** means the variability criteria have held for the required consecutive epochs. The
  scheduler sleeps and the published atlases remain read-only until a tracked event wakes it.

These are lifecycle states, not bounce-order labels. A field no longer claims to be S0, S1, or S2.
Its identity needs `serial`, `geometry_revision`, `radiance_revision`, `spacing_voxels`, and
`update_epoch`. `update_epoch` is diagnostic and sequences samples; it does not promise a precise
bounce count. A hard safety budget, if retained, is a scheduler pause reason while the field remains
`Converging`, not a `NonConverged` transport stage.

### First complete current-revision field

Initial load and geometry/density changes use a fresh Staging volume with zero valid history. Epoch
0 traces current geometry immediately:

- misses contribute the authored sky;
- front-face hits contribute current direct-sun diffuse radiance;
- indirect hit radiance reads a defined black source, so no old-geometry irradiance or visibility
  can enter the result;
- irradiance and visibility history weights are zero.

After **every valid probe** has been traced, filtered, guttered, reduced, and validated, this field
is promoted atomically and displayed as `Converging`. With the measured 64-ray workload this changes
the publication critical path from two full probe iterations to one. Sky-lit surface reflection and
higher-order transport still appear in later epochs, which is an accepted visual convergence cost.
No partially updated atlas is ever consumer-visible.

### One rotation and two immutable fields per update epoch

An update epoch owns one uniformly distributed global SO(3) rotation of the 64-direction Fibonacci
set. The same rotation must be used by:

- all probes and all multi-frame batches in the epoch;
- probe tracing;
- irradiance filtering; and
- visibility-moment filtering.

Generating a new rotation per display frame would be incorrect because one epoch currently spans
about ten frames: different probe batches would use different sampling bases, and forward/reverse
batch order would no longer describe the same computation. Generate the rotation from a
deterministic hash/PRNG of the field generation plus `update_epoch`, store it on scheduled work, and
advance it only when the complete epoch is published. RTXGI likewise computes a volume-wide probe
ray rotation when the volume is updated ([RTXGI source: `DDGIVolumeBase::Update`][rtxgi-volume-source]).

Each epoch is a Jacobi-style operation:

```text
immutable complete source field + epoch rotation
    -> trace/filter every batch into private destination field
    -> validate the complete destination
    -> atomically publish destination as the next source
```

The destination must never be sampled by another batch in the same epoch. Irradiance already has
two physical slots, but visibility currently has only one atlas and is written only during S0. Once
visibility uses rotated temporal samples, it also needs source/destination slots. Otherwise the
renderer and later batches can observe partially accumulated moments. At spacing 32 the additional
`RG32F` visibility atlas is 12,882,240 bytes per physical volume; this memory increase is an explicit
cost of preserving the complete-field contract.

### Hysteresis and history accumulation

For each irradiance or visibility texel, let `sample` be the filtered result of this epoch and
`history` be the matching texel in the immutable source field:

```text
destination = H * history + (1 - H) * sample
```

`H` is the **history retention** (DDGI hysteresis), not the new-sample alpha. The paper describes the
new result entering at `1 - H`, with `H` in the 0.85-0.98 range ([paper, section 4.4][paper-pdf]).
RTXGI's shader implements the same weighting and lowers hysteresis for large changes
([RTXGI probe blending source][rtxgi-probe-blending]).

The initial implementation should expose one GUI value named `DDGI History Retention`, defaulting
to `0.95`, while applying these non-optional overrides:

| Condition | Effective history retention |
|---|---|
| Fresh geometry/density history or uninitialized texel | `0` |
| First epoch after a radiance revision | `0` for atlas blending; the immutable old field may still be queried for recursive hit shading |
| Large finite per-texel radiance change | lower toward `0`, using the existing absolute/relative thresholds initially |
| Ordinary same-revision convergence | `min(gui_H, sample_age / (sample_age + 1))` |

The sample-age cap makes the early epochs a running average rather than allowing the first noisy
64-ray estimate to retain 95% weight immediately. It later becomes a bounded EMA at the configured
history retention. This is an implementation hypothesis, not a claim from the paper, and must be
A/B tested against constant `H=0.95`. Do not copy RTXGI's low-bit-depth darkening step or brightness
impulse clamp initially: Re: Flora stores irradiance in `RGBA32F`, so those heuristics add response
lag without addressing the format problem they document.

Transport feedback and history blending remain separate operations: ray-hit shading queries the
immutable source field to propagate indirect light; the filter then blends the newly traced sample
with that same source field to reduce sampling variance.

### Convergence, sleep, and wake

The current post-blend atlas delta cannot be the only stop metric. With `H=0.95`, it is mechanically
about 20 times smaller than the raw sample/history difference and can produce a false early sleep.
Each epoch must reduce at least:

1. a raw pre-blend irradiance variability metric, such as mean luminance coefficient of variation
   over valid texels (the RTXGI variability feature uses coefficient of variation as its proxy);
2. post-blend absolute and relative atlas deltas as a stability guard; and
3. non-finite and validity counts.

Sleep requires a minimum epoch count plus consecutive below-threshold epochs. The exact minimum,
variability threshold, and consecutive count are experiment outputs, not values to guess into the
design. Max-only variability is expected to be too sensitive to one texel; record mean, percentile,
and maximum during tuning. A sleeping field performs no trace or filter work and does not rotate its
rays.

| Event | History policy | Wake behavior |
|---|---|---|
| Geometry or density revision | fresh Staging, no old-revision history | build epoch 0, publish, converge |
| Sun/sky/palette radiance revision with matching geometry | keep immutable source for recursive shading; reset blend sample age and first-epoch `H` | update until stable |
| Camera-only movement | keep history | do not wake |
| No tracked change | keep history | remain asleep |

### Migration order and experiment gates

Implement and commit each validated step separately:

1. Replace `DdgiFieldStage` transport semantics with epoch identity and lifecycle state. Publish a
   direct-plus-sky epoch 0 after one complete update, while preserving old Active during Staging.
2. Add deterministic epoch rotation to trace and both filters; prove the same rotation survives all
   batches and capture metadata.
3. Ping-pong visibility as well as irradiance, then add source-to-destination history blending with
   forced reset behavior.
4. Add raw variability reduction, lifecycle sleep/wake, GUI history retention, and diagnostics.
5. Delete S0/S1/S2 branches, capture labels, tests, and documentation only after their replacement
   assertions cover publication and source ownership. Do not retain compatibility dead code.

The branch is not complete until release-mode evidence passes all of these gates:

- first current-revision publication takes one full iteration, not two;
- old Active indirect light remains visible throughout a terrain edit until epoch 0 promotion;
- epoch 0 contains finite sky and direct-sun response and never samples old-geometry history;
- forward and reverse batch order produce equivalent complete fields for the same epoch seed;
- repeated static epochs add angular information and reduce donor/portal noise at 64 rays;
- donor S1-like color transfer and dogleg multi-epoch propagation remain measurable without relying
  on stage labels;
- wall add/remove and portal open/close do not retain old visibility after epoch 0 promotion;
- radiance changes respond on the first epoch and then settle without permanent ghosting;
- a fixed camera converges, sleeps with zero DDGI dispatches, and wakes only for tracked changes;
- matched release measurements report time to first field, time to sleep, total DDGI GPU work,
  temporal variance, and the extra visibility-atlas memory.

Unit tests must cover identity, source/destination ownership, reset policy, deterministic rotation,
batch-order invariance, scheduler sleep/wake, and stale completion rejection. Final validation uses
`cargo fmt --check`, `cargo check`, `cargo test`, hidden muted release startup plus log inspection,
and the deterministic donor/dogleg/edit/capture workloads. Debug timings are not performance
evidence.

## Primary sources

- Zander Majercik et al., [*Dynamic Diffuse Global Illumination with Ray-Traced Irradiance
  Fields*][paper-page], JCGT 8(2), 2019. See sections 4, 4.2-4.4, and 5.3.
- NVIDIA, [RTXGI DDGI Integration Guide][rtxgi-integration] and [DDGIVolume
  Reference][rtxgi-volume], [`DDGIVolumeBase` random-rotation source][rtxgi-volume-source], and
  [probe-blending source][rtxgi-probe-blending], all pinned to upstream commit `f33e496c`.

[paper-page]: https://jcgt.org/published/0008/02/01/
[paper-pdf]: https://jcgt.org/published/0008/02/01/paper-lowres.pdf
[rtxgi-integration]: https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Integration.md#tracing-probe-rays-for-a-ddgivolume
[rtxgi-volume]: https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/DDGIVolume.md#updating-a-ddgivolume
[rtxgi-volume-update]: https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/DDGIVolume.md#updating-a-ddgivolume
[rtxgi-variability]: https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/DDGIVolume.md#probe-variability
[rtxgi-volume-source]: https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/src/ddgi/DDGIVolume.cpp#L103-L109
[rtxgi-probe-blending]: https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/ProbeBlendingCS.hlsl#L505-L570
[stage-enum]: ../../../src/ddgi/scheduler.rs#L8-L55
[bootstrap-publication]: ../../../src/ddgi/resources.rs#L1368-L1404
[probe-trace]: ../../../shader/slang/ddgi_probe_trace.slang#L61-L180
[transport-spec-secondary]: ../../ddgi_indirect_transport_spec.md#L162-L176
[transport-current-policy]: ../../ddgi_indirect_transport_spec.md#L187-L193
[transport-acceptance]: ../../ddgi_transport_acceptance.md#L45-L60
[terrain-reference]: ../../../shader/slang/tracer.slang#L188-L278
[path-tracing-gui]: ../../../config/gui.toml#L138-L171
[current-convergence-policy]: ../../../src/ddgi/resources.rs#L26-L43
[scheduler-stop-rule]: ../../../src/ddgi/scheduler.rs#L446-L485
