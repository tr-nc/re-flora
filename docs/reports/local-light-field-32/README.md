# Fixed-32 local-light experiment — evidence and provenance

Historical second-round evidence: 2026-11-03. **[View the visual report](index.html).**
For the later source-space visibility candidate and its still-failing performance,
see the [third-round report](../source-space-visibility/index.html). Statements
about delivery below describe this rejected round, not the later runtime.

This is a rejected experiment, not a feature specification or a claim of a
quality-preserving optimization. Probe spacing stayed at **32 voxels** throughout.
The runtime candidate is preserved in local commit **`0567fe61`** and withdrawn by
**`95cb32bb`**. At the end of this round runtime sources were restored to the
starting `74fdbf6b` implementation, with its previous shared-field experiment
available and off by default. The checkbox never runs this rejected candidate.
No push was performed.

## Findings and limits

- With no blockers, interpolating probe-side direct response can return zero
  where the real upward-facing receiver gets about 711.108. The diagnostic calls
  production point-light and surface-side weighting functions. This establishes
  one failure mode, not a complete explanation of the courtyard images.
- The candidate kept attenuation, direction, cosine, range and the original
  12-sample finite-emitter quadrature analytic at receivers; it cached RGB
  transmission per source connection. The cache stores stable identities for
  24 points over all box faces, not additional spatial probes.
- Unreliable, near, blocked or inconsistent support invokes the original per-light
  evaluator. It also uses short packed-voxel receiver/probe visibility queries.
- Stable **nighttime real-grass** audit: 9,376 sampled light/receiver pairs;
  9,295 shared accepts, 81 original queries, **0 nonzero shared results** and
  **58 nonzero original results**. No sampled pair exceeded the error tolerance
  `1e-4 + 5% * max(reference RGB)`. High acceptance and zero error therefore do
  not demonstrate useful illumination reuse. Audit sampling is approximately
  1/1024 receiver entries; these are sample counts, not a census of all pixels.
- The audit covers initial unavailability, publication and at least one later
  update; a zero-acceptance startup sample was not mistaken for steady state.
- Final short measurements were slower, not merely close to original cost.
  Earlier receiver-factor runs before exact cage-link support measured near
  original cost; those are **not** the final candidate's performance result.
- This rejects this representation/support policy, not every possible method
  on a 32-voxel grid. Visibility representation/confidence remains unresolved.

## Files and image provenance

| File | Role |
| --- | --- |
| [index.html](index.html) | Visual presentation; inline authored CSS, no external resources |
| [original-32.png](original-32.png) | Unmodified native reference screenshot; original mode, candidate build `0567fe61` |
| [irradiance-32.png](irradiance-32.png) | Unmodified native previous-experiment screenshot; preserved pre-change `74fdbf6b` executable |
| [connections-32.png](connections-32.png) | Unmodified native rejected-candidate screenshot; `0567fe61` |
| [receiver-audit.txt](receiver-audit.txt) | Exact selected producer/real-grass GPU log lines from the night audit |
| [point-lifecycle.txt](point-lifecycle.txt) | Exact selected connection/lifecycle GPU log lines from the point-light fixture |
| [final-perf.json](final-perf.json) | Parsed final short-run timing measurements |
| [summarize-perf.cjs](summarize-perf.cjs) | Timing extraction script; run from repo root after creating the named raw logs |
| [check-warmth.cjs](check-warmth.cjs) | Disposable image warmth-loss diagnostic, requires ImageMagick `magick` |
| [unoccluded-probe-repro.slang](unoccluded-probe-repro.slang) | Minimal executable diagnostic using production shader functions |

All three images originate from the local **Re: Flora Release renderer**, on an
Apple M4 Pro using the native Vulkan/MoltenVK path. Original filenames equal the
bundled filenames. Native format: **PNG, 2560×1440**. They were copied byte-for-byte
from `target/shared-light-32/`, with **no crop, trim, resize, retouching, exposure
correction or color processing**. There are no derivative image files. The HTML
uses identical CSS clipping/zoom for the three local comparisons and links to the
whole native files. No external image service, creator URL, asset ID, AI image
model, submitted/effective prompt, seed or generation settings apply. These are
rendered app captures, not AI-generated image assets.

Existing scene resources retain their existing license/attribution obligations;
their redistribution rights were not independently audited for this bundle.
No new third-party image/audio asset was imported.

SHA-256:

```text
488ed5365b1e1035ba4b5f16effb7107704ed090b7197823d603b1badcedaa8c  original-32.png
781a46e0c41ed29750a9dcf325cf2c8190ed4e47d436fc26e7fe53db9f79e560  irradiance-32.png
4d6e3852ae85241a1c6726c7853b5a5c2373800a8de59d053ed6534382c8ab20  connections-32.png
```

## Capture and measurement protocol

For captures only, the saved `time_of_day` was changed from **0.47 to 0.95**.
The saved `grass_local_light_field` was false for `original-32.png`, true for the
other two. Both were restored afterwards to **0.47 / false**. No other lighting,
exposure, scene or material defaults were changed. Indirect DDGI behavior and
probe density were not changed to improve the direct-light comparison.

With the indicated build and saved option, run:

```sh
BINARY --hidden --windowed --mute --perf \
  --environment-probe-spacing-voxels 32 \
  --screenshot garden-arrival target/shared-light-32/FILE.png \
  --screenshot-delay 10 --auto-exit 20 \
  > target/shared-light-32/final-MODE-32.log 2>&1
```

Use `MODE=original|irradiance|connections` and the respective PNG filename.
The previous experiment used a preserved pre-change Release executable,
`target/shared-light-32/baseline-re-flora`; the two other runs used the same
candidate Release executable. To reproduce the candidate, build the local
`0567fe61` snapshot in a disposable checkout with the project's shader tools;
do not expect the restored current renderer to reproduce it.

All three final captures have the same native size. Initial mismatched-size
captures were rejected and are not bundled. Separate runs retain wind,
simulation and temporal convergence: **this is not a pixel-identical frozen-frame
comparison**. Matched settings do not eliminate all indirect-history differences
between independent runs.

Timing uses the final approximately six seconds of each sequential 20-second
run, excluding the screenshot interval. FPS is frame-counter advancement divided
by log wall-clock time, not an average of conditional frame-duration logs.
GPU scopes are medians of only **3 / 6 / 2 samples** respectively. No interleaved
repeats, confidence interval, device-wide acceptance or RTX 3060 Ti rerun is
claimed. Audit was disabled during these timing captures. Raw local logs remain
under `target/shared-light-32/`; the parser's input names match the command above.

```sh
node docs/reports/local-light-field-32/summarize-perf.cjs
node docs/reports/local-light-field-32/check-warmth.cjs \
  docs/reports/local-light-field-32/original-32.png \
  docs/reports/local-light-field-32/irradiance-32.png
# Expected exit 1: old field retains about 0.820 of reference warmth.
node docs/reports/local-light-field-32/check-warmth.cjs \
  docs/reports/local-light-field-32/original-32.png \
  docs/reports/local-light-field-32/connections-32.png
# Exit 0 for this loss-only check; ratio is about 1.106, NOT visual equivalence.
```

The metric is mean linear-RGB `(R-B)` in the fixed normalized region
x=0.04..0.28, y=0.64..0.736, with a one-sided minimum-retention threshold of 0.95.
It detects the reported warmth loss in these images; it does not establish
spatial correctness, exact shadows, absence of over-lighting or a stable
acceptance threshold for future independent app runs.

For the unoccluded diagnostic, from repo root (compatible Slang 2025.23):

```sh
.tools/slang-2025.23/bin/slangc \
  docs/reports/local-light-field-32/unoccluded-probe-repro.slang \
  -std 2025 -I shader/slang -target executable \
  -o target/shared-light-32/unoccluded-repro
DYLD_LIBRARY_PATH="$PWD/.tools/slang-2025.23/lib" \
  target/shared-light-32/unoccluded-repro
# Expected exit 1: spacing=32, no_blockers, exact≈711.108, interpolation=0.
```

## Receiver and lifecycle checks

In the candidate checkout, with the saved shared option true and time 0.95:

```sh
RE_FLORA_LOCAL_LIGHT_FIELD_AUDIT=1 target/release/re-flora \
  --hidden --windowed --mute --perf --environment-probe-spacing-voxels 32 \
  --screenshot garden-arrival target/shared-light-32/receiver-audit-night.png \
  --screenshot-delay 16 --auto-exit 20

RE_FLORA_LOCAL_LIGHT_FIELD_AUDIT=1 target/release/re-flora \
  --hidden --windowed --mute --perf --environment-probe-spacing-voxels 32 \
  --environment-lighting-test-scene point-light-changes --auto-exit 180
```

The fixture completed light addition, selected-light overflow, blocker changes,
motion, photometric update and removal, ending with `direct_removed=true`,
`stale_contribution=false`, `mixed_in_flight=false`. Its post-removal producer
audit has zero nonzero contributions. **The fixture has zero grass receiver audit
samples** and therefore only establishes connection lifecycle behavior, not
real-grass interpolation correctness. The separate garden audit supplies that
consumer evidence.

Candidate validation: format/check/Release, 1832 app tests plus 4 library tests;
517 standalone Slang receiver cases; 62/67 standalone Slang files pass. Five
failures (`sprinkler_contact_contracts`, `sprinkler_radial_coverage`,
`sprinkler_rotary`, `sprinkler_spray_continuity`, `terrain_material_contract`)
all reproduce from `git archive 74fdbf6b shader` using the same compiler/runtime.
After withdrawal: format/check, 1829 app tests plus 4 library tests, Release
rebuild, `--hidden --mute --auto-exit 0.5`, and per-worktree native-log inspection
pass. No ERROR/panic/VUID matches in the inspected native runs. Generated source
files did not change. The HTML was inspected at desktop and narrow viewports.
