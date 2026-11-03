# Local lighting and planar-normal report — evidence bundle

[Open the visual HTML report](index.html). Prepared 2026-11-04 from local Re: Flora source and native Release runs. This is presentation evidence, not a new runtime asset bundle or a parallel feature handbook.

## Files and provenance

- `index.html`: the visual report, original HTML/CSS layout and a two-panel inline SVG concept diagram. Its diagram is authored vector markup explaining shading normals versus unchanged geometry; it is not a game capture, numerical visualization of a particular wall, or imported artwork. No external fonts, scripts or image dependencies.
- `day-0-on.png`, `day-0-off.png`: original unmodified garden captures with the **global** local-light switch on/off, time of day 0.45. These are not the previous report's original/RIS comparison.
- `night-0-on.png`, `night-0-off.png`: corresponding original captures at time 0.95; both use the implementation with night-flower self-emission and light clusters removed.
- `gui-on.png`, `gui-off.png`: original native GUI workbench captures, showing checked/unchecked saved controls. The opt-in Debug review deliberately uses an isolated world with zero local sources; these images do not demonstrate eight-light garden shading or performance.
- `captures.json`: unmodified native-capture metadata, including all six PNG SHA-256 values, the camera hash and the binary hash; original `target/local-lighting-captures-final/summary.json`.
- `performance.json`: unmodified twelve-run performance rows and aggregate summaries; original `target/local-lighting-delivery-final/summary.json`.
- `evidence.txt`: verbatim selected dimension, final live upload, DDGI publication, retained source-provider and successful-exit rows from the four first-pair native logs. Also includes test totals, actual planar-fit output, full-suite Slang failure, and the earlier independently compiled original-baseline failure. Each excerpt identifies its source path. Complete logs remain under ignored `target/` directories, not needed for viewing this report.
- `README.md`: source, processing and reproduction information for this folder.

Creator of PNGs: local Re: Flora renderer, using the repository's existing garden/artwork. No external source URL/asset ID applies. No image-generation service or image-generation model was used. There is no submitted/effective image prompt, generative image seed, negative prompt or prompt-improvement setting. The garden seed below is an application procedural seed, not an AI image-generation seed. Report text/layout and inline diagram were authored with coding-assistant assistance; the diagram was rendered by the browser from the committed markup, not synthesized by an image model.

## Capture settings and processing

Renderer implementation: `9f5ed544`, including the flower removal in `59d3d346`. Measurement harness: `c4de2b60`. Binary SHA-256 for every included native run:

`8879e1ffa27309f866aca2f57a38dd353331bf2fb32df379a04fcbc6ed451901`

- Linux Vulkan, RTX 3060 Ti; hidden normal window/surface/swapchain, windowed, globally muted.
- Output 1600×900; scene shading 320×180, integer scene pixel stride 5, native-resolution UI. These are observed dimensions, not a promise that another desktop uses the same default window size.
- PNG: 8-bit/channel RGBA, non-interlaced. All six originals are kept unchanged, byte-for-byte copies from `target/local-lighting-captures-final/` with the same filenames.
- No crop, resize, normalization, exposure/color change, denoise, re-encoding, compositing or comparison-image synthesis. HTML displays responsive CSS-scaled versions and links to the full originals. The inline SVG is a separate explanatory illustration, not an image edit.
- Garden camera `garden-arrival`, procedural seed `20260519`, DDGI probe spacing `32` voxels. Auto day/night disabled; time `0.45` or `0.95`; eight-light selection budget; RIS and visibility-cache experiments both disabled. Global local lighting alternates true/false. All other committed saved preferences retained.
- Capture at six seconds, native auto-exit at ten. Wind, simulation and DDGI convergence are not frozen; these are independent-run comparisons, not pixel-perfect regression images.
- GUI captures additionally set `RE_FLORA_DEBUG_PANEL_REVIEW=1` and `RE_FLORA_DEBUG_SEARCH_REVIEW='local lighting enabled'`, omit the garden seed and use `player-default`. This avoids claiming a review-owned empty world is the actual performance garden. No real user's manual native-window click latency was measured.
- SHA-256 for each PNG is in the same-folder `captures.json`; the report/JSON copy step does not rewrite hashes, fields, figures or filenames.

## Reproduction and measurement definitions

From the repository root, run exclusively (no overlapping app/GPU jobs):

```bash
cargo build --release
node --test scripts/validate-local-lighting.test.mjs scripts/validate-light-ris.test.mjs
node scripts/validate-local-lighting.mjs --case perf --output target/local-lighting-perf-new
node scripts/validate-local-lighting.mjs --case capture --output target/local-lighting-capture-new
```

Output directories must not exist. The committed harness validates actual selected-light counts, native errors, binary/camera identity, matching within-comparison dimensions, and exact unchanged temporary saved input during each run; it restores the original GUI file bytes in `finally`, including failure. `--help` states side effects, valid cases and recovery commands. These tools never push or shut down the machine.

Each day/night performance group has three alternating pairs, twenty seconds per process. Final-window GPU values are medians of actual scope samples within the final eight seconds, then medians over three runs. Actual FPS uses actual frame advancement divided by elapsed native log wall time, not `1000/grass_scope_ms`. Whole-profiled-run FPS uses the first/last logged performance frame and excludes process/resource startup. `performance.json` preserves every underlying per-run median/FPS and sample count. Receiver audits are not active during measurements. This report does not compare the new 1600×900 measurements numerically against an older report's 1920×1080 runs.

The report's primary-source links pin the rendering implementation at `9f5ed544`. Planar-normal code itself was not changed in this task. Quantized-plane test outputs are mathematical evidence, not direct GPU timings or a claim about all terrain. Slang full-suite execution stopped after its fiftieth test (the known sprinkler-contact failure); the remaining suite is not claimed to have passed.

## Rights

No new external artwork, paper figures, restricted SDK code, audio or licensed third-party implementation was imported. Screenshots depict existing project assets: the repository's [asset policy](../../../LICENSE-ASSETS) and applicable original asset-bundle rights/provenance still apply. This bundle grants no new rights to redistribute depicted third-party artwork and does not imply the code's GPL license applies to every pictured asset.

## Rendered report verification

Opened in a dedicated headless agent-browser session and visually inspected at 1440×1000 and 390×844, including the performance cards/table and the responsive stacked normal diagram. All six original PNGs loaded at their expected 1600-pixel native width. All three disclosures opened; the narrow scope table scrolled using keyboard ArrowRight. No page-level horizontal overflow. All local links, fragment IDs, binary hashes, screenshot hashes and actual eight/zero-light rows were checked. Final axe-core audits found zero violations at both sizes; one color-contrast review remained incomplete for image-containing links / a non-text check mark (and narrow clipping), which were visually inspected. Raw audit JSON/screenshots remain under ignored `target/local-lighting-report-*`; the browser session was closed. No visible game was automatically launched.

## Delivery order

All task-owned code, generated output, report and evidence are committed locally, then the current feature branch (including preceding RIS commits) is pushed. Remote ref equality with local `HEAD` is checked before normal shutdown is requested. No release, tag, main-branch promotion, forced shutdown or bypass of system inhibitors is authorized by these scripts. The report documents this order, rather than asserting that a machine has powered off while it is still writing the report.
