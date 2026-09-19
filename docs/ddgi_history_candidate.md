# DDGI history candidate experiments

Worktree `re-flora-agent-ddgi-history-candidate`, base `06d8257b`. Only the existing
portal far-wall symptom is reproduced; near-surface/user-saved-cave flicker remains
unverified. Display RGB is sampled, not HDR or a physical reference.

## Experiment 1: sampling progression only

Saved Debug / Lighting Diagnostics checkbox **DDGI continuous accepted-batch sampling
(experimental A/B)** defaults unchecked (original). Checked mode retains a deterministic
spatial-batch sequence across geometry allocations. Trace and both filters use the same
rotation. Only successful runtime batch completion advances the position; stale readback,
cancel-before-completion and encoding retries do not. Accepted but subsequently superseded
work consumes a position, rather than reusing its directions. Retained/untraced probes can
skip positions: this schedule is not an effective sample-count estimator. Different
physical batches advance independently; spacing is part of their identity. Live changes
switch the next batch without touching source readiness, source metadata, or resources.
The baseline still uses the original revision/epoch rotation and history policy.

Hypotheses in order: revision reseeding; e0 aggregate resets; relocation/visibility
invalidation; nonlocal recursive response. The first experiment changes only the first.

Commands (fresh directories under `target/history-temporal/`):

```
python3 scripts/check_ddgi_cave_edits.py target/history-temporal/baseline --temporal --case cave-edits-portal --duration 20 --max-mean-jump 1
python3 scripts/check_ddgi_cave_edits.py target/history-temporal/sequence-v2 --temporal --case cave-edits-portal --duration 20 --max-mean-jump 1 --continuous-sampling
python3 scripts/check_ddgi_cave_edits.py target/history-temporal/sequence-reference-complete --temporal --case cave-edits-portal --reference --max-mean-jump 1 --continuous-sampling
```

Matching camera/spacing/config except checkbox. Baseline RED, candidate GREEN, all 40 edits
and 22 live publications, no runtime errors. Left wall:

| run | temporal mean-jump p95 | p99/max | peak spatial p99 | peak pixel | peak >3 area |
|---|---:|---:|---:|---:|---:|
| original | .02786 | 1.28755 | 7.333 | 8.333 | 17.231% |
| continuous | .05929 | .21089 | 2.0 | 4.0 | .0719% |

Units are /255 except area. Ordinary p95 increases while extreme spikes fall; do not
summarize this as improvement of every metric. Right mean-jump max .23457 -> .02213.
Reference repeat max .21121. Reverse traversal (`--batch-order reverse`, fresh
`baseline-reverse` and `sequence-reverse`) yielded 1.28754 -> .21123 and spatial p99
7.333 -> 2.0. This is a second progression order, **not independent random-seed coverage**.

Independent-init final-geometry comparison at 56s (same candidate settings), explicit
spatial p99 budget <=1/255: left mean .02468, RMSE .15709, p99 .66667; right mean .01486,
RMSE .12191, p99 .33333. Passes; DDGI approximation, not physical truth.

An initial implementation mistakenly reset progression on staging allocation: its run
`sequence` is negative evidence (1.28755 unchanged), not the corrected candidate.
`sequence-reference` was interrupted by the tool's 200s timeout during its third run;
not accepted evidence. Exact initially-clean camera and unchecked GUI bytes were restored,
and the entire matrix rerun with a 400s timeout (`sequence-reference-complete`).

Validation: `cargo fmt --check`, `cargo check`; full Rust tests `sequence-tests-final.log`
(1033 main + 4 library passed, 2 ignored), 67 targeted temporal/analyzer Python tests,
16 Slang CPU tests. One earlier test correctly caught an unclassified new Debug setting;
classified it under Lighting Diagnostics and reran. Locked X11 hidden release 0.5s smoke
(`sequence-smoke{,-tail}.log`) ends failures=0, no ERROR/panic/VUID. GUI migration/save,
frame-input wiring, accepted-work/revision/density sequence tests added. Generated file:
`src/app/generated/gui_adjustables_gen.rs` via cargo check. No Cargo.lock changes.

These capture timings are not performance evidence. Opening/closing compatibility,
no-capture performance, stronger sequence variation, live-toggle GPU exercises and actual
geometry-qualified aggregate reuse are the next experiment stage, not claimed complete.

## Experiment 2: placement-qualified aggregate history

Independent saved checkbox **DDGI geometry-qualified aggregate history (experimental A/B)**,
unchecked original. The original edit bounds are latched with scheduled work and retained
only through local recovery (not reconstructed by shrinking clamped invalidation bounds).
An irradiance prior qualifies only with valid source/current probes, exactly equal nominal
and actual placement/clearance, and an edit box disjoint from that clearance ball. Changed
placement, clearance, invalid probes or edits entering that region replace history. This is
**approximate aggregate prior qualification**, not validation of its individual old rays or
proof of invariant radiance. No old ray contribution can be removed exactly from this atlas.

Qualified irradiance uses the existing topology-recovery cap min(configured, .93), not
an unbounded accumulated sample age. Every probe blends freshly traced current-visibility
transport, including the original Retain partition. 32 accepted updates reduce a constant
old-history error below 10% (recursive field settling is not guaranteed by that scalar bound).
Visibility does **not** inherit mature history: every probe refreshes moments at the original
epoch cap, and moved/invalid probes replace. Thus continuous local edits cannot indefinitely
freeze remote moments. Geometry-changing e0 reads source-owned metadata binding 40; subsequent
same-geometry updates use destination metadata. Readiness/source tuple ownership is unchanged.

Five separate diagnostic lanes report original Retain, zero-retention Blend, qualified Blend,
placement/validity reset and edit-proximity reset; RFIRR owner/proof lanes are unchanged.
`[DDGI][HISTORY]` records batch identity and those counts. For the first portal edit, 3,835 of
4,913 probes qualified and 1,078 were placement/validity resets. These are not all relocated:
invalid probes are included explicitly. Full per-batch records are retained in run logs.

Measured single-variable sequence (same source/camera/geometry, 40 edits / 22 publications):

| mode | left mean-jump p95 | p99/max | peak spatial p99 | peak pixel | >3 area |
|---|---:|---:|---:|---:|---:|
| history only, original remote moments (diagnostic) | .00687 | .05938 | 1 | 1 | 0 |
| history only, current remote moments (candidate) | .00964 | .06163 | 1 | 1 | 0 |
| both candidates | .00265 | .00337 | 0 | 1 | 0 |

The remote-moment diagnostic was revised, not retained as a selectable mode. Runs:
`aggregate-alone`, `aggregate-reference` (diagnostic), `aggregate-current-visibility`,
`combined`, and `combined-reference`. Same runner flags plus `--aggregate-history`, with
`--continuous-sampling` only for combined runs. Combined independent-reference error at 56s:
left mean .02468 / p99 .66667; right mean .01486 / p99 .33333, within the explicit <=1 budget.

Validation: full Rust 1033 main + 4 library passed (2 ignored), all 17 Slang CPU tests,
fmt/check and locked hidden release smoke (`aggregate-smoke{,-tail}.log`, failures=0).
New Slang tests exercise unchanged placement, moved probes, changed clearance, failed source,
edit intersection and both increasing/decreasing transport response. Existing source-owner
wiring and trace-buffer-size tests caught required integration changes and pass after fixes;
a test initially projected a batch from an uninitialized fixture, corrected to inspect its
latched work instead. Generated GUI file only. No physical geometry/material behavior changed.
Response compatibility and no-capture timing remain a separate validation step below.
