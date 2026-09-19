# Cave edit brightening investigation

Implementation evidence paths below are relative to the retained worker
`/home/terence/code/re-flora-agent-cave-edit-lighting`. The integration section records
independent verification in `/home/terence/code/re-flora-agent-butterfly-block-flight`.

## Reproduction (unfixed baseline)

`python3 scripts/check_ddgi_cave_edits.py target/cave-edits/warm-baseline3`

This release, hidden/muted runner holds `/tmp/re-flora-summer-gpu.lock`, restores exact
GUI/camera bytes, and measures the central 50% image rectangle (RGB excluding alpha).
`cave-edits` starts with a sealed room, waits for the initial field to converge, then
publishes 40 disjoint shallow roof-interior removals at >=100ms intervals. Eighteen
voxels of solid roof remain. The active screenshot is taken immediately after removal
40, while the field still reflects the ongoing edit stream. The settled screenshot
uses the same final geometry at 55 seconds. A repeat checks timing sensitivity;
`sealed` measures steady leakage, and `cave-edits-open` actually opens the skylight
on removal 40. The fixture's Ready state means the edits completed, not DDGI convergence.

Baseline at eb321645 plus harness: **RED**, active minus settled central display RGB
**40.694639 / 40.787639** code values; opening control **40.8779/255** absolute.
Both active images visibly show blue lighting throughout the sealed interior; settled
is nearly black. No runtime ERROR/panic/VUID. Complete publications continue during
all 40-edit runs. Images, commands, ROI linear/display measurements, config hash,
canonical log paths, and copied logs are in the output directory's `report.json`.
This isolates edit-specific brightening from initial startup and steady-state leakage.
The 3-code-value excess tolerance is a visual regression threshold, not a physical
brightness cap. The 10-code-value opening threshold catches blanket suppression.

Earlier diagnostic runs `target/cave-baseline` and `target/cave-edits/baseline` started
edits at first-ready rather than converged and also reproduced. Warm-harness attempts
`warm-baseline` and `warm-baseline2` were invalid: the first accidentally let the old
skylight lifecycle run; the second inadvertently gated every edit on convergence and
hit its explicit fixture timeout. These were harness bugs, not production failures,
and are corrected in `warm-baseline3`. A Wayland startup smoke logged an XDG portal
timeout; repeating with WAYLAND_DISPLAY unset used the same X11 path as the runner and
passed. No full RFIRR capture is used or claimed validated.

Harness validation: cargo fmt --check, cargo check, cargo test (1025 main + 4 library,
2 ignored before adding the roof-bound test); hidden/muted release smoke. Evidence
`/tmp/cave-{check,tests,smoke-x11}.log`. No generated binding changes.

## Confirmed cause and source-readiness fix

The geometry e0 trace already enables recursive feedback (`pc.has_history != 0`), but
`DdgiVolume::begin_scheduled_work` unconditionally set `source_ready=0` for geometry.
`ddgiTransportQueryInfo` copied that bit; the shared domain classifier interprets an
unready field as GlobalSky. Consequently every front-face terrain hit received
unoccluded sky as **indirect** irradiance, regardless of cave enclosure, on each new
e0. Later same-geometry epochs set readiness true and dissipated that injected energy.
This is not a fixed-color fallback (none was restored), nor ordinary steady leakage.

Diagnostic one-variable experiment: temporarily disabling recursive hit shading gave
active ROI **0.306865/255** (`target/cave-edits/no-recursion`). Restoring recursion and
changing readiness to `resident.source.is_some()` gave **0.306870/255**
(`target/cave-edits/source-ready`). Thus fresh sky misses/direct hit light are not the
source of this fixture's excess; it enters through the recursive unready query. The
no-recursion diagnostic was removed. Visibility/filter/normalization code was unchanged.
The lifecycle regression `terrain_staging_reads_resident_history_through_the_external_source_slot`
failed with source_ready 0 vs expected 1 before the fix (`/tmp/cave-source-ready-red.log`).
It now checks both inherited-source and initial-no-source cases and passes.

Full matched retest:
`python3 scripts/check_ddgi_cave_edits.py target/cave-edits/source-ready-full` **GREEN**.

| Central RGB mean /255 | Baseline | Source-ready fix |
|---|---:|---:|
| Active | 41.012900 | 0.306870 |
| Active repeat | 41.105900 | 0.306870 |
| Settled, same final geometry | 0.318261 | 0.306881 |
| Sealed, no edits | 0.306851 | 0.306851 |
| Actual skylight opened | 40.877900 | 40.872800 |

Every edit run completed 40 real terrain publications and **22 complete DDGI promotions
during the gesture**, with no ERROR/panic/VUID. Active/settled images are nearly black;
the opening remains visibly bright. The small residual dark value is measured, not
claimed zero energy. No geometry/physics, convergence schedule, history retention,
brightness, or rendering settings changed. The GUI hash is identical in both runs.
Sparse active-gesture release GPU render mean/max (microseconds): baseline active
5092.9/6250, repeat 4828.6/5480; fixed active 4836.2/5882, repeat 4701.7/4810. These are
not controlled performance acceptance measurements; no speedup is claimed.

Validation: fmt/check, 1026 main + 4 library tests (2 ignored), 16 Slang CPU tests,
63 analyzer tests, hidden/muted release smoke all pass (`/tmp/cave-ready-*.log`). The
existing sustained skylight runner also passes in `target/cave-edits/source-ready-sustained`
(receiver 86/255, no errors). No generated files changed for this Rust-only fix.
Unrelated regenerated Cargo.lock registry metadata is excluded. Full RFIRR capture
remains untested; the separately recorded baseline evidence-owner panic is not waived.

One additional source-tuple concern remains under investigation: transport atlas
bindings select the inherited field, but the shared probe metadata binding selects
the destination placement. The readiness fix exposes that path correctly for the
first time during local e0. This closed-roof fixture does not establish accuracy when
relocation changes; it needs ownership repair/validation rather than a brightness tweak.

## Complete source ownership and final acceptance

Recursive query metadata now has a source-owned descriptor alongside source irradiance
and visibility. `DdgiBuilderResources` chooses inherited Active for geometry and the
builder itself for same-volume temporal epochs. New probe origins still use destination
metadata. Consumers are unchanged. This adds one read-only descriptor, not another
allocation or a temporal delay. The source-selection unit test covers both owners;
the shader/binding wiring guard was RED against the previous shader
(`/tmp/cave-metadata-red.log`) and now passes. These are ownership/wiring guards, not a
numerical relocation-accuracy benchmark. The readiness mismatch, not relocation, was
the measured cause of the broad brightening in this fixture.

An intermediate source-metadata implementation failed startup because the initial
pipeline provider lacked the new descriptor (`target/cave-edits/source-metadata`).
The initial Volume provider now supplies its own metadata, with the builder view
rebinding the inherited source at the normal lifecycle seam. Subsequent
`source-metadata2`, final32, dense sustained, and default startup all pass. This new
failure was fixed, not treated as the unrelated legacy RFIRR owner panic.

**ROI correction:** inspection found the original central rectangle included sky in
the opened image. The runner now measures x=[25%,75%), y=[60%,80%), a lower interior-wall
rectangle that never includes sky. Existing baseline images were remeasured with the
same function; results are in `warm-baseline3/wall-roi-reanalysis.json`. Original tables
above describe the original central ROI and are retained for provenance.

`python3 scripts/check_ddgi_cave_edits.py target/cave-edits/final32` passes, repeated
active captures identical at the printed precision:

| Interior-wall mean | Baseline sRGB /255 | Final sRGB /255 | Baseline linear /255 | Final linear /255 |
|---|---:|---:|---:|---:|
| Active | 40.177800 | 0.317340 | 7.873960 | 0.024562 |
| Active repeat | 40.695400 | 0.317340 | 7.963830 | 0.024562 |
| Settled, same geometry | 0.329550 | 0.317334 | 0.025507 | 0.024562 |
| Sealed, no edits | 0.317339 | 0.317339 | 0.024562 | 0.024562 |
| Actual opening | 24.202100 | 24.196900 | 2.857220 | 2.856250 |

Every final32 edit run retains 22 complete publications during the 40 edits. No
ERROR/panic/VUID. Full-resolution active/settled/opened screenshots are retained and
were inspected. Final active GPU render sparse mean/max: 3013.7/4245us and
4147.8/5397us on repeat (substantial runtime variance, **not** a speedup claim).

Final validation: `cargo fmt --check`, `cargo check`, `cargo test` (1027 main + 4
library passed, 2 ignored), `python3 scripts/run_slang_tests.py` (16 passed),
`python3 -m unittest scripts.tests.test_analyze_environment_irradiance_capture`
(63 passed), and `cargo run --release -- --hidden --mute --auto-exit 0.5` under
GPU lock (X11 environment). Logs `/tmp/cave-final-{check,tests,slang,analyzer,smoke}.log`.
Dense existing scheduling test:
`python3 scripts/check_ddgi_sustained_edits.py target/cave-edits/final-sustained16 --spacing 16`
passes (40 edits, 3 during-edit publications, no errors; receiver 95.6667/255,
sparse GPU render mean/max 4047.11/6439us).

Limitations: the numerical cave regression is spacing32, a deterministic cuboid
removal fixture using the real terrain-publication path, not a replay of the user's
saved cave or shovel stroke. Dense spacing16 validation covers publication liveness,
not the full closed-room numerical matrix. Static leakage and general DDGI relocation
accuracy are not declared solved. Full RFIRR capture and the broader Python suite are
not acceptance claims here (recorded baseline limitations remain). No visible game
was launched, no fixed-color fallback restored, no generated bindings changed, and
no unrelated Cargo.lock registry metadata included.

## Independent integration verification

Commits `e24e9d66`, `413e47e8`, and `b542e5b8` were reviewed and fast-forwarded into
`agent/butterfly-block-flight`; the push was verified against the remote branch SHA.
The complete five-run matrix was independently repeated with the same GUI hash in
`target/cave-edits/integration32/`: active and repeat **0.317340/255**, settled
**0.317334/255**, sealed **0.317339/255**, opened interior wall **24.196900/255**.
Every edit run completed 40 edits and 22 during-edit publications without runtime errors.
`comparison.png` shows baseline-active / fixed-active / fixed-opened, left to right.
The baseline image is from the worker's `warm-baseline3`; the fixed images are from integration.

Fmt/check, 1027 main + 4 library tests (2 ignored), 16 Slang tests, and 63 capture-analyzer
tests passed. The first full Rust run failed once in the unchanged asynchronous emitter test
`stale_not_ready_result_cannot_requeue_after_live_revision_changes` (0 publications vs 1 within
its 1000-yield loop); the isolated recheck and full-suite repeat passed. This transient test
failure is retained in `target/cave-integration-tests-first-attempt.log`, not hidden or claimed
as a reproduced baseline failure. Other logs use `target/cave-integration-*.log`.

Default release hidden/muted startup and tree wind/stiffness/resize smoke also passed, with
canonical log-tail inspection. GUI/camera bytes were restored. The pre-existing Cargo.lock
patch was compared byte-for-byte with the starting patch and remains unchanged and uncommitted.
No visible game was launched and no release was made.
