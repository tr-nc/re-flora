# Cave edit brightening investigation

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
