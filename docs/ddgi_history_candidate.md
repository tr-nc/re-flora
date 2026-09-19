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
