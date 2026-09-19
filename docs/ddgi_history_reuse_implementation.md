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

## Lit shallow-edit fixture: a limited red reproduction

Added `cave-edits-portal`: the existing 40 warm-start shallow roof removals, with a
persistent 16×20-voxel roof aperture disjoint from every removal. Unit tests verify
the aperture is initially empty and edit bounds never intersect it. Geometry changes
still use the real publication path; no lighting or rendering policy changed.

```
python3 scripts/check_ddgi_cave_edits.py target/history-temporal/portal-repeat \
  --temporal --case cave-edits-portal --duration 20 --max-mean-jump 1
```

This command ran RED with complete captures and live publication. The exploratory
1-code-value mean-jump threshold was selected after the first run, not a pre-existing
acceptance budget. First run (`portal-baseline`, default 3 threshold) had left-wall
maximum mean jump 1.29466/255, spatial p99 7.333/255, pixel maximum 8.333/255 and
17.287% area jumping >3/255. Right-wall max mean was 0.28378/255. Both had 40 edits,
22 promotions and 47 during-edit captures. The repeat also reproduces the mean-jump
failure; exact values and all per-frame statistics are in its report.

Fresh image `target/ddgi-portal-view.png` was inspected. This is a dark lit cavity,
not the user's saved scene. Exploratory stable near roof/side-wall ROIs did **not**
show strong flicker (<0.013 mean, <=0.667 pixel jump); the lower far-wall ROI does.
Therefore this establishes a limited far-wall noise loop, **not** full reproduction
of strong near AND far flicker. ROI exploration reports are retained beside the first
run; they were not used to rewrite its original measurements. No cause established.

Validation: fmt/check; full Rust tests 1030 passed, 2 ignored; all 16 Slang CPU tests.
Initial new unit test failed compilation using private AABB `.max` rather than the
public `.max()` accessor; corrected before validation. No GPU runtime errors in either
portal run. No generated files changed. Next required work remains independent final-
geometry DDGI reference, real response controls, and only then attributed experiments.

## Independent final-geometry DDGI comparison

`cave-edits-portal-final` builds the same shell, aperture and all 40 removal cuboids
in its initial transaction, with no prior DDGI history. The live edit fixture and
reference share the canonical removal-bound function. A unit test checks all removed
voxel centers were rock initially and are empty in the independently built final scene;
the reference has a static lifecycle, not a simulated editing history.

```
python3 scripts/check_ddgi_cave_edits.py target/history-temporal/portal-reference-final \
  --temporal --case cave-edits-portal --reference --max-mean-jump 1
```

Fresh release matrix: RED for transient wall jumps, complete/live. 40 edits, 22 during-
edit promotions, 46 during-edit captures, maximum sampled gap 104.349ms. Left wall
maximum mean jump 1.28755/255; right 0.23457/255. The captured edit stream has fewer
large jumps than the first run, so temporal p95 alone would conceal the repeated
p99/max failure. This is why the raw per-frame series remains in the report.

Two additional runs capture edited/independent-final scenes at 56 render-elapsed
seconds with identical camera/config/spacing. Display-RGB errors (not HDR):

| Receiver | mean absolute | component RMSE | pixel p99 | pixel maximum |
| --- | ---: | ---: | ---: | ---: |
| left wall | 0.007467 | 0.086410 | 0.333333 | 0.666667 |
| right wall | 0.022861 | 0.151199 | 0.333333 | 0.666667 |

These are **DDGI-reference comparisons**, not path-tracing truth. They support lack
of a large persistent history-path error in this specific baseline fixture after 56s;
they do not establish physical accuracy, a universal settling deadline, or correctness
of any proposed history reuse. The runner also enforces an explicit configurable
spatial p99 reference-error budget (default 1/255); this guard was added after the
matrix above and its reported values meet that budget. Binary and camera hashes are
now recorded along with source revision/diff and GUI hash.

Validation: fmt/check, 1031 main Rust tests passed (2 ignored), 67 targeted Python
analyzer tests. Earlier Slang run: all 16 CPU tests passed; no shader changed since.
Final locked release hidden/muted 0.5s smoke passed with `failures=0`; canonical tail
in `target/ddgi-reference-smoke-tail.log`. Existing sustained opening/closing runner
passed at `target/history-temporal/sustained-compatibility` (receiver 86/255 diagnostic,
not an acceptance brightness floor). Earlier five-run cave/opening compatibility also
passed. No ERROR/panic/VUID in these GPU runs. GUI/camera bytes are restored. No
Cargo.lock or generated files changed. No performance claim: sampled captures add
readback/PNG cost; these are not authoritative no-capture performance comparisons.

## Outstanding scope / handoff

**No lighting fix or candidate A/B checkbox has been implemented.** No history cap,
sequence, recursive feedback, visibility, relocation, global brightness, material,
wind or physics behavior has changed. The available lit fixture reproduces only modest
far-wall transients, not the requested strong near-and-far symptom. Implementing a
history-retention design before obtaining that loop would violate the diagnosis gate.
The user's saved lit excavation scene/camera or a stronger procedural reproducer is
still needed for that exact symptom. Near/far ROI depth auditing, linear HDR readback,
legacy RFIRR-owner panic diagnosis, distant multi-bounce response, quantified opening
AND closing response curves, multiple-seed coverage, and runtime A/B experiments remain
outstanding. The existing alternating skylight runner is liveness compatibility, not
those response/accuracy measurements. No runtime configuration migration is needed
because no saved setting was added. This branch is measurement groundwork, **not** a
finished solution to the requested history-reuse task.
