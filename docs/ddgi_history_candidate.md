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
now latch at the next field (see final validation below), without touching source readiness,
source metadata, or resources.
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

## Final validation and recommended review scope

**Preserved as an opt-in visual candidate at 32-voxel spacing, not a general flicker fix.**
Both saved checkboxes remain false by default. The Debug group explicitly warns that
64-voxel left-wall spikes regressed. No visible app, merge or release was launched.

### Attribution and final repeated result

Final no-capture diagnostics for the first edit e0 (`final-baseline-timing`): **4,905 Retain,
8 zero-retention Blend**. Thus the baseline does not reset the whole field: a very small
reset partition can affect this receiver. Candidate: **3,835 qualified Blend, 1,078
placement/validity resets, zero Retain, zero zero-weight Blend**. The diagnostic now uses the
execution owner's actual decision, including invalid-fresh replacement, rather than merely
the requested policy. These counts distinguish partition retention, temporal reset and
placement rejection; they are not directional ray-validity proofs.

`final-combined-reference` reruns the current renderer, not just earlier code:

```
python3 scripts/check_ddgi_cave_edits.py target/history-temporal/final-combined-reference --temporal --case cave-edits-portal --reference --max-mean-jump 1 --aggregate-history --continuous-sampling
```

40 edits, 22 publications, complete temporal coverage and no runtime errors. Left mean-jump
p95 **.002654**, p99/max **.0033705**; peak spatial p99 **0**, pixel max **.66667**, >3 area **0**.
Right mean-jump p95 **.006608**, p99/max **.0225604**; peak spatial p99 **.33333**, pixel max
**1.66667**, >3 area **0**. These tiny code-value differences are not evidence that the field
is frozen: all eligible probes incorporate fresh transport, and actual response controls
below change by >23 code values. Reference at 56s: left mean **.024679**, RMSE **.157094**,
p99 **.66667**; right mean **.014859**, RMSE **.121896**, p99 **.33333**. Both pass <=1/255.

Reverse progression (`aggregate-reverse`, `combined-reverse`) also passed: history-only left
max .061625 / spatial p99 1; combined .0033705 / spatial p99 0. The baseline reverse result
was 1.287537 / 7.333. No independent random-seed claim; traversal and density cases are the
progression variations exercised.

### Negative density evidence and discarded visibility experiment

The same portal and camera at spacing 64 is a substantially harder, coarser representation.
All runs completed 40 edits and 39 publications. Mean-jump maxima / peak spatial p99:

| mode | left | right |
|---|---:|---:|
| original | 1.30400 / 52.333 | 2.81643 / 63.333 |
| sequence only | 1.67981 / 61 | 1.49300 / 45.333 |
| aggregate only | 1.53729 / 67.667 | 2.15081 / 51 |
| both | 1.56672 / 75 | .95817 / 35.333 |

The left regression is real negative evidence, not waived by averaging both walls. A further
single-variable experiment replaced dirty visibility but blended remote moments at .93,
instead of the selected all-probe original-epoch response. It worsened spacing-64 maxima to
**2.43103 / 3.44337** and spatial p99 to **83.667 / 75**, despite looking slightly better at
32. **Discarded its implementation**; no extra checkbox ships. Reproduction artifacts:
`combined-qualified-vis{32,64}` contain that exploratory source diff and binary hash.
The selected candidate remains the earlier current-moment-refresh mode. Spacing 16 and the
user's near-surface cave are untested. Do not enable these modes universally on this evidence.

### Response, compatibility and actual runtime toggles

The existing runner, not a duplicated orchestrator, gained `--response` and `--no-capture`.
`combined-response --response --duration 30 --aggregate-history --continuous-sampling`
uses existing real opening/closing cases. At 28s, fixed receiver means are initial-open
**24.1202**, closed **.341535**, opened **23.8023** /255. Both response directions pass the
explicit >10-code-value change guard. This is settled response evidence, **not measured
10%/90% latency curves**, and these three different geometries are not used as mutual
reference-error targets. The portal reference alone preserves exact final geometry.

`combined-compatibility` ran the original five-run cave/opening matrix: pass, active and
repeat excess over settled **-.000008** /255, real opening **23.8901** versus settled
**.317346**. `combined-sustained --temporal --case terrain-edits-sustained --duration 20
--max-mean-jump 255 --aggregate-history --continuous-sampling` passed 40 actual alternating
open/close edits and 22 during-gesture publications. The deliberately nonrestrictive temporal
threshold there is for liveness, not a flicker success claim during genuine light changes.

New bounded fixture `cave-edits-history-toggles` shares *exactly* the portal geometry and
forty removal bounds; it writes the same saved live controls at edits 0/8/16/24/32:
original -> sequence -> aggregate -> both -> original. Its unit test compares compiled
geometry and checks the phase table. Both `runtime-toggles` and `runtime-toggles-latched`
hidden GPU runs completed all five modes, 40 edits and live publications without errors.
`DdgiExperimentLatch` freezes those controls for one field: mid-sweep changes cannot mix
history policies inside one capture proof. Next-field application is shown in the Debug
panel and `[DDGI][EXPERIMENT]` logs. Unit tests cover retry, idle frames and switching back.
No source buffers, source readiness, allocation or terrain changes are needed for a toggle.

### No-capture cost, separate from capture evidence

```
python3 scripts/check_ddgi_cave_edits.py target/history-temporal/final-baseline-timing --no-capture --case cave-edits-portal --duration 20
python3 scripts/check_ddgi_cave_edits.py target/history-temporal/final-combined-timing --no-capture --case cave-edits-portal --duration 20 --aggregate-history --continuous-sampling
```

Same release binary, camera/config except controls; no screenshot readbacks or PNG encoding. Sampled active
GPU times in microseconds (the existing periodic profiler reports only 9–10 samples per pass,
so p95/p99 often equal max, **not every-frame tail estimates**):

| scope | original p50 / p95=p99=max | candidate p50 / p95=p99=max |
|---|---:|---:|
| frame.render | 2787 / 3515 | 2774 / 4077 |
| probe trace | 289 / 712 | 297 / 445 |
| irradiance filter | 7 / 8 | 18 / 24 |
| visibility filter | 9 / 11 | 56 / 80 |

Mean frame.render 2876.4 -> 2949.8 us (~2.55%). Earlier pairs recorded 5099.2 -> 5119.8 and
3291 -> 3452.5 us, showing substantial run-to-run GPU variation. Filters clearly cost more;
no optimization, performance acceptance, or arbitrary performance gate is claimed. The
candidate is offered for visual review first. All GPU runs held `/tmp/re-flora-summer-gpu.lock`
and unset WAYLAND_DISPLAY.

### Validation and artifact audit

- `cargo fmt --check`, `cargo check`, `cargo test`: **1035 main + 4 library passed, 2 ignored**.
  Logs `target/history-final-{fmt,check,tests}.log`.
- `python3 scripts/run_slang_tests.py`: **17 passed**, `target/history-final-slang.log`.
- `python3 -m unittest scripts.tests.test_ddgi_temporal scripts.tests.test_analyze_environment_irradiance_capture`:
  **69 passed**, `target/history-final-python.log`.
- Full Python discover: **349 run, 1 error**, specifically the unchanged missing `bd2` camera
  snapshot in `test_declared_bd2_snapshot_reaches_the_screenshot_analyzer`; all others passed.
  Exact traceback in `target/history-full-python.log`. Not a blanket Python waiver.
- Final locked hidden/muted release 0.5s smoke: `history-final-smoke{,-tail}.log`, failures=0,
  no ERROR/panic/VUID. No tree/physics/material source or common uniform layout changed; a
  separate tree GPU smoke was not required or run.
- Candidate images inspected: `target/history-{baseline,candidate}-view.png` (960x540 display
  copies of the unchanged portal scene near baseline's maximum jump). Full-resolution source
  sequences remain beside each report. The cavity is very dark: no claim of strong near-wall
  reproduction or visually approved improvement. No exposure change was made.
- One multi-command timing batch exceeded its 300s tool timeout during the last run; discarded
  `combined-timing-repeat`, restored the initially clean exact GUI/camera bytes, and reran at
  `combined-timing-repeat-complete`. The runner now stores exact `.before` backups, handles
  SIGTERM through restoration, and explicitly sets both controls for each run. SIGKILL cannot
  be caught. Final GUI/camera bytes match committed defaults; no Cargo.lock changes.

Changed source scopes across the three steps:
`src/ddgi/{resources,runtime,experiments,mod}.rs`, `src/tracer/{mod,pipeline_builder}.rs`,
`shader/slang/ddgi_{filter_policy,filter_evidence,irradiance_filter,irradiance_filter_owner,visibility_filter}.slang`,
`shader/tests/ddgi_aggregate_history.slang`, `config/gui.toml`,
`src/app/{core/render_frame_input,gui_config_loader,gui_config/debug_groups}.rs`,
`src/app/core/environment_lighting_test_scene.rs`, `src/cli.rs`,
`scripts/check_ddgi_cave_edits.py`, `scripts/tests/test_ddgi_temporal.py`, and these reports.
Only generated file changed: `src/app/generated/gui_adjustables_gen.rs`, regenerated by
cargo check. The original portal fixture, source-ready/source-owned fixes, geometry, physics,
materials and tree behavior remain intact. Remaining gaps: saved-cave/near-surface reproduction,
HDR/path-traced comparison, stronger seed coverage, 64-voxel regression, spacing 16, detailed
response curves and post-visual-approval performance optimization.
