# DDGI edit-history reuse investigation

Worktree: `/home/terence/code/re-flora-agent-ddgi-history-reuse`, starting commit
`6fb81dfa`. This is an in-progress measurement effort, **not a validated flicker fix**.

## Temporal capture foundation

The merged baseline passed locked hidden/muted X11 release startup. Evidence:
`target/ddgi-baseline-smoke.log`; canonical log ends in `failures=0` and successful exit.

The existing screenshot owner now supports `--screenshot-sequence <count> <interval-sec>`
with `--screenshot <preset> <path> --screenshot-delay <sec>`. It writes numbered PNGs
and logs actual render-elapsed capture times (not writer completion times). Unlike a
single screenshot, a sequence deliberately observes unready scene/lighting frames.
There is at most one readback writer in flight; slow writers skip time rather than
queue duplicate catch-up captures. This is sampled display RGB, not HDR and not a
claim of every-frame coverage. Capture overhead must be reported separately from
performance runs. The owner joins its final writer before destruction.

Validation: `cargo fmt --check`, `cargo check`, `cargo test` (1029 main tests passed,
2 ignored), 12 targeted screenshot tests. Locked hidden release command:

```
env -u WAYLAND_DISPLAY flock --close /tmp/re-flora-summer-gpu.lock \
  target/release/re-flora --hidden --mute \
  --screenshot player-default target/ddgi-sequence-smoke --screenshot-delay 0 \
  --screenshot-sequence 4 0.1 --auto-exit 1
```

Four fresh numbered images and no ERROR/panic/VUID; logs
`target/ddgi-sequence-{check,all-tests,smoke,smoke-tail}.log`. No shader, generated
files, GUI defaults, terrain, physics, or lighting algorithm changed in this step.
No visible game launched. Temporal symptom reproduction and history experiments
remain outstanding; these startup images are capture plumbing evidence only.

## Sampled temporal analyzer and merged cave baseline

`python3 scripts/check_ddgi_cave_edits.py target/history-temporal/sealed-baseline --temporal`
ran successfully: 40 edits, 22 complete during-edit publications, 47 captures recorded
during the gesture. Maximum gap 105.179ms (requested 100ms). Left lower-wall maximum
frame mean absolute RGB jump was 0.0000294/255; right was zero. Maximum individual
pixel mean-RGB jump was 1/255 on the left. Neither wall had pixels jumping >3/255.
This **does not reproduce the remaining strong flicker**: the sealed fixture is a
negative control, not justification for a history change. ROI labels remain left/right,
not near/far: depth and stable surface provenance still need a nonblack scene audit.

The same runner now supports sampled sequences, retains all screenshots and actual
capture timestamps, emits per-frame pixel p95/p99/max/mean jumps and spike area, then
summarizes those measures across time. It rejects nonempty output directories, missing
write-completion markers, missing frames, nonmonotonic times, incomplete edit intervals,
insufficient captures, and large capture gaps. A configurable mean-jump threshold can
fail a run; this is a measurement threshold, never a renderer brightness cap. Spatial
spike measures are separately reported rather than hidden by the ROI mean. Exact HDR
readback and the legacy RFIRR evidence-owner panic are not exercised or waived.

The original five-run matrix also passed at
`target/history-temporal/compatibility-baseline/`; active-repeat minus settled was
0.000006/255, real opening still brightens. All captures are fresh and output includes
Git revision/diff, config hash, commands, copied run logs, and image paths. Tests:
`python3 -m unittest scripts.tests.test_ddgi_temporal scripts.tests.test_analyze_environment_irradiance_capture`
(67 passed). Tests explicitly catch a spatial spike diluted in the frame mean, delayed
writer completion, missing files, and absent successful-write markers. No lighting
algorithm or shader changed. No performance improvement is claimed.
