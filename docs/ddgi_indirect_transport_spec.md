# DDGI Temporal Indirect Transport Specification

Status: `implemented-and-acceptance-qualified` (2026-08-17)

This document is the current runtime contract. The former `SeedSky` / `SingleBounce` / `Feedback`
stage model is historical and is not part of the runtime API.

## Goal

Re: Flora transports diffuse terrain lighting through a probe volume while preserving these player-
visible properties:

- direct sun and its crisp visible-surface shadows remain outside DDGI;
- sky and sun reflected by terrain can illuminate and tint other surfaces;
- a complete resident field remains visible while terrain, density, or radiance changes build;
- no partial probe batch becomes consumer-visible;
- a static scene eventually stops spending probe-update work.

The implementation is temporally sampled DDGI, not per-hit path tracing. A probe ray may perform an
exact direct-sun shadow ray at its terrain hit and may query the previous complete DDGI field there.
It does not spawn a secondary hemisphere or a ray tree.

## Lifecycle and identity

Physical work and logical field state are intentionally separate:

```text
unpublished staging work -> Converging e0 -> Converging e1 ... -> Converged
```

- `Building` is physical work owned by Active/Staging volume resources. It is not a field state.
- `Converging` means a complete finite field for the requested revision is already published.
- `Converged` means the field has gone to sleep after satisfying the threshold policy or exhausting
  the finite temporal sample budget.
- `update_epoch` counts complete temporal samples for one geometry/radiance/spacing identity. It is
  neither a display frame nor a claimed light-bounce order.

Every field identity contains a unique serial, geometry revision, radiance revision, spacing,
lifecycle state, and epoch. Epoch zero has no same-revision history after a geometry or density
change. Every later epoch names the previous complete field as its immutable source. A radiance
change also restarts at epoch zero, while retaining the previous same-geometry field as an explicit
source for continuity and recursive hit shading.

## First publication and continuity

For geometry and density updates, epoch zero:

- traces the current geometry and radiance snapshot;
- records authored sky on misses;
- records terrain albedo times exact direct-sun irradiance on front-face hits;
- uses zero indirect history and zero history retention;
- writes both irradiance and visibility to a private destination;
- publishes atomically after the entire atlas validates.

This replaces the old rule that waited for two complete full-volume updates before publication.
The prior Active volume remains bound while Staging builds, so editing never deliberately blanks all
indirect lighting. Exact voxel visibility follows the latest published terrain while stale
irradiance, relocation, and moment visibility remain explicitly owned by the older field until the
replacement epoch zero promotes.

Geometry publication is strict latest-revision-wins. One physical Staging update runs at a time;
new edits coalesce, obsolete candidates cannot promote, and geometry preempts density and temporal
convergence. Radiance changes finish one immutable in-flight epoch and coalesce queued changes to
the latest snapshot.

## Sampling and temporal accumulation

Each epoch uses the fixed 64-direction Fibonacci set under one deterministic, uniformly distributed
SO(3) rotation derived from geometry revision, radiance revision, and epoch. The same rotation is
used by every probe batch and by trace, irradiance filter, and visibility filter. This adds angular
samples across epochs without introducing batch seams or per-step random-number work.

Irradiance and two-moment visibility each have two full atlas slots. An epoch reads one immutable
source slot and writes the other; source/destination ownership changes only after full-atlas
validation. The temporal filter is:

```text
destination = H * history + (1 - H) * current_sample
```

`H` is history retention, traditionally called DDGI hysteresis. It is not the new-sample alpha.
The GUI exposes `DDGI History Retention` in `[0, 0.99]`, with default `0.99`. A value of zero disables
the temporal blend while retaining rotated samples and recursive previous-field transport.

History validity and the effective value are:

| Update | Irradiance H | Visibility H |
|---|---:|---:|
| geometry or density epoch zero | 0 | 0 |
| radiance epoch zero | 0 | configured H |
| unchanged revision epoch `e > 0` | `min(configured H, e/(e+1))` | same |

The sample-age cap gives early samples equal-weight running-average behavior (`e1 = 0.5`, `e2 =
0.667`, and so on) before reaching the configured EMA retention. A radiance change invalidates
irradiance history but can preserve visibility history because geometry is unchanged.

## Convergence and sleep

After every full epoch the runtime scans all valid stored irradiance texels. A field sleeps when:

- at least 8 complete epochs exist;
- maximum absolute RGB delta is at most `0.0025`;
- maximum relative RGB delta is at most `0.02`, using relative floor `0.05`;
- both thresholds pass for two consecutive epochs.

Epoch 63 is a hard finite backstop (64 complete temporal samples). Reaching it publishes the latest
finite field and transitions to `Converged` with reason `SampleBudget`; it does not falsely claim
that the numerical thresholds passed. This distinction is recorded in logs and convergence
evidence.

A `Converged` static field schedules no more probe work. Geometry, density, or radiance changes wake
the system by creating a new epoch-zero target. There is no periodic refresh solely because display
frames continue.

Current convergence metrics are post-blend atlas deltas. They are useful sleep signals but can be
reduced by high history retention; raw pre-blend variability and per-probe adaptive wake/sleep are
future refinements, not implied by the current `Threshold` reason.

## Lighting and energy contract

- Consumers receive linear diffuse irradiance divided by pi and apply stable base albedo once.
- A front-face probe hit uses stable voxel type/hash albedo, current exact terrain direct sun, and
  the previous field's visibility-aware diffuse irradiance when a source exists.
- Moisture, fertility, edit-preview tint, VSM, leaf shadows, and cloud shadows are excluded from
  probe-hit transport.
- A back-face hit does not contribute radiance; misses use the latched authored sky.
- DDGI storage is non-negative, unclamped linear HDR. Non-finite output cannot publish.
- Terrain and Flora share the same published DDGI consumer identity and moment-visibility query.

## Resource and publication contract

Both irradiance (`RGBA32F`) and visibility (`RG32F`) ping-pong. At spacing 32, the second visibility
atlas adds `12,882,240` bytes (`12.29 MiB`); the full measured DDGI allocation is `40.47 MiB`, versus
approximately `28.18 MiB` before visibility history was added. Spacing 16 remains substantially
larger and is a quality/debug option.

Probe work is batched across render frames. A partial destination, failed validation, obsolete
token, or superseded geometry revision is never bound to terrain or Flora. Descriptor rebinding and
Active/Staging promotion occur only for one complete field.

## Validation contract

The authoritative end-to-end seam is the hidden release renderer plus `.rfirr` capture analysis:

- capture v10 records lifecycle state, epoch, source identity, revisions, publication, batch order,
  full-atlas deltas, source-separated terrain/leaf/cloud direct-shadow transmittance, authoritative
  probe-grid dimensions, configured history-retention Q16 identity, and exact owner-generated
  filter evidence. History evidence retains action partitions plus Q16 retention sum/max witnesses;
  owner masks must contain only the expected owner-version bit. The configured identity comes from
  the App/Tracer input, while action/count/retention witnesses come independently from GPU owners;
- sealed, portal, donor, and dogleg scenes exercise no-created-energy, leak, color-transfer, and
  multi-epoch propagation behavior at spacing 32 and 16;
- forward/reverse epoch-zero captures verify batch-order independence;
- terrain-edit, radiance-change, and density-change scenarios verify resident-field continuity,
  coalescing, preemption, and first-epoch publication;
- terminal correctness captures explicitly target `Converged`; the Moment-only thin-wall
  exact-reference P99 ceilings are `0.400` at spacing 32 and `0.375` at spacing 16;
- general renderer validation follows `cargo fmt --check`, `cargo check`, `cargo test`, and a hidden
  muted release run with log inspection.

Source-shape audits only guard production wiring into the owner terminal store/accumulate seam.
They do not constitute runtime action proof; that claim requires a complete RFIRR v10 GPU epoch.
The analyzer keeps the fixed RFIRR v8 reference layout and the published 252-byte/11Q v9 layout
readable, but production acceptance never infers a same-version layout from file length.
Production runners use the current-schema analyzer entry, whose interface has no numeric version
option; explicit historical-version selection remains confined to compatibility tests and tools.
Their normalized direct-call function is guarded only as a source-wiring tripwire; runtime schema
ownership comes from the current-only analyzer interface, not from static shell interpretation.
The seven production runners also expose a CPU-only `--dry-run` contract that executes their normal
matrix construction and emits the current-analyzer command at each canonical analysis call site.
Dry-run and production use the same call sites; only the canonical wrapper changes execution into
emission, while transport's narrow execution-policy helper selects an output sink around one
analyzer invocation. That proves the maintained normal-entry inventory, not arbitrary Bash
control-flow reachability. The source tripwire additionally rejects canonical analysis execution
under a syntactic `dry_run` `if`/`elif`/`else` chain. Its controlled grammar carries pending
single-line, backslash-continued, or multiline headers through an independent `then`, treats the
whole chain (including `else`) as dry-run-owned, inventories every raw analyzer identifier, and
locks transport to `cat` in dry-run, `/usr/bin/env tee "$json"` in production, and one two-stage
analyzer-to-sink pipeline. Behavioral dry-run tests require an unchanged whole repository tree.
After argument parsing, each runner makes `dry_run` readonly; its root path is a single readonly
canonical assignment. Its controlled stateful lexical pass distinguishes comments and quote state
across lines. Control structure is scoped to the outer runner: ordinary double-quoted text and
command-substitution child-shell bodies cannot contribute an outer `if`/`fi`, while the full
code/active streams retain real parameter expansions and child commands for authority auditing.
Braced expansions are recursively enumerated by exact base identifier, including length,
indirection, operator, array, and nested fallback forms; only base `dry_run` owns a dry-run chain.
Actual policy/root simple, compound, arithmetic, parameter, or loop-variable assignment, unset,
readonly, and expansion facts are inventoried. `[[...]]` comparisons remain non-assigning. A separate logical
command/argv seam fail-closes code loading (`eval`, `source`, `.`, and shell `-c`), authority targets
of `printf -v`, `read`, `readarray`/`mapfile`, `getopts`, and `let`, dynamic writer targets, and all
`declare`/`typeset`/`local` namerefs. Non-authority literal targets and command names contained only
in comments or quoted data remain allowed. Together these rules keep the wrapper's analyzer path
immutable without claiming arbitrary Bash interpretation. `/usr/bin/env` owns external tool
resolution for Cargo, decision-related
`tee` sinks, and transport normalization Python: it bypasses shell functions while retaining PATH
lookup. Direct shebang analyzers already use the same owner. PATH plus repository and external
absolute-path sentinels dynamically cover the known launch entrypoints. This is not a claim about
arbitrary Bash variable encoding or general process tracing.

Three matched RTX 3060 Ti release samples measured six terrain edit-to-epoch-zero promotions at
`31-36 ms` (median `34.5 ms`, p95 `36 ms`). The retained two-stage baseline observations were
`87/88 ms` (median `87.5 ms`). Publication bookkeeping itself remained `0.0095 ms` median. A
five-second static portal run also recorded zero scheduler claims after `Converged e63`.

See [DDGI transport acceptance](ddgi_transport_acceptance.md) and
[DDGI convergence calibration](ddgi_convergence_calibration.md) for commands and measured evidence.

## Out of scope

- replacing visible-surface direct sun with DDGI;
- per-hit indirect hemisphere rays, specular GI, reflection, refraction, or transmission;
- Flora becoming a DDGI occluder or bounced-light emitter;
- local dependency-exact refresh, cascades, paging, or camera-relative volumes;
- compact atlas formats, per-probe activity, adaptive ray budgets, and raw-variance convergence;
- hiding energy errors with an indirect-strength clamp.
